---
title: "Salon v0.1 Task 04: SignalCollector + RRF Fuser"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v1.md
task_id: "04"
depends_on: ["01"]
parallel_group: "engine-core"
---

# Task 04 - SignalCollector + RRF Fuser

plan `salon-engine-v1.md` v0.1 작업 항목 4. 게이트를 넘은 후보들 중 최종 화자 1명을 RRF로 뽑는다. v0.1 신호는 세 개: 시간 강도 · 발화 균형 · 난수.

## Changed files

- `src/rrf.rs` - 신규. 신호 수집 + RRF 융합.
- `src/lib.rs` - 수정. `pub mod rrf;` 한 줄 추가(additive).

## Change description

설계 문서 3-2절 RRF + plan §2 틱 루프 3번.

- 신호별로 후보를 순위 매긴다.
  - 시간 강도: λ 높은 순.
  - 발화 균형: 최근 적게 말한 순(독점 방지). history에서 최근 발화 횟수로 계산.
  - 난수: 주입된 시드 RNG로 후보 순서를 섞음(예측 불가성).
- RRF 융합: `score(p) = Σ_i 1/(k + rank_i(p))`, `k`는 EngineConfig.k. 최고 score 1명 선택.
- **RNG는 주입식**(`&mut ChaCha8Rng` 인자). 전역 RNG 금지 - 시드가 같으면 선택이 동일해야 한다.
- 어느 신호가 1등을 만들었는지(설명)를 함께 반환해 ObservationRecord.rrf_reason에 들어가게 한다. 예: 선택된 후보의 신호별 기여 중 최대 항목 이름.
- v0.1 제외 신호(관심도·잔향·사람 반응성)는 두지 않는다. SignalCollector는 세 신호만 안다.

## Dependencies

- task-01 (EngineConfig, PersonaId, history/intensities 타입, RNG 타입).

## Verification

```bash
cargo test --lib rrf
```

- `cargo test --lib rrf` exit 0. 포함 테스트(4개 이상 통과 보고):
  1. 알려진 순위 입력에 대해 RRF score가 `Σ 1/(k+rank)` 공식과 일치.
  2. 발화 균형: 최근 많이 말한 페르소나의 균형 순위가 낮다(불리).
  3. 같은 시드 RNG로 두 번 호출 시 선택 결과 동일(결정성).
  4. k를 크게 한 분포가 작게 한 분포보다 1등 쏠림이 완만(고정 시드로 N회 선택 카운트 비교).

## Risks

| 위험 | 회피 |
|---|---|
| 전역 RNG 사용으로 비결정 | RNG 주입(`&mut ChaCha8Rng`), 테스트 3으로 결정성 고정 |
| rrf_reason 표현이 모호해 미터에서 안 읽힘 | "기여 최대 신호 이름" 같은 단순 규칙으로 명세, ObservationRecord에 문자열로 |
| k 효과 테스트가 시드 의존 flaky | 고정 시드 + 충분한 N회, 부등호(쏠림 완화)만 assert(정확값 X) |
| lib.rs 공유 수정 | `pub mod rrf;` 한 줄 추가만 |
