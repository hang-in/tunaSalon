---
title: "Salon v0.4 Task 26: v0.4 스모크 게이트"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "26"
depends_on: ["21", "22", "23", "24", "27"]
parallel_group: ""
---

# Task 26 - v0.4 스모크 게이트

plan `salon-engine-v4.md` subtask 26. v0.4 불변식을 자동 검증하는 게이트(`tests/smoke_v4.rs`, 기존 smoke/v2/v3 패턴). 핵심: **기본 경로(LLM off = FakeBackend, 풀 미사용)가 v0.1~v0.3 골든과 바이트 동일**(INV-1) — 풀/추상화/동시성 코드가 들어와도 결정 경로 불변. 풀 속성(라우팅/폴백/cap/num_ctx/Backend 분기)은 **네트워크 없이**(오프라인 백엔드/fake) 검증. 라이브는 `#[ignore]`.

> **정정**: `BackendPool`은 `Backend{Ollama,OpenAI}`만 담고 FakeBackend를 담지 않는다. 골든 경로는 풀이 아니라 main의 FakeBackend 직접 경로다(`--llm` 없을 때). 따라서 INV-1 게이트 = "headless FakeBackend 출력이 골든과 동일" + "두 번 실행이 바이트 동일"이며, 풀은 별도로 오프라인/단위 수준에서 검증한다.

## Changed files

- `tests/smoke_v4.rs` - 신규. v0.4 게이트. **src 변경 없음**(오프라인 백엔드 127.0.0.1:1 + 기존 공개 API로 충분, 프로덕션 훅 불필요).

## Change description

게이트가 assert하는 것:
- **INV-1 골든/결정성**: headless seed 42·theta 0.65·80틱(FakeBackend, `--llm` 없음) 출력이 (a) 두 번 실행 바이트 동일, (b) `/tmp/salon_golden/s42_t065.ndjson`와 동일. record JSON에 `utterance` 없음.
- **라우팅(task-22)**: `resolve(persona)`가 routing 지정→해당 백엔드, 미지정→default, 없으면 None.
- **폴백 체인(task-24)**: `fallback_chain`이 [primary, fallback] 순서, 사이클 안전(유한).
- **배치 순서/무패닉(task-23)**: 오프라인 백엔드 풀로 `generate_batch` → 입력 순서 보존, 전부 None, panic 없음. (엄격 cap≤n 피크는 `pool::tests`의 `run_with_caps` 단위 테스트가 이미 게이트하므로 여기선 통합 동작 위주.)
- **num_ctx(task-21)** / **Backend 분기(task-27)**: `build_request_body` num_ctx None 생략 / Some(n); OpenAI `parse_response`가 `content` 추출·`reasoning` 무시 — (대부분 단위 테스트 존재, smoke는 대표 1~2개 재확인 가능).
- 라이브(cloud/friend) 통합은 `#[ignore]`(네트워크).

## Dependencies

- task-21·22·23·24·27 전부(풀/라우팅/배치/폴백/추상화).

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
