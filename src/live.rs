//! 라이브 세션 코어 (v0.5 task-29).
//!
//! `LiveSession`: 실시간 채팅용 엔진 세션.
//! - 논블로킹 LLM 생성: 워커 스레드 + mpsc 채널.
//! - HumanChannel 사람 입력.
//! - 인과적 턴테이킹: in-flight 1개 강제 (`pending`).
//! - 모든 퍼블릭 메서드 즉시 반환(블록 없음).
//! - crossterm·ratatui 없음 — 순수 세션 로직.

use crate::gate::{self, GateResult};
use crate::hawkes::HawkesEngine;
use crate::human::HumanChannel;
use crate::model::{EngineConfig, EngineState, Event, Persona, PersonaId};
use crate::pool::BackendPool;
use crate::rrf;
use crate::utterance;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

/// `tick()` 반환값: 이번 틱에서 엔진이 무엇을 했는지.
#[derive(Debug, Clone, PartialEq)]
pub enum TickOutcome {
    /// 게이트 통과 후보 없음(침묵).
    Silent,
    /// 화자 선택 → 생성 job 디스패치 완료.
    Dispatched(PersonaId),
    /// 이미 생성 중(pending). 새 디스패치 없음.
    AwaitingGeneration,
}

/// 워커로 보내는 job: (placeholder_idx, speaker, history_snapshot, tick).
type Job = (usize, PersonaId, Vec<Event>, u64);
/// 워커가 돌려보내는 결과: (placeholder_idx, generated_text).
type Result = (usize, Option<String>);

/// 라이브 세션 코어.
///
/// 엔진 틱 + 사람 입력 + 논블로킹 LLM 생성을 결합한다.
/// 터미널 I/O 없음 — 렌더링은 task-30(TUI)에서 담당한다.
pub struct LiveSession {
    config: EngineConfig,
    personas: Vec<Persona>,
    state: EngineState,
    rng: ChaCha8Rng,
    human: HumanChannel,
    // pool 필드: 워커 스레드가 Arc 클론을 소유하므로 LiveSession에서는 직접 호출하지 않지만
    // Arc 카운트를 유지해 pool이 세션 수명 동안 살아있도록 한다.
    #[allow(dead_code)]
    pool: Arc<BackendPool>,
    /// 워커로 생성 job을 보내는 송신단.
    /// Drop 시 job_tx가 닫혀 워커 스레드가 종료된다.
    job_tx: Option<Sender<Job>>,
    /// 워커에서 결과를 받는 수신단(논블로킹 try_recv).
    result_rx: Receiver<Result>,
    /// 워커 스레드 JoinHandle. shutdown 또는 Drop 시 join한다.
    worker: Option<JoinHandle<()>>,
    /// 현재 in-flight 생성의 placeholder Event 인덱스.
    /// None이면 생성 대기 없음; Some(idx)이면 history[idx].content가 아직 None.
    pending: Option<usize>,
    /// 이번 세션에서 진행된 틱 카운터.
    tick_count: u64,
}

impl LiveSession {
    /// 새 LiveSession을 생성하고 워커 스레드를 스폰한다.
    ///
    /// 워커는 `pool.generate_one`을 off-thread에서 호출하며,
    /// `job_tx`가 Drop되면 recv 오류로 루프를 탈출해 종료한다.
    pub fn new(
        config: EngineConfig,
        personas: Vec<Persona>,
        seed: u64,
        pool: Arc<BackendPool>,
        human_speaker_id: impl Into<String>,
    ) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (result_tx, result_rx) = mpsc::channel::<Result>();

        // 워커 스레드: Arc<BackendPool>를 공유해 &self 경로로 generate_one 호출.
        let pool_clone = Arc::clone(&pool);
        let worker = std::thread::spawn(move || {
            // job_rx.recv()가 Err를 반환하면(job_tx drop) 루프 종료.
            while let Ok((idx, speaker, history, tick)) = job_rx.recv() {
                let text = pool_clone.generate_one(&speaker, &history, tick);
                // result_tx 오류(수신단 닫힘)는 무시하고 종료.
                if result_tx.send((idx, text)).is_err() {
                    break;
                }
            }
            // 워커 정상 종료.
        });

        // 초기 엔진 상태: driver::run의 initial_state와 동일.
        let intensities = personas
            .iter()
            .map(|p| (p.id.clone(), p.base_rate))
            .collect::<BTreeMap<PersonaId, f64>>();

        let state = EngineState {
            intensities,
            excitations: BTreeMap::new(),
            history: Vec::new(),
            last_speaker: None,
            rng_seed: seed,
        };

        let rng = ChaCha8Rng::seed_from_u64(seed);
        let human = HumanChannel::new(human_speaker_id);

        Self {
            config,
            personas,
            state,
            rng,
            human,
            pool,
            job_tx: Some(job_tx),
            result_rx,
            worker: Some(worker),
            pending: None,
            tick_count: 0,
        }
    }

    /// 사람 발화를 엔진 상태에 즉시 반영한다.
    ///
    /// `pending` 여부와 무관하게 즉시 호출(사람 입력은 인터럽트).
    /// 전 페르소나 λ가 일제히 상승하며, history에 사람 Event가 push된다.
    pub fn submit_human(&mut self, text: String) {
        // ts: 현재 틱 카운터로 논리 타임스탬프 계산.
        let ts = self.tick_count as f64 * self.config.tick_interval;
        self.human
            .speak(&mut self.state, &self.personas, text, ts);
    }

    /// 엔진을 1틱 전진한다.
    ///
    /// driver::run의 per-tick 로직과 동일한 순서:
    ///   1. update_intensities
    ///   2. decay_excitations
    ///   3. combined_intensities
    ///   4. gate::evaluate
    ///   5. forbid_self_repeat 필터
    ///   6. rrf::select
    ///   7. (pending 없을 때) 엔진측 speak 갱신 즉시 적용 + 워커로 생성 job 디스패치
    ///
    /// rng 소비 순서를 driver와 동일하게 유지해 엔진 선택 결정성을 보존한다.
    /// 모든 처리는 즉시 반환(생성은 워커에서).
    pub fn tick(&mut self) -> TickOutcome {
        let tick = self.tick_count;
        self.tick_count += 1;

        // 1. Hawkes 강도 갱신
        self.state.intensities =
            HawkesEngine::update_intensities(&self.state, 1, &self.config, &self.personas);

        // 2. excitation 감쇠
        self.state.excitations = HawkesEngine::decay_excitations(
            &self.state.excitations,
            1,
            self.config.beta,
            self.config.tick_interval,
        );

        // 3. combined intensities
        let combined_intensities = HawkesEngine::combined_intensities(
            &self.state.intensities,
            &self.state.excitations,
            &self.personas,
        );

        // 4. 게이트 평가
        let candidates = match gate::evaluate(&combined_intensities, self.config.theta) {
            GateResult::Candidates(c) => c,
            GateResult::Silent => {
                return TickOutcome::Silent;
            }
        };

        // 5. forbid_self_repeat 필터 (driver와 동일)
        let filtered: Vec<PersonaId> = if self.config.forbid_self_repeat {
            match &self.state.last_speaker {
                Some(last) => candidates
                    .iter()
                    .filter(|id| *id != last)
                    .cloned()
                    .collect(),
                None => candidates.clone(),
            }
        } else {
            candidates.clone()
        };

        if filtered.is_empty() {
            // driver와 동일: 강제 화자 연속 불가 + 다른 후보 없음 → 침묵.
            return TickOutcome::Silent;
        }

        // 6. rrf 화자 선택 (rng 소비: driver와 동일 순서)
        let selection = rrf::select(
            &filtered,
            &combined_intensities,
            &self.state.history,
            self.config.k,
            &mut self.rng,
        );

        // make_utterance도 rng를 소비(driver와 동일 순서, rng 소비 여부 driver 참조).
        // with_topic_tag=false: driver와 동일.
        let utterance = utterance::make_utterance(
            &selection.chosen,
            tick,
            self.config.tick_interval,
            false,
            &mut self.rng,
        );

        // 7. pending 확인: in-flight 1개 강제.
        if self.pending.is_some() {
            // 이미 생성 중 → 새 디스패치 안 함. 선택 자체는 이미 확정.
            // 주의: 엔진측 speak 갱신(suppress/excitation/last_speaker/history push)을
            //       pending 중에는 수행하지 않는다(인과: 생성 완료 후 다음 턴).
            return TickOutcome::AwaitingGeneration;
        }

        // 생성 job 디스패치: 먼저 엔진측 speak 갱신(driver와 동일 순서).
        let chosen = selection.chosen.clone();

        // placeholder Event(content=None)를 history에 push.
        let placeholder_event = utterance.event; // content는 이미 None
        let placeholder_idx = self.state.history.len();
        self.state.history.push(placeholder_event);

        // suppress, excitation, last_speaker 갱신 (driver::suppress_chosen 인라인).
        if let Some(persona) = self.personas.iter().find(|p| p.id == chosen) {
            self.state.intensities.insert(
                chosen.clone(),
                HawkesEngine::suppressed_after_speak(persona.base_rate),
            );
        }
        HawkesEngine::apply_excitation_on_speak(
            &mut self.state.excitations,
            &self.config.alpha,
            &chosen,
            &self.personas,
        );
        self.state.last_speaker = Some(chosen.clone());

        // history 스냅샷(워커로 전달; placeholder는 content=None으로 포함됨).
        let history_snapshot = self.state.history.clone();

        // 워커로 job 전송. 채널이 닫혔으면(워커 비정상 종료) 조용히 무시.
        if let Some(ref tx) = self.job_tx {
            let _ = tx.send((placeholder_idx, chosen.clone(), history_snapshot, tick));
            self.pending = Some(placeholder_idx);
        }

        TickOutcome::Dispatched(chosen)
    }

    /// 생성 결과를 논블로킹으로 폴링한다.
    ///
    /// 워커가 결과를 보내왔으면 해당 placeholder Event의 `content`를 채우고
    /// `pending`을 해제한 뒤 완성된 Event를 반환(렌더용).
    /// 결과가 아직 없으면 `None` 반환(즉시).
    pub fn poll_generation(&mut self) -> Option<Event> {
        match self.result_rx.try_recv() {
            Ok((idx, text)) => {
                // placeholder 채우기.
                if idx < self.state.history.len() {
                    self.state.history[idx].content = text;
                }
                // pending 해제: 다음 틱에서 새 디스패치 허용.
                self.pending = None;
                // 완성된 Event 클론 반환 (렌더용).
                self.state.history.get(idx).cloned()
            }
            Err(_) => None,
        }
    }

    /// 워커 스레드를 명시적으로 종료하고 join한다.
    ///
    /// Drop에서도 호출된다. 이중 호출 안전(job_tx/worker는 Option).
    pub fn shutdown(&mut self) {
        // job_tx를 drop해 워커 recv가 Err를 반환하도록 한다.
        drop(self.job_tx.take());
        // 워커 스레드 join (hang 없음: job_tx drop 후 워커는 루프 탈출).
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }

    // -------------------------------------------------------------------------
    // 렌더러 접근자 (task-30 TUI 전용)
    // -------------------------------------------------------------------------

    /// 현재 엔진 상태 참조(히스토리, 강도 등).
    pub fn state(&self) -> &EngineState {
        &self.state
    }

    /// 페르소나 목록 참조.
    pub fn personas(&self) -> &[Persona] {
        &self.personas
    }

    /// 현재 combined intensities (Hawkes + excitation 합산). 게이지 렌더용.
    pub fn combined_intensities(&self) -> BTreeMap<PersonaId, f64> {
        HawkesEngine::combined_intensities(
            &self.state.intensities,
            &self.state.excitations,
            &self.personas,
        )
    }

    /// 현재 in-flight pending 여부.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// 현재 틱 카운터.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CouplingMatrix;
    use crate::pool::{BackendConfig, BackendPool};
    use std::collections::BTreeMap;
    use std::time::Duration;

    /// 오프라인 BackendPool 생성 헬퍼(네트워크 없이 테스트).
    fn offline_pool() -> Arc<BackendPool> {
        let mut pool = BackendPool::new();
        pool.add(
            BackendConfig::new(
                "offline",
                "fake-model",
                "http://127.0.0.1:1", // 연결 거부 → generate_one이 즉시 None 반환
                None,
                1,
                None,
                Duration::from_millis(1),
            ),
            BTreeMap::new(),
        );
        pool.set_default("offline");
        Arc::new(pool)
    }

    fn config() -> EngineConfig {
        EngineConfig {
            beta: 0.5,
            theta: 0.5,
            k: 60.0,
            tick_interval: 1.0,
            alpha: CouplingMatrix::default(),
            forbid_self_repeat: false,
        }
    }

    fn personas() -> Vec<Persona> {
        vec![
            Persona {
                id: "aria".to_string(),
                name: "Aria".to_string(),
                base_rate: 0.8,
            },
            Persona {
                id: "bjorn".to_string(),
                name: "Bjorn".to_string(),
                base_rate: 0.7,
            },
        ]
    }

    /// (1) submit_human 후 history 마지막 = 사람 Event, 전 페르소나 excitation 상승.
    #[test]
    fn submit_human_appends_event_and_raises_excitations() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        // 발화 전: excitation 없음.
        assert!(session.state().excitations.is_empty());
        assert!(session.state().history.is_empty());

        session.submit_human("안녕하세요".to_string());

        // history에 사람 Event가 push됐어야 한다.
        let history = &session.state().history;
        assert_eq!(history.len(), 1);
        let ev = &history[0];
        assert_eq!(ev.speaker, "you");
        assert_eq!(ev.content, Some("안녕하세요".to_string()));

        // 전 페르소나 excitation이 양수로 상승해야 한다.
        let excitations = &session.state().excitations;
        for persona in session.personas() {
            let exc = excitations.get(&persona.id).copied().unwrap_or(0.0);
            assert!(exc > 0.0, "페르소나 {} excitation이 상승해야 한다", persona.id);
        }
    }

    /// (2) tick이 화자 선택 시 pending 설정 + placeholder Event push.
    ///     pending 중 추가 tick은 AwaitingGeneration 반환, 새 디스패치 없음.
    #[test]
    fn tick_dispatches_once_and_blocks_second_dispatch_while_pending() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        // 화자가 선택될 때까지 틱을 돌린다(최대 50틱).
        let mut dispatched = false;
        for _ in 0..50 {
            match session.tick() {
                TickOutcome::Dispatched(_) => {
                    dispatched = true;
                    break;
                }
                _ => {}
            }
        }

        assert!(dispatched, "50틱 내에 화자가 선택돼야 한다");
        // pending이 설정돼야 한다.
        assert!(session.is_pending(), "Dispatched 후 pending이 Some이어야 한다");
        // history에 placeholder Event(content=None)가 있어야 한다.
        let history = &session.state().history;
        assert!(!history.is_empty());
        let placeholder = history.last().unwrap();
        assert_eq!(placeholder.content, None, "placeholder content는 None이어야 한다");

        // pending 중 추가 tick은 AwaitingGeneration.
        let outcome = session.tick();
        assert_eq!(
            outcome,
            TickOutcome::AwaitingGeneration,
            "pending 중 tick은 AwaitingGeneration이어야 한다"
        );
    }

    /// (3) poll_generation: 워커 결과 도착 시 pending 해제 + Event 반환.
    ///     오프라인이라 generate_one → None 즉시. bounded 폴링(최대 2s).
    #[test]
    fn poll_generation_fills_placeholder_and_clears_pending() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        // 화자가 선택될 때까지 틱.
        let mut dispatched_speaker = None;
        for _ in 0..50 {
            if let TickOutcome::Dispatched(spk) = session.tick() {
                dispatched_speaker = Some(spk);
                break;
            }
        }
        assert!(dispatched_speaker.is_some(), "화자가 선택돼야 한다");
        assert!(session.is_pending());

        // bounded 폴링: 오프라인 → 워커가 즉시 None 반환 → poll이 빠르게 Some을 돌려줄 것.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut filled: Option<Event> = None;
        while std::time::Instant::now() < deadline {
            if let Some(ev) = session.poll_generation() {
                filled = Some(ev);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let ev = filled.expect("2s 내에 poll_generation이 Event를 반환해야 한다");
        // 오프라인이라 content는 None (generate_one → None).
        assert_eq!(ev.content, None, "오프라인 백엔드 → content None");
        // pending이 해제됐어야 한다.
        assert!(!session.is_pending(), "poll 후 pending이 해제돼야 한다");
    }

    /// (4) 엔진 선택 결정성: 같은 seed + 같은 호출 순서 → 같은 화자 선택 시퀀스.
    #[test]
    fn engine_selection_is_deterministic_with_same_seed() {
        let pool = offline_pool();

        // 두 세션에 동일 seed.
        let mut session_a = LiveSession::new(config(), personas(), 42, Arc::clone(&pool), "you");
        let mut session_b = LiveSession::new(config(), personas(), 42, pool, "you");

        // 각각 50틱 돌며 Dispatched 화자 순서를 수집.
        let collect_speakers = |session: &mut LiveSession| {
            let mut speakers = Vec::new();
            for _ in 0..50 {
                if let TickOutcome::Dispatched(spk) = session.tick() {
                    speakers.push(spk);
                    // pending 즉시 해소(결정성 비교 목적): poll로 flush.
                    // 오프라인이라 워커가 빠르게 반환하므로 짧게 대기.
                    let deadline = std::time::Instant::now()
                        + std::time::Duration::from_millis(200);
                    while std::time::Instant::now() < deadline {
                        if session.poll_generation().is_some() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
            }
            speakers
        };

        let speakers_a = collect_speakers(&mut session_a);
        let speakers_b = collect_speakers(&mut session_b);

        assert!(!speakers_a.is_empty(), "화자 선택이 최소 1회 있어야 한다");
        assert_eq!(
            speakers_a, speakers_b,
            "같은 seed면 화자 선택 시퀀스가 동일해야 한다"
        );
    }

    /// (5) Drop 시 워커 스레드가 hang/panic 없이 종료된다.
    #[test]
    fn drop_terminates_worker_cleanly() {
        let pool = offline_pool();
        {
            let mut session = LiveSession::new(config(), personas(), 42, pool, "you");
            // 몇 틱 돌리고 drop.
            for _ in 0..10 {
                let _ = session.tick();
            }
        } // Drop here — shutdown() 호출 → job_tx drop → 워커 종료 → join.
        // hang이나 panic이 없으면 통과.
    }
}
