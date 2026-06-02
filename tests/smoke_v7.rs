// smoke_v7.rs — v0.7 스모크 게이트
//
// 검증 항목:
//   INV-1  FakeBackend 결정성 (v0.7 코드에도 headless 결정 경로 불변)
//          + 모든 record.flow == None (FakeBackend content 없음 → 측정 불가)
//          + 직렬화된 NDJSON에 "flow" 키 없음 (skip_serializing_if)
//   MetaController no-op: cooling(None)==1.0 / 오프라인 LiveSession → mu_scale()==1.0
//   update_intensities golden math: mu_scale=1.0 no-op / mu_scale<1 → 낮은 회복 목표
//   render_chat: mu_scale 1.0/0.7 둘 다 panic 없이 렌더 + "식힘" 값 표시
//
// 전체 네트워크-프리: FakeBackend + 오프라인 BackendPool + TestBackend

use salon::chat::render_chat;
use salon::driver;
use salon::flow::FlowMetric;
use salon::hawkes::HawkesEngine;
use salon::live::LiveSession;
use salon::meta::MetaController;
use salon::model::{CouplingMatrix, EngineConfig, EngineState, Event, Persona};
use salon::pool::{BackendConfig, BackendPool};
use salon::runtime::FakeBackend;
use salon::sink::VecSink;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

// ──────────────────────────────────────────────────────────────────────────────
// 공통 헬퍼
// ──────────────────────────────────────────────────────────────────────────────

fn demo_personas() -> Vec<Persona> {
    vec![
        Persona {
            id: "aria".to_string(),
            name: "Aria".to_string(),
            base_rate: 0.80,
        },
        Persona {
            id: "bjorn".to_string(),
            name: "Bjorn".to_string(),
            base_rate: 0.70,
        },
        Persona {
            id: "clio".to_string(),
            name: "Clio".to_string(),
            base_rate: 0.25,
        },
    ]
}

fn base_config(theta: f64) -> EngineConfig {
    EngineConfig {
        beta: 0.5,
        theta,
        k: 60.0,
        tick_interval: 1.0,
        alpha: CouplingMatrix::default(),
        forbid_self_repeat: false,
    }
}

/// 테스트용 오프라인 BackendPool 헬퍼.
/// 포트 1은 즉시 연결 거부 → generate_one이 즉시 None 반환.
fn offline_pool() -> Arc<BackendPool> {
    let mut pool = BackendPool::new();
    pool.add(
        BackendConfig::new(
            "offline",
            "fake-model",
            "http://127.0.0.1:1", // 즉시 연결 거부
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

// ──────────────────────────────────────────────────────────────────────────────
// INV-1: FakeBackend 결정성 + flow None 게이팅
//
// (a) seed 42, θ 0.65, 80틱 두 번 실행 → records 바이트 동일 (mu_scale 경로 1.0)
// (b) 모든 record.flow == None (FakeBackend → content 없음 → 측정 불가)
// (c) 직렬화 NDJSON에 "flow" 키 없음 (skip_serializing_if)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn inv1_fake_backend_determinism_and_flow_none() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let seed = 42u64;
    let ticks = 80u64;

    // (a) 동일 seed 두 번 실행 → records 바이트 동일
    let mut sink_a = VecSink::default();
    let mut sink_b = VecSink::default();
    driver::run(&config, &personas, seed, ticks, &mut sink_a, &mut FakeBackend);
    driver::run(&config, &personas, seed, ticks, &mut sink_b, &mut FakeBackend);

    assert_eq!(
        sink_a.records, sink_b.records,
        "INV-1 위반: 동일 seed({seed}) 두 번 실행이 다른 records를 생성함 \
         — v0.7 코드가 결정 경로를 오염시키고 있음"
    );

    // (b) 모든 record.flow == None
    for record in &sink_a.records {
        assert_eq!(
            record.flow, None,
            "tick {}: FakeBackend 경로 record에 flow가 None이 아님 \
             — 결정 경로에 flow 계산이 끼어들었음",
            record.tick
        );
    }

    // (c) 직렬화 NDJSON에 "flow" 키 없음
    for record in &sink_a.records {
        let json = serde_json::to_string(record).expect("직렬화 성공");
        assert!(
            !json.contains("\"flow\""),
            "tick {}: flow=None인데 NDJSON에 \"flow\" 키가 있음. 실제: {json}",
            record.tick
        );
    }

    // records가 실제로 존재해야 단언이 의미 있음
    assert_eq!(
        sink_a.records.len(),
        ticks as usize,
        "tick 수({ticks})와 records 수({}) 불일치",
        sink_a.records.len()
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// MetaController no-op:
//   (a) MetaController::default().cooling(None) == 1.0
//   (b) 오프라인 BackendPool 기반 LiveSession → 20틱 후 mu_scale() == 1.0
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn meta_controller_no_op_without_content() {
    // (a) cooling(None) == 1.0 — content 없으면 항상 no-op
    let ctrl = MetaController::default();
    let scale = ctrl.cooling(None);
    assert!(
        (scale - 1.0).abs() < 1e-15,
        "MetaController::default().cooling(None) == 1.0이어야 한다, 실제: {scale}"
    );

    // (b) 오프라인 LiveSession: content가 없으므로 mu_scale() == 1.0
    let pool = offline_pool();
    let mut session = LiveSession::new(base_config(0.65), demo_personas(), 42, pool, "you");

    for _ in 0..20 {
        let _ = session.tick();
    }

    let mu = session.mu_scale();
    assert!(
        (mu - 1.0).abs() < 1e-15,
        "오프라인 LiveSession mu_scale()은 1.0이어야 한다 (content 없음), 실제: {mu}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// update_intensities golden math:
//   (a) mu_scale=1.0 → 기존 공식과 완전 동일 (base_rate + (prev-base_rate)*decay)
//   (b) mu_scale<1.0 → 회복 목표가 낮아져 강도가 더 낮게 회복 (유계 ≥ floor)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn update_intensities_golden_math() {
    let personas = vec![Persona {
        id: "aria".to_string(),
        name: "Aria".to_string(),
        base_rate: 0.8,
    }];
    let config = EngineConfig {
        beta: 0.5,
        theta: 0.5,
        k: 60.0,
        tick_interval: 1.0,
        alpha: CouplingMatrix::default(),
        forbid_self_repeat: false,
    };

    // 초기 강도: suppress 후 낮은 값에서 시작
    let prev_intensity = 0.2_f64;
    let state = EngineState {
        intensities: BTreeMap::from([("aria".to_string(), prev_intensity)]),
        excitations: BTreeMap::new(),
        history: Vec::new(),
        last_speaker: None,
        rng_seed: 42,
    };

    // (a) mu_scale=1.0: 기존 공식 수동 계산과 비트 동일해야 한다
    let decay = (-config.beta * config.tick_interval).exp();
    let base_rate = 0.8_f64;
    let expected_scale1 = base_rate + (prev_intensity - base_rate) * decay;

    let result_scale1 = HawkesEngine::update_intensities(&state, 1, &config, &personas, 1.0);
    let actual_scale1 = *result_scale1.get("aria").expect("aria 강도가 있어야 한다");
    assert!(
        (actual_scale1 - expected_scale1).abs() < 1e-15,
        "mu_scale=1.0: 기존 공식과 비트 동일해야 한다. 기대={expected_scale1}, 실제={actual_scale1}"
    );

    // (b) mu_scale=0.5: 회복 목표가 낮아져 결과가 scale1보다 낮아야 한다
    let result_scale_half =
        HawkesEngine::update_intensities(&state, 1, &config, &personas, 0.5);
    let actual_scale_half = *result_scale_half.get("aria").expect("aria 강도가 있어야 한다");
    assert!(
        actual_scale_half < actual_scale1,
        "mu_scale=0.5인 경우 회복 목표가 낮아 결과가 더 낮아야 한다. \
         scale1={actual_scale1}, scale_half={actual_scale_half}"
    );

    // (c) 결과가 음수가 되지 않는다 (유계 하한 보장)
    assert!(
        actual_scale_half >= 0.0,
        "update_intensities 결과가 음수가 되면 안 된다: {actual_scale_half}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// render_chat: mu_scale 1.0 / 0.7 둘 다 panic 없이 렌더 + "식힘" 값 표시
//
// TestBackend 한글 주의: 2-byte 한글은 셀+공백으로 분리됨.
// → "식힘" 문자 대신 ASCII 수치("1.00"/"0.70") + 'x' 로 검증.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn render_chat_with_mu_scale_no_panic_and_shows_value() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let history: Vec<Event> = vec![Event {
        ts: 0.0,
        speaker: "aria".to_string(),
        mark: 0.0,
        content: Some("hello v7".to_string()),
    }];
    let intensities: BTreeMap<String, f64> = BTreeMap::from([("aria".to_string(), 0.72)]);
    let names: BTreeMap<String, String> =
        BTreeMap::from([("aria".to_string(), "Aria".to_string())]);

    // ── (a) mu_scale=1.0 (식힘 없음) ────────────────────────────────────
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("TestBackend terminal 생성 실패");

    terminal
        .draw(|f| {
            render_chat(
                f,
                &history,
                &intensities,
                &names,
                0.65,
                "v7-scale1-input",
                false,
                Some(FlowMetric { convergence: 0.3 }),
                1.0, // mu_scale=1.0: 식힘 없음
            )
        })
        .expect("render_chat(mu_scale=1.0) panic 없이 완료돼야 한다");

    let text1: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    // "1.00" 수치가 사이드바에 나타나야 한다 (TestBackend ASCII 검증)
    assert!(
        text1.contains("1.00"),
        "mu_scale=1.0 렌더 시 '1.00' 값이 버퍼에 있어야 한다. 실제: {text1:?}"
    );
    // 입력 버퍼 검증
    assert!(
        text1.contains("v7-scale1-input"),
        "입력 버퍼 'v7-scale1-input'이 나타나야 한다"
    );

    // ── (b) mu_scale=0.7 (식힘 중) ──────────────────────────────────────
    let backend2 = TestBackend::new(100, 30);
    let mut terminal2 = Terminal::new(backend2).expect("TestBackend terminal 생성 실패");

    terminal2
        .draw(|f| {
            render_chat(
                f,
                &history,
                &intensities,
                &names,
                0.65,
                "v7-scale07-input",
                false,
                Some(FlowMetric { convergence: 0.8 }),
                0.7, // mu_scale=0.7: 식힘 중
            )
        })
        .expect("render_chat(mu_scale=0.7) panic 없이 완료돼야 한다");

    let text2: String = terminal2
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    // "0.70" 수치가 사이드바에 나타나야 한다
    assert!(
        text2.contains("0.70"),
        "mu_scale=0.7 렌더 시 '0.70' 값이 버퍼에 있어야 한다. 실제: {text2:?}"
    );
    // 입력 버퍼 검증
    assert!(
        text2.contains("v7-scale07-input"),
        "입력 버퍼 'v7-scale07-input'이 나타나야 한다"
    );
}
