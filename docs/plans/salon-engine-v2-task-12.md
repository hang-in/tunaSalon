---
title: "Salon v0.2 Task 12: FSM 전이 제약"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v2.md
task_id: "12"
depends_on: ["09"]
parallel_group: ""
---

# Task 12 - FSM 전이 제약 (같은 페르소나 2연속 금지)

plan `salon-engine-v2.md` subtask 12. RRF 위에 금지 전이를 얹는다. v0.2의 핵심 규칙은 "같은 페르소나 2연속 발화 금지". **기본 OFF로 두어 α=0 골든 경로를 보존**하고(`--fsm` 토글), α가 강한 구간(발화 후에도 λ가 θ 위에 남는 경우)에서 의미를 가진다.

## Changed files

- `src/model.rs` - 수정. `EngineConfig`에 `forbid_self_repeat: bool` 추가(기본 false). 모든 EngineConfig 리터럴에 `forbid_self_repeat: false` 추가(기계적).
- `src/driver.rs` - 수정. gate 후보에서 FSM 필터 적용.
- `src/main.rs` - 수정. `--fsm` 플래그로 `forbid_self_repeat = true`. usage 갱신.
- `src/preset.rs` - 수정(필요 시). `build_config(_with_modifiers)`가 `forbid_self_repeat: false`로 설정.

## Change description

설계 §3-3. 화자 선택 결과를 전이 제약으로 필터링한다.

- `EngineConfig.forbid_self_repeat: bool`. 기본 false.
- driver 틱 루프: gate가 `Candidates(c)`를 주면, `config.forbid_self_repeat == true && state.last_speaker == Some(x)`일 때 후보에서 x를 제거한다. 그 다음 RRF로 선택.
  - 필터 후 후보가 **비면 그 틱은 침묵**으로 처리한다(gate_passed=false, chosen=None). 이건 에러를 숨기는 silent fallback이 아니라 "강제 화자가 연속 불가 + 다른 후보 없음 → 침묵"의 정상 동작이다. 명시적으로 silence_count 증가.
- `--fsm`이 있으면 `forbid_self_repeat = true`. preset/모디파이어와 독립적인 토글(예: `--room chaos --fsm`).
- 기본 false면 driver 동작이 v0.1/task-11과 완전히 동일 → α=0 골든 5종 불변.
- 가드레일: 결정성 유지. `unwrap`/`panic` 금지.

## Dependencies

- task-09(EngineConfig). driver/gate/rrf(v0.1).

## Verification

```bash
cargo test --lib driver
cargo test
cargo run -- --room chaos --fsm --headless --seed 42 --ticks 20
```

- `cargo test --lib driver` (기존 + 신규 >=1): forbid_self_repeat=true로 driver를 돌리면 **연속 두 record의 chosen이 같은 경우가 없다**(self-repeat 금지). forbid_self_repeat=false면 기존과 동일.
- `cargo test` 전체 green(기본 false라 기존 42 유지).
- `--room chaos --fsm` 출력에서 같은 화자 2연속이 안 나오는지 육안 확인. **α=0 골든 5종 회귀 없음**(빌드 후 명시적 순차 실행으로 검증).

## Risks

| 위험 | 회피 |
|---|---|
| FSM ON이 기본이 되어 골든 깨짐 | 기본 false, `--fsm`로만 활성. 골든 5종 재확인 |
| 필터로 후보 고갈 시 동작 모호 | 빈 후보 = 침묵으로 명시(silent fallback 아님, 카운트 증가) |
| EngineConfig 리터럴 churn | 모든 생성처에 `forbid_self_repeat: false` 추가, 빌드로 확인 |
| last_speaker 의미 변화 | last_speaker는 이미 v0.1에 존재. FSM은 읽기만, 변경 안 함 |
