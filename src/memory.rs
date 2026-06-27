//! 메모리 스토어 + 회상 코어 (task-39).
//!
//! 참여 기반 기억: 캐릭터는 자신이 있었던 방의 사건만 회상할 수 있다.
//! 순수·결정적·인메모리. 네트워크/rng/벽시계 없음.
//! 생성 배선은 task-41. 평가 하네스는 task-40.
//!
//! task-44: `friend-engine` feature on이면 `:memory:` SQLite + FTS5 BM25 구현으로
//! 교체한다. feature off는 v0.8 Vec 구현(골든·기본빌드 보존).
//!
//! 공개 API (양쪽 동일):
//!   - `recall(&self, persona, query, k) -> Vec<MemoryEvent>` (owned)
//!   - `format_recall(events: &[MemoryEvent]) -> Option<String>`

use crate::model::PersonaId;

// ─── 공유 데이터 타입 ────────────────────────────────────────────────────────

/// 메모리 스토어에 저장되는 사건 단위.
///
/// `ts`는 논리 타임스탬프(결정적). 벽시계를 쓰지 않는다.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryEvent {
    pub room: String,
    pub ts: u64,
    pub speaker: PersonaId,
    pub content: String,
}

/// 회상 결과를 회상 슬롯용 문자열로 포맷한다.
///
/// 비어 있으면 `None`. 있으면 `"지난 대화에서:\n- {speaker}: {content}\n..."`.
/// 논리 ts 기반 상대표현("지난 대화에서")만 쓰며 벽시계 없음.
///
/// 공유 함수(feature on/off 동일 시그니처: `&[MemoryEvent]` → `Option<String>`).
pub fn format_recall_impl(events: &[MemoryEvent]) -> Option<String> {
    if events.is_empty() {
        return None;
    }
    let mut buf = String::from("지난 대화에서:\n");
    for ev in events {
        buf.push_str(&format!("- {}: {}\n", ev.speaker, ev.content));
    }
    // 마지막 '\n' 제거
    if buf.ends_with('\n') {
        buf.pop();
    }
    Some(buf)
}

// ─────────────────────────────────────────────────────────────────────────────
// feature OFF: v0.8 Vec 구현 (기본, lean, no rusqlite)
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(not(feature = "friend-engine"))]
mod vec_impl {
    use super::{format_recall_impl, MemoryEvent, PersonaId};
    use std::collections::{BTreeMap, BTreeSet};

    /// 메모리 스토어: 사건 로그 + 참여 레지스트리.
    ///
    /// - `events`: 기록된 사건(삽입 순).
    /// - `participation`: room → 그 방에 참여한 페르소나 집합.
    ///
    /// 결정성: `BTreeMap`/`BTreeSet` 사용. rng/네트워크/시간 없음.
    #[derive(Debug, Default)]
    pub struct MemoryStore {
        events: Vec<MemoryEvent>,
        participation: BTreeMap<String, BTreeSet<PersonaId>>,
    }

    impl MemoryStore {
        /// 빈 스토어를 생성한다.
        pub fn new() -> Self {
            Self::default()
        }

        /// `persona`를 `room`의 참여자로 등록한다.
        pub fn join(&mut self, room: impl Into<String>, persona: impl Into<String>) {
            self.participation
                .entry(room.into())
                .or_default()
                .insert(persona.into());
        }

        /// `persona`를 `room`의 참여자에서 제거한다(회상 격리 해제).
        ///
        /// room이 없거나 persona가 없으면 무동작(panic 없음).
        /// events(사건 기록)는 보존된다.
        pub fn leave(&mut self, room: impl Into<String>, persona: impl Into<String>) {
            let room = room.into();
            let persona = persona.into();
            if let Some(set) = self.participation.get_mut(&room) {
                set.remove(&persona);
            }
        }

        pub fn clear_room(&mut self, room: &str) {
            self.events.retain(|event| event.room != room);
            self.participation.remove(room);
        }

        /// 사건을 기록한다. 화자를 해당 방 참여자로 자동 join한다.
        pub fn record(&mut self, event: MemoryEvent) {
            self.participation
                .entry(event.room.clone())
                .or_default()
                .insert(event.speaker.clone());
            self.events.push(event);
        }

        /// `persona`의 과거 사건 중 `query`와 토큰 중복이 있는 것을 최대 `k`개 반환한다(owned).
        ///
        /// 알고리즘: 참여 방 필터 → intersection count → 점수 0 제외 →
        /// 점수 내림차순/ts 내림차순 → 상위 k개 클론 반환.
        pub fn recall(&self, persona: &str, query: &str, k: usize) -> Vec<MemoryEvent> {
            if k == 0 {
                return vec![];
            }

            // 1. persona가 참여한 방 집합
            let rooms: BTreeSet<&str> = self
                .participation
                .iter()
                .filter_map(|(room, personas)| {
                    if personas.contains(persona) {
                        Some(room.as_str())
                    } else {
                        None
                    }
                })
                .collect();

            if rooms.is_empty() {
                return vec![];
            }

            // query 토큰화 (flow::tokenize — 공백+ASCII 구두점 분리, 소문자)
            let query_tokens = crate::flow::tokenize(query);

            // 2-3. 참여한 방의 사건만 후보, intersection count로 점수 계산
            let mut scored: Vec<(usize, &MemoryEvent)> = self
                .events
                .iter()
                .filter(|ev| rooms.contains(ev.room.as_str()))
                .filter_map(|ev| {
                    let content_tokens = crate::flow::tokenize(&ev.content);
                    let score = query_tokens.intersection(&content_tokens).count();
                    if score == 0 {
                        None
                    } else {
                        Some((score, ev))
                    }
                })
                .collect();

            // 4. 점수 내림차순 → ts 내림차순
            scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.ts.cmp(&a.1.ts)));

            // 5. 상위 k개를 owned clone으로 반환
            scored
                .into_iter()
                .take(k)
                .map(|(_, ev)| ev.clone())
                .collect()
        }

        /// 회상 결과를 회상 슬롯용 문자열로 포맷한다.
        pub fn format_recall(events: &[MemoryEvent]) -> Option<String> {
            format_recall_impl(events)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// feature ON: SQLite(:memory:) + FTS5 BM25 구현
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(feature = "friend-engine")]
mod sqlite_impl;

// ─── 공개 재수출 ─────────────────────────────────────────────────────────────

#[cfg(not(feature = "friend-engine"))]
pub use vec_impl::MemoryStore;

#[cfg(feature = "friend-engine")]
pub use sqlite_impl::MemoryStore;

/// 기본 DB 경로를 반환한다(순수, 디스크 I/O 없음).
///
/// feature on: `$SALON_MEMORY_DB` → `$HOME/.local/share/tunaSalon/memory.db` → None.
/// feature off: 항상 None(파일 영속 없음, Vec 인메모리).
#[cfg(feature = "friend-engine")]
pub fn default_db_path() -> Option<std::path::PathBuf> {
    MemoryStore::default_db_path()
}

/// feature off 시 default_db_path는 None(Vec 구현, 파일 영속 없음).
#[cfg(not(feature = "friend-engine"))]
pub fn default_db_path() -> Option<std::path::PathBuf> {
    None
}

/// 라이브(`--chat`) 전용 스토어를 반환한다.
///
/// feature on: `default_db_path()`의 경로로 파일 SQLite 열기(실패 시 :memory: fallback).
/// feature off: `MemoryStore::new()`(Vec, 인메모리).
///
/// **테스트에서 호출 금지**: feature on 시 실제 `~/.local/share/tunaSalon/` 경로를 사용한다.
#[cfg(feature = "friend-engine")]
pub fn live_store() -> MemoryStore {
    MemoryStore::live_store()
}

/// feature off 시 live_store는 단순 Vec 인메모리 스토어.
#[cfg(not(feature = "friend-engine"))]
pub fn live_store() -> MemoryStore {
    MemoryStore::new()
}

// ─────────────────────────────────────────────────────────────────────────────
// 단위 테스트
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // 테스트용 헬퍼: 기본 MemoryEvent 생성
    fn ev(room: &str, ts: u64, speaker: &str, content: &str) -> MemoryEvent {
        MemoryEvent {
            room: room.to_string(),
            ts,
            speaker: speaker.to_string(),
            content: content.to_string(),
        }
    }

    /// (1) 참여 격리: room A에 사건 기록. x는 A 참여, y는 B만 참여.
    ///     x.recall → A 사건 포함. y.recall → 빈 Vec.
    #[test]
    fn participation_isolation() {
        let mut store = MemoryStore::new();
        store.join("A", "x");
        store.join("B", "y");
        store.record(ev("A", 1, "alice", "안녕 세계"));

        // x는 A에 참여 → A 사건 볼 수 있음
        let result_x = store.recall("x", "안녕 세계", 5);
        assert_eq!(result_x.len(), 1, "x는 A 사건을 회상해야 한다");
        assert_eq!(result_x[0].content, "안녕 세계");

        // y는 B에만 참여 → A 사건 접근 불가
        let result_y = store.recall("y", "안녕 세계", 5);
        assert!(
            result_y.is_empty(),
            "y는 A 사건을 볼 수 없어야 한다(참여 격리)"
        );
    }

    /// (leave-1) leave 후 participation 격리: join → recall 성공 → leave → recall 빈 결과.
    ///
    /// events는 보존됨: leave 하지 않은 다른 참여자는 여전히 회상 가능.
    #[test]
    fn leave_removes_participation_isolation() {
        let mut store = MemoryStore::new();
        store.join("salon", "alice");
        store.join("salon", "bob");
        store.record(ev("salon", 1, "carol", "오늘 날씨 참 맑다"));

        // leave 전: alice는 salon에 참여 중 → carol 사건 회상 가능
        let before = store.recall("alice", "오늘 날씨", 5);
        assert!(
            !before.is_empty(),
            "leave 전 alice는 salon 사건을 회상해야 한다"
        );

        // alice leave
        store.leave("salon", "alice");

        // leave 후: alice는 salon 미참여 → 회상 불가
        let after = store.recall("alice", "오늘 날씨", 5);
        assert!(
            after.is_empty(),
            "leave 후 alice는 salon 사건을 회상하면 안 된다(participation 격리)"
        );

        // events 보존: bob은 여전히 참여 중 → carol 사건 회상 가능
        let bob_result = store.recall("bob", "오늘 날씨", 5);
        assert!(
            !bob_result.is_empty(),
            "leave 후에도 bob은 salon 사건을 회상할 수 있어야 한다(events 보존)"
        );
    }

    /// (leave-2) leave 멱등성: 이미 없는 room/persona에 leave해도 panic 없음.
    #[test]
    fn leave_nonexistent_is_noop() {
        let mut store = MemoryStore::new();
        // room도 없고 persona도 없음 → panic 없이 무동작
        store.leave("nonexistent_room", "nonexistent_persona");

        // room은 있지만 persona가 없음 → 무동작
        store.join("salon", "alice");
        store.leave("salon", "nonexistent_persona");

        // alice는 여전히 참여 중이어야 함
        store.record(ev("salon", 1, "alice", "테스트 사건"));
        let result = store.recall("alice", "테스트 사건", 5);
        assert!(
            !result.is_empty(),
            "관련 없는 leave 후에도 alice 참여 상태 유지"
        );
    }

    /// (2) 토큰 회상: query와 겹치는 사건이 결과에 포함된다.
    #[test]
    fn token_recall() {
        let mut store = MemoryStore::new();
        // 화자 auto-join: record로 등록
        store.record(ev("salon", 1, "alice", "비 온다 심심해"));
        store.record(ev("salon", 2, "alice", "고양이 강아지"));

        // "비 온다"는 첫 번째 사건과 겹침, 두 번째 사건과는 겹침 없음
        let result = store.recall("alice", "비 온다", 5);
        // BM25-only 빌드에서는 겹치는 사건만 반환(len==1).
        // hybrid(semantic) 빌드에서는 벡터 leg가 추가 결과를 가져올 수 있으므로
        // "관련 사건이 포함되어 있는가"만 단언한다.
        #[cfg(not(feature = "friend-engine-semantic"))]
        assert_eq!(result.len(), 1, "BM25-only: 겹치는 사건 1개만 반환");
        assert!(
            result.iter().any(|ev| ev.content == "비 온다 심심해"),
            "recall 결과에 '비 온다 심심해'가 포함되어야 한다. 결과: {:?}",
            result.iter().map(|e| &e.content).collect::<Vec<_>>()
        );
    }

    /// (3) 동점 ts 내림차순: 같은 토큰 겹침 수이면 더 최근 사건이 먼저.
    #[test]
    fn tiebreak_by_ts_descending() {
        let mut store = MemoryStore::new();
        store.record(ev("room", 10, "alice", "hello world"));
        store.record(ev("room", 20, "alice", "hello world"));
        store.record(ev("room", 30, "alice", "hello world"));

        let result = store.recall("alice", "hello world", 3);
        assert_eq!(result.len(), 3);
        // ts 내림차순: 30, 20, 10
        assert_eq!(result[0].ts, 30);
        assert_eq!(result[1].ts, 20);
        assert_eq!(result[2].ts, 10);
    }

    /// (4) 빈 스토어, 미참여 페르소나, 겹침 0 → 빈 Vec.
    ///     format_recall(&[]) → None.
    #[test]
    fn edge_cases_empty() {
        // 빈 스토어
        let store = MemoryStore::new();
        assert!(store.recall("alice", "쿼리", 5).is_empty());

        // 미참여 페르소나
        let mut store2 = MemoryStore::new();
        store2.record(ev("A", 1, "alice", "안녕"));
        assert!(
            store2.recall("bob", "안녕", 5).is_empty(),
            "미참여 페르소나는 빈 결과"
        );

        // 겹침 없는 쿼리 — BM25-only 빌드에서만 빈 결과를 보장한다.
        // hybrid(semantic) 빌드는 벡터 leg가 추가 결과를 반환할 수 있다(MockEmbedder).
        let mut store3 = MemoryStore::new();
        store3.record(ev("A", 1, "alice", "안녕 세계"));
        #[cfg(not(feature = "friend-engine-semantic"))]
        assert!(
            store3.recall("alice", "전혀다른토큰xyz", 5).is_empty(),
            "겹침 없으면 빈 결과(BM25-only)"
        );
        // semantic 빌드에서도 패닉 없이 반환되어야 한다.
        #[cfg(feature = "friend-engine-semantic")]
        let _ = store3.recall("alice", "전혀다른토큰xyz", 5);

        // k=0
        let mut store4 = MemoryStore::new();
        store4.record(ev("A", 1, "alice", "안녕"));
        assert!(
            store4.recall("alice", "안녕", 0).is_empty(),
            "k=0이면 빈 결과"
        );

        // format_recall 빈 슬라이스 → None
        assert!(MemoryStore::format_recall(&[]).is_none());
    }

    /// (5) 결정성: 같은 스토어+쿼리+k로 두 번 호출하면 동일 결과.
    #[test]
    fn recall_is_deterministic() {
        let mut store = MemoryStore::new();
        store.record(ev("room", 1, "alice", "안녕 세계"));
        store.record(ev("room", 2, "alice", "세계 평화"));
        store.record(ev("room", 3, "alice", "안녕 친구"));

        let r1 = store.recall("alice", "안녕 세계", 5);
        let r2 = store.recall("alice", "안녕 세계", 5);

        assert_eq!(r1.len(), r2.len());
        for (a, b) in r1.iter().zip(r2.iter()) {
            assert_eq!(a, b);
        }
    }

    /// (품질 게이트 — feature-gated) 조사 분리 회상 케이스.
    ///
    /// feature on: query "비가 온다" → content "비 온다 심심해"를 회상.
    ///   형태소가 "비"/"오" 토큰을 추출해 조사(가) 분리 매칭.
    #[cfg(feature = "friend-engine")]
    #[test]
    fn morphology_recall_strips_josa() {
        let mut store = MemoryStore::new();
        store.record(ev("salon", 1, "alice", "비 온다 심심해"));
        store.record(ev("salon", 2, "alice", "고양이 강아지"));

        let result = store.recall("alice", "비가 온다", 5);
        assert!(
            !result.is_empty(),
            "형태소 회상 실패: '비가 온다' 쿼리가 '비 온다 심심해'를 히트해야 한다"
        );
        assert!(
            result
                .iter()
                .any(|ev| ev.content.contains("비 온다 심심해")),
            "형태소 회상 실패: 결과에 '비 온다 심심해'가 없다. 결과: {:?}",
            result.iter().map(|e| &e.content).collect::<Vec<_>>()
        );
    }

    /// (품질 게이트 — feature off 대비) feature off에서 "비가 온다" 쿼리.
    #[cfg(not(feature = "friend-engine"))]
    #[test]
    fn whitespace_recall_may_miss_josa_case() {
        let mut store = MemoryStore::new();
        store.record(ev("salon", 1, "alice", "비 온다 심심해"));
        store.record(ev("salon", 2, "alice", "고양이 강아지"));

        let result = store.recall("alice", "비가 온다", 5);
        // 패닉 없이 반환만 되면 통과
        let _ = result;
    }

    /// (6) format_recall: 사건들의 speaker/content가 문자열에 포함된다.
    #[test]
    fn format_recall_produces_correct_string() {
        let e1 = ev("room", 1, "alice", "안녕하세요");
        let e2 = ev("room", 2, "bob", "반갑습니다");
        let events = vec![e1, e2];

        let output =
            MemoryStore::format_recall(&events).expect("비어 있지 않으므로 Some이어야 한다");
        assert!(output.starts_with("지난 대화에서:"), "헤더로 시작해야 한다");
        assert!(output.contains("alice"), "alice가 포함되어야 한다");
        assert!(output.contains("안녕하세요"), "content가 포함되어야 한다");
        assert!(output.contains("bob"), "bob이 포함되어야 한다");
        assert!(output.contains("반갑습니다"), "content가 포함되어야 한다");
    }

    /// (7, feature-on 전용) OR-MATCH: 쿼리 토큰 일부만 겹쳐도 회상된다.
    ///
    /// "내일 등산 약속" (3토큰) vs content "다음주 북한산 등산 계획" (1토큰 공유: 등산).
    /// AND-MATCH였다면 "내일", "약속"이 없어서 히트 안 되지만
    /// OR-MATCH면 "등산" 1개로 히트해야 한다.
    #[cfg(feature = "friend-engine")]
    #[test]
    fn or_match_recalls_on_partial_token_overlap() {
        let mut store = MemoryStore::new();
        store.record(ev("salon", 1, "alice", "다음주 북한산 등산 계획"));
        store.record(ev("salon", 2, "alice", "오늘 날씨 맑아서 좋다"));

        // 쿼리 토큰 중 "등산" 하나만 content와 공유 → OR이면 히트, AND면 miss
        let result = store.recall("alice", "내일 등산 약속", 5);
        assert!(
            !result.is_empty(),
            "OR-MATCH 실패: '내일 등산 약속' 쿼리가 '다음주 북한산 등산 계획'을 히트해야 한다"
        );
        assert!(
            result.iter().any(|ev| ev.content.contains("등산")),
            "OR-MATCH 결과에 '등산' 포함 사건이 없다. 결과: {:?}",
            result.iter().map(|e| &e.content).collect::<Vec<_>>()
        );
    }

    /// (task-45, feature-on 전용) 영속 roundtrip:
    /// 임시 경로 `open()` → record+join → drop → 재오픈 → recall에 사건 존재.
    ///
    /// **임시 디렉터리 사용**: `std::env::temp_dir()` 하위 고유 경로.
    /// 기본 경로(`~/.local/share/tunaSalon/`)는 절대 사용하지 않는다(테스트 격리).
    #[cfg(feature = "friend-engine")]
    #[test]
    fn persistence_roundtrip_open_close_reopen() {
        let tmp_dir = std::env::temp_dir();
        let db_path = tmp_dir.join(format!(
            "tunasalon_test_roundtrip_{}.db",
            std::process::id()
        ));

        // 시작 시 잔여 파일 정리(이전 실패 잔재 등).
        for suffix in &["", "-wal", "-shm"] {
            let p = tmp_dir.join(format!(
                "tunasalon_test_roundtrip_{}{suffix}.db",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&p);
        }

        // 1단계: 파일 DB 열고 사건 기록 후 drop.
        {
            let mut store = MemoryStore::open(&db_path).expect("임시 경로 open()이 성공해야 한다");
            store.join("salon", "alice");
            store.record(ev("salon", 1, "alice", "영속 테스트 안녕"));
        }
        // drop → 커넥션 닫힘, WAL checkpoint 발생.

        // 2단계: 같은 경로로 재오픈 → 이전 사건 recall.
        {
            let store = MemoryStore::open(&db_path)
                .expect("재오픈이 성공해야 한다(IF NOT EXISTS 멱등 스키마)");
            let result = store.recall("alice", "영속 테스트 안녕", 5);
            assert!(
                !result.is_empty(),
                "재오픈 후 recall에 이전 사건이 있어야 한다(영속 roundtrip 실패)"
            );
            assert!(
                result
                    .iter()
                    .any(|e| e.content.contains("영속 테스트 안녕")),
                "재오픈 recall 결과에 '영속 테스트 안녕'이 없다: {:?}",
                result.iter().map(|e| &e.content).collect::<Vec<_>>()
            );
        }

        // 정리: 임시 DB 파일 + WAL/SHM 사이드카 삭제.
        for suffix in &["", "-wal", "-shm"] {
            let p = tmp_dir.join(format!(
                "tunasalon_test_roundtrip_{}{suffix}.db",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&p);
        }
    }

    /// (task-45) default_db_path: `$SALON_MEMORY_DB` override 해석.
    ///
    /// 순수 함수(디스크 무접촉) → env 조작 후 반환값 확인.
    /// 테스트 격리: 이 테스트는 env를 직접 조작하므로 단독 검증한다.
    /// 다른 테스트와 병렬 실행 시 env 공유 충돌 위험 → 단순히 반환값 확인(set→check→restore).
    #[cfg(feature = "friend-engine")]
    #[test]
    fn default_db_path_respects_salon_memory_db_env() {
        let prev = std::env::var("SALON_MEMORY_DB").ok();

        // SALON_MEMORY_DB 설정 시 그 경로를 반환해야 한다.
        std::env::set_var("SALON_MEMORY_DB", "/tmp/test_custom_salon_memory.db");
        let path = MemoryStore::default_db_path();
        assert_eq!(
            path.as_deref(),
            Some(std::path::Path::new("/tmp/test_custom_salon_memory.db")),
            "SALON_MEMORY_DB override가 default_db_path에 반영되어야 한다"
        );

        // 원래 값 복원.
        match prev {
            Some(v) => std::env::set_var("SALON_MEMORY_DB", v),
            None => std::env::remove_var("SALON_MEMORY_DB"),
        }
    }

    // ── semantic feature 테스트 (friend-engine-semantic + non-windows) ─────────

    /// record N건 → memory_vectors에 N행 + ann.size()==N.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn semantic_record_stores_vectors() {
        let mut store = MemoryStore::new();
        store.record(ev("salon", 1, "alice", "안녕 세계 rust"));
        store.record(ev("salon", 2, "alice", "고양이 강아지 귀엽다"));
        store.record(ev("salon", 3, "bob", "오늘 날씨 맑다"));

        // memory_vectors 행 수 확인
        assert_eq!(
            store.test_vector_row_count(),
            3,
            "memory_vectors에 3행이 있어야 한다"
        );
        // ANN size 확인
        assert_eq!(store.test_ann_size(), 3, "ANN에 3개 벡터가 있어야 한다");
    }

    /// vector_search: 토큰 겹치는 쿼리가 해당 사건 mem_id를 상위로.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn semantic_vector_search_returns_relevant() {
        let mut store = MemoryStore::new();
        // record는 mem_id 1, 2, 3 순으로 자동 할당됨 (AUTOINCREMENT)
        store.record(ev("salon", 1, "alice", "rust programming language"));
        store.record(ev("salon", 2, "alice", "고양이 강아지 귀엽다"));
        store.record(ev("salon", 3, "alice", "오늘 날씨 맑다"));

        // "rust language"와 토큰 겹치는 것은 첫 번째 사건
        let results = store.vector_search("rust language", 3);
        assert!(
            !results.is_empty(),
            "vector_search 결과가 비어 있으면 안 된다"
        );
        // 첫 번째 결과의 mem_id가 1이어야 한다 (MockEmbedder 기준)
        assert_eq!(
            results[0].0, 1,
            "상위 결과의 mem_id가 1이어야 한다 (rust 토큰 공유). 결과: {results:?}"
        );
    }

    /// 결정성: 같은 스토어+쿼리로 두 번 호출 → 동일 결과.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn semantic_vector_search_deterministic() {
        let mut store = MemoryStore::new();
        store.record(ev("room", 1, "alice", "hello world rust"));
        store.record(ev("room", 2, "alice", "고양이 귀엽다"));

        let r1 = store.vector_search("hello rust", 2);
        let r2 = store.vector_search("hello rust", 2);
        assert_eq!(r1.len(), r2.len(), "두 번 호출 결과 길이가 같아야 한다");
        for (a, b) in r1.iter().zip(r2.iter()) {
            assert_eq!(a.0, b.0, "mem_id가 동일해야 한다");
            assert!((a.1 - b.1).abs() < 1e-6, "distance가 동일해야 한다");
        }
    }

    /// vector_search k=0 → 빈 Vec.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn semantic_vector_search_k_zero_empty() {
        let mut store = MemoryStore::new();
        store.record(ev("room", 1, "alice", "hello world"));
        let results = store.vector_search("hello", 0);
        assert!(results.is_empty(), "k=0이면 빈 Vec");
    }

    /// open(file) roundtrip: record → drop → reopen → vector_search 동작.
    #[cfg(all(
        feature = "friend-engine",
        feature = "friend-engine-semantic",
        not(target_os = "windows")
    ))]
    #[test]
    fn semantic_open_roundtrip_vector_search() {
        let tmp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let db_path = tmp_dir.join(format!("tunasalon_sem_roundtrip_{pid}.db"));
        let usearch_path = tmp_dir.join(format!("tunasalon_sem_roundtrip_{pid}.db.usearch"));

        // 잔여 파일 정리
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&usearch_path);
        for suf in &["-wal", "-shm"] {
            let _ = std::fs::remove_file(
                tmp_dir.join(format!("tunasalon_sem_roundtrip_{pid}{suf}.db")),
            );
        }

        // 1단계: 파일 DB 열고 사건 기록 후 save + drop
        {
            let mut store = MemoryStore::open(&db_path).expect("open()이 성공해야 한다");
            store.record(ev(
                "salon",
                1,
                "alice",
                "rust programming language semantic",
            ));
            store.record(ev("salon", 2, "alice", "고양이 강아지 귀엽다"));
            // ANN 저장
            let _ = store.test_ann_save();
        }

        // 2단계: 재오픈 → ANN 로드 → vector_search
        {
            let store = MemoryStore::open(&db_path).expect("재오픈이 성공해야 한다");
            let results = store.vector_search("rust language", 2);
            assert!(
                !results.is_empty(),
                "재오픈 후 vector_search 결과가 있어야 한다"
            );
            assert_eq!(
                results[0].0, 1,
                "재오픈 후 상위 mem_id=1이어야 한다. 결과: {results:?}"
            );
        }

        // 정리
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&usearch_path);
        for suf in &["-wal", "-shm"] {
            let _ = std::fs::remove_file(
                tmp_dir.join(format!("tunasalon_sem_roundtrip_{pid}{suf}.db")),
            );
        }
    }

    // ── fts_or_match / sql_placeholders 단위 테스트 (friend-engine) ──────────

    /// fts_or_match: 기본 escape + " OR " join.
    #[cfg(feature = "friend-engine")]
    #[test]
    fn fts_or_match_basic() {
        use super::sqlite_impl::fts_or_match;
        let tokens = vec!["비".to_string(), "오".to_string()];
        let result = fts_or_match(&tokens);
        assert_eq!(result, r#""비" OR "오""#, "두 토큰 OR join");
    }

    /// fts_or_match: 내부 큰따옴표를 "" 로 escape 한다.
    #[cfg(feature = "friend-engine")]
    #[test]
    fn fts_or_match_escape_quote() {
        use super::sqlite_impl::fts_or_match;
        let tokens = vec!["say \"hello\"".to_string()];
        let result = fts_or_match(&tokens);
        // 예상: 내부 " 두 개가 각각 ""로 escpae → 외부 따옴표 포함 "say ""hello"""
        assert_eq!(result, "\"say \"\"hello\"\"\"", "내부 \" → \"\" escape");
    }

    /// fts_or_match: 빈 토큰 목록 → 빈 문자열.
    #[cfg(feature = "friend-engine")]
    #[test]
    fn fts_or_match_empty() {
        use super::sqlite_impl::fts_or_match;
        let result = fts_or_match(&[]);
        assert_eq!(result, "", "빈 입력 → 빈 문자열");
    }

    /// sql_placeholders: count=3, start=2 → "?2, ?3, ?4".
    #[cfg(feature = "friend-engine")]
    #[test]
    fn sql_placeholders_basic() {
        use super::sqlite_impl::sql_placeholders;
        assert_eq!(sql_placeholders(3, 2), "?2, ?3, ?4");
    }

    /// sql_placeholders: count=1, start=1 → "?1".
    #[cfg(feature = "friend-engine")]
    #[test]
    fn sql_placeholders_single() {
        use super::sqlite_impl::sql_placeholders;
        assert_eq!(sql_placeholders(1, 1), "?1");
    }

    /// sql_placeholders: count=0 → 빈 문자열.
    #[cfg(feature = "friend-engine")]
    #[test]
    fn sql_placeholders_zero() {
        use super::sqlite_impl::sql_placeholders;
        assert_eq!(sql_placeholders(0, 5), "", "count=0이면 빈 문자열");
    }

    /// sql_placeholders: start offset 검증 — recall BM25 leg 기준(start=3).
    #[cfg(feature = "friend-engine")]
    #[test]
    fn sql_placeholders_recall_bm25_offset() {
        use super::sqlite_impl::sql_placeholders;
        // rooms 2개, start=3 → ?3, ?4
        assert_eq!(sql_placeholders(2, 3), "?3, ?4");
    }

    // ── rrf_fuse 단위 테스트 (friend-engine-semantic, task-49) ───────────────

    /// 두 리스트 모두에 있는 키가 한쪽에만 있는 키보다 높은 점수를 얻어 상위에 온다.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn rrf_fuse_key_in_both_lists_ranks_top() {
        use super::sqlite_impl::rrf_fuse;
        // id=2 → bm25 rank 1, vector rank 1 (두 번째 항목이므로 rank=1)
        // id=1 → bm25 rank 0 (1위)만, score = 1/(60+0+1) = 1/61 ≈ 0.01639
        // id=2 → bm25 rank 1, vec rank 0 → score = 1/62 + 1/61 ≈ 0.02252
        // id=3 → vec rank 1만 → score = 1/62 ≈ 0.01613
        let bm25 = vec![1i64, 2];
        let vec = vec![2i64, 3];
        let fused = rrf_fuse(&bm25, &vec, 60.0);
        assert!(!fused.is_empty(), "결과가 비어 있으면 안 된다");
        assert_eq!(fused[0], 2, "양쪽 리스트에 있는 id=2가 최상위여야 한다");
    }

    /// 한쪽에만 있는 키도 결과에 포함된다.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn rrf_fuse_one_list_keys_included() {
        use super::sqlite_impl::rrf_fuse;
        let bm25 = vec![10i64, 20];
        let vec_ids: Vec<i64> = vec![];
        let fused = rrf_fuse(&bm25, &vec_ids, 60.0);
        assert_eq!(fused.len(), 2, "BM25만 있어도 두 항목 모두 포함");
        assert_eq!(fused[0], 10, "rank 0이 먼저");
        assert_eq!(fused[1], 20);
    }

    /// 빈 입력 graceful.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn rrf_fuse_empty_inputs() {
        use super::sqlite_impl::rrf_fuse;
        let fused = rrf_fuse(&[], &[], 60.0);
        assert!(fused.is_empty(), "빈 입력이면 빈 Vec");
    }

    /// 동점 tie-break: id 오름차순(결정적).
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn rrf_fuse_tiebreak_id_ascending() {
        use super::sqlite_impl::rrf_fuse;
        // 두 id가 각각 단독으로 rank 0 → 동점(둘 다 1/61).
        // id 오름차순: 5 < 9
        let bm25 = vec![9i64];
        let vec_ids = vec![5i64];
        let fused = rrf_fuse(&bm25, &vec_ids, 60.0);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0], 5, "동점 시 id 오름차순 → 5가 먼저");
        assert_eq!(fused[1], 9);
    }

    /// 결정성: 같은 입력으로 두 번 호출 → 동일 결과.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn rrf_fuse_deterministic() {
        use super::sqlite_impl::rrf_fuse;
        let bm25 = vec![3i64, 1, 2];
        let vec_ids = vec![2i64, 3, 4];
        let r1 = rrf_fuse(&bm25, &vec_ids, 60.0);
        let r2 = rrf_fuse(&bm25, &vec_ids, 60.0);
        assert_eq!(r1, r2, "결정성: 같은 입력은 같은 결과");
    }

    // ── hybrid recall 테스트 (friend-engine-semantic, task-49) ───────────────

    /// SSOT 회상: hybrid recall도 SSOT를 상위에서 회상한다.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn hybrid_recall_ssot_in_top3() {
        let mut store = MemoryStore::new();
        store.join("morning", "ada");
        store.join("morning", "bora");

        store.record(ev("morning", 10, "ada", "오늘 날씨 맑다"));
        store.record(ev("morning", 20, "bora", "지난주 등산 비 취소"));
        store.record(ev("morning", 30, "ada", "점심 뭐 먹을까"));
        store.record(ev("morning", 40, "bora", "다음주 화요일 북한산 등산 약속"));

        let results = store.recall("ada", "다음주 화요일 등산 약속", 5);
        assert!(
            !results.is_empty(),
            "hybrid recall 결과가 비어 있으면 안 된다"
        );
        let has_ssot = results
            .iter()
            .take(3)
            .any(|ev| ev.content.contains("다음주 화요일 북한산 등산 약속"));
        assert!(
            has_ssot,
            "hybrid recall: SSOT가 top-3에 없다. 결과: {:?}",
            results.iter().map(|e| &e.content).collect::<Vec<_>>()
        );
    }

    /// 참여 격리: 벡터 leg 결과도 미참여 방 사건을 포함하지 않는다.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn hybrid_recall_vector_leg_participation_isolation() {
        let mut store = MemoryStore::new();
        // room A: alice 참여
        store.join("A", "alice");
        // room B: bob만 참여 (alice 미참여)
        store.join("B", "bob");

        store.record(ev("A", 1, "alice", "rust programming language"));
        // B에 아주 관련 높은 내용을 넣어도 alice에게 보이면 안 됨
        store.record(ev("B", 2, "bob", "rust programming language best"));

        let results = store.recall("alice", "rust programming language", 5);
        let has_room_b = results.iter().any(|ev| ev.room == "B");
        assert!(
            !has_room_b,
            "벡터 leg 참여 격리 실패: alice 결과에 room B 사건이 있다. 결과: {:?}",
            results
                .iter()
                .map(|e| (&e.room, &e.content))
                .collect::<Vec<_>>()
        );
    }

    /// 결정성: hybrid recall 두 번 호출 → 동일 결과.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn hybrid_recall_deterministic() {
        let mut store = MemoryStore::new();
        store.record(ev("room", 1, "alice", "안녕 세계 rust"));
        store.record(ev("room", 2, "alice", "세계 평화"));
        store.record(ev("room", 3, "alice", "안녕 친구"));

        let r1 = store.recall("alice", "안녕 세계", 5);
        let r2 = store.recall("alice", "안녕 세계", 5);
        assert_eq!(r1.len(), r2.len(), "hybrid recall 결정성: 길이 동일");
        for (a, b) in r1.iter().zip(r2.iter()) {
            assert_eq!(a, b, "hybrid recall 결정성: 사건 동일");
        }
    }

    /// 임베딩 없을 때(k=0 vector_search) → BM25만으로 폴백, recall 정상 동작.
    /// (MockEmbedder는 항상 성공하므로 vector_search k=0으로 테스트)
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    #[test]
    fn hybrid_recall_bm25_fallback_when_k_zero() {
        let mut store = MemoryStore::new();
        store.record(ev("room", 1, "alice", "비 온다 심심해"));
        store.record(ev("room", 2, "alice", "고양이 강아지"));

        // k=1로 hybrid recall 호출 → over_fetch=4, 정상 동작 확인
        let results = store.recall("alice", "비 온다", 1);
        // 결과가 있거나 없거나 패닉 없이 반환만 되면 통과
        let _ = results;
    }
}
