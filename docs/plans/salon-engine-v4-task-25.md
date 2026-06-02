---
title: "Salon v0.4 Task 25: persona_collapse 병렬화 + mixed-model 벤치 + 지연 측정"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "25"
depends_on: ["24"]
parallel_group: ""
---

# Task 25 - persona_collapse 병렬화 + mixed-model 벤치 + 지연 측정

plan `salon-engine-v4.md` subtask 25. 동시성을 실제로 쓰는 도구. (1) `persona_collapse`를 `generate_batch`로 병렬화(순차 3-페르소나 → 동시), (2) **신규 mixed-model 벤치 example**: 한 방에 cloud(`gemma4:31b-cloud`) + friend(`qwen3.6-35b-fast`) 두 백엔드를 라우팅해 같은 맥락에 동시 생성, 모델별 출력 비교, (3) 라이브 순차 1발화 지연 측정(burst 보류 판단 근거).

> **상태(2026-06-02)**: 두 백엔드 모두 라이브 검증 완료(cloud gemma4:31b-cloud, friend qwen3.6-35b-fast). examples로 구현해 main/골든 경로 불침투. 로컬 ollama 금지 → 기본 모델은 cloud.

## Changed files

- `examples/persona_collapse.rs` - 수정. 단일 백엔드(cloud) `BackendPool` + `generate_batch`로 3 페르소나 동시 호출(같은 opening). 출력 순서 보존. cap 3 안에서 동시.
- `examples/mixed_bench.rs` - 신규. cloud + friend 풀, 페르소나별 라우팅(예: 1명 friend, 나머지 cloud), 같은 opening에 `generate_batch`, "[persona via backend/model] 출력" 나란히. + 순차 1발화 지연 micro-측정(K회, 첫 콜드 제외 avg/max).
- (메모) 측정한 지연 수치는 리뷰 시 plan §완료기준 또는 메모리에 기록.

## Change description

- persona_collapse: 의미 유지(같은 모델·다른 persona prompt → 톤 비교)하되 동시. `BackendPool::new()` + `add(BackendConfig::new("cloud", "gemma4:31b-cloud", "http://localhost:11434", None, 3, None, timeout), demo_prompts())` + `set_default("cloud")`. jobs=[(persona, opening_history) ...] → `generate_batch`. 서버 미가동/미응답이면 해당 줄 None("(no response)") — panic 없음.
- mixed_bench(신규): 두 백엔드 등록 —
  - cloud: `BackendConfig::new("cloud", env SALON_CLOUD_MODEL 기본 "gemma4:31b-cloud", "http://localhost:11434", None, 3, None, t)`
  - friend: `BackendConfig::new_openai("friend", env SALON_FRIEND_MODEL 기본 "qwen3.6-35b-fast", env SALON_FRIEND_ENDPOINT 기본 "http://yongseek.iptime.org:8008", None, 1, Some(256), t)`
  - `add(.., demo_prompts())` 둘 다, `set_default("cloud")`, `add_route(<한 페르소나>, "friend")`.
  - 같은 opening으로 `generate_batch`, 각 결과를 페르소나·백엔드(resolve로 조회)·모델과 함께 출력.
- 지연 측정: `pool`(PersonaRuntime) 또는 backend의 generate를 순차로 K회 호출해 `Instant` wall-clock 측정, 첫 호출(콜드) 제외 avg/max 출력. "라이브 순차 틱이 1발화당 ~Xms 블록 → burst 필요성" 판단 근거.
- 가드레일: examples는 라이브 도구 - 라이브 결정 경로·골든 불침touch. 네트워크 없으면 None/(no response). panic/unwrap 금지. 키 비노출.

## Dependencies

- task-24(폴백 포함 풀/배치).

## Verification

```bash
cargo build --examples       # 두 example 컴파일
cargo test                   # 기존 그대로 green (examples는 test 아님)
cargo run --example persona_collapse              # 병렬, cloud 미가동이면 (no response)
cargo run --example mixed_bench                    # cloud + friend 동시, 모델별 출력 + 지연
```

- `cargo build --examples` 성공, `cargo test` 기존 green 유지.
- persona_collapse 병렬 동작(3 페르소나 동시).
- mixed_bench가 **cloud + friend 두 모델 출력을 나란히** 보여줌(라이브). 미가동 백엔드는 폴백/None, panic 없음.
- 라이브 1발화 지연 수치 출력(avg/max).

## Risks

| 위험 | 회피 |
|---|---|
| 벤치가 cloud budget 소진 | 경량·짧은 발화·소수 반복. 정액제라 달러 폭증 없음, budget만. 사용자 모니터링 |
| friend 동시성 1이라 병렬 이점 적음 | friend는 1개만 라우팅(직렬), cloud는 다수(동시). 벤치 목적은 모델 비교지 처리량 아님 |
| reasoning 지연(친구 모델) | qwen3.6-35b-fast(reasoning off) + enable_thinking=false. ~0.7s 검증됨 |
| 측정 노이즈 | 첫 콜드 제외, K회 avg+max |
| example 컴파일 깨짐이 cargo test 막음 | `cargo build --examples`로 사전 확인 |
