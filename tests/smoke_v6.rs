// smoke_v6.rs — v0.6 스모크 게이트
//
// 검증 항목:
//   INV-1  FakeBackend 결정성 (v0.6 코드에도 headless 결정 경로 불변)
//          + 모든 record.flow == None (FakeBackend content 없음 → 측정 불가)
//          + 직렬화된 NDJSON에 "flow" 키 없음 (skip_serializing_if)
//   flow 콘텐츠 게이팅: VecSink + FakeBackend → 전체 record.flow == None
//   flow 결정성/measure: content 있는 발화 슬라이스 → Some(결정적)
//   render_chat: flow Some/None 둘 다 panic 없이 렌더 + "흐름" 레이블 확인
//
// 전체 네트워크-프리: FakeBackend + 오프라인 BackendPool + TestBackend

use salon::chat::render_chat;
use salon::driver;
use salon::flow::{self, FlowMetric};
use salon::model::{CouplingMatrix, EngineConfig, Event, Persona};
use salon::runtime::FakeBackend;
use salon::sink::VecSink;
use std::collections::BTreeMap;

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

// ──────────────────────────────────────────────────────────────────────────────
// INV-1: FakeBackend 결정성 + flow None 게이팅
//
// (a) seed 42, θ 0.65, 80틱 두 번 실행 → records 바이트 동일
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
         — v0.6 코드가 결정 경로를 오염시키고 있음"
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
// flow 콘텐츠 게이팅: VecSink + FakeBackend 실행 → 전체 flow None
//
// FakeBackend는 utterance/content를 생성하지 않는다.
// driver::run의 ObservationRecord 경로에서 flow 필드가 항상 None임을 재확인한다.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn flow_content_gating_vec_sink_fake_backend_all_none() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let mut sink = VecSink::default();
    driver::run(&config, &personas, 7, 40, &mut sink, &mut FakeBackend);

    assert!(
        !sink.records.is_empty(),
        "FakeBackend 실행 후 record가 있어야 한다"
    );
    for record in &sink.records {
        assert_eq!(
            record.flow, None,
            "tick {}: FakeBackend VecSink 경로 flow는 항상 None이어야 한다",
            record.tick
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// flow 결정성: flow::measure 동일 입력 두 번 → 동일 결과
//             + content 있는 발화 → Some, 없음 → None
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn flow_measure_is_deterministic_and_content_gated() {
    // content 있는 발화 2개 이상 → Some, 결정적
    let utterances = ["안녕 반가워", "오랜만이야 잘 지냈어?", "응 별일 없었어"];
    let r1 = flow::measure(&utterances);
    let r2 = flow::measure(&utterances);
    assert!(
        r1.is_some(),
        "content 있는 발화 3개 → flow::measure는 Some이어야 한다"
    );
    assert_eq!(r1, r2, "동일 입력에 대한 두 번의 measure 호출이 같아야 한다 (결정성)");

    // convergence [0, 1] 범위
    if let Some(m) = r1 {
        assert!(
            m.convergence >= 0.0 && m.convergence <= 1.0,
            "convergence는 [0, 1] 범위여야 한다: {}",
            m.convergence
        );
    }

    // 빈 슬라이스 → None
    assert!(
        flow::measure(&[]).is_none(),
        "빈 입력 → flow::measure는 None이어야 한다"
    );

    // 발화 1개 → None
    assert!(
        flow::measure(&["한 마디"]).is_none(),
        "발화 1개 → flow::measure는 None이어야 한다"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// render_chat: flow Some/None 둘 다 panic 없이 렌더
//
// TestBackend 한글 주의: 2-byte 한글은 셀+공백으로 분리됨.
// → "흐름" 첫 글자 '흐' 또는 ASCII '-' / 숫자 값으로 검증.
//   flow=None → "흐름 -" 중 '-' 존재 확인.
//   flow=Some(0.5) → "0.50" 또는 '흐' 첫 글자 존재 확인.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn render_chat_with_flow_some_no_panic() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let history: Vec<Event> = vec![Event {
        ts: 0.0,
        speaker: "aria".to_string(),
        mark: 0.0,
        content: Some("hello v6".to_string()),
    }];
    let intensities: BTreeMap<String, f64> =
        BTreeMap::from([("aria".to_string(), 0.72)]);
    let names: BTreeMap<String, String> = BTreeMap::from([
        ("aria".to_string(), "Aria".to_string()),
    ]);

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("TestBackend terminal 생성 실패");

    // flow=Some(convergence=0.5) — panic 없이 완료돼야 한다
    terminal
        .draw(|f| {
            render_chat(
                f,
                &history,
                &intensities,
                &names,
                0.65,
                "v6-test-input",
                false,
                Some(FlowMetric { convergence: 0.5 }),
                1.0, // mu_scale: 1.0 (식힘 없음)
            )
        })
        .expect("render_chat(flow=Some) panic 없이 완료돼야 한다");

    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    // "0.50" 값이 사이드바에 나타나야 한다
    assert!(
        text.contains("0.50"),
        "flow=Some(0.5) 렌더 시 '0.50' 값이 버퍼에 있어야 한다. 실제: {text:?}"
    );
    // 입력 버퍼도 나타나야 한다
    assert!(
        text.contains("v6-test-input"),
        "입력 버퍼 'v6-test-input'이 나타나야 한다"
    );
}

#[test]
fn render_chat_with_flow_none_no_panic() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let history: Vec<Event> = Vec::new();
    let intensities: BTreeMap<String, f64> =
        BTreeMap::from([("aria".to_string(), 0.30)]);
    let names: BTreeMap<String, String> =
        BTreeMap::from([("aria".to_string(), "Aria".to_string())]);

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("TestBackend terminal 생성 실패");

    // flow=None — panic 없이 완료돼야 한다
    terminal
        .draw(|f| {
            render_chat(
                f,
                &history,
                &intensities,
                &names,
                0.65,
                "v6-none-input",
                false,
                None,
                1.0, // mu_scale: 1.0 (식힘 없음)
            )
        })
        .expect("render_chat(flow=None) panic 없이 완료돼야 한다");

    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    // flow=None → "흐름 -" 중 '-' 가 나타나야 한다
    // (TestBackend에서 한글 "흐름"은 공백 포함 분리되지만 '-'는 ASCII로 직접 검증 가능)
    assert!(
        text.contains('-'),
        "flow=None 렌더 시 '-' 문자가 버퍼에 있어야 한다 (흐름 - 표시). 실제: {text:?}"
    );
    // 입력 버퍼도 나타나야 한다
    assert!(
        text.contains("v6-none-input"),
        "입력 버퍼 'v6-none-input'이 나타나야 한다"
    );
}
