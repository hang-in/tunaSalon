---
title: "Salon v0.8 Task 41: 회상 주입 배선 (LiveSession 생성 경로)"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v8.md
task_id: "41"
depends_on: ["39"]
parallel_group: ""
---

# Task 41 - 회상 주입 배선

plan `salon-engine-v8.md` subtask 41. task-39 회상을 **라이브 채팅 생성 경로에만** 연결한다. 핵심 전략: **recall은 LiveSession 경로에만 threading하고, `PersonaRuntime::generate`(driver/headless) 경로는 안 건드린다** → 골든/결정 경로 완전 보존. LiveSession이 사건 저장(사람 발화 포함) + 생성 전 회상 검색 + 회상 슬롯 주입.

## Changed files

- `src/pool.rs` - 수정. `Backend::generate`와 `BackendPool::generate_one`에 `recall: Option<&str>` 인자 추가(끝). `generate_batch`·`PersonaRuntime::generate`(BackendPool)는 `None` 전달(벤치/driver는 회상 없음).
- `src/ollama.rs` - 수정. `generate_shared(speaker, history, tick, recall)` → `assemble_user_prompt(recent, recall)`(슬롯 이미 있음). 트레이트 `PersonaRuntime::generate`는 `generate_shared(..., None)`.
- `src/openai.rs` - 수정. `generate(speaker, history, tick, recall)` + assemble에 회상 슬롯 추가(messages user 메시지에 `[기억]` 섹션, ollama와 동일 위치).
- `src/live.rs` - 수정. `MemoryStore` + room id 보유. `new`에서 페르소나+사람 room join. `submit_human`이 사람 발화 record. `poll_generation`이 도착 발화 record. `tick`에서 화자 선택 시 `store.recall(chosen, query, k)`→format→job에 recall(String) 포함. 워커가 `generate_one(..., recall.as_deref())`.

## Change description

- **threading(라이브 전용)**: `generate_one(speaker, history, tick, recall)` → `Backend::generate(..., recall)` → `OllamaBackend::generate_shared(..., recall)` / `OpenAIBackend::generate(..., recall)` → assemble가 회상 슬롯 채움. `generate_batch`/`PersonaRuntime::generate`는 `None`(회상 없음).
- **driver/headless 불변**: `driver::run`은 `PersonaRuntime::generate`(recall 없음) 그대로 사용 → 회상 미주입 → **골든 바이트 동일**. FakeBackend도 그대로.
- **LiveSession**:
  - `MemoryStore` + `room: String`("salon" 등) 보유. `new`: 모든 페르소나 + human_speaker_id를 room에 `join`.
  - `submit_human(text)`: 기존 HumanChannel.speak + `store.record(MemoryEvent{room, ts(tick_count), "나", text})`(사람 발화도 기억 대상).
  - `poll_generation`: 발화 content가 Some이면 `store.record(MemoryEvent{room, ts, speaker, content})`.
  - `tick` 디스패치 시: `query` = 최근 history content 합침(현재 맥락). `recall = MemoryStore::format_recall(&store.recall(chosen, &query, K))`(예 K=3). job에 `recall: Option<String>` 포함. 워커가 `pool.generate_one(speaker, &history, tick, recall.as_deref())`.
- 결정성: recall 계산은 결정적(task-39). 라이브는 어차피 비결정(LLM). 골든은 driver 경로라 무관.
- 가드레일: **골든 보존 최우선**(driver 경로 안 건드림). 회상은 생성 프롬프트에만(엔진 결정 gate/rrf/hawkes/cooling 입력에 불사용). unwrap/panic 금지. 호출처 전부 갱신(None).

## Dependencies

- task-39(MemoryStore/recall). v0.7 생성 경로(pool/ollama/openai/live).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 120 --theta 0.40 | diff - /tmp/salon_golden/s42_t040.ndjson && echo s42_t040 OK
cargo run -- --headless --seed 42 --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
cargo run -- --headless --seed 42 --ticks 120 --theta 0.78 | diff - /tmp/salon_golden/s42_t078.ndjson && echo s42_t078 OK
cargo run -- --headless --seed 7   --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s7_t065.ndjson  && echo s7_t065 OK
cargo run -- --headless --seed 99  --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s99_t065.ndjson && echo s99_t065 OK
```

- `cargo test` green(generate 시그니처 갱신 후 pool/ollama/openai/live 테스트 + 호출처 None). 신규 테스트(네트워크 없이):
  - (1) ollama/openai `build_request_body`/assemble가 recall=Some이면 회상 섹션 포함, None이면 생략.
  - (2) LiveSession이 submit_human + 도착 발화를 store에 record(오프라인 stub로 record 호출 확인), 미참여 격리 유지.
  - (3) `PersonaRuntime::generate`(driver 경로)는 recall 미주입(None) - 기존 동작.
- **골든 5종 바이트 동일**(driver/PersonaRuntime 경로 불변 → 핵심). 안 맞으면 driver 경로에 recall이 샌 것.

## Risks

| 위험 | 회피 |
|---|---|
| 골든 깨짐 | recall은 라이브(generate_one) 경로만. driver는 PersonaRuntime::generate(None) 그대로. 골든 5종 재확인(필수) |
| 시그니처 churn | generate_one/Backend::generate/generate_shared/openai generate + 호출처 None. generate_batch/PersonaRuntime None. 빌드 |
| 회상이 엔진 결정에 새어듦 | 회상은 생성 프롬프트(assemble)에만. gate/rrf/hawkes/cooling 입력 불사용 |
| openai 회상 슬롯 위치 | ollama와 동일(공유 로그 뒤, 지시 앞). user 메시지에 [기억] 섹션 |
| 사람 발화 미기록으로 회상 누락 | submit_human도 record(사람 말도 기억 대상) |
