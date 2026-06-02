---
title: "Salon v0.1 Task 08: 스모크 테스트 + 파라미터 스윕"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v1.md
task_id: "08"
depends_on: ["06"]
parallel_group: "outputs"
---

# Task 08 - 스모크 테스트 + 파라미터 스윕 모드

plan `salon-engine-v1.md` v0.1 작업 항목 9, 10. v0.1 완료 기준 5개를 headless 출력으로 자동 검증하고, μ/θ/k를 바꿔가며 리듬을 비교하는 스윕 모드를 붙인다. **이 task의 `cargo test --test smoke` 통과가 v0.1 완료 게이트다.**

## Changed files

- `tests/smoke.rs` - 신규. 통합 테스트. driver를 고정 seed headless로 돌려 완료 기준 assert.
- `src/sweep.rs` - 신규. μ/θ/k 그리드 스윕.
- `src/main.rs` - 수정. `--sweep` 플래그 배선.
- `src/lib.rs` - 수정. `pub mod sweep;` 추가(additive).

## Change description

plan §2 "완료 기준 (검증)" 5개를 통합 테스트로 박는다. 통합 테스트는 라이브러리 API(driver + VecSink)를 직접 호출해 record를 모으고 단언한다.

- 스모크(완료 기준 → assert):
  1. μ 높은 페르소나가 같은 seed에서 더 자주 발화(speak_count 비교). θ=0.65(중간 구간) 사용 — μ 차이는 중간 θ에서만 선명하다(낮으면 포화, 높으면 저-μ 0으로 굶음; 플랜 §2 참조).
  2. θ를 올리면 침묵 빈도 증가, 내리면 감소(silence_count 단조 방향).
  3. (기준 c + 격리 a) 분산은 balance가 담당하고 k는 순간 미세조정이다. 격리 검증 한 줄: `rrf::select`에 빈 history(balance 중립) + 고정 intensities로 여러 seed를 돌려, 비주도 후보 승률이 큰 k > 작은 k임을 assert(작은 k=intensity 1등 독점, 큰 k=분산). long-run 화자 분포로는 k를 검증하지 않는다(balance가 가림). 근거: 플랜 §2 완료 기준, 메모리 [[project-balance-rhythm-knob]].
  4. (v0.2로 이관) 길이/패턴의 seed 분포는 v0.1 기준에서 제외한다(α=0이라 거의 결정적). 플랜 §3 v0.2 완료 기준으로 옮겼으므로 스모크에서 단언하지 않는다.
  5. record에 화자 선택 이유(rrf_reason)가 채워져 "왜 이 페르소나"가 읽힌다.
- 단언은 부등호/방향/분산>0 형태로(정확값 금지). 고정 seed로 결정적이되 작은 구현 차이에 안 깨지게.
- 스윕 모드: `--sweep`이면 μ/θ/k 작은 그리드를 같은 seed로 돌려 config별 요약(발화/침묵 카운트, 화자 분포)을 stdout에 한 줄씩. 사람이 리듬 차이를 눈으로 비교하는 용도(테스트 아님).

## Dependencies

- task-06 (driver + headless + VecSink). task-01 타입.

## Verification

```bash
cargo test --test smoke
cargo run -- --sweep --seed 42 | wc -l
```

- `cargo test --test smoke` exit 0: 완료 기준 5개 단언 전부 통과(5개 이상 테스트 통과 보고). **v0.1 완료 게이트.**
- `cargo run -- --sweep --seed 42 | wc -l`: 출력 라인 수가 그리드 크기 이상(config별 요약이 나옴).

## Risks

| 위험 | 회피 |
|---|---|
| 스모크 단언이 seed/구현에 민감해 flaky | 부등호·방향·분산>0만 단언. θ/k 테스트 점을 효과가 확실히 나는 값으로 선택 |
| 기준 2(θ)·3(k) 효과가 약한 파라미터 구간 | 대비가 큰 두 점(낮은 θ vs 높은 θ 등)으로 테스트, 중간값 회피 |
| 스윕이 테스트로 오인됨 | 스윕은 stdout 출력 도구, 검증은 smoke.rs가 담당으로 역할 분리 |
| 통합 테스트가 stdout 파싱에 의존해 취약 | NDJSON 문자열 파싱 대신 라이브러리 API(driver+VecSink)로 record 직접 수집 |
