---
title: "Salon v0.2 Task 14: 스모크 게이트 v0.2"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v2.md
task_id: "14"
depends_on: ["10", "11", "12"]
parallel_group: ""
---

# Task 14 - v0.2 스모크 게이트

plan `salon-engine-v2.md` subtask 14. v0.2 완료 기준(plan §5)을 라이브러리 API로 자동 검증한다. v0.1 스모크(`tests/smoke.rs`)와 별도 파일 `tests/smoke_v2.rs`에 둔다.

## Changed files

- `tests/smoke_v2.rs` - 신규. v0.2 완료 기준 단언(driver + VecSink + preset + rrf, stdout 파싱 없이 API 직접 호출).

## Change description

plan §5 다섯 기준을 단언한다. 고정 seed, 부등호/방향/분산>0 형태(정확값 금지).

데모 3인(friend 0.80, chaos 0.70, summarizer 0.25). 모디파이어는 task-11 데모와 유사한 대비값 사용.

1. **케미**: 같은 μ·같은 preset(예 Pub)에서 persona modifier를 서로 다르게 두 번 구성해 같은 seed로 돌리면, 화자 전이 시퀀스(또는 인접쌍 분포)가 달라진다. `chosen` 시퀀스가 두 모디파이어에서 다름을 단언.
2. **안정**: 안정 preset(예 Pub, ρ=0.4)으로 충분히 길게(예 300틱) 돌렸을 때 record.intensities의 최댓값이 유한·유계(예 < 어떤 상한, 발산 없음). `is_stable`도 true.
3. **길이/seed 분포(이관 기준 4)**: α 활성 preset에서 여러 seed(예 0..12)로 돌려 발화 수(speak_count)나 화자 분포가 seed에 따라 갈린다(분산 > 0). α가 동시 후보 경쟁을 만들어 seed 민감도가 생김을 확인.
4. **토글(α=0 ≡ v0.1)**: 빈 CouplingMatrix(α=0) + forbid_self_repeat=false config로 driver를 돌린 결과가, v0.1 동등 baseline(같은 β/θ/k, α 없음)과 동일. (골든은 헤드리스 레벨에서 이미 검증되나, 여기선 API 레벨로 한 번 더.)
5. **FSM**: forbid_self_repeat=true면 인접 두 발화의 chosen이 같지 않다.

## Dependencies

- task-10(preset), task-11(PersonaModifier, build_config_with_modifiers), task-12(forbid_self_repeat). task-09 유틸. dev-deps의 rand/rand_chacha.

## Verification

```bash
cargo test --test smoke_v2
cargo test
```

- `cargo test --test smoke_v2` (5개 이상): 위 다섯 기준 전부 통과. **v0.2 완료 게이트.**
- `cargo test` 전체 green(v0.1 스모크 4 + v0.2 스모크 + 나머지 유지).

## Risks

| 위험 | 회피 |
|---|---|
| 기준이 seed/구현에 민감해 flaky | 부등호·방향·분산>0만 단언. 대비 큰 모디파이어/preset 선택 |
| 케미 차이가 미미해 시퀀스 동일 | 모디파이어 대비를 크게(도발/반응 차이 크게), 충분한 틱 |
| 기준 3 분포가 약함 | α 강한 preset(argument/chaos)에서 측정, 충분한 seed 수 |
| 안정 상한 임계가 자의적 | "발산 안 함"을 보수적 상한(예 base+α누적 이론 상한 근처)으로, 또는 유한성(is_finite) + 단조 증가 아님으로 |
