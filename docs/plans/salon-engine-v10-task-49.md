---
title: "Salon v0.10 Task 49: hybrid RRF 회상 (BM25 어휘 + 벡터 의미) (Stage 2c)"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v10.md
task_id: "49"
depends_on: ["48"]
parallel_group: ""
---

# Task 49 - hybrid RRF 회상 (Stage 2c)

plan `salon-engine-v10.md` Stage 2c. recall을 **hybrid**로: FTS5 BM25(어휘, v0.9) + ANN 벡터(의미, task-48) 각각 상위 → RRF(k=60) 융합 + 참여 격리. **`friend-engine-semantic`일 때만** hybrid; non-semantic(`friend-engine`)·기본은 v0.9 BM25-only 그대로(cfg 교체, v0.9 보존). 임베더는 MockEmbedder(결정적, 실 OrtEmbedder 배선은 task-50).

lift 원본: `/Users/d9ng/privateProject/seCall/crates/secall-core/src/search/hybrid.rs`(`reciprocal_rank_fusion`: `1/(k+rank+1)` 키별 합산, score 내림차순, k=60).

## Changed files

- `src/memory.rs`(sqlite_impl) - 수정:
  - **v0.9 `recall` 보존**: 기존 BM25 recall에 `#[cfg(not(feature = "friend-engine-semantic"))]` 부여(= friend-engine non-semantic에선 그대로). **바이트 동일**.
  - **신규 hybrid `recall`** `#[cfg(feature = "friend-engine-semantic")]`(같은 시그니처 `recall(&self, persona, query, k) -> Vec<MemoryEvent>`):
    1. 참여 방 집합(v0.9와 동일).
    2. **BM25 leg**: 기존 FTS5 OR-MATCH 쿼리(참여 방 필터) → `SELECT m.id ... ORDER BY bm25, ts DESC, id DESC LIMIT N` → `Vec<i64>` 랭크순(N=k*4 over-fetch).
    3. **벡터 leg**: `vector_search(query, k*4)`(task-48) → (mem_id, dist) → **참여 방으로 필터**(`SELECT id FROM memories WHERE id IN(...) AND room IN(참여)` 로 격리) → `Vec<i64>` 랭크순(dist 오름차순 유지).
    4. **RRF 융합**: `rrf_fuse(&bm25_ids, &vec_ids, 60.0) -> Vec<i64>`(순수 헬퍼). 상위 k.
    5. 최종 id들의 사건을 순서대로 fetch(`SELECT room,ts,speaker,content FROM memories WHERE id=?`) → owned `Vec<MemoryEvent>`.
  - 신규 순수 `fn rrf_fuse(bm25: &[i64], vector: &[i64], k_rrf: f64) -> Vec<i64>`: 각 리스트 `1/(k_rrf+rank+1)` 키별 합산, score 내림차순(동점은 id 오름차순으로 안정). 단위 테스트.
- (recall_eval/tests) - hybrid에서도 SSOT 회상·참여 격리 유지 확인(MockEmbedder 결정적). friend-engine(non-semantic) recall_eval은 v0.9 그대로 green.

## Change description

- **v0.9 보존 핵심**: non-semantic recall 미변경(cfg로 분리). hybrid는 semantic 전용. → 기본·`friend-engine` 빌드·골든·v0.9 recall_eval 전부 무변.
- **참여 격리(양 leg)**: BM25는 SQL `room IN(참여)`로(v0.9). 벡터는 ANN이 방을 모르므로 **검색 후 참여 방으로 필터**(over-fetch k*4 후 격리·절단). INV-2(참여 격리) 양쪽 보장 - recall_eval 격리 케이스로 검증.
- **결정성**: MockEmbedder(결정적) + FTS5 bm25(결정적) + RRF(결정적, 동점 id 안정). `:memory:`. hybrid recall 2회 동일.
- **RRF**: seCall 동형(`1/(k+rank+1)` 합산). observer penalty/정규화는 미적용(tunaSalon 무관).
- 가드: 요청 파일만(memory.rs). recall 시그니처 불변. unwrap/panic 금지(쿼리 실패→빈 Vec). 임베딩 실패→BM25만으로 폴백(graceful).

## Verification

```bash
cargo build && cargo test                              # 기본 무변
cargo test --features friend-engine                    # v0.9 recall 그대로 green
cargo build --features friend-engine-semantic
cargo test  --features friend-engine-semantic          # hybrid recall + rrf_fuse(mock 결정적)
cargo run -q -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
```

- 기본/`friend-engine`/semantic 빌드 OK. 골든 무변. **friend-engine(non-semantic) recall_eval·memory recall 테스트 그대로**(v0.9 보존).
- 신규 테스트(semantic):
  - `rrf_fuse`: 두 랭크 리스트 융합(양쪽 상위인 키가 최상위, 한쪽만인 키도 포함), 동점 안정, 빈 리스트 graceful.
  - hybrid recall: SSOT 회상(BM25+벡터 둘 다 잡는 케이스가 상위), 참여 격리(미참여 방 벡터 결과 제외), 결정성(2회 동일), 임베딩 없을 때 BM25 폴백.
  - recall_eval(semantic): SSOT top-3 유지 + 격리(mock으로 결정적).

## Risks

| 위험 | 회피 |
|---|---|
| v0.9 recall 회귀 | non-semantic recall 미변경(cfg 분리). friend-engine recall_eval로 검증 |
| 벡터 참여 격리 누락 | ANN 검색 후 `room IN(참여)` 필터(over-fetch k*4). recall_eval 격리 케이스 |
| HNSW 근사 비결정 | MockEmbedder 결정적 + 작은 셋. RRF 동점 id 안정. 정확 순위 강요 X(상위 포함만 단언) |
| 골든/기본빌드 | 전부 semantic cfg. recall 시그니처 불변. 기본·friend-engine 무변 |
| 임베딩 실패로 recall 깨짐 | 벡터 leg 실패→빈 → RRF는 BM25만 → v0.9급 결과(graceful) |
