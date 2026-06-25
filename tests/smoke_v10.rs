//! v0.10 friend engine 의미검색(hybrid RRF recall + OrtEmbedder 배선) 통합 게이트.
//!
//! 핵심 불변:
//!   - feature **off**(기본 빌드): 헤드리스 FakeBackend 결정성 유지 = 골든 경로 보존.
//!   - feature **friend-engine-semantic**: hybrid recall 결정적, 참여 격리 유지.
//!   - `open_with_embedder`: 같은 임베더 kind로 재오픈 시 기존 사건 recall 정상.
//!
//! 상세 단위는 memory.rs(open_with_embedder/임베더 일관성) + tests/recall_eval.rs(채점).
//! 여기서는 v0.10 계약을 한 파일로 박는다.

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

/// (1) 기본 빌드(friend-engine-semantic off): 같은 seed 두 번 실행 -> records 바이트 동일.
/// v0.10 코드가 v0.1~v0.9 결정성(골든 불변식)을 깨지 않음을 확인한다.
#[test]
fn v10_default_build_is_deterministic() {
    let config = base_config();
    let personas = personas();
    let mut a = VecSink::default();
    let mut b = VecSink::default();
    driver::run(&config, &personas, 42, 120, &mut a, &mut FakeBackend);
    driver::run(&config, &personas, 42, 120, &mut b, &mut FakeBackend);
    assert_eq!(
        a.records, b.records,
        "v0.10 불변식 위반: 같은 seed 두 번 실행이 다른 records를 생성함(결정성/골든)"
    );
}

/// (2) hybrid recall 결정성 + 참여 격리 (friend-engine-semantic, non-windows).
///
/// MemoryStore::new()(MockEmbedder, hybrid recall).
/// - 같은 쿼리 두 번 -> 동일 결과(결정성).
/// - 미참여 페르소나는 빈 결과(참여 격리).
#[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
#[test]
fn v10_hybrid_recall_deterministic_and_isolated() {
    use salon::memory::{MemoryEvent, MemoryStore};

    let mut store = MemoryStore::new(); // semantic -> MockEmbedder, hybrid recall
    store.join("morning", "ada");
    store.join("evening", "bora");

    store.record(MemoryEvent {
        room: "morning".to_string(),
        ts: 1,
        speaker: "ada".to_string(),
        content: "다음주 북한산 등산 약속 잡았어".to_string(),
    });
    store.record(MemoryEvent {
        room: "morning".to_string(),
        ts: 2,
        speaker: "ada".to_string(),
        content: "오늘 날씨 정말 맑다".to_string(),
    });
    store.record(MemoryEvent {
        room: "morning".to_string(),
        ts: 3,
        speaker: "ada".to_string(),
        content: "점심 뭐 먹을까 고민 중".to_string(),
    });

    // 결정성: 같은 사건열+쿼리 -> 같은 회상
    let r1 = store.recall("ada", "등산 약속", 5);
    let r2 = store.recall("ada", "등산 약속", 5);
    assert_eq!(r1.len(), r2.len(), "hybrid recall 결정성: 길이 동일");
    for (a, b) in r1.iter().zip(r2.iter()) {
        assert_eq!(a, b, "hybrid recall 결정성: 사건 동일");
    }

    // ada는 morning에서 등산 관련 사건을 회상할 수 있어야 한다
    assert!(
        r1.iter().any(|e| e.content.contains("등산")),
        "ada는 morning 등산 사건을 회상해야 한다. 결과: {:?}",
        r1.iter().map(|e| &e.content).collect::<Vec<_>>()
    );

    // 미참여 페르소나(cara) -> 빈 결과(참여 격리)
    let r_cara = store.recall("cara", "등산 약속", 5);
    assert!(
        r_cara.is_empty(),
        "참여 격리 위반: cara(미참여)가 morning 사건을 회상하면 안 된다"
    );
}

/// (3) 임베더 일관성: 같은 kind로 재오픈 -> 기존 사건 정상 recall.
///
/// open(tmp)(MockEmbedder, kind="mock") -> record -> drop -> 같은 경로 open(tmp) 재오픈
/// -> recall에 사건 존재(같은 kind라 재구축 없이 정상).
#[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
#[test]
fn v10_embedder_consistency_same_kind_reopens_clean() {
    use salon::memory::{MemoryEvent, MemoryStore};

    let tmp_dir = std::env::temp_dir();
    let pid = std::process::id();
    let db_path = tmp_dir.join(format!("tunasalon_v10_consistency_{pid}.db"));
    let usearch_path = tmp_dir.join(format!("tunasalon_v10_consistency_{pid}.db.usearch"));

    // 잔여 파일 정리
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&usearch_path);
    for suf in &["-wal", "-shm"] {
        let _ =
            std::fs::remove_file(tmp_dir.join(format!("tunasalon_v10_consistency_{pid}{suf}.db")));
    }

    // 1단계: 파일 DB 열고(MockEmbedder) 사건 기록 후 drop
    {
        let mut store = MemoryStore::open(&db_path).expect("임시 경로 open()이 성공해야 한다");
        store.join("salon", "alice");
        store.record(MemoryEvent {
            room: "salon".to_string(),
            ts: 1,
            speaker: "alice".to_string(),
            content: "임베더 일관성 테스트 안녕".to_string(),
        });
        // drop 시 커넥션 닫힘, WAL checkpoint 발생
    }
    // drop -> 커넥션 닫힘

    // 2단계: 같은 경로로 재오픈(MockEmbedder, 같은 kind) -> recall에 사건 존재
    {
        let store = MemoryStore::open(&db_path).expect("재오픈이 성공해야 한다");
        let result = store.recall("alice", "임베더 일관성 테스트", 5);
        assert!(
            !result.is_empty(),
            "재오픈 후 recall에 이전 사건이 있어야 한다(같은 kind Mock -> Mock 일관)"
        );
        assert!(
            result
                .iter()
                .any(|e| e.content.contains("임베더 일관성 테스트")),
            "재오픈 recall 결과에 '임베더 일관성 테스트'가 없다. 결과: {:?}",
            result.iter().map(|e| &e.content).collect::<Vec<_>>()
        );
    }

    // 정리
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&usearch_path);
    for suf in &["-wal", "-shm"] {
        let _ =
            std::fs::remove_file(tmp_dir.join(format!("tunasalon_v10_consistency_{pid}{suf}.db")));
    }
}
