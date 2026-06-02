use salon::driver;
use salon::hawkes::HawkesEngine;
use salon::model::{CouplingMatrix, EngineConfig, Persona, PersonaId, PersonaModifier};
use salon::preset::RoomPreset;
use salon::sink::VecSink;
use std::collections::BTreeMap;

// ──────────────────────────────────────────────────────────────────────────────
// 공통 헬퍼
// ──────────────────────────────────────────────────────────────────────────────

fn demo_personas() -> Vec<Persona> {
    vec![
        Persona {
            id: "friend".to_string(),
            name: "Friendly Regular".to_string(),
            base_rate: 0.80,
        },
        Persona {
            id: "chaos".to_string(),
            name: "Chaos Guest".to_string(),
            base_rate: 0.70,
        },
        Persona {
            id: "summarizer".to_string(),
            name: "Quiet Summarizer".to_string(),
            base_rate: 0.25,
        },
    ]
}

fn chosen_sequence(sink: &VecSink) -> Vec<Option<&str>> {
    sink.records
        .iter()
        .map(|r| r.chosen.as_deref())
        .collect()
}

// ──────────────────────────────────────────────────────────────────────────────
// 기준 1: 케미 — 같은 preset, 같은 seed, 다른 modifier → chosen 시퀀스가 달라진다
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn chemistry_modifiers_change_speaker_pattern() {
    let personas = demo_personas();
    let preset = RoomPreset::Pub;

    // Modifier set A: friend는 반응적이고 chaos는 자극적
    let mut modifiers_a: BTreeMap<PersonaId, PersonaModifier> = BTreeMap::new();
    modifiers_a.insert(
        "friend".to_string(),
        PersonaModifier { reactivity: 2.5, provocativeness: 0.5 },
    );
    modifiers_a.insert(
        "chaos".to_string(),
        PersonaModifier { reactivity: 0.5, provocativeness: 2.5 },
    );
    modifiers_a.insert(
        "summarizer".to_string(),
        PersonaModifier { reactivity: 1.0, provocativeness: 1.0 },
    );

    // Modifier set B: 반대 — chaos가 반응적이고 friend가 자극적
    let mut modifiers_b: BTreeMap<PersonaId, PersonaModifier> = BTreeMap::new();
    modifiers_b.insert(
        "friend".to_string(),
        PersonaModifier { reactivity: 0.5, provocativeness: 2.5 },
    );
    modifiers_b.insert(
        "chaos".to_string(),
        PersonaModifier { reactivity: 2.5, provocativeness: 0.5 },
    );
    modifiers_b.insert(
        "summarizer".to_string(),
        PersonaModifier { reactivity: 1.0, provocativeness: 1.0 },
    );

    let config_a = preset.build_config_with_modifiers(&personas, &modifiers_a);
    let config_b = preset.build_config_with_modifiers(&personas, &modifiers_b);

    // 동일 seed, 동일 ticks
    let seed = 42u64;
    let ticks = 300u64;

    let mut sink_a = VecSink::default();
    let mut sink_b = VecSink::default();
    driver::run(&config_a, &personas, seed, ticks, &mut sink_a);
    driver::run(&config_b, &personas, seed, ticks, &mut sink_b);

    // α 행렬이 비대칭적이어야 한다 (전제 확인)
    let ab = config_a.alpha.get(&"friend".to_string(), &"chaos".to_string());
    let ba = config_a.alpha.get(&"chaos".to_string(), &"friend".to_string());
    assert!(
        (ab - ba).abs() > 1e-9,
        "config_a의 α가 대칭 — modifier가 실제로 비대칭을 만들어야 한다: α_friend→chaos={ab} α_chaos→friend={ba}"
    );

    let seq_a = chosen_sequence(&sink_a);
    let seq_b = chosen_sequence(&sink_b);

    // 두 시퀀스가 달라야 한다 — 케미가 화자 전이 패턴을 바꾼다
    assert_ne!(
        seq_a, seq_b,
        "동일 seed+ticks에서 modifier만 달리했는데 chosen 시퀀스가 동일함 — 케미가 동작하지 않음"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 기준 2: 안정 — Pub preset(ρ=0.4), 300틱, is_stable true, intensities 유계
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn stable_preset_keeps_intensities_bounded() {
    let personas = demo_personas();
    let config = RoomPreset::Pub.build_config(&personas);

    // is_stable 검증
    assert!(
        HawkesEngine::is_stable(&config.alpha, &personas, config.beta),
        "Pub preset이 is_stable=false — spectral radius >= 1"
    );

    let mut sink = VecSink::default();
    driver::run(&config, &personas, 7, 300, &mut sink);

    // 모든 레코드의 intensities가 유한하고 보수적 상한(5.0) 아래
    for record in &sink.records {
        for (pid, &intensity) in &record.intensities {
            assert!(
                intensity.is_finite(),
                "tick {}: {pid} intensity={intensity}가 무한대 — 발산",
                record.tick
            );
            assert!(
                intensity < 5.0,
                "tick {}: {pid} intensity={intensity} >= 5.0 — 상한 초과",
                record.tick
            );
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 기준 3: seed 분산 — α 활성 preset(Chaos), 여러 seed, 화자 분포가 seed에 따라 갈린다
//
// speak_count는 거의 항상 ticks와 동일(theta가 낮아 침묵이 거의 없음)하므로
// seed 민감도를 화자별 발화 수(특히 summarizer)로 측정한다.
// α 교차 자극이 burst를 만들어 seed별로 각 화자의 지분이 달라진다.
// ──────────────────────────────────────────────────────────────────────────────

fn persona_speak_count(sink: &VecSink, persona_id: &str) -> u64 {
    sink.records
        .iter()
        .filter(|r| r.chosen.as_deref() == Some(persona_id))
        .count() as u64
}

#[test]
fn alpha_creates_seed_variance_in_length() {
    let personas = demo_personas();
    // Chaos preset: ρ=0.92로 α 자극이 강해 seed에 따른 분기가 뚜렷함.
    // summarizer(base_rate=0.25)는 α burst의 영향을 상대적으로 크게 받아
    // seed별 발화 수 분산이 가장 두드러진다.
    let config = RoomPreset::Chaos.build_config(&personas);
    let ticks = 300u64;

    // summarizer의 seed별 발화 수를 수집
    let summarizer_counts: Vec<u64> = (0u64..12)
        .map(|seed| {
            let mut sink = VecSink::default();
            driver::run(&config, &personas, seed, ticks, &mut sink);
            persona_speak_count(&sink, "summarizer")
        })
        .collect();

    // 모든 값이 동일하지 않아야 한다 — 분산 > 0
    let first = summarizer_counts[0];
    let all_same = summarizer_counts.iter().all(|&c| c == first);
    assert!(
        !all_same,
        "Chaos preset에서 seed 0..12의 summarizer 발화 수가 전부 {first}로 동일 \
         — α 자극으로 인한 seed 민감도가 없음: {summarizer_counts:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 기준 4: 토글 — α=0(빈 CouplingMatrix) 설정에서 v0.1 μ→빈도 동작 유지
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn alpha_zero_toggle_matches_v1_behavior() {
    let personas = demo_personas();

    // 빈 α, forbid_self_repeat=false: v0.1 동등 baseline
    let config = EngineConfig {
        beta: 0.5,
        theta: 0.65,
        k: 60.0,
        tick_interval: 1.0,
        alpha: CouplingMatrix::default(),
        forbid_self_repeat: false,
    };

    // (a) 동일 seed 두 번 → 동일 결과 (결정성)
    let mut sink_1 = VecSink::default();
    let mut sink_2 = VecSink::default();
    driver::run(&config, &personas, 42, 200, &mut sink_1);
    driver::run(&config, &personas, 42, 200, &mut sink_2);
    assert_eq!(
        sink_1.records, sink_2.records,
        "α=0에서 동일 seed가 동일 결과를 내지 않음 — 결정성 위반"
    );

    // (b) friend > chaos > summarizer 발화 수 (v0.1 μ→빈도 동작)
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for record in &sink_1.records {
        if let Some(chosen) = record.chosen.as_deref() {
            *counts.entry(chosen).or_insert(0) += 1;
        }
    }

    let friend_count = counts.get("friend").copied().unwrap_or(0);
    let chaos_count = counts.get("chaos").copied().unwrap_or(0);
    let summarizer_count = counts.get("summarizer").copied().unwrap_or(0);

    assert!(
        friend_count > chaos_count,
        "α=0에서 friend({friend_count}) > chaos({chaos_count}) 이어야 함 — μ→빈도 동작 위반"
    );
    assert!(
        chaos_count > summarizer_count,
        "α=0에서 chaos({chaos_count}) > summarizer({summarizer_count}) 이어야 함 — μ→빈도 동작 위반"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 기준 5: FSM — forbid_self_repeat=true면 인접 두 발화의 chosen이 동일하지 않음
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn fsm_forbids_consecutive_same_speaker() {
    let personas = demo_personas();

    // low theta(0.2) + α 활성(균일 0.4 off-diagonal) → 발화가 자주 일어남
    // forbid_self_repeat=true → 자기반복 금지가 실제로 발동될 환경
    let mut alpha = CouplingMatrix::default();
    let ids = ["friend", "chaos", "summarizer"];
    for &p in &ids {
        for &j in &ids {
            if p != j {
                alpha.values.insert((p.to_string(), j.to_string()), 0.4);
            }
        }
    }

    let config = EngineConfig {
        beta: 0.5,
        theta: 0.20,
        k: 60.0,
        tick_interval: 1.0,
        alpha,
        forbid_self_repeat: true,
    };

    let mut sink = VecSink::default();
    driver::run(&config, &personas, 42, 300, &mut sink);

    // 발화 시퀀스만 추출
    let spoken: Vec<&str> = sink
        .records
        .iter()
        .filter_map(|r| r.chosen.as_deref())
        .collect();

    // 발화가 충분히 있어야 테스트가 의미 있음
    assert!(
        spoken.len() >= 10,
        "발화가 너무 적음({}) — FSM 검증 불가",
        spoken.len()
    );

    // 인접 두 발화가 같은 화자면 실패
    for window in spoken.windows(2) {
        assert_ne!(
            window[0], window[1],
            "forbid_self_repeat=true인데 '{w}'가 2연속 발화됨",
            w = window[0]
        );
    }
}
