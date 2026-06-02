---
title: Salon 엔진 플랜 v0.4 - 동시 호출 (이종 백엔드 풀)
type: plan
status: in_progress
priority: P1
updated_at: 2026-06-02
owner: shared
summary: v0.4 - 이종 백엔드 풀(Ollama Cloud 동시성 3 + 지인서버 qwen3.6:32b 동시성 2) + 페르소나별 라우팅(mixed-model 방) + 백엔드별 세마포어/큐/타임아웃 폴백. 동시성은 persona collapse 비교·벤치 전용, 라이브 틱 루프는 순차 유지. async(tokio) 미도입(blocking + std::thread::scope). num_ctx 하드코딩 제거(백엔드별 Option). 라이브 burst는 보류(측정 후).
design_ref: ../reference/salon-engine-design.md
roadmap_ref: salon-engine-v1.md
---

# Salon - 플랜 v0.4 (동시 호출)

v0.3(`salon-engine-v3.md`)이 LLM 1명을 결정적 엔진에 붙였다. v0.4는 **여러 백엔드를 동시에** 쓴다. 단 원칙은 그대로: **화자 선택(누가/언제)은 엔진이 결정적으로, 생성만 LLM이.** 라이브 틱 루프는 발화 1명/틱 + 인과적 턴테이킹이라 **순차 유지**하고, 동시성은 persona collapse 비교/벤치에서만 쓴다.

백엔드 2종(실제 가용, 둘 다 도달 확인 2026-06-02): (1) **Ollama Cloud** `gemma4:31b-cloud`(256k ctx, 동시성 3, Ollama `/api/generate`), (2) **지인서버 vLLM** `qwen3.6-35b`(256k ctx, 동시성 1, **OpenAI `/v1/chat/completions`**, reasoning 모델). **프로토콜이 2종**이라 백엔드 추상화가 필요하다. 근거: [[ollama-cloud-limits]], [[friend-server-vllm]]. **로컬 ollama는 금지**(맥북 느림).

## 0. Context

조사 결론(2026-06-02): Ollama Cloud는 토큰 종량제가 아니라 **GPU 사용시간 정액 구독**이라 "발화마다 선형 과금" 우려가 없다. 서버가 동시성 3에서 이미 큐잉하고 초과 시 거부하므로, 클라이언트 세마포어(cloud 3)는 서버 동작과 정확히 일치한다. 16,384 ctx 캡은 버그였고 수정됨 → cloud/원격은 모델 최대 ctx로 자동 설정되므로 우리가 `num_ctx`를 보내면 오히려 깎아내린다. 동시도 ≤5(3+2)라 tokio의 고동시성 이점이 없어 **blocking reqwest + std::thread::scope**로 간다.

### 운영 제약 (2026-06-02 사용자 지시)

지인서버는 **가동 중**(이 맥북에서 도달 확인). 단 **Ollama가 아니라 vLLM의 OpenAI API**(`/v1/chat/completions`)라, 단일 `OllamaBackend`로는 안 되고 **백엔드 추상화(프로토콜 2종)가 선행 필요** → task-27(Backend enum + OpenAIBackend)을 task-23(동시성) 이전에 먼저 한다.

- **로컬 ollama 사용 금지**(맥북 극도로 느려짐): 라이브 LLM은 cloud(`gemma4:31b-cloud`) + 지인서버(`qwen3.6-35b`)만. cloud 모델 미pull이면 `ollama pull gemma4:31b-cloud`(원격 포인터, 로컬 RAM 안 씀).
- 결정적/CI 검증은 여전히 **fake 백엔드로 네트워크 없이**. 라이브(cloud/friend) 검증은 `#[ignore]` 또는 수동 실행.
- 지인서버 동시성은 `max-num-seqs 1` + vllm-swap → **1**(직렬). 기본 모델 `qwen3.6-35b-fast`(reasoning 꺼진 변형, ~0.7s). OpenAIBackend는 항상 `chat_template_kwargs.enable_thinking=false` 전송(일반 qwen3.6-35b도 안전) + `content`만 추출(검증 2026-06-02).

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | 기본 실행(`cargo run`, LLM off, FakeBackend)은 v0.1~v0.3 골든과 바이트 동일. 병렬 경로는 결정 경로에 진입 금지 |
| INV-2 | 엔진 결정(누가/언제/침묵)은 seed로 결정적. 백엔드 풀·라우팅과 무관. 생성 텍스트만 비결정 |
| INV-3 | 라이브 틱 루프는 순차(발화 1명/틱, 인과적 턴테이킹). 동시성은 bench/batch 경로 전용 |
| INV-4 | 백엔드별 max_concurrent 세마포어가 동시 in-flight를 상한(cloud=3, friend vLLM=1: max-num-seqs 1). 초과는 큐, 큐 풀/거부면 폴백 |
| INV-5 | async/tokio 미도입. blocking reqwest + std::thread::scope + 세마포어 |
| INV-6 | 비밀: 백엔드별 `api_key`도 Debug redacted, 로그/에러/출력에 키 노출 금지(v0.3 INV 계승) |
| INV-7 | num_ctx는 백엔드별 `Option<u64>`. None=요청에서 생략(cloud/원격 auto-max). 단일 백엔드 기본은 Some(8192) 유지로 기존 동작 보존 |
| INV-8 | v0.3의 72 테스트 + 스모크 게이트 3종 유지(+ v0.4 게이트 추가) |

## 2. Goals / Non-goals

### Goals
- (G1) `BackendPool`: 이름붙은 백엔드 레지스트리. 각 백엔드 = `{model, endpoint, api_key, max_concurrent, num_ctx: Option<u64>, timeout}` + 백엔드별 세마포어.
- (G2) 페르소나별 라우팅: `persona -> backend` config 맵. 미지정 페르소나는 기본 백엔드로 폴백(단일 백엔드 = v0.3 동작 그대로).
- (G3) num_ctx 백엔드별 `Option<u64>`(하드코딩 제거). None이면 요청 body에서 생략.
- (G4) 병렬 배치 API: `std::thread::scope` + 백엔드별 세마포어. persona_collapse 비교와 벤치를 동시화(현재 순차 3-페르소나 루프).
- (G5) 거부/타임아웃 폴백: 비2xx(429/queue-full)·타임아웃 → 폴백 백엔드(또는 FakeBackend) + 백오프. panic 금지.
- (G6) mixed-model 방: 한 살롱에서 일부 페르소나는 지인서버 `qwen3.6-35b`(256k ctx, reasoning), 일부는 cloud `gemma4:31b-cloud`. 비교 벤치 모드.
- (G7) 백엔드 추상화: `enum Backend { Ollama, OpenAI }` + 신규 `OpenAIBackend`(vLLM chat completions). 풀이 프로토콜 2종을 담는다.

### Non-goals
- ❌ 라이브 burst(같은 맥락 다중 페르소나 동시 반응) - **보류**. 순차 라이브 지연을 먼저 측정한 뒤 v0.4.x/v0.5에서 결정(인과적 턴테이킹과 충돌). (task-25 측정 2026-06-02: cloud `gemma4:31b-cloud` 순차 1발화 warm avg ~1.6s, max ~3.4s. 순차 틱이 체감될 만큼 블록 → burst/파이프라이닝은 생동감에 유의미하나 인과성 충돌로 v0.4.x/v0.5로.)
- ❌ async/tokio 전환. ❌ FlowMeter 임베딩(v0.5), MetaController(v0.6).
- ❌ 잔여 cloud budget 프로그램 조회(Ollama가 API 미노출, #15663) - 사용자 모니터링으로 커버. ❌ prompt cache 효과 측정(별도, 필요 시).

## 3. 데이터 모델 델타

| 구조 | 변경 |
|------|------|
| `BackendConfig` | `{name, model, endpoint, api_key, max_concurrent, timeout, protocol: Protocol, num_ctx: Option<u64>(Ollama용), max_tokens: Option<u64>(OpenAI용)}`. `Protocol { Ollama, OpenAI }`. `new`(Ollama 기본, 기존 호출 보존)/`new_openai` 생성자 |
| `Backend`(신규 enum) | `Ollama(OllamaBackend)` \| `OpenAI(OpenAIBackend)`. `generate(&self, speaker, history, tick) -> Option<String>` 디스패치. rng 불요·Send+Sync(향후 배치 공유) |
| `OpenAIBackend`(신규 `openai.rs`) | vLLM/OpenAI `/v1/chat/completions`. body `{model, messages:[system,user], max_tokens?, stream:false}`, 응답 `choices[0].message.content` 추출(reasoning 모델의 `reasoning`은 무시). api_key Debug redacted |
| `BackendPool` | name -> `Backend` 레지스트리(+task-23부터 백엔드별 Semaphore) + `persona -> backend_name` 라우팅 + 기본 백엔드. `add`가 `config.protocol`로 Ollama/OpenAI 분기 |
| `OllamaBackend` | `num_ctx: Option<u64>`(task-21 완료). 스레드 공유·enum 디스패치용 `generate_shared(&self, ...)` 추가(task-27). reqwest blocking Client는 Send+Sync+Clone |
| `PersonaRuntime` | 라이브 경로는 풀이 라우팅(트레이트 호환 유지). 배치는 별도 `generate_batch(...)`(task-23) |
| CLI/Config | room 정의에 backends + routing. `--llm` 단일 모델은 기존대로(단일 백엔드 풀로 매핑) |

## 4. Subtasks (위험 분리: 순수 → 라우팅 → 동시성 → 도구/게이트)

| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| 21 | num_ctx Option화 + BackendConfig + 풀 스켈레톤 | `num_ctx: Option<u64>`(None=생략), `build_request_body` 시그니처 갱신(테스트 갱신), BackendConfig/Pool 레지스트리(동시성 없이). 단일 백엔드 기본 8192 유지 | 낮음(순수). 골든 보존. 기존 build_request_body 테스트 갱신 | v0.3 |
| 22 | 페르소나별 라우팅 | `persona -> backend` 맵, 미지정 폴백. 풀이 PersonaRuntime 구현. 라이브 순차 유지 | 낮음~중 | 21 |
| 27 | **백엔드 추상화 + OpenAIBackend(vLLM friend)** | `enum Backend{Ollama,OpenAI}`, 신규 `OpenAIBackend`(`/v1/chat/completions`, `content` 추출, reasoning 무시), 풀이 `protocol`로 분기, `OllamaBackend::generate_shared(&self)`. 지인서버(`qwen3.6-35b`) 라이브 검증 | 중(신규 프로토콜·네트워크). 골든 보존 | 22 |
| 23 | 병렬 배치 API + 백엔드별 세마포어 | `std::thread::scope` + 백엔드별 Semaphore로 N 동시 generate(`Backend::generate` &self). cap 준수(cloud 3/friend 1). deterministic fake로 cap 검증(네트워크 없이) | 중(스레드 공유·결정성 오염 금지) | 27 |
| 24 | 거부/타임아웃 폴백 + 백오프 | 비2xx(429/queue-full)·타임아웃 분류 → 폴백 백엔드 또는 Fake + 백오프. 백엔드 unhealthy 우회 | 중(네트워크 분기). panic 금지 | 23 |
| 25 | persona_collapse 병렬화 + mixed-model 벤치 | example를 배치 API로 동시화. mixed-model(anchor=friend `qwen3.6-35b`, 나머지=cloud `gemma4:31b-cloud`) 벤치 모드. 라이브 순차 1발화 지연 측정(burst 보류 판단 근거) | 낮음 | 24, 27 |
| 26 | v0.4 스모크 게이트 | LLM off 풀=골든 바이트 동일(INV-1) 재확인. 배치 cap·폴백·num_ctx·라우팅·Backend 분기를 deterministic fake로 검증(`#[ignore]` 라이브 분리) | 낮음 | 21,23,24,27 |

> 실행 순서는 task 번호가 아니라 depends_on을 따른다(task-27은 22 다음, 23 이전에 신규 삽입). Phase A(21 ✅) → B(22 ✅) → **B2(27 백엔드 추상화 + OpenAIBackend)** → C(23 동시성, 24 폴백) → D(25 도구·측정, 26 게이트). v0.4 완료 게이트는 task-26 통과 + 골든 보존.

## 5. v0.4 완료 기준

- 기본 `cargo run`(LLM off, 풀=Fake)이 v0.1~v0.3과 동일하게 결정적이고 골든 5종 바이트 동일.
- 페르소나별 라우팅으로 **한 방에서 두 백엔드 동시 사용**(예: anchor→qwen3.6:32b, friend/chaos→cloud). 라우팅 미지정은 기본 백엔드로 폴백.
- 병렬 배치가 **백엔드별 cap 준수**(cloud 동시 ≤3, qwen 동시 ≤2). deterministic fake로 동시 in-flight 상한 검증.
- 거부(비2xx/queue-full)·타임아웃 시 폴백 동작(panic 없음), 백엔드 unhealthy면 라우팅 우회.
- num_ctx None이면 요청 body에 `options.num_ctx` 없음, Some(n)이면 n. cloud/qwen은 None, 로컬 e4b는 Some(8192).
- persona_collapse가 병렬로 동작(순차 대비 빠름) + mixed-model 벤치로 모델별 출력 비교 가능.
- 라이브 순차 1발화 지연을 측정해 기록(burst 도입/보류 판단 근거).

## 6. 위험과 대응

| 위험 | 대응 |
|------|------|
| 동시성이 엔진 결정성 오염 | 병렬은 bench/batch 전용, 라이브 결정 경로 불침투(INV-1/3). deterministic fake로 cap·폴백 테스트(네트워크 없이) |
| blocking Client 스레드 공유 | reqwest::blocking::Client는 Send+Sync+Clone. generate는 상태 불변 → `&self` 경로. rng는 Ollama가 미소비라 스레드별 더미 rng로 충분 |
| 세마포어 데드락/큐 풀 | 백엔드별 Semaphore + 타임아웃 + 거부 폴백 명시. 큐 상한 초과는 즉시 폴백(무한 대기 금지) |
| 지인서버 가용성/네트워크 | 폴백 백엔드(cloud 또는 Fake). 백엔드 unhealthy 감지 시 라우팅 우회. 타임아웃 짧게 |
| cloud budget 소진 | 잔여 조회 API 없음(#15663) → 사용자 모니터링. 경량 모델 우선(Level 1). 정액제라 달러 폭증은 없고 budget만 소진 |
| 골든 회귀(num_ctx 시그니처 변경) | `build_request_body` 테스트 갱신. 단일 백엔드 기본 8192 유지로 persona_collapse 등 기존 동작 보존 |
| API 키 노출(다중 백엔드) | 백엔드별 api_key도 Debug redacted, Authorization 헤더에만(INV-6) |
| 라이브 burst 욕심으로 인과성 붕괴 | v0.4 비목표. 순차 지연 측정 후 별도 결정 |

## 7. 산출물

- 이 문서(PLAN v0.4). 구현 시 §4를 `salon-engine-v4-task-21..26.md`로 분해.
- v0.4 한 줄: 엔진은 여전히 결정적으로 누가 언제 말할지 정하고, 한 방 안에서 페르소나마다 다른 백엔드(qwen-32b·cloud)에 라우팅해 생성하되, 동시 호출은 백엔드별 상한 안에서 비교/벤치 때만. 기본은 여전히 LLM 없이 결정적으로 돈다.
