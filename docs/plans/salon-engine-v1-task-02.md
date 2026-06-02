---
title: "Salon v0.1 Task 02: HawkesEngine (μ, self-decay)"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v1.md
task_id: "02"
depends_on: ["01"]
parallel_group: "engine-core"
---

# Task 02 - HawkesEngine (μ + self-decay)

plan `salon-engine-v1.md` v0.1 작업 항목 2. 발화 강도 λ_p(t)를 계산한다. v0.1은 교차 자극(α)을 끄므로 μ + 자기 억제 회복만으로 움직인다.

## Changed files

- `src/hawkes.rs` - 신규. 강도 갱신 로직.
- `src/lib.rs` - 수정. `pub mod hawkes;` 한 줄 추가(additive).

## Change description

plan §2 v0.1 틱 루프 1번 "강도 갱신"을 구현한다. 설계 문서 2절의 λ_p(t) 식에서 α 항을 0으로 둔 형태.

- 경과 시간(Δt = 경과 틱 수 × tick_interval)만큼 각 페르소나의 λ를 μ_p 쪽으로 회복시킨다. 지수 회복: `λ ← μ + (λ_prev − μ) · exp(−β·Δt)`. β가 클수록 빨리 μ로 수렴.
- 발화 직후 억제(self-decay): **1회성**이다. `suppressed_after_speak(base_rate)` 헬퍼가 억제값(μ 아래)을 돌려주고, 드라이버(task-06)가 발화 시점에 그 페르소나의 저장 강도에 한 번만 적용한다. 이후 틱은 위 회복식으로 μ 쪽으로 복귀한다. `update_intensities`는 `last_speaker`로 억제를 재적용하면 안 된다 — 매 틱 재적용하면 마지막 화자가 μ 아래(~0.65μ)에 고착되어 설계 §3-1의 idle 회복(침묵이 길어지면 base rate가 강도를 밀어올리는 동역학)이 깨진다.
- 교차 자극 없음. 한 페르소나의 발화가 다른 페르소나 λ를 올리지 않는다(α=0). 따라서 v0.1에선 Hawkes 폭주가 구조적으로 불가능.
- 순수 함수 지향: `(EngineState, elapsed_ticks, &EngineConfig, &[Persona]) → 갱신된 intensities`. 부수효과/전역 상태/벽시계 사용 금지. 같은 입력이면 같은 출력.

## Dependencies

- task-01 (EngineConfig, Persona, EngineState 타입).

## Verification

```bash
cargo test --lib hawkes
```

- `cargo test --lib hawkes` exit 0. 포함 테스트(3개 이상 통과 보고):
  1. 충분히 회복시키면 μ가 높은 페르소나의 정상상태 λ가 더 높다.
  2. 발화 직후 λ가 떨어졌다가 이후 틱에서 μ 쪽으로 단조 회복한다.
  3. 같은 (state, elapsed, config) 입력에 대해 두 번 호출 결과가 비트 단위로 동일하다(결정성).

## Risks

| 위험 | 회피 |
|---|---|
| 회복식과 억제가 섞여 비단조 진동 | 억제는 발화 틱에만 1회 적용, 회복은 매 틱 지수식. 테스트 2로 단조 회복 확인 |
| β=0 또는 음수로 발산 | EngineConfig 검증 또는 호출부 가드. v0.1은 β>0 가정, 경계 테스트 |
| 부동소수 누적 오차로 비결정 | 동일 연산 순서 유지, 전역 RNG/시간 미사용. 테스트 3으로 결정성 고정 |
| lib.rs 공유 수정 | `pub mod hawkes;` 한 줄 추가만. 다른 줄 건드리지 않음 |
