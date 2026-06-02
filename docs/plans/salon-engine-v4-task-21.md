---
title: "Salon v0.4 Task 21: num_ctx Option화 + BackendConfig + 풀 스켈레톤"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "21"
depends_on: []
parallel_group: ""
---

# Task 21 - num_ctx Option화 + BackendConfig + BackendPool 스켈레톤

plan `salon-engine-v4.md` subtask 21. **순수 Rust, 네트워크/동시성 없음.** v0.3의 `OllamaBackend`를 풀에 담을 수 있게 정리한다. 핵심 2가지: (1) `num_ctx` 하드코딩(`8192`)을 백엔드별 `Option<u64>`로, (2) `BackendConfig` + `BackendPool` 레지스트리 골격(동시성·라우팅은 이후 task). 골든 5종 보존이 게이트.

## Changed files

- `src/ollama.rs` - 수정. `DEFAULT_NUM_CTX` 제거, `OllamaBackend`에 `num_ctx: Option<u64>` 필드. `new()`에 `num_ctx` 인자 추가. `build_request_body(model, prompt, system, num_ctx)` 시그니처 변경: None이면 `options.num_ctx` 생략(options가 비면 options 자체 생략 가능), Some(n)이면 설정. 기존 테스트(`build_request_body_sets_num_ctx`, `build_request_body_*`) 갱신.
- `src/pool.rs` - 신규. `BackendConfig { name, model, endpoint, api_key: Option<String>, max_concurrent: usize, num_ctx: Option<u64>, timeout: Duration }` + `BackendPool` 골격(name → OllamaBackend 맵, 동시성/라우팅 필드는 자리만). 아직 generate 라우팅 없음.
- `src/lib.rs` - 수정. `pub mod pool;`.
- `examples/persona_collapse.rs` - 수정. `OllamaBackend::new` 호출에 `num_ctx`(로컬이면 `Some(8192)`) 인자 추가(빌드 깨짐 방지).
- `src/main.rs` - 수정. 기존 OllamaBackend 생성부에 `num_ctx` 인자(로컬 `Some(8192)`, cloud `None`) 전달.

## Change description

- `num_ctx: Option<u64>`: 백엔드가 보유. `build_request_body`가 인자로 받아 None이면 요청 body에서 `num_ctx`를 **완전히 생략**(cloud/원격이 모델 최대 ctx로 자동 설정하게). Some(n)이면 `options.num_ctx = n`.
  - 정책: 로컬 e4b는 `Some(8192)`(RAM 상한), cloud/원격은 `None`(auto-max). 단일 `--llm --model` 경로의 기본은 로컬 가정 `Some(8192)` 유지 → 기존 동작 보존.
- `BackendConfig`/`BackendPool`은 이 task에선 **데이터 구조 + 생성자만**. 라우팅(`generate_one`)은 task-22, 세마포어/배치는 task-23. 컴파일되고 단위 테스트(구성 보유)만.
- 가드레일: `unwrap`/`panic` 금지(빌더 실패는 v0.3처럼 폴백). 결정성·골든 보존. silent fallback 최소(num_ctx None은 의도된 생략이므로 OK).

## Dependencies

- v0.3 전체(`OllamaBackend`, `PersonaRuntime`).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 > /tmp/s42_t065.ndjson
diff /tmp/s42_t065.ndjson /tmp/salon_golden/s42_t065.ndjson   # 바이트 동일
```

- `cargo test` 전체 green. 갱신 테스트: `build_request_body`에 num_ctx 인자 → (1) `None`이면 body에 `num_ctx` 없음, (2) `Some(8192)`이면 8192. `BackendConfig` 생성/필드 보유 단위 테스트.
- **골든 5종 바이트 동일**(빌드 후 명시적 순차 diff. for-loop 안 cargo run 금지 - 첫 실행 재빌드로 빈 출력 거짓 회귀).
- `grep -c num_ctx` 로 cloud 경로(None)에서 요청 body에 num_ctx가 안 들어가는지(단위 테스트 수준).

## Risks

| 위험 | 회피 |
|---|---|
| 시그니처 변경으로 골든/테스트 회귀 | `build_request_body`·`new` 호출처 전부 갱신(example/main). 단일 백엔드 기본 8192 유지. 골든 5종 재확인 |
| options 비었을 때 직렬화 형태 변화 | num_ctx None이고 다른 options 없으면 `options` 키 자체 생략 가능 - 라이브 cloud 요청에만 영향(골든은 FakeBackend라 무관) |
| 풀 골격 과설계 | 이 task는 구조+생성자만. 라우팅/동시성 미포함(YAGNI). 투기적 필드 금지 |
| 리터럴 churn(num_ctx 인자) | example/main/테스트 호출처 기계적 갱신, 빌드로 확인 |
