---
title: "Salon v0.3 Task 15: PersonaRuntime + FakeBackend + content 배선"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v3.md
task_id: "15"
depends_on: []
parallel_group: ""
---

# Task 15 - PersonaRuntime 트레이트 + FakeBackend + content 배선

plan `salon-engine-v3.md` subtask 15. 내용 생성을 Contract(트레이트)로 추상화한다. 이번 task는 **순수 Rust, 네트워크 없음**. FakeBackend는 결정적이고 content를 만들지 않아 v0.2 골든을 보존한다. 실제 LLM(OllamaBackend)은 task-16.

## Changed files

- `src/runtime.rs` - 신규. `PersonaRuntime` 트레이트 + `FakeBackend`.
- `src/model.rs` - 수정. `Event`에 `content: Option<String>` 추가. 모든 Event 리터럴에 `content: None` 추가(기계적).
- `src/sink.rs` - 수정. `ObservationRecord`에 `utterance: Option<String>` 추가, `#[serde(default, skip_serializing_if = "Option::is_none")]`. 리터럴에 `utterance: None` 추가.
- `src/driver.rs` - 수정. 화자 확정 후 runtime으로 발화 생성, Event.content/record.utterance에 반영.
- `src/main.rs` - 수정. 기본 runtime = FakeBackend 배선.
- `src/lib.rs` - 수정. `pub mod runtime;`.

## Change description

- `PersonaRuntime` 트레이트: `fn generate(&mut self, speaker: &PersonaId, history: &[Event], tick: u64, rng: &mut ChaCha8Rng) -> Option<String>`. 반환이 Some이면 그 화자의 발화 텍스트, None이면 내용 없음(라벨만).
- `FakeBackend`: `generate`가 **항상 None**을 반환한다(내용 없음 = v0.1/v0.2 동작). 결정적. 이게 기본이라 골든이 보존된다.
- driver: 화자(chosen) 확정 후 `let content = runtime.generate(&chosen, &state.history, tick, &mut rng);`. Event를 만들 때 `content`를 넣고(make_utterance 유지하되 content 필드 세팅), record.utterance = content.
  - **결정성**: FakeBackend는 rng를 소비하지 않거나(None 반환 전 소비 X) v0.2와 동일한 rng 소비 순서를 유지해야 골든이 보존된다. 안전책: FakeBackend.generate는 rng를 건드리지 않고 None 반환. driver의 기존 rng 소비(rrf, make_utterance)는 그대로.
- `Event.content: Option<String>`(기본 None). `ObservationRecord.utterance: Option<String>` + serde 생략(None이면 JSON에서 빠짐).
- TUI(있으면): events 로그에 utterance가 Some이면 텍스트 표시, None이면 화자 이름만(지금처럼). (선택, 깨지면 task-16에서.)
- 가드레일: `unwrap`/`panic` 금지. 결정성 유지. `tunaflow:` 마커 금지.

## Dependencies

- v0.2 전체.

## Verification

```bash
cargo test
cargo run -- --headless --seed 42 --ticks 80 --theta 0.65   # FakeBackend → utterance 없음, v0.2 골든 동일
```

- `cargo test` 전체 green(50 유지 + runtime 단위 테스트: FakeBackend.generate가 None, 트레이트 객체로 호출 가능).
- **α=0 + FakeBackend 골든 5종 바이트 동일**(빌드 후 명시적 순차 실행). record JSON에 `"utterance"`가 없어야 함(`grep -c utterance` == 0).
- 트레이트가 dyn으로 driver에 주입 가능(OllamaBackend가 task-16에서 같은 트레이트로 들어올 수 있게).

## Risks

| 위험 | 회피 |
|---|---|
| content/utterance 필드로 골든 깨짐 | serde 생략(None) + FakeBackend가 None → 필드 생략. 골든 5종 재확인 |
| FakeBackend가 rng 소비 순서 바꿔 골든 깨짐 | Fake.generate는 rng 미사용 + None. driver 기존 rng 흐름 불변 |
| Event/Record 리터럴 churn | 모든 생성처에 content/utterance: None 추가, 빌드로 확인 |
| 트레이트 시그니처가 OllamaBackend에 안 맞음 | history+tick+rng를 넘겨 LLM이 맥락 쓸 수 있게. Result는 task-16에서 에러 처리 추가 가능하게 Option로 시작 |
