---
title: Salon 엔진 플랜 v0.10 - friend engine Stage 2 (BGE-M3 의미검색 + HNSW + hybrid)
type: plan
status: planned
priority: P1
updated_at: 2026-06-03
owner: shared
summary: v0.9(형태소 + SQLite/FTS5 BM25)가 어휘 회상을 깔았다. v0.10은 **의미 회상**을 얹는다 - BGE-M3 임베딩(ORT/ONNX in-process, CoreML)으로 발화를 벡터화하고 usearch HNSW로 ANN 검색, hybrid RRF로 BM25(어휘) + 벡터(의미)를 융합. seCall `embedding.rs`/`ann.rs`/`model_manager.rs`/`hybrid.rs` lift. "어휘는 다른데 의미는 같은"(부처님 대화의 겉돎) 회상이 가능해진다. ML 스택은 새 sub-feature `friend-engine-semantic` 뒤(v0.9는 그대로 사용 가능). recall_eval은 mock embedder로 결정적 + 실모델은 `#[ignore]`.
design_ref: ../reference/salon-engine-design.md
source_ref: /Users/d9ng/privateProject/seCall/crates/secall-core/src/search
roadmap_ref: salon-engine-v9.md
notes_ref: ../temp/salon-memory-engine-idea.md
---

# Salon - 플랜 v0.10 (friend engine Stage 2: 의미 회상)

## 0. Context

v0.9가 Stage 0+1 완료: Lindera 형태소 + SQLite/FTS5 BM25 회상(어휘 기반). v0.10 = **Stage 2: 의미 회상**. seCall 검색코어의 임베딩/ANN/hybrid 층을 lift([[secall-search-core]]):
- `embedding.rs`(ORT/ONNX BGE-M3, 1024d) + `model_manager.rs`(HF서 ONNX 다운로드, SHA256, `~/.cache`) → 발화 벡터화.
- `ann.rs`(usearch HNSW, cosine, `.usearch` 디스크 영속) → ANN 검색.
- `hybrid.rs`(RRF k=60) → BM25(어휘) + 벡터(의미) 융합.

**왜**: 라이브 관찰(부처님 대화) - 어휘는 매번 다른데 의미는 겉돎(흐름 수렴 0.04인데 의미적으로 같은 말 반복). FTS5 BM25(어휘)로는 못 잡는다. BGE-M3 의미 임베딩이 필요. 회상 품질뿐 아니라 이후 FlowMeter 정밀도(v0.6 measure 인터페이스 교체)에도 쓰인다.

## 1. 핵심 결정 (Architect)

- **임베딩 백엔드 = ORT/ONNX in-process (CoreML)**. 로컬 ollama 금지(맥북 랙)이지만, ORT는 **데몬 없이 프로세스 안**에서 BGE-M3 ONNX(~1.2GB)를 1회 로드(lazy) + Apple Silicon CoreML. seCall이 동일 맥에서 검증함(chat LLM 10GB+와 달리 1.2GB라 랙 우려 낮음, **단 로드 시간 측정 필수**). 원격(cloud bge-m3) 폴백은 옵션(네트워크 의존).
- **새 sub-feature `friend-engine-semantic`**(`= ["friend-engine", "dep:ort", "dep:usearch", "dep:ndarray", "dep:tokenizers"]`). v0.9(`friend-engine`)는 ORT 없이 그대로 사용 가능 - ML 스택은 더 무거우니 graduated feature로 격리. 기본 빌드·CI·골든은 여전히 ML-free.
- **recall_eval 결정성**: 임베딩은 float 비결정(ONNX 스레딩·HNSW 근사). → ① **mock embedder**(결정적 해시 기반 벡터)로 hybrid/ANN 배선을 결정적으로 테스트. ② 실 BGE-M3 품질은 `#[ignore]` 통합 테스트(수동, 모델 다운로드). 일상 `cargo test`는 mock으로 결정적 유지.
- **저장**: v0.9 SQLite에 임베딩 추가(`turn_vectors`류 테이블 또는 memories BLOB 컬럼) + `.usearch` 인덱스 파일(memory.db 옆). 라이브 영속 경로만.

## 2. Invariants

| ID | 내용 |
|----|------|
| INV-1 | **feature 격리**: ML 스택은 `friend-engine-semantic` 뒤. 기본/CI/골든 빌드는 ort/usearch 미컴파일. v0.9(`friend-engine`)·기본은 그대로 |
| INV-2 | **골든 무손상**: 임베딩/ANN도 라이브 recall 경로 전용(v0.8 불변식). driver/headless 불침투 → 골든 바이트 동일 |
| INV-3 | **일상 테스트 결정성**: mock embedder로 hybrid/ANN을 결정적으로 채점. 실모델·HNSW 근사 비결정은 `#[ignore]`로 분리 |
| INV-4 | **회상은 검색만**: 엔진 결정(gate/rrf/hawkes/cooling)에 불사용. 생성 프롬프트(회상 슬롯)에만 |
| INV-5 | v0.9의 222/230 tests + 스모크 유지(+ v0.10 게이트) |

## 3. Subtasks

| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| 47 | 임베더 + 모델 매니저(Stage 2a) | `embedding.rs` lift: `Embedder` trait + ORT/ONNX BGE-M3(1024d, mean pool + L2 norm) + `model_manager`(HF 다운로드/SHA256). **mock embedder**(결정적). `friend-engine-semantic` feature. 단위 테스트는 mock, 실모델은 `#[ignore]` | **높음**(ort/tokenizers dep, CoreML, 모델 다운로드·RAM·로드 측정) | v0.9 |
| 48 | HNSW ANN + 임베딩 저장(Stage 2b) | `ann.rs`(usearch) lift: cosine, add/search, `.usearch` 영속. SQLite에 임베딩 저장(BLOB 또는 turn_vectors). record 시 임베딩 계산·저장·ANN add | 높음(usearch, 영속, 동기화) | 47 |
| 49 | hybrid RRF 회상(Stage 2c) | `hybrid.rs` RRF lift: recall이 FTS5 BM25(어휘) + HNSW 벡터(의미) 각각 상위 → RRF(k=60) 융합 + 참여 격리. v0.9 BM25-only recall 대체(semantic feature on) | 중~높음(융합·참여격리·결정성) | 48 |
| 50 | recall_eval 의미 + 게이트 + 마감(Stage 2d) | recall_eval에 의미 케이스(어휘 다른데 의미 같은 SSOT) 추가, mock으로 결정적 채점 + 실모델 `#[ignore]`. smoke_v10. README/CLAUDE bump. 로드/RAM 측정 기록 | 중 | 49 |

Phase A(47 임베더) → B(48 ANN+저장, 49 hybrid) → C(50 eval+게이트).

## 4. 데이터 모델 / 아키텍처 델타

| 구조 | 변경 |
|------|------|
| `Cargo.toml` | `friend-engine-semantic = ["friend-engine","dep:ort","dep:usearch","dep:ndarray","dep:tokenizers"]`. ort/usearch/ndarray/tokenizers optional. `coreml` feature(`ort/coreml`) |
| 신규 `src/embed.rs`(가칭) | `Embedder` trait + `OrtEmbedder`(BGE-M3) + `MockEmbedder`(결정적, 테스트). `model_manager`(다운로드). feature-gated |
| 신규 `src/ann.rs` | usearch HNSW 래퍼(seCall `ann.rs` 동형). `.usearch` 영속. feature-gated |
| `src/memory.rs`(sqlite_impl) | 임베딩 저장(BLOB/turn_vectors) + ANN 인덱스 보유. `recall`이 semantic feature on이면 hybrid(BM25+벡터 RRF), off(v0.9)면 BM25-only. record가 임베딩 계산·저장·add |
| recall_eval | 의미 케이스 + mock embedder 결정적 + 실모델 `#[ignore]` |

## 5. 위험과 대응

| 위험 | 대응 |
|------|------|
| ORT BGE-M3 로드가 맥북 랙(ollama처럼) | 1.2GB(chat LLM 10GB+ 대비 작음) + CoreML + 1회 lazy 로드. **task-47에서 로드시간·RAM 측정**, 과하면 원격 임베딩 폴백 검토 |
| 기본/골든 빌드에 ML 스택 유입 | `friend-engine-semantic` 뒤로 격리. 기본·`friend-engine`·골든은 ort/usearch 미컴파일 |
| 테스트 비결정(임베딩 float·HNSW 근사) | mock embedder로 결정적 채점. 실모델은 `#[ignore]`(수동, 모델 다운로드) |
| usearch/ORT Apple Silicon | seCall 검증됨(usearch ✓, ort+coreml ✓). Kiwi는 안 씀(Lindera만) |
| 임베딩-SQLite-ANN 동기화 | record가 원자적으로 사건+임베딩+ANN add. 재오픈 시 ANN 인덱스 로드/재생성(seCall 패턴) |
| 골든 깨짐 | 라이브 recall 전용. driver/headless 불침투. 골든 재확인(양 feature) |

## 6. v0.10 완료 기준

- 기본/`friend-engine` 빌드·골든 무변(ort/usearch 미컴파일).
- `friend-engine-semantic` on: 라이브에서 어휘 안 겹쳐도 의미 회상(hybrid). 실모델로 수동 확인.
- recall_eval: 의미 케이스가 mock으로 결정적 채점 통과 + 실모델 `#[ignore]` 통과(수동).
- ORT BGE-M3 로드 시간·RAM 측정 기록(랙 여부 판단).

## 7. 산출물

- 이 문서. 구현 시 §3을 `salon-engine-v10-task-47..50.md`로 분해.
- v0.10 한 줄: BGE-M3 의미 임베딩 + HNSW + hybrid RRF로 회상을 "어휘"에서 "의미"로 끌어올린다. ORT in-process(데몬 없음), `friend-engine-semantic` feature 뒤, 골든·기본빌드 무손상.
