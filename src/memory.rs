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
    use super::{MemoryEvent, PersonaId, format_recall_impl};
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
            scored.sort_by(|a, b| {
                b.0.cmp(&a.0).then_with(|| b.1.ts.cmp(&a.1.ts))
            });

            // 5. 상위 k개를 owned clone으로 반환
            scored.into_iter().take(k).map(|(_, ev)| ev.clone()).collect()
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
mod sqlite_impl {
    use super::{MemoryEvent, format_recall_impl};
    use rusqlite::{Connection, params};
    use std::path::Path;

    /// 공유 DDL: `new()`(:memory:)와 `open()`(파일) 양쪽에서 호출한다.
    ///
    /// `CREATE TABLE IF NOT EXISTS` + `CREATE VIRTUAL TABLE IF NOT EXISTS`를 사용해
    /// 기존 DB 재오픈 시에도 안전하게 멱등 실행된다.
    fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id      INTEGER PRIMARY KEY AUTOINCREMENT,
                room    TEXT NOT NULL,
                ts      INTEGER NOT NULL,
                speaker TEXT NOT NULL,
                content TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS participation (
                room    TEXT NOT NULL,
                persona TEXT NOT NULL,
                UNIQUE(room, persona)
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                tokens,
                room    UNINDEXED,
                mem_id  UNINDEXED,
                tokenize='unicode61'
            );",
        )
    }

    /// 메모리 스토어: `:memory:` SQLite + FTS5 BM25 구현.
    ///
    /// 스키마:
    ///   - `memories(id, room, ts, speaker, content)`
    ///   - `participation(room, persona)` UNIQUE
    ///   - `memories_fts(tokens, room UNINDEXED, mem_id UNINDEXED, tokenize='unicode61')`
    ///
    /// 결정성: `:memory:` + 고정 insert 순서 + `ORDER BY score ASC, ts DESC, id DESC`.
    /// rusqlite Connection은 Send, !Sync → LiveSession 단일 스레드에서만 소유.
    pub struct MemoryStore {
        conn: Connection,
    }

    impl MemoryStore {
        /// 빈 스토어를 생성한다. `:memory:` SQLite + DDL 실행.
        ///
        /// 고정 DDL + 런타임 입력 없음이므로 실패 시 expect 허용.
        pub fn new() -> Self {
            let conn = Connection::open_in_memory()
                .expect("in-memory sqlite must open");
            init_schema(&conn).expect("in-memory sqlite schema must init");
            Self { conn }
        }

        /// 파일 경로 SQLite를 열거나 생성한다(영속 스토어).
        ///
        /// - 부모 디렉터리를 재귀적으로 생성한다(`create_dir_all`).
        /// - `PRAGMA journal_mode=WAL`(크래시 내성, 단일 writer).
        /// - 스키마는 `init_schema`(IF NOT EXISTS → 기존 DB 재오픈 안전).
        ///
        /// 런타임 경로이므로 `Result`를 반환한다(호출처 `live_store`가 fallback).
        pub fn open(path: &Path) -> rusqlite::Result<Self> {
            // 부모 디렉터리가 없으면 생성.
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        rusqlite::Error::InvalidPath(
                            std::path::PathBuf::from(format!(
                                "create_dir_all failed for {:?}: {e}",
                                parent
                            ))
                        )
                    })?;
                }
            }
            let conn = Connection::open(path)?;
            // WAL 모드: 크래시 복구·동시 읽기 성능 향상. 단일 writer(라이브 틱 순차) 환경에 적합.
            conn.pragma_update(None, "journal_mode", "WAL")?;
            init_schema(&conn)?;
            Ok(Self { conn })
        }

        /// 기본 DB 경로를 반환한다(순수, 디스크 I/O 없음).
        ///
        /// 우선순위:
        ///   1. `$SALON_MEMORY_DB` 환경 변수 (비어 있지 않으면)
        ///   2. `$HOME/.local/share/tunaSalon/memory.db`
        ///   3. `HOME`도 없으면 `None`
        pub fn default_db_path() -> Option<std::path::PathBuf> {
            // 1. 환경 변수 override
            if let Ok(val) = std::env::var("SALON_MEMORY_DB") {
                if !val.is_empty() {
                    return Some(std::path::PathBuf::from(val));
                }
            }
            // 2. $HOME 기반 기본 경로
            if let Ok(home) = std::env::var("HOME") {
                if !home.is_empty() {
                    return Some(std::path::PathBuf::from(home)
                        .join(".local/share/tunaSalon/memory.db"));
                }
            }
            // 3. HOME 없음
            None
        }

        /// 라이브(`--chat`) 전용 스토어를 반환한다.
        ///
        /// - `default_db_path()`가 Some → `open(path)`. 실패 시 eprintln 경고 + `new()`.
        /// - `default_db_path()`가 None → `new()`(`:memory:`).
        ///
        /// **테스트에서 호출 금지**: 실제 `~/.local/share/tunaSalon/` 경로를 사용한다.
        pub fn live_store() -> Self {
            match Self::default_db_path() {
                Some(path) => {
                    match Self::open(&path) {
                        Ok(store) => store,
                        Err(e) => {
                            eprintln!(
                                "[tunaSalon] warning: 영속 메모리 DB를 열 수 없습니다 ({:?}: {e}). \
                                 이 세션은 :memory: 로 동작합니다(재시작 시 기억 없음).",
                                path
                            );
                            Self::new()
                        }
                    }
                }
                None => {
                    eprintln!(
                        "[tunaSalon] warning: $HOME 환경 변수가 없어 영속 메모리 DB 경로를 \
                         결정할 수 없습니다. 이 세션은 :memory: 로 동작합니다."
                    );
                    Self::new()
                }
            }
        }

        /// `persona`를 `room`의 참여자로 등록한다(멱등).
        pub fn join(&mut self, room: impl Into<String>, persona: impl Into<String>) {
            let room = room.into();
            let persona = persona.into();
            let _ = self.conn.execute(
                "INSERT OR IGNORE INTO participation(room, persona) VALUES (?1, ?2)",
                params![room, persona],
            );
        }

        /// 사건을 기록한다.
        ///
        /// 1. `memories` 테이블에 row 삽입.
        /// 2. `morphological_tokens`로 FTS 토큰 생성 → `memories_fts`에 삽입.
        /// 3. 화자 자동 참여 등록.
        pub fn record(&mut self, event: MemoryEvent) {
            // memories 삽입
            let result = self.conn.execute(
                "INSERT INTO memories(room, ts, speaker, content) VALUES (?1, ?2, ?3, ?4)",
                params![event.room, event.ts as i64, event.speaker, event.content],
            );
            if result.is_err() {
                return;
            }
            let mem_id = self.conn.last_insert_rowid();

            // FTS 토큰 생성 (friend-engine feature on이면 morph, 아니면 fallback)
            let tokens = crate::tokenize_ko::morphological_tokens(&event.content).join(" ");

            // memories_fts 삽입
            let _ = self.conn.execute(
                "INSERT INTO memories_fts(tokens, room, mem_id) VALUES (?1, ?2, ?3)",
                params![tokens, event.room, mem_id],
            );

            // 화자 자동 참여 등록
            let _ = self.conn.execute(
                "INSERT OR IGNORE INTO participation(room, persona) VALUES (?1, ?2)",
                params![event.room, event.speaker],
            );
        }

        /// `persona`의 과거 사건 중 `query`와 FTS5 BM25 점수가 높은 것을 최대 `k`개 반환한다(owned).
        ///
        /// 알고리즘:
        /// 1. participation 테이블에서 persona가 참여한 방 집합을 구한다.
        /// 2. query를 morphological_tokens로 토큰화.
        /// 3. 각 토큰을 큰따옴표로 감싸고(FTS5 키워드 오해 방지) `" OR "`로 연결(OR-MATCH).
        /// 4. FTS5 MATCH + memories.room IN(방 집합) + bm25() 정렬.
        /// 5. row → MemoryEvent owned.
        ///
        /// k=0 / 빈쿼리 / 미참여 → 빈 Vec. 런타임 오류는 빈 Vec으로 조용히 처리.
        pub fn recall(&self, persona: &str, query: &str, k: usize) -> Vec<MemoryEvent> {
            if k == 0 {
                return vec![];
            }

            // 1. persona가 참여한 방 목록
            let rooms: Vec<String> = {
                let mut stmt = match self.conn.prepare(
                    "SELECT room FROM participation WHERE persona = ?1",
                ) {
                    Ok(s) => s,
                    Err(_) => return vec![],
                };
                let rows = match stmt.query_map(params![persona], |row| row.get(0)) {
                    Ok(r) => r,
                    Err(_) => return vec![],
                };
                rows.filter_map(|r| r.ok()).collect()
            };

            if rooms.is_empty() {
                return vec![];
            }

            // 2. query 토큰화
            let tokens = crate::tokenize_ko::morphological_tokens(query);
            if tokens.is_empty() {
                return vec![];
            }

            // 3. OR-MATCH 구성: 각 토큰을 큰따옴표로 감싸고(내부 " → "") " OR "로 연결
            //    예) ["비", "오"] → `"비" OR "오"`
            let match_expr: String = tokens
                .iter()
                .map(|t| {
                    // 내부 " 를 "" 로 escape
                    let escaped = t.replace('"', "\"\"");
                    format!("\"{}\"", escaped)
                })
                .collect::<Vec<_>>()
                .join(" OR ");

            // 4. rooms placeholders: ?2, ?3, ...
            //    주의: params! 매크로가 런타임 가변 길이를 지원 안 하므로
            //    동적 SQL + params_from_iter 사용.
            let placeholders = rooms
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 3))
                .collect::<Vec<_>>()
                .join(", ");

            let sql = format!(
                "SELECT m.id, m.room, m.ts, m.speaker, m.content, bm25(memories_fts) AS score
                 FROM memories_fts
                 JOIN memories m ON m.id = memories_fts.mem_id
                 WHERE memories_fts.tokens MATCH ?1
                   AND m.room IN ({placeholders})
                 ORDER BY score ASC, m.ts DESC, m.id DESC
                 LIMIT ?2"
            );

            let mut stmt = match self.conn.prepare(&sql) {
                Ok(s) => s,
                Err(_) => return vec![],
            };

            // 파라미터: [match_expr, k, room1, room2, ...]
            use rusqlite::types::ToSql;
            let mut param_vals: Vec<Box<dyn ToSql>> = Vec::new();
            param_vals.push(Box::new(match_expr));
            param_vals.push(Box::new(k as i64));
            for r in &rooms {
                param_vals.push(Box::new(r.clone()));
            }
            let param_refs: Vec<&dyn ToSql> = param_vals.iter().map(|b| b.as_ref()).collect();

            let rows = match stmt.query_map(param_refs.as_slice(), |row| {
                let _id: i64 = row.get(0)?;
                let room: String = row.get(1)?;
                let ts: i64 = row.get(2)?;
                let speaker: String = row.get(3)?;
                let content: String = row.get(4)?;
                Ok(MemoryEvent {
                    room,
                    ts: ts as u64,
                    speaker,
                    content,
                })
            }) {
                Ok(r) => r,
                Err(_) => return vec![],
            };

            rows.filter_map(|r| r.ok()).collect()
        }

        /// 회상 결과를 회상 슬롯용 문자열로 포맷한다.
        pub fn format_recall(events: &[MemoryEvent]) -> Option<String> {
            format_recall_impl(events)
        }
    }

    impl Default for MemoryStore {
        fn default() -> Self {
            Self::new()
        }
    }

    impl std::fmt::Debug for MemoryStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MemoryStore(SQLite:memory:)").finish()
        }
    }
}

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
        assert!(result_y.is_empty(), "y는 A 사건을 볼 수 없어야 한다(참여 격리)");
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
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "비 온다 심심해");
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
        assert!(store2.recall("bob", "안녕", 5).is_empty(), "미참여 페르소나는 빈 결과");

        // 겹침 없는 쿼리
        let mut store3 = MemoryStore::new();
        store3.record(ev("A", 1, "alice", "안녕 세계"));
        assert!(store3.recall("alice", "전혀다른토큰xyz", 5).is_empty(), "겹침 없으면 빈 결과");

        // k=0
        let mut store4 = MemoryStore::new();
        store4.record(ev("A", 1, "alice", "안녕"));
        assert!(store4.recall("alice", "안녕", 0).is_empty(), "k=0이면 빈 결과");

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
            result.iter().any(|ev| ev.content.contains("비 온다 심심해")),
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

        let output = MemoryStore::format_recall(&events).expect("비어 있지 않으므로 Some이어야 한다");
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
        let db_path = tmp_dir.join(format!("tunasalon_test_roundtrip_{}.db", std::process::id()));

        // 시작 시 잔여 파일 정리(이전 실패 잔재 등).
        for suffix in &["", "-wal", "-shm"] {
            let p = tmp_dir.join(format!("tunasalon_test_roundtrip_{}{suffix}.db", std::process::id()));
            let _ = std::fs::remove_file(&p);
        }

        // 1단계: 파일 DB 열고 사건 기록 후 drop.
        {
            let mut store = MemoryStore::open(&db_path)
                .expect("임시 경로 open()이 성공해야 한다");
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
                result.iter().any(|e| e.content.contains("영속 테스트 안녕")),
                "재오픈 recall 결과에 '영속 테스트 안녕'이 없다: {:?}",
                result.iter().map(|e| &e.content).collect::<Vec<_>>()
            );
        }

        // 정리: 임시 DB 파일 + WAL/SHM 사이드카 삭제.
        for suffix in &["", "-wal", "-shm"] {
            let p = tmp_dir.join(format!("tunasalon_test_roundtrip_{}{suffix}.db", std::process::id()));
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
}
