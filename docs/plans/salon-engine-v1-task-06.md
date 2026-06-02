---
title: "Salon v0.1 Task 06: 틱 루프 드라이버 + headless writer + CLI"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v1.md
task_id: "06"
depends_on: ["02", "03", "04", "05"]
parallel_group: ""
---

# Task 06 - 틱 루프 드라이버 + headless writer + CLI 진입점

plan `salon-engine-v1.md` v0.1 작업 항목 6, 7. 엔진 모듈들을 틱 루프로 엮고, headless sink(결정적 NDJSON)와 CLI 진입점을 붙여 "직접 돌려볼 수 있는" 첫 바이너리를 만든다. v0.1의 키스톤 task.

## Changed files

- `src/driver.rs` - 신규. 틱 루프(모듈명은 `loop`이 Rust 예약어라 `driver`).
- `src/headless.rs` - 신규. `ObservationSink` 구현체. record를 `serde_json`으로 한 줄씩 stdout에 씀.
- `src/main.rs` - 수정. CLI 인자 파싱(`--headless`, `--seed <N>`, `--ticks <N>`, 그리고 튜닝 손잡이 `--theta`/`--k`/`--beta <f64>`)과 driver+sink 배선. 손잡이 오버라이드는 미터/스모크로 리듬을 탐색하기 위함(전역 EngineConfig 값, 페르소나별 아님).
- `src/lib.rs` - 수정. `pub mod driver;` `pub mod headless;` 추가(additive).

## Change description

plan §2 v0.1 틱 루프 전체를 드라이버로 구현한다.

- 드라이버 시그니처(예): `run(config, personas, seed, ticks, sink: &mut dyn ObservationSink)`. seed로 `ChaCha8Rng`를 만들어 RRF/utterance에 주입.
- v0.1 데모 페르소나(1~3명)는 main.rs에서 손으로 구성한다. μ 초기값은 `docs/temp/salon-persona-ui.md` §6 역할별 μ 표에서 가져온다(예: 수다많은=friend 0.80, 조용한=summarizer 0.25, 가끔 끼어드는=chaos 0.70). 고정값이 아니라 미터로 보며 튜닝할 출발점이다.
- 매 틱: (1) task-02 `update_intensities`로 강도 회복 → (2) task-03 게이트 → (3) 후보 있으면 task-04 RRF로 1명 선택 → (4) task-05 fake 발화 → (5) 이벤트 기록 + 그 페르소나의 저장 강도에 `HawkesEngine::suppressed_after_speak(base_rate)`를 **1회 적용**(매 틱 재적용 금지) + ObservationRecord를 sink로 emit. 침묵 틱도 record를 emit(gate_passed=false).
- CLI 인자 파싱은 `std::env::args` 기반 최소 구현(외부 크레이트 불필요). `--headless`면 `HeadlessSink`로 NDJSON을 stdout에. **이 task에서 `--headless` 없이 실행하면 usage를 출력하고 종료**한다(TUI 기본 경로는 task-07에서 붙임).
- HeadlessSink는 매 emit마다 record를 `serde_json::to_string` 후 개행 1개로 출력. 한 줄 = 한 record(NDJSON). 벽시계/난수 직접 사용 금지(ts는 논리 시각).
- 결정성: 같은 seed면 출력 NDJSON이 바이트 단위로 동일해야 한다.

## Dependencies

- task-02, task-03, task-04, task-05 (엔진 모듈 전부). task-01의 ObservationSink/타입.

## Verification

```bash
cargo test --lib driver
cargo run -- --headless --seed 42 --ticks 50 | jq -e -c . > /dev/null
cargo run -- --headless --seed 42 --ticks 50 > /tmp/salon_a.ndjson
cargo run -- --headless --seed 42 --ticks 50 > /tmp/salon_b.ndjson
diff /tmp/salon_a.ndjson /tmp/salon_b.ndjson
```

- `cargo test --lib driver` exit 0(고정 seed + VecSink로 결정적 이벤트 시퀀스 1개 이상 테스트).
- `jq -e -c .` exit 0: 모든 출력 라인이 유효 JSON(NDJSON).
- `diff` exit 0: 같은 seed 두 실행이 동일 출력(결정성).

## Risks

| 위험 | 회피 |
|---|---|
| `loop` 예약어로 모듈명 충돌 | 모듈/파일명 `driver` |
| `--headless` 없는 기본 경로가 task-07 전엔 빈손 | 이 task에선 usage 출력 후 종료로 명세. TUI는 task-07 |
| stdout 버퍼링/플러시 누락으로 라인 유실 | emit마다 `writeln!` + 종료 시 flush, finish()에서 보장 |
| jq 미설치 환경 | Developer가 미설치면 보고. NDJSON 유효성은 `python -c`로 대체 가능 |
| 침묵 틱 미기록으로 미터 공백 | 침묵 틱도 record emit(gate_passed=false) |
