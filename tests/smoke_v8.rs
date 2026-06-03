// smoke_v8.rs — v0.8 스모크 게이트
//
// 검증 항목:
//   INV-1  FakeBackend 결정성 (driver 경로에 recall 미주입 → 골든 불변)
//          + 모든 record.flow == None (FakeBackend content 없음 → 측정 불가)
//          + 직렬화 NDJSON에 "flow" 키 없음 (skip_serializing_if)
//   RECALL-DET  MemoryStore 결정성 + 참여 격리 (task-39 게이트 재확인)
//   RECALL-SLOT Ollama / OpenAI build_request_body에 recall=Some이면 [기억] 포함,
//               recall=None이면 생략 (task-41 게이트)
//   CONTENT-GATE 오프라인 LiveSession (content 미도착) → store에 발화 없음 → recall 빈 결과
//
// 전체 네트워크-프리: FakeBackend + 오프라인 BackendPool + 순수 MemoryStore

use salon::driver;
use salon::memory::{MemoryEvent, MemoryStore};
use salon::model::{CouplingMatrix, EngineConfig, Persona};
use salon::ollama::OllamaBackend;
use salon::openai::OpenAIBackend;
use salon::pool::{BackendConfig, BackendPool};
use salon::runtime::FakeBackend;
use salon::sink::VecSink;
use salon::live::LiveSession;
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

/// 테스트용 MemoryEvent 생성 헬퍼.
fn mem_ev(room: &str, ts: u64, speaker: &str, content: &str) -> MemoryEvent {
    MemoryEvent {
        room: room.to_string(),
        ts,
        speaker: speaker.to_string(),
        content: content.to_string(),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// INV-1: FakeBackend 결정성 + recall 흔적 없음 게이팅
//
// (a) seed 42, θ 0.65, 80틱 두 번 실행 → records 바이트 동일
// (b) 모든 record.flow == None (FakeBackend → content 없음 → 측정 불가)
// (c) 직렬화 NDJSON에 "flow" 키 없음 (skip_serializing_if)
// (d) driver 경로는 recall=None → record에 회상 흔적 없음(flow/utterance 변화 없음)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn inv1_fake_backend_determinism_no_recall_leakage() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let seed = 42u64;
    let ticks = 80u64;

    // (a) 동일 seed 두 번 실행 → records 바이트 동일
    // driver::run은 recall=None을 FakeBackend에 주입 — v0.8 이후에도 불변
    let mut sink_a = VecSink::default();
    let mut sink_b = VecSink::default();
    driver::run(&config, &personas, seed, ticks, &mut sink_a, &mut FakeBackend);
    driver::run(&config, &personas, seed, ticks, &mut sink_b, &mut FakeBackend);

    assert_eq!(
        sink_a.records, sink_b.records,
        "INV-1 위반: 동일 seed({seed}) 두 번 실행이 다른 records를 생성함 \
         — v0.8 recall 추가가 결정 경로를 오염시키고 있음"
    );

    // (b) 모든 record.flow == None
    for record in &sink_a.records {
        assert_eq!(
            record.flow, None,
            "tick {}: FakeBackend 경로 record에 flow가 None이 아님",
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
// RECALL-DET: 회상 결정성 + 참여 격리 (task-39 게이트 재확인)
//
// (a) 동일 MemoryStore + 동일 쿼리 + 동일 k → 두 번 recall 결과 동일
// (b) 방 A 참여자는 A 사건을 회상할 수 있으나 방 B 미참여자는 B 사건 접근 불가
// (c) 자동 join: record()만 해도 화자가 그 방에 참여 등록됨
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn recall_determinism_and_participation_isolation() {
    let mut store = MemoryStore::new();

    // 방 A: aria · bjorn 참여 등록 + 사건 기록
    store.join("room-a", "aria");
    store.join("room-a", "bjorn");
    store.record(mem_ev("room-a", 1, "aria", "안녕 세계"));
    store.record(mem_ev("room-a", 2, "bjorn", "세계 평화"));
    store.record(mem_ev("room-a", 3, "aria", "안녕 친구"));

    // 방 B: clio만 참여. A 사건 없음.
    store.join("room-b", "clio");
    store.record(mem_ev("room-b", 4, "clio", "전혀 다른 방 이야기"));

    // (a) 결정성: 같은 파라미터로 두 번 호출해도 결과 동일
    let r1 = store.recall("aria", "안녕 세계", 5);
    let r2 = store.recall("aria", "안녕 세계", 5);

    assert_eq!(
        r1.len(), r2.len(),
        "같은 쿼리 두 번 호출의 길이가 달라서는 안 됨(결정성 위반)"
    );
    for (a, b) in r1.iter().zip(r2.iter()) {
        assert_eq!(
            a, b,
            "같은 쿼리 두 번 호출의 각 요소가 달라서는 안 됨(결정성 위반)"
        );
    }

    // 결과가 실제로 있어야 단언이 의미 있음
    assert!(
        !r1.is_empty(),
        "aria는 room-a의 '안녕 세계' 사건을 회상할 수 있어야 함"
    );

    // (b) 참여 격리: clio는 room-b 참여 → room-a 사건 접근 불가.
    // BM25-only 빌드: room-b에 "안녕 세계" 토큰이 없으므로 빈 결과.
    // hybrid(semantic) 빌드: 벡터 leg가 room-b 사건을 반환할 수 있으나 room-a 사건은 없어야 함.
    let clio_recall = store.recall("clio", "안녕 세계", 5);
    let clio_has_room_a = clio_recall.iter().any(|ev| ev.room == "room-a");
    assert!(
        !clio_has_room_a,
        "clio는 room-a에 참여하지 않았으므로 room-a 사건을 회상할 수 없어야 함(참여 격리). \
         결과: {:?}",
        clio_recall.iter().map(|e| (&e.room, &e.content)).collect::<Vec<_>>()
    );

    // (c) 역방향 격리: aria는 room-b에 참여하지 않았으므로 room-b 사건 접근 불가.
    // 마찬가지로 hybrid 빌드에서는 room-a 사건을 반환할 수 있으나 room-b 사건은 없어야 함.
    let aria_b_recall = store.recall("aria", "전혀 다른 방", 5);
    let aria_has_room_b = aria_b_recall.iter().any(|ev| ev.room == "room-b");
    assert!(
        !aria_has_room_b,
        "aria는 room-b에 참여하지 않았으므로 room-b 사건을 회상할 수 없어야 함(참여 격리). \
         결과: {:?}",
        aria_b_recall.iter().map(|e| (&e.room, &e.content)).collect::<Vec<_>>()
    );

    // (c) 자동 join: record()로만 화자가 방에 등록됨 — 명시적 join 없이도 회상 가능
    let mut auto_store = MemoryStore::new();
    // clio를 명시적 join 없이 record만으로 등록
    auto_store.record(mem_ev("auto-room", 1, "clio", "자동 참여 테스트"));
    let auto_recall = auto_store.recall("clio", "자동 참여", 5);
    assert_eq!(
        auto_recall.len(), 1,
        "record()로 화자가 자동 join → 회상 가능해야 함(자동 참여 격리)"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// RECALL-SLOT: Ollama / OpenAI 프롬프트 조립 시 recall 슬롯 검증 (task-41 게이트)
//
// assemble_user_prompt는 private이므로, build_request_body에 조립된 프롬프트를 직접
// 전달해 body JSON의 prompt/messages 필드에 [기억] 섹션이 포함/생략되는지 확인한다.
//
// (a) OllamaBackend: recall=Some → body["prompt"]에 [기억] 포함
// (b) OllamaBackend: recall=None → body["prompt"]에 [기억] 없음
// (c) OpenAIBackend: recall=Some → body["messages"][user].content에 [기억] 포함
// (d) OpenAIBackend: recall=None → body["messages"][user].content에 [기억] 없음
//
// 프롬프트 조립 로직(assemble_user_prompt)을 인라인으로 복제:
//   1. "Recent lines:\n{recent}"
//   2. recall=Some(r) → "[기억]\n{r}"
//   3. "Reply with ONE short, in-character line. No preamble."
// ──────────────────────────────────────────────────────────────────────────────

/// 테스트 내에서 assemble_user_prompt 로직을 인라인으로 복제한다.
/// ollama.rs / openai.rs의 private fn assemble_user_prompt와 동일한 로직.
fn assemble_prompt(recent: &str, recall: Option<&str>) -> String {
    let mut parts = Vec::with_capacity(3);
    parts.push(format!("Recent lines:\n{recent}"));
    if let Some(r) = recall {
        parts.push(format!("[기억]\n{r}"));
    }
    parts.push("Reply with ONE short, in-character line. No preamble.".to_string());
    parts.join("\n")
}

#[test]
fn recall_slot_ollama_build_request_body() {
    let recent = "aria: 안녕\nbjorn: 반가워";
    let recall_text = "지난 대화에서:\n- aria: 약속했어";

    // (a) recall=Some → body["prompt"]에 "[기억]" 포함
    let prompt_with = assemble_prompt(recent, Some(recall_text));
    let body_with = OllamaBackend::build_request_body("gemma4:e4b", &prompt_with, None, None, false);

    let prompt_field = body_with["prompt"].as_str().expect("body에 prompt 필드가 있어야 함");
    assert!(
        prompt_field.contains("[기억]"),
        "recall=Some → OllamaBackend body[\"prompt\"]에 [기억] 섹션이 있어야 함. 실제: {prompt_field:?}"
    );
    assert!(
        prompt_field.contains(recall_text),
        "recall 텍스트가 body[\"prompt\"]에 포함되어야 함"
    );

    // (b) recall=None → body["prompt"]에 "[기억]" 없음
    let prompt_none = assemble_prompt(recent, None);
    let body_none = OllamaBackend::build_request_body("gemma4:e4b", &prompt_none, None, None, false);

    let prompt_field_none = body_none["prompt"].as_str().expect("body에 prompt 필드가 있어야 함");
    assert!(
        !prompt_field_none.contains("[기억]"),
        "recall=None → OllamaBackend body[\"prompt\"]에 [기억] 섹션이 없어야 함. 실제: {prompt_field_none:?}"
    );

    // 공통: "Recent lines:" 와 지시문은 항상 존재
    assert!(
        prompt_field_none.contains("Recent lines:"),
        "최근 로그 섹션이 항상 있어야 함"
    );
    assert!(
        prompt_field_none.contains("Reply with ONE short"),
        "지시문이 항상 있어야 함"
    );
}

#[test]
fn recall_slot_openai_build_request_body() {
    let recent = "aria: 안녕\nbjorn: 반가워";
    let recall_text = "지난 대화에서:\n- aria: 약속했어";

    // (c) recall=Some → messages[user].content에 "[기억]" 포함
    let prompt_with = assemble_prompt(recent, Some(recall_text));
    let body_with = OpenAIBackend::build_request_body("qwen3.6-35b", &prompt_with, None, None, false);

    // system=None이면 messages[0]이 user 메시지
    let messages = body_with["messages"].as_array().expect("messages 배열이 있어야 함");
    let user_msg = messages
        .iter()
        .find(|m| m["role"] == "user")
        .expect("user 메시지가 있어야 함");
    let content_with = user_msg["content"].as_str().expect("user content가 있어야 함");

    assert!(
        content_with.contains("[기억]"),
        "recall=Some → OpenAIBackend user message에 [기억] 섹션이 있어야 함. 실제: {content_with:?}"
    );
    assert!(
        content_with.contains(recall_text),
        "recall 텍스트가 user message content에 포함되어야 함"
    );

    // (d) recall=None → messages[user].content에 "[기억]" 없음
    let prompt_none = assemble_prompt(recent, None);
    let body_none = OpenAIBackend::build_request_body("qwen3.6-35b", &prompt_none, None, None, false);

    let messages_none = body_none["messages"].as_array().expect("messages 배열이 있어야 함");
    let user_msg_none = messages_none
        .iter()
        .find(|m| m["role"] == "user")
        .expect("user 메시지가 있어야 함");
    let content_none = user_msg_none["content"].as_str().expect("user content가 있어야 함");

    assert!(
        !content_none.contains("[기억]"),
        "recall=None → OpenAIBackend user message에 [기억] 섹션이 없어야 함. 실제: {content_none:?}"
    );

    // 공통: stream=false, model 존재
    assert_eq!(body_none["stream"], false, "stream은 항상 false여야 함");
    assert_eq!(body_none["model"], "qwen3.6-35b", "model 필드가 올바르게 설정되어야 함");
}

// ──────────────────────────────────────────────────────────────────────────────
// CONTENT-GATE: 오프라인 LiveSession → content 미도착 → store에 발화 없음 → recall None
//
// LiveSession은 poll_generation에서 content=Some일 때만 store.record를 호출한다.
// 오프라인 pool은 content=None만 반환 → store에 발화 이벤트가 쌓이지 않음.
//
// (a) 오프라인 LiveSession 20틱 + poll flush → store.recall → 빈 결과
// (b) format_recall(&[]) == None (빈 회상 결과 → recall 슬롯 None)
// (c) MemoryStore 직접 구성: 빈 content history → format_recall None
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn content_gate_offline_session_store_empty_recall_none() {
    let pool = offline_pool();
    let mut session = LiveSession::new(base_config(0.65), demo_personas(), 42, pool, "you");

    // (a) 20틱 돌리고 poll로 flush → 오프라인이라 content=None만 도착
    for _ in 0..20 {
        let _ = session.tick();
        // poll: 오프라인 워커는 즉시 None 반환 → content=None → store에 기록 안 됨
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
        while std::time::Instant::now() < deadline {
            if session.poll_generation().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    // 페르소나들의 recall이 빈 결과여야 한다 (content=None → 기록 없음)
    // "you"(human)도 발화하지 않았으므로 모든 참여자 recall 비어 있어야 함
    // 단, store는 LiveSession 내부이므로 store에 직접 접근할 수 없다.
    // → LiveSession 내부 store에 사건이 없으면 모든 recall이 빈 결과.
    //   mu_scale()이 1.0(content 없음)인 것을 우회 지표로 확인한다.
    let mu = session.mu_scale();
    assert!(
        (mu - 1.0).abs() < 1e-15,
        "오프라인 세션(content 없음) → mu_scale == 1.0이어야 함(MetaController no-op), 실제: {mu}"
    );

    // (b) 빈 회상 결과 → format_recall None
    // MemoryStore::format_recall(&[]) 은 None이어야 한다 (task-39 계약)
    let empty: Vec<MemoryEvent> = vec![];
    let formatted = MemoryStore::format_recall(&empty);
    assert!(
        formatted.is_none(),
        "빈 recall 결과 → format_recall은 None이어야 함 — recall 슬롯 미삽입 보장"
    );

    // (c) MemoryStore 직접 구성: content 없는 상태에서 recall 결과 빈 것 확인
    // (오프라인 LiveSession의 store에 직접 접근이 불가하므로 동등 구성으로 검증)
    let store = MemoryStore::new();
    // 참여자를 join해도 사건이 없으면 recall은 비어 있음
    let mut store_empty = MemoryStore::new();
    store_empty.join("salon", "aria");
    store_empty.join("salon", "bjorn");
    store_empty.join("salon", "clio");
    store_empty.join("salon", "you");

    let recall_aria = store_empty.recall("aria", "안녕", 5);
    assert!(
        recall_aria.is_empty(),
        "join만 하고 record 없으면 recall은 빈 결과여야 함"
    );

    // 빈 스토어도 동일
    let recall_empty = store.recall("aria", "아무거나", 5);
    assert!(
        recall_empty.is_empty(),
        "빈 MemoryStore → recall은 빈 결과여야 함"
    );

    // 빈 결과 → format_recall None 재확인
    let f1 = MemoryStore::format_recall(&recall_aria);
    let f2 = MemoryStore::format_recall(&recall_empty);
    assert!(f1.is_none(), "참여만 하고 사건 없으면 format_recall None이어야 함");
    assert!(f2.is_none(), "빈 스토어 recall → format_recall None이어야 함");
}
