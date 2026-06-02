---
title: Salon 엔진 플랜 v0.6 - FlowMeter (수렴/발산 측정)
type: plan
status: in_progress
priority: P1
updated_at: 2026-06-02
owner: shared
summary: v0.6 - 대화가 식는지(수렴) 살아있는지(발산)를 측정하는 FlowMeter. 처음부터 BGE-M3 가지 말고 키워드/토큰 중복 근사로 시작(design §4). **관찰만** - 엔진 파라미터에 피드백 안 함(그건 v0.7 MetaController). 결정성/골든 보존: flow 지표는 content 있을 때만(content 없으면 None → serde 생략 → 골든 바이트 동일). 채팅 TUI 사이드바에 수렴/발산 게이지(persona-ui §5).
design_ref: ../reference/salon-engine-design.md
roadmap_ref: salon-engine-v1.md
---

# Salon - 플랜 v0.6 (FlowMeter)

## 0. Context

design §4(거시층): "최근 N개 발화를 임베딩해 벡터 분산을 본다. 분산이 작아지면(발화들이 비슷) 수렴, 새 개념이 계속 나오면(분산 큼) 발산. **키워드 중복도로 더 싸게 근사 가능.**" §10 시작 전략: "FlowMeter를 켜되 MetaController는 관찰만." §11: 수렴/발산 임계 = 마무리 시점.

v0.5에서 사람이 참여하는 채팅방이 완성됐다. v0.6은 그 대화가 **달아오르는지 식는지를 숫자로** 본다. 처음부터 BGE-M3 임베딩으로 가지 않고(무겁다), 토큰/키워드 중복 근사로 시작한다(design 명시). **관찰만** - 엔진에 피드백 걸지 않는다(피드백 진동 위험은 v0.7 MetaController에서, 약하게). 이 단계 목표: 수렴/발산 숫자가 의미 있게 움직이는지 미터로 본다.

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | 기본 실행(LLM off, FakeBackend)은 v0.1~v0.5 골든과 바이트 동일. flow 지표는 **content 있을 때만** 계산, 없으면 None → serde 생략(utterance 패턴 그대로) |
| INV-2 | FlowMeter는 **관찰만** - 엔진 결정(누가/언제/침묵)·파라미터에 피드백 금지. 엔진 결정성 불변 |
| INV-3 | FlowMeter 계산은 **결정적**(토큰 중복, rng·네트워크 없음). 같은 입력 → 같은 지표 |
| INV-4 | 토글 가능(design §7): 끄면 flow 계산·출력 없음(코어 오염 없음) |
| INV-5 | v0.5의 114 tests + 스모크 게이트 5종 유지(+ v0.6 게이트) |

## 2. Goals / Non-goals

### Goals
- (G1) **FlowMeter 코어**(`src/flow.rs`): 최근 N개 발화 텍스트 → 수렴 지표(토큰/키워드 중복 근사). 순수·결정적·테스트 가능. BGE-M3는 이후(인터페이스만 교체 가능하게).
- (G2) **ObservationRecord에 flow 필드**(Option, content 없으면 None → serde 생략 → 골든 보존). driver(배치)·LiveSession(라이브) 둘 다 content로부터 계산.
- (G3) **채팅 TUI 사이드바 수렴/발산 게이지**(persona-ui §5: "수렴 ▓░ 발산 ▓▓"). headless NDJSON에도 필드.
- (G4) v0.6 스모크 게이트: 골든 보존 + flow 결정성 + content 게이팅.

### Non-goals
- ❌ MetaController 피드백(flow로 base rate 조정) - **v0.7**. v0.6은 관찰만.
- ❌ BGE-M3 임베딩 - 이후(키워드 근사로 시작). 인터페이스는 교체 가능하게 두되 구현은 근사.
- ❌ 사람 입력으로 화제 선점은 v0.5 HumanChannel이 이미 함(flow의 기준점으로 활용은 가능, 별도 로직 최소).

## 3. 데이터 모델 / 아키텍처 델타

| 구조 | 변경 |
|------|------|
| `FlowMeter`(신규 `src/flow.rs`) | `measure(recent_utterances: &[&str]) -> Option<FlowMetric>`. 토큰 중복 기반 수렴도 ∈ [0,1](1=반복/수렴, 0=새로움/발산). <2개면 None. 순수·결정적 |
| `FlowMetric`(신규) | 예 `{ convergence: f64 }` (필요 시 novelty 등 추가). serde 직렬화 |
| `ObservationRecord` | `flow: Option<FlowMetric>` 추가, `#[serde(skip_serializing_if = "Option::is_none")]`. content 없으면 None → 골든 보존 |
| `driver.rs` / `live.rs` | 매 틱(또는 발화 시) 최근 발화 content로 FlowMeter 계산해 record/state에 반영. content 없으면 None. 토글 플래그 |
| `chat.rs` / `tui.rs` | 사이드바에 수렴/발산 게이지 1줄 |

## 4. Subtasks

| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| 33 | FlowMeter 코어 | `flow.rs`: 토큰 중복 수렴 지표(순수·결정적). 한국어 등은 공백 토큰 근사(design "키워드 근사"). 단위 테스트(반복=고수렴, 새 토큰=저수렴, <2=None) | 낮음(순수). 지표 감각은 라이브로 튜닝 | v0.5 |
| 34 | record/state 배선 + 골든 보존 | ObservationRecord.flow Option(serde 생략) + driver/live가 content로 계산. **content 없으면 None → 골든 바이트 동일**. 토글 | 중(골든 보존이 핵심) | 33 |
| 35 | TUI 게이지 + v0.6 게이트 | chat.rs 사이드바 수렴/발산 게이지(persona-ui §5) + smoke_v6(골든 보존, flow 결정성, content 게이팅) | 낮음 | 34 |

Phase A(33 코어) → B(34 배선·골든) → C(35 TUI·게이트). 완료: task-35 + 골든 보존.

## 5. v0.6 완료 기준

- 기본 `cargo run`(LLM off)·headless 골든 5종 바이트 동일(flow는 content 없으면 생략).
- `--llm`/`--chat`에서 발화 content가 쌓이면 flow 지표가 계산돼 record/사이드바에 나타남.
- flow 지표가 결정적(같은 발화열 → 같은 값). 반복적 대화는 수렴↑, 새 화제는 발산↑.
- FlowMeter 토글 off면 계산·출력 없음.
- **엔진 결정 불변**(관찰만, 피드백 없음).

## 6. 위험과 대응

| 위험 | 대응 |
|------|------|
| flow 필드로 골든 깨짐 | Option + serde 생략, content 없으면 None(utterance 패턴). 골든 5종 재확인 |
| 관찰이 엔진에 새어 피드백됨 | flow는 record/표시 전용. 엔진 파라미터·선택에 불사용(INV-2). v0.7까지 피드백 금지 |
| 토큰 근사가 한국어에 부정확 | v0.6은 근사가 목표(design 명시). 공백/간단 정규화. 정밀도는 BGE-M3(이후)로 |
| 지표 스케일 감각 | 라이브 관전으로 튜닝(엔진 손잡이 철학). 임계는 v0.7에서 쓸 때 정함 |
| 비결정 유입 | 토큰 중복은 rng·네트워크 없음. 결정적 단위 테스트 |

## 7. 산출물

- 이 문서(PLAN v0.6). 구현 시 §4를 `salon-engine-v6-task-33..35.md`로 분해.
- v0.6 한 줄: 대화가 식는지 살아있는지를 토큰 중복으로 싸게 재서 미터에 띄우되, 엔진엔 아직 손대지 않는다(관찰만). 피드백은 v0.7.
