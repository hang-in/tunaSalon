---
title: "Salon v0.4 Task 26: v0.4 스모크 게이트"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "26"
depends_on: ["21", "23", "24"]
parallel_group: ""
---

# Task 26 - v0.4 스모크 게이트

plan `salon-engine-v4.md` subtask 26. v0.4 불변식을 자동 검증하는 게이트. 핵심은 **LLM off 풀이 v0.1~v0.3 골든과 바이트 동일**(INV-1)이고, 동시성/cap/폴백/num_ctx가 **네트워크 없이 deterministic fake로** 검증되는 것. 라이브 호출은 `#[ignore]`로 분리.

## Changed files

- `tests/smoke_v4.rs` - 신규. v0.4 게이트(기존 smoke / smoke_v2 / smoke_v3 패턴 따름).
- (필요 시) `src/pool.rs`/`src/ollama.rs` - 테스트 훅(fake 백엔드가 outcome·지연을 주입할 수 있는 최소 인터페이스). 프로덕션 경로 영향 없게.

## Change description

게이트가 assert하는 것:
- **INV-1 골든 보존**: BackendPool을 FakeBackend로 구성(LLM off 등가) → headless seed 42, theta 0.65, 80틱 출력이 v0.3 골든과 바이트 동일. record JSON에 `utterance` 없음(`grep -c` 0).
- **INV-4 cap 준수**: fake 백엔드(진입/이탈 atomic 카운터 + 인위 지연)로 `generate_batch` 동시 in-flight 피크 ≤ max_concurrent(백엔드별; 예 3, 2). 두 백엔드 독립 cap 동시 확인.
- **폴백**(task-24): Rejected/Timeout fake → 폴백 백엔드 호출, 양쪽 실패 → None(panic 없음).
- **num_ctx**(task-21): None이면 요청 body에 num_ctx 생략, Some(n)이면 n.
- **라우팅**(task-22): persona→backend 매핑·기본 폴백.
- 라이브 Ollama 통합은 `#[ignore]`(네트워크 필요, CI skip).

## Dependencies

- task-21(num_ctx/풀), task-23(배치/세마포어), task-24(폴백). task-22 라우팅 포함.

## Verification

```bash
cargo build
cargo test                          # 전체 green: 72(v0.3) + v0.4 신규. 게이트 4종(smoke/v2/v3/v4)
cargo test --test smoke_v4          # v0.4 게이트만
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson
```

- `cargo test --test smoke_v4` green. 위 5개 assert 통과.
- 전체 `cargo test` green(기존 72 유지 + 신규). 스모크 게이트 4종 green.
- 골든 5종 바이트 동일(빌드 후 명시적 순차).

## Risks

| 위험 | 회피 |
|---|---|
| 게이트가 네트워크에 의존 | 모든 게이트 assert는 fake/headless. 라이브는 `#[ignore]` |
| cap 테스트 flaky(타이밍) | 인위 지연을 충분히 크게 + atomic 피크. 동시 진입 보장 후 단언 |
| 테스트 훅이 프로덕션 경로 오염 | 훅은 `#[cfg(test)]` 또는 생성자 주입(fake 백엔드). 라이브 경로 불변 |
| 골든 거짓 회귀(빌드 캐시) | cargo build 후 명시적 순차 실행. for-loop 안 cargo run 금지 |
