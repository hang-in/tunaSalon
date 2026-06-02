---
title: "Salon v0.4 Task 23: 병렬 배치 API + 백엔드별 세마포어"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v4.md
task_id: "23"
depends_on: ["22"]
parallel_group: ""
---

# Task 23 - 병렬 배치 API + 백엔드별 세마포어

plan `salon-engine-v4.md` subtask 23. 여러 페르소나를 **동시에** 호출하는 배치 API를 추가한다. `std::thread::scope` + **백엔드별 카운팅 세마포어**(동시 in-flight 상한: cloud 3, qwen 2)로. async/tokio 미사용. **결정성 오염 금지**: 이 경로는 bench/비교 전용으로 라이브 결정 경로에 절대 안 들어간다. 검증은 deterministic fake로 네트워크 없이.

## Changed files

- `src/pool.rs` - 수정. 백엔드별 `Semaphore`(아래 자체 구현) 보유. `pub fn generate_batch(&self, jobs: &[(PersonaId, &[Event])], tick) -> Vec<(PersonaId, Option<String>)>`: 각 job을 라우팅된 백엔드로 보내되 백엔드별 세마포어로 동시 상한. `std::thread::scope`로 스레드 생성, 각 스레드가 permit 획득 후 백엔드의 `&self` generate 호출.
- `src/semaphore.rs` - 신규(또는 pool.rs 내부 모듈). `Semaphore`(Mutex<usize> + Condvar) 최소 구현 + RAII permit guard. **크레이트 추가 없음**.
- `src/ollama.rs` - 수정. 스레드 공유용 `fn generate_shared(&self, speaker, history, tick) -> Option<String>`(상태 불변, rng 불필요). 기존 `&mut self` 트레이트 generate는 이를 호출하게 위임. `OllamaBackend`가 `Sync`(reqwest::blocking::Client는 Send+Sync+Clone).

## Change description

- 세마포어: `Semaphore::new(n)`, `acquire() -> Permit`(Condvar로 슬롯 대기), `Permit` drop 시 슬롯 반환. 백엔드별로 1개씩(cloud=3, qwen=2).
- `generate_batch`: `thread::scope`로 job마다 스레드. 스레드는 (1) 라우팅으로 백엔드·세마포어 선택, (2) `sem.acquire()`, (3) `backend.generate_shared(...)`, (4) permit drop. 결과를 입력 순서 보존해 수집(인덱스로).
  - **rng 미사용**: 배치는 엔진 rng를 안 건드린다(생성만, 결정 무관). 따라서 결정성 불변식 유지.
- `generate_shared(&self, ...)`: 현재 generate 본문에서 `&mut self`/rng 의존 제거(원래도 self 불변·rng 미소비). 트레이트 generate는 `self.generate_shared(...)`로 위임 → 라이브 경로 동작 불변.
- 운영 제약: 라이브 배치는 cloud only. cap 검증은 **fake 백엔드 + 인위적 지연(스레드 sleep)**으로 동시 in-flight 최대치를 측정(피크 카운터 ≤ cap 단언). 네트워크 없음.
- 가드레일: 데드락 금지(permit는 RAII drop). panic 금지. 배치 결과는 입력 순서 보존.

## Dependencies

- task-22(라우팅, BackendPool = PersonaRuntime).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 > /tmp/s42.ndjson
diff /tmp/s42.ndjson /tmp/salon_golden/s42_t065.ndjson   # 라이브 결정 경로 불변 → 동일
```

- `cargo test` green. 단위 테스트(fake, 네트워크 없이): (1) `generate_batch`가 N개 결과를 입력 순서로 반환, (2) **동시 in-flight 피크 ≤ max_concurrent**(fake가 진입/이탈 시 atomic 카운터 증감, 피크 기록; cap=3이면 피크 ≤3; 백엔드별 독립), (3) 세마포어 permit이 drop 후 반환돼 데드락 없음(배치 완료), (4) Semaphore 단위 테스트(acquire/release 카운트).
- **골든 보존**: 라이브 틱 루프(순차)는 변경 없음 → 바이트 동일.

## Risks

| 위험 | 회피 |
|---|---|
| 동시성이 엔진 결정성 오염 | 배치는 rng 미사용·라이브 결정 경로 불침투. golden 재확인. fake로 cap 테스트 |
| blocking Client 스레드 공유 안전성 | reqwest::blocking::Client는 Send+Sync+Clone. generate_shared는 &self(상태 불변). thread::scope로 'static 불요 |
| 세마포어 데드락/슬롯 누수 | Permit RAII drop으로 반환. 단위 테스트로 배치 완료(데드락 없음) 확인 |
| 자체 세마포어 버그 | Mutex+Condvar 표준 패턴. acquire/release 단위 테스트. 크레이트 추가 회피(공급망) |
| cap 초과 in-flight | atomic 피크 카운터로 ≤cap 단언. 백엔드별 독립 세마포어 |
