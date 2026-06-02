---
title: "Salon v0.4 Task 24: 거부/타임아웃 폴백 + 백오프"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "24"
depends_on: ["23"]
parallel_group: ""
---

# Task 24 - 거부/타임아웃 폴백 + 백오프

plan `salon-engine-v4.md` subtask 24. Ollama Cloud는 동시성 3 초과·큐 풀이면 **거부(비2xx, 429 등)** 한다. 거부·타임아웃을 분류해 폴백 백엔드(또는 FakeBackend)로 우회하고 백오프한다. 백엔드 unhealthy면 라우팅에서 우회. panic 금지. 검증은 fake로 거부/타임아웃을 시뮬레이션.

## Changed files

- `src/ollama.rs` - 수정. generate 결과를 `Option`이 아니라 분류 가능한 형태로(내부 enum `GenerateOutcome { Ok(String), Rejected, Timeout, Failed }` 또는 `Result`). 비2xx 중 429/503/큐풀을 `Rejected`로, 타임아웃을 `Timeout`으로 구분(키는 절대 로그 금지 - v0.3 INV-6 계승).
- `src/pool.rs` - 수정. `generate_one`/`generate_batch`에 폴백: 1차 백엔드가 Rejected/Timeout/Failed면 (a) 백오프 후 1회 재시도(선택) 또는 (b) `fallback_backend`(config) 또는 FakeBackend로 우회. 백엔드 unhealthy 마킹(연속 실패 시 일시 우회).
- `src/pool.rs` - `BackendConfig`/풀에 `fallback: Option<String>`(폴백 백엔드 이름) + 간단 백오프(고정/지수, 짧게).

## Change description

- 분류: HTTP 상태로 Rejected(429/503/queue-full 류) vs Failed(기타 비2xx/네트워크) vs Timeout. 라이브 경로에선 v0.3처럼 최종적으로 None이 되더라도, **폴백을 먼저 시도**.
- 폴백 정책(라이브 generate_one): 1차 백엔드 거부/타임아웃 → fallback 백엔드 시도 → 그것도 실패면 None(내용 없는 발화, 엔진 결정 유지). 백오프는 짧게(예: 250ms 1회), 무한 재시도 금지.
- unhealthy 우회: 한 백엔드가 연속 N회 실패하면 짧은 기간 라우팅에서 우회(다음 후보/기본). 시간 기반 회복.
- 운영 제약: 지인서버 미가용이므로 **qwen는 영구 unhealthy처럼 동작 → cloud/fake로 폴백**되는 게 현재 정상 경로. 이 task가 그 폴백을 명시적으로 처리.
- 가드레일: panic/unwrap 금지. 키 비노출. 폴백은 명시적(silent 아님) - stderr 한 줄(키 제외) 또는 record에 폴백 표시(선택).

## Dependencies

- task-23(배치/세마포어, generate 경로).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 > /tmp/s42.ndjson
diff /tmp/s42.ndjson /tmp/salon_golden/s42_t065.ndjson   # 골든 동일
```

- `cargo test` green. 단위 테스트(fake, 네트워크 없이): (1) Rejected 반환 fake → 폴백 백엔드 호출됨, (2) Timeout fake → 폴백, (3) 1차·폴백 모두 실패 → None(panic 없음), (4) 연속 실패 백엔드가 unhealthy로 우회됨, (5) 상태 분류(429→Rejected 등). fake가 지정 outcome을 반환하도록 구성.
- **골든 보존**: LLM off 라이브 결정 경로 불변.
- (수동) cloud 동시성 3 초과 burst 시 거부→폴백이 panic 없이 동작(서버 가동 후).

## Risks

| 위험 | 회피 |
|---|---|
| 폴백 무한 루프/재시도 폭주 | 재시도 1회 + 짧은 백오프 상한. 최종 None. 무한 대기 금지 |
| 키가 에러/로그에 샘 | 분류는 상태코드만 사용. 메시지에 키·Authorization 금지(INV-6). 단위 테스트로 미노출 단언 |
| 폴백이 결정성/골든 오염 | 폴백은 라이브 생성 경로만. 엔진 결정·rng 불변. 골든 재확인 |
| unhealthy 마킹이 영구 차단 | 시간 기반 회복(짧은 쿨다운 후 재시도). qwen 미가용은 의도된 폴백 |
| Rejected 오분류(정상 비2xx를 거부로) | 상태코드 화이트리스트(429/503/queue-full만 Rejected), 나머지 Failed |
