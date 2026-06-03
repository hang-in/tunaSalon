---
title: "Salon v0.10 Task 48: HNSW ANN (usearch) + 임베딩 저장 (Stage 2b)"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v10.md
task_id: "48"
depends_on: ["47"]
parallel_group: ""
---

# Task 48 - HNSW ANN + 임베딩 저장 (Stage 2b)

plan `salon-engine-v10.md` Stage 2b. task-47의 임베더 위에 **벡터 인덱스**를 얹는다: usearch HNSW 래퍼(`ann.rs`) + SQLite 임베딩 저장 + record 시 임베딩 계산·저장·ANN add + `vector_search`. **recall 자체는 아직 안 바꾼다**(BM25-only 유지) - hybrid 융합은 task-49. → 이 task는 저장·인덱스·검색 메서드만, MockEmbedder로 결정적·모델 불요.

lift 원본: `/Users/d9ng/privateProject/seCall/crates/secall-core/src/search/ann.rs`(usearch 래퍼).

## Changed files

- `Cargo.toml` - 수정. `friend-engine-semantic`에 `dep:usearch` 추가. `usearch = { version = "2", optional = true }`(seCall과 동일). **cfg(not(windows))** 환경에서만 컴파일되도록 ann 모듈에서 게이팅.
- `src/ann.rs` - **신규, `#![cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]`**. seCall `ann.rs` lift(String 에러, tracing 제거):
  - `pub struct AnnIndex { index, path, dims }`.
  - `open_or_create(path: &Path, dims) -> Result<Self,String>`: 파일 있으면 load + reserve, 없으면 new + reserve(10_000). `IndexOptions{ dimensions, metric: MetricKind::Cos, quantization: ScalarKind::F32, connectivity:0, expansion_*:0, multi:false }`.
  - `in_memory(dims) -> Result<Self,String>`(신규): 파일 없이 메모리 인덱스(`:memory:` store·테스트용. save 안 함).
  - `add(&self, key: u64, vec: &[f32]) -> Result<(),String>`(capacity 초과 시 auto-reserve).
  - `search(&self, query: &[f32], limit) -> Result<Vec<(u64,f32)>,String>`((key, distance); **distance 낮을수록 가까움**).
  - `save(&self) -> Result<(),String>`, `size()`, `dimensions()`.
  - `src/lib.rs`: `#[cfg(all(feature="friend-engine-semantic", not(target_os="windows")))] mod ann;`.
- `src/memory.rs`(sqlite_impl, **`#[cfg(feature="friend-engine-semantic")]` 추가분**) - 수정:
  - 필드(semantic만): `embedder: Box<dyn crate::embed::Embedder>`(기본 `MockEmbedder::default()`, dim 1024), `ann: Option<crate::ann::AnnIndex>`.
  - 스키마: `CREATE TABLE IF NOT EXISTS memory_vectors(mem_id INTEGER PRIMARY KEY, embedding BLOB NOT NULL)`(init_schema에 추가, semantic 시).
  - `new()`(:memory:): `ann = AnnIndex::in_memory(1024)`(없으면 None + 경고). embedder = Mock.
  - `open(path)`(파일): `.usearch`는 memory.db 옆(`<db>.usearch`). 있으면 load, 없으면 memory_vectors에서 재구축(저장된 BLOB→add). embedder = Mock(실 OrtEmbedder 선택은 task-50).
  - `record()`: 사건 insert 후(mem_id) → `embedding = embedder.embed(content)`(실패 시 skip + 경고) → memory_vectors에 BLOB 저장 → `ann.add(mem_id, &embedding)`. (사람 발화 포함, 기존 흐름 뒤.)
  - `pub fn vector_search(&self, query: &str, k) -> Vec<(i64, f32)>`(신규, semantic): `embedder.embed(query)` → `ann.search` → (mem_id, distance). 참여 격리는 task-49의 recall에서(여기선 raw 검색). 없으면 빈 Vec.
  - **recall 자체는 불변**(BM25-only). hybrid는 task-49.

## Change description

- **BLOB 직렬화**: f32 Vec ↔ bytes(`bytemuck` 없이 `to_le_bytes`/`from_le_bytes` 수동, 또는 단순 루프). dim 1024 고정.
- **결정성**: MockEmbedder(결정적) → 같은 사건열 → 같은 임베딩·ANN. `:memory:` + in-memory ANN. HNSW 근사지만 작은 셋(테스트)에선 안정. 테스트는 명확한 분리 케이스로(겹치는 토큰 쿼리 → 해당 사건이 top).
- **저장-ANN 동기화**: record가 memory_vectors BLOB + ann.add를 같이. open 시 .usearch 우선, 없으면 BLOB에서 재구축(seCall 패턴).
- **골든·기본빌드**: 전부 `friend-engine-semantic` cfg 안. 기본·`friend-engine`(non-semantic) 빌드는 ann/embedder/memory_vectors 없음 → v0.9 그대로. driver/headless 불침투 → 골든 무손상. recall 미변경.
- 가드: 요청 파일만. unwrap/panic 금지(임베딩/ANN 실패는 skip+경고 또는 Result). usearch cfg(not(windows)).

## Verification

```bash
cargo build
cargo test
cargo build --features friend-engine                 # non-semantic v0.9 그대로(ann/embedder 없음)
cargo build --features friend-engine-semantic        # usearch 컴파일(느림)
cargo test  --features friend-engine-semantic        # ann + 임베딩 저장 + vector_search(mock 결정적)
cargo run -q -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
```

- 기본/`friend-engine`/semantic 빌드 OK. 골든 무변. `friend-engine`(non-semantic)에 ann 미컴파일.
- 신규 테스트(semantic, mock, :memory:):
  - `ann.rs`: open_or_create/in_memory + add + search(가까운 벡터가 낮은 distance) + save/load roundtrip.
  - memory: record N건 → memory_vectors에 N행 + ann.size()==N. `vector_search("토큰 겹치는 쿼리")`가 해당 사건 mem_id를 상위로. 결정성(2회 동일). 빈/실패 graceful.
  - open(file) roundtrip: record→drop→reopen→ANN 재구축/load→vector_search 동작(temp 경로, 정리).
- recall(BM25)·recall_eval 기존 테스트 그대로 green(recall 미변경).

## Risks

| 위험 | 회피 |
|---|---|
| usearch C++ 컴파일·플랫폼 | seCall 검증(Apple Silicon ✓). cfg(not(windows)). 빌드 느림 예상 |
| HNSW 근사로 테스트 불안정 | 작은 셋 + 명확 분리 케이스. MockEmbedder 결정적. distance 비교로 단언(정확 순위 강요 X) |
| 저장-ANN 불일치 | record가 BLOB+add 동시. open 시 .usearch 없으면 BLOB 재구축 |
| 골든/v0.9 회귀 | 전부 semantic cfg. recall 미변경. friend-engine(non-semantic)·기본 빌드 무변 확인 |
| BLOB 직렬화 endian | to_le_bytes/from_le_bytes 일관 |
| RefCell/Send 등 | AnnIndex는 store가 단일 스레드 소유(라이브 tick). embedder Box<dyn> |
