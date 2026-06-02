---
title: "Salon v0.5 Task 29: LiveSession - 논블로킹 생성 + 사람 입력 (라이브 세션 코어)"
type: plan-task
status: done
updated_at: 2026-06-02
parent_plan: salon-engine-v5.md
task_id: "29"
depends_on: ["28"]
parallel_group: ""
---

# Task 29 - LiveSession (라이브 세션 코어)

plan `salon-engine-v5.md` subtask 29. v0.5의 **최난점**. 배치 `driver::run`(N틱 돌고 종료)과 별개로, 실시간 채팅을 위한 **세션 코어**를 만든다. **터미널 I/O(crossterm)·ratatui는 task-30**; 이 task는 그 둘 없이 **네트워크 없이 테스트 가능한 로직**만: 엔진 틱 + HumanChannel 주입 + **논블로킹 LLM 생성**(워커 스레드 + mpsc). 핵심 불변식: ~1.6s 생성이 메인 흐름을 블록하지 않고, 생성은 한 번에 1개 in-flight(인과적 턴테이킹).

## Changed files

- `src/live.rs` - 신규. `LiveSession`.
- `src/pool.rs` - 수정. `pub fn generate_one(&self, speaker, history, tick) -> Option<String>`(&self, 폴백 체인 사용) 추가. `PersonaRuntime::generate(&mut self)`가 이를 위임(동작 불변). 워커 스레드가 `Arc<BackendPool>`로 off-thread 호출하기 위함.
- `src/lib.rs` - 수정. `pub mod live;`.

## Change description

- `pool.rs`: 기존 `PersonaRuntime::generate`(&mut self)는 실제로 &mut가 불필요(fallback_chain·Backend::generate 모두 &self) → `generate_one(&self, ...)`로 본문 이동, 트레이트는 위임. 이로써 `Arc<BackendPool>`를 워커 스레드에서 공유 호출 가능(Backend는 Send+Sync).
- `LiveSession`(src/live.rs):
  - 보유: `EngineConfig`, `Vec<Persona>`, `EngineState`, `ChaCha8Rng`(seed 주입), `HumanChannel`, `Arc<BackendPool>`, 생성 워커(job_tx, result_rx, JoinHandle), `pending: Option<PersonaId>`(in-flight 1개), tick 카운터.
  - `new(config, personas, seed, pool: Arc<BackendPool>, human_speaker_id)`: 워커 스레드 spawn — `loop { recv (idx, speaker, history, tick); let text = pool.generate_one(...); send (idx, text) }`. job_tx drop 시 워커 종료.
  - `submit_human(&mut self, text: String)`: `HumanChannel::speak(&mut state, &personas, text, ts)` 즉시 호출(pending과 무관 — 사람은 인터럽트). 사람 발화로 전 페르소나 λ 자극 → 다음 선택이 사람에게 반응.
  - `tick(&mut self) -> TickOutcome`: 엔진 1틱 전진(update_intensities + decay_excitations + combined + gate + rrf select, driver와 동일 로직·rng 순서). **pending이 없고** 화자가 선택되면: (1) 엔진측 speak 갱신 즉시 적용(suppress chosen, apply_excitation_on_speak, last_speaker, history에 placeholder Event content=None push), (2) 생성 job을 워커로 디스패치, pending=Some(chosen). pending이 있으면 새 디스패치 안 함(인과). 반환: Silence / SpeakingDispatched(speaker) / Pending 등.
  - `poll_generation(&mut self) -> Option<Event>`: `result_rx.try_recv()` 논블로킹. 결과 도착 시 해당 placeholder Event의 content를 채우고(history 갱신), pending=None, 그 Event 반환(렌더용). 없으면 None.
  - 모든 메서드는 **즉시 반환**(블록 없음). 생성은 워커에서.
- 결정성: 엔진 선택(누가/언제)은 seed로 결정적(rng 순서 보존). 라이브 전체는 실시간 타이밍·LLM이라 비결정 — opt-in, headless 골든 불침투.
- 가드레일: unwrap/panic 금지(채널 send/recv 실패는 graceful). 워커 종료 깔끔히(Drop). 키 비노출.

## Dependencies

- task-28(HumanChannel), v0.4 풀(generate 경로, Backend Send+Sync).

## Verification

```bash
cargo build
cargo test
cargo build && cargo run -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo OK
```

- `cargo test` green. 신규 단위 테스트(`live::tests`, **오프라인 백엔드 127.0.0.1:1로 네트워크 없이**):
  - (1) `submit_human` 후 history 마지막 = 사람 Event, 전 페르소나 excitation 상승(HumanChannel 연동).
  - (2) `tick`이 화자 선택 시 pending 설정 + placeholder Event push. pending 중 추가 tick은 새 디스패치 안 함(in-flight 1개).
  - (3) `poll_generation`이 워커 결과를 받아 pending 해제(오프라인이라 content=None placeholder 채움), 입력 후 bounded 폴링으로 도달 확인(타이밍 견고: 오프라인은 즉시 None).
  - (4) 엔진 선택 결정성: 같은 seed·같은 호출 순서 → 같은 화자 선택 시퀀스(생성/타이밍 제외).
  - (5) Drop 시 워커 스레드 깔끔히 종료(panic·hang 없음).
- **골든 5종 바이트 동일**(LiveSession은 라이브 전용, `driver::run` 불변).
- `generate_one` 추가 후 기존 풀 테스트·라우팅·폴백 그대로 green(트레이트 위임 동작 불변).

## Risks

| 위험 | 회피 |
|---|---|
| 생성이 메인 블록 → 채팅 멈춤 | 워커 스레드 + mpsc, tick/poll/submit 모두 즉시 반환. 메인은 폴링만 |
| in-flight 다중으로 인과 붕괴 | pending Option으로 1개 강제. 완료(poll) 후에만 다음 선택 |
| 워커 종료 hang/panic | job_tx Drop → recv 종료, JoinHandle. send/recv 실패 graceful(채널 닫힘 = 종료) |
| 엔진 결정성/골든 오염 | tick의 엔진 로직은 driver와 동일 rng 순서. LiveSession은 별도, headless 불변. 골든 재확인 |
| placeholder content=None이 잔류(생성 실패) | poll에서 None이면 placeholder content None 유지(내용 없는 발화 = 엔진 결정 유지), pending 해제 |
| generate_one 추가가 기존 동작 변경 | 트레이트 generate가 위임만 — 동작 동일. 기존 풀 테스트로 회귀 확인 |
