//! 라이브 세션 코어 (v0.5 task-29).
//!
//! `LiveSession`: 실시간 채팅용 엔진 세션.
//! - 논블로킹 LLM 생성: 워커 스레드 + mpsc 채널.
//! - HumanChannel 사람 입력.
//! - 인과적 턴테이킹: in-flight 1개 강제 (`pending`).
//! - 모든 퍼블릭 메서드 즉시 반환(블록 없음).
//! - crossterm·ratatui 없음 — 순수 세션 로직.

use crate::debate::{
    build_directive, cross_room_memory_is_topic_relevant, format_hint, infer_debate_plan,
    mentioned_persona_id, repetition_guard, sanitize_generated_text, significant_topic_tokens,
    strip_speaker_prefix, summary_persona_id, twist_card, DebatePhase, DebatePlan, PhaseController,
};
use crate::flow;
use crate::gate::{self, GateResult};
use crate::hawkes::HawkesEngine;
use crate::human::HumanChannel;
use crate::memory::{MemoryEvent, MemoryStore};
use crate::meta::MetaController;
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

/// 페르소나 4축 정보(초대 시 raw 토큰).
///
/// blood/mbti/zodiac/role 값을 그대로 보관한다(예: blood="O", mbti="ENTP", zodiac="leo", role="friend").
#[derive(Debug, Clone)]
pub struct PersonaAxes {
    pub blood: String,
    pub mbti: String,
    pub zodiac: String,
    pub role: String,
}

/// 페르소나별 백엔드 라우팅 + system_prompt 보관.
///
/// `LiveSession`이 `persona_meta` 맵으로 관리한다.
/// pool은 Arc<BackendPool>로 공유하며 가변 불필요.
#[derive(Debug, Clone)]
pub struct PersonaMeta {
    /// pool의 backend 이름("cloud"/"friend" 등).
    pub backend: String,
    /// 해당 persona에 주입할 system_prompt.
    pub system_prompt: String,
    /// 케미 계수 보관용(이 단계 alpha에서는 미사용).
    pub modifier: crate::model::PersonaModifier,
    /// 초대 시 4축 정보. 동적 persona만 Some; 데모/복원 persona는 None.
    pub axes: Option<PersonaAxes>,
}

/// 워커로 보내는 job: (epoch, placeholder_idx, speaker, history_snapshot, tick, recall, route).
/// route: Some((backend, system_prompt)) = persona_meta가 있는 경우, None = 기존 generate_one 경로.
type Job = (
    u64,
    usize,
    PersonaId,
    Vec<Event>,
    u64,
    Option<String>,
    Option<(String, String)>,
);
/// 워커가 돌려보내는 결과: (epoch, placeholder_idx, generated_text).
type Result = (u64, usize, Option<String>);

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
    /// MetaController: flow 수렴도 → mu_scale 계산. task-37에서 추가.
    meta: MetaController,
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
    generation_epoch: u64,
    /// 이번 세션에서 진행된 틱 카운터.
    tick_count: u64,
    /// 회상 스토어 (task-41). 사람 발화 + 도착 발화를 기록하고 생성 전 recall.
    /// driver/headless 경로와 공유하지 않는다 — 라이브 전용.
    store: MemoryStore,
    /// 방 이름. 참여 격리 기준 단위.
    room: String,
    /// 사람 화자 ID. submit_human 시 MemoryEvent 생성에 사용(HumanChannel 필드 직접 노출 회피).
    human_id: PersonaId,
    /// 사람(나)이 직접 고른 4축 캐릭터. 엔진엔 영향 없고 시각/정체성용(아바타 렌더). 영속됨.
    human_axes: Option<PersonaAxes>,
    /// 종료 시 메타 분석가가 만든 마크다운 리포트. 영속·재접속 시 재표시·로비 요약에 사용.
    report: Option<String>,
    /// 페르소나별 백엔드 라우팅 + system_prompt. with_persona_meta 빌더로 설정.
    /// 빈 맵이면 모든 persona가 기존 generate_one(폴백 체인) 경로를 사용한다.
    persona_meta: BTreeMap<PersonaId, PersonaMeta>,
    /// 방 화제 태그(최대 5개). 생성 워커로 보내는 history 스냅샷에만 주입(INV-2).
    topics: Vec<String>,
    /// topics에서 결정적으로 추론한 토론 연출 계획(Stage D). topics 비면 None.
    /// 생성 지시·형식 변주에만 쓰이고 화자선택/골든에는 무영향(history_snapshot 전용).
    debate_plan: Option<DebatePlan>,
    /// 단계형 토론 컨트롤러(오프닝→클로징→종료). debate_plan이 Some일 때만 Some.
    /// 종료 시 dispatch 중단. plan None이면 None → 단계 비활성(기존 동작·골든 보존).
    phase: Option<PhaseController>,
    /// 직전 틱에서 토론이 막 종료됐는지(web가 1회 "토론 마무리" 알림을 보내고 소비).
    just_concluded: bool,
    /// alpha 정규화 목표 spectral radius. 0.0이면 coupling_from_modifiers가 빈 행렬 반환(케미 없음).
    /// with_target_rho 빌더로 설정. add/remove_persona 시 alpha 재계산에 사용.
    target_rho: f64,
    /// 직전 사람 발화(사람 우선 지시용). human_focus와 함께 설정된다.
    last_human_msg: Option<String>,
    /// 사람 발화 후 그 메시지를 최우선으로 둘 남은 페르소나 턴 수.
    human_focus: u32,
    /// 사용자가 특정 닉네임을 부른 경우 다음 발화자로 우선 배정할 persona.
    forced_next_speaker: Option<PersonaId>,
    /// 요약/쟁점정리 persona가 개입하지 않은 최근 persona 발화 수.
    turns_since_summary: u32,
    /// 마지막 twist(새 국면) 투입 이후 경과한 디스패치 턴 수(Stage E 쿨다운).
    turns_since_twist: u32,
    /// 화자 라벨 집합(소문자). 생성 결과 앞 `이름:`/`나:` echo 제거용.
    speaker_labels: std::collections::BTreeSet<String>,
}

/// 과수렴 판정 임계값(flow convergence). 이 값을 넘고 쿨다운이 지나면 twist 투입.
const CONVERGENCE_TWIST_THRESHOLD: f64 = 0.6;
/// twist(새 국면) 투입 간 최소 디스패치 턴 간격(연속 투입 방지).
const TWIST_COOLDOWN: u32 = 3;

/// 사람 발화 후 그 메시지를 최우선으로 둘 페르소나 턴 수.
const HUMAN_FOCUS_TURNS: u32 = 4;
const SUMMARY_CADENCE_TURNS: u32 = 4;

// 발화/지시 생성(producer) 순수 로직은 `crate::debate`로 이관됨(Stage A, 2026-06-26).
// strip_speaker_prefix / sanitize_generated_text / mentioned_persona_id / summary_persona_id /
// repetition_guard / significant_topic_tokens / cross_room_memory_is_topic_relevant /
// build_directive / length_hint — 상단 `use crate::debate::{...}` 참조.

impl LiveSession {
    /// 새 LiveSession을 생성하고 워커 스레드를 스폰한다.
    ///
    /// 워커는 `pool.generate_one`을 off-thread에서 호출하며,
    /// `job_tx`가 Drop되면 recv 오류로 루프를 탈출해 종료한다.
    ///
    /// 내부적으로 `MemoryStore::new()`(`:memory:`)를 사용한다.
    /// 모든 테스트/스모크가 이 생성자를 통해 디스크에 쓰지 않도록 보장한다.
    pub fn new(
        config: EngineConfig,
        personas: Vec<Persona>,
        seed: u64,
        pool: Arc<BackendPool>,
        human_speaker_id: impl Into<String>,
    ) -> Self {
        Self::with_store(
            config,
            personas,
            seed,
            pool,
            human_speaker_id,
            MemoryStore::new(),
        )
    }

    /// 외부에서 주입한 `MemoryStore`를 사용해 LiveSession을 생성한다.
    ///
    /// `new()`와 동일하지만 `store`를 직접 받는다.
    /// - 테스트/스모크: `new()`를 사용(`:memory:` 고정, 디스크 쓰기 0).
    /// - 라이브(`--chat`, main.rs): `with_store(..., salon::memory::live_store())`로 호출.
    ///
    /// 서명이 `new`와 동일하므로 기존 호출처는 변경 없이 유지된다.
    pub fn with_store(
        config: EngineConfig,
        personas: Vec<Persona>,
        seed: u64,
        pool: Arc<BackendPool>,
        human_speaker_id: impl Into<String>,
        store: MemoryStore,
    ) -> Self {
        Self::with_store_for_room(
            config,
            personas,
            seed,
            pool,
            human_speaker_id,
            store,
            "salon",
        )
    }

    /// 외부에서 주입한 `MemoryStore`와 room id를 사용해 LiveSession을 생성한다.
    ///
    /// Redis 멀티세션 트랙의 첫 경계다. 기존 `new`/`with_store`는 계속 기본 방
    /// `"salon"`을 쓰므로 기존 테스트와 TUI 경로는 보존된다.
    pub fn with_store_for_room(
        config: EngineConfig,
        personas: Vec<Persona>,
        seed: u64,
        pool: Arc<BackendPool>,
        human_speaker_id: impl Into<String>,
        store: MemoryStore,
        room_id: impl Into<String>,
    ) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (result_tx, result_rx) = mpsc::channel::<Result>();

        // 워커 스레드: Arc<BackendPool>를 공유해 &self 경로로 generate_one/generate_on 호출.
        let pool_clone = Arc::clone(&pool);
        let worker = std::thread::spawn(move || {
            // job_rx.recv()가 Err를 반환하면(job_tx drop) 루프 종료.
            while let Ok((epoch, idx, speaker, history, tick, recall, route)) = job_rx.recv() {
                // recall: Option<String> → as_deref()로 Option<&str> 변환.
                let text = if let Some((backend, ref prompt)) = route {
                    // persona_meta 있는 경우: generate_on 우선, 실패 시 generate_one 폴백.
                    pool_clone
                        .generate_on(
                            &backend,
                            &speaker,
                            &history,
                            tick,
                            recall.as_deref(),
                            Some(prompt),
                        )
                        .or_else(|| {
                            pool_clone.generate_one(&speaker, &history, tick, recall.as_deref())
                        })
                } else {
                    // persona_meta 없는 경우: 기존과 정확히 동일하게 generate_one.
                    pool_clone.generate_one(&speaker, &history, tick, recall.as_deref())
                };
                // result_tx 오류(수신단 닫힘)는 무시하고 종료.
                if result_tx.send((epoch, idx, text)).is_err() {
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
        let human_id: String = human_speaker_id.into();
        let human = HumanChannel::new(human_id.clone());
        // MetaController: 환경 변수에서 gain 읽기(없으면 기본값).
        let meta = MetaController::from_env();

        // 회상 스토어 초기화: 모든 페르소나 + 사람 화자를 방에 join(참여 격리 기준).
        let room = room_id.into();
        let mut store = store;
        for p in &personas {
            store.join(&room, &p.id);
        }
        store.join(&room, &human_id);

        // 화자 라벨(소문자): 페르소나 이름·id + 이름의 각 단어 + 사람 id + "나" + "(진행)".
        // 생성 결과 앞에 모델이 붙이는 `이름:`/`나:` echo를 제거할 때 매칭에 쓴다.
        let mut speaker_labels: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for p in &personas {
            speaker_labels.insert(p.name.to_lowercase());
            speaker_labels.insert(p.id.to_lowercase());
            for w in p.name.split_whitespace() {
                speaker_labels.insert(w.to_lowercase());
            }
        }
        speaker_labels.insert(human_id.to_lowercase());
        speaker_labels.insert("나".to_string());
        speaker_labels.insert("(진행)".to_string());

        Self {
            config,
            personas,
            state,
            rng,
            human,
            meta,
            pool,
            job_tx: Some(job_tx),
            result_rx,
            worker: Some(worker),
            pending: None,
            generation_epoch: 0,
            tick_count: 0,
            store,
            room,
            human_id,
            human_axes: None,
            report: None,
            persona_meta: BTreeMap::new(),
            topics: Vec::new(),
            debate_plan: None,
            phase: None,
            just_concluded: false,
            target_rho: 0.0,
            turns_since_twist: 0,
            last_human_msg: None,
            human_focus: 0,
            forced_next_speaker: None,
            turns_since_summary: 0,
            speaker_labels,
        }
    }

    /// persona_meta 맵을 통째로 설정하는 빌더 메서드.
    ///
    /// `with_store(...).with_persona_meta(map)` 체이닝으로 사용.
    /// main.rs에서 `--chat`/`--web` 경로에만 적용한다.
    /// 빈 맵이면(기본) 모든 persona가 generate_one(폴백 체인) 경로를 유지 — 기존 동작 보존.
    pub fn with_persona_meta(mut self, meta: BTreeMap<PersonaId, PersonaMeta>) -> Self {
        self.persona_meta = meta;
        self
    }

    fn speaker_label_for_generation(&self, speaker: &str, content: Option<&str>) -> String {
        if speaker == self.human_id {
            if content
                .map(|text| text.trim_start().starts_with("토론을 시작합니다."))
                .unwrap_or(false)
            {
                return "Moderator".to_string();
            }
            return self.human_id.clone();
        }
        if speaker == "(진행)" {
            return "Moderator".to_string();
        }
        self.personas
            .iter()
            .find(|persona| persona.id == speaker)
            .map(|persona| persona.name.clone())
            .unwrap_or_else(|| speaker.to_string())
    }

    fn history_for_generation(&self) -> Vec<crate::model::Event> {
        self.state
            .history
            .iter()
            .map(|event| {
                let mut event = event.clone();
                event.speaker =
                    self.speaker_label_for_generation(&event.speaker, event.content.as_deref());
                event
            })
            .collect()
    }

    fn recall_for_generation(&self, events: &[MemoryEvent]) -> Option<String> {
        let topic_tokens = significant_topic_tokens(&self.topics);
        let display_events = events
            .iter()
            .filter(|event| {
                event.room == self.room || cross_room_memory_is_topic_relevant(event, &topic_tokens)
            })
            .map(|event| MemoryEvent {
                room: event.room.clone(),
                ts: event.ts,
                speaker: self
                    .speaker_label_for_generation(&event.speaker, Some(event.content.as_str())),
                content: event.content.clone(),
            })
            .collect::<Vec<_>>();
        MemoryStore::format_recall(&display_events)
    }

    /// alpha 정규화 목표 spectral radius를 설정하는 빌더 메서드.
    ///
    /// `with_store(...).with_target_rho(rho)` 체이닝으로 사용.
    /// 설정 후 `add_persona`/`remove_persona` 호출 시 alpha가 이 rho로 정규화된다.
    /// 기본 0.0 = coupling_from_modifiers가 빈 행렬 반환 = 케미 없음 = 기존 동작 보존.
    pub fn with_target_rho(mut self, rho: f64) -> Self {
        self.target_rho = rho;
        self
    }

    /// 사람 발화를 엔진 상태에 즉시 반영한다.
    ///
    /// `pending` 여부와 무관하게 즉시 호출(사람 입력은 인터럽트).
    /// 전 페르소나 λ가 일제히 상승하며, history에 사람 Event가 push된다.
    /// 사람 발화도 회상 대상이므로 store에 기록한다(task-41).
    pub fn submit_human(&mut self, text: String) {
        // ts: 현재 틱 카운터로 논리 타임스탬프 계산.
        let ts = self.tick_count as f64 * self.config.tick_interval;
        // 엔진 상태에 반영(excitation 상승, history push).
        self.human
            .speak(&mut self.state, &self.personas, text.clone(), ts);
        // 사람 메시지를 이후 HUMAN_FOCUS_TURNS 페르소나 턴 동안 최우선 화제로 둔다
        // (사람이 화제를 바꾸면 페르소나가 몇 턴 확실히 따라오게).
        self.last_human_msg = Some(text.clone());
        self.human_focus = HUMAN_FOCUS_TURNS;
        self.forced_next_speaker = mentioned_persona_id(&text, &self.personas);
        // 종료된 토론에 사람이 끼어들면 공방으로 재진입(단계 흐름 재개) + 옛 리포트 폐기.
        let was_concluded = self.phase.as_ref().is_some_and(|p| p.is_concluded());
        if was_concluded {
            if let Some(pc) = self.phase.as_mut() {
                pc.reopen_to_clash();
            }
            self.report = None;
        }
        // 회상 스토어에 사람 발화 기록(task-41). 페르소나가 사람 말을 회상할 수 있다.
        self.store.record(MemoryEvent {
            room: self.room.clone(),
            ts: self.tick_count,
            speaker: self.human_id.clone(),
            content: text,
        });
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

        // 0. 단계형 토론이 종료됐으면 화자 선택·dispatch를 건너뛴다(방 idle, 토큰 0).
        //    사람이 발화하면 submit_human이 reopen_to_clash로 재진입시킨다.
        if self.phase.as_ref().is_some_and(|p| p.is_concluded()) {
            return TickOutcome::Silent;
        }

        // 1. Hawkes 강도 갱신 (MetaController mu_scale 적용).
        // flow()는 content 있는 최근 발화 기반 수렴도. content 없으면 None → mu_scale=1.0(no-op).
        let flow_now = self.flow();
        let mu_scale = self.meta.cooling(flow_now);
        self.state.intensities = HawkesEngine::update_intensities(
            &self.state,
            1,
            &self.config,
            &self.personas,
            mu_scale,
        );

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
            // 강제 화자 연속 불가 + 다른 후보 없음 → 침묵.
            // 1인 방(동적 초대로 persona 1명): 막 발화한 persona는 자기 차례를 건너뛰고
            // 사람 입력을 기다린다(자기 자신과 자문자답하는 것을 막는다 — 이게 올바른 동작).
            return TickOutcome::Silent;
        }

        // 6. 화자 선택.
        // 사용자 직접 호출 > (클로징 단계 or 주기적) 요약자 개입 > 기존 RRF 순서.
        // 클로징 단계에서는 cadence와 무관히 정리자를 우선해 마지막 말을 맡긴다.
        let closing_phase = self
            .phase
            .as_ref()
            .is_some_and(|p| p.phase == DebatePhase::Closing);
        let mut direct_call = false;
        let chosen = if let Some(forced) = self.forced_next_speaker.take() {
            if self.personas.iter().any(|p| p.id == forced) {
                direct_call = true;
                forced
            } else {
                rrf::select(
                    &filtered,
                    &combined_intensities,
                    &self.state.history,
                    self.config.k,
                    &mut self.rng,
                )
                .chosen
            }
        } else if closing_phase || self.turns_since_summary >= SUMMARY_CADENCE_TURNS {
            if let Some(summary_id) = summary_persona_id(&self.personas) {
                if self.state.last_speaker.as_deref() != Some(summary_id.as_str()) {
                    summary_id
                } else {
                    rrf::select(
                        &filtered,
                        &combined_intensities,
                        &self.state.history,
                        self.config.k,
                        &mut self.rng,
                    )
                    .chosen
                }
            } else {
                rrf::select(
                    &filtered,
                    &combined_intensities,
                    &self.state.history,
                    self.config.k,
                    &mut self.rng,
                )
                .chosen
            }
        } else {
            rrf::select(
                &filtered,
                &combined_intensities,
                &self.state.history,
                self.config.k,
                &mut self.rng,
            )
            .chosen
        };

        // make_utterance도 rng를 소비(driver와 동일 순서, rng 소비 여부 driver 참조).
        // with_topic_tag=false: driver와 동일.
        let utterance = utterance::make_utterance(
            &chosen,
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
        // 회상 계산 (task-41): 생성 전 최근 맥락 기반 query로 store.recall 호출.
        // recall은 생성 프롬프트에만 전달 — gate/rrf/hawkes 입력에 불사용(INV 준수).
        // query: 토론에서는 최근 맥락을 길게 잡아 반박/동조가 이어지게 한다.
        const RECALL_K: usize = 5;
        const RECALL_QUERY_LINES: usize = 8;
        let query: String = self
            .state
            .history
            .iter()
            .rev()
            .filter_map(|e| e.content.as_deref())
            .take(RECALL_QUERY_LINES)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" ");
        let recall_events = self.store.recall(&chosen, &query, RECALL_K);
        let recall: Option<String> = self.recall_for_generation(&recall_events);

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
        // 생성 프롬프트에는 내부 id("chaos") 대신 표시명("Grounded Realist")을 넣는다.
        let mut history_snapshot = self.history_for_generation();

        // 진행 지시 주입 (INV-2): 생성 워커로 보내는 스냅샷에만. state.history/flow/recall 불변.
        // query/flow/recall은 이미 위에서 계산 완료됨 — 스냅샷 조작은 그 이후.
        //
        // 우선순위: submit_human 후 human_focus 턴 동안은 사람 메시지를 최우선 화제로
        // 둔다(사람이 화제를 바꾸면 페르소나가 몇 턴 확실히 따라오게). 그 외엔 표준 화제 지시.
        let repetition = repetition_guard(&self.state.history, &chosen);
        let directive = build_directive(
            self.last_human_msg.as_deref(),
            self.human_focus > 0,
            &self.topics,
            direct_call,
            repetition,
        );
        // 발화 형식 변주(tick+화자 기반 결정적, rng 무소비). plan 있으면 모드별 형식.
        let fmt_hint = format_hint(tick, &chosen, self.debate_plan.as_ref());
        // 루프 차단 producer(Stage E): 과수렴(flow) 또는 반복 신호가 지속되고 쿨다운이
        // 지났으면 모드별 새 국면(twist) 1장 투입. content 없는 골든/FakeBackend는
        // flow None + repetition None → 신호 false → 개입 없음(불변식 보존). plan 없으면 미투입.
        let over_converging = flow_now.is_some_and(|m| m.convergence > CONVERGENCE_TWIST_THRESHOLD);
        let loop_signal = repetition.is_some() || over_converging;
        let twist: Option<&'static str> = match self.debate_plan.as_ref() {
            Some(plan) if loop_signal && self.turns_since_twist >= TWIST_COOLDOWN => {
                self.turns_since_twist = 0;
                Some(twist_card(plan.mode, tick as usize))
            }
            _ => {
                self.turns_since_twist = self.turns_since_twist.saturating_add(1);
                None
            }
        };
        // 토론 연출 프레임(모드 anchor + 대립축 1개). plan 없으면 생략 → 기존 동작 보존.
        // 순서: [토론 프레임] [진행 지시] [새 국면] [형식] — 한 줄로 합쳐 최근 로그 1줄만 차지.
        let mut segs: Vec<String> = Vec::new();
        if let Some(plan) = self.debate_plan.as_ref() {
            segs.push(plan.directive_line(tick));
        }
        // 단계 지시(오프닝/입장/공방/클로징). plan과 같은 게이팅 — phase Some일 때만.
        if let Some(pc) = self.phase.as_ref() {
            let d = pc.directive();
            if !d.is_empty() {
                segs.push(d.to_string());
            }
        }
        if let Some(d) = directive {
            // 진행 지시(사람 우선/화제)를 실제로 쓴 경우 human_focus 1턴 소모.
            if self.human_focus > 0 {
                self.human_focus -= 1;
            }
            segs.push(d);
        }
        if let Some(t) = twist {
            segs.push(t.to_string());
        }
        segs.push(fmt_hint);
        let combined = segs.join(" ");
        let topic_event = crate::model::Event {
            ts: tick as f64 * self.config.tick_interval,
            speaker: "(진행)".to_string(),
            mark: 0.0,
            content: Some(combined),
        };
        // 맨 뒤(push): 생성은 최근 로그를 뒤에서 보므로 대화가 길어져도 지시가 컨텍스트에 들어간다.
        history_snapshot.push(topic_event);

        // persona_meta에서 라우팅 정보(backend, system_prompt) 추출.
        // None이면 워커가 기존 generate_one 경로를 사용한다(persona_meta 빈 세션 = 기존 동작 보존).
        let route = self
            .persona_meta
            .get(&chosen)
            .map(|m| (m.backend.clone(), m.system_prompt.clone()));

        // 워커로 job 전송. 채널이 닫혔으면(워커 비정상 종료) 조용히 무시.
        if let Some(ref tx) = self.job_tx {
            let _ = tx.send((
                self.generation_epoch,
                placeholder_idx,
                chosen.clone(),
                history_snapshot,
                tick,
                recall,
                route,
            ));
            self.pending = Some(placeholder_idx);
        }

        if summary_persona_id(&self.personas).as_deref() == Some(chosen.as_str()) {
            self.turns_since_summary = 0;
        } else {
            self.turns_since_summary = self.turns_since_summary.saturating_add(1);
        }

        // 단계 전진: 한 발화가 디스패치됐으니 카운트 + 수렴 신호로 전환 판정.
        // flow_now는 content 있는 최근 발화 기반(없으면 None → 카운트만). 화자선택 이후라 rng 불변.
        if let Some(pc) = self.phase.as_mut() {
            if pc.on_utterance(flow_now.map(|m| m.convergence)) {
                self.just_concluded = true;
            }
        }

        TickOutcome::Dispatched(chosen)
    }

    /// 생성 결과를 논블로킹으로 폴링한다.
    ///
    /// 워커가 결과를 보내왔으면 해당 placeholder Event의 `content`를 채우고
    /// `pending`을 해제한 뒤 완성된 Event를 반환(렌더용).
    /// content가 Some이면 회상 스토어에 기록한다(task-41).
    /// 결과가 아직 없으면 `None` 반환(즉시).
    pub fn poll_generation(&mut self) -> Option<Event> {
        match self.result_rx.try_recv() {
            Ok((epoch, idx, text)) => {
                if epoch != self.generation_epoch {
                    return None;
                }
                // 모델이 앞에 붙인 화자 라벨(`이름:`/`나:`) echo 제거.
                let text = text.map(|t| {
                    sanitize_generated_text(&strip_speaker_prefix(&t, &self.speaker_labels))
                });
                // placeholder 채우기.
                if idx < self.state.history.len() {
                    self.state.history[idx].content = text;
                }
                // pending 해제: 다음 틱에서 새 디스패치 허용.
                self.pending = None;
                // 완성된 Event 클론 반환 (렌더용).
                let ev = self.state.history.get(idx).cloned();
                // 도착한 발화의 content가 Some이면 회상 스토어에 기록(task-41).
                if let Some(ref landed) = ev {
                    if let Some(ref content) = landed.content {
                        self.store.record(MemoryEvent {
                            room: self.room.clone(),
                            ts: landed.ts as u64,
                            speaker: landed.speaker.clone(),
                            content: content.clone(),
                        });
                    }
                }
                ev
            }
            Err(_) => None,
        }
    }

    /// 현재 진행 중인 생성 결과를 무효화하고 아직 비어 있는 placeholder를 제거한다.
    ///
    /// LLM HTTP 요청 자체는 워커 스레드에서 이미 진행 중일 수 있으므로 여기서 즉시
    /// 중단할 수 없다. 대신 epoch를 올려 늦게 도착한 결과가 화면, DB, 장기기억에
    /// 반영되지 않게 한다.
    pub fn cancel_pending_generation(&mut self) -> bool {
        let Some(idx) = self.pending.take() else {
            return false;
        };

        self.generation_epoch = self.generation_epoch.wrapping_add(1);

        if idx < self.state.history.len() && self.state.history[idx].content.is_none() {
            self.state.history.remove(idx);
        }
        self.state.last_speaker = self
            .state
            .history
            .iter()
            .rev()
            .find(|e| e.content.is_some())
            .map(|e| e.speaker.clone());
        true
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

    /// 최근 content 발화 최대 FLOW_WINDOW개로 수렴/발산 지표를 계산한다.
    ///
    /// content 없는 발화(FakeBackend/placeholder)는 제외.
    /// 유효 발화 2개 미만이면 None. 관찰 전용 — 엔진 결정에 영향 없음(INV-2).
    /// task-35 채팅 TUI 수렴 게이지가 이 메서드를 사용한다.
    pub fn flow(&self) -> Option<crate::flow::FlowMetric> {
        let content_utterances: Vec<&str> = self
            .state
            .history
            .iter()
            .filter_map(|e| e.content.as_deref())
            .collect();
        let window_start = content_utterances
            .len()
            .saturating_sub(crate::flow::FLOW_WINDOW);
        flow::measure(&content_utterances[window_start..])
    }

    /// 현재 MetaController가 계산하는 mu_scale을 반환한다. task-38 사이드바 표시용.
    ///
    /// `self.flow()`가 None이면 → `cooling(None)` = 1.0(no-op).
    /// content 있는 high-convergence 히스토리면 < 1.0.
    pub fn mu_scale(&self) -> f64 {
        self.meta.cooling(self.flow())
    }

    /// 현재 틱 카운터.
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// 최근 발화 빈도 기반 활기 지표. 0.0 ~ 1.0.
    ///
    /// 최근 LIVELINESS_WINDOW 틱 안에 완성된 발화(content=Some)의 수를 정규화한다.
    /// 기대치: W틱당 ~W/4개 발화가 1.0. in-flight 1개 제약으로 발화가 드문 라이브에서
    /// 너무 빨리 포화하지 않도록 보수적으로 잡음(라이브에서 체감 보고 조정).
    ///
    /// ts 단위: event.ts = tick * tick_interval(f64). 현재 tick_count와 비교할 때
    /// tick_interval로 나눠 tick 단위로 환산. tick_interval=0이면 0 반환(방어).
    ///
    /// web 표시 전용(읽기 메서드). 엔진 결정에 미사용 -- 골든 무관.
    pub fn liveliness(&self) -> f64 {
        /// 최근 몇 틱을 창으로 볼지. 라이브 관찰 후 조정 가능.
        const LIVELINESS_WINDOW: u64 = 20;
        /// W 틱 안에 발화가 몇 개면 1.0으로 볼지(기대 포화점).
        /// W/4 = 5개. 라이브에서 체감 보고 조정.
        const LIVELINESS_SCALE: f64 = LIVELINESS_WINDOW as f64 / 4.0;

        if self.config.tick_interval == 0.0 {
            return 0.0;
        }
        let window_start_tick = self.tick_count.saturating_sub(LIVELINESS_WINDOW);
        // ts = tick * tick_interval 이므로 tick = ts / tick_interval
        let recent_count = self
            .state
            .history
            .iter()
            .filter(|e| {
                e.content.is_some()
                    && (e.ts / self.config.tick_interval) as u64 >= window_start_tick
            })
            .count();
        (recent_count as f64 / LIVELINESS_SCALE).clamp(0.0, 1.0)
    }

    /// 현재 speak 임계값 theta (web/디버그 표시용).
    pub fn theta(&self) -> f64 {
        self.config.theta
    }

    /// 현재 생성 중(pending)인 화자 id. 없으면 None.
    pub fn pending_speaker(&self) -> Option<PersonaId> {
        self.pending
            .and_then(|idx| self.state.history.get(idx).map(|e| e.speaker.clone()))
    }

    // -------------------------------------------------------------------------
    // 토픽 관리 (topic-tags)
    // -------------------------------------------------------------------------

    /// 방 화제 태그를 설정한다.
    ///
    /// - 각 태그는 trim 후 빈 문자열이면 제거.
    /// - 최대 5개까지 허용(초과분은 잘림).
    /// - 빈 Vec을 전달하면 화제를 해제한다.
    pub fn set_topics(&mut self, topics: Vec<String>) {
        self.topics = topics
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .take(5)
            .collect();
        // 토론 연출 계획 재추론(결정적). topics 비면 None → plan 미주입(기존 동작 보존).
        self.debate_plan = if self.topics.is_empty() {
            None
        } else {
            Some(infer_debate_plan(&self.topics))
        };
        // 단계 컨트롤러도 plan과 동기화. 새 화제 = 새 토론 → 오프닝부터 시작.
        self.phase = self
            .debate_plan
            .as_ref()
            .map(|plan| PhaseController::new(plan.mode, self.personas.len() as u32));
        self.just_concluded = false;
    }

    pub fn reset_discussion(&mut self, topics: Vec<String>) {
        self.generation_epoch = self.generation_epoch.wrapping_add(1);
        self.pending = None;
        self.tick_count = 0;
        self.state.history.clear();
        self.state.last_speaker = None;
        self.state.excitations.clear();
        self.state.intensities = self
            .personas
            .iter()
            .map(|p| (p.id.clone(), p.base_rate))
            .collect();
        self.last_human_msg = None;
        self.human_focus = 0;
        self.forced_next_speaker = None;
        self.turns_since_summary = 0;
        self.turns_since_twist = 0;
        self.report = None;
        self.set_topics(topics);
        self.store.clear_room(&self.room);
        for persona in &self.personas {
            self.store.join(&self.room, &persona.id);
        }
        self.store.join(&self.room, &self.human_id);
    }

    /// 현재 활성 화제 태그 참조.
    pub fn topics(&self) -> &[String] {
        &self.topics
    }

    /// 현재 세션의 room id.
    pub fn room_id(&self) -> &str {
        &self.room
    }

    /// persona_meta 맵 참조(task D web에서 backend->model 매핑에 사용).
    pub fn persona_meta(&self) -> &BTreeMap<PersonaId, PersonaMeta> {
        &self.persona_meta
    }

    /// 풀에 주어진 backend 이름이 실제로 존재하는지.
    /// 초대 시 죽은 백엔드(예: friend 서버 다운 → cloud-only)로 라우팅해 침묵하는 것을 막는다.
    pub fn has_backend(&self, name: &str) -> bool {
        self.pool.backend_names().iter().any(|n| *n == name)
    }

    /// 영속 복원용: 저장된 대화 로그와 tick_count를 주입한다.
    ///
    /// add_persona로 참가자를 먼저 복원한 뒤 호출한다.
    /// 강도(intensities/excitations)는 복원하지 않는다(base_rate에서 재차오름).
    pub fn restore_history(&mut self, messages: Vec<crate::model::Event>, tick_count: u64) {
        self.state.last_speaker = messages.last().map(|e| e.speaker.clone());
        self.state.history = messages;
        self.tick_count = tick_count;
        self.pending = None;
        // 종료된 토론(report 있음)은 Concluded 로 복원 → dispatch 중단(리포트만 표시,
        // 사용자가 발화하면 submit_human 이 공방으로 재개). 진행 중이던 방은 공방부터 재개.
        // 주의: 호출 전에 set_report 가 선행되어야 한다(main.rs 복원 순서).
        let was_concluded = self.report.is_some();
        if let Some(pc) = self.phase.as_mut() {
            if was_concluded {
                pc.mark_concluded();
            } else {
                pc.reopen_to_clash();
            }
        }
        self.just_concluded = false;
    }

    /// 현재 토론 단계(단계형 활성 시). plan 없으면 None.
    pub fn current_phase(&self) -> Option<DebatePhase> {
        self.phase.as_ref().map(|p| p.phase)
    }

    /// 사람(나)의 4축 캐릭터를 설정한다(시각/정체성용, 엔진 무영향).
    pub fn set_human_axes(&mut self, axes: Option<PersonaAxes>) {
        self.human_axes = axes;
    }

    /// 사람(나)의 4축 캐릭터 참조.
    pub fn human_axes(&self) -> Option<&PersonaAxes> {
        self.human_axes.as_ref()
    }

    /// 종료 리포트(마크다운)를 설정/조회한다. 영속·재접속 재표시·로비 요약용.
    pub fn set_report(&mut self, report: Option<String>) {
        self.report = report;
    }
    pub fn report(&self) -> Option<&str> {
        self.report.as_deref()
    }

    /// 직전 틱에서 토론이 막 종료됐는지 확인하고 플래그를 소비한다(web가 1회 알림용).
    pub fn take_just_concluded(&mut self) -> bool {
        let v = self.just_concluded;
        self.just_concluded = false;
        v
    }

    /// 토론 종료 시 메타 분석가가 전체 전사를 정리해 한글 리포트를 생성한다(채팅 아님).
    ///
    /// 진행 지시("(진행)")·placeholder는 제외한 실제 발화만 전사로 넘긴다. 한국어 품질을 위해
    /// gemma 백엔드(thinking=false 태그) 우선. 발화가 너무 적거나 생성 실패면 None.
    /// 블로킹 호출이므로 호출측(web 엔진 스레드)은 방이 idle일 때만 부른다.
    pub fn summarize_debate(&self, past_conclusions: &[String]) -> Option<String> {
        let transcript: Vec<Event> = self
            .state
            .history
            .iter()
            .filter(|e| {
                e.speaker != "(진행)"
                    && e.content.as_ref().is_some_and(|c| !c.trim().is_empty())
            })
            .map(|e| {
                let mut e = e.clone();
                e.speaker = self.speaker_label_for_generation(&e.speaker, e.content.as_deref());
                e
            })
            .collect();
        if transcript.len() < 2 {
            return None;
        }

        // 한국어 리포트 품질 우선: thinking=false인 gemma 태그 백엔드 → cloud → 아무거나.
        let names = self.pool.backend_names();
        let backend = if names.iter().any(|n| *n == "gemma4:31b-cloud") {
            "gemma4:31b-cloud"
        } else if names.iter().any(|n| *n == "cloud") {
            "cloud"
        } else {
            *names.first()?
        };

        let topic = self.topics.join(", ");
        let past_context = if past_conclusions.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = past_conclusions
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{}. {c}", i + 1))
                .collect();
            format!(
                "이전 토론 결론(맥락 참고용, 평가 대상 아님):\n{}\n\n",
                items.join("\n")
            )
        };
        let prompt = format!(
            "{past_context}You are a neutral debate analyst. The discussion above is a FINISHED debate on the topic \"{topic}\". \
             Write a DEBRIEF REPORT in Korean using GitHub-flavored MARKDOWN — this is a report document, NOT a chat reply, \
             so do not address anyone or continue the debate. Lead with the conclusion (두괄식): the report MUST start with the \
             '## 결론' section. Use exactly these sections in this order:\n\
             ## 결론\n\
             (2-3 문장: 한 줄 핵심 결론 먼저 — 무엇으로 귀결됐는지 또는 끝내 갈렸는지, 가장 설득력 있던 논지.)\n\
             ## 주제\n\
             (한 줄.)\n\
             ## 참가자 입장\n\
             (각 참가자마다 `- **닉네임**: 핵심 주장` 형식 한 줄씩.)\n\
             ## 합의점\n\
             (동의한 지점. 없으면 '뚜렷한 합의 없음'.)\n\
             ## 끝까지 갈린 지점\n\
             (합의되지 않은 핵심 쟁점.)\n\
             Stay objective, do not take a side, do not invent new arguments. Use markdown headings, bold, and bullet lists. Korean only.",
            topic = topic
        );

        self.pool.generate_on(
            backend,
            &"(분석)".to_string(),
            &transcript,
            self.tick_count,
            None,
            Some(&prompt),
        )
    }

    // -------------------------------------------------------------------------
    // 런타임 persona 추가 / 제거 (task B)
    // -------------------------------------------------------------------------

    /// 새 persona를 런타임에 방에 추가한다.
    ///
    /// - intensities: base_rate로 초기화.
    /// - excitations: apply_excitation_on_speak의 or_insert(0.0)가 채우므로 명시 불필요.
    /// - speaker_labels: 이름 + id + 이름 각 단어를 소문자로 추가.
    /// - store: join(방, id) 호출로 참여 격리 등록.
    /// - persona_meta: 전달받은 meta로 설정.
    /// - config.alpha: 신규 쌍은 CouplingMatrix.get이 없으면 0을 반환 → 명시 insert 불필요(기본 0).
    pub fn add_persona(&mut self, persona: Persona, meta: PersonaMeta) {
        let id = persona.id.clone();
        self.state.intensities.insert(id.clone(), persona.base_rate);
        // excitations는 apply_excitation_on_speak의 or_insert(0.0)가 채우므로 명시 불필요.
        self.speaker_labels.insert(persona.name.to_lowercase());
        self.speaker_labels.insert(id.to_lowercase());
        for w in persona.name.split_whitespace() {
            self.speaker_labels.insert(w.to_lowercase());
        }
        self.store.join(&self.room, &id);
        self.persona_meta.insert(id, meta);
        self.personas.push(persona);
        // 단계 쿼터(인원 비례) 갱신.
        if let Some(pc) = self.phase.as_mut() {
            pc.set_persona_count(self.personas.len() as u32);
        }
        // target_rho가 설정된 세션에서 신규 persona 쌍의 alpha를 정규화된 값으로 갱신.
        self.recompute_alpha();
    }

    /// persona를 런타임에 방에서 제거한다.
    ///
    /// - personas: id 기준으로 제거.
    /// - state.intensities / state.excitations / persona_meta: 키 제거.
    /// - store: leave(방, id) 호출.
    /// - config.alpha: 해당 id가 포함된 양방향 쌍 모두 제거.
    /// - last_speaker: 제거 대상이면 None으로 초기화.
    /// - speaker_labels: personas 전체를 기준으로 재구성(공유 단어 라벨 손실 방지).
    ///
    /// pending(생성 중)인 persona를 제거하는 경우: 상태 정리만 수행하고
    /// placeholder는 건드리지 않는다. poll_generation이 기존대로 처리한다.
    pub fn remove_persona(&mut self, id: &str) {
        self.personas.retain(|p| p.id != id);
        self.state.intensities.remove(id);
        self.state.excitations.remove(id);
        self.persona_meta.remove(id);
        self.store.leave(&self.room, id);
        if self.state.last_speaker.as_deref() == Some(id) {
            self.state.last_speaker = None;
        }
        self.rebuild_speaker_labels();
        // 단계 쿼터(인원 비례) 갱신.
        if let Some(pc) = self.phase.as_mut() {
            pc.set_persona_count(self.personas.len() as u32);
        }
        // 제거 후 나머지 persona 쌍의 alpha를 전체 재계산.
        self.recompute_alpha();
    }

    /// speaker_labels를 personas 전체 + human_id + "나" + "(진행)"로 재구성한다.
    ///
    /// remove_persona 후에 호출해 공유 단어 라벨이 잘못 제거되지 않게 한다.
    /// with_store의 초기 라벨 구성 코드와 동일 규칙을 따른다.
    fn rebuild_speaker_labels(&mut self) {
        let mut labels: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for p in &self.personas {
            labels.insert(p.name.to_lowercase());
            labels.insert(p.id.to_lowercase());
            for w in p.name.split_whitespace() {
                labels.insert(w.to_lowercase());
            }
        }
        labels.insert(self.human_id.to_lowercase());
        labels.insert("나".to_string());
        labels.insert("(진행)".to_string());
        self.speaker_labels = labels;
    }

    /// personas 현재 목록과 persona_meta modifier로 config.alpha를 재계산한다.
    ///
    /// target_rho == 0.0이면 coupling_from_modifiers가 빈 행렬을 반환해 케미 없음(기존 동작).
    /// add_persona / remove_persona 후에 호출해 신규/제거 쌍의 alpha를 갱신한다.
    fn recompute_alpha(&mut self) {
        let modifiers: std::collections::BTreeMap<
            crate::model::PersonaId,
            crate::model::PersonaModifier,
        > = self
            .persona_meta
            .iter()
            .map(|(id, m)| (id.clone(), m.modifier.clone()))
            .collect();
        self.config.alpha = crate::preset::coupling_from_modifiers(
            &self.personas,
            &modifiers,
            self.config.beta,
            self.target_rho,
        );
    }
}

/// '## 결론' 섹션 본문(다음 `##` 전까지)을 공백으로 이어 반환한다.
/// 섹션이 없으면 첫 줄 반환.
pub(crate) fn extract_conclusion_section(markdown: &str) -> String {
    let mut in_section = false;
    let mut body = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## 결론") {
            in_section = true;
            continue;
        }
        if in_section {
            if trimmed.starts_with("##") {
                break;
            }
            if !trimmed.is_empty() {
                body.push(trimmed.to_string());
            }
        }
    }
    if body.is_empty() {
        markdown.lines().next().unwrap_or("").to_string()
    } else {
        body.join(" ")
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

    // 순수 producer 함수 단위 테스트는 `crate::debate`로 이관됨(Stage A): text.rs / directive.rs.
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
            assert!(
                exc > 0.0,
                "페르소나 {} excitation이 상승해야 한다",
                persona.id
            );
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
        assert!(
            session.is_pending(),
            "Dispatched 후 pending이 Some이어야 한다"
        );
        // history에 placeholder Event(content=None)가 있어야 한다.
        let history = &session.state().history;
        assert!(!history.is_empty());
        let placeholder = history.last().unwrap();
        assert_eq!(
            placeholder.content, None,
            "placeholder content는 None이어야 한다"
        );

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

    #[test]
    fn cancel_pending_generation_removes_placeholder_and_ignores_late_result() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        for _ in 0..50 {
            if let TickOutcome::Dispatched(_) = session.tick() {
                break;
            }
        }
        assert!(session.is_pending(), "생성 취소 전 pending이 있어야 한다");
        assert!(
            session
                .state()
                .history
                .last()
                .is_some_and(|event| event.content.is_none()),
            "생성 취소 전 placeholder가 있어야 한다"
        );

        assert!(
            session.cancel_pending_generation(),
            "진행 중 생성이 취소되어야 한다"
        );
        assert!(!session.is_pending(), "생성 취소 후 pending 해제");
        assert!(
            session
                .state()
                .history
                .iter()
                .all(|event| event.content.is_some()),
            "생성 취소 후 비어 있는 placeholder가 남으면 안 된다"
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if session.poll_generation().is_some() {
                panic!("취소된 epoch의 늦은 결과가 반영되면 안 된다");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn recall_for_generation_filters_weak_cross_room_topic_overlap() {
        let pool = offline_pool();
        let mut session = LiveSession::with_store_for_room(
            config(),
            personas(),
            42,
            pool,
            "you",
            MemoryStore::new(),
            "judge-room",
        );
        session.set_topics(vec!["AI 판사가 인간 판사보다 공정할 수 있을까?".to_string()]);

        let events = vec![
            MemoryEvent {
                room: "old-open-source-room".to_string(),
                ts: 1,
                speaker: "aria".to_string(),
                content: "AI 규제와 오픈소스 책임은 투명한 거버넌스가 중요하다".to_string(),
            },
            MemoryEvent {
                room: "judge-room".to_string(),
                ts: 2,
                speaker: "bjorn".to_string(),
                content: "AI 판사의 항소 절차를 따져봐야 한다".to_string(),
            },
        ];

        let recall = session
            .recall_for_generation(&events)
            .expect("현재 방 기억은 남아야 한다");
        assert!(recall.contains("항소 절차"));
        assert!(
            !recall.contains("오픈소스 책임"),
            "넓은 AI 토큰만 겹친 다른 방 기억은 제외되어야 한다: {recall}"
        );
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
                    let deadline =
                        std::time::Instant::now() + std::time::Duration::from_millis(200);
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

    /// (task-34) content 없는 history(오프라인/FakeBackend) → flow()는 None.
    #[test]
    fn live_session_flow_returns_none_for_empty_content_history() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        // 틱을 돌려도 오프라인이라 content는 None → flow None.
        for _ in 0..20 {
            let _ = session.tick();
        }

        assert!(
            session.flow().is_none(),
            "content 없는 history → flow()는 None이어야 한다"
        );
    }

    /// (task-34) content 있는 발화를 수동으로 push했을 때 flow()가 Some을 반환한다.
    #[test]
    fn live_session_flow_returns_some_for_content_bearing_history() {
        use crate::model::Event;

        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        // content 있는 Event 2개를 history에 직접 push(결정적 stub content).
        session.state.history.push(Event {
            ts: 0.0,
            speaker: "aria".to_string(),
            mark: 0.0,
            content: Some("안녕 반가워".to_string()),
        });
        session.state.history.push(Event {
            ts: 1.0,
            speaker: "bjorn".to_string(),
            mark: 0.0,
            content: Some("안녕 오랜만이야".to_string()),
        });

        let result = session.flow();
        assert!(
            result.is_some(),
            "content 있는 발화 2개 이상이면 flow()는 Some이어야 한다"
        );
        // convergence는 [0, 1] 범위
        if let Some(metric) = result {
            assert!(
                metric.convergence >= 0.0 && metric.convergence <= 1.0,
                "convergence는 [0, 1] 범위여야 한다: {}",
                metric.convergence
            );
        }
    }

    // -------------------------------------------------------------------------
    // task-37 신규 테스트: mu_scale() 접근자 + MetaController 배선
    // -------------------------------------------------------------------------

    /// (task-37-4a) content 없는 history → flow None → mu_scale() == 1.0.
    ///
    /// FakeBackend/오프라인 → content 없음 → cooling(None) = 1.0.
    #[test]
    fn mu_scale_returns_one_for_empty_content_history() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        // 틱을 돌려도 오프라인이라 content는 None → mu_scale == 1.0.
        for _ in 0..20 {
            let _ = session.tick();
        }

        let scale = session.mu_scale();
        assert!(
            (scale - 1.0).abs() < 1e-15,
            "content 없는 history → mu_scale()은 1.0이어야 한다, 실제: {scale}"
        );
    }

    // -------------------------------------------------------------------------
    // task-41: 회상 스토어 기록 + 격리 테스트 (네트워크 없음)
    // -------------------------------------------------------------------------

    /// (task-41-1) submit_human 후 store에 사람 발화가 기록된다.
    ///
    /// store.recall로 기록 여부를 간접 확인한다(참여한 화자가 회상 가능).
    #[test]
    fn submit_human_records_to_store() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        session.submit_human("안녕 여기 있어".to_string());

        // "you"는 "salon" 방에 참여. "안녕"이 포함된 query로 recall.
        let recall = session.store.recall("you", "안녕", 5);
        assert_eq!(recall.len(), 1, "사람 발화 1개가 store에 기록되어야 함");
        assert_eq!(recall[0].content, "안녕 여기 있어");
        assert_eq!(recall[0].speaker, "you");
    }

    /// (task-41-2) 도착한 발화(poll_generation)가 store에 기록된다.
    ///
    /// 오프라인이라 content=None → store에 기록하지 않는다.
    /// content=Some인 경우를 직접 history에 주입해 검증.
    #[test]
    fn poll_generation_records_landed_utterance_to_store() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        // 화자가 선택될 때까지 틱.
        let mut dispatched_speaker: Option<PersonaId> = None;
        for _ in 0..50 {
            if let TickOutcome::Dispatched(spk) = session.tick() {
                dispatched_speaker = Some(spk);
                break;
            }
        }
        assert!(dispatched_speaker.is_some(), "화자가 선택돼야 한다");

        // bounded 폴링: 오프라인 → content=None으로 도착.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if session.poll_generation().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // 오프라인이라 content=None이므로 store에는 기록 없어야 한다.
        // (content=Some일 때만 기록하는 규칙 검증)
        let speaker = dispatched_speaker.unwrap();
        let recall = session.store.recall(&speaker, "아무거나", 5);
        assert!(
            recall.is_empty(),
            "오프라인(content=None) → store에 기록 없어야 함(speaker={speaker})"
        );
    }

    /// (task-41-3) 참여 격리: "you"가 기록한 발화를 미참여자는 볼 수 없다.
    ///
    /// store.join은 new()에서 페르소나+human만 등록한다.
    /// 등록되지 않은 화자는 recall 결과 없음.
    #[test]
    fn store_participation_isolation_preserved() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        session.submit_human("비밀 이야기".to_string());

        // "stranger"는 salon에 참여하지 않았으므로 회상 불가.
        let recall = session.store.recall("stranger", "비밀", 5);
        assert!(
            recall.is_empty(),
            "미참여 화자는 회상 결과 없어야 함(참여 격리)"
        );

        // "aria"는 참여했으므로 회상 가능.
        let recall_aria = session.store.recall("aria", "비밀", 5);
        assert_eq!(
            recall_aria.len(),
            1,
            "참여 페르소나(aria)는 사람 발화를 회상할 수 있어야 함"
        );
    }

    /// (task-41-4a) mu_scale_returns_one_for_empty_content_history — 기존 테스트 유지.

    // -------------------------------------------------------------------------
    // topic-tags 테스트
    // -------------------------------------------------------------------------

    /// (topics-1) set_topics: 빈 Vec → topics() 빈 슬라이스.
    #[test]
    fn set_topics_clear() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");
        session.set_topics(vec!["rust".to_string(), "ai".to_string()]);
        assert_eq!(session.topics().len(), 2);
        session.set_topics(vec![]);
        assert!(session.topics().is_empty(), "빈 Vec → topics 해제");
    }

    /// (topics-2) set_topics: 6개 주면 5개로 cap.
    #[test]
    fn set_topics_cap_at_5() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");
        let six = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
            "f".to_string(),
        ];
        session.set_topics(six);
        assert_eq!(session.topics().len(), 5, "6개 → 5개로 cap");
        assert_eq!(session.topics()[4], "e", "5번째까지만 유지");
    }

    /// (topics-3) set_topics: trim + 빈 문자열 제거.
    #[test]
    fn set_topics_trim_and_drop_empty() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");
        session.set_topics(vec![
            "  rust  ".to_string(),
            "".to_string(),
            "  ".to_string(),
            "ai".to_string(),
        ]);
        assert_eq!(session.topics().len(), 2, "빈 항목 2개 제거 후 2개");
        assert_eq!(session.topics()[0], "rust", "trim 적용");
        assert_eq!(session.topics()[1], "ai");
    }

    /// (task-41-4b) content + high-convergence history → mu_scale() < 1.0.
    ///
    /// 거의 동일한 발화 여러 개를 history에 주입해 수렴도를 높인다.
    /// MetaController 기본값(gain=0.6, threshold=0.5, floor=0.4)에서
    /// convergence > 0.5이면 mu_scale < 1.0이어야 한다.
    #[test]
    fn mu_scale_below_one_for_high_convergence_history() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        // 거의 동일한 내용의 발화 4개: Jaccard 유사도가 높아 convergence > 0.5 기대.
        for i in 0..4u64 {
            session.state.history.push(crate::model::Event {
                ts: i as f64,
                speaker: "aria".to_string(),
                mark: 0.0,
                content: Some("안녕 반가워 오늘도".to_string()),
            });
        }

        let scale = session.mu_scale();
        assert!(
            scale < 1.0,
            "high-convergence history → mu_scale()은 < 1.0이어야 한다, 실제: {scale}"
        );
        // floor 이상이어야 한다(MetaController 기본 floor=0.4).
        assert!(
            scale >= 0.4,
            "mu_scale()은 floor(0.4) 이상이어야 한다, 실제: {scale}"
        );
    }

    // -------------------------------------------------------------------------
    // task-B: add_persona / remove_persona 단위 테스트
    // -------------------------------------------------------------------------

    fn make_persona_meta(backend: &str) -> PersonaMeta {
        PersonaMeta {
            backend: backend.to_string(),
            system_prompt: format!("system prompt for {backend}"),
            modifier: crate::model::PersonaModifier::default(),
            axes: None,
        }
    }

    /// (task-B-a) add_persona 후: personas / intensities / persona_meta 키에 새 id 존재,
    /// store.recall이 그 persona로 동작(join됨).
    #[test]
    fn add_persona_registers_in_all_structures() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        let new_p = Persona {
            id: "nova".to_string(),
            name: "Nova Test".to_string(),
            base_rate: 0.60,
        };
        let meta = make_persona_meta("cloud");
        session.add_persona(new_p, meta);

        // personas 목록에 존재
        assert!(
            session.personas().iter().any(|p| p.id == "nova"),
            "add 후 personas에 nova 있어야 함"
        );
        // intensities에 존재
        assert!(
            session.state().intensities.contains_key("nova"),
            "add 후 intensities에 nova 있어야 함"
        );
        // persona_meta에 존재
        assert!(
            session.persona_meta().contains_key("nova"),
            "add 후 persona_meta에 nova 있어야 함"
        );
        // store join 확인: submit_human 이후 nova가 recall 가능해야 함.
        session.submit_human("nova join test".to_string());
        let recall = session.store.recall("nova", "nova join test", 5);
        assert!(
            !recall.is_empty(),
            "join 후 nova가 사람 발화를 회상할 수 있어야 함"
        );
    }

    /// (task-B-b) remove_persona 후:
    /// personas / intensities / excitations / persona_meta에서 사라짐,
    /// config.alpha에 그 id 포함 쌍 0개, last_speaker 정리.
    #[test]
    fn remove_persona_cleans_all_structures() {
        let pool = offline_pool();
        // config에 alpha 쌍을 명시적으로 추가해 제거를 검증한다.
        let mut cfg = config();
        cfg.alpha
            .values
            .insert(("aria".to_string(), "bjorn".to_string()), 0.3);
        cfg.alpha
            .values
            .insert(("bjorn".to_string(), "aria".to_string()), 0.2);
        let mut session = LiveSession::new(cfg, personas(), 42, Arc::clone(&pool), "you");

        // excitation을 수동으로 추가(remove 후 사라지는지 검증용).
        session.state.excitations.insert("aria".to_string(), 0.5);
        // last_speaker를 aria로 설정.
        session.state.last_speaker = Some("aria".to_string());

        session.remove_persona("aria");

        // personas에서 사라짐
        assert!(
            !session.personas().iter().any(|p| p.id == "aria"),
            "remove 후 personas에 aria 없어야 함"
        );
        // intensities에서 사라짐
        assert!(
            !session.state().intensities.contains_key("aria"),
            "remove 후 intensities에 aria 없어야 함"
        );
        // excitations에서 사라짐
        assert!(
            !session.state().excitations.contains_key("aria"),
            "remove 후 excitations에 aria 없어야 함"
        );
        // persona_meta에서 사라짐
        assert!(
            !session.persona_meta().contains_key("aria"),
            "remove 후 persona_meta에 aria 없어야 함"
        );
        // config.alpha에 aria 포함 쌍 0개
        let aria_pairs: Vec<_> = session
            .config
            .alpha
            .values
            .keys()
            .filter(|(a, b)| a == "aria" || b == "aria")
            .collect();
        assert_eq!(
            aria_pairs.len(),
            0,
            "remove 후 alpha에 aria 포함 쌍 없어야 함"
        );
        // last_speaker 정리
        assert_eq!(
            session.state().last_speaker,
            None,
            "remove 후 last_speaker는 None이어야 함"
        );
    }

    /// (task-B-c) add -> remove -> add 시퀀스 후 상태 일관(키셋 일치).
    #[test]
    fn add_remove_add_sequence_is_consistent() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        let make_nova = || Persona {
            id: "nova".to_string(),
            name: "Nova".to_string(),
            base_rate: 0.55,
        };

        // add
        session.add_persona(make_nova(), make_persona_meta("cloud"));
        assert!(session.personas().iter().any(|p| p.id == "nova"));
        assert!(session.persona_meta().contains_key("nova"));

        // remove
        session.remove_persona("nova");
        assert!(!session.personas().iter().any(|p| p.id == "nova"));
        assert!(!session.persona_meta().contains_key("nova"));
        assert!(!session.state().intensities.contains_key("nova"));

        // re-add
        session.add_persona(make_nova(), make_persona_meta("friend"));
        assert!(session.personas().iter().any(|p| p.id == "nova"));
        assert!(session.persona_meta().contains_key("nova"));
        assert!(session.state().intensities.contains_key("nova"));

        // 상태 일관: nova가 personas / intensities / persona_meta 세 곳 모두에 있어야 함.
        // (persona_meta는 add_persona로 추가된 persona만 관리한다 — new()의 초기 personas는 포함 안 됨.)
        assert!(
            session.personas().iter().any(|p| p.id == "nova"),
            "re-add 후 personas에 nova 있어야 함"
        );
        assert!(
            session.state().intensities.contains_key("nova"),
            "re-add 후 intensities에 nova 있어야 함"
        );
        assert!(
            session.persona_meta().contains_key("nova"),
            "re-add 후 persona_meta에 nova 있어야 함"
        );
        // 초기 personas(aria/bjorn)는 여전히 존재해야 함.
        assert!(session.personas().iter().any(|p| p.id == "aria"));
        assert!(session.personas().iter().any(|p| p.id == "bjorn"));
    }

    // -------------------------------------------------------------------------
    // task-G: restore_history 단위 테스트
    // -------------------------------------------------------------------------

    /// (task-G-restore) restore_history 후 state().history / tick_count() / last_speaker 일치.
    #[test]
    fn restore_history_sets_history_tick_and_last_speaker() {
        use crate::model::Event;

        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        let messages = vec![
            Event {
                ts: 0.0,
                speaker: "aria".to_string(),
                mark: 0.0,
                content: Some("안녕".to_string()),
            },
            Event {
                ts: 1.0,
                speaker: "bjorn".to_string(),
                mark: 0.0,
                content: Some("반가워".to_string()),
            },
        ];

        session.restore_history(messages.clone(), 77);

        // tick_count 일치
        assert_eq!(session.tick_count(), 77);
        // history 일치
        assert_eq!(session.state().history.len(), 2);
        assert_eq!(session.state().history[0].speaker, "aria");
        assert_eq!(session.state().history[1].speaker, "bjorn");
        // last_speaker = 마지막 event의 speaker
        assert_eq!(
            session.state().last_speaker,
            Some("bjorn".to_string()),
            "last_speaker는 마지막 event의 speaker이어야 함"
        );
        // pending은 None
        assert!(!session.is_pending(), "restore 후 pending은 None이어야 함");
    }

    /// (task-G-restore-empty) 빈 messages로 restore_history 시 last_speaker = None.
    #[test]
    fn restore_history_empty_messages_sets_no_last_speaker() {
        let pool = offline_pool();
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");

        session.restore_history(vec![], 0);

        assert_eq!(session.state().last_speaker, None);
        assert!(session.state().history.is_empty());
        assert_eq!(session.tick_count(), 0);
    }

    // -------------------------------------------------------------------------
    // with_target_rho / recompute_alpha 테스트
    // -------------------------------------------------------------------------

    /// (alpha-recompute-1) with_target_rho(0.40) 세션에 add_persona 2명 후:
    /// - config.alpha.values 비어있지 않음(케미 생성됨)
    /// - branching_spectral_radius < 1 (안정)
    #[test]
    fn add_persona_with_target_rho_generates_stable_alpha() {
        use crate::hawkes::HawkesEngine;
        use crate::preset::RoomPreset;

        let pool = offline_pool();
        // 빈 personas로 시작 + target_rho 설정 (복원 분기와 동일 패턴).
        let mut session = LiveSession::with_store(
            config(),
            vec![],
            42,
            pool,
            "you",
            crate::memory::MemoryStore::new(),
        )
        .with_target_rho(RoomPreset::Pub.target_rho());

        let meta_a = make_persona_meta("cloud");
        let meta_b = make_persona_meta("cloud");
        let meta_c = make_persona_meta("friend");

        let p_a = Persona {
            id: "alpha".to_string(),
            name: "Alpha".to_string(),
            base_rate: 0.70,
        };
        let p_b = Persona {
            id: "beta".to_string(),
            name: "Beta".to_string(),
            base_rate: 0.60,
        };
        let p_c = Persona {
            id: "gamma".to_string(),
            name: "Gamma".to_string(),
            base_rate: 0.55,
        };

        // 1명 추가: n<=1이므로 alpha는 빈 행렬
        session.add_persona(p_a, meta_a);
        assert!(
            session.config.alpha.values.is_empty(),
            "1명이면 alpha는 빈 행렬이어야 한다"
        );

        // 2명 추가: n=2, alpha 생성됨
        session.add_persona(p_b, meta_b);
        assert!(
            !session.config.alpha.values.is_empty(),
            "2명 이후 alpha.values가 비어있으면 안 된다(케미 생성)"
        );

        // 안정성: branching_spectral_radius < 1
        let beta = session.config.beta;
        let radius = HawkesEngine::branching_spectral_radius(
            &session.config.alpha,
            session.personas(),
            beta,
        );
        assert!(
            radius < 1.0,
            "alpha는 안정(spectral radius < 1)이어야 한다, 실제: {radius}"
        );
        // target_rho 근접: Pub = 0.40
        let target = RoomPreset::Pub.target_rho();
        assert!(
            (radius - target).abs() < 1e-6,
            "spectral radius({radius})가 target_rho({target})와 1e-6 이내이어야 한다"
        );

        // 3명 추가 후에도 안정
        session.add_persona(p_c, meta_c);
        let radius3 = HawkesEngine::branching_spectral_radius(
            &session.config.alpha,
            session.personas(),
            beta,
        );
        assert!(radius3 < 1.0, "3명 후에도 안정이어야 한다, 실제: {radius3}");
        assert!(
            (radius3 - target).abs() < 1e-6,
            "3명 후 spectral radius({radius3})가 target_rho({target})와 1e-6 이내이어야 한다"
        );
    }

    /// (alpha-recompute-2) remove 후에도 안정.
    #[test]
    fn remove_persona_with_target_rho_keeps_alpha_stable() {
        use crate::hawkes::HawkesEngine;
        use crate::preset::RoomPreset;

        let pool = offline_pool();
        let mut session = LiveSession::with_store(
            config(),
            vec![],
            42,
            pool,
            "you",
            crate::memory::MemoryStore::new(),
        )
        .with_target_rho(RoomPreset::Pub.target_rho());

        // 3명 추가
        for (id, name) in [("aa", "AA"), ("bb", "BB"), ("cc", "CC")] {
            let p = Persona {
                id: id.to_string(),
                name: name.to_string(),
                base_rate: 0.60,
            };
            session.add_persona(p, make_persona_meta("cloud"));
        }
        let beta = session.config.beta;
        let target = RoomPreset::Pub.target_rho();

        // 1명 제거 후 2명 남음 → alpha 재계산, 안정 유지
        session.remove_persona("cc");
        assert_eq!(session.personas().len(), 2, "cc 제거 후 2명");
        assert!(
            !session.config.alpha.values.is_empty(),
            "2명 남았으므로 alpha.values가 비어있으면 안 된다"
        );
        let radius = HawkesEngine::branching_spectral_radius(
            &session.config.alpha,
            session.personas(),
            beta,
        );
        assert!(
            (radius - target).abs() < 1e-6,
            "remove 후 spectral radius({radius})가 target_rho({target})와 1e-6 이내이어야 한다"
        );

        // 1명 더 제거: 1명 남음 → alpha 빈 행렬
        session.remove_persona("bb");
        assert_eq!(session.personas().len(), 1, "bb 제거 후 1명");
        assert!(
            session.config.alpha.values.is_empty(),
            "1명이면 alpha는 빈 행렬이어야 한다"
        );
    }

    /// (alpha-recompute-3) with_target_rho 없는 세션(기본 0.0)은 add_persona 후에도 alpha 빈 행렬
    /// → 기존 동작 보존.
    #[test]
    fn add_persona_without_target_rho_keeps_empty_alpha() {
        let pool = offline_pool();
        // with_target_rho 호출 없음 → target_rho = 0.0
        let mut session = LiveSession::with_store(
            config(),
            vec![],
            42,
            pool,
            "you",
            crate::memory::MemoryStore::new(),
        );

        let p_a = Persona {
            id: "x".to_string(),
            name: "X".to_string(),
            base_rate: 0.70,
        };
        let p_b = Persona {
            id: "y".to_string(),
            name: "Y".to_string(),
            base_rate: 0.60,
        };
        session.add_persona(p_a, make_persona_meta("cloud"));
        session.add_persona(p_b, make_persona_meta("cloud"));

        // target_rho=0.0 → coupling_from_modifiers가 빈 행렬 반환
        assert!(
            session.config.alpha.values.is_empty(),
            "with_target_rho 없으면 alpha는 빈 행렬이어야 한다(기존 동작 보존)"
        );
    }

    /// (task-B-d) persona_meta 빈 세션의 tick/poll_generation이 기존과 동일하게 동작(회귀 없음).
    ///
    /// `new()`로 생성한 세션은 persona_meta가 비어 있으므로 generate_one 경로를 타야 한다.
    /// 오프라인 백엔드(즉시 None 반환) + poll 후 pending 해제 확인.
    #[test]
    fn empty_persona_meta_session_behaves_identically_to_before() {
        let pool = offline_pool();
        // new()로 생성 = persona_meta 빈 맵.
        let mut session = LiveSession::new(config(), personas(), 42, pool, "you");
        assert!(
            session.persona_meta().is_empty(),
            "new()는 persona_meta가 빈 맵이어야 함"
        );

        // 화자가 선택될 때까지 틱.
        let mut dispatched = false;
        for _ in 0..50 {
            if let TickOutcome::Dispatched(_) = session.tick() {
                dispatched = true;
                break;
            }
        }
        assert!(
            dispatched,
            "50틱 내에 화자가 선택돼야 한다(persona_meta 빈 세션)"
        );
        assert!(
            session.is_pending(),
            "Dispatched 후 pending이 Some이어야 한다"
        );

        // bounded 폴링: 오프라인 → content=None으로 즉시 반환.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut filled = false;
        while std::time::Instant::now() < deadline {
            if session.poll_generation().is_some() {
                filled = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            filled,
            "2s 내에 poll_generation이 Event를 반환해야 한다(persona_meta 빈 세션)"
        );
        assert!(!session.is_pending(), "poll 후 pending이 해제돼야 한다");
    }
}
