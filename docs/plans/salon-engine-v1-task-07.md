---
title: "Salon v0.1 Task 07: DebugMeter (TUI sink)"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v1.md
task_id: "07"
depends_on: ["06"]
parallel_group: "outputs"
---

# Task 07 - DebugMeter (TUI sink)

plan `salon-engine-v1.md` v0.1 작업 항목 8. headless와 동일한 ObservationRecord를 받아 터미널에 실시간으로 그리는 TUI sink. 사람이 손잡이를 돌리며 관찰하는 용도.

## Changed files

- `src/tui.rs` - 신규. `ObservationSink` 구현체(TUI 렌더러).
- `src/main.rs` - 수정. `--headless` 없는 기본 경로를 TUI로 배선.
- `Cargo.toml` - 수정. TUI 의존성 추가(`ratatui` + `crossterm` 권장).

## Change description

설계 문서 9절 표시 항목 + 9-1절(출력 sink 분리).

- ObservationRecord를 입력으로 받아 그린다: 페르소나별 λ 막대(실시간), 이번 틱 화자 선택 이유(rrf_reason), 침묵/발화 카운트, 대화 길이(턴).
- 레이아웃/비주얼 참고: `docs/temp/salon-persona-ui.md` §5(2칼럼 = 채팅 | 게이지 사이드바 + 하단 입력, 80컬럼 기준 3칼럼 회피, 각 줄 λ 막대 + θ 임계선), §4(λ 구간 → 포즈, 막대 ↔ 캐릭터 렌더러 토글 = 뷰 분리), §2 끝(게이지 값 = 모델 / 그리기 = 뷰 분리). **v0.1은 사이드바 = 막대**, 캐릭터 뷰는 이후 같은 λ 데이터에 얹는다.
- headless와 **같은 record**를 소비한다. 코어 동역학은 sink 종류와 무관(설계 9-1 계약). TUI는 표현만 담당.
- 렌더 로직은 가능한 한 순수하게: `(ObservationRecord) → 프레임 구성`을 분리해 테스트 가능하게. 터미널 raw mode 진입/복귀 같은 부수효과는 얇은 바깥 레이어에 격리.
- TUI 크레이트는 `ratatui` 권장(헤드리스 검증용 `TestBackend` 제공). 다른 크레이트 채택 시 동등한 버퍼 렌더 검증을 제공할 것.

## Dependencies

- task-06 (driver가 record를 만들고 main이 sink를 배선). task-01의 ObservationSink/Record.

## Verification

```bash
cargo build
cargo test --lib tui
```

- `cargo build` exit 0: TUI sink가 ObservationSink를 구현하고 main 기본 경로가 컴파일됨.
- `cargo test --lib tui` exit 0(1개 이상 통과 보고): `ratatui::backend::TestBackend`로 ObservationRecord 몇 개를 버퍼에 렌더 → 버퍼에 페르소나 라벨/카운트 문자열이 나타나고 panic이 없음.

## Risks

| 위험 | 회피 |
|---|---|
| TUI 렌더는 객관 검증이 어려움 | 렌더 로직을 record→프레임 순수 함수로 분리, TestBackend 버퍼 스냅샷으로 assert |
| TUI 크레이트 선택이 미확정 | `ratatui`+`crossterm` 권장(TestBackend). 변경 시 동등 검증 제공 조건 명시 |
| 터미널 raw mode 미복귀로 셸 깨짐 | 진입/복귀를 RAII 가드 또는 finish()에서 보장 |
| TUI가 코어 상태를 직접 만지며 결합 | record만 입력. 코어 → sink 단방향(설계 9-1) |
