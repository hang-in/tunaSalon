---
title: "Salon v0.1 Task 03: SilenceGate (θ)"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v1.md
task_id: "03"
depends_on: ["01"]
parallel_group: "engine-core"
---

# Task 03 - SilenceGate (θ)

plan `salon-engine-v1.md` v0.1 작업 항목 3. 이번 틱에 누군가 말할지(발화) 아무도 안 말할지(침묵)를 임계 θ로 판정한다.

## Changed files

- `src/gate.rs` - 신규. 게이트 판정.
- `src/lib.rs` - 수정. `pub mod gate;` 한 줄 추가(additive).

## Change description

설계 문서 3-1절 "침묵 게이트" + plan §2 틱 루프 2번. 순수 함수.

- 입력: 현재 intensities(map), `theta`(EngineConfig.theta).
- λ_p ≥ θ 인 페르소나들을 후보로 반환한다.
- 후보가 하나도 없으면 침묵 신호(빈 후보)를 반환한다. 호출부(틱 루프)는 이 틱에 아무도 말하지 않게 한다.
- 침묵이 이어지면 task-02 회복식이 λ를 서서히 밀어올려 결국 누군가 θ를 넘는다(idle 발화). 이 동역학은 게이트가 아니라 HawkesEngine 책임이며, 게이트는 매 틱 임계 비교만 한다.
- 반환 타입 예: `enum GateResult { Silent, Candidates(Vec<PersonaId>) }` 또는 빈 Vec = 침묵.

## Dependencies

- task-01 (EngineConfig, PersonaId, intensities 타입).

## Verification

```bash
cargo test --lib gate
```

- `cargo test --lib gate` exit 0. 포함 테스트(3개 이상 통과 보고):
  1. 모든 λ < θ ⇒ 침묵(빈 후보).
  2. 일부 λ ≥ θ ⇒ 그 페르소나들만 후보.
  3. λ == θ 경계 처리가 명세(≥)대로 동작.

## Risks

| 위험 | 회피 |
|---|---|
| 경계(λ==θ) 처리 모호 | ≥ 로 명세 고정, 경계 테스트로 박음 |
| 게이트가 회복/idle 발화 로직을 떠안아 책임 혼입 | 게이트는 임계 비교만. 회복은 task-02 책임(결합 분리) |
| lib.rs 공유 수정 | `pub mod gate;` 한 줄 추가만 |
