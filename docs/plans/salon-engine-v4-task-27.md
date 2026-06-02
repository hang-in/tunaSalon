---
title: "Salon v0.4 Task 27: 백엔드 추상화(Backend enum) + OpenAIBackend (vLLM friend)"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "27"
depends_on: ["22"]
parallel_group: ""
---

# Task 27 - 백엔드 추상화 + OpenAIBackend (지인서버 vLLM)

plan `salon-engine-v4.md` subtask 27 (**번호는 신규 생성순, 실행은 22 다음·23 이전**). 지인서버가 **Ollama가 아니라 vLLM의 OpenAI 호환 API**(`/v1/chat/completions`)라, 단일 `OllamaBackend`로는 담을 수 없다. 풀이 **프로토콜 2종**을 담도록 `enum Backend{Ollama,OpenAI}`로 추상화하고, 신규 `OpenAIBackend`를 추가한다. 이게 동시성(task-23)이 올라탈 기반이다. 메모리 [[friend-server-vllm]].

## Changed files

- `src/openai.rs` - 신규. `OpenAIBackend`(vLLM/OpenAI chat completions).
- `src/lib.rs` - 수정. `pub mod openai;`.
- `src/ollama.rs` - 수정. `pub fn generate_shared(&self, speaker, history, tick) -> Option<String>` 추출(현 `generate` 본문, `&self`·rng 불요). 트레이트 `generate(&mut self,...,rng)`는 이를 위임.
- `src/pool.rs` - 수정. `Protocol{Ollama,OpenAI}` + `enum Backend{Ollama(OllamaBackend),OpenAI(OpenAIBackend)}`(`generate(&self,...)` 디스패치). `BackendConfig`에 `protocol`+`max_tokens` 필드, `new`(Ollama 기본, 기존 호출 보존)/`new_openai` 생성자. `BackendPool.backends: BTreeMap<String, Backend>`로 변경, `add`가 protocol로 분기. `PersonaRuntime::generate`는 `backends.get(name)` → `Backend::generate`.
- `src/main.rs` - **변경 불필요 예상**(BackendConfig::new = Ollama 기본 유지, pool.add 시그니처 동일). 빌드 깨지면 최소 수정.

## Change description

- `OpenAIBackend { client, model, endpoint, api_key: Option<String>, system_prompts: BTreeMap<PersonaId,String>, max_tokens: Option<u64> }`. **Debug 수동 구현(api_key redacted)**.
  - `generate(&self, speaker, history, tick) -> Option<String>`: `POST {endpoint}/v1/chat/completions`, body `{"model":model,"messages":[{"role":"system","content":<persona prompt if Some>},{"role":"user","content":<recent log + 지시>}],"stream":false, "max_tokens":<if Some>}`. system prompt 없으면 system 메시지 생략. user 메시지는 OllamaBackend와 같은 조립(최근 4줄 + "Reply with ONE short, in-character line. No preamble.").
  - 응답 파싱: `choices[0].message.content` 추출, trim, 비면 None. **`reasoning` 필드는 무시**(qwen3.6-35b는 reasoning 모델 → CoT가 `reasoning`에, 답은 `content`에).
  - api_key Some이면 `Authorization: Bearer` 헤더. 지인서버는 무인증이라 보통 None.
  - 에러/비2xx/타임아웃/파싱 실패 → None(panic·unwrap 금지). **에러 메시지에 키 금지(INV-6)**.
- `Backend::generate(&self,...)`: `match self { Ollama(b)=>b.generate_shared(...), OpenAI(b)=>b.generate(...) }`. rng 불요(둘 다 미소비), `Send+Sync`(task-23 배치 공유 대비).
- `BackendConfig`: `protocol: Protocol`, `max_tokens: Option<u64>` 추가. `new(...)`(기존 시그니처)은 protocol=Ollama·max_tokens=None로 채워 **기존 호출처(task-21/22 테스트·main) 불변**. `new_openai(name,model,endpoint,api_key,max_concurrent,max_tokens,timeout)` 추가(protocol=OpenAI·num_ctx=None).
- `BackendPool.add(config, system_prompts)`: `config.protocol`로 `Backend::Ollama`(OllamaBackend::new) 또는 `Backend::OpenAI`(OpenAIBackend::new) 빌드.
- `PersonaRuntime for BackendPool::generate`: `resolve(speaker)` → `backends.get(name)`(이제 `&self`로 충분) → `backend.generate(speaker,history,tick)`. rng 미소비.
- 결정성: 라이브 결정 경로 불변. 기본 `cargo run`은 FakeBackend(풀 미사용) → 골든 보존.
- 가드레일: 신규 크레이트 금지(reqwest blocking 재사용). unwrap/panic 금지. 키 비노출.

## Dependencies

- task-22(BackendPool 라우팅, resolve, PersonaRuntime).

## Verification

```bash
cargo build
cargo test    # 라이브 테스트는 #[ignore]
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
# (5종 모두: s42_t040/120, s42_t065/80, s42_t078/120, s7_t065/80, s99_t065/80)
```

- `cargo test` green(+신규: OpenAI build_request_body messages 구조·max_tokens, parse가 `choices[0].message.content` 추출 + `reasoning` 무시 + 결측 None, api_key redaction/미노출, Backend enum 분기, pool.add_openai → Backend::OpenAI, mixed 풀 resolve).
- **골든 5종 바이트 동일**(LLM off = FakeBackend).
- 라이브 `#[ignore]` 테스트: 지인서버(`SALON_FRIEND_ENDPOINT` 기본 `http://yongseek.iptime.org:8008`, `SALON_FRIEND_MODEL` 기본 `qwen3.6-35b`)에 1회 generate → Some(텍스트). 리뷰어가 수동 실행해 실제 응답 확인.

## Risks

| 위험 | 회피 |
|---|---|
| 프로토콜 혼동(Ollama body를 OpenAI로) | OpenAIBackend는 messages 포맷·choices 파싱 별도. 단위 테스트로 body/parse 검증 |
| reasoning 필드를 답으로 오추출 | `choices[0].message.content`만. `reasoning` 무시. parse 테스트에 reasoning 포함 샘플 |
| enum 전환으로 pool generate가 &mut 필요해짐 | `Backend::generate(&self)` → `backends.get`(get_mut 불요). task-22 동작 보존 |
| BackendConfig 시그니처 변경 churn | `new`(Ollama 기본) 유지로 기존 호출 불변. `new_openai` 추가만 |
| API 키 노출 | OpenAIBackend Debug redacted, Authorization 헤더에만, 에러에 키 금지. 미노출 단위 테스트 |
| 골든 회귀 | 라이브 결정 경로·FakeBackend 불변. 골든 5종 재확인 |
| 지인서버 라이브 의존이 CI 깨뜨림 | 라이브 테스트는 `#[ignore]`. 단위 테스트는 네트워크 없이 |
