#![cfg(feature = "web")]
//! 방 상태 영속 스토어 (task F).
//!
//! 채팅방의 참가자/대화로그/화제를 SQLite(`rooms.db`)에 저장하고 복원한다.
//! friend engine의 `memory.db`와는 독립된 별 파일·별 스키마.
//!
//! 저장 계층만 담당하며, web.rs 배선과 LiveSession 복원 주입은 task G에서 수행한다.

use crate::live::{PersonaAxes, PersonaMeta};
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
    /// 사람(나)이 고른 4축 캐릭터. 없으면 None.
    pub human_axes: Option<PersonaAxes>,
    /// 종료 시 생성된 마크다운 리포트. 결론 난 방만 Some.
    pub report: Option<String>,
}

/// 방의 리포트 히스토리 레코드 (room_reports 테이블 1행).
#[derive(Debug, Clone)]
pub struct ReportRecord {
    pub seq: u32,
    pub created_at: i64,
    pub topic: String,
    pub markdown: String,
    pub conclusion: String,
}

/// "이전 토론" 목록 항목 (room_meta 1행 + 리포트 수). 메시지/참가자 전량 미포함(경량).
#[derive(Debug, Clone)]
pub struct RoomListItem {
    pub room_id: String,
    pub topics: Vec<String>,
    pub updated_at: i64,
    pub concluded: bool,
    pub report_count: i64,
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
    /// - `room_meta`: topics = JSON, tick_count = 전달값, updated_at = 저장 시점 unix 초.
    #[allow(clippy::too_many_arguments)]
    pub fn save(
        &self,
        room_id: &str,
        personas: &[Persona],
        persona_meta: &BTreeMap<String, PersonaMeta>,
        history: &[Event],
        topics: &[String],
        tick_count: u64,
        human_axes: Option<&PersonaAxes>,
        report: Option<&str>,
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
                let axes = meta.axes.as_ref();
                self.conn.execute(
                    "INSERT INTO room_participants(
                        room_id, ord, persona_id, name, base_rate,
                        backend, system_prompt, reactivity, provocativeness,
                        blood, mbti, zodiac, role
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
                        axes.map(|a| a.blood.clone()),
                        axes.map(|a| a.mbti.clone()),
                        axes.map(|a| a.zodiac.clone()),
                        axes.map(|a| a.role.clone()),
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

            // room_meta 삽입 (사람 4축 포함)
            let topics_json = serde_json::to_string(topics).unwrap_or_else(|_| "[]".to_string());
            // updated_at: 저장 시점 unix 초("이전 토론" 목록 최신순 정렬·날짜 표시용).
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            self.conn.execute(
                "INSERT INTO room_meta(
                    room_id, topics_json, tick_count, updated_at,
                    human_blood, human_mbti, human_zodiac, human_role, report
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    room_id,
                    topics_json,
                    tick_count as i64,
                    now,
                    human_axes.map(|a| a.blood.clone()),
                    human_axes.map(|a| a.mbti.clone()),
                    human_axes.map(|a| a.zodiac.clone()),
                    human_axes.map(|a| a.role.clone()),
                    report,
                ],
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

    /// 새 리포트를 room_reports에 삽입하고 부여된 seq를 반환한다.
    /// seq는 해당 room_id의 최대 seq + 1 (없으면 1부터).
    pub fn append_report(
        &self,
        room_id: &str,
        topic: &str,
        markdown: &str,
        conclusion: &str,
    ) -> rusqlite::Result<u32> {
        let next_seq: u32 = {
            let max: Option<i64> = self
                .conn
                .query_row(
                    "SELECT MAX(seq) FROM room_reports WHERE room_id = ?1",
                    params![room_id],
                    |row| row.get(0),
                )
                .unwrap_or(None);
            (max.unwrap_or(0) + 1) as u32
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO room_reports(room_id, seq, created_at, topic, markdown, conclusion) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![room_id, next_seq, now, topic, markdown, conclusion],
        )?;
        Ok(next_seq)
    }

    /// room_id의 모든 리포트를 seq 오름차순으로 반환한다.
    pub fn load_reports(&self, room_id: &str) -> rusqlite::Result<Vec<ReportRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, created_at, topic, markdown, conclusion \
             FROM room_reports WHERE room_id = ?1 ORDER BY seq ASC",
        )?;
        let records = stmt
            .query_map(params![room_id], |row| {
                Ok(ReportRecord {
                    seq: row.get::<_, i64>(0)? as u32,
                    created_at: row.get(1)?,
                    topic: row.get(2)?,
                    markdown: row.get(3)?,
                    conclusion: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(records)
    }

    /// room_id의 최근 n개 결론(conclusion 필드)을 seq 오름차순으로 반환한다.
    pub fn recent_conclusions(&self, room_id: &str, n: usize) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT conclusion FROM room_reports \
             WHERE room_id = ?1 ORDER BY seq DESC LIMIT ?2",
        )?;
        let mut results: Vec<String> = stmt
            .query_map(params![room_id, n as i64], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        results.reverse(); // DESC → ASC
        Ok(results)
    }

    /// 영속된 모든 방의 경량 메타를 최신 활동순으로 반환한다("이전 토론" 목록용).
    ///
    /// room_meta 1행당 1개. 메시지/참가자 전량은 로드하지 않는다(목록 화면용 경량 쿼리).
    /// concluded = report(종료 SSOT) 존재 여부. report_count = 누적 단계 리포트 수.
    pub fn list_rooms(&self) -> rusqlite::Result<Vec<RoomListItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.room_id, m.topics_json, m.updated_at,
                    (m.report IS NOT NULL) AS concluded,
                    (SELECT COUNT(*) FROM room_reports r WHERE r.room_id = m.room_id) AS report_count
             FROM room_meta m
             ORDER BY m.updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let topics_json: String = row.get(1)?;
            let topics: Vec<String> = serde_json::from_str(&topics_json).unwrap_or_default();
            Ok(RoomListItem {
                room_id: row.get(0)?,
                topics,
                updated_at: row.get(2)?,
                concluded: row.get::<_, i64>(3)? != 0,
                report_count: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// 방 상태를 복원한다.
    ///
    /// 해당 `room_id` row가 `room_meta`에 없으면 `Ok(None)`.
    /// 있으면 participants(ord ASC) / messages(seq ASC) / topics / tick_count를
    /// `RoomSnapshot`으로 조합해 반환한다.
    pub fn load(&self, room_id: &str) -> rusqlite::Result<Option<RoomSnapshot>> {
        // room_meta 확인 (사람 4축 + 리포트 포함)
        type MetaRow = (
            String,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let meta_row: Option<MetaRow> = {
            let mut stmt = self.conn.prepare(
                "SELECT topics_json, tick_count,
                        human_blood, human_mbti, human_zodiac, human_role, report
                 FROM room_meta WHERE room_id = ?1",
            )?;
            let mut rows = stmt.query(params![room_id])?;
            match rows.next()? {
                Some(row) => Some((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                )),
                None => None,
            }
        };

        let (topics_json, tick_count_raw, h_blood, h_mbti, h_zodiac, h_role, report) =
            match meta_row {
                Some(t) => t,
                None => return Ok(None),
            };
        let human_axes = match (h_blood, h_mbti, h_zodiac, h_role) {
            (Some(blood), Some(mbti), Some(zodiac), Some(role)) => {
                Some(PersonaAxes { blood, mbti, zodiac, role })
            }
            _ => None,
        };

        // topics 역직렬화
        let topics: Vec<String> = serde_json::from_str(&topics_json).unwrap_or_default();

        // participants (ord ASC)
        let participants: Vec<(Persona, PersonaMeta)> = {
            let mut stmt = self.conn.prepare(
                "SELECT persona_id, name, base_rate,
                        backend, system_prompt, reactivity, provocativeness,
                        blood, mbti, zodiac, role
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
                // 4축은 nullable(구 DB row는 NULL). 넷이 모두 있을 때만 axes 복원.
                let blood: Option<String> = row.get(7)?;
                let mbti: Option<String> = row.get(8)?;
                let zodiac: Option<String> = row.get(9)?;
                let role: Option<String> = row.get(10)?;
                let axes = match (blood, mbti, zodiac, role) {
                    (Some(blood), Some(mbti), Some(zodiac), Some(role)) => {
                        Some(PersonaAxes { blood, mbti, zodiac, role })
                    }
                    _ => None,
                };
                let meta = PersonaMeta {
                    backend: row.get(3)?,
                    system_prompt: row.get(4)?,
                    modifier: PersonaModifier {
                        reactivity: row.get(5)?,
                        provocativeness: row.get(6)?,
                    },
                    axes,
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
            human_axes,
            report,
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
    )?;
    // 4축(혈액형/MBTI/별자리/역할) 컬럼 마이그레이션. 기존 DB에도 추가(이미 있으면 무시).
    // CREATE TABLE IF NOT EXISTS는 컬럼을 추가하지 못하므로 ALTER로 멱등 보강한다.
    for col in ["blood", "mbti", "zodiac", "role"] {
        let _ = conn.execute(
            &format!("ALTER TABLE room_participants ADD COLUMN {col} TEXT"),
            [],
        );
    }
    // 사람(나) 4축은 room_meta에 보관(참가자가 아니라 방 단위 1개).
    for col in ["human_blood", "human_mbti", "human_zodiac", "human_role"] {
        let _ = conn.execute(&format!("ALTER TABLE room_meta ADD COLUMN {col} TEXT"), []);
    }
    // 종료 리포트(마크다운)도 방 단위 1개.
    let _ = conn.execute("ALTER TABLE room_meta ADD COLUMN report TEXT", []);
    // 단계형 토론 리포트 히스토리 테이블 (멱등 생성).
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS room_reports(
            room_id    TEXT NOT NULL,
            seq        INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            topic      TEXT NOT NULL,
            markdown   TEXT NOT NULL,
            conclusion TEXT NOT NULL,
            PRIMARY KEY(room_id, seq)
        );",
    )?;
    Ok(())
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

    /// 4축(axes) 라운드트립: Some이면 그대로 복원, 없는 persona는 None.
    #[test]
    fn axes_roundtrip_preserves_persona_axes() {
        let store = make_store();
        let personas = vec![
            make_persona("withaxes", "축있음", 0.8),
            make_persona("noaxes", "축없음", 0.7),
        ];
        let mut persona_meta = BTreeMap::new();
        let mut m = make_meta("cloud", "p1", 1.0, 1.0);
        m.axes = Some(PersonaAxes {
            blood: "O".to_string(),
            mbti: "ENTP".to_string(),
            zodiac: "leo".to_string(),
            role: "critic".to_string(),
        });
        persona_meta.insert("withaxes".to_string(), m);
        persona_meta.insert("noaxes".to_string(), make_meta("cloud", "p2", 1.0, 1.0));

        store
            .save("r", &personas, &persona_meta, &[], &[], 0, None, None)
            .expect("save");
        let snap = store.load("r").expect("load").expect("some");

        let axes = snap.participants[0].1.axes.as_ref().expect("axes Some");
        assert_eq!(axes.blood, "O");
        assert_eq!(axes.mbti, "ENTP");
        assert_eq!(axes.zodiac, "leo");
        assert_eq!(axes.role, "critic");
        assert!(snap.participants[1].1.axes.is_none(), "축 없으면 None");
    }

    /// 사람(나) 4축 라운드트립: room_meta에 저장·복원.
    #[test]
    fn human_axes_roundtrip() {
        let store = make_store();
        let human = PersonaAxes {
            blood: "A".to_string(),
            mbti: "INFJ".to_string(),
            zodiac: "pis".to_string(),
            role: "strategist".to_string(),
        };
        store
            .save("hr", &[], &BTreeMap::new(), &[], &[], 0, Some(&human), None)
            .expect("save");
        let snap = store.load("hr").expect("load").expect("some");
        let h = snap.human_axes.expect("human_axes Some");
        assert_eq!(h.blood, "A");
        assert_eq!(h.mbti, "INFJ");
        assert_eq!(h.zodiac, "pis");
        assert_eq!(h.role, "strategist");

        // 사람 미설정 방은 None.
        store
            .save("hr2", &[], &BTreeMap::new(), &[], &[], 0, None, None)
            .expect("save2");
        assert!(store.load("hr2").unwrap().unwrap().human_axes.is_none());
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
            .save("room1", &personas, &persona_meta, &history, &topics, 42, None, None)
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

    /// list_rooms: 저장된 모든 방을 메타와 함께 반환(concluded/report_count 포함).
    #[test]
    fn list_rooms_returns_all_with_meta() {
        let store = make_store();
        let personas = vec![make_persona("aria", "Aria", 0.8)];
        let meta = BTreeMap::new();
        let history = vec![make_event(0.0, "aria", 0.5, Some("hi"))];

        // room1: 진행 중(report None, 리포트 0개)
        store
            .save("room1", &personas, &meta, &history, &["러스트".to_string()], 1, None, None)
            .expect("save room1");
        // room2: 종료(report Some) + 단계 리포트 2개
        store
            .save("room2", &personas, &meta, &history, &["민초".to_string()], 1, None, Some("종료 리포트"))
            .expect("save room2");
        store.append_report("room2", "민초", "# md", "결론1").expect("rep1");
        store.append_report("room2", "민초", "# md", "결론2").expect("rep2");

        let rooms = store.list_rooms().expect("list_rooms 성공");
        assert_eq!(rooms.len(), 2);

        let r1 = rooms.iter().find(|r| r.room_id == "room1").expect("room1 있어야");
        assert_eq!(r1.topics, vec!["러스트".to_string()]);
        assert!(!r1.concluded);
        assert_eq!(r1.report_count, 0);

        let r2 = rooms.iter().find(|r| r.room_id == "room2").expect("room2 있어야");
        assert_eq!(r2.topics, vec!["민초".to_string()]);
        assert!(r2.concluded);
        assert_eq!(r2.report_count, 2);
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
            .save("room2", &personas, &persona_meta, &history, &[], 0, None, None)
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
            .save("room3", &personas, &persona_meta, &[], &[], 0, None, None)
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
                None,
                None,
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
                None,
                None,
            )
            .expect("save 2");

        let snap = store.load("room4").expect("load").expect("Some");
        assert_eq!(snap.tick_count, 20);
        assert_eq!(snap.topics, vec!["topic2".to_string()]);
        assert_eq!(snap.messages.len(), 2);
        assert_eq!(snap.messages[0].content, Some("두 번째 A".to_string()));
    }
}
