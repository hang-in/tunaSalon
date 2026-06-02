---
title: "Salon v0.4 Task 25: persona_collapse 병렬화 + mixed-model 벤치 + 라이브 지연 측정"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "25"
depends_on: ["24"]
parallel_group: ""
---

# Task 25 - persona_collapse 병렬화 + mixed-model 벤치 + 라이브 지연 측정

plan `salon-engine-v4.md` subtask 25. 동시성을 실제로 쓰는 도구 2개. (1) `persona_collapse` 예제를 배치 API로 병렬화(순차 3-페르소나 → 동시), (2) mixed-model 벤치 모드(일부 qwen-32b, 일부 cloud) - **qwen 라이브는 서버 가동 후 수동**, 코드는 fake로. 더불어 (3) 라이브 순차 1발화 지연을 측정해 burst 도입/보류 판단 근거를 남긴다.

## Changed files

- `examples/persona_collapse.rs` - 수정. 순차 for-loop → `pool.generate_batch`로 N 페르소나 동시 호출(같은 opening 맥락). 출력 순서는 보존. 단일 백엔드(cloud)면 cap 3 안에서 동시.
- `src/main.rs` (또는 `src/bench.rs` 신규) - 수정/신규. `--bench` 모드: room config의 backends/routing으로 mixed-model 비교(같은 맥락에 각 페르소나가 자기 백엔드로). 라이브는 cloud only, qwen 경로는 미가용 시 폴백 표시. 1발화 평균/최대 지연(wall-clock) 출력.
- `docs/plans/salon-engine-v4.md` 또는 `docs/temp/` - 측정 결과 메모(지연 수치 → burst 판단).

## Change description

- persona_collapse: 기존 의미 유지(같은 모델·다른 prompt → 출력 비교)하되 동시 호출. Ollama 미가동이면 각 줄 "(no response)"(panic 없음, 기존 동작).
- mixed-model 벤치: 같은 opening에 대해 페르소나별 라우팅 백엔드로 생성, 모델별 출력을 나란히. **현재는 cloud only 라이브** → qwen 라우팅 페르소나는 폴백(cloud/fake)로 표시되고, 서버 가동 후 동일 명령으로 실측.
- 지연 측정: 라이브 순차 틱에서 generate_one 1회 wall-clock을 N회 측정해 평균/최대 기록. 이 수치가 "라이브 burst가 필요한가"의 판단 입력(plan §완료기준).
- 가드레일: 측정/벤치는 도구 경로 - 라이브 결정 경로·골든 불침투. 네트워크 없으면 fake로 동작. panic 금지.

## Dependencies

- task-24(폴백 포함 풀/배치).

## Verification

```bash
cargo build
cargo test
cargo run --example persona_collapse                 # 병렬, Ollama 없으면 (no response)
cargo run -- --bench --seed 42                        # mixed-model 벤치(cloud only 라이브, qwen 폴백)
```

- `cargo test` green(도구라 단위 테스트는 가벼움: 배치 호출 형태·출력 순서).
- persona_collapse가 병렬로 동작하고 순차 대비 빠름(체감/타이밍 로그).
- `--bench`가 panic 없이 동작(cloud 또는 fake). qwen 라우팅은 폴백으로 표시.
- 라이브 1발화 지연 수치가 출력/기록됨(burst 판단 근거).

## Risks

| 위험 | 회피 |
|---|---|
| 벤치가 cloud budget 빨리 소진 | 경량 모델·짧은 발화·소수 반복. 사용자 모니터링(잔여 API 없음). 정액제라 달러는 안전 |
| qwen 미가용으로 벤치 의미 반감 | 코드 경로는 fake로 검증, 실측은 서버 가동 후 동일 명령. 폴백 명시 표시 |
| 측정 노이즈 | 평균+최대 둘 다, N회 반복. 첫 호출(콜드) 분리 |
| 병렬화가 example 결정성 기대 깨뜨림 | persona_collapse는 원래 라이브(비결정). 출력 순서만 보존하면 됨 |
