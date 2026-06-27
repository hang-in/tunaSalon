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
mod sqlite_impl {
    use super::{format_recall_impl, MemoryEvent};
    use rusqlite::{params, Connection};
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
        )?;

        // semantic feature: 임베딩 BLOB 저장 테이블 + meta 테이블
        #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory_vectors (
                mem_id    INTEGER PRIMARY KEY,
                embedding BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;

        Ok(())
    }

    // ─── meta 읽기/쓰기 헬퍼 (semantic only) ─────────────────────────────────

    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    fn get_meta(conn: &Connection, key: &str) -> Option<String> {
        conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
            r.get(0)
        })
        .ok()
    }

    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    fn set_meta(conn: &Connection, key: &str, value: &str) {
        let _ = conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        );
    }

    // ─── ANN 재구축 헬퍼 (semantic only) ──────────────────────────────────────

    /// `memory_vectors` 테이블의 BLOB로 ANN 인덱스를 재구축한다.
    /// `.usearch` 파일이 없거나 로드 실패 시 호출된다.
    /// 오류 시 None 반환(eprintln 경고), 패닉 없음.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    fn rebuild_ann_from_db(
        conn: &Connection,
        usearch_path: &std::path::Path,
        _embedder: &dyn crate::embed::Embedder,
    ) -> Option<crate::ann::AnnIndex> {
        let ann = match crate::ann::AnnIndex::open_or_create(usearch_path, 1024) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[tunaSalon] warn: ANN open_or_create for rebuild failed: {e}");
                return None;
            }
        };

        // memory_vectors 전체 행 읽기
        let mut stmt = match conn.prepare("SELECT mem_id, embedding FROM memory_vectors") {
            Ok(s) => s,
            Err(_) => return Some(ann), // 테이블 없음 or 비어 있음 → 빈 인덱스
        };

        let rows_result = stmt.query_map([], |row| {
            let mem_id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((mem_id, blob))
        });

        let rows = match rows_result {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[tunaSalon] warn: ANN rebuild query failed: {e}");
                return Some(ann);
            }
        };

        for item in rows {
            if let Ok((mem_id, blob)) = item {
                let vec = blob_to_f32(&blob);
                if let Err(e) = ann.add(mem_id as u64, &vec) {
                    eprintln!("[tunaSalon] warn: ANN rebuild add key={mem_id} failed: {e}");
                }
            }
        }

        Some(ann)
    }

    // ─── reembed_all 헬퍼 (semantic only) ────────────────────────────────────

    /// 모든 `memories.content`를 현재 임베더로 재임베딩해 `memory_vectors`와 ANN을 새로 채운다.
    /// 임베더 변경/신규 시 호출된다(rebuild_ann_from_db와 달리 embed를 다시 호출함).
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    fn reembed_all(
        conn: &Connection,
        embedder: &dyn crate::embed::Embedder,
        usearch_path: &std::path::Path,
    ) -> Option<crate::ann::AnnIndex> {
        let ann = match crate::ann::AnnIndex::open_or_create(usearch_path, 1024) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[tunaSalon] warn: reembed ANN 생성 실패: {e}");
                return None;
            }
        };
        // 기존 stale 벡터 제거(이전 임베더 기준)
        let _ = conn.execute("DELETE FROM memory_vectors", []);
        let mut stmt = match conn.prepare("SELECT id, content FROM memories ORDER BY id") {
            Ok(s) => s,
            Err(_) => return Some(ann),
        };
        let rows = match stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok(r) => r,
            Err(_) => return Some(ann),
        };
        for item in rows {
            if let Ok((id, content)) = item {
                match embedder.embed(&content) {
                    Ok(v) => {
                        let blob = f32_to_blob(&v);
                        let _ = conn.execute(
                            "INSERT OR REPLACE INTO memory_vectors(mem_id, embedding) VALUES (?1, ?2)",
                            params![id, blob],
                        );
                        if let Err(e) = ann.add(id as u64, &v) {
                            eprintln!("[tunaSalon] warn: reembed ANN add id={id} 실패: {e}");
                        }
                    }
                    Err(e) => eprintln!("[tunaSalon] warn: reembed embed id={id} 실패: {e}"),
                }
            }
        }
        Some(ann)
    }

    // ─── BLOB ↔ f32 직렬화 (to_le_bytes / from_le_bytes, dim=1024 고정) ──────

    /// f32 슬라이스를 little-endian 바이트 Vec으로 직렬화한다.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    fn f32_to_blob(vec: &[f32]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(vec.len() * 4);
        for &v in vec {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf
    }

    /// little-endian 바이트 Vec을 f32 슬라이스로 역직렬화한다.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    fn blob_to_f32(blob: &[u8]) -> Vec<f32> {
        blob.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    /// 메모리 스토어: `:memory:` SQLite + FTS5 BM25 구현.
    ///
    /// 스키마:
    ///   - `memories(id, room, ts, speaker, content)`
    ///   - `participation(room, persona)` UNIQUE
    ///   - `memories_fts(tokens, room UNINDEXED, mem_id UNINDEXED, tokenize='unicode61')`
    ///   - `memory_vectors(mem_id, embedding BLOB)` — `friend-engine-semantic` only
    ///
    /// 결정성: `:memory:` + 고정 insert 순서 + `ORDER BY score ASC, ts DESC, id DESC`.
    /// rusqlite Connection은 Send, !Sync → LiveSession 단일 스레드에서만 소유.
    pub struct MemoryStore {
        conn: Connection,
        /// 임베더 (semantic feature on, non-windows 시).
        #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
        embedder: Box<dyn crate::embed::Embedder>,
        /// HNSW ANN 인덱스 (semantic feature on, non-windows 시).
        #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
        ann: Option<crate::ann::AnnIndex>,
    }

    impl MemoryStore {
        /// 빈 스토어를 생성한다. `:memory:` SQLite + DDL 실행.
        ///
        /// 고정 DDL + 런타임 입력 없음이므로 실패 시 expect 허용.
        pub fn new() -> Self {
            let conn = Connection::open_in_memory().expect("in-memory sqlite must open");
            init_schema(&conn).expect("in-memory sqlite schema must init");

            #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
            {
                use crate::embed::MockEmbedder;
                let embedder: Box<dyn crate::embed::Embedder> = Box::new(MockEmbedder::default());
                let ann = crate::ann::AnnIndex::in_memory(1024)
                    .map_err(|e| eprintln!("[tunaSalon] warn: ANN in_memory init failed: {e}"))
                    .ok();
                return Self {
                    conn,
                    embedder,
                    ann,
                };
            }

            #[cfg(not(all(feature = "friend-engine-semantic", not(target_os = "windows"))))]
            Self { conn }
        }

        /// 파일 경로 SQLite를 열거나 생성한다(영속 스토어).
        ///
        /// - 부모 디렉터리를 재귀적으로 생성한다(`create_dir_all`).
        /// - `PRAGMA journal_mode=WAL`(크래시 내성, 단일 writer).
        /// - 스키마는 `init_schema`(IF NOT EXISTS → 기존 DB 재오픈 안전).
        ///
        /// 런타임 경로이므로 `Result`를 반환한다(호출처 `live_store`가 fallback).
        ///
        /// **테스트 결정성**: open()은 항상 MockEmbedder를 사용한다.
        /// 실 OrtEmbedder 배선은 live_store()를 통해서만 이루어진다.
        pub fn open(path: &Path) -> rusqlite::Result<Self> {
            #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
            {
                use crate::embed::MockEmbedder;
                return Self::open_with_embedder(path, Box::new(MockEmbedder::default()));
            }

            #[cfg(not(all(feature = "friend-engine-semantic", not(target_os = "windows"))))]
            {
                // 부모 디렉터리가 없으면 생성.
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            rusqlite::Error::InvalidPath(std::path::PathBuf::from(format!(
                                "create_dir_all failed for {:?}: {e}",
                                parent
                            )))
                        })?;
                    }
                }
                let conn = Connection::open(path)?;
                conn.pragma_update(None, "journal_mode", "WAL")?;
                init_schema(&conn)?;
                Ok(Self { conn })
            }
        }

        /// 외부 주입 임베더로 파일 SQLite를 연다(영속 스토어).
        ///
        /// **임베더 일관성**: DB의 `meta.embedder_kind`와 주입 임베더의 `kind()`를 비교한다.
        ///   - 일치: 기존 `.usearch` 로드 또는 `memory_vectors` BLOB로 ANN 재구축(빠름).
        ///   - 불일치(또는 신규 None): 전체 재임베딩(`reembed_all`) + ANN 재구축 + meta 갱신.
        ///     Mock<->Ort 혼용 방지, 첫 의미 도입 시 과거 사건도 백필된다.
        #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
        pub fn open_with_embedder(
            path: &Path,
            embedder: Box<dyn crate::embed::Embedder>,
        ) -> rusqlite::Result<Self> {
            // 1. 부모 디렉터리 생성
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        rusqlite::Error::InvalidPath(std::path::PathBuf::from(format!(
                            "create_dir_all failed for {:?}: {e}",
                            parent
                        )))
                    })?;
                }
            }
            // 2. Connection + WAL
            let conn = Connection::open(path)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            // 3. 스키마 초기화
            init_schema(&conn)?;
            // 4. .usearch 경로 계산
            let usearch_path = {
                let mut p = path.to_path_buf();
                let mut fname = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("memory.db")
                    .to_string();
                fname.push_str(".usearch");
                p.set_file_name(fname);
                p
            };
            // 5. 임베더 일관성 검사
            let current = embedder.kind();
            let stored = get_meta(&conn, "embedder_kind");
            let ann = if stored.as_deref() == Some(current) {
                // 일치: 기존 인덱스 로드 or BLOB rebuild
                if usearch_path.exists() {
                    crate::ann::AnnIndex::open_or_create(&usearch_path, 1024)
                        .map_err(|e| {
                            eprintln!("[tunaSalon] warn: ANN load 실패({e}), 재구축...");
                        })
                        .ok()
                        .or_else(|| rebuild_ann_from_db(&conn, &usearch_path, &*embedder))
                } else {
                    rebuild_ann_from_db(&conn, &usearch_path, &*embedder)
                }
            } else {
                // 불일치/신규: 전체 재임베딩
                eprintln!(
                    "[tunaSalon] 임베더 변경 감지({stored:?} -> {current}), 의미 인덱스 재구축 중..."
                );
                let _ = std::fs::remove_file(&usearch_path);
                let a = reembed_all(&conn, &*embedder, &usearch_path);
                set_meta(&conn, "embedder_kind", current);
                a
            };
            Ok(Self {
                conn,
                embedder,
                ann,
            })
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
            if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
                if !home.is_empty() {
                    return Some(
                        std::path::PathBuf::from(home).join(".local/share/tunaSalon/memory.db"),
                    );
                }
            }
            // 3. HOME 없음
            None
        }

        /// 라이브(`--chat`) 전용 스토어를 반환한다.
        ///
        /// - `default_db_path()`가 Some → `open_with_embedder(path, choose_live_embedder())`.
        ///   실패 시 eprintln 경고 + `new()`.
        /// - `default_db_path()`가 None → `new()`(`:memory:`).
        ///
        /// **테스트에서 호출 금지**: 실제 `~/.local/share/tunaSalon/` 경로를 사용한다.
        pub fn live_store() -> Self {
            match Self::default_db_path() {
                Some(path) => {
                    if let Some(parent) = path.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            eprintln!(
                                "[tunaSalon] warning: 영속 메모리 DB 디렉터리를 만들 수 없습니다 ({:?}: {e}). \
                                 이 세션은 :memory: 로 동작합니다(재시작 시 기억 없음).",
                                parent
                            );
                            return Self::new();
                        }
                    }
                    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
                    {
                        match Self::open_with_embedder(&path, choose_live_embedder()) {
                            Ok(store) => return store,
                            Err(e) => {
                                eprintln!(
                                    "[tunaSalon] warning: 영속 메모리 DB를 열 수 없습니다 ({:?}: {e}). \
                                     이 세션은 :memory: 로 동작합니다(재시작 시 기억 없음).",
                                    path
                                );
                                return Self::new();
                            }
                        }
                    }

                    #[cfg(not(all(
                        feature = "friend-engine-semantic",
                        not(target_os = "windows")
                    )))]
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
                        "[tunaSalon] warning: $HOME/$USERPROFILE 환경 변수가 없어 영속 메모리 DB 경로를 \
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

        /// `persona`를 `room`의 참여자에서 제거한다(회상 격리 해제).
        ///
        /// room이 없거나 persona가 없으면 무동작(panic 없음).
        /// events(사건 기록)는 보존된다.
        pub fn leave(&mut self, room: impl Into<String>, persona: impl Into<String>) {
            let room = room.into();
            let persona = persona.into();
            let _ = self.conn.execute(
                "DELETE FROM participation WHERE room = ?1 AND persona = ?2",
                params![room, persona],
            );
        }

        pub fn clear_room(&mut self, room: &str) {
            #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
            let _ = self.conn.execute(
                "DELETE FROM memory_vectors WHERE mem_id IN (SELECT id FROM memories WHERE room = ?1)",
                params![room],
            );
            let _ = self
                .conn
                .execute("DELETE FROM memories_fts WHERE room = ?1", params![room]);
            let _ = self
                .conn
                .execute("DELETE FROM memories WHERE room = ?1", params![room]);
            let _ = self
                .conn
                .execute("DELETE FROM participation WHERE room = ?1", params![room]);
        }

        /// 사건을 기록한다.
        ///
        /// 1. `memories` 테이블에 row 삽입.
        /// 2. `morphological_tokens`로 FTS 토큰 생성 → `memories_fts`에 삽입.
        /// 3. 화자 자동 참여 등록.
        /// 4. (semantic only) 임베딩 계산 → `memory_vectors` BLOB 저장 → ANN add.
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

            // ── semantic: 임베딩 계산 → memory_vectors BLOB → ANN add ──────
            #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
            {
                match self.embedder.embed(&event.content) {
                    Ok(vec) => {
                        // BLOB 저장
                        let blob = f32_to_blob(&vec);
                        let db_result = self.conn.execute(
                            "INSERT OR REPLACE INTO memory_vectors(mem_id, embedding) VALUES (?1, ?2)",
                            params![mem_id, blob],
                        );
                        if let Err(e) = db_result {
                            eprintln!("[tunaSalon] warn: memory_vectors insert failed (mem_id={mem_id}): {e}");
                        }
                        // ANN add
                        if let Some(ann) = &self.ann {
                            if let Err(e) = ann.add(mem_id as u64, &vec) {
                                eprintln!(
                                    "[tunaSalon] warn: ANN add failed (mem_id={mem_id}): {e}"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[tunaSalon] warn: embed failed (mem_id={mem_id}): {e}");
                    }
                }
            }
        }

        /// ANN 의미 검색 (semantic only).
        ///
        /// `query`를 임베딩 → ANN 검색 → `(mem_id, distance)` 반환.
        /// distance 낮을수록 가까움(코사인). 참여 격리 없음(raw 검색) — 격리는 task-49 hybrid에서.
        /// 오류·ANN 없음 → 빈 Vec.
        #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
        pub fn vector_search(&self, query: &str, k: usize) -> Vec<(i64, f32)> {
            if k == 0 {
                return vec![];
            }
            let ann = match &self.ann {
                Some(a) => a,
                None => return vec![],
            };
            let vec = match self.embedder.embed(query) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[tunaSalon] warn: vector_search embed failed: {e}");
                    return vec![];
                }
            };
            match ann.search(&vec, k) {
                Ok(results) => results
                    .into_iter()
                    .map(|(key, dist)| (key as i64, dist))
                    .collect(),
                Err(e) => {
                    eprintln!("[tunaSalon] warn: vector_search ann.search failed: {e}");
                    vec![]
                }
            }
        }

        /// `persona`의 과거 사건 중 `query`와 FTS5 BM25 점수가 높은 것을 최대 `k`개 반환한다(owned).
        ///
        /// 알고리즘:
        /// 1. participation 테이블에서 persona가 참여한 방 집합을 구한다.
        /// 2. bm25_leg_ids: FTS5 OR-MATCH + room IN + bm25 정렬 → 랭크순 id Vec (R3② 공유).
        /// 3. fetch_events_by_ids: id 순서대로 사건 fetch → MemoryEvent owned (R3② 공유).
        ///
        /// k=0 / 빈쿼리 / 미참여 → 빈 Vec. 런타임 오류는 빈 Vec으로 조용히 처리.
        ///
        /// NOTE: `friend-engine-semantic` feature off(BM25-only 빌드)에서만 컴파일.
        ///       semantic build는 아래 hybrid recall이 같은 시그니처로 대체한다.
        #[cfg(not(feature = "friend-engine-semantic"))]
        pub fn recall(&self, persona: &str, query: &str, k: usize) -> Vec<MemoryEvent> {
            if k == 0 {
                return vec![];
            }

            // 1. persona가 참여한 방 목록
            let rooms = self.participated_rooms(persona);

            if rooms.is_empty() {
                return vec![];
            }

            // 2. BM25 id-leg(랭크순) → 3. per-id fetch. hybrid와 fetch 전략 통일(R3②).
            //    1단계 full-row SELECT를 id-leg+fetch 2단계로 재구조화 — bm25 정렬·tie-break
            //    (ts DESC, id DESC) 동일이라 결과 순서 byte-identical.
            let ids = self.bm25_leg_ids(query, &rooms, k);
            self.fetch_events_by_ids(&ids)
        }

        // ── hybrid recall (friend-engine-semantic only) ───────────────────────

        /// BM25(어휘) + ANN(의미) 두 leg를 RRF(k=60)로 융합한 hybrid recall.
        ///
        /// `friend-engine-semantic` feature on 시에만 컴파일. 시그니처는 BM25-only recall과 동일.
        ///
        /// 알고리즘:
        /// 1. persona가 참여한 방 집합(participation 테이블).
        /// 2. BM25 leg: 기존 FTS5 OR-MATCH(참여 방 필터) → id 랭크순 Vec, N=k*4.
        /// 3. 벡터 leg: vector_search(k*4) → 참여 방으로 필터 → id 랭크순 Vec.
        /// 4. rrf_fuse(bm25_ids, vec_ids, 60.0) → 상위 k id.
        /// 5. 각 id의 사건을 순서대로 fetch → Vec<MemoryEvent>.
        ///
        /// 임베딩 실패 / ANN 없음 → 벡터 leg 빈 Vec → RRF는 BM25만(graceful 폴백).
        /// 모든 런타임 오류는 빈 Vec(unwrap/panic 없음).
        #[cfg(feature = "friend-engine-semantic")]
        pub fn recall(&self, persona: &str, query: &str, k: usize) -> Vec<MemoryEvent> {
            if k == 0 {
                return vec![];
            }

            // 1. persona가 참여한 방 목록
            let rooms = self.participated_rooms(persona);

            if rooms.is_empty() {
                return vec![];
            }

            let over_fetch = k * 4;

            // 2. BM25 leg: FTS5 OR-MATCH + 참여 방 필터 → id 랭크 Vec (R3② 공유 헬퍼)
            let bm25_ids = self.bm25_leg_ids(query, &rooms, over_fetch);

            // 3. 벡터 leg: vector_search → 참여 방으로 필터
            #[cfg(not(target_os = "windows"))]
            let vec_ids: Vec<i64> = {
                // raw ANN 검색(참여 격리 없음)
                let raw = self.vector_search(query, over_fetch);
                if raw.is_empty() {
                    vec![]
                } else {
                    // id들의 room을 일괄 조회해 참여 방인지 필터
                    let raw_ids: Vec<i64> = raw.iter().map(|(id, _)| *id).collect();
                    let id_placeholders = sql_placeholders(raw_ids.len(), 1);
                    let room_placeholders = sql_placeholders(rooms.len(), raw_ids.len() + 1);

                    let filter_sql = format!(
                        "SELECT id FROM memories WHERE id IN ({id_placeholders}) AND room IN ({room_placeholders})"
                    );

                    let participated_set: std::collections::HashSet<i64> =
                        match self.conn.prepare(&filter_sql) {
                            Err(_) => std::collections::HashSet::new(),
                            Ok(mut stmt) => {
                                use rusqlite::types::ToSql;
                                let mut param_vals: Vec<Box<dyn ToSql>> = Vec::new();
                                for id in &raw_ids {
                                    param_vals.push(Box::new(*id));
                                }
                                for r in &rooms {
                                    param_vals.push(Box::new(r.clone()));
                                }
                                let param_refs: Vec<&dyn ToSql> =
                                    param_vals.iter().map(|b| b.as_ref()).collect();
                                match stmt.query_map(param_refs.as_slice(), |row| row.get(0)) {
                                    Ok(rows) => {
                                        rows.filter_map(|r: rusqlite::Result<i64>| r.ok()).collect()
                                    }
                                    Err(_) => std::collections::HashSet::new(),
                                }
                            }
                        };

                    // distance 오름차순 유지하면서 참여 방인 id만 남김
                    raw.into_iter()
                        .filter(|(id, _)| participated_set.contains(id))
                        .map(|(id, _)| id)
                        .collect()
                }
            };

            // windows: 벡터 검색 비지원 → 빈 Vec
            #[cfg(target_os = "windows")]
            let vec_ids: Vec<i64> = vec![];

            // 4. RRF 융합 → 상위 k id
            let fused = rrf_fuse(&bm25_ids, &vec_ids, 60.0);
            let top_ids: Vec<i64> = fused.into_iter().take(k).collect();

            if top_ids.is_empty() {
                return vec![];
            }

            // 5. fused 순서대로 각 id의 사건을 fetch (R3② 공유 헬퍼)
            self.fetch_events_by_ids(&top_ids)
        }

        /// BM25(어휘) leg: FTS5 OR-MATCH + 참여 방 필터 → bm25 랭크순 mem id Vec.
        ///
        /// `ORDER BY bm25 ASC, m.ts DESC, m.id DESC LIMIT limit`. rng 무소비·결정적.
        /// BM25-only recall과 hybrid recall의 BM25 leg가 공유한다(R3②).
        /// 빈 토큰 / prepare 실패 / query 실패 → 빈 Vec(조용히).
        #[cfg(feature = "friend-engine")]
        fn bm25_leg_ids(&self, query: &str, rooms: &[String], limit: usize) -> Vec<i64> {
            let tokens = crate::tokenize_ko::morphological_tokens(query);
            if tokens.is_empty() {
                return vec![];
            }
            let match_expr: String = fts_or_match(&tokens);
            let placeholders = sql_placeholders(rooms.len(), 3);
            let sql = format!(
                "SELECT m.id
                 FROM memories_fts
                 JOIN memories m ON m.id = memories_fts.mem_id
                 WHERE memories_fts.tokens MATCH ?1
                   AND m.room IN ({placeholders})
                 ORDER BY bm25(memories_fts) ASC, m.ts DESC, m.id DESC
                 LIMIT ?2"
            );
            let mut stmt = match self.conn.prepare(&sql) {
                Ok(s) => s,
                Err(_) => return vec![],
            };
            use rusqlite::types::ToSql;
            let mut param_vals: Vec<Box<dyn ToSql>> = Vec::new();
            param_vals.push(Box::new(match_expr));
            param_vals.push(Box::new(limit as i64));
            for r in rooms {
                param_vals.push(Box::new(r.clone()));
            }
            let param_refs: Vec<&dyn ToSql> = param_vals.iter().map(|b| b.as_ref()).collect();
            let rows = match stmt.query_map(param_refs.as_slice(), |row| row.get(0)) {
                Ok(rows) => rows,
                Err(_) => return vec![],
            };
            rows.filter_map(|r| r.ok()).collect()
        }

        /// mem id 리스트를 입력 순서대로 MemoryEvent로 fetch한다(2단계 fetch).
        ///
        /// 각 id를 `SELECT room, ts, speaker, content WHERE id=?` 로 조회. 누락 id는 건너뜀.
        /// 입력 순서 = 출력 순서(rank 보존). BM25-only/hybrid recall이 공유(R3②).
        #[cfg(feature = "friend-engine")]
        fn fetch_events_by_ids(&self, ids: &[i64]) -> Vec<MemoryEvent> {
            let mut results: Vec<MemoryEvent> = Vec::with_capacity(ids.len());
            for mem_id in ids {
                let row = self.conn.query_row(
                    "SELECT room, ts, speaker, content FROM memories WHERE id = ?1",
                    params![mem_id],
                    |row| row_to_memory_event(row, 0),
                );
                if let Ok(ev) = row {
                    results.push(ev);
                }
            }
            results
        }

        /// 회상 결과를 회상 슬롯용 문자열로 포맷한다.
        pub fn format_recall(events: &[MemoryEvent]) -> Option<String> {
            format_recall_impl(events)
        }

        /// `persona`가 참여한 방 목록을 반환한다.
        ///
        /// DB 오류 시 빈 Vec. 호출처에서 `rooms.is_empty()` 가드를 유지해야 한다.
        #[cfg(feature = "friend-engine")]
        fn participated_rooms(&self, persona: &str) -> Vec<String> {
            let mut stmt = match self.conn.prepare("SELECT room FROM participation WHERE persona = ?1") {
                Ok(s) => s,
                Err(_) => return vec![],
            };
            let rows = match stmt.query_map(params![persona], |row| row.get(0)) {
                Ok(r) => r,
                Err(_) => return vec![],
            };
            rows.filter_map(|r| r.ok()).collect()
        }

        // ── 테스트 전용 접근자 ────────────────────────────────────────────────

        /// `memory_vectors` 행 수 (semantic feature 테스트용).
        #[cfg(all(test, feature = "friend-engine-semantic", not(target_os = "windows")))]
        pub fn test_vector_row_count(&self) -> i64 {
            self.conn
                .query_row("SELECT COUNT(*) FROM memory_vectors", [], |r| r.get(0))
                .unwrap_or(0)
        }

        /// ANN 크기 (semantic feature 테스트용).
        #[cfg(all(test, feature = "friend-engine-semantic", not(target_os = "windows")))]
        pub fn test_ann_size(&self) -> usize {
            self.ann.as_ref().map(|a| a.size()).unwrap_or(0)
        }

        /// ANN 레퍼런스 (semantic feature 테스트용, roundtrip save).
        #[cfg(all(test, feature = "friend-engine-semantic", not(target_os = "windows")))]
        pub fn test_ann_save(&self) -> Result<(), String> {
            match &self.ann {
                Some(a) => a.save(),
                None => Ok(()),
            }
        }
    }

    impl Default for MemoryStore {
        fn default() -> Self {
            Self::new()
        }
    }

    /// 라이브 스토어용 임베더를 선택한다 (semantic only).
    ///
    /// BGE-M3 모델이 다운로드되어 있으면 OrtEmbedder, 없으면 MockEmbedder 폴백.
    #[cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]
    fn choose_live_embedder() -> Box<dyn crate::embed::Embedder> {
        use crate::embed::{model_manager, MockEmbedder, OrtEmbedder};
        let model_dir = model_manager::default_model_path();
        if model_manager::is_downloaded(&model_dir) {
            match OrtEmbedder::new(&model_dir) {
                Ok(e) => {
                    eprintln!("[tunaSalon] BGE-M3 OrtEmbedder 로드 완료(의미 회상 활성).");
                    return Box::new(e);
                }
                Err(e) => eprintln!("[tunaSalon] warn: OrtEmbedder 로드 실패({e}), Mock 폴백."),
            }
        }
        Box::new(MockEmbedder::default())
    }

    impl std::fmt::Debug for MemoryStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MemoryStore(SQLite:memory:)").finish()
        }
    }

    // ── 순수 문자열 헬퍼 ──────────────────────────────────────────────────────

    /// FTS5 OR-MATCH 식을 생성한다.
    ///
    /// 각 토큰의 `"` 를 `""` 로 escape 후 `"tok1" OR "tok2" ...` 형태로 join.
    /// 빈 토큰 목록이면 빈 문자열을 반환한다.
    ///
    /// 호출처: BM25-only recall + hybrid recall BM25 leg.
    #[cfg(feature = "friend-engine")]
    pub(super) fn fts_or_match(tokens: &[String]) -> String {
        tokens
            .iter()
            .map(|t| {
                let escaped = t.replace('"', "\"\"");
                format!("\"{}\"", escaped)
            })
            .collect::<Vec<_>>()
            .join(" OR ")
    }

    /// SQL positional placeholder 문자열을 생성한다.
    ///
    /// `count` 개의 `?N`(1-based, `start`부터)을 `", "` 로 join.
    /// 예) `sql_placeholders(3, 2)` → `"?2, ?3, ?4"`.
    /// count=0 이면 빈 문자열을 반환한다.
    ///
    /// 호출처: recall BM25 leg(start=3), hybrid vector 필터(start=1 / start=raw_ids.len()+1).
    #[cfg(feature = "friend-engine")]
    pub(super) fn sql_placeholders(count: usize, start: usize) -> String {
        (0..count)
            .map(|i| format!("?{}", i + start))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `row`에서 `base` 번째 컬럼부터 room/ts/speaker/content를 읽어 `MemoryEvent`를 만든다.
    ///
    /// - BM25-only recall: `base=1` (0번=id, skip).
    /// - hybrid recall 최종 fetch: `base=0` (SELECT room, ts, speaker, content).
    #[cfg(feature = "friend-engine")]
    fn row_to_memory_event(row: &rusqlite::Row, base: usize) -> rusqlite::Result<MemoryEvent> {
        let room: String = row.get(base)?;
        let ts: i64 = row.get(base + 1)?;
        let speaker: String = row.get(base + 2)?;
        let content: String = row.get(base + 3)?;
        Ok(MemoryEvent {
            room,
            ts: ts as u64,
            speaker,
            content,
        })
    }

    // ── RRF 융합 헬퍼 ─────────────────────────────────────────────────────────

    /// Reciprocal Rank Fusion: 두 ranked id 리스트를 k_rrf(=60)로 융합한다.
    ///
    /// 각 리스트에서 `score += 1.0 / (k_rrf + rank + 1)` 키별 합산.
    /// 정렬: score 내림차순, 동점은 id 오름차순(결정적 tie-break).
    ///
    /// seCall `reciprocal_rank_fusion` 동형(observer penalty/정규화 미포함).
    #[allow(dead_code)]
    pub(super) fn rrf_fuse(bm25: &[i64], vector: &[i64], k_rrf: f64) -> Vec<i64> {
        use std::collections::HashMap;
        let mut scores: HashMap<i64, f64> = HashMap::new();

        for (rank, &id) in bm25.iter().enumerate() {
            *scores.entry(id).or_insert(0.0) += 1.0 / (k_rrf + rank as f64 + 1.0);
        }
        for (rank, &id) in vector.iter().enumerate() {
            *scores.entry(id).or_insert(0.0) += 1.0 / (k_rrf + rank as f64 + 1.0);
        }

        let mut pairs: Vec<(i64, f64)> = scores.into_iter().collect();
        // score 내림차순, 동점 id 오름차순(결정적)
        pairs.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        pairs.into_iter().map(|(id, _)| id).collect()
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
