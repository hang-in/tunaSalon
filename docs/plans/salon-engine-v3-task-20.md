---
title: "Salon v0.3 Task 20: v0.3 스모크 게이트"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v3.md
task_id: "20"
depends_on: ["15", "18"]
parallel_group: ""
---

# Task 20 - v0.3 스모크 게이트

plan `salon-engine-v3.md` subtask 20. v0.3 완료 기준(§5)을 라이브러리 API로 자동 검증한다. 라이브 LLM 없이 **StubBackend**(결정적, 고정 텍스트 반환)로 검증한다. 테스트 전용 파일 `tests/smoke_v3.rs`.

## Changed files

- `tests/smoke_v3.rs` - 신규. v0.3 기준 단언 + StubBackend 헬퍼.

## Change description

`PersonaRuntime`을 구현하는 테스트용 StubBackend를 둔다(rng 미소비, 고정 Some 반환). 두 종류: 평범한 텍스트 stub, 특정 페르소나 이름을 호명하는 stub.

기준(고정 seed, 부등호/존재 단언):

1. **기본 결정성(Fake)**: FakeBackend로 같은 seed 2회 → records 동일. 그리고 θ=0.65에서 friend>chaos>summarizer(μ→빈도, v0.2 동작 유지).
2. **content 배선**: StubBackend로 돌리면 gate_passed인 모든 record의 utterance가 Some(stub 텍스트). FakeBackend면 utterance가 None.
3. **백엔드별 결정성**: StubBackend로 같은 seed 2회 → records 동일.
4. **content가 결정에 영향(내용 RRF)**: 항상 "friend"를 포함한 텍스트를 반환하는 StubBackend로 돌리면, FakeBackend(내용 없음) 대비 friend의 발화 수가 더 많다(interest 신호가 호명된 후보를 띄움). content가 화자 선택을 실제로 바꾼다는 증거.

driver::run은 `runtime: &mut dyn PersonaRuntime`를 받으므로 StubBackend를 넘기면 된다.

## Dependencies

- task-15(PersonaRuntime/Event.content/record.utterance/driver 주입), task-18(content RRF). preset/model 등.

## Verification

```bash
cargo test --test smoke_v3
cargo test
```

- `cargo test --test smoke_v3` (4개 이상): 위 기준 전부 통과. **v0.3 완료 게이트.**
- `cargo test` 전체 green(v0.1/v0.2 스모크 + v0.3 스모크 + 나머지 유지).

## Risks

| 위험 | 회피 |
|---|---|
| 기준 4가 seed/구현에 민감 | 충분한 ticks + 부등호(friend 발화 수 stub > fake). 호명 대비를 확실히 |
| StubBackend가 rng 소비해 결정성 오염 | stub.generate는 rng 미사용, 고정 반환 |
| 기준 2가 침묵 record까지 utterance 요구 | gate_passed(발화)인 record만 utterance Some 단언. 침묵은 None |
