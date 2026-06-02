---
title: "Salon v0.3 Task 17: 페르소나 system prompt"
type: plan-task
status: todo
updated_at: 2026-06-02
parent_plan: salon-engine-v3.md
task_id: "17"
depends_on: ["16"]
parallel_group: ""
---

# Task 17 - 페르소나 system prompt 주입

plan `salon-engine-v3.md` subtask 17. LLM이 페르소나 캐릭터로 말하도록 화자별 system prompt를 주입한다. v0.3은 데모 3인(friend/chaos/summarizer)의 **역할 기반 짧은 프롬프트**로 시작한다(40조각 on-demand 조립은 v0.x 비목표, `docs/temp/salon-persona-fragments.md` 참조).

## Changed files

- `src/ollama.rs` - 수정. OllamaBackend가 화자별 system prompt를 `/api/generate` body의 `system` 필드로 넣는다.
- `src/main.rs` - 수정. 데모 persona system prompt 맵을 만들어 OllamaBackend에 주입.

## Change description

- `OllamaBackend`에 `system_prompts: BTreeMap<PersonaId, String>` 필드 추가. `new(model, endpoint, api_key, system_prompts, timeout)`.
- `build_request_body(model, prompt, system: Option<&str>) -> Value`: system이 Some이면 body에 `"system": <prompt>` 추가. None이면 생략.
- `generate`: `let system = self.system_prompts.get(speaker).map(String::as_str);` → body에 포함. user prompt는 task-16의 최근 history 기반 유지("Recent lines... Reply with ONE short, in-character line.").
- 데모 프롬프트(main, `docs/temp/salon-persona-fragments.md` §6 역할 톤):
  - friend: 따뜻하고 편한 단골. 가볍게 1~2문장, 감정·분위기에 반응.
  - chaos: 장난스러운 분위기 메이커. 1문장, 살짝 엉뚱하게 던지고 빠짐.
  - summarizer: 조용한 정리자. 흐름이 쌓였을 때만 1문장으로 요약하듯.
  - 공통 제약: 실제 상담가처럼 굴지 말 것, 과한 사과/칭찬 금지, 앞사람 말 반복 금지, 짧게.
- 결정성/골든: 기본 FakeBackend(프롬프트 안 씀)라 불변. OllamaBackend만 영향.
- 가드레일: `unwrap`/`panic` 금지. 보안(키 처리)은 task-16 그대로 유지.

## Dependencies

- task-16(OllamaBackend).

## Verification

```bash
cargo test
cargo run -- --headless --seed 42 --ticks 80 --theta 0.65   # 기본 골든 동일
```

- `cargo test` (신규 단위 테스트): (1) `build_request_body(..., Some("you are X"))`가 body에 `system` 필드 포함, None이면 미포함, (2) system_prompts 맵 조회가 화자별 프롬프트 반환.
- `cargo test` 전체 green(기본 FakeBackend라 58 유지 + 신규).
- **골든 5종 바이트 동일**(기본 경로 불변).
- (수동) 로컬 ollama + gemma4:e4b로 `--llm` 시 페르소나별 톤 차이 확인.

## Risks

| 위험 | 회피 |
|---|---|
| Persona 리터럴 churn | system prompt를 Persona에 안 넣고 사이드 맵(PersonaId→String). 데모는 main에서 구성 |
| 골든 회귀 | 기본 FakeBackend 불변. OllamaBackend만 system 추가. 골든 재확인 |
| 프롬프트가 길어 작은 모델이 뭉갬 | 짧고 선명하게(원문 §작은 모델 주의). 한두 줄 |
| 보안 회귀 | task-16의 키 처리/Debug redacted 유지, 변경 없음 |
