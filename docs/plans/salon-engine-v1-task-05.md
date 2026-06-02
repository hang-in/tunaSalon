---
title: "Salon v0.1 Task 05: fake utterance generator"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v1.md
task_id: "05"
depends_on: ["01"]
parallel_group: "engine-core"
---

# Task 05 - fake utterance generator

plan `salon-engine-v1.md` v0.1 작업 항목 5. LLM 없이 라벨만 있는 가짜 발화를 만든다. v0.1은 발화 리듬만 검증하므로 내용은 필요 없다.

## Changed files

- `src/utterance.rs` - 신규. fake 발화 생성.
- `src/lib.rs` - 수정. `pub mod utterance;` 한 줄 추가(additive).

## Change description

plan §2 틱 루프 4번 "fake 발화".

- 선택된 페르소나 id + 현재 tick 으로 라벨 발화를 만든다. 실제 텍스트 내용 없음.
- 결과는 task-01의 `Event`(ts, speaker, mark)로 표현. mark는 v0.1에서 고정값(예: 1.0) 또는 설정값.
- 선택적 화제 태그: 플래그가 켜지면 임의 화제 태그(고정 목록에서 시드 RNG로 선택)를 붙여 v0.3 관심도 신호를 미리 흉내. 기본은 꺼짐. 태그 선택도 주입 RNG 사용(결정성).
- LLM 호출/네트워크/파일 IO 없음. 순수 + 주입 RNG.

## Dependencies

- task-01 (Event, PersonaId, RNG 타입).

## Verification

```bash
cargo test --lib utterance
```

- `cargo test --lib utterance` exit 0. 포함 테스트(2개 이상 통과 보고):
  1. 주어진 speaker/tick으로 Event를 만들고 speaker/ts가 일치.
  2. 화제 태그 플래그 on/off가 동작하고, 같은 시드면 태그 선택이 동일(결정성).

## Risks

| 위험 | 회피 |
|---|---|
| 화제 태그가 v0.3 설계를 앞당겨 과설계 | 고정 목록 + 단순 선택만. 기본 off. v0.1 검증엔 불필요한 옵션임을 주석에 명시 |
| 태그 RNG가 전역이라 비결정 | 주입 RNG 사용 |
| lib.rs 공유 수정 | `pub mod utterance;` 한 줄 추가만 |
