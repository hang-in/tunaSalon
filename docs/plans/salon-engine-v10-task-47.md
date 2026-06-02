---
title: "Salon v0.10 Task 47: 임베더 trait + MockEmbedder + ORT BGE-M3 + model_manager (Stage 2a)"
type: plan-task
status: done
updated_at: 2026-06-03
parent_plan: salon-engine-v10.md
task_id: "47"
depends_on: ["46"]
parallel_group: ""
---

# Task 47 - 임베더 (Stage 2a)

plan `salon-engine-v10.md` Stage 2a. 의미 회상의 첫 벽돌: 발화→벡터 임베더. **recall에 아직 배선하지 않는다**(48/49) → embed 모듈은 독립, 골든·기본빌드에 trivially 무영향.

**구조(중요): 안전부 먼저 단단히, 위험부는 lift+측정+폴백 보고.**
- 안전부(반드시 green): `friend-engine-semantic` feature + `Embedder` trait + `MockEmbedder`(결정적) + 단위 테스트. **새 heavy dep(ort 등) 없이 빌드/테스트 통과**.
- 위험부(lift+보고): `OrtEmbedder`(BGE-M3 ONNX) + `model_manager`(다운로드). 실모델 테스트는 `#[ignore]`. ort/ONNX-runtime/다운로드가 막히면 **안전부는 유지하고 ORT 블로커를 명확히 보고**(빌드를 깨지 말 것).

lift 원본(같은 머신, 정독): `/Users/d9ng/privateProject/seCall/crates/secall-core/src/search/embedding.rs`(`OrtEmbedder`) + `model_manager.rs`(다운로드). seCall은 async지만 **tunaSalon은 sync**(tokio 없음, reqwest blocking 이미 사용).

## Changed files

- `Cargo.toml` - 수정. `[features]`에 `friend-engine-semantic = ["friend-engine", "dep:ort", "dep:ndarray", "dep:tokenizers"]` + `coreml = ["ort/coreml"]`. optional deps: `ort = { version = "=2.0.0-rc.10", features = ["load-dynamic", "ndarray"], optional = true }`, `ndarray = { version = "0.16", optional = true }`, `tokenizers = { version = "0.21", default-features = false, features = ["fancy-regex"], optional = true }`. (reqwest blocking 이미 있음 — 다운로드에 재사용.)
- `src/embed.rs` - **신규, `#![cfg(feature = "friend-engine-semantic")]`**:
  - `pub trait Embedder { fn embed(&self, text: &str) -> Result<Vec<f32>, String>; fn dim(&self) -> usize; }` (**sync**).
  - `pub struct MockEmbedder { dim }` - 결정적: 텍스트를 토큰화(공백/소문자)해 각 토큰을 dim 버킷에 해시 누적 후 L2 정규화(= 결정적 bag-of-words 벡터, 같은 텍스트→같은 벡터, 토큰 겹치면 코사인↑). 테스트 + ORT 미가용 시 폴백용.
  - `pub struct OrtEmbedder` - seCall `OrtEmbedder` lift(sync로): `Session::builder()` + CoreML EP(`#[cfg(all(feature="coreml", target_os="macos", target_arch="aarch64"))]`) + `tokenizers::Tokenizer`(tokenizer.json) + `session.run(ort::inputs![...])` + 출력키 `token_embeddings`→`last_hidden_state` + mean pool(attention mask) + L2 norm. dim 1024. **세션 풀 불필요**(단일 Session; recall은 단일 스레드). `new(model_dir) -> Result<Self,String>`(lazy 로드).
  - `mod model_manager`(또는 같은 파일): seCall lift. HF BAAI/bge-m3 `onnx/model.onnx` + `onnx/model.onnx_data` + `tokenizer.json` 다운로드(reqwest blocking) → `default_model_path()`. **default_model_path = `$HOME/.cache/tunaSalon/models/bge-m3/`**, 단 **seCall 캐시(`$HOME/.cache/secall/models/bge-m3-onnx/`)에 model.onnx+tokenizer.json이 있으면 그걸 우선 사용**(1.2GB 재다운로드 회피). SHA256은 선택(생략 가능, v1).
- `src/lib.rs` - `#[cfg(feature = "friend-engine-semantic")] mod embed;`(공개 불필요, 48/49에서 사용).

## Change description

- **sync 변환**: seCall의 `async fn embed`/`embed_batch`/`spawn_blocking`/세션풀을 버리고 `fn embed(&self,&str)->Result<Vec<f32>,String>` 단일 동기 호출. ORT 추론은 원래 blocking이라 tokio 불요.
- **ORT load-dynamic 주의**: `ort` `load-dynamic`은 런타임에 libonnxruntime이 필요. seCall이 같은 맥에서 동작하므로 환경에 있을 것 — 빌드는 link 없이 통과하나 실행은 lib 필요. 실모델 테스트(`#[ignore]`)가 lib 가용성을 드러냄. 없으면 보고(블로커).
- **측정(필수)**: `#[ignore]` 테스트 또는 example로 **OrtEmbedder 로드 시간 + 첫 embed 시간 + 대략 RAM**을 stderr로 출력(사용자 맥북 랙 판단용 — 로컬 ollama 금지 맥락). 보고에 수치 포함.
- **골든·기본빌드**: embed.rs는 recall에 미배선 → driver/headless/memory 불변. 기본·`friend-engine` 빌드는 ort 미컴파일(optional+feature). 골든 무관(이 task는 검증만으로 충분, 회귀 없음).
- 가드: 요청 파일만. unwrap/panic 금지(embed는 Result, 로드 실패는 에러 반환). MockEmbedder는 순수.

## Verification

```bash
# 1) 안전부: 기본 빌드/테스트 그대로 + semantic feature 빌드/단위테스트(mock)
cargo build
cargo test
cargo build --features friend-engine-semantic
cargo test  --features friend-engine-semantic        # MockEmbedder 결정성 등(실모델 #[ignore] 제외)
# 2) 골든 5종(기본 빌드) 무변 — 회귀 없음 확인
cargo run -q -- --headless --seed 42 --ticks 80 --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
# 3) (위험부, 수동) 실모델 — 모델 있으면 다운로드/로드/측정
cargo test --features "friend-engine-semantic coreml" -- --ignored ort_embed   # 실 BGE-M3, 모델 다운로드~1.2GB
```

- 안전부 green: 기본/semantic 빌드 OK, `cargo test --features friend-engine-semantic` green(mock 결정성), 골든 무변, 기본 빌드에 ort 미컴파일.
- 단위 테스트(mock): 같은 텍스트→같은 벡터(결정성), 토큰 겹치는 텍스트 코사인 > 안 겹치는 것, dim 일치, L2 norm≈1.
- 위험부(보고): `#[ignore]` 실모델 테스트가 (a) 모델 다운로드/로드 성공 + embed 1024d 반환, (b) **로드/embed 시간·RAM 수치 출력**. ort/runtime/다운로드 블로커면 명확히 보고(안전부는 유지).

## Risks

| 위험 | 회피 |
|---|---|
| ort rc API 까다로움 | seCall `embedding.rs` OrtEmbedder를 거의 그대로 lift(검증된 코드). sync만 조정 |
| ort load-dynamic libonnxruntime 부재 | seCall이 같은 맥서 동작 → 있을 것. 빌드는 통과. 실행 못 하면 `#[ignore]` 테스트가 드러냄 → 보고(안전부 무관) |
| 1.2GB 모델 다운로드 | seCall 캐시 우선 재사용(재다운로드 회피). 다운로드는 `#[ignore]` 수동 |
| 맥북 랙(로드) | 측정 필수(로드+embed+RAM). 과하면 v0.10에서 원격 임베딩 폴백 재검토 |
| 기본 빌드에 ML 유입 | optional dep + `friend-engine-semantic` feature 뒤. 기본·`friend-engine`은 ort 미컴파일 |
| recall 오염/골든 | 이 task는 embed 모듈만(미배선). memory/driver/headless 불변 |
