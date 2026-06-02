---
title: "Salon v0.8 Task 42: v0.8 스모크 게이트 + 마감"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v8.md
task_id: "42"
depends_on: ["40", "41"]
parallel_group: ""
---

# Task 42 - v0.8 스모크 게이트

plan `salon-engine-v8.md` subtask 42. friend engine 첫 증분의 불변식을 통합 검증(smoke_v8, 스모크 8종째). 핵심: **회상 추가가 골든을 안 깬다**(driver/PersonaRuntime 경로 recall 미주입) + 회상 결정성·참여 격리·content 게이팅. v0.8 완료.

## Changed files

- `tests/smoke_v8.rs` - 신규. v0.8 게이트. src 변경 없음(공개 API).

## Change description

게이트 assert:
- **INV-1 결정성/골든**: headless FakeBackend(seed 42, θ 0.65, 80틱) 두 번 실행 바이트 동일. record에 회상 흔적 없음(driver는 recall=None).
- **회상 결정성 + 참여 격리**(task-39): MemoryStore에 사건 심고 `recall` 두 번 동일, 미참여 방 회상 안 함(recall_eval과 별개로 게이트에서 재확인).
- **회상 슬롯**(task-41): `OllamaBackend::assemble_user_prompt`(또는 build_request_body)와 `OpenAIBackend` 조립이 recall=Some이면 `[기억]` 섹션 포함, None이면 생략.
- **content 게이팅**: 오프라인 LiveSession(content 없음)에서 store에 발화 사건이 안 쌓임 → 회상 None.
- 라이브(cloud/friend) 통합은 recall_eval(검색층) + 수동(--chat)으로. 게이트는 네트워크 없이.

## Dependencies

- task-39·40·41.

## Verification

```bash
cargo build
cargo test
cargo test --test smoke_v8
cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test --test smoke_v8` green. 전체 cargo test green, 스모크 8종(smoke ~ smoke_v8).
- **골든 5종 바이트 동일**.

## Risks

| 위험 | 회피 |
|---|---|
| 게이트가 네트워크 의존 | 오프라인 백엔드/FakeBackend/순수 MemoryStore. 라이브는 #[ignore]/수동 |
| 회상 슬롯 검증이 backend 내부 의존 | 공개 build_request_body/parse로 검증(기존 패턴) |
| 골든 거짓 회귀 | cargo build 후 명시적 순차 |
