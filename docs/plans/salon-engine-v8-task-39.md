---
title: "Salon v0.8 Task 39: 메모리 스토어 + 회상 코어"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v8.md
task_id: "39"
depends_on: []
parallel_group: ""
---

# Task 39 - 메모리 스토어 + 회상 코어

plan `salon-engine-v8.md` subtask 39. friend engine의 검색 코어. 인메모리 사건/참여 + 키워드 회상(토큰 중복). **순수·결정적, 네트워크/rng/벽시계 없음.** 생성 배선은 task-41, 평가 하네스는 task-40. 이 task는 저장+검색 데이터 구조만.

## Changed files

- `src/memory.rs` - 신규. `MemoryEvent` + `MemoryStore`(record/join/recall/format_recall).
- `src/flow.rs` - (필요 시) 토큰화를 `pub(crate)`로 노출해 재사용. 안 되면 memory.rs에 동일 로직 복제.
- `src/lib.rs` - 수정. `pub mod memory;`.

## Change description

- `#[derive(Debug, Clone, PartialEq)] pub struct MemoryEvent { pub room: String, pub ts: u64, pub speaker: PersonaId, pub content: String }`. ts = 논리 타임스탬프(결정적).
- `pub struct MemoryStore { events: Vec<MemoryEvent>, participation: BTreeMap<String, BTreeSet<PersonaId>> }`:
  - `new()` / `Default`.
  - `pub fn join(&mut self, room: impl Into<String>, persona: impl Into<String>)`: 참여 등록(room → persona 집합).
  - `pub fn record(&mut self, event: MemoryEvent)`: 사건 추가. 화자를 해당 방 참여자로 자동 join(발화했으면 있었던 것).
  - `pub fn recall(&self, persona: &str, query: &str, k: usize) -> Vec<&MemoryEvent>`:
    1. persona가 참여한 방 집합(participation에서) 산출.
    2. 후보 = 그 방들의 사건만(**참여 격리** - 없던 방 사건 제외).
    3. 각 후보를 query와 토큰 중복으로 점수(flow와 동일 토큰화: 소문자+공백+구두점 trim, 집합 교집합 크기 또는 Jaccard). 자기 발화 제외는 선택(쿼리=현재 맥락이면 자기 과거도 회상 가능 - 일단 포함).
    4. 점수 내림차순 상위 k. **동점은 ts 내림차순(최신 우선)** 으로 결정적 정렬. 점수 0(겹침 없음)은 제외.
    5. `&MemoryEvent` 반환(입력 순서 아닌 점수순).
  - `pub fn format_recall(events: &[&MemoryEvent]) -> Option<String>`: 비면 None. 아니면 "지난 대화에서:\n- {speaker}: {content}\n..." 형태(회상 슬롯용). 논리 ts 기반 상대표현은 "지난 대화에서"로 시작(벽시계 없이).
- 토큰화: `flow.rs`의 토큰화 재사용(`pub(crate)` 노출) 또는 memory.rs에 동일 소형 로직.
- 결정성: BTreeMap/BTreeSet + 안정 정렬 + ts 동점 처리. rng/네트워크/벽시계 없음.
- 가드레일: unwrap/panic 금지. k=0이나 빈 스토어/미참여 → 빈 Vec/None.

## Dependencies

- v0.7. (flow.rs 토큰화 참고)

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test` green. 신규 `memory::tests`(순수):
  - (1) **참여 격리**: room A에 사건들, persona X는 A 참여 / Y는 B만 참여 → X.recall은 A 사건 후보, Y.recall은 A 사건 회상 안 함(없던 방).
  - (2) 토큰 회상: query가 한 사건 content와 토큰 겹치면 그 사건이 상위.
  - (3) 동점 ts 내림차순(최신 우선) 결정적.
  - (4) 빈 스토어/미참여 persona/겹침 0 → 빈 결과, format_recall None.
  - (5) 같은 스토어+쿼리 두 번 → 동일(결정성).
- **골든 5종 바이트 동일**(순수 모듈, 미배선).

## Risks

| 위험 | 회피 |
|---|---|
| 참여 격리 실패 | recall이 참여 방으로 먼저 좁힘. 격리 단위 테스트(필수) |
| 비결정 정렬 | 점수 동점 → ts 내림차순. BTreeMap/Set 순회. 안정 정렬 |
| 토큰화 중복/드리프트 | flow 토큰화 재사용(pub(crate)). 불가 시 동일 로직 복제 + 주석 |
| 0-division(빈 합집합) | 겹침 0 사건 제외. 빈 결과 None |
