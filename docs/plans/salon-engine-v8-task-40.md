---
title: "Salon v0.8 Task 40: 회상 평가 하네스 (SSOT 자동 채점)"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v8.md
task_id: "40"
depends_on: ["39"]
parallel_group: ""
---

# Task 40 - 회상 평가 하네스

plan `salon-engine-v8.md` subtask 40. 설계 노트 `salon-recall-eval-harness.md`의 핵심 - 정답(SSOT)/함정(distractor) 심은 회상방으로 검색층을 **자동 채점**한다. 재현율(정답 끌어옴) + 정확도(함정 안 끌어옴) + 참여 격리(없던 방 회상 금지). 결정적·네트워크 없음. 이게 tunaSalon이 검색코어 테스트베드가 되는 지점.

## Changed files

- `tests/recall_eval.rs` - 신규. 회상방 시나리오 빌더 + 검색층 채점 + assert. (task-39 `MemoryStore` 공개 API 사용.)
- (선택) `src/memory.rs` - 채점에 필요하면 작은 헬퍼 노출(예 점수/id). 가능하면 기존 API로.

## Change description

- **회상방 시나리오**(결정적, 손으로 심음): 방 2~3개 + 페르소나 3명(참여 비대칭). 예:
  - room "morning": A, B 참여. SSOT 사건(예 "다음주 화요일 등산 약속") + distractor(주제 비슷·답 아님, 예 "지난주 등산은 취소").
  - room "evening": B, C 참여. 다른 SSOT + distractor.
  - C는 morning에 **없음** → morning SSOT 회상 금지.
- **검색층 채점**(자동, id/내용 대조):
  - 재현율: SSOT 주제로 쿼리 → `recall(persona, query, k)` 상위 k에 SSOT 사건이 들어오는가(recall@k).
  - 정확도: 같은 쿼리에서 distractor가 SSOT보다 위로 안 오는가 / 상위에서 걸러지는가.
  - **참여 격리**: morning 비참여자 C가 morning SSOT를 회상하지 않는가(빈 결과/없음).
- 채점 헬퍼: `recall_at_k(results, ssot_id_or_content) -> bool`, 상위 정확도 체크 등(test 파일 내 함수면 충분).
- 합격 임계(노트 미결): v0.8은 **SSOT가 top-3 안**이면 재현 통과로 시작(주석에 임계 명시, 이후 튜닝 가능).
- 결정성: 시나리오 고정 + recall 결정적(task-39) → 채점 재현. 벽시계·rng 없음.
- 발화층 채점(LLM judge)은 **비목표**(노트: 자동화 무게는 검색층). v0.8은 검색층만.

## Dependencies

- task-39(MemoryStore, recall).

## Verification

```bash
cargo build
cargo test
cargo test --test recall_eval
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test --test recall_eval` green. 케이스:
  - (1) 재현: morning 참여자(A 또는 B)가 SSOT 주제 쿼리 → SSOT가 top-3.
  - (2) 정확도: 같은 쿼리에서 distractor가 SSOT 위로 안 옴(또는 SSOT가 1위).
  - (3) **참여 격리**: C(morning 미참여)의 morning SSOT 쿼리 → 빈 결과(없던 방).
  - (4) evening 대칭(B/C는 evening SSOT 회상, A는 격리).
  - (5) 결정적: 같은 시나리오 두 번 채점 동일.
- **골든 5종 바이트 동일**(테스트 전용, 엔진 불침투).

## Risks

| 위험 | 회피 |
|---|---|
| distractor 변별력 부족 | 정답과 주제 비슷·답 다르게 설계(노트). 토큰 일부만 겹치게 |
| 임계 자의성 | top-3 시작, 주석 명시. 이후 튜닝(노트 미결) |
| 채점이 비결정 | 시나리오 고정 + recall 결정적. rng/벽시계 없음 |
| 토큰 근사 한계로 재현 실패 | SSOT 쿼리에 SSOT content 핵심 토큰 포함(근사 검색이 잡게). 정밀도는 BGE-M3(이후) |
