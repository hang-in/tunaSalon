---
title: "Salon v0.5 Task 28: HumanChannel (엔진)"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v5.md
task_id: "28"
depends_on: []
parallel_group: ""
---

# Task 28 - HumanChannel (엔진 코어)

plan `salon-engine-v5.md` subtask 28. design §5의 "사람 참여"를 엔진 로직으로 구현한다. **순수 엔진, 네트워크/UI/rng 없음, 결정적 단위 테스트.** 사람 발화 = 큰 mark Hawkes 외부 이벤트 → history push + 전 페르소나 λ 강자극(관심 집중) + 기존 excitation 일부 감쇠(주목 = "일부 리셋"). 라이브 드라이버(task-29)가 이걸 호출한다. **headless 골든 경로 불침투**(새 모듈, `driver::run` 미사용).

## Changed files

- `src/human.rs` - 신규. `HumanChannel` + `speak()`.
- `src/lib.rs` - 수정. `pub mod human;`.

## Change description

design §5 메커니즘(엔진은 α 행렬로 자극하지만 사람은 α에 없으므로 **전 페르소나 flat 강자극** + 기존 excitation 감쇠로 모델링; mark는 현 동역학에 미사용이나 observability·설계 충실 위해 큰 값 기록):

- `pub struct HumanChannel { speaker_id: PersonaId, attention: f64, reset_factor: f64 }`
  - `speaker_id`: 사람의 화자 이름(예 "you"). `attention`: 전 페르소나에 더할 자극 크기(큰 값, 예 1.5). `reset_factor`: 사람 발화 시 기존 excitation에 곱할 감쇠(예 0.5 = "일부 리셋", 사람에게 주목).
  - `new(speaker_id)` 기본값(attention/reset 합리적 상수) + 필요 시 명시 생성자.
- `pub fn speak(&self, state: &mut EngineState, personas: &[Persona], text: String, ts: f64)`:
  1. **일부 리셋**: 기존 `state.excitations`의 모든 값에 `reset_factor`를 곱한다(페르소나끼리의 누적 모멘텀을 줄여 사람에게 주목하게).
  2. **강자극**: 각 persona에 대해 `state.excitations[p] += self.attention`(없으면 0에서 시작) → 전 페르소나 λ가 사람 발화로 일제히 차오름.
  3. **history push**: `Event { ts, speaker: self.speaker_id.clone(), mark: HUMAN_MARK(큰 값, 예 5.0), content: Some(text) }`를 `state.history`에 추가. (이후 RRF 내용 신호가 이 발화를 화제 기준점으로 삼음 - 화제 선점은 v0.3 내용 RRF로 이미 창발, 별도 로직 불요.)
  - rng 미소비, 비결정 요소 없음 → 결정적.
- 상수 `HUMAN_MARK` + 기본 attention/reset_factor를 `human.rs`에 명시(하드코딩 금지 원칙상 const로).
- 가드레일: `driver::run`·`hawkes.rs`·기존 함수 **변경 없음**(새 모듈만). unwrap/panic 금지. 결정성·골든 보존.

## Dependencies

- v0.4 전체(EngineState, Event, Persona, excitations 모델).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test` green. 신규 단위 테스트(`human::tests`, 네트워크/rng 없이):
  - (1) `speak` 후 `state.history` 마지막 = 사람 Event(speaker = speaker_id, mark = HUMAN_MARK, content = 입력 텍스트).
  - (2) `speak` 후 **모든** 페르소나 excitation이 호출 전보다 증가(combined_intensities 일제 상승).
  - (3) 기존 excitation이 있을 때 `reset_factor`로 감쇠된 뒤 attention이 더해짐(예: 기존 2.0, reset 0.5, attention 1.5 → 2.0*0.5+1.5 = 2.5 확인).
  - (4) 같은 입력 두 번 호출이 결정적(동일 state 변화).
- **골든 5종 바이트 동일**(HumanChannel은 라이브 전용, headless 불침투).

## Risks

| 위험 | 회피 |
|---|---|
| 사람 자극이 Hawkes 안정성 깸 | attention은 일시적(이후 decay_excitations로 회복), spectral radius 조건(α) 불변. 큰 mark는 기록용일 뿐 동역학은 flat 자극 |
| 기존 엔진/골든 오염 | 새 모듈만 추가, driver/hawkes 미변경. 골든 재확인 |
| reset_factor/attention 값 감각 | const 기본값 + 라이브 관전(task-31)으로 튜닝. 값 자체는 손잡이 |
| mark 미사용 혼란 | mark는 현재 동역학 미반영(observability·설계 충실용). 자극은 attention flat 보장 — 주석 명시 |
