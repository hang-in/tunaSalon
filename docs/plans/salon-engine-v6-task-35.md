---
title: "Salon v0.6 Task 35: 수렴/발산 게이지 (채팅 TUI) + v0.6 게이트"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v6.md
task_id: "35"
depends_on: ["34"]
parallel_group: ""
---

# Task 35 - 수렴/발산 게이지 + v0.6 스모크 게이트

plan `salon-engine-v6.md` subtask 35. FlowMeter 지표를 **눈에 보이게** 한다 - 채팅 TUI 사이드바에 수렴/발산 게이지(persona-ui §5 "수렴 ▓░ 발산 ▓▓"). + chat_demo 전사에 flow 출력(라이브 확인) + v0.6 스모크 게이트(smoke_v6). v0.6 완료.

## Changed files

- `src/chat.rs` - 수정. `render_chat`에 `flow: Option<FlowMetric>` 인자 추가 → 사이드바에 수렴 게이지 1줄. ChatApp.run()이 `session.flow()` 전달. (호출처 + 렌더 테스트 갱신.)
- `examples/chat_demo.rs` - 수정. 매 발화 후 `session.flow()`를 전사에 출력(예 `[흐름] 수렴 0.23`) → 라이브로 지표가 움직이는 게 보임.
- `tests/smoke_v6.rs` - 신규. v0.6 게이트.
- (필요 시) `src/chat.rs`/`tests/smoke_v5.rs`의 `render_chat` 호출/테스트에 flow 인자 추가.

## Change description

- `render_chat(..., flow: Option<FlowMetric>)`: 사이드바 게이지 아래에 흐름 1줄.
  - convergence ∈ [0,1] → 게이지 막대(채움=convergence) + 값. 예 `"흐름 수렴 {bar} {conv:.2}"`. None이면 `"흐름 -"`(아직 측정 불가). persona-ui §5 취지: 수렴 높으면 식는 중, 낮으면 살아있음.
  - lambda_bar류 막대 재사용 또는 간단 막대.
- ChatApp.run(): `render_chat(..., self.session.flow())`. 매 루프 갱신(content 쌓이면 움직임).
- chat_demo: 발화 출력 사이/후에 `session.flow()` 출력(예 콜드 시작엔 None, 발화 쌓이면 수렴도). 아키텍트가 비인터랙티브로 flow 동작 확인 가능.
- `smoke_v6.rs`:
  - INV-1: headless FakeBackend(seed 42, θ 0.65, 80틱) 두 번 실행 바이트 동일 + record에 `flow` 키 없음(content 없음 → None → 생략).
  - flow 결정성/게이팅: content 있는 history → flow Some(결정적); content 없음 → None.
  - render_chat가 flow Some/None 둘 다 panic 없이 렌더(TestBackend).
- 가드레일: 골든 보존(채팅/게이지는 라이브 전용). render_chat 시그니처 변경 시 호출처/테스트 전부 갱신. panic 금지.

## Dependencies

- task-34(record.flow, LiveSession::flow()).

## Verification

```bash
cargo build
cargo build --examples
cargo test                       # 스모크 6종(smoke ~ smoke_v6)
cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
cargo run --example chat_demo    # 전사에 [흐름] 수렴 지표가 보이고 발화 쌓이며 움직임
```

- `cargo test` green(render_chat 시그니처 갱신 후 chat/smoke_v5 테스트 포함). `smoke_v6` green.
- **골든 5종 바이트 동일**.
- chat_demo 전사에 flow 지표 출력(콜드 None → 발화 쌓이며 수렴도 갱신). 라이브 동작 확인.
- (수동) `--chat` 사이드바에 수렴/발산 게이지 표시(사용자 실제 터미널).

## Risks

| 위험 | 회피 |
|---|---|
| render_chat 시그니처 변경 회귀 | 호출처(ChatApp.run) + 렌더 테스트(chat::tests, smoke_v5) 전부 flow 인자 추가. 빌드/테스트로 확인 |
| 게이지가 골든/headless 오염 | 채팅/게이지는 라이브 전용. driver/sink 불변(task-34에서 record는 이미 처리). 골든 재확인 |
| flow None일 때 렌더 panic | Option 분기("흐름 -"). TestBackend 테스트로 None/Some 둘 다 |
| chat_demo flow 출력 노이즈 | 간결히 1줄, 값 바뀔 때만/주기적. |
