---
title: "Salon v0.3 Task 16: OllamaBackend (HTTP)"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v3.md
task_id: "16"
depends_on: ["15"]
parallel_group: ""
---

# Task 16 - OllamaBackend (실제 LLM, HTTP)

plan `salon-engine-v3.md` subtask 16. task-15의 `PersonaRuntime`을 실제 Ollama 호출로 구현한다. 로컬(localhost) + cloud(ollama.com + 키) 모두. **opt-in(`--llm`)이라 기본 실행은 FakeBackend 그대로 → 골든 보존.** 보안(키 비노출)과 에러/타임아웃이 핵심.

## Changed files

- `Cargo.toml` - 수정. `reqwest`(blocking feature) + `dotenvy`(. env 로드) 추가.
- `src/ollama.rs` - 신규. `OllamaBackend`(PersonaRuntime 구현).
- `src/lib.rs` - 수정. `pub mod ollama;`.
- `src/main.rs` - 수정. `--llm` opt-in, `--model`(기본 `gemma4:e4b`), `--cloud`(cloud host + 키), `--ollama-host` override.

## Change description

- `OllamaBackend { client: reqwest::blocking::Client, model: String, endpoint: String, api_key: Option<String> }`.
  - `new(model, endpoint, api_key, timeout)`. 백엔드는 키를 **인자로** 받는다(env 직접 안 읽음 → 순수·테스트 가능). 
  - `generate`: `POST {endpoint}/api/generate` body `{ "model": model, "prompt": <조립>, "stream": false }`. cloud면 헤더 `Authorization: Bearer {api_key}`. 응답 JSON의 `response` 필드를 텍스트로. 한 줄로 trim.
  - 프롬프트(최소, 페르소나 조각은 task-17): "You are {speaker} in a casual group chat. Recent lines: {최근 history n개}. Reply with ONE short, in-character line. No preamble." (task-17에서 system prompt로 교체).
- **보안(INV-6)**: api_key는 `Authorization` 헤더에만. **로그/`eprintln`/에러 메시지/`Debug` 출력에 키를 절대 넣지 않는다.** `OllamaBackend`에 `#[derive(Debug)]`를 쓰면 키가 노출되니, Debug 수동 구현하거나 키 필드를 제외/마스킹. 커밋·출력에 키 금지.
- **에러/타임아웃**: 네트워크 실패·비2xx·파싱 실패·타임아웃 → `None` 반환(panic 금지, `unwrap` 금지). 실패 사유는 키를 뺀 간단한 메시지로 stderr에 한 줄(선택). None이면 그 틱은 내용 없는 발화(라벨)로 처리 → 엔진 결정은 유지.
- main: `dotenvy::dotenv().ok()`로 .env 로드(키 없으면 무시). `--llm` 있을 때만 OllamaBackend, 없으면 FakeBackend(기본). `--cloud`면 endpoint=cloud + `OLLAMA_CLOUD_API_KEY`(env). 아니면 local `http://localhost:11434`(키 None). `--model` 기본 `gemma4:e4b`.
- 결정성: LLM 텍스트는 비결정. 그러나 **엔진 결정(화자/침묵)은 그대로**(OllamaBackend.generate도 rng를 안 건드림 → FakeBackend와 동일한 rng 흐름). 텍스트만 record.utterance에 채워짐.

## Dependencies

- task-15(PersonaRuntime, Event.content/record.utterance, driver 주입).

## Verification

```bash
cargo test
cargo build
cargo run -- --headless --seed 42 --ticks 80 --theta 0.65   # 기본(FakeBackend) → 골든 동일
```

- `cargo test` (신규 단위 테스트, **네트워크 없이**): (1) 요청 body JSON이 model/prompt/stream:false를 담음, (2) 응답 JSON `{"response":"..."}` 파싱이 텍스트 추출, (3) Debug/에러 문자열에 키가 안 나옴(키를 넣고 format!해서 미포함 단언). 라이브 호출은 `#[ignore]`.
- `cargo test` 전체 green(기본 FakeBackend라 52 유지).
- **α=0 기본 골든 5종 바이트 동일**(`--llm` 없으면 FakeBackend → utterance 생략). 빌드 후 명시적 순차 diff.
- (수동) `--llm`(로컬 ollama + gemma4:e4b 떠 있으면)으로 utterance가 채워지는지. 에이전트는 네트워크 없으니 생략, 사람이 확인.

## Risks

| 위험 | 회피 |
|---|---|
| API 키 노출 | Authorization 헤더에만. Debug 수동 구현/마스킹. 로그·에러·커밋에 키 금지. 단위 테스트로 키 미노출 단언 |
| 네트워크 실패로 panic | 모든 실패 → None. unwrap/expect 금지. 타임아웃 설정 |
| opt-in 아닌 기본이 LLM으로 새서 골든·결정성 깨짐 | 기본 FakeBackend. `--llm` 없으면 절대 OllamaBackend 안 씀. 골든 재확인 |
| reqwest/dotenvy 공급망 | 표준·널리 쓰이는 크레이트. blocking feature만. v0.4 병렬 때 async 재검토 |
| .env 키 커밋 | .env는 gitignored. 코드/출력에 키 금지 |
