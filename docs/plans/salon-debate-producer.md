---
title: Debate Producer Layer + God-File Boundary Refactor
type: plan
status: draft
priority: P1
updated_at: 2026-06-26
owner: claude-code
summary: >
  토론을 "정중한 정책 메모 교환"에서 "재미있는 토론"으로 끌어올리는 debate-producer
  레이어(DebatePlan/숨은목표/format 변주/evidence card/ledger)를 설계한다. 핵심 제약:
  이 레이어를 live.rs/web.rs/main.rs(이미 갓파일)에 욱여넣지 말고, framework-independent
  순수 모듈 `src/debate/`로 처음부터 분리한다. dsp_cad_gcs 리팩토링 규율 차용.
---

# Debate Producer Layer + God-File Boundary Refactor

## 결론 (먼저)

1. **다음 제품 가치 = debate-producer 레이어**다. 현재 시스템은 LLM에게 "좋은 토론
   답변을 해라"라고만 시켜서 유능한 에세이가 나오지만 재미있는 토론은 아니다. 빠진 것은
   *토론 연출자*: 주제를 어떤 종류의 토론으로 만들지 결정하고, 참가자에게 비대칭 역할과
   숨은 목표를 주고, 발화 형식을 변주하고, 고착되면 새 증거/제약/반전을 투입하는 층.

2. **그러나 이 레이어를 `live.rs`/`web.rs`/`main.rs`에 추가하면 안 된다.** 이 셋은 이미
   갓파일이다. 핸드오프의 "Step 1~5"는 전부 이 파일들에 코드를 더 붓는 방향이라, 그대로
   따르면 사용자가 우려한 갓파일 문제가 악화된다.

3. **권고**: debate-producer를 **framework-independent 순수 모듈 `src/debate/`로 먼저
   분리**하고, `LiveSession`은 그 결과(지시 텍스트)를 *소비*만 한다. 이렇게 하면 (a) 새
   기능이 갓파일을 키우지 않고, (b) producer 로직이 순수 함수라 단위테스트 가능하고,
   (c) 골든 불변식(LLM-off 바이트 동일)을 content 게이팅으로 그대로 보존한다.

---

## 1. 현재 사실 (검증됨)

### 1.1 갓파일 실측 (`wc -l src/*.rs`)

| 파일 | 줄 수 | 비고 |
|---|---:|---|
| `live.rs` | 2236 | 실코드 ~1150 + 테스트 ~1080 |
| `memory.rs` | 1818 | friend engine(BM25/벡터/회상) |
| `web.rs` | 1807 | axum/WS/룸런타임/pause/redis gateway |
| `pool.rs` | 1222 | 백엔드 풀/세마포어/라우팅 |
| `persona_kit.rs` | 1129 | 페르소나 합성/축/케미 |
| `main.rs` | 1043 | CLI/데모 페르소나/라우팅/복원 |

dsp_cad_gcs 규율 기준(아래 §4): **파일당 목표 ~400줄, 3개 이상 별개 책임이면 갓파일.**
위 6개 전부 위반. 단 이 계획은 *전부 지금 쪼개자*가 아니라 **"건드리는 김에 그 부분부터"**
(빅뱅 금지) 원칙을 따른다.

### 1.2 코덱스가 이미 구현한 것 (코드 확인)

- Redis 멀티세션 버스(`session_bus.rs`, `redis-bus` feature) — `LiveSession`은 Redis-free 유지(불변식 지킴).
- 룸 영속/로비/삭제/리셋, `room_id` 경계.
- 토론 품질 1차 패치(`live.rs`): `repetition_guard`, `length_hint`, `human_focus`(직접호출 강제발화),
  주기 summarizer 강제, cross-room 회상 필터, `나님→사용자님` 정화, pause 시 in-flight 무효화.
- `cargo check --features "web redis-bus"` = exit 0(컴파일 클린).

### 1.3 아직 없는 것 (grep 확인 — 핸드오프 문서에만 존재)

`DebatePlan` / `DebateMode` / `format_hint` / `private_goal`(숨은목표) / `EvidenceCard` /
`DebateLedger`. = 이번 설계의 대상.

### 1.4 이미 모여 있는 producer 성격 순수 로직 (`live.rs` L141~286, ~145줄)

`repetition_guard` · `build_directive` · `length_hint` · `significant_topic_tokens` ·
`cross_room_memory_is_topic_relevant` · `sanitize/normalize/mentioned/summary_persona`.
**전부 순수 함수(rng·IO·상태 없음).** `length_hint`는 주석에 "rng를 소비하지 않는다(골든
무영향)"라고 명시. → 이 묶음이 `src/debate/`의 씨앗이다.

---

## 2. 설계: debate-producer를 어디에 둘 것인가

### 2.1 모듈 경계 (핵심 결정)

```
src/debate/
  mod.rs          // 공개 타입 재노출 + DebateContext(입력) → Directive(출력)
  plan.rs         // DebatePlan, DebateMode, infer_debate_plan(topics)  [결정적, LLM 미사용]
  role.rs         // PersonaDebateRole(public_stance/private_goal/...)
  format.rs       // UtteranceFormat + format_hint(tick, speaker, plan)  ← length_hint 대체
  directive.rs    // build_directive(...) 이관 + producer 개입 조립
  ledger.rs       // DebateLedger(open_questions/claims/...) [후순위 stage]
  evidence.rs     // EvidenceCard, 투입 트리거 [후순위 stage]
```

원칙(=dsp_cad_gcs "framework-independent core"):
- `src/debate/`는 `live`/`web`/`pool`/네트워크/tokio를 import하지 않는다. 입력은 plain
  데이터(topics, history 슬라이스, tick, speaker, 신호값), 출력은 `Option<String>` 지시
  텍스트와 화자선택 힌트뿐.
- `LiveSession`은 `debate::build_directive(&ctx)`를 **호출만** 한다. 현재 `live.rs`가 직접
  들고 있는 producer 로직은 `src/debate/`로 옮기고, `live.rs`는 엔진(Hawkes/gate/RRF/
  recall/pending/worker)에만 집중한다.

### 2.2 데이터 흐름

```
주제(topics) ──infer_debate_plan──▶ DebatePlan ──(rooms.db에 영속)
                                        │
LiveSession.tick:                       ▼
  history/tick/speaker/신호 ─▶ DebateContext ─▶ debate::build_directive ─▶ "[진행 지시]…"
                                                                              │
                                                       history_snapshot에만 주입(state 불변)
                                                                              ▼
                                                                  pool.generate_one 프롬프트
```

- **DebatePlan 영속**: `rooms.db`에 `debate_plan_json` 컬럼 추가(누락 시 안전 마이그레이션,
  기존 룸은 topics로 재추론). roomstore.rs 책임.
- **주입 지점**: 기존 `build_directive` 호출부 1곳(`live.rs` L745 부근). content 게이팅
  유지 → driver/PersonaRuntime(골든 경로)은 plan=None → 바이트 동일.

### 2.3 골든 불변식 보존 (= dsp_cad_gcs의 headlessParity에 대응)

- producer 로직은 `history_snapshot`(복제본)에만 주입, `state.history` 불변(INV-2).
- `format_hint`는 `length_hint`처럼 tick+speaker 결정적, **rng 미소비**. 화자선택/골든 무영향.
- DebatePlan 추론은 라이브 경로에서만. LLM-off 골든 5종은 plan을 받지 않음.
- 검증: 골든 5/5 + `cargo test`(default/friend-engine/semantic) 카운트 ≥ 베이스라인.

---

## 3. 단계 계획 (각 단계 = 1 책임, 독립 검증/커밋)

핸드오프의 Step 1~6을 **갓파일 분리와 결합**해 재배열한다.

### Stage A — `src/debate/` 모듈 신설 + 기존 순수 로직 이관 (행위 동일) ✅ DONE(9cfc248)
- `live.rs` L141~286의 producer 순수 함수들을 `src/debate/`로 **이동만**(시그니처 동일,
  `live.rs`는 `use crate::debate::…`). 신규 동작 0.
- 단위테스트를 새 위치에 동반(TDD: 이동 전 카운트 확인 → 이동 → green).
- **이점**: 이후 모든 producer 기능이 `live.rs`가 아니라 `src/debate/`에서 자란다.
- 검증: `cargo test` 카운트 동일 + 골든 5/5.

### Stage B — DebatePlan 타입 + 결정적 추론 (핸드오프 Step 1) ✅ DONE(43be2d5)
- `plan.rs`: `infer_debate_plan(topics) -> DebatePlan`(키워드/카테고리 매칭, LLM 미사용).
- 모드: PolicyDuel / MoralDilemma / Courtroom / Forecasting / DesignReview / PersonalStakes.
- 테스트: "AI 판사…"→Courtroom/MoralDilemma, "AI 규제와 오픈소스"→PolicyDuel,
  "연애 앱…"→PersonalStakes.

### Stage C — DebatePlan 영속 (핸드오프 Step 2, roomstore.rs)
- `rooms.db`에 plan 저장/복원, 누락 컬럼 안전 마이그레이션, 기존 룸은 topics로 추론.

### Stage D — 지시 주입 + format 변주 (핸드오프 Step 3+4) ✅ DONE(Stage D.1)
- `directive.rs`가 plan(mode/stakes/공개입장/숨은목표 1개/format 1개)을 간결히 조립.
- `format_hint`로 `length_hint` 대체(summarizer는 다른 가중치).
- 프롬프트 비대화 주의(길면 모델 드리프트) — plan 텍스트는 압축.

### Stage E — 루프 차단 producer (핸드오프 Step 5)
- 신호: `flow()`(과수렴) + `repetition_guard` + `turns_since_summary` + 동일화자 반복.
- 개입: evidence card 1장 투입 / 미응답 질문 강제 / summarizer 도전. 전부 hidden producer
  텍스트(처음엔 가시 system 메시지 아님).

### Stage F — 프런트 소폭 UX (핸드오프 Step 6, 선택)
- 로비 카드에 debate mode/요약, 룸 상태(live/paused/generating). 숨은목표는 비노출.

> Stage A는 **순수 리팩토링(행위 동일)**, B~E는 기능 추가. A를 먼저 해야 B~E가 갓파일을
> 키우지 않는다.

---

## 4. 차용한 리팩토링 규율 (dsp_cad_gcs)

출처: `dsp_cad_gcs/docs/plans/{refactorBoundariesPlan,refactoringFollowup,
monorepoP1EngineExtraction}.md` + `CLAUDE.md §8`.

- **파일 크기**: 목표 ~400줄. **3+ 책임 = 갓파일** → 추출. 단일책임 <200줄이면 형제 모듈로.
- **framework-independent core**: 도메인 로직(여기선 debate-producer)은 transport/UI/네트워크
  import 0. 의존 방향 단방향(`live → debate`, 역방향 금지). → tunaSalon CLAUDE.md의
  "엔진 코어는 출력 sink와 분리"와 동일 철학.
- **추출 절차(TDD)**: 순수함수 추출 → 테스트 → 호출부 재배선. 빌딩 전 테스트 카운트 baseline
  확인(거짓green 방지).
- **검증 분리**: verify(테스트+골든) → 출력 확인 → **그 다음** commit → push. 한 배치 금지.
  파이프로 exit 판정 금지(이 레포 골든 회귀에서 반복 겪은 함정과 동일).
- **커밋 스코프**: 1 책임 = 1 커밋. 폴더 이동 + import 재배선 = 1 커밋.
- **안티패턴**: 빅뱅 리팩토링, 투기적 추상화, silent fallback. (tunaSalon 컨벤션과 일치.)
- **위임**: 순수함수 추출/상수 이동/import 갱신 = Sonnet 서브에이전트 위임 안전. 상태소유
  결정/순환참조 분석 = Architect(Opus) 직접.

---

## 5. 리스크 / 미해결

- `live.rs`는 워커 스레드 + rng 순서 + 골든에 민감. Stage A 이동 시 **시그니처/호출순서를
  바꾸지 말 것**(순수 이동만). 보류된 `decide_one_tick` 추출(driver/live 통합)은 이번에
  하지 않는다(중간 리스크, 별도 세션).
- DebatePlan을 LLM으로 추론할지(메타콜)는 v1에서 **안 함**(결정적). 후속 검토.
- friend 백엔드 불안정 → cloud-only 폴백 유지. plan 추론은 LLM 불요라 영향 없음.
- 기존 룸의 낡은 history/페르소나 → plan 평가 시 새 룸 생성 권장.

## 6. 다음 액션 (사용자 확인 후)

Stage A(순수 모듈 분리, 행위 동일)부터 착수 → 골든/테스트로 무손상 확인 → Stage B(DebatePlan).
이 doc은 SSOT, 진행 시 status/updated_at 갱신.
