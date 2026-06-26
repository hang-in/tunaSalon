---
type: plan
status: implemented
updated_at: 2026-06-26
---

> 구현 완료(2026-06-26): `src/debate/phase.rs`(9 단위테스트) + LiveSession 배선 +
> web 종료 배너. 전체 테스트 통과(phase 9종 포함), 골든 headless main과 byte-identical(무드리프트).
> 미적용(후속): 프런트 단계 배지(State.phase 필드 + React). getter `current_phase()`는 준비됨.

# salon-debate-phases — 단계형 토론 (오프닝 → 클로징 → 종료)

## 0. 한 줄 목표

지금 토론방은 **영원히 안 끝난다**(web 틱 루프 무한 + MetaController는 `floor: 0.4`까지만
식힘 + Hawkes는 base_rate로 재충전). 이를 **단계형 토론**으로 만든다: 오프닝 → 입장개진 →
공방 → 클로징 → **종료(dispatch 중단, 방 idle)**. 단계 전환은 **발화 수(주) + 수렴 신호(보조)**.

## 1. 결정 사항 (확정)

- **전환 기준**: `(단계 발화수 ≥ 쿼터) OR (수렴 > 임계 AND 단계 발화수 ≥ 최소)`.
  - 수렴↑ = 같은 말 반복(할 말 떨어짐) → 조기 전환. 발산(의견 안 좁혀짐)은 정상 → 쿼터까지 진행.
- **종료 후**: 방을 idle(새 dispatch 중단). **사용자 발화 시 공방(Clash)으로 재진입**.
- **사람 참여**: 단계 흐름은 유지하고 `human_focus`만 얹는다(기존 메커니즘). 단 *종료된* 방이면
  공방으로 재진입시킨다.
- **길이**: 넉넉히. 반복은 기존 3중 방어(`repetition_guard` + twist 카드 + 수렴 조기전환)가 막는다.

## 2. 단계 모델 + 쿼터

`N` = persona 수(동적, 보통 3).

| 단계 (`DebatePhase`) | 하는 일 | 최소 | 쿼터(일반) | CasualBanter | Courtroom/PolicyDuel |
|---|---|---|---|---|---|
| `Opening` | 사회자처럼 쟁점 개막 + 첫 입장 유도 | 1 | 1 | 1 | 1 |
| `Positions` | 각자 입장 1번씩 분명히 | `N` | `N` | `N` | `N` |
| `Clash` | 닉네임 부르며 동의/반박(현 동작) | `N` | `3*N` | `2*N` | `4*N` |
| `Closing` | **새 논거 금지**, 각자 최종 입장 정리(정리자 우선) | 1 | 2 | 1 | 2 |
| `Concluded` | dispatch 중단(방 idle) | — | — | — | — |

쿼터 표는 모드별로 `Clash`/`Closing`만 다르다. 나머지는 동일.
수렴 조기전환 임계 = 기존 `CONVERGENCE_TWIST_THRESHOLD`(0.6) 재사용.

## 3. 새 모듈 — `src/debate/phase.rs` (순수·결정적)

`src/debate/`는 framework-independent(live/web/net import 0). 이 규율 유지. rng·IO·상태변이 없음
(advance만 내부 카운터 증가). 골든 무영향(아래 §6).

```rust
//! 단계형 토론 상태머신. 순수·결정적. live/web 비의존.

use super::plan::DebateMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebatePhase { Opening, Positions, Clash, Closing, Concluded }

/// 단계 진행을 추적하는 컨트롤러. debate_plan이 있을 때만 LiveSession이 구동한다.
#[derive(Debug, Clone)]
pub struct PhaseController {
    pub phase: DebatePhase,
    /// 현재 단계에서 누적된 (실)발화 수.
    utterances_in_phase: u32,
    mode: DebateMode,
    persona_count: u32,
}

impl PhaseController {
    pub fn new(mode: DebateMode, persona_count: u32) -> Self {
        Self { phase: DebatePhase::Opening, utterances_in_phase: 0,
               mode, persona_count: persona_count.max(1) }
    }

    pub fn is_concluded(&self) -> bool { self.phase == DebatePhase::Concluded }

    /// persona 수 변동(동적 초대/퇴장) 반영. 쿼터 재계산용.
    pub fn set_persona_count(&mut self, n: u32) { self.persona_count = n.max(1); }

    /// 한 발화가 디스패치된 뒤 호출. flow 수렴도(없으면 None)를 받아 단계 전환 판정.
    /// Concluded 도달 순간 true 반환(호출자가 "토론 종료" 1회 알림용).
    pub fn on_utterance(&mut self, convergence: Option<f64>) -> bool {
        if self.phase == DebatePhase::Concluded { return false; }
        self.utterances_in_phase += 1;
        let (min, quota) = self.bounds();
        let conv_high = convergence.is_some_and(|c| c > 0.6); // CONVERGENCE_TWIST_THRESHOLD
        let advance = self.utterances_in_phase >= quota
            || (conv_high && self.utterances_in_phase >= min);
        if advance {
            self.phase = self.next_phase();
            self.utterances_in_phase = 0;
            return self.phase == DebatePhase::Concluded;
        }
        false
    }

    fn next_phase(&self) -> DebatePhase {
        match self.phase {
            DebatePhase::Opening => DebatePhase::Positions,
            DebatePhase::Positions => DebatePhase::Clash,
            DebatePhase::Clash => DebatePhase::Closing,
            DebatePhase::Closing => DebatePhase::Concluded,
            DebatePhase::Concluded => DebatePhase::Concluded,
        }
    }

    /// (최소, 쿼터). persona 수·모드 반영.
    fn bounds(&self) -> (u32, u32) {
        let n = self.persona_count;
        match self.phase {
            DebatePhase::Opening => (1, 1),
            DebatePhase::Positions => (n, n),
            DebatePhase::Clash => {
                let rounds = match self.mode {
                    DebateMode::CasualBanter => 2,
                    DebateMode::Courtroom | DebateMode::PolicyDuel => 4,
                    _ => 3,
                };
                (n, rounds * n)
            }
            DebatePhase::Closing => {
                let q = if self.mode == DebateMode::CasualBanter { 1 } else { 2 };
                (1, q)
            }
            DebatePhase::Concluded => (0, 0),
        }
    }

    /// 종료된 방에 사람이 발화 → 공방으로 재진입.
    pub fn reopen_to_clash(&mut self) {
        self.phase = DebatePhase::Clash;
        self.utterances_in_phase = 0;
    }

    /// 생성 워커에 주입할 단계 지시 한 줄. plan과 합쳐 segs에 push.
    pub fn directive(&self) -> &'static str {
        match self.phase {
            DebatePhase::Opening =>
                "[단계: 오프닝] 사회자처럼 쟁점을 한 문장으로 열고, 당신의 첫 입장을 짧게 선언하세요. 아직 반박은 하지 마세요.",
            DebatePhase::Positions =>
                "[단계: 입장개진] 다른 사람을 반박하기 전에, 먼저 당신의 입장과 핵심 근거 하나를 분명히 세우세요.",
            DebatePhase::Clash =>
                "[단계: 공방] 최근 발언자 닉네임을 직접 부르며 동의/반박/보완 중 하나를 분명히 하고, 새 근거나 사례를 보태세요.",
            DebatePhase::Closing =>
                "[단계: 클로징] 새 논거를 꺼내지 말고, 지금까지의 논의를 받아 당신의 최종 입장을 한두 문장으로 정리하세요. 합의가 안 됐다면 무엇이 끝까지 갈렸는지 짚으세요.",
            DebatePhase::Concluded => "",
        }
    }
}
```

테스트(같은 파일 `#[cfg(test)]`):
1. `Opening`은 1발화로 `Positions`로 간다.
2. `Positions`는 N발화로 `Clash`(N=3).
3. `Clash`에서 수렴 high면 최소(N) 이후 조기 `Closing`; 수렴 low면 쿼터(3N)까지 유지.
4. `Closing` 쿼터 채우면 `Concluded` + `on_utterance` true 1회.
5. `Concluded`에서 `on_utterance`는 항상 false(추가 증가 없음).
6. `reopen_to_clash` 후 다시 진행 가능.
7. 모드별 `bounds` 손계산(CasualBanter Clash=2N, Courtroom Clash=4N).
8. `set_persona_count`가 다음 `bounds`에 반영.

## 4. LiveSession 배선 (`src/live.rs`)

- **필드 추가**(`:127` `debate_plan` 옆):
  `phase: Option<PhaseController>` — `debate_plan`이 Some일 때만 Some로 초기화
  (`PhaseController::new(plan.mode, personas.len() as u32)`). plan 없으면 None → 단계 비활성.
- **`with_debate_plan`/plan을 세팅하는 빌더**에서 `phase`도 함께 채운다. (현재 plan은
  topics→`infer_debate_plan`으로 설정되는 지점과 동일하게.)
- **add/remove_persona**: `phase`가 Some면 `set_persona_count(personas.len())`.

### 4.1 tick() — 종료 시 dispatch 차단 (`:440~`)
`tick()` 진입 직후, `self.phase`가 `Some(pc)`이고 `pc.is_concluded()`면:
- 화자 선택·dispatch를 건너뛰고 `TickOutcome::Silent` 반환(방 idle, 토큰 0).
- 단 Hawkes 강도 갱신/감쇠는 돌려도 무방하나, 단순화를 위해 **즉시 Silent return** 권장
  (어차피 종료 상태). `tick_count`는 증가시킨다(기존 라인 유지).

### 4.2 dispatch — 단계 지시 주입 (`:647~662`)
`segs` 조립에서 `plan.directive_line(tick)` push **직후**:
```rust
if let Some(pc) = self.phase.as_ref() {
    let d = pc.directive();
    if !d.is_empty() { segs.push(d.to_string()); }
}
```
순서: `[토론 프레임]` `[단계 지시]` `[진행 지시]` `[새 국면]` `[형식]`.

### 4.3 클로징 화자 = 정리자 우선 (`:500~517` 화자 선택)
`phase == Closing`이면 `summary_persona_id`를 우선 화자로(이미 SUMMARY_CADENCE 경로 존재 —
Closing 단계에서는 cadence 무시하고 정리자 우선). 정리자가 직전 화자면 RRF 폴백(기존 패턴 동일).

### 4.4 단계 전진 — 디스패치 카운트 (`dispatch` 끝, `TickOutcome::Dispatched` 직전)
실제 생성 job을 보낸 뒤(`:691` 이후):
```rust
if let Some(pc) = self.phase.as_mut() {
    let concluded_now = pc.on_utterance(flow_now.map(|m| m.convergence));
    if concluded_now { self.just_concluded = true; } // web가 1회 알림
}
```
`flow_now`는 dispatch 상단에서 이미 계산됨(`:633` 인근). content 없으면 None → 카운트만.
`just_concluded: bool` 필드 추가(또는 `TickOutcome`에 `Concluded(PersonaId)` 변형 추가 —
**권장: TickOutcome 변형**이 web 분기에 깔끔. 아래 §5 참고).

### 4.5 사람 발화 재진입 (`submit_human`)
`submit_human` 처리에서 `phase`가 Some이고 `is_concluded()`면 `reopen_to_clash()` 호출.
그 외에는 기존 `human_focus` 로직 그대로(단계 유지).

### 4.6 복원 (`restore_history`)
저장 로그로 복원 시(`:958`), `phase`가 Some면 `Clash`로 시작(카운터 0)하도록 세팅 —
복원된 방도 다시 클로징까지 도달 가능. (Opening/Positions를 재실행하지 않기 위함.)

## 5. web 배선 (`src/web.rs`)

- 틱 루프(`:451`, `:729` `session.tick()`)에서 반환이 **종료 신호**면(권장:
  `TickOutcome::Concluded(speaker)` 또는 `just_concluded` 플래그를 poll로 노출) **1회**
  "토론이 마무리됐습니다" 류의 system frame을 프런트로 전송. 이후 tick은 Silent(idle).
- 클라이언트 발화(`submit_human`) 들어오면 LiveSession이 알아서 `reopen_to_clash` → 다시
  진행되므로 web은 추가 처리 불필요.
- (선택) 프런트에 현재 단계 배지 표시: `LiveSession::current_phase()` getter 추가해
  사이드바/헤더에 "오프닝/공방/클로징/종료" 노출. v1에서는 backend getter만 만들고 프런트는
  후속.

## 6. 결정성·골든 불변식 (필수)

- `phase`는 **`debate_plan`이 Some일 때만** 활성. driver/headless 경로(`driver.rs`)는 plan을
  쓰지 않으므로 **전혀 영향 없음**. 골든 5종은 driver 경로 → 무손상.
- live에서도 plan이 None이면(예: chat_demo 일부 경로) phase None → 기존 동작 보존.
- 단계 지시는 `history_snapshot`(생성 워커 전용)에만 들어간다 — `state.history`/flow/recall
  불변(기존 INV-2와 동일).
- `phase.rs`는 rng 무소비. dispatch 카운트는 화자선택 *이후*라 RRF 시퀀스 불변.
- 검증: `cargo build -q` 후 `--headless --seed 42 --ticks 200` 2회 byte-identical 확인.

## 7. 작업 순서 (Sonnet 위임 단위)

1. **`src/debate/phase.rs`** 신규 + `mod.rs` 재노출 + 단위테스트 8종. (순수, 위험 0)
2. **LiveSession 배선**: 필드 + 빌더 + tick 차단 + dispatch 주입/카운트 + 클로징 화자 +
   submit 재진입 + restore. `cargo test --features "web redis-bus"` 통과.
3. **web 종료 알림** + (선택) phase getter.
4. **검증**: 골든 byte-identical + 전체 테스트 + `--web`으로 라이브 1회(짧은 쿼터로 종료 관찰).

## 8. 함정 (핸드오프에서)

- `--web`은 반드시 `--features "web redis-bus"`로 빌드(default 빌드는 web 없는 바이너리 → 즉시 종료).
- salon.exe 실행 중이면 링크 막힘 → 빌드/test 전 8080 프로세스 정지.
- 골든 검증은 `cargo build` 후 명시적 순차 실행(for-loop 안 `cargo run` 금지).
