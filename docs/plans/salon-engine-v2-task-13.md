---
title: "Salon v0.2 Task 13: 미터 교차자극 표시 + 스윕 preset 비교"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v2.md
task_id: "13"
depends_on: ["10", "11", "12"]
parallel_group: ""
---

# Task 13 - 미터에 교차 자극 표시 + 스윕 preset 비교

plan `salon-engine-v2.md` subtask 13. v0.2의 케미(교차 자극)를 관찰 가능하게 한다. 미터에 각 페르소나의 현재 자극량 E_p를 보이고, 스윕에 방 프리셋 비교를 추가한다. **단 α=0이면 ObservationRecord가 v0.1 골든과 바이트 동일해야 한다(serde 생략).**

## Changed files

- `src/sink.rs` - 수정. `ObservationRecord`에 `excitations: BTreeMap<PersonaId, f64>` 추가. **`#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`** 로 비면 직렬화에서 생략.
- `src/driver.rs` - 수정. record.excitations에 state.excitations의 **0이 아닌 항목만** 넣는다.
- `src/tui.rs` - 수정. render에서 자극받은 페르소나에 E_p 표시.
- `src/sweep.rs` - 수정. 4개 preset 비교 출력 추가.

## Change description

### 골든 보존 (가장 중요)
- `ObservationRecord.excitations`는 0이 아닌 E_p만 담는다. α=0이면 모든 E_p == 0.0 → 필터 결과 빈 맵 → `skip_serializing_if`로 JSON에서 생략 → v0.1 골든과 바이트 동일.
- driver: `record.excitations = state.excitations.iter().filter(|(_, v)| **v != 0.0).map(...).collect()`. (α=0이면 정확히 0.0이라 전부 걸러짐.)

### 미터 (tui.rs render)
- 각 페르소나 게이지 줄에서 λ 숫자 옆에 자극량을 표시한다. record.excitations에 항목이 있으면 `+0.09` 형태로(없으면 표시 안 함). 예: `########|.... 0.67 +0.09`.
- 교차 자극의 "경로"는 직전 화자(chosen)가 모두를 자극한 것이므로, 별도 화살표 도식은 생략하고 E_p 수치로 충분히 읽힌다.
- α=0(기본)이면 excitations가 비어 표시가 v0.1과 동일.

### 스윕 (sweep.rs)
- 기존 θ×k 그리드 출력은 유지. 그 아래에 **preset 비교 섹션**을 추가: calm/pub/argument/chaos 각각을 같은 seed·데모 페르소나로 돌려 한 줄 요약(preset 이름, 침묵 수, 페르소나별 발화 수)을 출력. 방 분위기에 따라 리듬이 어떻게 갈리는지 비교용.
- preset 비교는 build_config(균일 α) 사용(모디파이어는 main 데모에만).

### 가드레일
- 결정성 유지. `unwrap`/`panic` 금지. TUI 렌더는 순수 함수 유지(TestBackend 검증 가능).

## Dependencies

- task-09(excitations 상태), task-10(preset), task-11(modifier), task-12. v0.1 sink/tui/sweep/driver.

## Verification

```bash
cargo test
cargo run -- --headless --seed 42 --ticks 80 --theta 0.65   # α=0 → excitations 필드 없음(골든 동일)
cargo run -- --room pub --headless --seed 42 --ticks 6       # α 활성 → excitations 필드 등장
cargo run -- --sweep                                          # θ×k + preset 비교
cargo run --example tui_preview                               # 미터에 자극 표시 확인(있으면)
```

- `cargo test` 전체 green(49 유지 + TUI 렌더 테스트가 excitations 처리). 가능하면 tui 렌더 테스트에 excitation 있는 record 케이스 추가.
- **α=0 골든 5종 바이트 동일**(빌드 후 명시적 순차 실행으로 검증). excitations 필드가 α=0에서 생략되는지 확인.
- `--room pub` 출력 JSON에 `"excitations"` 등장, `--headless`(기본) 출력엔 없음.
- `--sweep`에 preset 비교 줄(4개) 등장.

## Risks

| 위험 | 회피 |
|---|---|
| excitations 필드로 α=0 골든 깨짐 | `skip_serializing_if = is_empty` + driver가 0 항목 필터 → α=0이면 빈 맵 → 생략. 골든 5종 재확인 |
| 0.0 비교 부동소수 | α=0이면 E_p가 정확히 0.0(0*decay+0). `!= 0.0`로 필터 OK |
| TUI 줄이 좁아 깨짐 | 자극 표시는 짧게(`+0.09`), 폭 초과 시 생략 |
| sweep preset 출력이 기존 파싱 깸 | 그리드 출력 형식 유지, preset 섹션은 그 아래 추가만 |
