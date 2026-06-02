---
title: "Salon v0.6 Task 33: FlowMeter 코어 (수렴/발산 측정)"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v6.md
task_id: "33"
depends_on: []
parallel_group: ""
---

# Task 33 - FlowMeter 코어

plan `salon-engine-v6.md` subtask 33. 대화 수렴/발산을 **토큰 중복**으로 싸게 측정한다(design §4 "키워드 중복도 근사"). **순수·결정적, 네트워크/rng 없음.** 배선(record/TUI)은 task-34/35. 이 task는 측정 함수만.

## Changed files

- `src/flow.rs` - 신규. `FlowMetric` + `FlowMeter::measure`.
- `src/lib.rs` - 수정. `pub mod flow;`.

## Change description

- `pub struct FlowMetric { pub convergence: f64 }` — `serde::Serialize`(record 직렬화용). `convergence ∈ [0,1]`: **1 = 발화들이 서로 비슷함(반복/수렴, 대화 식음)**, **0 = 다 새로움(발산, 살아있음)**. (필요 시 novelty=1-convergence 등 추가 가능하나 v0.6은 convergence만.)
- `pub fn measure(recent: &[&str]) -> Option<FlowMetric>`:
  - 각 발화를 토큰화: 소문자화 + 공백 분리 + 양끝 구두점 정도 제거(한국어는 공백 토큰 근사 - design 명시). 빈 토큰셋 발화는 제외.
  - 토큰셋 2개 미만이면 `None`(측정 불가).
  - 수렴도 = 윈도우 내 **평균 pairwise Jaccard**(|A∩B|/|A∪B|)를 모든 쌍에 대해. 높을수록 반복적=수렴.
  - 결정적: rng·네트워크·시간 없음. 같은 입력 → 같은 값.
- 토큰화/지표는 단순하게(v0.6 목표는 근사). BGE-M3 임베딩은 이후 단계에서 `measure` 인터페이스를 유지한 채 내부만 교체.
- 가드레일: unwrap/panic 금지(빈 입력·0 division 방어 - 합집합 0이면 그 쌍은 스킵 또는 0). 순수 함수.

## Dependencies

- 없음(v0.5 위 신규 모듈).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test` green. 신규 `flow::tests`(네트워크/rng 없이):
  - (1) 동일/거의 동일 발화 여러 개 → convergence 높음(예 >0.8).
  - (2) 전부 다른 토큰의 발화 → convergence 낮음(예 <0.2).
  - (3) 발화 1개 이하 또는 빈 입력 → None.
  - (4) 같은 입력 두 번 measure → 동일 값(결정성).
  - (5) Jaccard 손계산 검증(예: ["a b", "a c"] → 교집합{a}/합집합{a,b,c}=1/3 ≈ 0.333).
- **골든 5종 바이트 동일**(flow는 아직 record에 안 들어감 - 순수 모듈만 추가).

## Risks

| 위험 | 회피 |
|---|---|
| 0-division(빈 합집합) | 합집합 0인 쌍 스킵, 유효 쌍 없으면 None. 빈 토큰셋 발화 제외 |
| 한국어 토큰 부정확 | v0.6은 근사가 목표. 공백 토큰 + 소문자. 정밀도는 BGE-M3(이후) |
| 비결정 유입 | Jaccard는 집합 연산만(BTreeSet 등 결정적 순회). rng·시간·네트워크 없음 |
| 지표 스케일 감각 | 라이브 관전(task-35)으로 튜닝. 임계는 v0.7에서 |
