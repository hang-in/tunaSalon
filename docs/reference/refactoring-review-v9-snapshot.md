---
type: reference
status: snapshot
updated_at: 2026-06-03
title: tunaSalon 리팩토링 검토 보고서 - v0.9 스냅샷
---

# tunaSalon 리팩토링 검토 보고서 - v0.9 스냅샷

> 검토 대상: `/Users/d9ng/privateProject/tunaSalon` (Rust 2021, v0.9 done, 222 tests / friend-engine feature 230)
> 검토 모드: 리팩토링 관점 한정. 기능 추가 / 행동 변경 제안 제외.
> 검토자: pi (MiniMax-M3)
> 검토 일자: 2026-06-03
> 검토 기준: `../prompts/refactoring-review-prompt.md`
>
> **이 문서는 스냅샷입니다.** 향후 v0.10, web 프런트엔드, 페르소나 합성 단계 진입 시점에 새 스냅샷을 찍어 둘 것을 권장합니다.
>
> 실행하지 않은 상태에서 검토했으므로, 기능 정상 여부는 단정하지 않습니다("확인 필요"로 표시).

---

## 0. 검토 원칙 적용 메모

- 변동성이 높고 변경 전파가 큰 결합을 최우선
- 현재 잘 작동하는 코드는 **이유 없이 분리/추상화/재작성하지 않음**
- 솔로 개발 단일 레포 기준 (마이크로서비스/과도한 레이어링 배제)
- "추측 / 일반론 / 취향 기반 조언" 배제, 코드에서 확인된 사실만 보고

---

## 1. 기능 안정성

### 1-1. `live_store()`의 글로벌 부작용

- **위치**: `src/memory.rs:413-440` (`MemoryStore::live_store()`)
- **분류**: 기능 / 변동성 정렬
- **결합 태그**: Functional
- **문제**: `live_store()`가 테스트/스모크에서 호출되면 실제 `~/.local/share/tunaSalon/memory.db`를 만지고, 실패 시 `eprintln!` 후 `new()`로 폴백. 모듈 자체에는 `#[cfg(test)]` 가드가 없고 문서 코멘트로만 "테스트에서 호출 금지"로 막혀 있다. `live.rs:152`의 docstring도 같은 경고만 적혀 있다.
- **왜 문제인가**: 회귀가 한 줄짜리 import 실수로 사용자 디스크에 `memory.db`를 덮어쓸 수 있다. `Cargo.toml`의 `[features] friend-engine`만 켜면 자동으로 활성화된다.
- **변경 전파 범위**:
  - 함께 바뀔 가능성: `tests/recall_eval.rs` (직접 `MemoryStore::new()`만 쓰는 점은 OK이나 v0.10 mock+실모델 분기 시 깨질 여지), `src/main.rs:127-133`의 호출처.
  - 전파 원인: 글로벌 경로 + 라이브 폴백의 부작용이 단일 진입점에 집중되지 않음.
  - 결합 유형: Functional.
- **제안**: `live_store()`는 `main.rs`에서만 호출하고, `LiveSession::new`(테스트 경로)는 항상 `MemoryStore::new()`로 들어가도록 강제. 가장 작은 변경은 `pub(crate)`로 좁히고 `cfg(not(test))` 게이트.
- **리스크**: 낮음.
- **우선순위**: 중간.

### 1-2. ChatApp 시작 실패 시 한 줄 종료

- **위치**: `src/main.rs:140-142`
- **분류**: 기능 (확인 필요)
- **문제**: `ChatApp::new(session, ...)`가 실패하면 `process::exit(1)`로 즉시 종료. 부분적으로 초기화된 `LiveSession`(워커 스레드 + 채널)이 살아있을 수 있는데 `Drop`이 호출 안 됨. 운영 환경(시그널 핸들러 없음)에서 종료 핸들러가 없는 점이 우려.
- **왜 문제인가**: 라이브에서 raw-mode 진입/실패 시 TTY가 복원 안 될 수 있음.
- **제안**: 실행하지 않고는 단정 불가. **확인 필요**.
- **우선순위**: 낮음.

### 1-3. 외부 I/O / panic

- **위치**: `src/ollama.rs:186,192`, `src/openai.rs`(동급), `src/memory.rs:298-299, 419-430`
- **분류**: 기능 (확인 필요)
- **문제**: 핵심 경로의 unwrap/expect는 `memory.rs:298-299`("in-memory sqlite must open")처럼 컴파일 타임 가정에 기대고 있어 OK. 라이브 LLM 호출은 `eprintln!` 후 `None` 반환 패턴이라 panic 없음. `chat.rs`의 `chat_personas()` ↔ `system_prompts()`는 길이 1:1 매핑이 코드적으로 묵시적(런타임에 `persona.id`를 못 찾으면 `unwrap_or` 등 명시적 fallback 없음).
- **제안**: 실행 안 했으므로 **확인 필요**.

---

## 2. 결합 강도

### 2-1. `[높음]` `driver::run` ↔ `live::LiveSession::tick` - per-tick 알고리즘의 100% 복제

- **위치**: `src/driver.rs:38-114` ↔ `src/live.rs:286-454`
- **분류**: 결합 강도 / 책임 / 변동성 정렬
- **결합 태그**: **Functional**
- **문제**: 두 함수가 다음 시퀀스를 1:1로 복제하고 있다(코멘트로도 명시):
  1. `HawkesEngine::update_intensities(state, 1, config, personas, mu_scale)`
  2. `HawkesEngine::decay_excitations(state.excitations, 1, beta, tick_interval)`
  3. `HawkesEngine::combined_intensities(...)`
  4. `gate::evaluate(combined, theta)`
  5. `forbid_self_repeat` 필터
  6. `rrf::select(filtered, combined, history, k, rng)`
  7. `utterance::make_utterance(...)` (with_topic_tag=false)
  8. speak 시 `suppress_chosen` / `apply_excitation_on_speak` / `last_speaker` 갱신 / `history.push` 순서
- **왜 문제인가**:
  - `live.rs:269-272`의 docstring에 "driver::run의 per-tick 로직과 동일한 순서"라고 박혀 있다. 즉 **동일 알고리즘이 두 곳에 중복**돼 있고, 결정성 보존이 둘 사이의 rng 소비 순서까지 묶어버린 상태. 누군가가 v0.7 식힘을 driver에만 추가하면 라이브 경로의 엔진 선택이 결정성을 잃는다.
  - 변동성이 가장 높은 코드(엔진 틱)가 두 군데에 흩어져 있고, 둘 중 하나가 v0.10(flow→ANN 입력 등) 과정에서 수정될 때 다른 쪽이 침묵하게 깨진다.
- **변경 전파 범위**:
  - 함께 바뀔 파일: `driver.rs` (강도/게이트/rrf 시그니처 변경 시), `live.rs` (동일), `driver.rs`의 테스트(강제 무손상 골든), `live.rs`의 `engine_selection_is_deterministic_with_same_seed` 테스트.
  - 전파 원인: 두 함수가 동일한 상태 천이 사양을 "라이브러리가 아니라 두 함수의 본문"으로 표현하고 있음.
  - 결합 유형: Functional.
- **제안**: per-tick 시퀀스를 단일 함수로 추출. 시그니처 예:
  ```rust
  pub struct Decision {
      pub chosen: Option<PersonaId>,
      pub rrf_reason: Option<String>,
      pub utterance_event: Option<Event>,
      pub suppressed: Option<(PersonaId, f64)>,
  }
  pub fn decide_one_tick<R: Rng>(
      state: &mut EngineState,
      config: &EngineConfig,
      personas: &[Persona],
      mu_scale: f64,
      rng: &mut R,
  ) -> Decision
  ```
  `driver::run`은 `Decision`을 받아 `ObservationRecord`로 직렬화하고, `LiveSession::tick`은 추가로 `pending`/`human_focus`/워커 디스패치만 얹는다. 골든 보존은 기존 rng 순서를 함수 내부에서 한 번만 표현하면 자동으로 양쪽이 같이 옴.
- **리스크**: 중간 - 단위 테스트가 두 경로에서 거의 같은 시퀀스를 직접 검사하고 있어(특히 `live.rs`의 `mu_scale_returns_one_for_empty_content_history`, `fake_backend_produces_no_flow_in_records`) 추출 후에도 통과하도록 맞춰야 함. 골든 자체는 결정성 보존이 코어 추출과 정렬되므로 더 강해짐.
- **우선순위**: **높음**. v0.10에서 임베딩·ANN이 `decide_one_tick` 호출 직전/직후로 들어올 가능성이 큰데(예: recall 게이팅), 두 함수가 따로 놀면 이중 수정이 강제됨.

### 2-2. `LiveSession`이 `PersonaRuntime`을 직접 안 가짐 - `BackendPool`을 통해 우회

- **위치**: `src/live.rs:68-72` (`#[allow(dead_code)] pool: Arc<BackendPool>`)
- **분류**: 결합 강도 / 모델
- **결합 태그**: Model
- **문제**: `LiveSession`은 생성 호출을 직접 안 하지만 워커가 `Arc<BackendPool>`을 소유한다. **타입(BackendPool)이 라이브 세션의 필드에 들어와 있고 그 자체로는 dead code**.
- **왜 문제인가**: `LiveSession`의 공개 API 시그니처에 `Arc<BackendPool>`이 박혀 있어, `EngineConfig`처럼 값으로 다룰 수 있는 작은 데이터와는 다르게 라이브러리 외부(테스트, web sink)에서 구성하기 까다로워짐. v0.10의 `web 프런트엔드` 트랙에서 sink 추상화(WebSocket)가 들어오면 더 두꺼워질 수 있음.
- **제안**: `LiveSession`은 `pool`을 들고 있지 말고, `with_pool(store, pool)` 또는 트레이트 객체로 좁히는 방향. **단, 현재 동작은 정상이고 테스트도 통과 중이므로 성급한 추상화 위험**이 있음.
- **리스크**: 중간. v0.10 / web 단계에서 web sink가 동일한 결합을 또 복제할 가능성 큼.
- **우선순위**: **낮음**(지금) → **중간**(v0.10 착수 직전).

### 2-3. `chat_personas` ↔ `system_prompts` 묵시적 1:1 결합

- **위치**: `src/main.rs:271-306` (`chat_personas`), `main.rs:312-377` (`demo_persona_system_prompts`), `main.rs:380-396` (`demo_persona_modifiers`)
- **분류**: 결합 강도 / 변동성 정렬
- **결합 태그**: Model (`PersonaId` 문자열 키)
- **문제**: 세 함수가 모두 `PersonaId` 문자열("friend", "chaos", "summarizer")을 하드코딩 키로 공유. 키가 한 곳에서 바뀌면 세 곳을 다 수정해야 함.
- **왜 문제인가**: 페르소나 추가/이름 변경 시 silent fallback(`BTreeMap::get`의 `Option`)이 분기마다 다름. 예: `system_prompts`는 키가 없으면 기본 prompt 폴백이 없고, `modifiers`는 1.0 기본값으로 폴백.
- **제안**: `chat_personas`에서 `id → (name, base_rate, system_prompt, modifier)` 단일 테이블 1개로 합치고 `chat_config` 빌더가 그걸 소비. **v0.10 이전에 페르소나 5개 이상으로 늘릴 계획이 없다면 건드리지 말 것**.
- **리스크**: 중간. 추상화 시 매개변수 폭이 커지면 가독성 저하.
- **우선순위**: 낮음.

---

## 3. 경계와 거리

### 3-1. `human.rs` ↔ `live.rs` - `HumanChannel::speak` 직접 호출

- **위치**: `src/human.rs:75-100` ↔ `src/live.rs:218-234`
- **분류**: 경계와 거리 / 결합 강도
- **결합 태그**: Contract (양호)
- **현재 평가**: **문제 없음**. `HumanChannel`의 단일 책임(외부 이벤트 → state projection)이 깨끗하다. driver 경로도 같은 `HumanChannel::speak`을 거치므로 INV("headless 골든 경로 불침투")가 잘 유지됨.

### 3-2. `flow.rs` ↔ `memory.rs` (vec_impl) - `flow::tokenize`를 memory가 빌림

- **위치**: `src/memory.rs:80-90` (vec_impl `recall` 내부) → `src/flow.rs:32-50` (`pub(crate) fn tokenize`)
- **분류**: 경계와 거리 / 변동성 정렬
- **결합 태그**: Contract (양호)
- **현재 평가**: **문제 없음**. SQLite feature on일 때는 `tokenize_ko::morphological_tokens`로 대체되므로 분기처리가 이미 깔끔하다. 다만 `tokenize`가 `flow` 모듈에 살고 있어 "flow의 토크나이저"가 사실은 프로젝트 전역의 라이트 토크나이저다. **v0.10에서 어휘/의미 양쪽에 공통 토크나이저가 필요해질 때 `tokenize.rs`로 분리**.

### 3-3. `preset.rs` ↔ `hawkes.rs` - 안정성 정규화 의존

- **위치**: `src/preset.rs:114-130`
- **분류**: 경계와 거리
- **결합 태그**: Contract (양호)
- **현재 평가**: **문제 없음**.

### 3-4. `main.rs`의 모든 경로(4-갈래) 분기

- **위치**: `src/main.rs:94-237`
- **분류**: 책임
- **결합 태그**: 해당 없음
- **현재 평가**: 솔로 프로젝트 + 4갈래 한정. **지금 건드리지 말 것**.

### 3-5. 의존성 방향 / 순환 의존

- **관찰**: `lib.rs:1-29`의 module 선언을 보면, `flow`/`meta`/`memory`가 driver/live로 들어가는 단방향. `ann`/`embed`는 feature-gated. `chat`이 `live`을 import. **순환 없음**.

---

## 4. 변동성 정렬

### 4-1. `[높음]` driver.rs ↔ live.rs per-tick 복제 (위 2-1과 동일)

- **변경 전파 범위**:
  - 함께 바뀔 파일: `driver.rs`, `live.rs`, 둘 다의 `#[cfg(test)] mod tests`, `tests/smoke_*.rs` 중 `live` 경로 검증하는 것.
  - 전파 원인: 동일 알고리즘 사양이 두 함수 본문에 박혀 있고, 둘 중 하나가 변경되면 다른 쪽의 `// driver와 동일` 주석이 깨짐.
  - 결합 유형: Functional.
- **이미 2-1에 보고된 항목. 우선순위 높음.**

### 4-2. `[중간]` `MemoryStore` sqlite/vec 분기가 같은 모듈 안에 공존

- **위치**: `src/memory.rs:50-130` (vec_impl) ↔ `src/memory.rs:148-880` (sqlite_impl) ↔ `src/memory.rs:946-` (재수출 분기)
- **분류**: 변동성 정렬 / 결합 강도
- **결합 태그**: **Model** (기록 구조 / 트레이트 없이 같은 시그니처 두 구현)
- **문제**: 같은 `MemoryStore` 이름이 feature flag로 완전히 다른 구현체(`BTreeMap<Vec<MemoryEvent>>` vs `rusqlite::Connection`)를 가리킨다. 두 모듈이 같은 공개 시그니처(`recall`, `format_recall`, `join`, `record`)를 손으로 동기 유지. v0.10에서 세 번째 구현(hybrid) 또는 `MockEmbedder` 기반 분기가 들어온다.
- **왜 문제인가**:
  - `record` / `recall` 본문이 sqlite_impl에 80줄 단위로 복제돼 있고, 차이는 SQL과 인덱스뿐. recall 본문 내 동적 SQL과 params_from_iter는 sqlite 버전 한 곳에만 있는 코드라, vec_impl의 `recall`이 이 분기를 모른다. **v0.10이 들어오면 네 번째 분기 또는 세 번째 분기가 추가될 가능성이 높다**.
- **변경 전파 범위**:
  - 함께 바뀔 파일: `src/memory.rs` 내부 feature 분기 전체, `tests/recall_eval.rs`, `live.rs`의 `with_store` 호출처.
  - 전파 원인: feature flag별 코드 분기 + `pub use` 라인이 한 곳에 모여 있음.
  - 결합 유형: Model.
- **제안**: 변경 폭이 작으려면 **trait + impl Vec/Hybrid** 방향. 단, 현 시점에서 v0.10의 hybrid까지 가지 않은 상태에서 추상화하면 한 번 더 리팩토링 필요. **실용적 제안**: 두 구현의 `recall` 본문이 공유하는 보조 함수(`placeholders_for_rooms(rooms) -> String`, `escape_fts_token(t) -> String`)를 sqlite_impl 내부 `fn`으로 모으고, vec_impl은 그대로 두기. v0.10 착수 시점에 trait 도입 결정.
- **리스크**: 중간. trait 도입은 `LiveSession`의 제네릭 화 또는 `Box<dyn MemoryStore>` 이슈와 직결.
- **우선순위**: **중간**(v0.10 task-47~50과 동시에 결정). 지금은 안 건드림이 더 안전.

### 4-3. `meta.rs` - `from_env`이 `main.rs`에서 직접 호출되지 않음

- **위치**: `src/meta.rs:32-47`
- **분류**: 변동성 정렬
- **결합 태그**: Contract (양호)
- **현재 평가**: **문제 없음**. 한 곳에서 env를 읽고, 모듈 자체가 순수 결정적(테스트 가능). `from_env`이 main.rs에 누수되지 않음.

### 4-4. `tokenize_ko.rs` - feature-gated 한정

- **위치**: `src/tokenize_ko.rs`
- **분류**: 변동성 정렬
- **결합 태그**: Contract (양호)
- **현재 평가**: **문제 없음**. friend-engine feature 뒤에 격리, 골든 보존을 위한 명확한 분기.

### 4-5. `live_store()` 결정성/영속 경로의 외부 효과

- **위치**: `src/memory.rs:413-440` (위 1-1과 일부 중복)
- **변경 전파 범위**:
  - 함께 바뀔 파일: `main.rs`(직접 호출), `live.rs`(docstring 경고), 그리고 v0.10의 web sink가 같은 경로를 import할 가능성.
  - 전파 원인: 디스크 I/O가 라이브러리 함수에 결합됨.
  - 결합 유형: Contract (side-effect 강함).
- **우선순위**: 위 1-1과 함께 **중간**.

---

## 5. 책임 분리

### 5-1. `[중간]` `live.rs`가 너무 많은 일을 함 (1059줄, 13개 필드, 15+ 메서드)

- **위치**: `src/live.rs:62-117` (struct 본체), `src/live.rs:155-244` (`with_store` 90줄)
- **분류**: 책임
- **결합 태그**: 해당 없음
- **문제**: `LiveSession` 단일 struct가 7가지 책임을 한꺼번에 들고 있다:
  1. 엔진 상태(`state`, `rng`, `personas`, `config`)
  2. 외부 입력(`human`, `human_id`, `last_human_msg`, `human_focus`)
  3. 비동기 디스패치(`job_tx`, `result_rx`, `worker`, `pending`)
  4. 회상 스토어(`store`, `room`)
  5. 화제 태그(`topics`, `build_directive`)
  6. 라벨 정규화(`speaker_labels`, `strip_speaker_prefix`)
  7. 거시 측정(`meta`, `flow()`, `mu_scale()`)
- **왜 문제인가**:
  - `submit_human` ↔ `tick` ↔ `poll_generation`이 13개 필드를 동시에 만진다. 단위 테스트가 14개나 있는 게 이 결합을 증명한다.
  - 변동 이유가 7가지(외부 입력/디스패치/회상/화제/라벨/거시/엔진)이다. **이건 책임 과다**.
- **변경 전파 범위**:
  - 함께 바뀔 파일: `main.rs`(LiveSession 소비), `chat.rs`(ChatApp이 LiveSession에 의존), 그리고 v0.10의 web sink가 `LiveSession`을 그대로 가져갈 가능성.
  - 전파 원인: LiveSession이 한 덩어리라 변경 시 모든 필드를 같이 옮겨야 함.
- **제안**: 분리는 가능하지만 **지금 변동성이 높은 부분이 아니라면 비용이 큼**. 핵심 분리 후보:
  1. **워커 디스패치** (`Job` + `Result` + `worker`) → `GenerationDispatcher` (Arc 공유 가능)
  2. **외부 입력** (`HumanChannel` + `last_human_msg` + `human_focus`) → 작은 도메인 객체 `HumanFocus`로 추출
  3. **라벨 정규화** → `SpeakerLabeler`로 작은 모듈
  4. **화제 태그** → 작은 모듈로 추출 가능
  5. 엔진 상태와 거시 측정(`flow`/`meta`/`mu_scale`)은 코어
- **리스크**: 중간~높음. 한 번에 다 분리하면 회귀가 여러 모듈에 흩어짐. **1~2개씩 단계적으로**.
- **우선순위**: **중간**. v0.10 착수 전 "디스패치 분리"는 신중히 검토, "라벨/화제 분리"는 1-step. 안 건드리는 선택지도 합리적.

### 5-2. `driver.rs::run`의 단일 함수 vs 단계 함수

- **위치**: `src/driver.rs:18-117`
- **분류**: 책임
- **현재 평가**: **문제 없음**. `initial_state`가 분리돼 있고, 본문은 1-tick 루프 + sink 호출. 책임(틱 실행 + 레코드 emit)이 단일.

### 5-3. `preset.rs::build_config_with_modifiers` - 책임 양호

- **위치**: `src/preset.rs:84-141`
- **현재 평가**: **문제 없음**. α 행렬 빌드 한 가지 책임. 모디파이어 처리가 단일 함수 안에 깔끔히 들어 있음.

---

## 6. 코드 품질

### 6-1. `[중간]` 매직 넘버 / 공유 상수 중복

- **위치**:
  - `driver.rs:18` (`FLOW_WINDOW: usize = 6`)
  - `live.rs:506` (`FLOW_WINDOW: usize = 6` - **driver와 동일 상수 두 번**)
  - `live.rs:131` (`HUMAN_FOCUS_TURNS = 4`), `live.rs:338-343` (`RECALL_K: usize = 3`)
  - `main.rs:11-15` (`DEFAULT_SEED`, `DEFAULT_TICKS`, `DEFAULT_BETA`, `DEFAULT_THETA`, `DEFAULT_K`, `DEFAULT_DELAY_MS`, `TICK_INTERVAL`)
  - `chat.rs:36-38` (`TICK_PERIOD`, `POLL_TIMEOUT`, `BAR_WIDTH`)
  - `human.rs:8-18` (`HUMAN_MARK`, `DEFAULT_ATTENTION`, `DEFAULT_RESET_FACTOR`)
- **분류**: 코드 품질
- **문제**: 같은 상수(`FLOW_WINDOW = 6`)가 `driver.rs`와 `live.rs`에 **동일 값으로 두 번 정의**되어 있고, 한쪽만 바뀌면 엔진 결정성/측정 윈도우가 분기한다.
- **제안**: `flow.rs` 또는 `driver.rs`의 `pub const FLOW_WINDOW: usize = 6;`로 한 곳에 두고 양쪽이 import. `main.rs`의 `DEFAULT_*`는 이미 단일 위치라 OK.
- **리스크**: 낮음.
- **우선순위**: **중간**(per-tick 로직 통합과 함께 처리하면 가장 자연스러움).

### 6-2. `live.rs::strip_speaker_prefix`와 `build_directive`의 모듈 위치

- **위치**: `src/live.rs:75-129`
- **분류**: 코드 품질 / 책임
- **현재 평가**: 단위 테스트가 둘 다 `live.rs::tests` 안에 있어 locality는 양호. 다만 `chat.rs`의 화자 라벨 표시 코드와 중복 가능성이 있어, v0.10에서 web sink에 동일 로직이 필요해질 때 작은 헬퍼 모듈로 추출.
- **우선순위**: 낮음.

### 6-3. `task` docstring 일부가 길고 v0.x 회귀 노트로 가득

- **위치**: `src/driver.rs:34-37`("FakeBackend → content 항상 None..."), `src/live.rs:280-284` 등 다수.
- **분류**: 코드 품질
- **현재 평가**: 의도된 회귀 방지 코멘트. **문제 없음** - 골든 바이트 보존이 핵심 invariant이므로 정당화됨.

### 6-4. `MemoryEvent.content` 빈 문자열 가능성

- **위치**: `src/memory.rs:23-30`
- **분류**: 코드 품질
- **현재 평가**: `format_recall_impl`이 빈 `content`도 그대로 직렬화한다. 라이브에서 빈 content가 들어갈 수 있는데(`pending` 자리표시자 직후), `record` 호출처가 그쪽을 막고 있는지 확인 필요 - `live.rs:240-242`는 사람 발화만 `content`를 보장, 도착 발화는 `content.is_some()` 검사 후 기록(`live.rs:464-470`).
- **제안**: 실행하지 않았으므로 **확인 필요**. 빈 content가 store에 들어가는 경로가 있는지 코드만으로는 단정 어려움.
- **우선순위**: 낮음.

### 6-5. `live.rs::tick` 내 5단계 분기 복잡도

- **위치**: `src/live.rs:286-454`
- **분류**: 코드 품질
- **현재 평가**: 168줄의 단일 함수. `pending` 분기, `human_focus` 분기, `topics` 분기, `recalled` 분기가 한 함수 본문에 섞여 있다. **per-tick 로직이 한 단계 더 길어지면 cyclomatic이 폭증**.
- **제안**: 위 2-1 + 5-1과 함께 처리. `decide_one_tick` 추출 + `human_focus`/`topics`의 분기는 그 위에 얹기.
- **우선순위**: **중간**.

### 6-6. 테스트의 `eprintln!` / `!json.contains` 검증 패턴

- **위치**: `src/driver.rs:319-330` 등
- **분류**: 코드 품질
- **현재 평가**: 직렬화 결과 문자열에 `"flow"` 키가 없는지 `!json.contains("\"flow\"")`로 검사. 이 방식은 결정적이지만 직렬화 포맷에 약하게 결합. **의도된 골든 보존 패턴이며 정당화됨**. 건드리지 않음.
- **우선순위**: 없음.

---

## 가장 시급한 리팩토링 3가지

| 순위 | 항목 | 이유 | 예상 수정 범위 |
|---|---|---|---|
| 1 | **`driver::run` ↔ `live::tick`의 per-tick 알고리즘 통합** | 같은 시퀀스가 두 곳에 코딩되어 있고, v0.10(임베딩/ANN), v0.7 boost, 또는 v0.6의 flow 시그니처 변경 시 **양쪽 동기 비용이 폭발**한다. 결정성(rng 소비 순서)이 의존하는 단일 사양이 두 곳에 박혀 있는 게 가장 큰 변동성 폭탄 | `src/driver.rs`와 `src/live.rs`에서 `decide_one_tick` / `apply_speak` 2개 함수를 추출. 골든 + 222 tests는 결정성만 보존하면 그대로 통과 예상. **4-8시간** |
| 2 | **공유 상수(`FLOW_WINDOW = 6`)를 `flow` 또는 driver 한 곳으로 모으기** | `driver.rs:18`과 `live.rs:506`에 동일 값이 박혀 있고, 한쪽만 바뀌면 **엔진 결정성과 측정 윈도우가 분기**. per-tick 통합과 동반 처리하면 1-step | `pub const FLOW_WINDOW: usize = 6;`를 단일 위치로 이동. 5-10분. **단독도 가능** |
| 3 | **`live_store()`의 side-effect를 `main.rs` 전용으로 좁히기** | 글로벌 `~/.local/share/tunaSalon/memory.db`가 라이브러리 함수의 부작용으로 잠들어 있고, 테스트/스모크 회귀가 한 줄짜리 import로 사용자 디스크를 건드릴 수 있음 | `pub(crate)` + `#[cfg(not(test))]` 또는 `live_store()` 호출을 `main.rs` 한 곳으로 격리. **1-2시간** |

---

## 지금 건드리지 말아야 할 것

| 항목 | 이유 |
|---|---|
| `LiveSession`의 한꺼번 모듈 분리(워커/인풋/라벨/화제/회상) | 변동성 낮은(잘 작동하는) 코드를 7개 책임으로 한 번에 쪼개면, 1059줄에 분산된 14개 테스트의 회귀 비용이 크다. 변동이 일어날 때(v0.10/web) 점진적으로 |
| `MemoryStore`를 `trait` + 다중 impl 구조로 추상화 | v0.10이 vec/sqlite/hybrid **세 가지 분기**를 한꺼번에 도입할 가능성이 크다. 지금 추상화하면 한 번 더 리팩토링이 필요. v0.10 착수 시점에 trait 결정 |
| `main.rs`의 4-갈래 분기 정리 | 솔로 프로젝트 + 4갈래 한정. 분기 자체가 책임을 분리함 |
| `chat.rs::render_chat` 분리 | 순수 렌더 함수 + `ChatApp`(이벤트 루프) 두 책임이 이미 깔끔히 나뉘어 있고 693줄이 둘을 합친 자연스러운 크기 |
| `human.rs`의 `speak`이 `&mut EngineState`를 직접 mutate | 단일 책임("외부 이벤트 → state projection")이고, `HumanChannel` 자체엔 상태가 거의 없어(`speaker_id`/`attention`/`reset_factor`) 인터페이스가 안정적 |
| `meta.rs`의 `from_env`을 `main.rs`에 누수시키지 않음 | 이미 잘 격리됨 |
| `rrf::select`의 5-시그널 동적 융합 | 골든 보존 + INTERST/ECHO 시그널 추가의 의도가 코드와 코멘트에 정확히 박혀 있다. 결정성 회귀 위험이 큰 코드를 손대지 않는 게 옳음 |
| 결정성 보존 코멘트(`// driver와 동일`, `// 골든 보존`) | 의도된 invariant 문서화. 회귀 방지의 1차 방어선 |

---

## 확인 필요 항목

| 항목 | 확인이 필요한 이유 |
|---|---|
| `LiveSession`의 `Arc<BackendPool>`을 `chat.rs`가 직접 만지는지 (`#[allow(dead_code)]` 마커가 붙어 있어 의도적 dead_code가 아닌지) | v0.10 web sink 설계 시 트레이트 분리가 필요한지 결정 |
| `live_store()`가 `friend-engine` 빌드 + `chat.rs` 테스트 안에서 우연히 호출되는 경로가 있는지 (현재는 `new()`만 부르는 게 보이지만) | v0.10 hybrid 평가에서 mock + 실모델 분기 시 깨질 수 있음 |
| `MemoryEvent.content` 빈 문자열이 `store`에 들어갈 수 있는 경로 (placeholder→poll 사이, 라이브 워커가 `None` 반환 시 등) | 빈 content가 `format_recall_impl`에 들어가면 `- speaker: `로 출력되어 프롬프트에 의미 없는 행이 끼어들 가능성 |
| `chat::ChatApp::new` 실패 시 raw-mode 복원 경로 (`Drop` 자동 복원에 의존) | 비-TTY 환경에서 실패했을 때 사용자가 raw-mode 잔재 상태로 남는 가능성 |
| v0.9의 `recall_eval` 결정성 보존이 `MockEmbedder`(feature on일 때)와 `vec_impl`(feature off일 때) **양쪽 시나리오**에서 깨지지 않는지 | 실행 안 했고, feature별 recall 분기문은 봤지만 결정성 단언은 직접 못 함 |
| `id = "chaos"`인데 표시명은 "Grounded Realist"인 `demo_personas`/`chat_personas`가 `BTreeMap<PersonaId, _>` 키로 `"chaos"`만 쓰는 점(의도적 골든 보존) | v0.10에서 페르소나 id/name이 분리될 때 `coupling matrix`/`participation`이 어느 키를 쓰는지 일관성 검토 필요 |

---

## 부록: 검토 대상 파일 통계

| 파일 | 줄 수 | 책임 |
|---|---|---|
| `src/memory.rs` | 1494 | MemoryStore(vec/sqlite), feature-gated |
| `src/pool.rs` | 1135 | BackendPool + 라우팅 + 폴백 |
| `src/live.rs` | 1059 | LiveSession(엔진+디스패치+인풋+회상+화제+라벨+거시) |
| `src/chat.rs` | 693 | ChatApp + render_chat |
| `src/rrf.rs` | 642 | 5-시그널 동적 RRF |
| `src/main.rs` | 595 | CLI 4-갈래 |
| `src/openai.rs` | 514 | OpenAI 호환 백엔드 |
| `src/hawkes.rs` | 472 | Hawkes 강도 동역학 |
| `src/embed.rs` | 435 | Embedder + Mock/Ort(feature-gated) |
| `src/ann.rs` | 422 | usearch HNSW(feature-gated) |
| 합계 | 10,510줄 | |

---

## 부록: 다음 스냅샷 권장 시점

- v0.10 done 직후 (`friend-engine-semantic` feature가 합쳐진 직후 - `MemoryStore` 분기 복잡도 재평가)
- web 프런트엔드 1차 구현 직후 (sink 추상화 도입 후 결합 재평가)
- 페르소나 합성 1차 구현 직후 (`chat_personas` ↔ `system_prompts` 결합 재평가)

각 스냅샷은 `refactoring-review-v{N}-{milestone}.md`로 보관하고 `index.md`에 등록한다.
