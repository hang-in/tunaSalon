---
title: "Salon v0.5 Task 30: 채팅 TUI (chat pane + 게이지 사이드바 + 입력창)"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v5.md
task_id: "30"
depends_on: ["29"]
parallel_group: ""
---

# Task 30 - 채팅 TUI

plan `salon-engine-v5.md` subtask 30. persona-ui §5 레이아웃을 구현한다: **채팅 pane | 게이지 사이드바 + 하단 입력창**. LiveSession(task-29)을 구동하는 crossterm 이벤트 루프. 렌더 함수는 `TestBackend`로 단위 테스트, 루프는 수동 검증(터미널 필요). 기존 `tui.rs`(DebugMeter, ObservationRecord 구동)와 별개 - 채팅은 LiveSession 상태 구동.

## Changed files

- `src/chat.rs` - 신규. `render_chat`(순수, 테스트 가능) + `ChatApp`(raw-mode + 이벤트 루프, LiveSession 구동).
- `src/tui.rs` - 수정(최소). `lambda_bar`를 `pub(crate)`로 노출(게이지 막대 재사용). 그 외 불변.
- `src/lib.rs` - 수정. `pub mod chat;`.

## Change description

- `pub fn render_chat(frame, history: &[Event], intensities: &BTreeMap<PersonaId,f64>, names: &BTreeMap<PersonaId,String>, theta: f64, input: &str, pending: bool)` (LiveSession을 직접 안 받고 **평범한 데이터**로 받아 테스트 용이; 루프가 LiveSession에서 추출해 전달):
  - persona-ui §5: 세로 [상단(Min) | 입력창(Length 3)]; 상단 가로 [채팅 62% | 사이드바 38%].
  - 채팅 pane: `history`를 "이름: content"로(최근 N개). content=None placeholder는 "이름: (...)"/"(생각 중)". 사람 발화도 자기 이름으로 표시.
  - 사이드바: 페르소나별 name + `tui::lambda_bar(λ, θ)` + 값. (게이지가 일급 요소 - §5.)
  - 입력창: `> {input}`. pending이면 상태 표시(예 "(...생각 중)").
- `pub struct ChatApp`: LiveSession + Terminal(raw-mode) 보유. `new`(TuiSink처럼 enable_raw_mode + EnterAlternateScreen, 비-터미널이면 Err). `run(&mut self)` 루프:
  - wall-clock 페이스로 `session.tick()`(예 tick_period ~700ms, const/인자). 매 루프 `session.poll_generation()` drain(완료 발화 history 반영).
  - `event::poll(짧은 timeout ~50ms)` 논블로킹: `Enter`→`session.submit_human(buffer)` + buffer clear; `Char(c)`→buffer push; `Backspace`→pop; `q`/`Esc`(빈 입력시)→종료; `Tab`/`Ctrl-D`등으로 디버그 토글(선택).
  - 매 루프 `terminal.draw(|f| render_chat(...))`. 종료 시 raw-mode 복원(TuiSink restore 패턴, Drop에서도).
  - **블로킹 없음**: tick/poll/submit 즉시 반환(task-29 보장), event::poll 짧은 timeout → 생성 중에도 입력/렌더 반응.
- 가드레일: raw-mode/AlternateScreen 복원 견고(panic/조기반환에도, Drop). unwrap/panic 금지. 키 비노출. crossterm/ratatui 기존 의존만(신규 크레이트 없음).

## Dependencies

- task-29(LiveSession + 접근자 state/personas/combined_intensities/is_pending).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test` green. 신규 `chat::tests`(`TestBackend`로, 기존 tui 테스트 패턴):
  - (1) `render_chat`가 채팅 pane에 발화("이름: content")를 그린다(사람 + 페르소나).
  - (2) 사이드바에 페르소나 이름 + 게이지 막대가 나온다.
  - (3) 입력창에 `> {input}` 버퍼가 나온다.
  - (4) pending placeholder(content None)가 "생각 중" 류로 표시되고 panic 없음.
- **골든 5종 바이트 동일**(채팅은 라이브 전용, headless 불변). `lambda_bar` pub(crate) 변경이 기존 tui 테스트 회귀 없음.
- (수동) task-31에서 `--chat` 결선 후 실제 입력/반응 확인. 이 task는 빌드 + 렌더 테스트까지.

## Risks

| 위험 | 회피 |
|---|---|
| raw-mode 미복원으로 터미널 깨짐 | TuiSink restore 패턴 그대로(show_cursor + LeaveAlternateScreen + disable_raw_mode), Drop에서도 복원 |
| 루프가 생성/입력에서 블록 | tick/poll/submit 즉시 반환(task-29), event::poll 짧은 timeout. 메인 루프는 폴링 |
| 렌더 함수가 LiveSession 결합돼 테스트 곤란 | render_chat은 평범한 데이터 인자 → TestBackend로 직접 테스트(LiveSession 불요) |
| 입력 위젯 raw 키 처리 복잡 | Char/Backspace/Enter/Esc 최소셋부터. IME/멀티바이트는 char 단위 push(한글 입력은 task-31 수동 확인) |
| 골든/headless 오염 | chat은 별도 모듈, driver/sink 불변. lambda_bar만 pub(crate). 골든 재확인 |
