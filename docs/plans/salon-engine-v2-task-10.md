---
title: "Salon v0.2 Task 10: Room preset"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v2.md
task_id: "10"
depends_on: ["09"]
parallel_group: "chemistry"
---

# Task 10 - Room preset (calm / pub / argument / chaos)

plan `salon-engine-v2.md` subtask 10. 방 분위기 프리셋이 β/θ + 전역 α 성격을 한 번에 세팅한다. α는 task-09의 `is_stable`을 만족하도록 **구성 단계에서 안정**해야 한다(폭주 금지, INV-2).

## Changed files

- `src/preset.rs` - 신규. `RoomPreset` enum + config 빌더.
- `src/lib.rs` - 수정. `pub mod preset;`.
- `src/main.rs` - 수정. `--room <calm|pub|argument|chaos>` 플래그. preset이 β/θ/α를 세팅하고, 이후 명시적 `--theta`/`--k`/`--beta`가 있으면 그 값으로 덮어쓴다.

## Change description

plan §3 v0.2 표(calm/pub/argument/chaos)를 구현한다.

- `RoomPreset { Calm, Pub, Argument, Chaos }`.
- 각 preset은 `(beta, theta, target_rho, mu_scale)`를 정한다. `target_rho`는 분기행렬 목표 spectral radius(< 1)로 "α 성격"을 한 손잡이로 표현한다.
  | preset | beta | theta | target_rho | mu_scale | 성격 |
  |---|---|---|---|---|---|
  | Calm | 0.8 | 0.70 | 0.10 | 0.8 | 조용·자극 약 |
  | Pub | 0.5 | 0.50 | 0.40 | 1.0 | 적당 |
  | Argument | 0.3 | 0.38 | 0.80 | 1.0 | 낮은 θ·강한 자극 |
  | Chaos | 0.3 | 0.35 | 0.92 | 1.0 | 거의 한계까지 자극 |
- **α 역산(안정 보장)**: 균일 off-diagonal α 행렬(대각=0, 모든 p≠j 쌍 = α_base)의 분기 spectral radius = `α_base·(N−1)/beta`. 목표 ρ를 맞추려면 `α_base = target_rho · beta / (N − 1)`. 이렇게 만들면 모든 preset이 `is_stable`(ρ<1)을 만족한다. N은 페르소나 수.
- `RoomPreset::build_config(personas) -> EngineConfig`: 위 (beta, theta) + k(기본 60.0) + `alpha`(역산한 균일 행렬). mu_scale은 personas의 base_rate에 곱해 적용하거나(별도 함수) 일단 EngineConfig엔 안 들어가니, μ 반영은 main에서 personas의 base_rate에 곱한다.
- main: `--room`이 있으면 `build_config`로 config를 만들고 personas의 μ에 mu_scale 적용. 그 다음 `--theta/--k/--beta`가 명시됐으면 해당 필드만 덮어쓴다(우선순위: 명시 플래그 > preset > 기본).
- chaos의 "랜덤 μ"는 v0.2 task-10에선 보류(mu_scale 고정). 결정성 유지.
- 가드레일: `unwrap`/`panic` 금지. preset 파싱 실패 시 명확한 에러.

## Dependencies

- task-09 (`CouplingMatrix`, `is_stable`, `branching_spectral_radius`, `EngineConfig.alpha`).

## Verification

```bash
cargo test --lib preset
cargo test
cargo run -- --room argument --headless --seed 42 --ticks 5
```

- `cargo test --lib preset` (4개 이상): (1) 네 preset 모두 `is_stable(config.alpha, personas, config.beta)` == true, (2) `branching_spectral_radius`가 각 preset의 target_rho와 근사 일치(1e-6), (3) α 행렬이 균일 off-diagonal·대각 0, (4) Argument의 θ < Calm의 θ, Argument의 ρ > Calm의 ρ(성격 순서).
- `cargo test` 전체 green(32개 유지, α=0 기본 경로 불변).
- `cargo run -- --room argument --headless ...`가 유효 NDJSON 출력(α 활성 경로 동작).

## Risks

| 위험 | 회피 |
|---|---|
| preset α가 폭주(ρ≥1) | target_rho<1에서 α_base 역산 → 구성상 안정. 테스트 1로 강제 |
| N에 따라 ρ 어긋남 | α_base = ρ·β/(N−1)로 N 반영. 테스트 2로 확인 |
| 플래그 우선순위 혼란 | 명시 > preset > 기본 순서 고정, 문서화 |
| μ 반영 위치 모호 | mu_scale은 personas base_rate에 곱(main), EngineConfig엔 β/θ/k/α만 |
