---
title: "Salon v0.6 Task 34: FlowMeter 배선 (record/live) + 골든 보존"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v6.md
task_id: "34"
depends_on: ["33"]
parallel_group: ""
---

# Task 34 - FlowMeter 배선 + 골든 보존

plan `salon-engine-v6.md` subtask 34. task-33의 `flow` 측정을 ObservationRecord(헤드리스)와 LiveSession(채팅)에 연결한다. **핵심 불변식: content 없으면(FakeBackend) flow=None → serde 생략 → 골든 바이트 동일**(v0.3 utterance 패턴 그대로). 관찰만 - 엔진 결정/파라미터 불변.

## Changed files

- `src/sink.rs` - 수정. `ObservationRecord`에 `flow: Option<FlowMetric>` 추가(**구조체 맨 끝**), `#[serde(default, skip_serializing_if = "Option::is_none")]`. 모든 record 리터럴에 `flow: None`/계산값 추가(기계적).
- `src/driver.rs` - 수정. record 빌드 시 최근 N개 **content 발화**로 flow 계산해 채움. content 없으면 None.
- `src/live.rs` - 수정. `LiveSession`에 `pub fn flow(&self) -> Option<FlowMetric>`(history content로 measure). 채팅 TUI(task-35)가 사용.
- (필요 시) 테스트들의 ObservationRecord 리터럴에 `flow: None` 추가(driver/tui/sink/smoke 테스트).

## Change description

- `ObservationRecord.flow: Option<FlowMetric>` - 맨 끝 필드 + `#[serde(default, skip_serializing_if = "Option::is_none")]`. None이면 NDJSON에서 완전 생략 → **FakeBackend(content 전부 None) 기록은 직렬화 바이트 동일**.
- flow 계산 헬퍼: history에서 content 있는 발화만 추출 → 최근 N개(예 `const FLOW_WINDOW = 6`) → `src/flow.rs`의 공개 measure 함수 호출. content 발화 2개 미만이면 None.
  - 매 틱 record 빌드 시 계산(침묵 틱도 최근 content 기준). FakeBackend는 content가 항상 None이라 추출 결과 빈 슬라이스 → measure → None.
- `LiveSession::flow()`: 동일 방식으로 `self.state.history`의 content에서 measure. 채팅 사이드바 게이지용(task-35).
- **관찰만(INV-2)**: flow는 record/표시 전용. 엔진 선택·강도·파라미터에 절대 영향 없음. rrf/gate/hawkes 호출에 flow 안 들어감.
- 토글(INV-4): v0.6은 **content 게이팅이 곧 암묵 토글**(FakeBackend면 자동 None). 명시 config 플래그는 EngineConfig churn 회피 위해 보류(필요 시 이후).
- 가드레일: 골든 보존이 최우선. 필드는 맨 끝 + skip 생략. unwrap/panic 금지.

## Dependencies

- task-33(flow.rs measure, FlowMetric).

## Verification

```bash
cargo build
cargo test
cargo build && for s in "42 120 0.40 s42_t040" "42 80 0.65 s42_t065" "42 120 0.78 s42_t078" "7 80 0.65 s7_t065" "99 80 0.65 s99_t065"; do :; done
# (golden은 명시적 순차로, zsh 워드분할 주의 - 아래처럼 한 줄씩)
cargo run -- --headless --seed 42 --ticks 120 --theta 0.40 | diff - /tmp/salon_golden/s42_t040.ndjson && echo s42_t040 OK
cargo run -- --headless --seed 42 --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
cargo run -- --headless --seed 42 --ticks 120 --theta 0.78 | diff - /tmp/salon_golden/s42_t078.ndjson && echo s42_t078 OK
cargo run -- --headless --seed 7   --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s7_t065.ndjson  && echo s7_t065 OK
cargo run -- --headless --seed 99  --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s99_t065.ndjson && echo s99_t065 OK
```

- `cargo test` green(record 리터럴 갱신 후). 신규 테스트: (1) content 없는 record는 NDJSON에 `flow` 키 없음, (2) content 있는 history로 driver/live가 flow 채움(stub content로 결정적), (3) `grep -c flow` on FakeBackend 헤드리스 출력 == 0.
- **골든 5종 바이트 동일**(핵심). flow는 FakeBackend에서 항상 None → 생략.
- 엔진 결정 불변(관찰만).

## Risks

| 위험 | 회피 |
|---|---|
| flow 필드로 골든 깨짐 | 맨 끝 필드 + skip_serializing_if None. FakeBackend content None → flow None. 골든 5종 재확인(필수) |
| 관찰이 엔진에 피드백됨 | flow는 record/표시 전용. hawkes/gate/rrf 입력에 불사용 |
| record 리터럴 churn으로 빌드 실패 | 모든 ObservationRecord 생성처에 flow 추가(driver/tests/smoke). 빌드로 확인 |
| 매 틱 measure 비용 | 윈도우 작음(N~6), 토큰 근사라 가벼움. content 없으면 즉시 None |
