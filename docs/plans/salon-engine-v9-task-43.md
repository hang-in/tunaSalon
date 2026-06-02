---
title: "Salon v0.9 Task 43: friend-engine feature scaffold + Lindera 한국어 형태소 토크나이저 (Stage 0)"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v9.md
task_id: "43"
depends_on: ["39"]
parallel_group: ""
status_note: "done 2026-06-03. 리뷰에서 redundant `unsafe impl Send/Sync` 제거(LinderaKoTokenizer는 auto Send+Sync, seCall과 동일 확인). 기본 211 / feature 215 tests green, 골든 5/5."
---

# Task 43 - feature scaffold + Lindera 형태소 (Stage 0)

plan `salon-engine-v9.md` Stage 0. 회상 토큰화를 토큰중복(공백분리 근사)에서 **한국어 형태소(Lindera ko-dic)**로 끌어올린다. 전 검색코어가 들어갈 `friend-engine` feature flag를 세우고(기본 off), 그 뒤에서만 형태소를 쓴다. **`flow.rs`는 손대지 않는다**(FlowMeter와 골든 보존). lift 원본: `/Users/d9ng/privateProject/seCall/crates/secall-core/src/search/tokenizer.rs`(`LinderaKoTokenizer` + keep-tags + `tokenize_fallback`. Kiwi/factory는 가져오지 않음).

## Changed files

- `Cargo.toml` - 수정. `[features] friend-engine = ["dep:lindera"]`(기본 off). `lindera = { version = "2.3.4", features = ["embed-ko-dic"], optional = true }`.
- `src/tokenize_ko.rs` - **신규, `#[cfg(feature = "friend-engine")]`**. seCall `LinderaKoTokenizer`(Lindera 경로만) lift. 공개: `pub fn morphological_tokens(text: &str) -> Vec<String>`. 내부에 `OnceLock<LinderaKoTokenizer>`로 1회 초기화(임베디드 사전, Send+Sync). 토큰 오류/빈 결과/init 실패 시 whitespace fallback(seCall `tokenize_fallback` 동형). keep-tags NNG/NNP/NNB/VV/VA/SL, surface 소문자, 1글자 토큰 제외.
- `src/lib.rs` - 수정. `#[cfg(feature = "friend-engine")] mod tokenize_ko;` 추가(공개 불필요, memory에서만 사용).
- `src/memory.rs` - 수정. `recall`이 쓰는 토큰화를 헬퍼로 분리:
  ```rust
  fn recall_tokens(s: &str) -> std::collections::BTreeSet<String> {
      #[cfg(feature = "friend-engine")]
      { crate::tokenize_ko::morphological_tokens(s).into_iter().collect() }
      #[cfg(not(feature = "friend-engine"))]
      { crate::flow::tokenize(s) }
  }
  ```
  `recall` 내부의 `tokenize(query)`/`tokenize(&ev.content)` 호출만 `recall_tokens(...)`로 교체. **점수 계산(intersection count)·참여 격리·정렬·시그니처는 불변.**
- `tests/recall_eval.rs` - (필요 시) feature on/off 양쪽에서 green 유지. 형태소 효과 케이스 추가는 아래 품질 게이트 참조.

## Change description

- **flow.rs 불변**: `flow::tokenize`는 FlowMeter `measure`와 memory가 공유한다. 건드리면 FlowMeter 거동/테스트가 바뀐다. Stage 0은 memory에만 별도 형태소 경로를 준다. feature off면 memory는 v0.8과 **완전히 동일**(`flow::tokenize`).
- **결정성**: Lindera + 임베디드 ko-dic은 rng·네트워크·벽시계 없음 → 같은 입력 같은 토큰. 회상 결정성 유지(plan INV-3).
- **silent fallback 최소화**(CLAUDE.md): 임베디드 사전이라 init 실패는 사실상 없음. 만약 실패하면 `eprintln!`로 1회 경고 후 whitespace fallback. 토큰 단위 빈결과 fallback은 seCall과 동일(정상 동작).
- **골든**: 회상은 라이브 경로 전용(v0.8). driver/headless/`PersonaRuntime`는 recall 미주입 → feature 유무와 무관하게 골든 바이트 동일. feature는 driver를 컴파일/실행상으로 건드리지 않는다.
- 가드레일: 요청 파일만 수정, flow.rs/driver.rs/pool.rs/live.rs 등 미변경. `recall` 공개 시그니처 고정. unwrap/panic 금지(OnceLock init은 내부 처리).

## Dependencies

- task-39(`MemoryStore`/`recall`/`flow::tokenize`). v0.8 골든·테스트.
- lift 원본 seCall tokenizer.rs(같은 머신). lindera 2.3.4 + embed-ko-dic API는 seCall에서 검증됨.

## Verification

```bash
# 1) 기본 빌드/테스트 (lindera 미컴파일)
cargo build
cargo test

# 2) feature 빌드/테스트 (lindera 컴파일 + 형태소 경로)
cargo build --features friend-engine
cargo test  --features friend-engine

# 3) 골든 5종 바이트 동일 (기본 빌드; cargo build 후 명시적 순차)
cargo build
cargo run -- --headless --seed 42 --ticks 120 --theta 0.40 | diff - /tmp/salon_golden/s42_t040.ndjson && echo s42_t040 OK
cargo run -- --headless --seed 42 --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
cargo run -- --headless --seed 42 --ticks 120 --theta 0.78 | diff - /tmp/salon_golden/s42_t078.ndjson && echo s42_t078 OK
cargo run -- --headless --seed 7   --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s7_t065.ndjson  && echo s7_t065 OK
cargo run -- --headless --seed 99  --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s99_t065.ndjson && echo s99_t065 OK
```

- 기본/​feature 양쪽 `cargo test` green. flow.rs 테스트(FlowMeter) 불변.
- 골든 5종 바이트 동일.
- 신규 단위 테스트(`tokenize_ko` + memory, feature-gated):
  - 한국어 형태소 분해(예: "아키텍처를 설계한다" → 의미 토큰 포함, 조사 분리).
  - 빈/특수문자 → 빈 결과(또는 fallback) panic 없음.
  - **품질 게이트**: 조사 분리 회상 케이스 - feature on일 때 query "비가 온다"가 content "비 온다 심심해"를 회상(형태소가 "비"/"오"를 매칭). 같은 케이스가 feature off(토큰중복)에선 miss될 수 있음을 대비 케이스로 명시(형태소 우위 증명). recall_eval은 양쪽 green.

## Risks

| 위험 | 회피 |
|---|---|
| flow.rs 변경으로 FlowMeter/골든 흔들림 | flow.rs 절대 미수정. memory만 별도 경로. feature off=v0.8 동일 |
| 기본 빌드가 무거워짐 | lindera는 `dep:lindera` optional, `friend-engine` feature 뒤. 기본 빌드 미컴파일 |
| OnceLock 동시성/Send+Sync | LinderaKoTokenizer는 Send+Sync(seCall 검증). OnceLock 1회 init |
| 형태소 출력이 dict 의존이라 테스트 취약 | 정확한 토큰 나열 대신 "특정 의미토큰 포함/조사 분리 매칭"으로 느슨히 단언 |
| init 실패 silent fallback | 임베디드라 사실상 없음. 실패 시 eprintln 1회(loud) + whitespace fallback |
