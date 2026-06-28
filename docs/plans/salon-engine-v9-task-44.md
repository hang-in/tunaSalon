---
title: "Salon v0.9 Task 44: :memory: SQLite + FTS5 BM25 회상 (friend-engine, Stage 1a)"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v9.md
task_id: "44"
depends_on: ["43"]
parallel_group: ""
status_note: "done 2026-06-03. 리뷰 통과(코드 정독+독립 재검증). feature off=v0.8 Vec(owned), on=:memory: SQLite+FTS5 OR-MATCH BM25. 골든 5/5, 기본 빌드 rusqlite 미컴파일, recall_eval feature-on 9/9(SSOT top-3 유지). 후속(task-45): record()가 INSERT 오류를 silent swallow(let _=) - 파일 영속 단계에서 로깅/트랜잭션 검토."
---

# Task 44 - :memory: SQLite + FTS5 BM25 회상 (Stage 1a)

plan `salon-engine-v9.md` Stage 1a. `friend-engine` feature가 켜지면 `MemoryStore`를 인메모리(`:memory:`) **SQLite + FTS5 BM25**로 교체한다. v0.8 토큰중복(intersection count)을 진짜 BM25 랭킹으로 끌어올린다. **이번 단계는 디스크 영속 없음**(`:memory:`만) → 결정적·DB 위치 결정 불요. 파일 영속(cross-session)은 task-45. feature off는 v0.8 Vec 구현 그대로(골든·기본빌드 보존).

lift 원본: seCall `crates/secall-core/src/store/{schema.rs,search_repo.rs}` + `search/bm25.rs`(FTS5 패턴). **단, MATCH 구성은 AND→OR로 바꾼다(아래 §주의).**

## Changed files

- `Cargo.toml` - 수정. `friend-engine = ["dep:lindera", "dep:rusqlite"]`로 확장. `rusqlite = { version = "0.31", features = ["bundled"], optional = true }`.
- `src/memory.rs` - 수정(핵심). `MemoryEvent`·`format_recall`은 공유. `MemoryStore`를 **cfg 두 구현**으로:
  - `#[cfg(not(feature = "friend-engine"))]`: 기존 v0.8 Vec 구현(거의 그대로, 단 recall 반환형 변경 아래 참조).
  - `#[cfg(feature = "friend-engine")]`: SQLite(`:memory:`) + FTS5 구현(신규).
  - **API 변경(양쪽 동일)**: `recall(&self, persona, query, k) -> Vec<MemoryEvent>`(owned), `format_recall(events: &[MemoryEvent]) -> Option<String>`. (DB 백엔드는 row를 owned로 만들어 반환하므로 `&MemoryEvent` 차용 불가.)
- `src/live.rs` - 필요 시 미세 수정. `MemoryStore::format_recall(&store.recall(...))`는 `&Vec<MemoryEvent>`→`&[MemoryEvent]` 자동 강제라 대개 무변경. 컴파일 확인 후 필요한 곳만.
- `tests/recall_eval.rs` - 수정. `recall_at_k(results: &[MemoryEvent], ...)`로 시그니처 변경 + 본문(차용→owned). `format_recall(&results)`는 그대로. 결정성 테스트(r1==r2)는 `Vec<MemoryEvent>` 비교로 더 단순.
- `src/memory.rs` 단위 테스트 - `format_recall` 테스트의 `vec![&e1, &e2]`→`vec![e1, e2]`(owned). recall 테스트는 owned 반환에 맞게(대부분 `result[0].content` 등 그대로 동작).

## Change description

### feature ON: SQLite(:memory:) + FTS5 구조

스키마(`MemoryStore::new()`에서 `:memory:` 연결 + DDL 실행):
```sql
CREATE TABLE memories (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  room TEXT NOT NULL, ts INTEGER NOT NULL,
  speaker TEXT NOT NULL, content TEXT NOT NULL
);
CREATE TABLE participation (
  room TEXT NOT NULL, persona TEXT NOT NULL, UNIQUE(room, persona)
);
CREATE VIRTUAL TABLE memories_fts USING fts5(
  tokens, room UNINDEXED, mem_id UNINDEXED, tokenize='unicode61'
);
```
- `new() -> Self`(시그니처 유지, infallible): `:memory:` 열고 DDL. 고정 DDL + 런타임 입력 없음이므로 실패 시 `.expect("in-memory sqlite schema must init")` 허용(런타임 입력 panic 아님).
- `join(&mut self, room, persona)`: `INSERT OR IGNORE INTO participation(room, persona)`.
- `record(&mut self, event)`: `INSERT INTO memories(...)` → `last_insert_rowid()`=mem_id. `tokens = tokenize_ko::morphological_tokens(&event.content).join(" ")`. `INSERT INTO memories_fts(tokens, room, mem_id) VALUES(?,?,?)`. 화자 자동 참여: `INSERT OR IGNORE INTO participation(room, speaker)`.
- `recall(&self, persona, query, k) -> Vec<MemoryEvent>`:
  1. `k==0` → `vec![]`.
  2. rooms = `SELECT room FROM participation WHERE persona=?1`. 비면 `vec![]`.
  3. `q = morphological_tokens(query)`. 비면 `vec![]`.
  4. **OR-MATCH 구성(주의)**: 각 토큰을 큰따옴표로 감싸고 내부 `"`는 `""`로 escape, `" OR "`로 연결.
     예) `["비","오"]` → `"비" OR "오"`. (FTS5 기본은 공백=AND인데 회상은 "토큰 일부라도 겹침"이라 **OR** 필수. 따옴표로 키워드/연산자 오해 방지.)
  5. SQL:
     ```sql
     SELECT m.id, m.room, m.ts, m.speaker, m.content, bm25(memories_fts) AS score
     FROM memories_fts JOIN memories m ON m.id = memories_fts.mem_id
     WHERE memories_fts.tokens MATCH ?1
       AND m.room IN (<rooms placeholders>)
     ORDER BY score ASC, m.ts DESC, m.id DESC
     LIMIT ?
     ```
     bm25() 오름차순 = 가장 관련 먼저. 동점은 ts DESC(최신)·id DESC로 **결정적 정렬**.
  6. row → `MemoryEvent{room,ts,speaker,content}` owned. 반환.

### feature OFF: v0.8 Vec 구현 유지
- 기존 로직 그대로(참여 격리 + `flow::tokenize` intersection count + ts 내림차순). **단 recall 반환형만 `Vec<MemoryEvent>`(owned)로** - 후보를 `.clone()`해 owned로 반환(상위 k개라 비용 무시). task-43의 `recall_tokens` 헬퍼는 feature off 전용(=`flow::tokenize`)으로 정리; feature on은 Vec 경로를 안 쓰므로 morphological 분기 제거(SQLite가 `morphological_tokens` 직접 사용).

### 불변/가드
- **골든**: recall은 라이브 경로 전용(v0.8). driver/headless 불침투 → feature 유무·SQLite와 무관하게 골든 바이트 동일. feature off 기본 빌드엔 rusqlite 미컴파일.
- **결정성**: `:memory:` + 고정 insert 순서 + `ORDER BY score, ts DESC, id DESC` → 같은 사건열+쿼리 → 같은 결과. recall_eval 결정성 테스트로 검증.
- **참여 격리 유지**(INV-2): `m.room IN (참여 방)`으로 먼저 좁힘.
- **Send/Sync**: feature on `MemoryStore`는 `rusqlite::Connection` 보유(Send, !Sync). LiveSession은 이를 단일 스레드(tick)에서만 소유·사용(워커는 recall 문자열만 받음). `cargo build --features friend-engine` 컴파일로 LiveSession 제약 확인.
- 요청 파일만 수정. unwrap/panic은 `:memory:` DDL init 외 금지(런타임 쿼리는 `?`로 에러 전파 또는 빈 결과).

## Verification

```bash
cargo build
cargo test
cargo build --features friend-engine
cargo test  --features friend-engine
# 골든 5종(기본 빌드, 명시적 순차 - zsh word-split 주의: set -- 금지)
cargo build
cargo run -q -- --headless --seed 42 --ticks 120 --theta 0.40 | diff - /tmp/salon_golden/s42_t040.ndjson && echo s42_t040 OK
cargo run -q -- --headless --seed 42 --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
cargo run -q -- --headless --seed 42 --ticks 120 --theta 0.78 | diff - /tmp/salon_golden/s42_t078.ndjson && echo s42_t078 OK
cargo run -q -- --headless --seed 7   --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s7_t065.ndjson  && echo s7_t065 OK
cargo run -q -- --headless --seed 99  --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s99_t065.ndjson && echo s99_t065 OK
```

- 기본/feature 양쪽 `cargo test` green. **`recall_eval`이 feature on(SQLite+FTS5+형태소)에서도 green** - SSOT가 top-3 안에. 만약 BM25 랭킹으로 어떤 단언이 실패하면 **느슨하게 고치지 말고** 원인 조사(참여 격리·OR-MATCH·tokens 인덱싱 점검). SSOT-in-top3는 형태소+BM25면 유지돼야 함.
- 골든 5종 바이트 동일.
- 신규/갱신 테스트(feature on, `:memory:`):
  - record→recall roundtrip(저장한 사건이 회상됨), 참여 격리(미참여 방 회상 0), 결정성(2회 동일), OR 의미(쿼리 토큰 일부만 겹쳐도 회상), k=0/빈쿼리/미참여 → 빈 Vec.
  - BM25 랭킹 효과(선택): 더 관련 높은 사건이 상위.

## Risks

| 위험 | 회피 |
|---|---|
| MATCH 기본 AND로 회상 거의 0 | **OR-MATCH 명시**(따옴표 escape). 단위 테스트로 "토큰 일부 겹침 회상" 검증 |
| 골든 깨짐 | recall 라이브 전용. driver/headless 불변. feature off=Vec. 골든 5종 재확인 |
| 기본 빌드 무거워짐 | rusqlite는 `friend-engine` feature 뒤(optional). 기본 미컴파일 |
| API 변경 누락 호출처 | recall→owned, format_recall(&[MemoryEvent]). live.rs/recall_eval/단위테스트 전부 갱신 후 양쪽 빌드 |
| Connection !Sync로 LiveSession 컴파일 실패 | 단일 스레드 소유. feature 빌드로 확인. 실패 시 store 접근 위치 조정(워커 미전달) |
| FTS 결정성/동점 | `:memory:` + `ORDER BY score, ts DESC, id DESC`. recall_eval 결정성 테스트 |
| recall_eval feature-on 실패 | 원인 조사 우선(랭킹 변화는 정상이나 SSOT-in-top3 유지돼야). 임계 임의 완화 금지 |
