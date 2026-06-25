#![cfg(feature = "web")]
//! 방 상태 영속 스토어 (task F).
//!
//! 채팅방의 참가자/대화로그/화제를 SQLite(`rooms.db`)에 저장하고 복원한다.
//! friend engine의 `memory.db`와는 독립된 별 파일·별 스키마.
//!
//! 저장 계층만 담당하며, web.rs 배선과 LiveSession 복원 주입은 task G에서 수행한다.

use crate::live::PersonaMeta;
use crate::model::{Event, Persona, PersonaModifier};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 방 상태 저장소.
pub struct RoomStore {
    conn: Connection,
}

/// 특정 room의 스냅샷 (복원 단위).
#[derive(Debug, Clone)]
pub struct RoomSnapshot {
    /// 참가자 목록 (등록 순서 ord 보존).
    pub participants: Vec<(Persona, PersonaMeta)>,
    /// 완성된 발화만 (content = Some 인 것).
    pub messages: Vec<Event>,
    /// 방 화제 태그.
    pub topics: Vec<String>,
    /// 누적 틱 카운터.
    pub tick_count: u64,
}

impl RoomStore {
    /// 파일 경로의 SQLite를 열거나 생성한다.
    ///
    /// - `:memory:` 경로도 열린다(단위 테스트용).
    /// - 파일 경로일 때 부모 디렉터리를 재귀 생성한다.
    /// - WAL 모드 + IF NOT EXISTS 스키마 init.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        // 파일 경로이면 부모 디렉터리를 생성한다.
        // ":memory:" 는 parent() = None 이므로 건너뛴다.
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    rusqlite::Error::InvalidPath(PathBuf::from(format!(
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

    /// 기본 `rooms.db` 경로를 반환한다.
    ///
    /// 우선순위:
    ///   1. `$SALON_ROOMS_DB` (비어 있지 않으면)
    ///   2. `$HOME/.local/share/tunaSalon/rooms.db`
    ///   3. 둘 다 없으면 `None`
    pub fn default_rooms_db_path() -> Option<PathBuf> {
        if let Ok(val) = std::env::var("SALON_ROOMS_DB") {
            if !val.is_empty() {
                return Some(PathBuf::from(val));
            }
        }
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            if !home.is_empty() {
                return Some(PathBuf::from(home).join(".local/share/tunaSalon/rooms.db"));
            }
        }
        None
    }

    /// 방 상태를 `rooms.db`에 저장한다 (트랜잭션, 전량 교체).
    ///
    /// 해당 `room_id`의 기존 row를 모두 DELETE한 뒤 새로 INSERT한다.
    ///
    /// - `participants`: `personas` 순회, ord = 인덱스.
    ///   `persona_meta.get(&p.id)` 없는 persona는 건너뛴다.
    /// - `messages`: `history` 에서 content = Some 인 것만 seq(0부터) 부여.
    /// - `room_meta`: topics = JSON, tick_count = 전달값, updated_at = 0.
    pub fn save(
        &self,
        room_id: &str,
        personas: &[Persona],
        persona_meta: &BTreeMap<String, PersonaMeta>,
        history: &[Event],
        topics: &[String],
        tick_count: u64,
    ) -> rusqlite::Result<()> {
        // 트랜잭션 시작
        self.conn.execute("BEGIN", [])?;

        let result = (|| -> rusqlite::Result<()> {
            // 기존 row 삭제
            self.conn.execute(
                "DELETE FROM room_participants WHERE room_id = ?1",
                params![room_id],
            )?;
            self.conn.execute(
                "DELETE FROM room_messages WHERE room_id = ?1",
                params![room_id],
            )?;
            self.conn
                .execute("DELETE FROM room_meta WHERE room_id = ?1", params![room_id])?;

            // participants 삽입
            for (ord, persona) in personas.iter().enumerate() {
                let meta = match persona_meta.get(&persona.id) {
                    Some(m) => m,
                    None => continue, // persona_meta에 없으면 건너뜀
                };
                self.conn.execute(
                    "INSERT INTO room_participants(
                        room_id, ord, persona_id, name, base_rate,
                        backend, system_prompt, reactivity, provocativeness
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        room_id,
                        ord as i64,
                        persona.id,
                        persona.name,
                        persona.base_rate,
                        meta.backend,
                        meta.system_prompt,
                        meta.modifier.reactivity,
                        meta.modifier.provocativeness,
                    ],
                )?;
            }

            // messages 삽입 (content = Some 만)
            let mut seq: i64 = 0;
            for event in history {
                if let Some(ref content) = event.content {
                    self.conn.execute(
                        "INSERT INTO room_messages(room_id, seq, ts, speaker, mark, content)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![room_id, seq, event.ts, event.speaker, event.mark, content,],
                    )?;
                    seq += 1;
                }
            }

            // room_meta 삽입
            let topics_json = serde_json::to_string(topics).unwrap_or_else(|_| "[]".to_string());
            self.conn.execute(
                "INSERT INTO room_meta(room_id, topics_json, tick_count, updated_at)
                 VALUES (?1, ?2, ?3, 0)",
                params![room_id, topics_json, tick_count as i64],
            )?;

            Ok(())
        })();

        match result {
            Ok(()) => {
                self.conn.execute("COMMIT", [])?;
                Ok(())
            }
            Err(e) => {
                let _ = self.conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    pub fn delete_room(&self, room_id: &str) -> rusqlite::Result<()> {
        self.conn.execute("BEGIN", [])?;
        let result = (|| -> rusqlite::Result<()> {
            self.conn.execute(
                "DELETE FROM room_participants WHERE room_id = ?1",
                params![room_id],
            )?;
            self.conn.execute(
                "DELETE FROM room_messages WHERE room_id = ?1",
                params![room_id],
            )?;
            self.conn
                .execute("DELETE FROM room_meta WHERE room_id = ?1", params![room_id])?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.conn.execute("COMMIT", [])?;
                Ok(())
            }
            Err(e) => {
                let _ = self.conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    /// 방 상태를 복원한다.
    ///
    /// 해당 `room_id` row가 `room_meta`에 없으면 `Ok(None)`.
    /// 있으면 participants(ord ASC) / messages(seq ASC) / topics / tick_count를
    /// `RoomSnapshot`으로 조합해 반환한다.
    pub fn load(&self, room_id: &str) -> rusqlite::Result<Option<RoomSnapshot>> {
        // room_meta 확인
        let meta_row: Option<(String, i64)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT topics_json, tick_count FROM room_meta WHERE room_id = ?1")?;
            let mut rows = stmt.query(params![room_id])?;
            match rows.next()? {
                Some(row) => {
                    let topics_json: String = row.get(0)?;
                    let tick_count: i64 = row.get(1)?;
                    Some((topics_json, tick_count))
                }
                None => None,
            }
        };

        let (topics_json, tick_count_raw) = match meta_row {
            Some(pair) => pair,
            None => return Ok(None),
        };

        // topics 역직렬화
        let topics: Vec<String> = serde_json::from_str(&topics_json).unwrap_or_default();

        // participants (ord ASC)
        let participants: Vec<(Persona, PersonaMeta)> = {
            let mut stmt = self.conn.prepare(
                "SELECT persona_id, name, base_rate,
                        backend, system_prompt, reactivity, provocativeness
                 FROM room_participants
                 WHERE room_id = ?1
                 ORDER BY ord ASC",
            )?;
            let rows = stmt.query_map(params![room_id], |row| {
                let persona = Persona {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    base_rate: row.get(2)?,
                };
                let meta = PersonaMeta {
                    backend: row.get(3)?,
                    system_prompt: row.get(4)?,
                    modifier: PersonaModifier {
                        reactivity: row.get(5)?,
                        provocativeness: row.get(6)?,
                    },
                    axes: None,
                };
                Ok((persona, meta))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        // messages (seq ASC)
        let messages: Vec<Event> = {
            let mut stmt = self.conn.prepare(
                "SELECT ts, speaker, mark, content
                 FROM room_messages
                 WHERE room_id = ?1
                 ORDER BY seq ASC",
            )?;
            let rows = stmt.query_map(params![room_id], |row| {
                Ok(Event {
                    ts: row.get(0)?,
                    speaker: row.get(1)?,
                    mark: row.get(2)?,
                    content: Some(row.get(3)?), // 저장 시 content=Some 만 기록
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        Ok(Some(RoomSnapshot {
            participants,
            messages,
            topics,
            tick_count: tick_count_raw as u64,
        }))
    }
}

/// 스키마 초기화 (IF NOT EXISTS, 멱등).
fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS room_participants(
            room_id           TEXT NOT NULL,
            ord               INTEGER NOT NULL,
            persona_id        TEXT NOT NULL,
            name              TEXT NOT NULL,
            base_rate         REAL NOT NULL,
            backend           TEXT NOT NULL,
            system_prompt     TEXT NOT NULL,
            reactivity        REAL NOT NULL,
            provocativeness   REAL NOT NULL,
            PRIMARY KEY(room_id, persona_id)
        );
        CREATE TABLE IF NOT EXISTS room_messages(
            room_id  TEXT NOT NULL,
            seq      INTEGER NOT NULL,
            ts       REAL NOT NULL,
            speaker  TEXT NOT NULL,
            mark     REAL NOT NULL,
            content  TEXT NOT NULL,
            PRIMARY KEY(room_id, seq)
        );
        CREATE TABLE IF NOT EXISTS room_meta(
            room_id      TEXT PRIMARY KEY,
            topics_json  TEXT NOT NULL,
            tick_count   INTEGER NOT NULL,
            updated_at   INTEGER NOT NULL
        );",
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// 단위 테스트 (web feature 전용, :memory: SQLite)
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::PersonaMeta;
    use crate::model::{Event, Persona, PersonaModifier};
    use std::collections::BTreeMap;

    fn make_store() -> RoomStore {
        RoomStore::open(Path::new(":memory:")).expect("in-memory RoomStore must open")
    }

    fn make_persona(id: &str, name: &str, base_rate: f64) -> Persona {
        Persona {
            id: id.to_string(),
            name: name.to_string(),
            base_rate,
        }
    }

    fn make_meta(
        backend: &str,
        prompt: &str,
        reactivity: f64,
        provocativeness: f64,
    ) -> PersonaMeta {
        PersonaMeta {
            backend: backend.to_string(),
            system_prompt: prompt.to_string(),
            modifier: PersonaModifier {
                reactivity,
                provocativeness,
            },
            axes: None,
        }
    }

    fn make_event(ts: f64, speaker: &str, mark: f64, content: Option<&str>) -> Event {
        Event {
            ts,
            speaker: speaker.to_string(),
            mark,
            content: content.map(|s| s.to_string()),
        }
    }

    /// (1) save -> load 라운드트립: participants 순서·meta 포함, messages, topics, tick_count 일치.
    #[test]
    fn roundtrip_save_load() {
        let store = make_store();

        let personas = vec![
            make_persona("aria", "Aria", 0.8),
            make_persona("bjorn", "Bjorn", 0.7),
        ];

        let mut persona_meta = BTreeMap::new();
        persona_meta.insert(
            "aria".to_string(),
            make_meta("cloud", "system aria", 1.2, 0.9),
        );
        persona_meta.insert(
            "bjorn".to_string(),
            make_meta("friend", "system bjorn", 0.8, 1.1),
        );

        let history = vec![
            make_event(0.0, "aria", 0.5, Some("안녕하세요")),
            make_event(1.0, "bjorn", 0.3, Some("반갑습니다")),
            make_event(2.0, "aria", 0.0, None), // placeholder: 저장 제외
            make_event(3.0, "bjorn", 0.4, Some("날씨 좋네요")),
        ];

        let topics = vec!["러스트".to_string(), "AI".to_string()];

        store
            .save("room1", &personas, &persona_meta, &history, &topics, 42)
            .expect("save 성공");

        let snap = store
            .load("room1")
            .expect("load 성공")
            .expect("Some이어야 함");

        // tick_count
        assert_eq!(snap.tick_count, 42);

        // topics
        assert_eq!(snap.topics, topics);

        // messages: content=Some 인 3개만 (None placeholder 제외)
        assert_eq!(snap.messages.len(), 3);
        assert_eq!(snap.messages[0].speaker, "aria");
        assert_eq!(snap.messages[0].content, Some("안녕하세요".to_string()));
        assert_eq!(snap.messages[1].speaker, "bjorn");
        assert_eq!(snap.messages[1].content, Some("반갑습니다".to_string()));
        assert_eq!(snap.messages[2].speaker, "bjorn");
        assert_eq!(snap.messages[2].content, Some("날씨 좋네요".to_string()));

        // participants 순서·meta
        assert_eq!(snap.participants.len(), 2);
        let (p0, m0) = &snap.participants[0];
        assert_eq!(p0.id, "aria");
        assert_eq!(p0.name, "Aria");
        assert!((p0.base_rate - 0.8).abs() < 1e-10);
        assert_eq!(m0.backend, "cloud");
        assert_eq!(m0.system_prompt, "system aria");
        assert!((m0.modifier.reactivity - 1.2).abs() < 1e-10);
        assert!((m0.modifier.provocativeness - 0.9).abs() < 1e-10);

        let (p1, m1) = &snap.participants[1];
        assert_eq!(p1.id, "bjorn");
        assert_eq!(m1.backend, "friend");
        assert!((m1.modifier.provocativeness - 1.1).abs() < 1e-10);
    }

    /// (2) content=None placeholder는 messages에서 제외된다.
    #[test]
    fn placeholder_excluded_from_messages() {
        let store = make_store();

        let personas = vec![make_persona("aria", "Aria", 0.8)];
        let mut persona_meta = BTreeMap::new();
        persona_meta.insert("aria".to_string(), make_meta("cloud", "prompt", 1.0, 1.0));

        // 전부 content=None
        let history = vec![
            make_event(0.0, "aria", 0.5, None),
            make_event(1.0, "aria", 0.5, None),
        ];

        store
            .save("room2", &personas, &persona_meta, &history, &[], 0)
            .expect("save");

        let snap = store.load("room2").expect("load").expect("Some");
        assert_eq!(
            snap.messages.len(),
            0,
            "placeholder만 있으면 messages는 비어야 함"
        );
    }

    /// (3) 없는 room_id load -> None.
    #[test]
    fn load_nonexistent_room_returns_none() {
        let store = make_store();
        let result = store.load("no_such_room").expect("load 자체는 성공");
        assert!(result.is_none(), "존재하지 않는 room_id -> None");
    }

    /// (4) persona_meta에 없는 persona는 participants에서 제외된다.
    #[test]
    fn persona_without_meta_is_excluded() {
        let store = make_store();

        let personas = vec![
            make_persona("aria", "Aria", 0.8),
            make_persona("ghost", "Ghost", 0.5), // meta 없음
        ];

        let mut persona_meta = BTreeMap::new();
        persona_meta.insert("aria".to_string(), make_meta("cloud", "prompt", 1.0, 1.0));
        // ghost는 persona_meta에 없음

        store
            .save("room3", &personas, &persona_meta, &[], &[], 0)
            .expect("save");

        let snap = store.load("room3").expect("load").expect("Some");
        assert_eq!(snap.participants.len(), 1, "meta 없는 persona는 저장 제외");
        assert_eq!(snap.participants[0].0.id, "aria");
    }

    /// (5) save를 두 번 호출하면 두 번째 결과로 덮어써진다 (전량 교체).
    #[test]
    fn save_twice_overwrites() {
        let store = make_store();

        let personas = vec![make_persona("aria", "Aria", 0.8)];
        let mut persona_meta = BTreeMap::new();
        persona_meta.insert("aria".to_string(), make_meta("cloud", "prompt", 1.0, 1.0));

        let history1 = vec![make_event(0.0, "aria", 0.5, Some("첫 번째"))];
        let history2 = vec![
            make_event(0.0, "aria", 0.5, Some("두 번째 A")),
            make_event(1.0, "aria", 0.5, Some("두 번째 B")),
        ];

        store
            .save(
                "room4",
                &personas,
                &persona_meta,
                &history1,
                &["topic1".to_string()],
                10,
            )
            .expect("save 1");
        store
            .save(
                "room4",
                &personas,
                &persona_meta,
                &history2,
                &["topic2".to_string()],
                20,
            )
            .expect("save 2");

        let snap = store.load("room4").expect("load").expect("Some");
        assert_eq!(snap.tick_count, 20);
        assert_eq!(snap.topics, vec!["topic2".to_string()]);
        assert_eq!(snap.messages.len(), 2);
        assert_eq!(snap.messages[0].content, Some("두 번째 A".to_string()));
    }
}
