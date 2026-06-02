//! 메모리 스토어 + 회상 코어 (task-39).
//!
//! 참여 기반 기억: 캐릭터는 자신이 있었던 방의 사건만 회상할 수 있다.
//! 순수·결정적·인메모리. 네트워크/rng/벽시계 없음.
//! 생성 배선은 task-41. 평가 하네스는 task-40.

use std::collections::{BTreeMap, BTreeSet};

use crate::flow::tokenize;
use crate::model::PersonaId;

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
    ///
    /// 이미 등록되어 있으면 무시한다(멱등).
    pub fn join(&mut self, room: impl Into<String>, persona: impl Into<String>) {
        self.participation
            .entry(room.into())
            .or_default()
            .insert(persona.into());
    }

    /// 사건을 기록한다.
    ///
    /// 화자를 해당 방 참여자로 자동 join한다(발화했으면 그 방에 있었던 것).
    pub fn record(&mut self, event: MemoryEvent) {
        // 화자 자동 참여 등록
        self.participation
            .entry(event.room.clone())
            .or_default()
            .insert(event.speaker.clone());
        self.events.push(event);
    }

    /// `persona`의 과거 사건 중 `query`와 토큰 중복이 있는 것을 최대 `k`개 반환한다.
    ///
    /// 알고리즘:
    /// 1. `persona`가 참여한 방 집합을 구한다(participation 기반).
    /// 2. 후보 = 그 방들의 사건만(참여 격리 — 없던 방 사건 접근 불가).
    /// 3. 각 후보와 query 사이의 토큰 교집합 크기(intersection count)로 점수를 매긴다.
    ///    (flow.rs와 동일한 tokenize 사용: 소문자+공백+구두점 trim, BTreeSet)
    /// 4. 점수 0(겹침 없음)은 제외한다.
    /// 5. 점수 내림차순 → 동점은 ts 내림차순(최신 우선)으로 안정 정렬 후 상위 k 반환.
    ///
    /// k=0 / 빈 스토어 / 미참여 / 겹침 없음 → 빈 Vec.
    pub fn recall(&self, persona: &str, query: &str, k: usize) -> Vec<&MemoryEvent> {
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

        // query 토큰화 (한 번만)
        let query_tokens = tokenize(query);

        // 2-3. 참여한 방의 사건만 후보로 삼고 점수 계산
        let mut scored: Vec<(usize, &MemoryEvent)> = self
            .events
            .iter()
            .filter(|ev| rooms.contains(ev.room.as_str()))
            .filter_map(|ev| {
                let content_tokens = tokenize(&ev.content);
                let score = query_tokens.intersection(&content_tokens).count();
                // 4. 점수 0 제외
                if score == 0 {
                    None
                } else {
                    Some((score, ev))
                }
            })
            .collect();

        // 5. 점수 내림차순 → ts 내림차순(안정 정렬: sort_by는 stable)
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0) // 점수 내림차순
                .then_with(|| b.1.ts.cmp(&a.1.ts)) // 동점: ts 내림차순
        });

        scored.into_iter().take(k).map(|(_, ev)| ev).collect()
    }

    /// 회상 결과를 회상 슬롯용 문자열로 포맷한다.
    ///
    /// 비어 있으면 `None`. 있으면 `"지난 대화에서:\n- {speaker}: {content}\n..."`.
    /// 논리 ts 기반 상대표현("지난 대화에서")만 쓰며 벽시계 없음.
    pub fn format_recall(events: &[&MemoryEvent]) -> Option<String> {
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
}

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

    /// (6) format_recall: 사건들의 speaker/content가 문자열에 포함된다.
    #[test]
    fn format_recall_produces_correct_string() {
        let e1 = ev("room", 1, "alice", "안녕하세요");
        let e2 = ev("room", 2, "bob", "반갑습니다");
        let refs = vec![&e1, &e2];

        let output = MemoryStore::format_recall(&refs).expect("비어 있지 않으므로 Some이어야 한다");
        assert!(output.starts_with("지난 대화에서:"), "헤더로 시작해야 한다");
        assert!(output.contains("alice"), "alice가 포함되어야 한다");
        assert!(output.contains("안녕하세요"), "content가 포함되어야 한다");
        assert!(output.contains("bob"), "bob이 포함되어야 한다");
        assert!(output.contains("반갑습니다"), "content가 포함되어야 한다");
    }
}
