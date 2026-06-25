//! v0.9 friend engine 심화(Stage 0 형태소 + Stage 1 SQLite/FTS5 BM25) 통합 게이트.
//!
//! 핵심 불변:
//!   - feature **off**(기본 빌드): 헤드리스 FakeBackend 결정성 유지 = v0.8 골든 경로 보존.
//!   - feature **on**: `MemoryStore`가 SQLite+FTS5 BM25 회상. 결정적이고 참여 격리(다른 방 사건 회상 불가).
//!
//! 상세 단위는 memory.rs(SQLite recall/OR-MATCH/영속 roundtrip) + tests/recall_eval.rs(검색층 채점).
//! 여기서는 v0.9 계약을 한 파일로 박는다.

use salon::driver;
use salon::model::{CouplingMatrix, EngineConfig, Persona};
use salon::runtime::FakeBackend;
use salon::sink::VecSink;

fn personas() -> Vec<Persona> {
    vec![
        Persona {
            id: "friend".to_string(),
            name: "Friendly Regular".to_string(),
            base_rate: 0.80,
        },
        Persona {
            id: "chaos".to_string(),
            name: "Grounded Realist".to_string(),
            base_rate: 0.70,
        },
        Persona {
            id: "summarizer".to_string(),
            name: "Quiet Summarizer".to_string(),
            base_rate: 0.25,
        },
    ]
}

fn base_config() -> EngineConfig {
    EngineConfig {
        beta: 0.5,
        theta: 0.65,
        k: 60.0,
        tick_interval: 1.0,
        alpha: CouplingMatrix::default(),
        forbid_self_repeat: false,
    }
}

/// (1) 기본 빌드(friend-engine off): 같은 seed 두 번 실행 → records 바이트 동일.
/// v0.9 코드가 v0.1~v0.8 결정성(골든 불변식)을 깨지 않음을 확인한다.
#[test]
fn v9_default_build_is_deterministic() {
    let config = base_config();
    let personas = personas();
    let mut a = VecSink::default();
    let mut b = VecSink::default();
    driver::run(&config, &personas, 42, 120, &mut a, &mut FakeBackend);
    driver::run(&config, &personas, 42, 120, &mut b, &mut FakeBackend);
    assert_eq!(
        a.records, b.records,
        "v0.9 불변식 위반: 같은 seed 두 번 실행이 다른 records를 생성함(결정성/골든)"
    );
}

/// (2) feature on: SQLite + FTS5 BM25 회상이 결정적이고 참여 격리를 지킨다.
#[cfg(feature = "friend-engine")]
#[test]
fn v9_sqlite_recall_deterministic_and_participation_isolated() {
    use salon::memory::{MemoryEvent, MemoryStore};

    let mut store = MemoryStore::new(); // feature on → :memory: SQLite
    store.join("morning", "ada");
    store.join("evening", "bora");
    store.record(MemoryEvent {
        room: "morning".to_string(),
        ts: 1,
        speaker: "ada".to_string(),
        content: "다음주 북한산 등산 약속 잡았어".to_string(),
    });

    // 결정성: 같은 사건열+쿼리 → 같은 회상.
    let r1 = store.recall("ada", "등산 약속", 5);
    let r2 = store.recall("ada", "등산 약속", 5);
    assert_eq!(r1, r2, "SQLite 회상이 결정적이어야 한다");
    assert!(
        r1.iter().any(|e| e.content.contains("등산")),
        "ada는 morning 사건을 회상해야 한다. 결과: {:?}",
        r1.iter().map(|e| &e.content).collect::<Vec<_>>()
    );

    // 참여 격리: bora는 morning에 없었으므로 회상 불가.
    assert!(
        store.recall("bora", "등산 약속", 5).is_empty(),
        "참여 격리 위반: bora가 morning 사건을 회상하면 안 된다"
    );

    // format_recall: 비면 None.
    assert!(MemoryStore::format_recall(&[]).is_none());
}
