// recall_eval.rs — 회상 평가 하네스 (task-40)
//
// 목적: SSOT(정답)와 distractor(함정)를 심은 회상방 시나리오로
//       MemoryStore 검색층을 자동 채점한다.
//
// 시나리오
//   room "morning": A(ada), B(bora) 참여.
//     SSOT:       "다음주 화요일 북한산 등산 약속"
//     distractor: "지난주 등산 비 취소"
//     filler:     "오늘 날씨 맑다", "점심 뭐 먹을까"
//   room "evening": B(bora), C(cara) 참여.
//     SSOT:       "금요일 저녁 영화 관람 약속"
//     distractor: "저번 주 영화 취소 됐어"
//     filler:     "퇴근 후 카페 갈까", "오늘 좀 피곤해"
//   참여 비대칭:
//     cara(C)는 morning에 없음 → morning SSOT 회상 불가.
//     ada(A)는  evening에 없음 → evening SSOT 회상 불가.
//
// 합격 임계(v0.8 시작 기준): SSOT가 top-3 안 → 재현 통과.
//   (이후 회상 엔진 정밀도 향상에 따라 top-1 요건으로 올릴 수 있음.)
//
// 결정성 보장: 시나리오 고정 + MemoryStore 결정적 → rng/벽시계 없음.
// task-44: recall 반환형이 Vec<MemoryEvent>(owned)로 변경됨.

use salon::memory::{MemoryEvent, MemoryStore};

// ─────────────────────────────────────────────────────────────────────────────
// 채점 헬퍼
// ─────────────────────────────────────────────────────────────────────────────

/// recall@k: 결과 상위 k 개 안에 `ssot_substring`을 content에 포함하는 사건이 있는가.
///
/// 합격 임계: top-3(k=3) → v0.8 재현 통과 기준.
/// 이후 임베딩 기반 검색 도입 시 임계를 낮출(top-1) 수 있다.
fn recall_at_k(results: &[MemoryEvent], ssot_substring: &str, k: usize) -> bool {
    results
        .iter()
        .take(k)
        .any(|ev| ev.content.contains(ssot_substring))
}

/// 정확도 체크: 결과에서 SSOT가 distractor보다 앞(낮은 인덱스)에 오는가.
///
/// SSOT 위치가 distractor 위치보다 앞이면 true.
/// 둘 중 하나가 결과에 없으면 None(판단 불가).
fn ssot_ranks_above_distractor(
    results: &[MemoryEvent],
    ssot_substring: &str,
    distractor_substring: &str,
) -> Option<bool> {
    let ssot_pos = results
        .iter()
        .position(|ev| ev.content.contains(ssot_substring));
    let distractor_pos = results
        .iter()
        .position(|ev| ev.content.contains(distractor_substring));

    match (ssot_pos, distractor_pos) {
        (Some(s), Some(d)) => Some(s < d),
        // distractor가 결과에 없으면 SSOT가 이기는 셈(또는 판단 불가)
        (Some(_), None) => Some(true),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 시나리오 빌더
// ─────────────────────────────────────────────────────────────────────────────

/// 테스트 공용 이벤트 생성 헬퍼
fn ev(room: &str, ts: u64, speaker: &str, content: &str) -> MemoryEvent {
    MemoryEvent {
        room: room.to_string(),
        ts,
        speaker: speaker.to_string(),
        content: content.to_string(),
    }
}

/// 회상 평가 시나리오를 담은 `MemoryStore`를 반환한다.
///
/// 페르소나: ada(A), bora(B), cara(C)
///   - room "morning": ada, bora 참여
///   - room "evening": bora, cara 참여
///   → cara는 morning에 없음. ada는 evening에 없음.
///
/// ts는 단조 증가 → 동점 tie-break가 명확하다.
fn build_scenario() -> MemoryStore {
    let mut store = MemoryStore::new();

    // morning 참여 선언 (ada, bora)
    store.join("morning", "ada");
    store.join("morning", "bora");

    // evening 참여 선언 (bora, cara)
    store.join("evening", "bora");
    store.join("evening", "cara");

    // ── room "morning" 이벤트 ────────────────────────────────────────────
    // filler 1 (토큰 겹침 없음 — 쿼리와 무관)
    store.record(ev("morning", 10, "ada", "오늘 날씨 맑다"));
    // distractor: "등산" 토큰 포함하지만 쿼리 핵심 토큰(다음주, 화요일, 약속)은 없음
    // → 쿼리 "다음주 화요일 등산 약속"과의 교집합 = {등산} (score=1)
    store.record(ev("morning", 20, "bora", "지난주 등산 비 취소"));
    // filler 2
    store.record(ev("morning", 30, "ada", "점심 뭐 먹을까"));
    // SSOT: 쿼리 "다음주 화요일 등산 약속"과의 교집합 = {다음주, 화요일, 등산, 약속} (score=4)
    store.record(ev("morning", 40, "bora", "다음주 화요일 북한산 등산 약속"));

    // ── room "evening" 이벤트 ────────────────────────────────────────────
    // filler 1
    store.record(ev("evening", 50, "cara", "퇴근 후 카페 갈까"));
    // distractor: "영화" 토큰 포함하지만 쿼리 핵심 토큰(금요일, 저녁, 약속)은 없음
    // → 쿼리 "금요일 저녁 영화 약속"과의 교집합 = {영화} (score=1)
    store.record(ev("evening", 60, "bora", "저번 주 영화 취소 됐어"));
    // filler 2
    store.record(ev("evening", 70, "cara", "오늘 좀 피곤해"));
    // SSOT: 쿼리 "금요일 저녁 영화 약속"과의 교집합 = {금요일, 저녁, 영화, 약속} (score=4)
    store.record(ev("evening", 80, "bora", "금요일 저녁 영화 관람 약속"));

    store
}

// SSOT content 중 쿼리와 교집합이 큰 핵심 부분문자열
const MORNING_SSOT: &str = "다음주 화요일 북한산 등산 약속";
const MORNING_DISTRACTOR: &str = "지난주 등산 비 취소";
const MORNING_QUERY: &str = "다음주 화요일 등산 약속";

const EVENING_SSOT: &str = "금요일 저녁 영화 관람 약속";
const EVENING_DISTRACTOR: &str = "저번 주 영화 취소 됐어";
const EVENING_QUERY: &str = "금요일 저녁 영화 약속";

// ─────────────────────────────────────────────────────────────────────────────
// 테스트 케이스
// ─────────────────────────────────────────────────────────────────────────────

/// (1) 재현(recall): morning 참여자(ada)가 SSOT 주제 쿼리 → SSOT가 top-3 안.
///
/// 쿼리 "다음주 화요일 등산 약속"은 SSOT와 토큰 4개 공유(score=4),
/// distractor와는 1개 공유(score=1) → SSOT가 1위.
/// 합격 임계: recall@3 = true(v0.8 시작 기준).
#[test]
fn recall_morning_ssot_in_top3() {
    let store = build_scenario();
    let results = store.recall("ada", MORNING_QUERY, 5);

    assert!(
        !results.is_empty(),
        "ada의 morning 쿼리 결과가 비어 있으면 안 된다"
    );

    assert!(
        recall_at_k(&results, MORNING_SSOT, 3),
        "재현 실패: morning SSOT '{MORNING_SSOT}'이 top-3 안에 없다. \
         결과: {:?}",
        results.iter().map(|e| &e.content).collect::<Vec<_>>()
    );
}

/// (2) 정확도(precision): 같은 쿼리에서 SSOT가 distractor보다 앞서야 한다.
///
/// SSOT score=4, distractor score=1 → SSOT가 rank 1.
/// SSOT가 distractor보다 위에 있어야 채점 통과.
#[test]
fn precision_ssot_ranks_above_distractor_morning() {
    let store = build_scenario();
    let results = store.recall("ada", MORNING_QUERY, 5);

    let ranks_above = ssot_ranks_above_distractor(&results, MORNING_SSOT, MORNING_DISTRACTOR)
        .expect("SSOT가 결과에 있어야 한다(정밀도 판단 불가 상태)");

    assert!(
        ranks_above,
        "정확도 실패: morning distractor '{MORNING_DISTRACTOR}'이 SSOT '{MORNING_SSOT}'보다 \
         위에 나타났다. 결과: {:?}",
        results.iter().map(|e| &e.content).collect::<Vec<_>>()
    );
}

/// (3) 참여 격리: cara(C)는 morning에 없으므로 morning SSOT를 회상할 수 없다.
///
/// cara는 "evening"에만 join → morning 사건은 후보에서 제외(참여 격리).
/// recall("cara", MORNING_QUERY, 5) → morning 사건 없음.
#[test]
fn participation_isolation_cara_cannot_recall_morning() {
    let store = build_scenario();
    let results = store.recall("cara", MORNING_QUERY, 5);

    // cara는 morning에 없으므로 morning 사건이 결과에 포함되면 안 된다
    let has_morning = results.iter().any(|ev| ev.room == "morning");
    assert!(
        !has_morning,
        "참여 격리 실패: cara(morning 미참여)의 쿼리 결과에 morning 사건이 있다. \
         결과: {:?}",
        results
            .iter()
            .map(|e| (&e.room, &e.content))
            .collect::<Vec<_>>()
    );

    // 구체적으로 morning SSOT도 없어야 한다
    let has_morning_ssot = results.iter().any(|ev| ev.content.contains(MORNING_SSOT));
    assert!(
        !has_morning_ssot,
        "참여 격리 실패: cara 결과에 morning SSOT '{MORNING_SSOT}'이 있다"
    );
}

/// (4a) evening 대칭 — 재현: bora(B/evening 참여자)가 evening SSOT를 top-3에서 회상한다.
#[test]
fn recall_evening_ssot_in_top3_for_bora() {
    let store = build_scenario();
    let results = store.recall("bora", EVENING_QUERY, 5);

    assert!(
        recall_at_k(&results, EVENING_SSOT, 3),
        "재현 실패(evening/bora): SSOT '{EVENING_SSOT}'이 top-3 안에 없다. \
         결과: {:?}",
        results.iter().map(|e| &e.content).collect::<Vec<_>>()
    );
}

/// (4b) evening 대칭 — 재현: cara(C/evening 참여자)가 evening SSOT를 top-3에서 회상한다.
#[test]
fn recall_evening_ssot_in_top3_for_cara() {
    let store = build_scenario();
    let results = store.recall("cara", EVENING_QUERY, 5);

    assert!(
        recall_at_k(&results, EVENING_SSOT, 3),
        "재현 실패(evening/cara): SSOT '{EVENING_SSOT}'이 top-3 안에 없다. \
         결과: {:?}",
        results.iter().map(|e| &e.content).collect::<Vec<_>>()
    );
}

/// (4c) ada(A)는 evening에 없으므로 evening SSOT를 회상할 수 없다.
///
/// ada는 "morning"에만 join → evening 사건은 후보에서 제외.
#[test]
fn participation_isolation_ada_cannot_recall_evening() {
    let store = build_scenario();
    let results = store.recall("ada", EVENING_QUERY, 5);

    let has_evening = results.iter().any(|ev| ev.room == "evening");
    assert!(
        !has_evening,
        "참여 격리 실패: ada(evening 미참여)의 쿼리 결과에 evening 사건이 있다. \
         결과: {:?}",
        results
            .iter()
            .map(|e| (&e.room, &e.content))
            .collect::<Vec<_>>()
    );
}

/// (5) 결정성: 같은 시나리오 + 같은 쿼리로 두 번 recall → 동일 결과.
///
/// build_scenario()가 결정적 + recall이 결정적 → 보장.
/// task-44: Vec<MemoryEvent>(owned) 직접 비교.
#[test]
fn recall_is_deterministic_across_two_runs() {
    // 첫 번째 실행
    let store1 = build_scenario();
    let r1_morning = store1.recall("ada", MORNING_QUERY, 5);
    let r1_evening_b = store1.recall("bora", EVENING_QUERY, 5);

    // 두 번째 실행 (독립적으로 새 스토어 빌드)
    let store2 = build_scenario();
    let r2_morning = store2.recall("ada", MORNING_QUERY, 5);
    let r2_evening_b = store2.recall("bora", EVENING_QUERY, 5);

    // Vec<MemoryEvent>: MemoryEvent가 PartialEq를 구현하므로 직접 비교 가능
    assert_eq!(
        r1_morning.len(),
        r2_morning.len(),
        "결정성 실패: morning 결과 길이가 다르다"
    );
    for (a, b) in r1_morning.iter().zip(r2_morning.iter()) {
        assert_eq!(a, b, "결정성 실패: morning 결과 사건이 다르다");
    }

    assert_eq!(
        r1_evening_b.len(),
        r2_evening_b.len(),
        "결정성 실패: evening(bora) 결과 길이가 다르다"
    );
    for (a, b) in r1_evening_b.iter().zip(r2_evening_b.iter()) {
        assert_eq!(a, b, "결정성 실패: evening(bora) 결과 사건이 다르다");
    }
}

/// (4d) 정확도(precision): evening 쿼리에서도 SSOT가 distractor보다 앞서야 한다.
///
/// SSOT score=4, distractor score=1 → SSOT rank 1.
#[test]
fn precision_ssot_ranks_above_distractor_evening() {
    let store = build_scenario();
    let results = store.recall("cara", EVENING_QUERY, 5);

    let ranks_above = ssot_ranks_above_distractor(&results, EVENING_SSOT, EVENING_DISTRACTOR)
        .expect("evening SSOT가 결과에 있어야 한다(정밀도 판단 불가 상태)");

    assert!(
        ranks_above,
        "정확도 실패: evening distractor '{EVENING_DISTRACTOR}'이 SSOT '{EVENING_SSOT}'보다 \
         위에 나타났다. 결과: {:?}",
        results.iter().map(|e| &e.content).collect::<Vec<_>>()
    );
}

/// 실모델 의미 회상: 어휘가 거의 겹치지 않아도 의미로 회상한다.
/// 저장 "강아지랑 산책 다녀왔어" / 쿼리 "반려동물 데리고 나갔어".
/// BM25(어휘)로는 약하고 BGE-M3 벡터(의미)가 잡아야 한다.
///
/// 수동 실행: cargo test --features "friend-engine-semantic coreml" -- --ignored semantic_recall_real_model --nocapture
#[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
#[test]
#[ignore]
fn semantic_recall_real_model_lexical_differs_meaning_same() {
    use salon::embed::{model_manager, OrtEmbedder};

    let model_dir = model_manager::default_model_path();
    assert!(
        model_manager::is_downloaded(&model_dir),
        "모델이 없다: {} (먼저 다운로드). 이 테스트는 #[ignore]라 일상 CI에서는 제외된다.",
        model_dir.display()
    );

    let embedder = OrtEmbedder::new(&model_dir).expect("OrtEmbedder::new 실패");

    let tmp_dir = std::env::temp_dir();
    let pid = std::process::id();
    let db_path = tmp_dir.join(format!("tunasalon_sem_real_{pid}.db"));
    let usearch_path = tmp_dir.join(format!("tunasalon_sem_real_{pid}.db.usearch"));
    // 잔여 정리
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&usearch_path);
    for suf in &["-wal", "-shm"] {
        let _ = std::fs::remove_file(tmp_dir.join(format!("tunasalon_sem_real_{pid}{suf}.db")));
    }

    {
        let mut store = MemoryStore::open_with_embedder(&db_path, Box::new(embedder))
            .expect("open_with_embedder 실패");
        store.join("salon", "ada");
        // SSOT: 어휘가 쿼리와 거의 안 겹침
        store.record(MemoryEvent {
            room: "salon".to_string(),
            ts: 1,
            speaker: "ada".to_string(),
            content: "강아지랑 산책 다녀왔어".to_string(),
        });
        // distractor/filler: 의미 무관
        store.record(MemoryEvent {
            room: "salon".to_string(),
            ts: 2,
            speaker: "ada".to_string(),
            content: "오늘 주식 시장이 폭락했대".to_string(),
        });
        store.record(MemoryEvent {
            room: "salon".to_string(),
            ts: 3,
            speaker: "ada".to_string(),
            content: "새 노트북 사양 알아보는 중".to_string(),
        });

        let results = store.recall("ada", "반려동물 데리고 나갔어", 3);
        eprintln!(
            "[semantic_recall_real_model] results: {:?}",
            results.iter().map(|e| &e.content).collect::<Vec<_>>()
        );
        assert!(
            results.iter().any(|e| e.content.contains("강아지랑 산책")),
            "실모델 의미 회상 실패: top-3에 '강아지랑 산책'이 없다. 어휘≠의미 회상이 동작해야 한다. 결과: {:?}",
            results.iter().map(|e| &e.content).collect::<Vec<_>>()
        );
    }

    // 정리
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&usearch_path);
    for suf in &["-wal", "-shm"] {
        let _ = std::fs::remove_file(tmp_dir.join(format!("tunasalon_sem_real_{pid}{suf}.db")));
    }
}

/// (6) format_recall 연기: morning SSOT 회상 결과를 회상 슬롯 문자열로 포맷한다.
///
/// 직접 채점은 아니지만 회상 결과가 프롬프트 슬롯에 올바르게 포맷되는지 확인.
/// 결과가 비어 있지 않으면 "지난 대화에서:" 헤더를 포함해야 한다.
#[test]
fn format_recall_morning_ssot_non_empty() {
    let store = build_scenario();
    let results = store.recall("ada", MORNING_QUERY, 3);

    // SSOT가 결과에 있어야 format_recall이 의미 있다
    assert!(
        recall_at_k(&results, MORNING_SSOT, 3),
        "format_recall 전제 실패: morning SSOT가 top-3에 없다"
    );

    let formatted = MemoryStore::format_recall(&results).expect("비어 있지 않은 결과 → Some");
    assert!(
        formatted.starts_with("지난 대화에서:"),
        "format_recall 결과가 '지난 대화에서:'로 시작해야 한다. 실제: {formatted:?}"
    );
    assert!(
        formatted.contains("북한산") || formatted.contains("다음주"),
        "format_recall 결과에 SSOT 토큰(북한산 또는 다음주)이 포함되어야 한다. 실제: {formatted:?}"
    );
}
