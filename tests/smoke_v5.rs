// smoke_v5.rs — v0.5 스모크 게이트
//
// 검증 항목:
//   INV-1  FakeBackend 결정성 (v0.5 라이브 코드가 들어와도 headless 결정 경로 불변)
//   HumanChannel  speak 후 history + excitation 상승 (task-28)
//   LiveSession   submit_human/tick/poll_generation 오프라인 통합 (task-29)
//   render_chat   TestBackend로 panic 없는 렌더 + 입력/이름 표시 (task-30)
//
// 전체 네트워크-프리: FakeBackend + 오프라인 BackendPool (http://127.0.0.1:1) + TestBackend

use salon::chat::render_chat;
use salon::driver;
use salon::hawkes::HawkesEngine;
use salon::human::HumanChannel;
use salon::live::{LiveSession, TickOutcome};
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
            "http://127.0.0.1:1", // 즉시 연결 거부 — 빠른 타임아웃
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
// INV-1: FakeBackend 결정성 게이트 (v0.5 헤드라인)
//
// v0.5 라이브 코드(human/live/chat 모듈)가 존재해도
// FakeBackend 직접 경로(driver::run)가 동일 seed에서 바이트 동일 records를
// 생성해야 한다.
// (a) 두 번 실행 → 동일 records
// (b) 모든 record의 utterance == None (FakeBackend는 텍스트를 생성하지 않음)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn inv1_fake_backend_determinism_preserved_with_v05_code() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let seed = 42u64;
    let ticks = 80u64; // 스펙 기준: seed 42, theta 0.65, 80 ticks

    // (a) 동일 seed 두 번 실행 → records 바이트 동일
    let mut sink_a = VecSink::default();
    let mut sink_b = VecSink::default();
    driver::run(&config, &personas, seed, ticks, &mut sink_a, &mut FakeBackend);
    driver::run(&config, &personas, seed, ticks, &mut sink_b, &mut FakeBackend);

    assert_eq!(
        sink_a.records, sink_b.records,
        "INV-1 위반: 동일 seed({seed}) 두 번 실행이 다른 records를 생성함 \
         — v0.5 코드가 결정 경로를 오염시키고 있음"
    );

    // (b) FakeBackend는 발화 텍스트를 생성하지 않으므로 모든 utterance == None
    for record in &sink_a.records {
        assert_eq!(
            record.utterance, None,
            "tick {}: FakeBackend 경로인데 utterance가 None이 아님 \
             — v0.5 모듈이 headless 결정 경로를 오염시키고 있음",
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

// 여러 seed로 결정성을 추가 확인 (회귀 방지)
#[test]
fn inv1_determinism_multiple_seeds() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let ticks = 80u64;

    for seed in [7u64, 99u64] {
        let mut sink_x = VecSink::default();
        let mut sink_y = VecSink::default();
        driver::run(&config, &personas, seed, ticks, &mut sink_x, &mut FakeBackend);
        driver::run(&config, &personas, seed, ticks, &mut sink_y, &mut FakeBackend);

        assert_eq!(
            sink_x.records, sink_y.records,
            "INV-1 위반: seed={seed}에서 두 실행 결과가 다름"
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// HumanChannel (task-28): speak 후 history + excitation 상승
//
// - HumanChannel::new("you") 생성
// - EngineState에 페르소나 2개
// - speak 호출 후 history 마지막 = 사람 Event (speaker/content 일치)
// - 모든 페르소나 excitation이 발화 전보다 증가 (combined_intensities 비교)
// 네트워크 없음. rng 소비 없음.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn human_channel_speak_appends_event_and_raises_excitations() {
    let personas = vec![
        Persona {
            id: "aria".to_string(),
            name: "Aria".to_string(),
            base_rate: 0.4,
        },
        Persona {
            id: "bjorn".to_string(),
            name: "Bjorn".to_string(),
            base_rate: 0.7,
        },
    ];

    let mut state = EngineState {
        intensities: BTreeMap::from([
            ("aria".to_string(), 0.4),
            ("bjorn".to_string(), 0.7),
        ]),
        excitations: BTreeMap::new(),
        history: Vec::new(),
        last_speaker: None,
        rng_seed: 42,
    };

    let channel = HumanChannel::new("you");

    // 발화 전 combined_intensities 기록
    let before = HawkesEngine::combined_intensities(
        &state.intensities,
        &state.excitations,
        &personas,
    );

    channel.speak(&mut state, &personas, "hello smoke gate".to_string(), 3.0);

    // history 마지막 이벤트가 사람 Event여야 한다
    let last = state.history.last().expect("speak 후 history가 비어있지 않아야 한다");
    assert_eq!(last.speaker, "you", "speaker가 'you'여야 한다");
    assert_eq!(
        last.content,
        Some("hello smoke gate".to_string()),
        "content가 입력 텍스트여야 한다"
    );
    assert_eq!(last.ts, 3.0, "ts가 전달된 값이어야 한다");

    // 전 페르소나 excitation이 양수로 상승해야 한다
    for persona in &personas {
        let exc = state.excitations.get(&persona.id).copied().unwrap_or(0.0);
        assert!(
            exc > 0.0,
            "페르소나 {} excitation이 speak 후 양수여야 한다 (실제: {exc})",
            persona.id
        );
    }

    // combined_intensities도 모두 상승해야 한다 (λ = base + excitation)
    let after = HawkesEngine::combined_intensities(
        &state.intensities,
        &state.excitations,
        &personas,
    );
    for persona in &personas {
        let b = before.get(&persona.id).copied().unwrap_or(0.0);
        let a = after.get(&persona.id).copied().unwrap_or(0.0);
        assert!(
            a > b,
            "페르소나 {} combined intensity가 speak 후 상승해야 한다 (before={b:.4}, after={a:.4})",
            persona.id
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// LiveSession 통합 (task-29): 오프라인 풀로 전체 생명주기 검증
//
// - submit_human → 사람 Event + excitation 상승
// - tick → Dispatched 시 placeholder(content=None) + is_pending()
// - pending 중 tick → AwaitingGeneration (새 디스패치 없음)
// - poll_generation bounded 폴링(≤2s) → pending 해제
// - Drop 시 hang/panic 없음
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn live_session_submit_human_appends_event_and_raises_excitations() {
    let pool = offline_pool();
    let mut session = LiveSession::new(base_config(0.5), demo_personas(), 42, pool, "you");

    // 발화 전: excitation 없음, history 없음
    assert!(session.state().excitations.is_empty());
    assert!(session.state().history.is_empty());

    session.submit_human("smoke gate v05".to_string());

    // history에 사람 Event가 push됐어야 한다
    let history = &session.state().history;
    assert_eq!(history.len(), 1, "submit_human 후 history 길이 1이어야 한다");
    let ev = &history[0];
    assert_eq!(ev.speaker, "you");
    assert_eq!(ev.content, Some("smoke gate v05".to_string()));

    // 전 페르소나 excitation이 양수여야 한다
    let excitations = &session.state().excitations;
    for persona in session.personas() {
        let exc = excitations.get(&persona.id).copied().unwrap_or(0.0);
        assert!(
            exc > 0.0,
            "submit_human 후 페르소나 {} excitation이 양수여야 한다 (실제: {exc})",
            persona.id
        );
    }
}

#[test]
fn live_session_tick_dispatches_placeholder_and_blocks_second() {
    let pool = offline_pool();
    let mut session = LiveSession::new(base_config(0.5), demo_personas(), 42, pool, "you");

    // 화자가 선택될 때까지 틱 (최대 50틱)
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

    assert!(dispatched, "50틱 내에 Dispatched가 발생해야 한다");
    // pending이 설정됐어야 한다
    assert!(session.is_pending(), "Dispatched 후 is_pending()이 true여야 한다");

    // history 마지막에 placeholder(content=None) Event가 있어야 한다
    let history = &session.state().history;
    assert!(!history.is_empty(), "Dispatched 후 history가 비어있지 않아야 한다");
    let placeholder = history.last().unwrap();
    assert_eq!(
        placeholder.content, None,
        "placeholder Event의 content는 None이어야 한다"
    );

    // pending 중 추가 tick은 AwaitingGeneration이어야 한다 (새 디스패치 없음)
    let outcome = session.tick();
    assert_eq!(
        outcome,
        TickOutcome::AwaitingGeneration,
        "pending 중 tick은 AwaitingGeneration이어야 한다"
    );
}

#[test]
fn live_session_poll_generation_clears_pending() {
    let pool = offline_pool();
    let mut session = LiveSession::new(base_config(0.5), demo_personas(), 42, pool, "you");

    // 화자가 선택될 때까지 틱
    let mut dispatched = false;
    for _ in 0..50 {
        if let TickOutcome::Dispatched(_) = session.tick() {
            dispatched = true;
            break;
        }
    }
    assert!(dispatched, "화자가 선택돼야 한다");
    assert!(session.is_pending(), "Dispatched 후 pending이어야 한다");

    // bounded 폴링: 오프라인 → 워커가 즉시 None 반환 → 빠르게 해제됨
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut filled: Option<Event> = None;
    while std::time::Instant::now() < deadline {
        if let Some(ev) = session.poll_generation() {
            filled = Some(ev);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let ev = filled.expect("2s 내에 poll_generation이 Event를 반환해야 한다");
    // 오프라인 백엔드 → content는 None (generate_one → None)
    assert_eq!(ev.content, None, "오프라인 백엔드 → content None이어야 한다");
    // pending이 해제됐어야 한다
    assert!(
        !session.is_pending(),
        "poll_generation 후 is_pending()이 false여야 한다"
    );
}

#[test]
fn live_session_drop_does_not_hang_or_panic() {
    let pool = offline_pool();
    {
        let mut session = LiveSession::new(base_config(0.5), demo_personas(), 99, pool, "you");
        // 몇 틱 돌리고 drop
        for _ in 0..10 {
            let _ = session.tick();
        }
    } // Drop here: shutdown() → job_tx drop → 워커 루프 종료 → join
    // hang이나 panic이 없으면 통과
}

// ──────────────────────────────────────────────────────────────────────────────
// render_chat (task-30): TestBackend로 채팅 화면 렌더 검증
//
// - 사람 발화(content=Some), 페르소나 발화(content=Some), placeholder(content=None)
// - render_chat panic 없이 완료
// - 입력 버퍼(ASCII) + 페르소나 이름이 렌더 버퍼에 포함
//
// TestBackend 한글 주의: 2-byte 한글은 셀+공백으로 분리됨.
// → ASCII 입력 텍스트와 페르소나 이름(ASCII)으로만 검증.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn render_chat_no_panic_and_shows_input_and_names() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let history = vec![
        // 사람 발화 (content 있음)
        Event {
            ts: 0.0,
            speaker: "you".to_string(),
            mark: 5.0,
            content: Some("hello smoke v5".to_string()),
        },
        // 페르소나 발화 (content 있음)
        Event {
            ts: 1.0,
            speaker: "aria".to_string(),
            mark: 1.0,
            content: Some("nice to meet you here".to_string()),
        },
        // placeholder (content=None) — pending 시 "생각 중" 표시
        Event {
            ts: 2.0,
            speaker: "bjorn".to_string(),
            mark: 1.0,
            content: None,
        },
    ];

    let intensities: BTreeMap<String, f64> = BTreeMap::from([
        ("aria".to_string(), 0.80),
        ("bjorn".to_string(), 0.70),
    ]);

    let names: BTreeMap<String, String> = BTreeMap::from([
        ("aria".to_string(), "Aria".to_string()),
        ("bjorn".to_string(), "Bjorn".to_string()),
        ("you".to_string(), "You".to_string()),
    ]);

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("TestBackend terminal 생성 실패");

    // panic 없이 렌더돼야 한다
    terminal
        .draw(|f| {
            render_chat(
                f,
                &history,
                &intensities,
                &names,
                0.65,
                "smoke-input-ascii", // ASCII 입력 버퍼
                true,                // pending=true (placeholder가 있으므로)
                None,                // flow: None (smoke_v5는 flow 없음)
                1.0,                 // mu_scale: 1.0 (식힘 없음)
            )
        })
        .expect("render_chat가 panic 없이 완료돼야 한다");

    // TestBackend 버퍼를 문자열로 변환
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();

    // 입력 버퍼 ASCII 문자열이 나타나야 한다
    assert!(
        text.contains("smoke-input-ascii"),
        "입력창에 'smoke-input-ascii'가 나타나야 한다. 실제: {text:?}"
    );

    // '>' 프롬프트가 있어야 한다
    assert!(
        text.contains('>'),
        "입력창에 '>' 프롬프트가 있어야 한다"
    );

    // 페르소나 이름이 사이드바 또는 채팅 pane에 나타나야 한다
    assert!(
        text.contains("Aria"),
        "페르소나 이름 'Aria'가 렌더 버퍼에 나타나야 한다"
    );
}
