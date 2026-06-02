---
title: "Salon v0.2 Task 09: α 활성 + 안정 조건"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v2.md
task_id: "09"
depends_on: []
parallel_group: ""
---

# Task 09 - 교차 자극 α 활성 + 안정 조건 (spectral radius < 1)

plan `salon-engine-v2.md` subtask 09. v0.1의 자기 억제 위에 **교차 자극**을 얹는다: 한 페르소나의 발화가 α 행렬을 통해 다른 페르소나의 강도를 끌어올린다. 폭주를 막는 안정 조건(분기행렬 spectral radius < 1) 유틸도 같이 만든다. **α=0이면 v0.1과 바이트 동일해야 한다(INV-3).**

## Changed files

- `src/model.rs` - 수정. `EngineConfig`에 `alpha: CouplingMatrix` 추가(정적 튜닝값). `EngineState`에 `excitations: BTreeMap<PersonaId, f64>` 추가(동적 누적기, E_p). `CouplingMatrix`에 빈 생성(`Default`/`new`)과 조회 헬퍼(`get(p, j) -> f64`, 없으면 0.0).
- `src/hawkes.rs` - 수정. 교차 자극 함수들 + 안정 조건 유틸 추가. 기존 `update_intensities`/`suppressed_after_speak`는 그대로(base 담당).
- `src/driver.rs` - 수정. 틱 루프에 excitation 갱신을 끼움. `config.alpha` 사용.
- `src/main.rs`, `src/sweep.rs`, `tests/smoke.rs` - 수정. `EngineConfig` 리터럴에 `alpha: CouplingMatrix::default()` 추가(전부 α=0, 프리셋은 task-10).

## Change description

설계 §2(Hawkes α)를 v0.1 위에 더한다. **base(μ 회복 + 억제)는 v0.1 그대로 두고, 교차 자극 항을 분리해서 더한다** — 그래야 α=0이면 base만 남아 v0.1과 동일하다.

- 모델: α는 정적이라 `EngineConfig.alpha`(CouplingMatrix, `alpha[p][j]` = 페르소나 j의 발화가 p를 자극하는 세기). 누적 자극 E_p는 동적이라 `EngineState.excitations`(0에서 시작).
- 결합 강도: `λ_p = base_p + E_p`. 이 λ가 게이트·RRF·record.intensities에 쓰인다.
- hawkes.rs 추가:
  - `decay_excitations(excitations, elapsed_ticks, beta, tick_interval) -> BTreeMap`: `E_p *= exp(-beta * elapsed_ticks * tick_interval)` (지수 커널 κ). 이벤트 없으면 0으로 식는다.
  - `apply_excitation_on_speak(excitations, alpha, speaker, personas)`: 발화 시 모든 p에 대해 `E_p += alpha.get(p, speaker)`. (j=speaker가 모두를 자극.)
  - `combined_intensities(base, excitations, personas) -> BTreeMap`: `base_p + E_p`.
  - `branching_spectral_radius(alpha, personas, beta) -> f64`: 분기행렬 `B[p][j] = alpha[p][j] / beta`의 spectral radius(거듭제곱법; N 작음). 지수 Hawkes 안정 조건은 ∫κ = 1/β라 분기행렬 = α/β.
  - `is_stable(alpha, personas, beta) -> bool` = `branching_spectral_radius < 1`.
- driver.rs 틱 루프(v0.1 순서에 2·3 삽입):
  1. base = `update_intensities(...)` (v0.1 회복). `state.intensities = base`.
  2. `state.excitations = decay_excitations(state.excitations, 1, beta, tick_interval)`.
  3. `λ = combined_intensities(base, state.excitations, personas)`. 이 λ로 게이트·RRF.
  4. on speak chosen: base 억제(`state.intensities[chosen] = suppressed_after_speak(μ)`, v0.1 그대로) + `apply_excitation_on_speak(state.excitations, config.alpha, chosen, personas)`.
  5. `record.intensities = λ`(결합). emit.
- α=0(빈 CouplingMatrix): E_p가 늘 0 → λ = base → v0.1과 동일. self-excitation(대각 α_pp)도 행렬이 정하며 기본 0.
- 가드레일: 결정성 유지(벽시계·전역 RNG 없음, BTreeMap). `unwrap`/`panic` 금지. spectral radius는 음수/NaN 안 나오게.

## Dependencies

- v0.1 전체(특히 `update_intensities`, `suppressed_after_speak`, driver, sink).

## Verification

```bash
cargo test --lib hawkes
cargo test --lib driver
cargo test
cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 > /tmp/v2_check.ndjson
```

- `cargo test --lib hawkes` (5개 이상): (1) α[p][j]>0면 j 발화 후 p의 결합 λ가 base보다 오른다, (2) 이벤트 없으면 E_p가 0으로 단조 감쇠, (3) **α=0이면 결합 λ가 base(v0.1)와 동일**, (4) 알려진 2x2 α에서 `branching_spectral_radius`가 손계산과 일치하고 `is_stable` 경계(<1 true, ≥1 false), (5) 안정 α는 반복 자기자극에도 λ 유한, 불안정 α(분기 spectral radius>1)는 λ가 증가(발작 모드 재현).
- `cargo test` 전체 green(α=0 기본이라 기존 27개 유지).
- **α=0 골든 회귀**: 위 출력이 v0.1 골든과 바이트 동일해야 한다(리뷰어가 `diff`로 확인). 엔진을 바꿔도 α=0이면 출력 불변.

## Risks

| 위험 | 회피 |
|---|---|
| α=0인데 출력이 미세하게 달라짐(부동소수 연산 순서 바뀜) | base 경로를 v0.1과 동일 연산 순서로 유지, E_p는 0일 때 더해도 무영향. 골든 diff로 강제 |
| 분기행렬 조건 오해(α<β vs α/β<1) | 지수 커널 ∫κ=1/β → 분기행렬=α/β, spectral radius<1. 테스트 4로 고정 |
| 거듭제곱법 비수렴/NaN | 작은 N이라 반복 횟수 상한 + 정규화. 비음수 행렬이라 Perron 고유값 수렴 |
| EngineConfig 필드 추가로 리터럴 전부 깨짐 | 모든 생성처에 `alpha: CouplingMatrix::default()` 추가(기계적), 빌드로 확인 |
| self-excitation과 억제 충돌 | base 억제(v0.1)와 E_p 자극을 분리. 대각 α는 기본 0, 켜면 행렬이 책임 |
