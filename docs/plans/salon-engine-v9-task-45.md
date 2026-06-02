---
title: "Salon v0.9 Task 45: 파일 영속 (cross-session) (friend-engine, Stage 1b)"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v9.md
task_id: "45"
depends_on: ["44"]
parallel_group: ""
status_note: "done 2026-06-03. 리뷰 통과(diff 정독+독립 재검증). open(path)/default_db_path()/live_store() 추가, new()=with_store(:memory:) 위임. 영속은 main.rs --chat만(테스트 격리: 실행 전후 ~/.local/share/tunaSalon clean 확인). 골든 5/5, 영속 roundtrip green. 기본 경로 ~/.local/share/tunaSalon/memory.db(SALON_MEMORY_DB override)."
---

# Task 45 - 파일 영속 (cross-session) (Stage 1b)

plan `salon-engine-v9.md` Stage 1b. task-44의 `:memory:` SQLite 스토어에 **파일 영속**을 더해 페르소나가 세션을 넘어 기억하게 한다. 사용자 결정(2026-06-03): **기본 경로 자동 영속** — `~/.local/share/tunaSalon/memory.db`(`$SALON_MEMORY_DB`로 override), friend-engine on이면 별다른 설정 없이 자동.

**핵심 격리 원칙**: 영속은 **실제 `--chat`(main.rs) 경로에서만**. 모든 테스트/스모크는 `MemoryStore::new()`(:memory:)를 그대로 써서 디스크에 쓰지 않는다(테스트가 실제 데이터 디렉터리를 오염·상호간섭하면 안 됨). 그래서 `LiveSession::new()`는 :memory:를 유지하고, 영속 스토어는 별도 생성자로 주입한다.

## Changed files

- `src/memory.rs`(feature on) - 수정. 추가:
  - `pub fn open(path: &Path) -> rusqlite::Result<Self>`: 파일 SQLite 열기. 부모 디렉터리 `create_dir_all`, `PRAGMA journal_mode=WAL`, 스키마 실행. **스키마는 `IF NOT EXISTS`**(기존 DB 재오픈 시 안전). `new()`(:memory:)와 `open()`이 **공유 DDL**을 쓰도록 리팩터(예: `fn init_schema(conn)`).
  - `pub fn default_db_path() -> Option<PathBuf>`: `$SALON_MEMORY_DB` 있으면 그 경로; 없으면 `$HOME/.local/share/tunaSalon/memory.db`; HOME도 없으면 `None`. (순수 — 디스크 안 건드림.)
  - `pub fn live_store() -> MemoryStore`: feature on = `match default_db_path() { Some(p) => open(p).unwrap_or_else(|e| { eprintln!(경고); new() }), None => new() }`. feature off = `new()`(Vec). **라이브(main.rs) 전용 헬퍼** — 테스트에서 호출 금지(실제 경로 사용).
- `src/live.rs` - 수정. `LiveSession::with_store(config, personas, seed, pool, human_id, store: MemoryStore) -> ...`(신규, 기존 new 본문 + 주입 store). `new(...)`는 `with_store(..., MemoryStore::new())`로 위임(시그니처·동작 불변 → 모든 기존 테스트/스모크는 :memory: 그대로). room join 등은 with_store 안에서.
- `src/main.rs` - 수정. `--chat` 경로(현 `LiveSession::new(config, demo_personas(), cli.seed, pool, "나")`)를 `LiveSession::with_store(config, demo_personas(), cli.seed, pool, "나", salon::memory::live_store())`로. feature off에선 live_store()=new()라 동작 동일. **chat_demo(examples)는 미변경**(:memory: 데모로 충분).
- `Cargo.toml` - 변경 없음(rusqlite/lindera는 task-44에서 friend-engine에 이미 포함). `dirs` 같은 dep 추가 금지(HOME 직접 사용).

## Change description

- **영속 동작**: friend-engine on + `--chat` 실행 → `live_store()`가 default 경로 파일 SQLite 열고(없으면 생성, 있으면 재사용) WAL. record/recall이 그 파일에 누적 → 다음 실행에서 지난 대화 회상. `$SALON_MEMORY_DB`로 경로 변경 가능.
- **테스트 격리(필수)**: `LiveSession::new()`(= with_store(MemoryStore::new()))는 :memory:. live.rs/smoke_v5/v7/v8 + recall_eval은 전부 new()/:memory: → **디스크 쓰기 0**. `cargo test --features friend-engine`가 `~/.local/share/tunaSalon`를 만들면 안 된다(검증 항목).
- **결정성/골든**: 영속은 라이브 전용. driver/headless/recall_eval 불침투(:memory:/Vec) → 골든 바이트 동일. recall_eval 결정적 유지.
- **idempotent 스키마**: `CREATE TABLE IF NOT EXISTS` + `CREATE VIRTUAL TABLE IF NOT EXISTS`. 기존 memory.db 재오픈해도 에러 없이 누적.
- **WAL 사이드카**(`-wal`/`-shm`)는 정상. 단일 프로세스/스레드 writer.
- 가드: 요청 파일만. recall은 여전히 라이브 경로만(엔진 결정 불침투). open()은 Result(호출처 live_store가 fallback 처리, loud eprintln). 런타임 쿼리 panic 금지.

## Verification

```bash
cargo build
cargo test
cargo build --features friend-engine
cargo test  --features friend-engine
# 테스트가 실제 데이터 디렉터리를 만들지 않았는지 확인(핵심):
ls ~/.local/share/tunaSalon/ 2>/dev/null && echo "!! 테스트가 실제 경로 오염 !!" || echo "clean (테스트는 :memory: 사용)"
# 골든 5종(기본 빌드, 명시적 순차 — set -- 금지)
cargo build
cargo run -q -- --headless --seed 42 --ticks 120 --theta 0.40 | diff - /tmp/salon_golden/s42_t040.ndjson && echo s42_t040 OK
cargo run -q -- --headless --seed 42 --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
cargo run -q -- --headless --seed 42 --ticks 120 --theta 0.78 | diff - /tmp/salon_golden/s42_t078.ndjson && echo s42_t078 OK
cargo run -q -- --headless --seed 7   --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s7_t065.ndjson  && echo s7_t065 OK
cargo run -q -- --headless --seed 99  --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s99_t065.ndjson && echo s99_t065 OK
```

- 기본/feature 양쪽 `cargo test` green. **테스트가 `~/.local/share/tunaSalon`를 만들지 않음**(위 ls 체크).
- 골든 5종 바이트 동일. recall_eval(:memory:) 결정적 green.
- 신규 테스트(feature on):
  - **영속 roundtrip**(핵심): `std::env::temp_dir()`의 고유 임시 경로로 `open()` → record+join → `drop(store)` → 같은 경로 `open()` 재오픈 → recall에 사건 존재. 임시 파일(+`-wal`/`-shm`) 시작 시 정리. **default 경로 사용 금지**(temp만).
  - `default_db_path()` 해석: `$SALON_MEMORY_DB` 설정 시 그 경로 반환(순수, 디스크 무접촉). (env 조작 테스트는 직렬/주의 — 또는 함수 분리해 입력 주입.)
- 수동 라이브(선택): `SALON_CLOUD_ONLY=1 cargo run --features friend-engine -- --chat` → 대화 후 종료 → 재실행 시 회상. `~/.local/share/tunaSalon/memory.db` 생성 확인.

## Risks

| 위험 | 회피 |
|---|---|
| 테스트가 실제 데이터 dir 오염/상호간섭 | `LiveSession::new()`=:memory: 유지(모든 테스트). 영속은 main.rs `with_store(live_store())`만. 영속 테스트는 temp 경로. ls 체크로 검증 |
| 골든 깨짐 | 영속 라이브 전용. driver/headless/recall_eval=:memory:/Vec. 골든 재확인 |
| 기존 DB 재오픈 에러 | 스키마 `IF NOT EXISTS`. open() Result + live_store fallback |
| HOME 미설정 | default_db_path None → live_store가 new()(:memory:) + eprintln |
| WAL 사이드카 잔여 | 정상 동작. 영속 테스트 cleanup이 *.db/-wal/-shm 제거 |
| LiveSession 시그니처 변경 파급 | new() 시그니처·동작 불변(with_store로 위임). 기존 호출처 무변경 |
| record() silent INSERT 실패(task-44 잔여) | 이번에 한해 유지 가능하나, 영속에선 누락이 더 아픔 → eprintln 경고 추가 검토(선택, 과하면 생략) |
