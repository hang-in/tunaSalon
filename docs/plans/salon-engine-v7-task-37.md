---
title: "Salon v0.7 Task 37: MetaController 배선 (driver/live μ 식히기) + 골든 보존"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v7.md
task_id: "37"
depends_on: ["36"]
parallel_group: ""
---

# Task 37 - MetaController 배선 + 골든 보존

plan `salon-engine-v7.md` subtask 37. **최위험**: 엔진 파라미터(μ)를 거시 신호(flow)로 처음 조정한다. 핵심 불변식: **flow None(FakeBackend) → mu_scale=1.0 → 강도 갱신 완전 동일 → 골든 바이트 동일.** 안정성: 약한 게인 + floor(task-36에서 보장).

## Changed files

- `src/hawkes.rs` - 수정. `update_intensities(state, elapsed, config, personas, mu_scale: f64)` 인자 추가. 회복 공식의 base_rate를 `base_rate * mu_scale`로(effective μ). 1.0이면 기존과 완전 동일. hawkes 테스트 호출에 1.0 추가.
- `src/driver.rs` - 수정. `MetaController`(from_env) 보유. 매 틱 **시작에 flow 계산**(history content) → `cooling(flow)` → mu_scale → `update_intensities(..., mu_scale)`. content 없으면 1.0. (record.flow는 task-34대로, 같은 flow 재사용 가능.)
- `src/live.rs` - 수정. LiveSession이 `MetaController` 보유. tick()에서 동일하게 flow→cooling→mu_scale 적용. `pub fn mu_scale(&self) -> f64`(또는 cooling 값) 접근자 - task-38 사이드바용.

## Change description

- `update_intensities` 회복 공식: 기존 `intensity = base_rate + (previous - base_rate)*decay` → `let mu = base_rate * mu_scale; intensity = mu + (previous - mu)*decay`. mu_scale<1이면 회복 목표가 낮아져 강도가 점차 내려감 → θ 통과 줄어 침묵↑ → 방 식음. **mu_scale=1.0이면 한 글자도 안 바뀜**(골든 보존).
- driver/live 틱 순서: (1) 현재 history content로 flow 계산, (2) `meta.cooling(flow)` → mu_scale, (3) `update_intensities(..., mu_scale)`, (4) decay/gate/rrf/speak(기존), (5) record(flow는 task-34). FakeBackend는 (1) flow None → (2) mu_scale 1.0 → (3) 강도 불변.
- MetaController는 `from_env()`(SALON_META_GAIN, 기본 약한 게인). driver/live가 1개 보유.
- **관찰만의 확장**: flow→μ 단방향. flow는 record/표시 전용이었으나 이제 cooling 입력으로도. 단 mu_scale=1.0 게이팅으로 LLM-off 경로는 완전 불변.
- mu_scale 표시(task-38): LiveSession::mu_scale() 접근자 추가(사이드바). record 필드 추가는 churn·골든위험 회피 위해 보류(채팅에서 관찰).
- 가드레일: **골든 보존 최우선**. mu_scale 1.0 경로가 바이트 동일임을 반드시 확인. update_intensities 호출처 전부 갱신. unwrap/panic 금지.

## Dependencies

- task-36(MetaController), task-33/34(flow, LiveSession::flow).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 120 --theta 0.40 | diff - /tmp/salon_golden/s42_t040.ndjson && echo s42_t040 OK
cargo run -- --headless --seed 42 --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
cargo run -- --headless --seed 42 --ticks 120 --theta 0.78 | diff - /tmp/salon_golden/s42_t078.ndjson && echo s42_t078 OK
cargo run -- --headless --seed 7   --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s7_t065.ndjson  && echo s7_t065 OK
cargo run -- --headless --seed 99  --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s99_t065.ndjson && echo s99_t065 OK
```

- `cargo test` green(update_intensities 호출 갱신 후). 신규 테스트: (1) mu_scale=1.0 update_intensities == 기존 동작(같은 입력 동일 결과), (2) mu_scale<1이면 회복 목표가 낮아짐(강도가 더 낮게 수렴), (3) driver/live가 FakeBackend(flow None)면 mu_scale 1.0 적용(record/동작 불변), (4) content-bearing stub history면 mu_scale<1 적용됨.
- **골든 5종 바이트 동일**(필수 - flow None → mu_scale 1.0 → 강도 불변). 이게 안 맞으면 배선 오류.

## Risks

| 위험 | 회피 |
|---|---|
| **골든 깨짐**(엔진 파라미터 피드백) | flow None → mu_scale 1.0 → update_intensities 완전 동일. clamp/곱셈만. 골든 5종 재확인(최우선) |
| 피드백 진동/고착 | 약한 게인 + floor(task-36). 라이브 관찰(task-38). headless는 무관(no-op) |
| update_intensities 시그니처 churn | driver/live/hawkes 테스트 호출에 mu_scale 추가(테스트는 1.0). 빌드 확인 |
| flow 이중 계산 비용 | 윈도우 작음. tick-start flow를 record와 공유 가능 |
| 비결정 유입 | mu_scale=1.0이면 완전 동일. cooling은 결정적. 비결정은 LLM content뿐 |
