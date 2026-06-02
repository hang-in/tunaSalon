---
title: Salon 엔진 플랜 v0.7 - MetaController (거시→미시 피드백)
type: plan
status: in_progress
priority: P1
updated_at: 2026-06-02
owner: shared
summary: v0.7 - FlowMeter 수렴도를 읽어 μ(base rate)를 낮춰 방을 식히는 거시→미시 피드백. design §4/§10. 가장 불안정한 부분이라 약한 게인 + floor(고착/침묵 방지) + 진동 시 게인↓. content 게이팅: flow None(FakeBackend)이면 mu_scale=1.0 no-op → 골든 바이트 동일(headless 결정 경로 보존). SALON_META_GAIN env 튜닝. 관찰: flow 게이지 + cooling 표시.
design_ref: ../reference/salon-engine-design.md
roadmap_ref: salon-engine-v1.md
---

# Salon - 플랜 v0.7 (MetaController)

## 0. Context

design §4(메타 컨트롤러): "수렴이 임계를 넘으면(다 한 얘기) 모든 base rate μ_p를 낮춰 방을 식히고, 발산이 높으면 그대로 둔다. LLM이 아니라 휴리스틱 레이어." §10: "가장 큰 위험은 MetaController의 피드백 루프다. 거시 상태가 미시 파라미터를 바꾸고 그 결과가 다시 거시에 영향 → 잘못 잡으면 진동(말했다 조용했다 발작)이나 고착(영원 침묵/수다). 약하게 켜서 식히기 피드백을 넣고, 진동 나면 게인을 낮춘다." §11: "MetaController 게인 = 식히기 강도. 너무 크면 진동."

v0.6 FlowMeter가 수렴/발산을 측정만 했다. v0.7은 그 수치를 **엔진에 약하게 연결**한다. 로드맵 시작 전략의 **마지막 단계**(가장 불안정 → 맨 마지막, 가장 약하게).

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | **content 게이팅**: flow None(FakeBackend, LLM off)이면 MetaController는 mu_scale=1.0 **no-op** → 엔진 결정 불변 → v0.1~v0.6 골든 바이트 동일 |
| INV-2 | MetaController는 **휴리스틱 레이어**(LLM 아님). 거시(수렴도) → 미시(μ) 단방향 조정만 |
| INV-3 | **안정성**: mu_scale에 floor(예 ≥0.4)로 완전 침묵/고착 방지. 게인은 약하게 기본, env 튜닝. 단조·유계 |
| INV-4 | 결정성: MetaController 계산 자체는 결정적(flow·gain만, rng 없음). 비결정은 LLM content뿐 |
| INV-5 | v0.6의 168 tests + 스모크 게이트 6종 유지(+ v0.7 게이트) |

## 2. Goals / Non-goals

### Goals
- (G1) **MetaController 코어**(`src/meta.rs`): `cooling(flow: Option<FlowMetric>, gain, threshold, floor) -> f64`(mu_scale ∈ [floor, 1.0]). 수렴 > threshold면 게인 비례로 mu_scale↓, 아니면/None이면 1.0. 순수·결정적·유계.
- (G2) **driver/live 배선**: 매 틱 flow → mu_scale → 강도 갱신에 적용(μ 낮춤). content 없으면 mu_scale=1.0(골든 보존). `HawkesEngine::update_intensities`에 mu_scale 반영.
- (G3) **관찰**: cooling/mu_scale를 record(Option, content 없으면 생략) + 채팅 사이드바에 표시(미터로 진동 관찰).
- (G4) v0.7 스모크 게이트: 골든 보존(no-op without flow) + cooling 단조·유계 + 수렴 시 식힘.

### Non-goals
- ❌ 발산 시 끌어올리기(boost) - v0.7은 **식히기만**(design 우선). 끌어올림은 이후 검토.
- ❌ θ·β·α 동적 조정 - v0.7은 μ만(최소 표면적, 안정성). 다른 파라미터 피드백은 이후.
- ❌ BGE-M3(FlowMeter 정밀도) - 별개 이후 트랙.

## 3. 데이터 모델 / 아키텍처 델타

| 구조 | 변경 |
|------|------|
| `MetaController`(신규 `src/meta.rs`) | `{ gain, threshold, floor }` + `cooling(flow) -> mu_scale`. 기본 약한 게인. env `SALON_META_GAIN` 튜닝 |
| `HawkesEngine::update_intensities` | `mu_scale: f64` 인자 추가(base_rate에 곱해 회복 목표를 낮춤). 1.0이면 기존과 동일 |
| `driver.rs` / `live.rs` | 매 틱 flow→cooling→mu_scale를 update_intensities에 전달. content 없으면 1.0 |
| `ObservationRecord` | `mu_scale`(또는 cooling) Option 표시용(content 없으면 None → 생략 → 골든 보존). 선택 |
| `chat.rs` | 사이드바에 cooling/mu_scale 한 줄(선택) |

## 4. Subtasks

| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| 36 | MetaController 코어 | `meta.rs`: cooling(flow,gain,threshold,floor)→mu_scale∈[floor,1]. 순수·결정적·유계. 단위 테스트(수렴↑→scale↓ 단조, floor 하한, None→1.0, threshold 이하→1.0) | 낮음(순수). 게인 감각은 라이브 | v0.6 |
| 37 | driver/live 배선 + 골든 보존 | update_intensities에 mu_scale. driver/live가 flow→cooling→적용. **content 없으면 1.0 → 골든 동일**. mu_scale 표시(선택) | **중~높음**(엔진 파라미터 피드백 첫 도입, 골든 보존 필수, 호출처 churn) | 36 |
| 38 | v0.7 게이트 + 관찰 | smoke_v7(골든 보존, no-op without flow, cooling 단조·유계) + 채팅 사이드바 cooling 표시. chat_demo로 라이브 관찰 | 낮음~중(진동은 라이브 관찰) | 37 |

Phase A(36 코어) → B(37 배선·골든, 최위험) → C(38 게이트·관찰). 완료: task-38 + 골든 보존.

## 5. v0.7 완료 기준

- 기본 `cargo run`(LLM off)·headless 골든 5종 바이트 동일(flow None → mu_scale 1.0 → 강도 불변).
- `--chat`/`--llm`에서 대화가 수렴하면 mu_scale↓ → 발화 줄고 침묵 늘어 방이 식음(미터/사이드바로 관찰).
- mu_scale는 floor 이상 유지(완전 침묵/고착 없음), 게인 약함(진동 없음 - 라이브 관찰).
- cooling 계산 결정적(같은 flow·gain → 같은 mu_scale).

## 6. 위험과 대응

| 위험 | 대응 |
|------|------|
| **피드백 진동**(말했다 조용했다 발작) | 약한 게인 기본, floor, 단조 cooling. SALON_META_GAIN으로 낮춤. 라이브 미터로 관찰(design §10) |
| **고착**(영원 침묵) | mu_scale floor(≥0.4)로 μ가 0으로 안 감. 침묵 길어지면 idle 회복(기존 동역학) 유지 |
| 골든 깨짐(엔진 파라미터 피드백) | content 게이팅: flow None → mu_scale 1.0 → 강도 불변. 골든 5종 재확인(필수). FakeBackend는 항상 no-op |
| update_intensities 시그니처 churn | mu_scale 인자 추가, 호출처(driver/live/hawkes 테스트) 1.0으로 갱신. 빌드 확인 |
| 비결정 유입 | cooling은 flow·gain만(결정적). 비결정은 LLM content뿐. mu_scale 1.0이면 완전 동일 |

## 7. 산출물

- 이 문서(PLAN v0.7). 구현 시 §4를 `salon-engine-v7-task-36..38.md`로 분해.
- v0.7 한 줄: 대화가 식어가면(수렴) 엔진이 스스로 μ를 낮춰 방을 더 식힌다 - 약하게, floor 안에서, 진동 안 나게. content 없으면 아무 일 없음(골든 보존). 로드맵의 마지막·가장 조심스러운 손잡이.
