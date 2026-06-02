---
title: "Salon v0.7 Task 36: MetaController 코어 (수렴→cooling)"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v7.md
task_id: "36"
depends_on: []
parallel_group: ""
---

# Task 36 - MetaController 코어

plan `salon-engine-v7.md` subtask 36. FlowMeter 수렴도 → μ 식히기 비율(mu_scale)을 계산하는 휴리스틱. **순수·결정적·유계.** 배선(driver/live)은 task-37. 이 task는 cooling 함수만.

## Changed files

- `src/meta.rs` - 신규. `MetaController` + `cooling`.
- `src/lib.rs` - 수정. `pub mod meta;`.

## Change description

- `pub struct MetaController { gain: f64, threshold: f64, floor: f64 }`.
  - `gain`: 식히기 강도(클수록 수렴 시 μ 많이 낮춤). **기본 약하게**(예 0.6). `threshold`: 이 수렴도 넘어야 식히기 시작(예 0.5). `floor`: mu_scale 하한(예 0.4 - 완전 침묵/고착 방지).
  - `pub fn new(gain, threshold, floor)` + `default()`(약한 기본값) + `from_env()`(`SALON_META_GAIN` 있으면 gain 대체, 나머지 기본).
- `pub fn cooling(&self, flow: Option<crate::flow::FlowMetric>) -> f64` → mu_scale ∈ [floor, 1.0]:
  - `flow == None` → **1.0**(content 없음 = no-op, 골든 보존의 핵심).
  - `conv = flow.convergence`. `conv <= threshold` → 1.0(아직 식힐 만큼 안 수렴).
  - else: `overshoot = (conv - threshold) / (1 - threshold)`(∈[0,1] 정규화) → `mu_scale = (1.0 - gain * overshoot).clamp(floor, 1.0)`.
  - 즉 conv=threshold→1.0, conv=1.0→`(1-gain)` clamp floor. **단조 감소**(conv↑ → mu_scale↓), **유계**[floor,1].
  - `gain == 0.0`이면 항상 1.0(비활성).
  - 순수: flow·필드만. rng·네트워크·시간 없음.
- 가드레일: clamp로 유계 보장. unwrap/panic 금지. 1-threshold가 0이면(threshold=1) 0-division 방어(그 경우 conv>threshold 불가 → 1.0).

## Dependencies

- task-33(flow.rs FlowMetric). v0.6.

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test` green. 신규 `meta::tests`(순수):
  - (1) flow None → 1.0.
  - (2) conv ≤ threshold → 1.0.
  - (3) conv↑ → mu_scale 단조 감소(예 0.6 < 0.8 < 1.0의 mu_scale가 ≥ 순).
  - (4) conv=1.0에서 mu_scale ≥ floor(하한 보장).
  - (5) gain=0 → 항상 1.0(비활성).
  - (6) 손계산(예 gain 0.6, threshold 0.5, conv 0.75 → overshoot 0.5 → mu_scale 1-0.3=0.7).
- **골든 5종 바이트 동일**(순수 모듈, 미배선).

## Risks

| 위험 | 회피 |
|---|---|
| 유계 깨짐(mu_scale 음수/>1) | clamp(floor, 1.0). 단위 테스트로 경계 |
| threshold=1 0-division | 분모 (1-threshold)=0 방어(그땐 cooling 없음=1.0) |
| 게인 감각 | 약한 기본 + SALON_META_GAIN. 진동은 task-38 라이브 관찰 |
| 비결정 유입 | flow·gain만. rng 없음 |
