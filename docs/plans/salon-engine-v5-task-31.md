---
title: "Salon v0.5 Task 31: 데모 룸 통합 + --chat + chat_demo"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v5.md
task_id: "31"
depends_on: ["29", "30"]
parallel_group: ""
---

# Task 31 - 데모 룸 통합 + `--chat` + chat_demo

plan `salon-engine-v5.md` subtask 31. v0.4 백엔드 풀 + 데모 페르소나 + LiveSession(29) + ChatApp(30)을 결선해 **실제 채팅방**을 띄운다. (a) `main.rs --chat`(인터랙티브 TUI, 사용자가 검증), (b) `examples/chat_demo.rs`(비인터랙티브 - 스크립트된 사람 턴 + 전사 stdout, 아키텍트가 실제 cloud로 라이브 루프 검증; TTY 불요).

## Changed files

- `src/main.rs` - 수정. `--chat` opt-in: 데모 룸 풀(cloud `gemma4:31b-cloud` + friend `qwen3.6-35b-fast`, friend→cloud 폴백) + 데모 페르소나 + LiveSession + ChatApp.run(). 비-TTY면 graceful 에러(ChatApp::new Err → eprintln + exit, headless 권유). 로컬 모델 가드 계승(cloud 모델이라 미발동).
- `examples/chat_demo.rs` - 신규. 비인터랙티브 라이브 루프 데모: 같은 데모 룸 + LiveSession, ~N틱(또는 bounded time) 돌며 중간에 스크립트된 `submit_human` 1회, `poll_generation`으로 발화 도착 시 stdout 전사 출력. 라이브 코어 end-to-end 증명.

## Change description

- 데모 룸 구성(공통 헬퍼 or 인라인): BackendPool = cloud("cloud", gemma4:31b-cloud, localhost:11434, cap 3, num_ctx None) + friend("friend", qwen3.6-35b-fast, yongseek.iptime.org:8008, OpenAI, cap 1, max_tokens 256). `add(.., demo_prompts())` 양쪽. `set_default("cloud")`, `add_route("summarizer","friend")`, `set_fallback("friend","cloud")`(지인서버 다운 시 cloud로). `Arc::new(pool)`.
- 데모 페르소나: 기존 demo_personas(friend/chaos/summarizer μ) + demo_persona_system_prompts 재사용. human_speaker_id = "나"(또는 "you").
- `--chat`: `LiveSession::new(config, personas, seed, Arc<pool>, "나")` → `ChatApp::new(session, names, theta)?.run()?`. 비-TTY/에러 시 graceful(headless 안내), panic 금지.
- `chat_demo.rs`: 동일 룸 + LiveSession. 루프: 일정 횟수/시간 tick + poll, 새 발화(content Some)면 "이름: content" 출력. 중간(예 5번째 발화 후) `submit_human("나: 안녕, 다들 뭐해?")` 스크립트 → 이후 페르소나가 사람에게 반응하는지 전사로 관찰. cloud 미가동/None이면 "(...)" 표시, panic 없음. bounded(예 최대 ~30s 또는 K발화).
- 가드레일: `--chat`는 헤드리스/기본 경로 불침투(별도 모드). 골든 불변. 키 비노출. 로컬 ollama 금지 계승.

## Dependencies

- task-29(LiveSession), task-30(ChatApp), v0.4 풀.

## Verification

```bash
cargo build
cargo build --examples
cargo test
# 아키텍트 라이브 검증(비인터랙티브, TTY 불요):
cargo run --example chat_demo
# 비-TTY graceful 확인:
cargo run -- --chat            # 파이프/비터미널이면 graceful 에러 + exit(hang/panic 없음)
# 골든:
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo build`/`--examples`/`cargo test` green.
- `chat_demo`가 cloud로 실제 페르소나 발화를 전사 출력(라이브 루프 동작), 스크립트된 사람 턴 후 반응 관찰. cloud 미가동이면 graceful(no panic).
- `--chat` 비-TTY에서 graceful 에러(터미널 안 깨짐). **인터랙티브 실사용은 사용자가 실제 터미널에서 검증**(타이핑 → 페르소나 반응, 종료 시 터미널 복원).
- **골든 5종 바이트 동일**.

## Risks

| 위험 | 회피 |
|---|---|
| 인터랙티브 TUI를 아키텍트가 완전 검증 불가(비-TTY) | chat_demo(비인터랙티브)로 라이브 코어 증명 + --chat 비-TTY graceful 확인. 인터랙티브 실사용은 사용자 검증 |
| 지인서버 다운으로 mixed-model 반쪽 | friend→cloud 폴백(task-24). friend 라우팅 페르소나도 cloud로 응답 |
| cloud budget 소진 | chat_demo bounded(시간/발화 수), 경량 발화. 정액제라 달러 안전 |
| --chat가 골든/headless 오염 | 별도 모드, driver/sink 불변. 골든 재확인 |
| 터미널 복원 실패 | ChatApp restore/Drop(task-30). --chat 비-TTY는 진입 전 Err |
