---
title: Salon 엔진 플랜 v0.3 - 로컬 LLM
type: plan
status: draft
priority: P1
updated_at: 2026-06-02
owner: shared
summary: v0.3 - 화자 선택은 엔진이, 내용 생성만 LLM이. PersonaRuntime(Fake/Ollama), Event.content, 내용 기반 RRF(관심도·잔향), persona collapse 관전. 기본은 LLM 없이 결정적 유지.
design_ref: ../reference/salon-engine-design.md
roadmap_ref: salon-engine-v1.md
---

# Salon - 플랜 v0.3 (로컬 LLM)

v0.2(`salon-engine-v2.md`)까지 LLM 없이 발화 리듬과 케미를 검증했다. v0.3은 **실제 대사**를 붙인다. 단 원칙은 그대로: **화자 선택(누가/언제)은 엔진이 결정적으로, 내용 생성만 LLM이.** 기본 `cargo run`은 LLM 없이 결정적이고, LLM은 opt-in이다.

모델: 우선 `gemma4:e4b`로 시작, PersonaRuntime은 모델 교체 가능. cloud는 2026 출시 7-12b. 메모리 [[v03-llm-backend]] 참조.

## 0. Context

설계 §6은 "결정은 싼 휴리스틱, 생성만 비싼 LLM"(AutoGen 권고)이다. v0.1~v0.2가 결정 층(엔진)을 완성했으니, v0.3은 생성 층만 붙이면 된다. `docs/temp/salon-persona-fragments.md`(내용층 32 조각)와 `salon-persona-ui.md`(역할·조립)가 여기서 쓰인다.

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | 기본 실행(`cargo run`, 플래그 없음)은 LLM 없이 결정적. LLM은 opt-in 플래그로만 |
| INV-2 | 엔진 결정(누가/언제/침묵)은 seed로 결정적. 백엔드와 무관. 생성 텍스트만 비결정 |
| INV-3 | α=0 + FakeBackend면 v0.2 골든 5종 바이트 동일. content/utterance는 신규 필드라 비었을 때 serde 생략 |
| INV-4 | PersonaRuntime은 Contract(trait). FakeBackend(결정적) / OllamaBackend(라이브) 교체 |
| INV-5 | v0.2의 50 테스트 + 스모크 게이트 유지 |
| INV-6 | 비밀: `OLLAMA_CLOUD_API_KEY`는 .env(gitignored)에서만. 로그/출력/코드에 키 노출 금지 |

## 2. Goals / Non-goals

### Goals
- (G1) `PersonaRuntime` 트레이트: 선택된 화자의 발화 텍스트 생성. FakeBackend(결정적 stub) + OllamaBackend(HTTP, 로컬 localhost:11434 또는 cloud + 키).
- (G2) `Event.content` / `ObservationRecord.utterance`: 실제 텍스트(없으면 serde 생략).
- (G3) Persona에 system prompt(조각 조립) + model 연결.
- (G4) 내용 기반 RRF 신호 활성화: 관심도(직전 화제 ↔ 역할 키워드), 잔향(과거 화제 재소환).
- (G5) persona collapse 관전: 같은 모델 + 다른 persona prompt → 출력 비교(관전 도구).

### Non-goals
- ❌ 동시/병렬 호출(v0.4), 임베딩 FlowMeter(v0.5), MetaController(v0.6).
- ❌ 40조각 on-demand 조립 UI / `/invite`(이후). v0.3은 데모 persona 몇 개를 직접 정의해도 됨.

## 3. 데이터 모델 델타

| 구조 | 변경 |
|------|------|
| `PersonaRuntime` | 신규 trait `generate(persona, context) -> String`(또는 Result) |
| `FakeBackend` | 신규. 현 fake utterance를 백엔드로. 결정적, content 없음(None) |
| `OllamaBackend` | 신규. blocking reqwest. 로컬/cloud, 모델·키 설정 |
| `Persona` | system_prompt(조각 조립) + model 필드(또는 사이드 맵). 리터럴 churn 주의 → 사이드 맵 우선 검토 |
| `Event` | content: Option<String> 추가 |
| `ObservationRecord` | utterance: Option<String> 추가. `skip_serializing_if = Option::is_none` → Fake/없을 때 생략(골든 보존) |

## 4. Subtasks (위험 분리: 순수 Rust → 네트워크 순서)

| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| 15 | PersonaRuntime + FakeBackend + content 배선 | trait, Fake(결정적), Event.content/record.utterance(serde 생략), driver가 runtime 통해 발화 | 낮음(순수, 네트워크 없음). 골든 보존 | v0.2 |
| 16 | OllamaBackend (HTTP) | blocking reqwest, 로컬/cloud, 모델·키(.env), 에러·타임아웃·키 비노출. `--llm`/`--model` 플래그 | 중(네트워크/키) | 15 |
| 17 | Persona system prompt 조립 | persona-fragments 조각(역할+축) → system prompt. 데모 persona 몇 개 | 낮음 | 16 |
| 18 | 내용 기반 RRF 신호 | 관심도·잔향 활성화(Event.content 사용). RRF에 신호 추가(토글) | 중(결정성: content 있을 때만, seed 고정 stub로 테스트) | 15 |
| 19 | persona collapse 관전 도구 | 같은 모델 + 다른 prompt 출력 비교(example/모드) | 낮음 | 16,17 |
| 20 | v0.3 스모크 | Fake로 결정 결정성·content 배선 검증. Ollama 통합 테스트는 `#[ignore]`(네트워크 필요) | 낮음 | 15,18 |

Phase A(15 순수 기반) → B(16 HTTP, 집중) → C(17 prompt, 18 신호) → D(19 관전, 20 스모크).

## 5. v0.3 완료 기준

- 기본 `cargo run`(LLM off)이 v0.2와 동일하게 결정적이고 골든 보존.
- `--llm`(또는 모델 지정)으로 OllamaBackend 사용 시 실제 대사가 record.utterance/TUI에 나타남.
- 엔진 결정(화자/침묵 시퀀스)은 백엔드와 무관하게 seed로 동일(Fake든 Ollama든).
- 내용 기반 RRF 신호(관심도·잔향)가 content 있을 때 화자 선택에 반영됨(stub로 결정적 검증).
- persona collapse를 관전 도구로 비교 가능(작은 모델에서 페르소나 구분 정도).

## 6. 위험과 대응

| 위험 | 대응 |
|------|------|
| LLM 비결정이 엔진 검증 오염 | 결정(엔진)과 생성(LLM) 분리. 테스트/기본은 FakeBackend(결정적). Ollama는 opt-in |
| 골든 회귀 | content/utterance는 serde 생략(Fake=None). task별 골든 5종 재확인 |
| API 키 노출 | .env(gitignored)만. 로그/에러/커밋에 키 금지(INV-6). reqwest 헤더에만 |
| 네트워크 실패/타임아웃 | OllamaBackend에 타임아웃·재시도·명시적 에러(panic 금지). 실패 시 침묵 또는 placeholder 명시 |
| RAM(로컬 e4b) | 기본은 LLM off. 로컬 e4b는 collapse 테스트 때만 |
| reqwest 의존 추가 | blocking feature만. v0.4 병렬 때 async 검토 |

## 7. 산출물

- 이 문서(PLAN v0.3). 구현 시 §4를 `salon-engine-v3-task-15..20.md`로 분해.
- v0.3 한 줄: 엔진이 누가 언제 말할지 결정적으로 정하고, 그 한 명에게만 LLM이 실제 대사를 붙이면 v0.3 성공. 기본은 여전히 LLM 없이 돈다.
