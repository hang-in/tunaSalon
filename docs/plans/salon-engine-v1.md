---
type: plan
status: in_progress
updated_at: 2026-06-02
version: v1
title: Salon 엔진 실행 플랜 v1
design_ref: ../reference/salon-engine-design.md
---

# Salon — 플랜 v1

대화 흐름 엔진 Salon의 실행 플랜. 설계 근거는 `docs/reference/salon-engine-design.md`(DESIGN)에 있고, 이 문서는 무엇을 어떤 순서로 만드는지를 정한다.

핵심 원칙 네 가지로 시작한다.

1. 엔진이 본체다. 페르소나/LLM은 부품이다.
2. LLM 없이 먼저 만든다. 발화 리듬은 내용과 무관하게 검증된다.
3. α(케미)는 v0.1에서 끈다. v0.1 목표는 μ(수다력)만으로 달성된다.
4. 모듈은 토글 가능하게. 한 번에 다 켜지 않는다.

---

## 1. 단계 로드맵

| 버전 | 한 줄 목표 | 새로 켜는 것 | LLM |
|---|---|---|---|
| v0.1 | μ 차이만으로 발화/침묵 리듬이 나오는가 | HawkesEngine(μ, self-decay), SilenceGate, RRF(시간·균형·난수), ObservationSink(TUI+headless), DebugMeter, 고정 seed 스모크, fake utterance | 없음 |
| v0.2 | 페르소나 케미가 생기는가 | α 행렬(room preset + persona modifier), FSM 전이 | 없음/선택 |
| v0.3 | 작은 모델이 페르소나를 유지하는가 | PersonaRuntime, Ollama 1명, 내용 기반 RRF(관심도·잔향) | 1명 |
| v0.4 | 동시 호출이 안정적인가 | 백엔드 풀, max_concurrent, 큐·타임아웃, e4b 3개 병렬 | 2~3명 |
| v0.5 | 대화가 식는 걸 잡는가 | FlowMeter(키워드/유사도 근사 → BGE-M3) | 2~3명 |
| v0.6 | 거시 피드백이 안정적인가 | MetaController(약하게) | 2~3명 |

각 단계는 앞 단계의 완료 기준을 만족한 뒤에만 다음으로 간다.

---

## 2. v0.1 상세 (MVP)

### 목표
LLM 없이, fake utterance만으로 대화방의 발화/침묵 리듬이 페르소나 μ 차이로 다르게 나오는지 확인한다.

성공하면: 같은 엔진에서 "수다 많은 애 / 조용한 애 / 가끔 끼어드는 애"가 숫자 튜닝만으로 구분된다. 이게 확인되면 프로젝트가 살아난다.

### 포함 / 제외

| 포함 | 제외 |
|---|---|
| 페르소나 1~3명 (μ만) | Ollama / 모든 LLM |
| HawkesEngine (base rate + self-decay) | α 행렬 / 케미 |
| SilenceGate (θ) | FSM 전이 |
| RRF (시간 강도 · 발화 균형 · 난수) | FlowMeter / 임베딩 |
| ObservationSink Contract (코어 ↔ 출력 분리) | MetaController |
| DebugMeter = TUI sink (λ 막대 · 선택 이유 · 카운트) | persistence / 세션 메모리 |
| headless sink (결정적 NDJSON) + 고정 seed 스모크 | 성능/부하 벤치 |
| fake utterance generator | persona randomizer / 병렬 호출 |
| EngineConfig (β, θ, k) | 진짜 LLM 내용 · 관심도 신호 |

α를 끄므로 v0.1의 강도는 교차 자극(케미) 없이, μ(기본 수다력) + 발화 후 자기 억제(방금 말했으니 잠깐 쉼) + 시간 회복으로만 움직인다. 교차 자극이 없으니 Hawkes 폭주 위험도 v0.1에선 자동으로 없다.

### 데이터 모델 (최소)

| 구조 | 필드 | 의미 |
|---|---|---|
| Persona | id, name, base_rate(μ) | model/prompt는 v0.3부터 |
| EngineConfig | β(감쇠), θ(게이트 임계), k(RRF), tick_interval | 핵심 튜닝 손잡이. 하드코딩 금지 |
| Event | ts, speaker, mark | content는 v0.3부터 |
| EngineState | intensities(λ map), history, last_speaker, rng_seed | seed는 재현성용 |
| ObservationRecord | tick, ts, intensities, gate_passed, candidates, chosen, rrf_reason, counts | sink로 내보내는 틱 스냅샷. NDJSON 한 줄 = 한 record |

CouplingMatrix(α)는 구조만 두고 v0.1에선 사용 안 함(전부 0 또는 미주입).

ObservationRecord는 코어가 sink에 넘기는 유일한 출력 계약이다. TUI도 headless도 같은 record를 받으므로, 둘 사이에 동역학 차이가 생기지 않는다.

### v0.1 틱 루프

매 틱:

1. 강도 갱신 — 경과 시간만큼 각 λ_p를 μ_p 쪽으로 회복. 방금 발화한 페르소나는 억제 상태에서 서서히 복귀.
2. 게이트 — max(λ_p)가 θ 미만이면 침묵, 다음 틱.
3. RRF — θ를 넘은 후보를 [강도 순위, 발화 균형 순위, 난수 순위]로 융합해 1명 선택.
4. fake 발화 — 선택된 페르소나가 가짜 발화(라벨만, 내용 없음) 출력. 원하면 임의 화제 태그를 붙여 v0.3 관심도 신호를 미리 흉내.
5. 갱신 — 발화 이벤트 기록, 그 페르소나 λ 억제. 이번 틱 ObservationRecord를 sink로 방출(TUI 갱신 또는 headless NDJSON 한 줄).

### 완료 기준 (검증)

각 기준은 headless 고정 seed 출력(NDJSON)으로 자동 검증한다. 스모크 테스트(`cargo test --test smoke`)가 아래를 assert하고, 사람은 같은 seed를 TUI로 눈으로도 확인할 수 있다.

- μ가 높은 페르소나가 같은 시드에서 더 자주 발화한다. **단, μ→빈도 차이는 θ 중간 구간에서만 선명하다**(2026-06-02 스윕 관측): θ가 너무 낮으면 모두 포화해 차이가 사라지고(θ=0.40 friend=chaos=100), 너무 높으면 낮은 μ가 0으로 굶어 순서는 자명하나 방이 대부분 침묵한다(θ=0.78 chaos=0). 차등은 중간 θ에서 또렷하다(θ=0.65 friend 62 > chaos 38). 스모크는 θ=0.65를 쓴다.
- θ를 올리면 침묵 빈도가 늘고, 내리면 방이 시끄러워진다.
- (기준 c) 분산은 balance 신호가 담당한다. k는 동점·순간 선택의 미세조정 손잡이이고, balance가 켜진 long-run 분포에는 k 효과가 거의 안 나타난다(2026-06-02 헤드리스 관측: θ=0.1·200틱에서 k=1과 k=1000의 화자 분포 동일). 격리 검증(a): history를 비워 balance를 중립화하고 intensities를 고정하면, 작은 k는 intensity 1등 독점·큰 k는 분산이 순간 선택 수준에서 확인된다.
- 미터에서 "왜 이번에 이 페르소나가 말했는지"(어느 RRF 신호가 1등)가 읽힌다.

> 구 기준 4 "대화 길이/패턴이 seed별 분포를 이룬다"는 **v0.2 완료 기준으로 이관**했다(아래 §3 v0.2). α=0인 v0.1에선 동시 후보가 드물어 거의 결정적이라 약한 기준이었다.

### v0.1 작업 항목

1. EngineConfig / Persona / Event / EngineState / ObservationRecord 정의(α 자리는 두되 미사용).
2. HawkesEngine: μ + self-decay 회복 모델, 발화 시 억제.
3. SilenceGate: θ 임계 판정.
4. SignalCollector + RRF Fuser: 시간 강도 · 발화 균형 · 난수 세 신호.
5. fake utterance generator: 라벨 발화(+선택적 화제 태그).
6. 틱 루프 드라이버(고정 간격, seed 고정). 매 틱 ObservationRecord를 sink로 방출.
7. ObservationSink Contract + headless writer: 결정적 NDJSON을 stdout으로. CLI 진입점 `--headless --seed <N> --ticks <N>`.
8. DebugMeter (TUI sink): λ 막대, 선택 이유, 침묵/발화 카운트, 대화 길이.
9. 스모크 테스트(`cargo test --test smoke`): 고정 seed headless 출력으로 완료 기준 5개를 assert.
10. 파라미터 스윕 모드: μ/θ/k를 바꿔가며 같은 seed로 리듬 비교.

### v0.1 작업 분해 (task 문서)

위 작업 항목을 구현 지시서로 떼어냈다(8개). Developer는 Phase 순서대로, 같은 group은 병렬로 진행한다.

| task | 제목 | plan 항목 | depends_on | group |
|---|---|---|---|---|
| [task-01](salon-engine-v1-task-01.md) | 스캐폴드 + 코어 타입 + ObservationSink | 1 | - | A 기반 |
| [task-02](salon-engine-v1-task-02.md) | HawkesEngine (μ, self-decay) | 2 | 01 | B engine-core |
| [task-03](salon-engine-v1-task-03.md) | SilenceGate (θ) | 3 | 01 | B engine-core |
| [task-04](salon-engine-v1-task-04.md) | SignalCollector + RRF | 4 | 01 | B engine-core |
| [task-05](salon-engine-v1-task-05.md) | fake utterance generator | 5 | 01 | B engine-core |
| [task-06](salon-engine-v1-task-06.md) | 틱 루프 + headless + CLI | 6, 7 | 02,03,04,05 | C 통합 |
| [task-07](salon-engine-v1-task-07.md) | DebugMeter (TUI sink) | 8 | 06 | D 출력 |
| [task-08](salon-engine-v1-task-08.md) | 스모크 테스트 + 파라미터 스윕 | 9, 10 | 06 | D 출력 |

Phase A(기반) → B(엔진 코어, 병렬) → C(통합/실행) → D(출력/검증, 병렬). v0.1 완료 게이트는 task-08의 `cargo test --test smoke` 통과다.

---

## 3. v0.2 이후 단계 요약

### v0.2 — 케미 (α)
α를 도입하되 N×N을 손으로 채우지 않는다. room preset이 α의 전역 성격을 정하고(calm/pub/argument/chaos), persona modifier가 쌍별 비대칭을 더한다. preset만 쓰면 같은 방에서 모두 같은 자극이라 케미가 사라지므로(케미 collapse), modifier로 "ENTP→INTJ 자극은 강하고 그 반대는 약함" 같은 비대칭을 복원하는 게 이 단계의 핵심이다. 안정 조건은 α 행렬 spectral radius < 1 유지. FSM 전이(같은 페르소나 연속 금지 등)도 여기서.

> v0.2 메모 (2026-06-02 v0.1 관측): 발화 분산(누가 얼마나 도느냐)을 실제로 조절하는 손잡이는 RRF k가 아니라 **balance 신호의 가중치**다. v0.1 RRF는 신호를 등가중으로 합산(`Σ 1/(k+rank)`)하는데, balance(적게 말한 순)가 분산을 강하게 강제해 k 효과를 덮는다. v0.2에서 신호별 가중치/토글을 도입할 때 balance 가중치를 "리듬 분산" 손잡이로 노출하면 잡담(분산)↔회의(독점) 리듬을 직접 조절할 수 있다. 메모리 [[project-balance-rhythm-knob]] 참조.

**v0.2 완료 기준 (v0.1에서 이관):** 대화 길이/패턴이 seed별로 분포를 이룬다(어떤 런은 짧게, 어떤 런은 길게). v0.1은 α=0이라 거의 결정적이어서 이 기준이 약했다. v0.2의 α(교차 자극)는 동시 후보 경쟁과 burst를 만들어 seed에 따라 길이/리듬이 갈리게 하므로, 이 단계에서 비로소 의미 있는 완료 기준이 된다.

| preset | μ | α | β | θ |
|---|---|---|---|---|
| calm | 낮음 | 낮음 | 높음 | 높음 |
| pub | 중간 | 중간 | 중간 | 중간 |
| argument | 중간 | 높음 | 낮음 | 낮음 |
| chaos | 랜덤 | 높음 | 낮음 | 낮음 |

### v0.3 — LLM 1명
PersonaRuntime이 Ollama 1 백엔드를 추상화. 화자 선택은 엔진이, 생성만 LLM이. 같은 모델(e4b)에 다른 persona prompt를 주입해 persona collapse(작은 모델이 페르소나를 구분 못 함) 여부를 본다. 이때부터 발화에 내용이 생기므로 RRF의 관심도·잔향 신호가 비로소 활성화된다.

> 페르소나 참고 문서(v0.2~v0.3 시점에 사용): 3층 구조·역할별 μ/α 초기값(§6)·런타임 조립 절차·TUI 레이아웃은 `docs/temp/salon-persona-ui.md`. 내용층 프롬프트 조각(MBTI 16/혈액형 4/별자리 12 = 32개) + 시스템 프롬프트 템플릿 + 3슬롯 이름 생성은 `docs/temp/salon-persona-fragments.md`. 역할 기반 α는 v0.2, 조각 주입은 v0.3에서 쓴다. 작은 모델은 collapse가 있어 조각은 짧고 선명하게 유지(원문 주의 참조).

### v0.4 — LLM 2~3명 + 병렬
상세 플랜: `salon-engine-v4.md`. 이종(heterogeneous) 백엔드 풀 + 백엔드별 max_concurrent 세마포어 + 큐 + 타임아웃 폴백. 실제 백엔드 2종: Ollama Cloud(동시성 3, ctx auto-max) + 지인서버 qwen3.6:32b(ctx 100k, 동시성 2). **페르소나별 라우팅**으로 한 방에서 mixed-model(일부 32b, 일부 cloud)이 가능하다. 동시 호출은 persona collapse 비교/벤치 전용이고 **라이브 틱 루프는 순차**(발화 1명/틱 + 인과적 턴테이킹). async(tokio)는 비채택(동시도 ≤5라 std::thread::scope + 세마포어로 충분, blocking reqwest 유지). num_ctx는 백엔드별 Option(None=cloud/원격 auto-max). 라이브 burst는 보류(순차 지연 측정 후 결정). cloud 과금은 GPU시간 정액 구독이라 선형 과금 없음(메모리 [[ollama-cloud-limits]]).

### v0.5 — FlowMeter
수렴/발산을 측정. 처음부터 BGE-M3 가지 말고 키워드 중복/문장 유사도 근사로 시작. 관찰만 하고 피드백은 아직 안 건다.

### v0.6 — MetaController
거시→미시 피드백(수렴 높으면 base rate 낮춰 식힘)을 약하게 켠다. 진동이 나면 게인을 낮춘다. 이 피드백 루프가 전체에서 가장 불안정한 부분이라 맨 마지막, 가장 약하게.

---

## 4. 위험과 대응

| 위험 | 대응 |
|---|---|
| α 행렬 튜닝 지옥 | v0.1에서 끔. v0.2에서 room preset + modifier로 차원 축소 |
| 케미 collapse(preset 단일값) | persona modifier로 쌍별 비대칭 복원(v0.2) |
| Hawkes 폭주 | spectral radius < 1 유지. v0.1은 α≈0이라 자동 안정 |
| MetaController 피드백 진동 | 맨 마지막 단계, 약한 게인부터, 미터로 관찰 |
| fake utterance가 RRF 내용 신호를 못 검증 | 임의 화제 태그로 흉내, 진짜 검증은 v0.3 |
| persona collapse(작은 모델) | v0.3에서 e4b로 조기 확인. 차이를 톤이 아니라 구조(발화 길이·빈도)로 강제 |
| 제품 폭 > 구조 수용력 | 단계마다 완료 기준 통과 전 다음으로 안 감 |

---

## 5. 산출물 구조

- `docs/reference/salon-engine-design.md` — 설계 근거(DESIGN). 알고리즘과 층위.
- `docs/plans/salon-engine-v1.md` — 이 문서(PLAN). 단계 로드맵과 v0.1 스펙.
- 다음으로 만들 것: v0.1 작업 항목을 구현 에이전트용 프롬프트로 떼어내기(필요 시).

v0.1 한 줄: LLM도 α도 없이, μ와 θ와 k만으로 "수다 많은 애·조용한 애·끼어드는 애"가 미터에서 다르게 보이면 성공. 그 차이는 headless 고정 seed 스모크로 자동 검증한다.
