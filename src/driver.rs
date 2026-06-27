use crate::flow;
use crate::gate::{self, GateResult};
use crate::hawkes::HawkesEngine;
use crate::meta::MetaController;
use crate::model::{EngineConfig, EngineState, Persona, PersonaId};
use crate::rrf;
use crate::runtime::PersonaRuntime;
use crate::sink::{ObservationRecord, ObservationSink};
use crate::utterance;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

pub fn run(
    config: &EngineConfig,
    personas: &[Persona],
    seed: u64,
    ticks: u64,
    sink: &mut dyn ObservationSink,
    runtime: &mut dyn PersonaRuntime,
) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut state = initial_state(personas, seed);
    let mut silence_count = 0;
    let mut speak_count = 0;
    // MetaController: 환경 변수에서 gain 읽기(없으면 기본값). 루프 전 1회 생성.
    let meta = MetaController::from_env();

    for tick in 0..ticks {
        // 틱 시작: content 있는 최근 FLOW_WINDOW개 발화로 flow 계산(measure_recent).
        // FakeBackend → content 항상 None → 빈 슬라이스 → flow_metric None.
        // None → meta.cooling(None) = 1.0 → update_intensities 완전 동일(골든 보존).
        let flow_metric = flow::measure_recent(&state.history);

        // mu_scale: flow None → 1.0(no-op), flow Some → cooling 계산.
        let mu_scale = meta.cooling(flow_metric);

        // intensity 전진(μ 갱신 → excitation 감쇠 → combined). rng 무소비, live와 공유.
        let combined_intensities = advance_intensities(&mut state, config, personas, mu_scale);
        let intensity_snapshot = combined_intensities.clone();

        let (gate_passed, candidates, chosen, rrf_reason, utterance_content) =
            match gate::evaluate(&combined_intensities, config.theta) {
                GateResult::Candidates(candidates) => {
                    // FSM 전이 제약: forbid_self_repeat ON이면 직전 화자를 후보에서 제거한다.
                    let filtered = filter_self_repeat(
                        &candidates,
                        &state.last_speaker,
                        config.forbid_self_repeat,
                    );

                    if filtered.is_empty() {
                        // 강제 화자가 연속 불가이고 다른 후보 없음 → 침묵으로 처리한다.
                        // silent fallback이 아닌 정상 동작으로, silence_count를 증가시킨다.
                        silence_count += 1;
                        (false, candidates, None, None, None)
                    } else {
                        let selection = rrf::select(
                            &filtered,
                            &combined_intensities,
                            &state.history,
                            config.k,
                            &mut rng,
                        );
                        let mut utterance = utterance::make_utterance(
                            &selection.chosen,
                            tick,
                            config.tick_interval,
                            false,
                            &mut rng,
                        );

                        // rrf::select + make_utterance의 rng 소비가 끝난 뒤 runtime을 호출한다.
                        // FakeBackend는 rng를 소비하지 않으므로 기존 결정성이 보존된다.
                        let content =
                            runtime.generate(&selection.chosen, &state.history, tick, &mut rng);
                        utterance.event.content = content.clone();

                        state.history.push(utterance.event);
                        suppress_chosen(&mut state, personas, &selection.chosen);
                        HawkesEngine::apply_excitation_on_speak(
                            &mut state.excitations,
                            &config.alpha,
                            &selection.chosen,
                            personas,
                        );
                        state.last_speaker = Some(selection.chosen.clone());
                        speak_count += 1;

                        (
                            true,
                            candidates,
                            Some(selection.chosen),
                            Some(selection.reason),
                            content,
                        )
                    }
                }
                GateResult::Silent => {
                    silence_count += 1;
                    (false, Vec::new(), None, None, None)
                }
            };

        let conversation_len = speak_count + silence_count;
        // α=0이면 모든 E_p가 정확히 0.0이므로 필터 결과 빈 맵 → JSON 직렬화 생략 → v0.1 골든 보존.
        let excitations: BTreeMap<PersonaId, f64> = state
            .excitations
            .iter()
            .filter(|(_, v)| **v != 0.0)
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        // flow_metric은 틱 시작 시 이미 계산됨(meta.cooling에도 사용).
        // FakeBackend → None → JSON 직렬화 생략 → 골든 보존.

        let record = ObservationRecord {
            tick,
            ts: tick as f64 * config.tick_interval,
            intensities: intensity_snapshot,
            gate_passed,
            candidates,
            chosen,
            rrf_reason,
            silence_count,
            speak_count,
            conversation_len,
            excitations,
            utterance: utterance_content,
            flow: flow_metric,
        };

        sink.emit(&record);
    }

    sink.finish();
}

fn initial_state(personas: &[Persona], seed: u64) -> EngineState {
    let intensities = personas
        .iter()
        .map(|persona| (persona.id.clone(), persona.base_rate))
        .collect::<BTreeMap<PersonaId, f64>>();

    EngineState {
        intensities,
        excitations: BTreeMap::new(),
        history: Vec::new(),
        last_speaker: None,
        rng_seed: seed,
    }
}

/// 한 틱의 Hawkes 강도 전진: μ 갱신(mu_scale 적용) → excitation 감쇠 → combined 산출.
///
/// rng 무소비·결정적. driver/live가 공유하는 per-tick 결정적 코어(R3①).
/// mu_scale은 호출부가 계산(flow를 record/twist에 재사용하므로) 후 주입한다.
/// 골든 불변식: 호출부 인라인과 byte-identical.
pub(crate) fn advance_intensities(
    state: &mut EngineState,
    config: &EngineConfig,
    personas: &[Persona],
    mu_scale: f64,
) -> BTreeMap<PersonaId, f64> {
    state.intensities = HawkesEngine::update_intensities(&*state, 1, config, personas, mu_scale);
    state.excitations = HawkesEngine::decay_excitations(
        &state.excitations,
        1,
        config.beta,
        config.tick_interval,
    );
    HawkesEngine::combined_intensities(&state.intensities, &state.excitations, personas)
}

/// forbid_self_repeat: ON이면 직전 화자를 후보에서 제거(연속 발화 금지), OFF면 후보 그대로.
///
/// rng 무소비. driver/live 공유(동일 로직, R3①).
pub(crate) fn filter_self_repeat(
    candidates: &[PersonaId],
    last_speaker: &Option<PersonaId>,
    forbid_self_repeat: bool,
) -> Vec<PersonaId> {
    if forbid_self_repeat {
        match last_speaker {
            Some(last) => candidates.iter().filter(|id| *id != last).cloned().collect(),
            None => candidates.to_vec(),
        }
    } else {
        candidates.to_vec()
    }
}

pub(crate) fn suppress_chosen(state: &mut EngineState, personas: &[Persona], chosen: &PersonaId) {
    if let Some(persona) = personas.iter().find(|persona| &persona.id == chosen) {
        state.intensities.insert(
            chosen.clone(),
            HawkesEngine::suppressed_after_speak(persona.base_rate),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::FakeBackend;
    use crate::sink::VecSink;

    fn config() -> EngineConfig {
        EngineConfig {
            beta: 0.5,
            theta: 0.5,
            k: 60.0,
            tick_interval: 1.0,
            alpha: crate::model::CouplingMatrix::default(),
            forbid_self_repeat: false,
        }
    }

    fn personas() -> Vec<Persona> {
        vec![
            Persona {
                id: "friend".to_string(),
                name: "Friend".to_string(),
                base_rate: 0.8,
            },
            Persona {
                id: "chaos".to_string(),
                name: "Chaos".to_string(),
                base_rate: 0.7,
            },
            Persona {
                id: "summarizer".to_string(),
                name: "Summarizer".to_string(),
                base_rate: 0.25,
            },
        ]
    }

    #[test]
    fn fixed_seed_produces_identical_records() {
        let config = config();
        let personas = personas();
        let mut left = VecSink::default();
        let mut right = VecSink::default();

        run(&config, &personas, 42, 100, &mut left, &mut FakeBackend);
        run(&config, &personas, 42, 100, &mut right, &mut FakeBackend);

        assert_eq!(left.records, right.records);
        assert_eq!(left.records.len(), 100);
    }

    /// forbid_self_repeat=true면 연속 두 발화 레코드의 chosen이 같으면 안 된다.
    /// alpha가 균일하게 강하면(θ가 낮고 α가 강함) 발화 후 강도가 θ 위에 유지되어
    /// 자기반복 금지 효과가 실제로 발동된다.
    #[test]
    fn forbid_self_repeat_prevents_consecutive_same_speaker() {
        use crate::model::CouplingMatrix;

        // forbid_self_repeat=true 설정: theta를 낮게 잡아 후보가 자주 생기게 한다.
        let mut alpha = CouplingMatrix::default();
        let ids = ["friend", "chaos", "summarizer"];
        // 균일 α=0.3 (off-diagonal만 설정)
        for &p in &ids {
            for &j in &ids {
                if p != j {
                    alpha.values.insert((p.to_string(), j.to_string()), 0.3);
                }
            }
        }
        let config = EngineConfig {
            beta: 0.5,
            theta: 0.2, // 낮은 theta: 후보가 거의 항상 존재
            k: 60.0,
            tick_interval: 1.0,
            alpha,
            forbid_self_repeat: true,
        };
        let personas = personas();
        let mut sink = VecSink::default();

        run(&config, &personas, 42, 200, &mut sink, &mut FakeBackend);

        // 발화가 한 번 이상 있어야 테스트가 의미 있다.
        let spoken: Vec<&str> = sink
            .records
            .iter()
            .filter_map(|r| r.chosen.as_deref())
            .collect();
        assert!(
            spoken.len() >= 10,
            "너무 많은 침묵: spoken={}",
            spoken.len()
        );

        // 연속 두 발화가 같은 화자면 실패
        for window in spoken.windows(2) {
            assert_ne!(
                window[0],
                window[1],
                "forbid_self_repeat=true인데 {w} 가 2연속 발화됨",
                w = window[0]
            );
        }
    }

    /// forbid_self_repeat=false(기본)면 동작이 변하지 않는다 — 동일 시드로 동일 결과.
    #[test]
    fn forbid_self_repeat_false_is_identical_to_default() {
        let config_default = config();
        let config_explicit = EngineConfig {
            forbid_self_repeat: false,
            ..config_default.clone()
        };
        let personas = personas();
        let mut sink_default = VecSink::default();
        let mut sink_explicit = VecSink::default();

        run(
            &config_default,
            &personas,
            42,
            100,
            &mut sink_default,
            &mut FakeBackend,
        );
        run(
            &config_explicit,
            &personas,
            42,
            100,
            &mut sink_explicit,
            &mut FakeBackend,
        );

        assert_eq!(sink_default.records, sink_explicit.records);
    }

    /// (task-34) FakeBackend(content 항상 None) → 모든 record의 flow가 None.
    /// JSON 직렬화 시 "flow" 키가 없어야 한다(골든 보존 확인).
    #[test]
    fn fake_backend_produces_no_flow_in_records() {
        let config = config();
        let personas = personas();
        let mut sink = VecSink::default();

        run(&config, &personas, 42, 50, &mut sink, &mut FakeBackend);

        for record in &sink.records {
            assert!(
                record.flow.is_none(),
                "FakeBackend에서 flow는 항상 None이어야 한다 (tick={})",
                record.tick
            );
            // JSON에 "flow" 키가 없어야 한다.
            let json = serde_json::to_string(record).expect("직렬화 성공");
            assert!(
                !json.contains("\"flow\""),
                "FakeBackend record JSON에 \"flow\" 키가 없어야 한다 (tick={}): {json}",
                record.tick
            );
        }
    }

    /// (task-34) content 있는 history를 직접 구성해 flow 계산 헬퍼 동작 검증.
    /// Event::content = Some("...") 2개 이상이면 flow::measure가 Some을 반환한다.
    #[test]
    fn flow_measure_returns_some_for_content_bearing_history() {
        use crate::flow;
        use crate::model::Event;

        let events = vec![
            Event {
                ts: 0.0,
                speaker: "aria".to_string(),
                mark: 0.0,
                content: Some("hello world".to_string()),
            },
            Event {
                ts: 1.0,
                speaker: "bjorn".to_string(),
                mark: 0.0,
                content: Some("hello friend".to_string()),
            },
            Event {
                ts: 2.0,
                speaker: "aria".to_string(),
                mark: 0.0,
                content: None, // content 없는 발화는 제외됨
            },
        ];

        let result = flow::measure_recent(&events);

        assert!(
            result.is_some(),
            "content 있는 발화 2개 이상이면 flow::measure는 Some이어야 한다"
        );
    }

    /// (task-37-3) FakeBackend → content 없음 → flow None → mu_scale 1.0 경로.
    ///
    /// FakeBackend로 두 번 실행해 동일한 결과가 나오면 mu_scale=1.0이 no-op임을 간접 확인.
    /// (MetaController::from_env 기본값도 동일하게 적용되므로 결정성 그대로.)
    #[test]
    fn fake_backend_flow_none_gives_same_records_as_deterministic_run() {
        let config = config();
        let personas = personas();
        let mut left = VecSink::default();
        let mut right = VecSink::default();

        run(&config, &personas, 42, 50, &mut left, &mut FakeBackend);
        run(&config, &personas, 42, 50, &mut right, &mut FakeBackend);

        // 동일 시드·동일 백엔드 → 완전 동일(mu_scale=1.0이므로 기존과 바이트 동일).
        assert_eq!(
            left.records, right.records,
            "FakeBackend 두 번 실행 결과가 동일해야 한다"
        );

        // 모든 record의 flow가 None(FakeBackend → content 없음).
        for record in &left.records {
            assert!(
                record.flow.is_none(),
                "FakeBackend: flow는 None이어야 한다 (tick={})",
                record.tick
            );
        }
    }
}
