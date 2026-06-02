---
title: "Salon v0.4 Task 24: 폴백 체인 (백엔드 실패 시 전환)"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "24"
depends_on: ["23"]
parallel_group: ""
---

# Task 24 - 폴백 체인 (백엔드 실패 시 전환)

plan `salon-engine-v4.md` subtask 24. 한 백엔드가 실패(다운/타임아웃/거부)하면 **폴백 백엔드로 전환**한다(예: friend 서버 다운 → cloud). 라이브 경로와 배치 둘 다. panic 금지, 골든 보존. 검증은 오프라인 백엔드(연결 거부)로 네트워크 없이.

> **스코프 조정(2026-06-02)**: 원안의 outcome 분류(Rejected/Timeout/Failed)·백오프·unhealthy 상태머신은 **보류**한다. 근거: (1) 백엔드별 세마포어가 cap(cloud 3/friend 1)을 서버 한도에 맞춰 이미 과포화를 막아 거부가 드물다, (2) 120s 타임아웃이라 같은 백엔드 재시도는 지연을 2배로 키운다(차라리 다른 백엔드로), (3) `generate_batch`가 `&self`라 unhealthy 카운터는 interior mutability가 필요해 비용 대비 가치가 낮다. → 이번엔 **실패(None) 시 폴백 백엔드로 전환**만. 분류·백오프·unhealthy는 측정 후 v0.4.x에서 필요하면.

## Changed files

- `src/pool.rs` - 수정. `fallbacks: BTreeMap<String, String>`(백엔드 이름 → 폴백 백엔드 이름) + `set_fallback(name, fallback)`. `fallback_chain(&self, speaker) -> Vec<String>`(순수, 네트워크 없이 테스트). `generate`(라이브)·`generate_batch`가 체인을 순서대로 시도해 첫 Some 반환.

## Change description

- `fallback_chain(&self, speaker: &str) -> Vec<String>`: `[resolve(speaker)]`에 그 백엔드의 폴백을 이어붙인다. **단일 레벨 + 사이클/중복 제거**(폴백의 폴백까지는 따라가되 이미 방문한 이름은 제외, 무한 루프 금지). resolve None이면 빈 Vec.
- `PersonaRuntime::generate`(라이브, &mut self): `fallback_chain` 순서로 각 백엔드 `Backend::generate` 시도 → 첫 Some 반환, 모두 실패면 None(내용 없는 발화, 엔진 결정 유지). rng 미소비.
- `generate_batch`(&self): 각 job의 closure가 체인을 순회 — 각 후보 백엔드의 세마포어 acquire 후 generate, 첫 Some에서 멈춤(permit은 각 시도마다 RAII drop). 입력 순서 보존.
- 폴백은 **명시적**: 폴백이 실제로 쓰이면 stderr 한 줄(키 제외) 또는 무음(선택, silent fallback 최소 원칙은 "조용히 잘못된 값" 금지 의미 — 여기선 폴백이 의도된 동작이라 OK). 키 비노출(INV-6).
- 운영: 현재 두 백엔드 모두 가동이라 폴백은 장애 대비. 라우팅 예) anchor=friend(폴백 cloud), 나머지 cloud(폴백 없음 또는 friend).
- 가드레일: panic/unwrap 금지. 무한 폴백 루프 금지(방문 set으로 차단). 결정성·골든 불변.

## Dependencies

- task-23(generate_batch/세마포어, Backend enum).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test` green. 단위 테스트(네트워크 없이): (1) `fallback_chain`이 [primary, fallback] 순서 반환, (2) 폴백 미설정이면 [primary]만, (3) 사이클(a→b→a) 시 방문 중복 제거로 유한, (4) generate_batch에서 오프라인 primary+오프라인 fallback → None, 입력 순서 보존, panic 없음.
- **골든 5종 바이트 동일**(라이브 결정 경로·FakeBackend 불변).

## Risks

| 위험 | 회피 |
|---|---|
| 폴백 무한 루프 | 방문 set으로 사이클 차단, 단일/유한 체인. fallback_chain 단위 테스트 |
| 폴백이 결정성/골든 오염 | 폴백은 라이브 생성 경로만. 엔진 결정·rng 불변. 골든 재확인 |
| 폴백 백엔드도 느려 지연 가중 | 체인은 짧게(보통 1단계). 각 백엔드 타임아웃이 상한 |
| 키가 폴백 로그에 샘 | 폴백 로그는 백엔드 이름만(키·endpoint 민감정보 주의). INV-6 |
| 보류한 분류/unhealthy가 나중에 필요 | 측정 후 v0.4.x. 현재 세마포어가 과포화 방지라 우선순위 낮음 |
