---
title: Salon 엔진 플랜 v0.5 - 사람 참여 채팅방 (HumanChannel + 채팅 TUI)
type: plan
status: done
priority: P0
updated_at: 2026-06-02
owner: shared
summary: v0.5 - 제품(채팅방) 되찾기. v0.1~v0.4가 완성한 대화 흐름 엔진 위에 사람을 1급 참여자로 앉힌다. HumanChannel(design §5) + 인터랙티브 라이브 드라이버(비블로킹 생성) + 채팅 TUI(persona-ui §5: 채팅 pane + 게이지 사이드바 + 입력창). headless/결정성은 dev 회귀 도구로 강등(유지). 로컬 ollama 금지·cloud 강제 계승. FlowMeter는 v0.6으로 이동.
design_ref: ../reference/salon-engine-design.md
roadmap_ref: salon-engine-v1.md
---

# Salon - 플랜 v0.5 (사람 참여 채팅방)

## 0. Context - 재정렬

원래 제품은 **사용자가 LLM 페르소나들과 스몰토크하는 채팅방**이다(README 첫 줄, persona-ui §5의 채팅 pane + 입력창 레이아웃). 그 뒤의 대화 흐름 엔진(Hawkes 타이밍/침묵/누가-언제)은 이 채팅을 *살아있게* 만드는 비밀 소스다(design §12).

그런데 v0.1~v0.4는 엔진 + headless 결정성 + 스모크 + 동시성/벤치만 쌓았고 **제품(채팅 화면·입력·사람 참여)은 한 번도 안 만들었다.** 스모크테스트 편의로 넣은 headless가 앱의 얼굴이 되어 실험 하네스로 드리프트했다. design §5/§7의 `HumanChannel`은 **코어**인데 미구현이다.

v0.5는 이 드리프트를 되돌린다: **사람을 방에 앉힌다.** 엔진은 버리지 않고 채팅방의 생동감으로 재배치하고, headless/스모크는 엔진 회귀 검증용 dev 도구로 강등(유지)한다. SSOT: design §5(사람 참여)·§8(인터럽트), persona-ui §5(TUI 레이아웃).

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | headless 배치 경로(`driver::run` + FakeBackend)는 v0.1~v0.4 골든과 바이트 동일. 라이브 채팅 모드는 **별도 드라이버**, 결정 경로 불침투 |
| INV-2 | 엔진 결정 코어(Hawkes/gate/RRF/pool) 재사용, 변경 최소. 사람 발화도 Hawkes **외부 이벤트**로 일관 처리(design §5) |
| INV-3 | 라이브 채팅은 본질적 비결정(실시간 입력 + LLM) → **opt-in 모드**(`--chat`). 기본 `cargo run`(현 DebugMeter)·headless 불변 |
| INV-4 | LLM 생성(~1.6s 측정)이 입력/렌더를 **블록하지 않는다**. 생성은 워커 스레드 + 채널 전달(v0.4 풀 활용), 사람 입력은 즉시 인터럽트 |
| INV-5 | 보안(키 비노출)·로컬 ollama 금지(가드)·cloud 강제 계승 |
| INV-6 | v0.4의 125 tests + 스모크 게이트 4종 유지 |

## 2. Goals / Non-goals

### Goals
- (G1) **HumanChannel**(design §5): 사람 발화 = 큰 mark Hawkes 이벤트 → history push + 전 페르소나 λ 강자극(관심 집중) + 강도 일부 리셋 + 화제 기준점 갱신. 어느 틱에든 인터럽트(§8).
- (G2) **인터랙티브 라이브 드라이버**: crossterm 이벤트 루프 + 엔진 틱 + **비블로킹 LLM 생성**(워커 스레드 + mpsc 채널, 풀의 동시성 활용) + 사람 입력 인터럽트. 생성 중에도 UI 반응.
- (G3) **채팅 TUI**(persona-ui §5): 현 `tui.rs` 확장 - 채팅 pane(페르소나 발화 흐름) + 게이지 사이드바(λ 막대 + θ, 기존 렌더 재사용) + **하단 입력창**. `d`로 디버그 상세 토글. 사람 입력 시 막대 리셋 시각화.
- (G4) **데모 룸 + `--chat`**: v0.4 백엔드 풀(cloud `gemma4:31b-cloud` + friend `qwen3.6-35b-fast`) 위에 고정 데모 페르소나 + 사람이 한 방. end-to-end 라이브.

### Non-goals
- ❌ `/invite`·`/persona` 인터랙티브 페르소나 생성(persona-ui §3) - v0.5.x/이후. v0.5는 **고정 데모 룸**으로 시작.
- ❌ 캐릭터 스프라이트 비주얼(persona-ui §4) - 이후(게이지 막대 유지).
- ❌ FlowMeter 수렴/발산(기존 v0.5) - **v0.6으로 이동**. 단 화제 선점과 맞물리니 채팅방 다음에 자연스럽다.
- ❌ 사람 입력의 결정적 재현 - 라이브는 비결정 opt-in. 결정 검증은 headless가 담당.

## 3. 데이터 모델 / 아키텍처 델타

| 구조 | 변경 |
|------|------|
| `Event` | 사람 발화도 Event(speaker = 사용자명 예 "you", 큰 mark, content = 입력 텍스트). 기존 필드 재사용 |
| `HumanChannel`(신규) | 사람 입력 텍스트 → 엔진 이벤트 변환: 큰 mark Event + 전 페르소나 λ 강자극(Hawkes mutual excitation 큰 mark) + 강도 일부 리셋 + 화제 기준점 |
| 라이브 드라이버(신규, 예 `src/live.rs`) | batch `driver::run`과 별개. crossterm 이벤트 폴링 + 엔진 틱 + 생성 워커(mpsc) + HumanChannel 주입. 비블로킹 |
| `tui.rs`(확장) | 채팅 pane + 입력 위젯 추가. 기존 게이지 사이드바 렌더 재사용. 라이브 드라이버 상태로 구동(기존 ObservationSink 배치 렌더와 공존) |
| `main.rs` | `--chat` opt-in 모드: 라이브 드라이버 + 채팅 TUI + 데모 룸 풀 |

## 4. Subtasks (위험 분리: 엔진 → 라이브 루프 → TUI → 통합)

| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| 28 | HumanChannel (엔진) | 사람 발화 = 큰 mark Hawkes 이벤트(history + 전 λ 강자극 + 강도 리셋 + 화제 기준점). 순수 엔진 로직, **결정적 단위 테스트**(사람 이벤트 후 λ/history 단언). headless 골든 불변 | 낮음~중(엔진 코어 접촉, 결정성 보존) | v0.4 |
| 29 | 인터랙티브 라이브 드라이버 | crossterm 이벤트 루프 + 엔진 틱 + **비블로킹 생성**(워커 스레드 + mpsc, 풀 활용) + 사람 입력 인터럽트. headless 배치 드라이버와 별개 | **높음**(동시성·UI·블로킹 회피가 핵심 난점) | 28 |
| 30 | 채팅 TUI 레이아웃 | `tui.rs` 확장: 채팅 pane + 게이지 사이드바(재사용) + 하단 입력창. `d` 디버그 토글. 사람 입력 막대 리셋 시각화 | 중(ratatui 레이아웃·raw mode 입력) | 29 |
| 31 | 데모 룸 통합 + `--chat` | 풀(cloud+friend) + 데모 페르소나 + 라이브 드라이버 + 채팅 TUI 결선. 라이브 end-to-end | 중(결선·라이브 검증) | 29, 30 |
| 32 | v0.5 게이트 | HumanChannel 결정적 테스트 + headless 골든 불변 재확인 + 라이브 모드가 headless/기본 경로 안 깨뜨림. 라이브는 `#[ignore]`/수동 | 낮음 | 28, 31 |

Phase A(28 엔진) → B(29 라이브 루프, 최난점) → C(30 TUI) → D(31 통합, 32 게이트). 완료 게이트: task-32 + 골든 보존.

## 5. v0.5 완료 기준

- 기본 `cargo run`(현 DebugMeter)·headless 골든 5종 바이트 동일(라이브는 별도 경로).
- `--chat`로 채팅방이 뜨고: 페르소나들이 cloud/friend로 스몰토크 → **사람이 입력창에 치면** 화제가 사람 쪽으로 쏠리고(λ 자극·막대 리셋 시각화) → 사람이 빠지면 페르소나끼리 다시 burst.
- **생성 중(~1.6s)에도 입력/렌더 반응성 유지**(UI 안 멈춤).
- HumanChannel 큰-mark 이벤트가 전 페르소나 λ를 의도대로 자극(결정적 단위 테스트로 검증).

## 6. 위험과 대응

| 위험 | 대응 |
|------|------|
| 라이브 루프가 생성에서 블록 → UI 멈춤 | 생성은 워커 스레드 + mpsc 채널, 메인 루프는 입력/렌더/틱만. 사람 입력 항상 우선 처리 |
| ~1.6s 지연 UX | 비블로킹 + "(...생각 중)" 표시 + 사람 입력 즉시 반영(생성 대기와 무관) |
| 결정성/골든 오염 | 라이브는 **별도 드라이버**(`src/live.rs`), headless `driver::run` 불변. 골든 5종 재확인. HumanChannel 엔진 로직은 결정적 단위 테스트 |
| crossterm raw mode·입력 파싱 복잡 | 점진 구현(task-30: 입력 위젯·렌더부터, 그 다음 결선). 기존 tui.rs 게이지 렌더 재사용 |
| 사람 이벤트가 Hawkes 안정성 깸 | 큰 mark는 일시적(κ 감쇠로 회복), spectral radius 조건 유지. 미터로 관찰 |
| 키 비노출·로컬 금지 | v0.4 계승. 라이브도 cloud/friend만 |

## 7. 산출물

- 이 문서(PLAN v0.5). 구현 시 §4를 `salon-engine-v5-task-28..32.md`로 분해.
- v0.5 한 줄: 사람이 입력창에 말을 치면 LLM 페르소나들이 (자기들 리듬으로) 살아서 반응하는 **실제 채팅방**이 뜨면 v0.5 성공. 엔진은 그 방을 살아있게 만드는 비밀 소스로 자리한다. headless는 그 엔진을 검증하는 dev 도구로 남는다.
