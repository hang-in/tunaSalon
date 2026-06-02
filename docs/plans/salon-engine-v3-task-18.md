---
title: "Salon v0.3 Task 18: 내용 기반 RRF (관심도·잔향)"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v3.md
task_id: "18"
depends_on: ["15"]
parallel_group: ""
---

# Task 18 - 내용 기반 RRF 신호 (관심도·잔향)

plan `salon-engine-v3.md` subtask 18. Event.content가 생겼으니 RRF에 내용 신호 2개를 더한다. **핵심 불변식: content가 없으면(FakeBackend) RRF는 정확히 기존 3신호로 동작해 골든 바이트 동일.** content가 있을 때만 신호가 추가된다.

## Changed files

- `src/rrf.rs` - 수정. select에 내용 신호(관심도·잔향)를 조건부로 추가.

## Change description

- 현재 select는 3신호(intensity, balance, random)를 고정 융합한다. 이를 **동적 신호 리스트**로 바꾼다.
- **골든 보존 규칙**: history(또는 직전 발화)에 content가 하나도 없으면 신호 리스트 = 기존 3신호 그대로 → 융합 결과·tie-break가 v0.1과 비트 단위 동일. content 신호는 **추가만** 하지, 3신호 계산·순서는 절대 안 바꾼다.
- content가 있을 때만 추가하는 신호(각각 후보를 순위화):
  - **관심도(interest)**: 가장 최근 content(직전 발화 텍스트)가 어떤 후보의 이름/id를 (대소문자 무시) 포함하면 그 후보가 더 높은 순위(말 걸린 사람이 반응). 포함 없으면 후보 간 중립(동순위 → PersonaId tie-break).
  - **잔향(echo)**: 어떤 후보의 직전 자기 발화 content가 최근 content와 단어(공백 토큰, 길이 3+ 정도) 하나라도 겹치면 더 높은 순위(자기 화제 이어가기). 없으면 중립.
- 신호 추가 판정: `history.iter().any(|e| e.content.is_some())`가 false면 content 신호 미추가(= 기존 3신호). true면 추가. FakeBackend는 항상 content None → 영원히 3신호 → 골든 보존.
- 결정성: 내용 신호도 순수(문자열 매칭만, rng·시간 안 씀). 같은 입력 → 같은 순위.
- 가드레일: `unwrap`/`panic` 금지. rrf의 RNG 주입·tie-break 규칙 유지.

## Dependencies

- task-15(Event.content). rrf(v0.1).

## Verification

```bash
cargo test --lib rrf
cargo test
cargo run -- --headless --seed 42 --ticks 80 --theta 0.65   # content 없음 → 골든 동일
```

- `cargo test --lib rrf` (기존 + 신규 >=2): (1) content 없는 history면 select 결과가 3신호 기존 동작과 동일(회귀 테스트), (2) 관심도: 최근 content가 후보 A 이름을 포함하면 동률 상황에서 A가 더 자주/우선 선택, (3) 잔향: 후보의 과거 content가 최근 content와 단어 겹치면 우선.
- `cargo test` 전체 green(58→ +신규, 기존 유지).
- **α=0 + FakeBackend 골든 5종 바이트 동일**(content 영원히 없음 → 3신호 → 불변). 빌드 후 명시적 순차 diff.

## Risks

| 위험 | 회피 |
|---|---|
| content 신호가 3신호 path를 건드려 골든 깨짐 | content 없으면 신호 리스트 = 기존 3개 그대로(추가 0). 3신호 계산·tie-break 불변. 골든 5종 필수 재확인 |
| 내용 신호 비결정 | 문자열 매칭만(rng·시간 없음). 결정적 |
| 관심도/잔향이 너무 둔감/예민 | 단순 substring/word-overlap로 시작, v0.x에서 임베딩(설계 §)으로 고도화 |
| RNG 소비 순서 변화로 골든 깨짐 | random 신호의 rng 사용 위치/횟수를 기존과 동일 유지. content 신호는 rng 미사용 |
