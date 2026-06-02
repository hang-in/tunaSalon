---
title: "Salon v0.1 Task 01: 스캐폴드 + 코어 타입 + ObservationSink 트레이트"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v1.md
task_id: "01"
depends_on: []
parallel_group: ""
---

# Task 01 - 프로젝트 스캐폴드 + 코어 타입 + ObservationSink 트레이트

plan `salon-engine-v1.md` v0.1 작업 항목 1. Cargo 프로젝트를 만들고 엔진 코어가 다룰 데이터 타입과 출력 계약(ObservationSink)을 정의한다. 이후 모든 task의 기반이다.

## Changed files

전부 신규.

- `Cargo.toml` - 신규. 의존성: `serde` + `serde_json`(NDJSON 직렬화), `rand` + `rand_chacha`(결정적 seed RNG).
- `src/lib.rs` - 신규. 라이브러리 루트. `pub mod model;` `pub mod sink;` 선언.
- `src/main.rs` - 신규. 바이너리 진입점 stub. 이 task에서는 usage만 출력(실제 실행은 task-06).
- `src/model.rs` - 신규. 도메인 타입.
- `src/sink.rs` - 신규. `ObservationRecord` + `ObservationSink` 트레이트 + 테스트용 in-memory sink.
- `Cargo.lock` - `cargo build`가 생성. `.gitignore` 정책상 커밋한다(삭제 금지).

## Change description

plan §2 "데이터 모델 (최소)" 를 그대로 옮긴다. v0.1 범위(μ만)에 필요한 최소 필드만.

- `EngineConfig` - `beta: f64`(감쇠), `theta: f64`(게이트 임계), `k: f64`(RRF), `tick_interval: f64`. 하드코딩 금지, 전부 필드.
- `Persona` - `id`, `name`, `base_rate: f64`(μ). model/prompt 필드는 두지 않는다(v0.3부터).
- `Event` - `ts: f64`, `speaker: PersonaId`, `mark: f64`. content는 v0.3부터.
- `EngineState` - `intensities`(PersonaId → λ map), `history: Vec<Event>`, `last_speaker: Option<PersonaId>`, `rng_seed: u64`.
- `CouplingMatrix`(α) - 구조만 정의하고 v0.1에선 사용하지 않는다(전부 0 또는 미주입). 주석으로 "v0.2부터 사용" 명시.
- `ObservationRecord`(src/sink.rs) - `tick: u64`, `ts: f64`, `intensities`(map), `gate_passed: bool`, `candidates: Vec<PersonaId>`, `chosen: Option<PersonaId>`, `rrf_reason: Option<String>`, `silence_count: u64`, `speak_count: u64`, `conversation_len: u64`. `serde::Serialize` 파생. **`ts`는 논리 시뮬레이션 시각(`tick * tick_interval`)이며 벽시계 시각이 아니다** - headless 출력이 실행마다 동일해야 하기 때문.
- `ObservationSink`(src/sink.rs) - `trait ObservationSink { fn emit(&mut self, record: &ObservationRecord); fn finish(&mut self) {} }`. 코어가 출력 종류를 모르게 하는 유일한 출력 계약. 테스트용 `VecSink`(받은 record를 `Vec`에 쌓음)도 같이 둔다.

α는 미사용, LLM/임베딩 없음, 직렬화는 serde_json 한 줄.

맵(`EngineState.intensities`, `CouplingMatrix.values`, `ObservationRecord.intensities`)은 **`BTreeMap`을 쓴다(`HashMap` 금지)**. HashMap은 프로세스마다 반복 순서가 무작위라 NDJSON 키 순서가 실행마다 달라져 결정성(task-06 diff 검증)이 깨진다. BTreeMap은 정렬 순서로 항상 동일.

## Dependencies

없음. 최초 task.

## Verification

```bash
cargo build
cargo test --lib
```

- `cargo build` exit 0.
- `cargo test --lib` exit 0. 포함 테스트(3개 통과 보고): (1) `EngineConfig`/`Persona`/`EngineState` 구성, (2) `ObservationRecord`를 `serde_json::to_string`으로 직렬화 → 개행 없는 한 줄 JSON, (3) `VecSink`에 emit하면 길이가 늘어남. (`cargo test --lib`는 필터를 하나만 받으므로 `model sink`처럼 두 개를 붙이지 않는다.)

## Risks

| 위험 | 회피 |
|---|---|
| 타입을 너무 일찍 확정해 후속 task에서 깨짐 | v0.1 범위(μ만) 최소 필드만. content/model/α는 두지 않거나 미사용 구조만 |
| `ts`에 벽시계를 넣어 출력 비결정 | `ts = tick * tick_interval` 논리 시각으로 고정 |
| serde 파생 누락으로 NDJSON 직렬화 실패 | `ObservationRecord`에 `#[derive(Serialize)]`, 검증 테스트로 한 줄 직렬화 확인 |
| RNG 크레이트 버전 간 비재현 | `rand_chacha::ChaCha8Rng`(버전 안정) 채택, 전역 RNG 금지(주입식) |
