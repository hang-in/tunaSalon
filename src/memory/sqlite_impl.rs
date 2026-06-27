//! SQLite(:memory:/파일) + FTS5 BM25 + (semantic) 벡터 회상 구현.
//!
//! memory.rs(god-file)에서 `mod sqlite_impl` 본문을 파일로 분리(god-file 분해).
//! 로직·SQL 무변경(선행 들여쓰기만 파일 모듈 기준으로 정렬). 회상 결과 byte-identical.

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
