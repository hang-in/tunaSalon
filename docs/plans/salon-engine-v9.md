---
title: Salon 엔진 플랜 v0.9 - friend engine 심화 (seCall 검색코어 lift, 1단계: 형태소 + SQLite/FTS5 BM25)
type: plan
status: planned
priority: P1
updated_at: 2026-06-03
owner: shared
summary: v0.8 friend engine 첫 증분(인메모리 + 토큰중복 회상) 위에, 본인 소유 레포 seCall(`crates/secall-core/src/search/`)의 production 검색코어를 단계 lift한다. v0.9 범위 = Stage 0(Lindera 한국어 형태소 토크나이저) + Stage 1(SQLite 영속 MemoryStore + FTS5 BM25 회상). BGE-M3 의미검색(ORT)·HNSW(usearch)·hybrid RRF는 Stage 2~3 = v0.10. 전 검색코어는 `friend-engine` feature flag 뒤(기본 off) → 기본/골든 빌드는 lean·ML-free 유지. 회상은 라이브 경로 전용(v0.8 불변식)이라 골든 바이트 동일.
design_ref: ../reference/salon-engine-design.md
notes_ref: ../temp/salon-memory-engine-idea.md
source_ref: /Users/d9ng/privateProject/seCall/crates/secall-core/src/search
roadmap_ref: salon-engine-v1.md
---

# Salon - 플랜 v0.9 (friend engine 심화, 1단계)

## 0. Context

v0.8이 friend engine 첫 증분을 깔았다: 인메모리 `MemoryStore`(사건{방,ts,화자,내용} + 참여) + 토큰중복 키워드 회상 + 회상 평가 하네스(`tests/recall_eval.rs`). v0.8 플랜이 명시적으로 미뤄둔 심화 항목 - BGE-M3 의미검색 / SQLite 영속(L1) / 한국어 회상 정밀도 / BM25+벡터 융합 - 은 **본인 소유 레포 seCall**(github.com/hang-in/seCall, `crates/secall-core/src/search/`)에 production 수준으로 이미 구현돼 있다. 라이선스 무관하게 가져다 쓰기로 함(사용자 2026-06-03). 메모리 [[secall-search-core]].

seCall 검색코어 ↔ v0.8 deferred 매핑:
- BGE-M3 의미검색 → `embedding.rs`(ORT/ONNX) + `ann.rs`(usearch HNSW) + `model_manager.rs`
- SQLite 영속(L1) → `store/schema.rs`(rusqlite + FTS5)
- 한국어 회상 정밀도 → `tokenizer.rs`(Lindera ko-dic 형태소) + `bm25.rs`(FTS5)
- BM25+벡터 융합 → `hybrid.rs` RRF(k=60) - tunaSalon이 화자선택에 쓰는 그 RRF
- (망각·주관저장·인물기억은 seCall에도 없음 → tunaSalon 고유, 이 트랙 밖)

**반입 전략(결정)**: secall-core 통째 의존은 비추(default=web-ui, axum/rmcp/ingest/vault/mcp까지 끌고 옴 + search가 store/Session/Config에 결합). → `search/`를 tunaSalon에 **lift**하며 `Session`→우리 `MemoryEvent`, DB층만 적응. "한 경로씩" 기조로 v0.8 deferred 순서대로 단계화.

**Stage 0~3 (전체 arc)**:
- Stage 0: Lindera 형태소 토크나이저 (ML 의존성 0, 즉효)
- Stage 1: SQLite 영속 MemoryStore + FTS5 BM25 회상
- Stage 2: BGE-M3 임베딩(ORT/ONNX) + usearch HNSW ANN
- Stage 3: hybrid RRF (BM25 + 벡터 융합)

**v0.9 범위 = Stage 0 + Stage 1** (비-ML 기반층). Stage 2~3(ML 스택: ort/usearch/ndarray/tokenizers + 모델 다운로드 + recall_eval 비결정성 전략)은 무게·위험이 달라 **v0.10**으로 분리.

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | **feature flag 격리**: 검색코어 전체를 `friend-engine` feature 뒤(기본 off). 기본 빌드·CI·골든 경로는 rusqlite/lindera(추후 ort/usearch) 미컴파일 → lean·ML-free 유지. feature off면 v0.8 인메모리 회상 그대로 |
| INV-2 | **골든 바이트 동일**: 회상은 라이브 경로 전용(v0.8 불변식 - driver/`PersonaRuntime`/headless는 recall 미주입). SQLite/형태소도 라이브 recall 경로에만 → v0.1~v0.8 골든 5종 바이트 동일 |
| INV-3 | **회상 결정성**: 같은 사건열+쿼리 → 같은 회상. FTS5 `bm25()` 랭킹은 결정적. 테스트는 `:memory:` SQLite(디스크·벽시계 없음). 논리 ts 유지 |
| INV-4 | **참여 기반 격리 유지**(v0.8 INV-2): 캐릭터는 참여한 방의 사건만 회상. FTS 쿼리가 참여 방으로 먼저 좁힘. recall_eval 격리 케이스로 자동 검증 |
| INV-5 | **회상은 검색만**: 엔진 결정(gate/rrf/hawkes/cooling 입력)에 불사용. 생성 프롬프트(회상 슬롯)에만 |
| INV-6 | v0.8의 210 tests + 스모크 8종 유지(+ v0.9 게이트). `recall(persona, query, k)` **공개 시그니처 보존**(내부만 교체) |

## 2. Goals / Non-goals

### Goals
- (G1) **`friend-engine` feature scaffold + Lindera 형태소**(Stage 0): optional dep `lindera`(embed-ko-dic). 신규 토크나이저(seCall `tokenizer.rs` Lindera 경로 lift: keep-tags NNG/NNP/NNB/VV/VA/SL). feature on이면 회상 토큰화에 형태소 사용, off면 v0.8 토큰중복.
- (G2) **SQLite 영속 MemoryStore**(Stage 1a): optional dep `rusqlite`(bundled). 사건/참여를 SQLite에 저장(파일=영속, `:memory:`=테스트). `record`/`join`이 SQL 기록. 라이브=파일 경로, 세션 넘어 기억 유지.
- (G3) **FTS5 BM25 회상**(Stage 1b): seCall `bm25.rs` 패턴 lift. `turns_fts`류 FTS5 가상테이블에 Lindera 토큰 인덱싱, `recall`이 참여 방 필터 + `bm25()` 상위 K. v0.8 토큰중복 대체(feature on).
- (G4) **회상 평가 하네스 갱신**: `recall_eval`이 새 검색층(형태소+FTS5 BM25)을 결정적으로 채점(재현율/정확도/참여 격리). 형태소 도입 효과 측정.
- (G5) **v0.9 게이트 + 마감**: smoke_v9(골든 보존 + feature on/off 양쪽 + 결정성 + 영속 roundtrip + 참여 격리 + content 게이팅).

### Non-goals (v0.10 이후)
- ❌ BGE-M3 임베딩(ORT/ONNX) + usearch HNSW ANN(Stage 2) - ML 스택·CoreML·모델 다운로드·RAM 측정.
- ❌ hybrid RRF 융합(Stage 3) - Stage 2 의존.
- ❌ recall_eval의 임베딩 비결정성 전략(고정모델+허용오차 / mock embedder) - Stage 2와 함께.
- ❌ seCall의 chunker(현 발화는 짧아 불요), query_expand(claude CLI 호출), graph/wiki/ingest.
- ❌ 망각·주관적 저장·인물기억(tunaSalon 고유, 별도 트랙).
- ❌ 임베딩 백엔드 결정(ORT vs cloud)은 Stage 2 플랜에서. **단 로컬 ollama 금지**는 그때도 유효 → ORT/ONNX(in-process, CoreML) 유력.

## 3. 데이터 모델 / 아키텍처 델타

| 구조 | 변경 |
|------|------|
| `Cargo.toml` | `[features] friend-engine = ["dep:lindera", "dep:rusqlite"]`(기본 off). `lindera`/`rusqlite`(bundled) optional |
| `src/memory.rs` | `recall(persona,query,k)`/`record`/`join`/`format_recall` 공개 시그니처 유지. 내부: feature off=v0.8 Vec+토큰중복, on=SQLite+FTS5 BM25. `cfg(feature="friend-engine")`로 backend 분기(또는 trait + 두 impl) |
| 신규 `src/tokenize_ko.rs`(가칭, feature-gated) | seCall `tokenizer.rs` Lindera 경로 lift. keep-tags 필터. `flow.rs` 토크나이저는 **불변**(FlowMeter/골든 보존) |
| 신규 SQLite 스키마(feature-gated) | seCall `store/schema.rs` 참고 축소: `memories(id,room,ts,speaker,content)` + `participation(room,persona)` + FTS5 `memories_fts(content, room UNINDEXED, mem_id UNINDEXED)`. 임베딩 컬럼(BLOB)은 Stage 2에서 추가 |
| `live.rs` | feature on이면 파일 경로 SQLite MemoryStore 생성(영속), off면 v0.8 인메모리. 라이브 record/recall 경로만 |
| `tests/recall_eval.rs` | 새 검색층 채점. `:memory:` SQLite로 결정적. 형태소 효과 비교 |
| `driver.rs`/headless | **불변**(recall 미주입 → 골든 보존). 변경 없음 |

설계 메모: feature off 시 v0.8 인메모리 경로가 그대로 살아 있어야 함(기본 `--chat`/`--llm`은 flag 없이도 동작). on은 라이브 영속 + 정밀 회상 opt-in(= `--llm` opt-in 철학과 동형).

## 4. Subtasks

| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| 43 | feature scaffold + Lindera 형태소(Stage 0) | `friend-engine` feature + optional `lindera`. 형태소 토크나이저(keep-tags) lift. feature on이면 회상 토큰화에 사용. `flow.rs` 불변. 단위 테스트(한국어 형태소 분해), feature off 빌드/테스트 그대로 | 중(lindera embed-ko-dic 빌드시간·바이너리 크기, cfg 분기) | v0.8 |
| 44 | SQLite 영속 MemoryStore(Stage 1a) | optional `rusqlite`(bundled). 스키마(memories+participation, FTS5). record/join 영속. `:memory:`(테스트)/파일(라이브). v0.8 인메모리는 feature off로 보존 | 중~높음(새 의존성, cfg 두 impl, 영속 경로) | 43 |
| 45 | FTS5 BM25 회상 + eval(Stage 1b) | recall이 참여 필터 + FTS5 `bm25()` 상위 K(Lindera 토큰 인덱싱). recall_eval 새 검색층 결정적 채점 + 형태소 효과 | 중(FTS 쿼리·참여 격리·결정성) | 44 |
| 46 | v0.9 게이트 + 마감 | smoke_v9: 골든 보존(feature off=v0.8, 기본 빌드 lean) + feature on 결정성/영속 roundtrip/참여 격리/content 게이팅. README/CLAUDE/index bump | 낮음~중 | 45 |

Phase A(43 형태소) → B(44 영속, 45 BM25 회상) → C(46 게이트). 완료: task-46 + 골든 5종 보존 + feature on/off 양쪽 green.

## 5. v0.9 완료 기준

- **기본 빌드(feature off)**: `cargo run`(LLM off)·headless 골든 5종 바이트 동일, rusqlite/lindera 미컴파일, v0.8 인메모리 회상 동작.
- **feature on(`--features friend-engine`)**: 라이브에서 페르소나가 SQLite 영속 + 형태소 FTS5 BM25로 지난 대화 회상(같은 방 참여 캐릭터만), 세션 넘어 기억 유지.
- 회상 평가 하네스: 형태소+BM25 검색층이 재현율/정확도/참여 격리로 자동 채점, 결정적(`:memory:`).
- 회상 결정성(같은 사건열+쿼리 → 같은 결과). `recall` 공개 시그니처 불변.

## 6. 위험과 대응

| 위험 | 대응 |
|------|------|
| 골든 깨짐 | 회상=라이브 전용(v0.8 INV). feature off=v0.8 경로. driver/headless 불침투. 골든 5종 재확인(필수, 양 feature) |
| 새 의존성으로 기본 빌드 무거워짐 | `friend-engine` feature 뒤로 격리(기본 off). CI/골든은 ML-free·SQLite-free |
| cfg 두 impl 분기 복잡 | `recall` 공개 시그니처 고정. 내부만 cfg. 가능하면 backend trait 1개 + 두 impl로 단순화 |
| FTS5 비결정/플랫폼차 | `bm25()` 결정적. 테스트 `:memory:`(디스크·벽시계 없음). 논리 ts |
| Lindera 빌드시간·크기(embed-ko-dic) | feature-gated라 기본 빌드 무영향. seCall에서 검증된 dep(동일 버전대) |
| 영속 경로 동시성(라이브 워커 스레드) | rusqlite WAL + 단일 커넥션 조율(seCall 패턴). 라이브 record/recall은 tick 경로(순차) |
| 회상이 엔진 결정에 샘 | 회상은 생성 프롬프트만(INV-5). gate/rrf/hawkes/cooling 입력 불사용 |

## 7. 산출물

- 이 문서(PLAN v0.9). 구현 시 §4를 `salon-engine-v9-task-43..46.md`로 분해.
- v0.9 한 줄: 본인 레포 seCall의 검색코어를 단계 lift해, 페르소나 장기기억을 인메모리·토큰중복에서 **SQLite 영속 + 한국어 형태소 FTS5 BM25**로 끌어올린다(feature off면 v0.8 그대로·골든 보존). BGE-M3 의미검색은 v0.10.

## 8. v0.10 예고 (Stage 2~3, 참고)

- Stage 2: BGE-M3 임베딩(seCall `embedding.rs` ORT/ONNX 경로, **로컬 ollama 금지→ORT in-process + CoreML 유력**) + usearch HNSW(`ann.rs`, cosine, `.usearch` 영속) + `model_manager.rs`(HF서 ONNX 다운로드). 임베딩 컬럼(BLOB) 추가.
- Stage 3: hybrid RRF(`hybrid.rs`, k=60)로 BM25 + 벡터 융합 + 세션/방 다양성.
- 핵심 난제: recall_eval 결정성 - 임베딩 float 비결정 → 고정 모델 + 허용오차 또는 mock embedder로 분리. RAM/로드 랙 측정(bge-m3 ~1.2GB 1회 로드).
