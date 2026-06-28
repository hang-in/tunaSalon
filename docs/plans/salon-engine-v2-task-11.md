---
title: "Salon v0.2 Task 11: Persona modifier (케미 비대칭)"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v2.md
task_id: "11"
depends_on: ["09", "10"]
parallel_group: "chemistry"
---

# Task 11 - Persona modifier: 쌍별 α 비대칭 (케미 collapse 방지)

plan `salon-engine-v2.md` subtask 11. task-10의 preset α는 모든 쌍이 동일(균일)이라 "모두가 똑같이 자극"하는 케미 collapse다. persona modifier가 쌍별 비대칭(α_pj ≠ α_jp)을 만든다. **단 전체를 target_rho로 재정규화해 안정(is_stable)을 유지한다.**

## Changed files

- `src/preset.rs` - 수정. modifier 기반 비대칭 α 빌더 추가.
- `src/model.rs` - 수정. `PersonaModifier { reactivity: f64, provocativeness: f64 }`(기본 1.0) + 모디파이어 맵 타입(`BTreeMap<PersonaId, PersonaModifier>`)이나 헬퍼. **Persona 구조체는 건드리지 않는다**(리터럴 churn 회피) - 모디파이어는 별도 사이드 맵.
- `src/main.rs` - 수정. 데모 모디파이어(쌍별 케미가 보이게)를 정의하고 `--room` 사용 시 적용.

## Change description

- `PersonaModifier`: `reactivity`(p가 남의 발화에 얼마나 자극받나) + `provocativeness`(j가 남을 얼마나 자극하나). 둘 다 기본 1.0.
- 비대칭 α 패턴: p≠j에 대해 `raw_pj = reactivity[p] * provocativeness[j]`(대각 0). reactivity/provocativeness가 페르소나마다 다르면 `raw_pj ≠ raw_jp`(비대칭). 모디파이어가 모두 1.0이면 raw가 균일 → task-10과 동일.
- **안정 재정규화**: raw 행렬의 분기 spectral radius `cur = branching_spectral_radius(raw, personas, beta)`를 구하고, `scale = target_rho / cur`(cur>0일 때)로 전체에 곱한다. 결과 spectral radius == target_rho < 1 → `is_stable` 항상 true. cur==0(모디파이어로 전부 0)이면 빈 행렬.
- preset에 `build_config_with_modifiers(personas, modifiers) -> EngineConfig` 추가. 기존 `build_config`(균일)는 `build_config_with_modifiers(personas, 전부 기본)`와 동일하도록(또는 그대로 두고 내부 공유).
- main: 데모 모디파이어를 정의(예: chaos = 도발성 높음, friend = 반응성 높음, summarizer = 둘 다 낮음)해 `--room` 시 비대칭 케미가 출력에 보이게. `--room` 없으면 α=0 경로 불변(모디파이어 미적용).
- 가드레일: 결정성 유지. `unwrap`/`panic` 금지. cur==0 분모 0 방지.

## Dependencies

- task-09(`branching_spectral_radius`, `is_stable`, `EngineConfig.alpha`), task-10(preset, target_rho, build_config).

## Verification

```bash
cargo test --lib preset
cargo test
cargo run -- --room pub --headless --seed 42 --ticks 6
```

- `cargo test --lib preset` (기존 + 신규 >=3): (1) 모디파이어가 다르면 `α_pj != α_jp`(비대칭 존재), (2) 비대칭 α도 `is_stable`이고 분기 spectral radius ≈ target_rho(1e-6), (3) 모디파이어가 전부 기본(1.0)이면 `build_config_with_modifiers` 결과가 `build_config`(균일)와 동일.
- `cargo test` 전체 green(α=0 골든 경로 불변, 회귀 없음).
- `cargo run -- --room pub ...`가 유효 NDJSON, 비대칭 케미 동작.

## Risks

| 위험 | 회피 |
|---|---|
| 재정규화로 안정 깨짐 | scale = target_rho/cur로 spectral radius를 target_rho에 고정 → 항상 <1. 테스트 2 |
| cur==0 분모 0 | cur<=0이면 빈 행렬 반환(자극 없음) |
| Persona 리터럴 churn | 모디파이어를 Persona에 안 넣고 사이드 맵으로. 기존 리터럴 불변 |
| α=0 경로 회귀 | `--room` 없으면 모디파이어 미적용, 골든 5종 재확인 |
| 비대칭이 출력에 안 보임 | 데모 모디파이어를 충분히 대비되게(도발/반응 차이 크게) |
