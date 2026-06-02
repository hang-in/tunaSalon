---
title: "Salon v0.7 Task 38: cooling 표시(사이드바) + v0.7 게이트"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v7.md
task_id: "38"
depends_on: ["37"]
parallel_group: ""
---

# Task 38 - cooling 표시 + v0.7 스모크 게이트

plan `salon-engine-v7.md` subtask 38. MetaController 작동을 **눈에 보이게** - 채팅 사이드바에 식힘(mu_scale) 표시 + chat_demo cooling 출력(진동 관찰) + v0.7 게이트(smoke_v7). v0.7 완료.

## Changed files

- `src/chat.rs` - 수정. `render_chat`에 `mu_scale: f64` 인자 추가 → 사이드바 흐름 게이지 아래 "식힘 ×{mu_scale:.2}" 한 줄. ChatApp.run()이 `session.mu_scale()` 전달. (호출처/렌더 테스트 갱신.)
- `examples/chat_demo.rs` - 수정. 새 발화 시 `session.mu_scale()`도 출력(예 `[식힘] ×0.78`) - 수렴 쌓이면 내려가는 게 보임.
- `tests/smoke_v7.rs` - 신규. v0.7 게이트.
- (필요 시) `src/chat.rs`/`tests/smoke_v5.rs`/`tests/smoke_v6.rs`의 `render_chat` 호출에 mu_scale 인자 추가.

## Change description

- `render_chat(..., mu_scale: f64)`: 흐름 게이지 줄 아래에 "식힘 ×{mu_scale:.2}"(1.00=식힘 없음, 낮을수록 식힘 강함). 사용자가 수렴→식힘을 미터로 관찰.
- ChatApp.run(): `render_chat(..., self.session.mu_scale())`.
- chat_demo: 새 발화마다 flow와 함께 `[식힘] ×{:.2}` 출력. content 쌓여 수렴 오르면 mu_scale 내려가는지 라이브 확인.
- `smoke_v7.rs`:
  - INV-1: headless FakeBackend(seed 42, θ 0.65, 80틱) 두 번 실행 바이트 동일(mu_scale 경로 1.0). flow None → record flow 키 부재.
  - MetaController no-op: `MetaController::default().cooling(None) == 1.0`; driver/live FakeBackend → mu_scale 1.0.
  - update_intensities: mu_scale 1.0 == 기존, mu_scale<1 → 더 낮게 회복(유계 ≥ floor).
  - render_chat가 mu_scale(1.0/<1) 둘 다 panic 없이 렌더 + "식힘" 라벨/값 표시.
- 가드레일: 골든 보존(표시는 라이브 전용). render_chat 시그니처 변경 시 호출처/테스트 전부 갱신. panic 금지.

## Dependencies

- task-37(LiveSession::mu_scale, update_intensities mu_scale).

## Verification

```bash
cargo build
cargo build --examples
cargo test                       # 스모크 7종(smoke ~ smoke_v7)
cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
cargo run --example chat_demo    # [흐름] 수렴 + [식힘] ×mu_scale 같이 출력, 수렴 오르면 식힘 내려감
```

- `cargo test` green(render_chat 시그니처 갱신 후 chat/smoke_v5/v6 포함). `smoke_v7` green.
- **골든 5종 바이트 동일**.
- chat_demo 전사에 흐름·식힘 둘 다 출력(라이브 관찰). `--chat` 사이드바에 식힘 표시(수동).

## Risks

| 위험 | 회피 |
|---|---|
| render_chat 시그니처 churn | 호출처(ChatApp.run) + 테스트(chat/smoke_v5/v6) 전부 mu_scale 추가. 빌드/테스트 |
| 표시가 골든/headless 오염 | 표시는 라이브 전용. driver/sink 불변. 골든 재확인 |
| mu_scale=1.0 표시 노이즈 | 식힘 없을 땐 "식힘 ×1.00"(또는 "-"). 간결히 |
| 진동 라이브 관찰 필요 | 약한 게인 기본. SALON_META_GAIN로 조절. 미터로 보고 사용자가 튜닝 |
