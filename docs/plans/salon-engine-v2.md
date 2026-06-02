---
title: Salon 엔진 플랜 v0.2 - 케미(α)
type: plan
status: done
priority: P1
updated_at: 2026-06-02
owner: shared
summary: v0.2 - 교차 자극 α 도입(room preset + persona modifier) + FSM 전이. 같은 μ에서도 케미로 화자 전이 패턴이 갈리는지 검증. 안정 조건 spectral radius < 1.
design_ref: ../reference/salon-engine-design.md
roadmap_ref: salon-engine-v1.md
---

# Salon - 플랜 v0.2 (케미 α)

v0.1(`salon-engine-v1.md`)에서 μ·θ·k만으로 발화/침묵 리듬을 검증했다. v0.2는 **α(교차 자극)** 를 켜서 "누가 누구의 발화에 자극받는가" = 케미를 만든다. 설계 근거는 `../reference/salon-engine-design.md` §2(Hawkes α), §3-3(FSM), §11(튜닝 손잡이).

핵심 원칙: α를 N×N 손으로 안 채운다. **room preset**이 전역 성격을, **persona modifier**가 쌍별 비대칭을 만든다. α를 끄면(=0) v0.1과 똑같이 거동해야 한다(토글 = Contract 결합).

---

## 0. Context

v0.1 관측에서 두 가지가 v0.2를 부른다. (1) α=0이라 한 페르소나 발화가 다른 페르소나를 못 건드려, 화자 전이가 balance/θ로만 정해지고 "성격 궁합"이 없다. (2) α=0이라 대화 길이가 거의 결정적이어서 구 기준 4(길이 seed 분포)가 v0.2로 이관됐다(`salon-engine-v1.md` §3). α는 동시 후보 경쟁과 burst를 만들어 이 둘을 푼다.

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | 결정성 유지: 같은 seed → 바이트 동일 출력. α 경로도 주입 RNG·논리 시각만 사용 |
| INV-2 | 폭주 금지: 교차 자극 행렬 spectral radius < 1이면 λ가 발산하지 않는다(상한 유지) |
| INV-3 | 토글: α=0(미주입)이면 v0.1 스모크와 동일 거동. 모듈은 Contract로만 결합 |
| INV-4 | 출력 계약 유지: headless/TUI 모두 같은 `ObservationSink`/`ObservationRecord` |
| INV-5 | v0.1 27개 테스트 + 스모크 게이트는 계속 green |

## 2. Goals / Non-goals

### Goals
- (G1) λ_p(t) = μ_p + Σ_j α_pj·Σ_{t_k<t} κ(t−t_k) 형태로 교차 자극 활성화(κ = exp(−β·Δt)).
- (G2) room preset(calm/pub/argument/chaos)으로 μ/α/β/θ 전역값 세팅.
- (G3) persona modifier로 쌍별 α 비대칭(케미 collapse 방지).
- (G4) FSM 전이 제약(같은 페르소나 2연속 금지 등)을 RRF 위에 얹기.
- (G5) 안정 조건 spectral radius < 1 계산/검증, 의도적 발작 모드(>1) 토글.

### Non-goals
- ❌ LLM 생성(v0.3) / 내용 기반 RRF 신호(관심도·잔향, v0.3).
- ❌ 임베딩·FlowMeter(v0.5), MetaController(v0.6).
- ❌ persona randomizer / `/invite` UX(이후).

## 3. 데이터 모델 델타 (v0.1 대비)

| 구조 | 변경 |
|------|------|
| `HawkesEngine` | 교차 자극 항 추가. 이벤트 히스토리로 Σκ 누적, α 행렬 곱. α=0이면 v0.1 회복식과 동일 |
| `CouplingMatrix`(α) | v0.1의 구조-only에서 **활성화**. preset+modifier로 생성 |
| `RoomPreset` | 신규. preset → (μ scale, α scale, β, θ) |
| `PersonaModifier` | 신규. 역할/축 기반 쌍별 α 가중(persona-ui §6, persona-fragments 참조) |
| `FsmTransition` | 신규. 금지/가중 전이 행렬 |
| `ObservationRecord` | (선택) 교차 자극 경로 = 이번 틱 누구 발화가 누구를 자극했나(설계 §9) |

## 4. Subtasks (구현 시 `-task-NN.md`로 분해)

| task | 제목 | 핵심 | depends_on |
|---|---|---|---|
| 09 | α 활성 + 안정 조건 | HawkesEngine 교차 자극 항, `spectral_radius(α)<1` 유틸, α=0 토글 동등성 | v0.1 |
| 10 | Room preset | calm/pub/argument/chaos → 전역 μ/α/β/θ, CLI `--room` | 09 |
| 11 | Persona modifier | preset 균일 α에 쌍별 비대칭 복원(케미 collapse 방지) | 09 |
| 12 | FSM 전이 | 금지 전이 행렬로 RRF 후보 필터(빈 후보 시 폴백 명시) | 09 |
| 13 | 미터 + 스윕 | TUI에 교차 자극 경로 표시, 스윕에 preset 비교 | 10,11,12 |
| 14 | 스모크 v0.2 | 케미·안정·길이 분포·토글·FSM 검증(아래 §5) | 10,11,12 |

Phase A(09 기반) → B(10·11 케미, 병렬) → C(12 FSM) → D(13·14 관찰/검증).

### 각 task 메모
- **09**: 교차 자극은 이벤트 기반. 발화 이벤트가 히스토리에 쌓이고, 각 페르소나 λ는 자기에게 들어오는 α_pj·κ 합을 받는다. self-excitation α_pp는 "한번 말 시작하면 잠깐 더"인데 v0.1 self-decay 억제와 충돌하지 않게 분리. spectral radius는 거듭제곱법 또는 작은 N이라 직접.
- **10**: preset 표는 `salon-engine-v1.md` §3 v0.2 표(calm/pub/argument/chaos) 사용.
- **11**: 비대칭 예 "ENTP→INTJ 자극 강, 역은 약". 역할/축 가중은 persona-ui §6 + persona-fragments 참조.
- **12**: 설계 §3-3. 필터 후 후보가 비면 침묵 처리(silent fallback 명시, 숨기지 말 것).

## 5. v0.2 완료 기준 (검증)

- **케미**: 같은 μ·preset에서 persona modifier(쌍별 α)를 바꾸면 화자 전이 패턴(누가 누구 뒤에 자주 오나)이 달라진다.
- **안정**: spectral radius < 1에서 λ 상한 유지(발산 없음). 일부러 > 1로 하면 발산(발작 모드)이 재현된다.
- **길이/seed 분포**(v0.1에서 이관): α로 동시 후보·burst가 생겨 seed별 대화 길이/리듬이 분포를 이룬다(분산 > 0).
- **토글**: α=0이면 v0.1 스모크 결과와 동일.
- **FSM**: 금지 전이(같은 페르소나 2연속 등)가 출력에 나타나지 않는다.

각 기준은 v0.1처럼 headless 고정 seed + 라이브러리 API로 자동 검증(`cargo test --test smoke`).

## 6. 위험과 대응

| 위험 | 대응 |
|------|------|
| α 튜닝 지옥 | preset(전역) + modifier(쌍별)로 차원 축소. N×N 직접 입력 금지 |
| 케미 collapse(preset 단일값) | persona modifier로 쌍별 비대칭 복원 |
| Hawkes 폭주 | spectral radius < 1 검증/클램프(INV-2). 발작 모드는 명시적 옵션으로만 |
| FSM가 후보를 고갈 | 필터 후 빈 후보 = 침묵으로 명시 처리(silent fallback 숨기지 않기) |
| v0.1 회귀 | INV-3/INV-5: α=0 토글 동등성 + 기존 27 테스트 유지 |

## 7. 산출물

- 이 문서(PLAN v0.2). 구현 착수 시 §4 subtask를 `salon-engine-v2-task-09..14.md`로 분해(Codex 위임, MCP off - 메모리 [[reference-codex-delegation]]).
- v0.1 한 줄에 이어: α를 켜도 폭주 없이, preset+modifier만으로 "누구는 누구한테 발끈하고 누구는 무덤덤한" 케미가 화자 전이에서 보이면 v0.2 성공.
