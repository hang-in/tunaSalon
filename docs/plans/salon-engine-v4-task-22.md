---
title: "Salon v0.4 Task 22: 페르소나별 라우팅 (BackendPool = PersonaRuntime)"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "22"
depends_on: ["21"]
parallel_group: ""
---

# Task 22 - 페르소나별 라우팅

plan `salon-engine-v4.md` subtask 22. `BackendPool`이 `PersonaRuntime`을 구현해 **driver 시그니처를 안 건드리고** 화자(persona)별로 다른 백엔드에 라우팅한다. 미지정 페르소나는 기본 백엔드로 폴백. 라이브 틱 루프는 **순차 유지**(발화 1명/틱). 단일 백엔드 구성이면 v0.3과 동일 동작.

## Changed files

- `src/pool.rs` - 수정. `BackendPool { backends: BTreeMap<String, OllamaBackend>, routing: BTreeMap<PersonaId, String>, default_backend: String }`. `impl PersonaRuntime for BackendPool`: `generate`가 `routing.get(speaker).unwrap_or(&default_backend)`로 백엔드 골라 그 백엔드의 generate 호출. 라우팅 키 없으면 기본으로 폴백.
- `src/main.rs` - 수정. `--llm` 경로에서 BackendPool 구성. 기본은 단일 백엔드(모든 페르소나 → 그 백엔드 = v0.3 동작). room config가 backends/routing 주면 다중. driver에는 `&mut pool as &mut dyn PersonaRuntime` 주입(driver 변경 없음).

## Change description

- `impl PersonaRuntime for BackendPool::generate(&mut self, speaker, history, tick, rng)`:
  - `let name = self.routing.get(speaker).unwrap_or(&self.default_backend);`
  - `let backend = self.backends.get_mut(name)?;` (없으면 default로, default도 없으면 None - panic 금지)
  - `backend.generate(speaker, history, tick, rng)` 반환.
  - rng 미소비(Ollama 백엔드들 모두 미소비) → 엔진 결정성 보존.
- driver는 그대로. main이 FakeBackend(기본, LLM off) 또는 BackendPool(`--llm`)을 dyn으로 주입.
- 운영 제약(plan §0): 라이브 백엔드는 **cloud only**. 지인서버 qwen는 config에 정의는 하되 라이브 보류(폴백 대상). 이 task의 라우팅 로직은 백엔드 종류와 무관하므로 fake/cloud로 검증 가능.
- 가드레일: 라우팅 누락·백엔드 부재 시 명시적 기본 폴백 또는 None. `unwrap` 금지. silent fallback 최소(기본 폴백은 의도된 동작이라 OK, 단 로그로 드러냄은 선택).

## Dependencies

- task-21(BackendConfig/BackendPool 골격, num_ctx Option).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 > /tmp/s42.ndjson
diff /tmp/s42.ndjson /tmp/salon_golden/s42_t065.ndjson   # 기본은 FakeBackend → 동일
```

- `cargo test` green. 단위 테스트(네트워크 없이, **fake/stub 백엔드로**): (1) routing에 있는 persona는 지정 백엔드로, (2) 없는 persona는 default로 폴백, (3) BackendPool을 dyn PersonaRuntime으로 driver에 주입 가능. fake 백엔드 2개(이름만 다름)로 어느 백엔드가 호출됐는지 카운터로 단언.
- **골든 보존**: 기본 실행(LLM off, FakeBackend)은 여전히 바이트 동일.
- (수동) `--llm`(cloud) 단일 백엔드면 v0.3처럼 utterance 채워짐.

## Risks

| 위험 | 회피 |
|---|---|
| 라우팅 누락 persona가 panic | default_backend 폴백, 그것도 없으면 None. unwrap 금지 |
| BackendPool generate가 rng 소비해 골든 깨짐 | 내부 백엔드 generate가 rng 미소비(v0.3 보장). pool도 미소비 |
| 단일 백엔드 경로 회귀 | 기본 routing 비움 + default 1개 = 모든 persona 동일 백엔드 = v0.3 동작. 골든 재확인 |
| 지인서버 라우팅이 라이브 의존 만듦 | qwen 라우팅 대상은 config로만. 라이브 cloud only. fake로 테스트(서버 가동 무관) |
