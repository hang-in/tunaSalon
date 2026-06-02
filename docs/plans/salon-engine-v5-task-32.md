---
title: "Salon v0.5 Task 32: v0.5 스모크 게이트"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v5.md
task_id: "32"
depends_on: ["28", "31"]
parallel_group: ""
---

# Task 32 - v0.5 스모크 게이트

plan `salon-engine-v5.md` subtask 32. v0.5 불변식을 통합 검증하는 게이트(`tests/smoke_v5.rs`, 스모크 5종째). 핵심: **라이브(HumanChannel/LiveSession/채팅 TUI) 추가가 headless 골든을 깨지 않았다**(INV-1) + 사람 참여·라이브 세션 통합이 **네트워크 없이** 동작. 라이브 cloud는 chat_demo(수동).

## Changed files

- `tests/smoke_v5.rs` - 신규. v0.5 게이트. **src 변경 없음**(공개 API + 오프라인 백엔드 127.0.0.1:1 + TestBackend로 충분).

## Change description

게이트가 assert하는 것:
- **INV-1 골든/결정성**: headless seed 42·θ 0.65·80틱(FakeBackend, `driver::run`) 두 번 실행 바이트 동일 + 모든 record `utterance` None. v0.5 라이브 코드(human/live/chat 모듈)가 들어와도 결정 경로 불변.
- **HumanChannel(task-28)**: `HumanChannel::speak` 후 history 마지막 = 사람 Event + 전 페르소나 excitation 상승(결정적, 네트워크 없음).
- **LiveSession 통합(task-29)**: 오프라인 풀로 LiveSession 구성 → `submit_human` 후 사람 Event + excitation; `tick`이 화자 선택 시 placeholder + pending; pending 중 추가 tick은 새 디스패치 안 함; `poll_generation` bounded 폴링으로 pending 해제; Drop 깔끔(no panic/hang).
- **채팅 TUI(task-30)**: `render_chat`가 사람+페르소나 발화 + 입력 + placeholder를 TestBackend에 panic 없이 렌더.
- 라이브 cloud/friend 통합은 `chat_demo` 예제(수동), 게이트는 네트워크 없이.

## Dependencies

- task-28(HumanChannel)·29(LiveSession)·30(chat)·31(통합) 전부.

## Verification

```bash
cargo build
cargo test                       # 전체 green: ... + smoke_v5. 스모크 게이트 5종
cargo test --test smoke_v5
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test --test smoke_v5` green(위 4묶음).
- 전체 `cargo test` green, 스모크 게이트 5종(smoke/v2/v3/v4/v5).
- **골든 5종 바이트 동일**.

## Risks

| 위험 | 회피 |
|---|---|
| 게이트가 네트워크 의존 | 모든 assert는 오프라인 백엔드/FakeBackend/TestBackend. 라이브는 chat_demo(수동) |
| LiveSession poll 타이밍 flaky | 오프라인은 즉시 None → bounded 폴링(짧은 sleep). 도달 보장 |
| 골든 거짓 회귀(빌드 캐시) | cargo build 후 명시적 순차 실행 |
| 라이브 모듈이 headless 오염 | human/live/chat 모두 driver::run 불침투. 게이트가 재확인 |
