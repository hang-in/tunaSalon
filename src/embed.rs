#![cfg(feature = "friend-engine-semantic")]

//! Embedder trait + MockEmbedder (deterministic) + OrtEmbedder (BGE-M3 ONNX, sync).
//!
//! `friend-engine-semantic` feature gate: ort/ndarray/tokenizers only compile here.
//! Not wired into recall yet (task 48/49).

use std::path::{Path, PathBuf};

// ─── Embedder trait ───────────────────────────────────────────────────────────

/// Synchronous embedding interface.
/// `embed` is intentionally blocking — ORT inference is blocking, recall is
/// single-threaded, and tunaSalon has no async runtime.
pub trait Embedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;
    fn dim(&self) -> usize;
}

// ─── MockEmbedder ─────────────────────────────────────────────────────────────

/// Deterministic bag-of-words embedder for testing and fallback.
///
/// Algorithm: lowercase + whitespace-split tokens → each token hashed into one
/// of `dim` buckets (FNV-like hash mod dim) → bucket counter incremented →
/// L2-normalize.  Same text → same vector.  Texts sharing tokens have higher
/// cosine similarity.
pub struct MockEmbedder {
    dim: usize,
}

impl MockEmbedder {
    pub fn new(dim: usize) -> Self {
        assert!(dim > 0, "dim must be > 0");
        MockEmbedder { dim }
    }
}

impl Default for MockEmbedder {
    fn default() -> Self {
        MockEmbedder::new(1024)
    }
}

/// Cheap hash that maps a token string to a bucket index in `[0, dim)`.
/// Uses FNV-1a-style mixing for good distribution.
fn token_bucket(token: &str, dim: usize) -> usize {
    let mut h: u64 = 0xcbf29ce484222325; // FNV offset basis
    for byte in token.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3); // FNV prime
    }
    (h % dim as u64) as usize
}

impl Embedder for MockEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let mut vec = vec![0.0f32; self.dim];

        // tokenize: lowercase, split on whitespace, skip empty
        for token in text.to_lowercase().split_whitespace() {
            let bucket = token_bucket(token, self.dim);
            vec[bucket] += 1.0;
        }

        // L2 normalize
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-9 {
            for v in vec.iter_mut() {
                *v /= norm;
            }
        }
        // If text is entirely empty/whitespace, return zero vector (valid unit test case).

        Ok(vec)
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

// ─── model_manager ────────────────────────────────────────────────────────────

pub mod model_manager {
    use super::PathBuf;

    const MODEL_URL: &str =
        "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/model.onnx";
    const MODEL_DATA_URL: &str =
        "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/model.onnx_data";
    const TOKENIZER_URL: &str =
        "https://huggingface.co/BAAI/bge-m3/resolve/main/tokenizer.json";

    /// Preferred cache path: reuse seCall's cache if model files are already
    /// present (avoids a 1.2 GB re-download).  Otherwise return tunaSalon's
    /// own cache directory.
    pub fn default_model_path() -> PathBuf {
        let home = dirs_next();
        let secall = home
            .join(".cache")
            .join("secall")
            .join("models")
            .join("bge-m3-onnx");
        if secall.join("model.onnx").exists() && secall.join("tokenizer.json").exists() {
            return secall;
        }
        home.join(".cache")
            .join("tunaSalon")
            .join("models")
            .join("bge-m3")
    }

    fn dirs_next() -> PathBuf {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }

    /// Returns true when model.onnx + tokenizer.json are present in `dir`.
    pub fn is_downloaded(dir: &std::path::Path) -> bool {
        dir.join("model.onnx").exists() && dir.join("tokenizer.json").exists()
    }

    /// Download BGE-M3 ONNX files into `dir` using reqwest blocking.
    /// `model.onnx_data` is downloaded only if `model.onnx` is small
    /// (the "external data" variant of the ONNX); both files are attempted.
    ///
    /// Returns `Err` if any required file cannot be fetched.
    /// SHA256 verification is not performed in this version (v1).
    pub fn download(dir: &std::path::Path, force: bool) -> Result<(), String> {
        if is_downloaded(dir) && !force {
            return Ok(());
        }
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("create model dir: {e}"))?;

        download_file(MODEL_URL, &dir.join("model.onnx"))?;
        // model.onnx_data is the external-weights shard (~1.1 GB).
        // Best-effort: if it fails we still return Ok so the caller
        // can attempt loading and surface a more useful ORT error.
        let _ = download_file(MODEL_DATA_URL, &dir.join("model.onnx_data"));
        download_file(TOKENIZER_URL, &dir.join("tokenizer.json"))?;

        Ok(())
    }

    fn download_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
        let tmp = dest.with_extension("tmp");
        let resp = reqwest::blocking::get(url)
            .map_err(|e| format!("GET {url}: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("download failed ({}) for {url}", resp.status()));
        }
        let bytes = resp.bytes().map_err(|e| format!("read bytes: {e}"))?;
        std::fs::write(&tmp, &bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, dest)
            .map_err(|e| format!("rename to {}: {e}", dest.display()))?;
        Ok(())
    }
}

// ─── OrtEmbedder ─────────────────────────────────────────────────────────────

/// ONNX-runtime-based BGE-M3 embedder (sync, single session).
///
/// Requires `model.onnx` and `tokenizer.json` in `model_dir`.
/// CoreML execution provider is registered on macOS aarch64 when the
/// `coreml` feature is enabled.
///
/// `session` is wrapped in `RefCell` because `ort::Session::run` needs
/// `&mut self` but our `Embedder::embed` signature is `&self`.
/// tunaSalon recall is single-threaded so `RefCell` (not `Mutex`) is correct.
pub struct OrtEmbedder {
    session: std::cell::RefCell<ort::session::Session>,
    tokenizer: tokenizers::Tokenizer,
    dim: usize,
}

impl OrtEmbedder {
    pub fn new(model_dir: &Path) -> Result<Self, String> {
        use ort::session::builder::GraphOptimizationLevel;

        let tokenizer =
            tokenizers::Tokenizer::from_file(model_dir.join("tokenizer.json"))
                .map_err(|e| format!("tokenizer load: {e}"))?;

        #[allow(unused_mut)]
        let mut builder = ort::session::Session::builder()
            .map_err(|e| format!("ort builder: {e}"))?;

        #[cfg(all(feature = "coreml", target_os = "macos", target_arch = "aarch64"))]
        {
            use ort::execution_providers::CoreMLExecutionProvider;
            builder = builder
                .with_execution_providers([CoreMLExecutionProvider::default().build()])
                .map_err(|e| format!("coreml ep: {e}"))?;
        }

        let mut session = builder
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| format!("opt level: {e}"))?
            .commit_from_file(model_dir.join("model.onnx"))
            .map_err(|e| format!("load model: {e}"))?;

        let dim = probe_dim(&mut session, &tokenizer).unwrap_or(1024);

        Ok(OrtEmbedder {
            session: std::cell::RefCell::new(session),
            tokenizer,
            dim,
        })
    }
}

fn probe_dim(
    session: &mut ort::session::Session,
    tokenizer: &tokenizers::Tokenizer,
) -> Result<usize, String> {
    let v = run_inference(session, tokenizer, "test")?;
    Ok(v.len())
}

fn run_inference(
    session: &mut ort::session::Session,
    tokenizer: &tokenizers::Tokenizer,
    text: &str,
) -> Result<Vec<f32>, String> {
    use ndarray::Array2;
    use ort::value::TensorRef;

    let encoding = tokenizer
        .encode(text, true)
        .map_err(|e| format!("tokenize: {e}"))?;

    let ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
    let mask: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .map(|&x| x as i64)
        .collect();
    let seq_len = ids.len();

    let ids_arr = Array2::<i64>::from_shape_vec((1, seq_len), ids)
        .map_err(|e| format!("array reshape ids: {e}"))?;
    let mask_arr = Array2::<i64>::from_shape_vec((1, seq_len), mask.clone())
        .map_err(|e| format!("array reshape mask: {e}"))?;

    let ids_ref = TensorRef::<i64>::from_array_view(ids_arr.view())
        .map_err(|e| format!("tensor ids: {e}"))?;
    let mask_ref = TensorRef::<i64>::from_array_view(mask_arr.view())
        .map_err(|e| format!("tensor mask: {e}"))?;

    let outputs = session
        .run(ort::inputs![
            "input_ids" => ids_ref,
            "attention_mask" => mask_ref,
        ])
        .map_err(|e| format!("session.run: {e}"))?;

    // bge-m3 ONNX exports token-level embeddings as "token_embeddings";
    // fall back to "last_hidden_state" for standard BERT-style models.
    let out_key = if outputs.contains_key("token_embeddings") {
        "token_embeddings"
    } else {
        "last_hidden_state"
    };
    let hidden_arr = outputs[out_key]
        .try_extract_array::<f32>()
        .map_err(|e| format!("extract array: {e}"))?;
    let shape = hidden_arr.shape();
    let dim = shape[2];

    // mean pool weighted by attention mask
    let mask_float: Vec<f32> = mask_arr.iter().map(|&m| m as f32).collect();
    let mask_sum: f32 = mask_float.iter().sum::<f32>().max(1e-9);
    let mut embedding = vec![0.0f32; dim];
    for i in 0..seq_len {
        let m = mask_float[i];
        for d in 0..dim {
            embedding[d] += hidden_arr[[0, i, d]] * m;
        }
    }
    for e in embedding.iter_mut() {
        *e /= mask_sum;
    }

    // L2 normalize
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        for e in embedding.iter_mut() {
            *e /= norm;
        }
    }

    Ok(embedding)
}

impl Embedder for OrtEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        // `session.run` needs `&mut Session`; use RefCell for interior mutability.
        // tunaSalon recall is single-threaded so RefCell (not Mutex) is sufficient.
        let mut session = self
            .session
            .try_borrow_mut()
            .map_err(|_| "ort session already borrowed (concurrent embed call)".to_string())?;
        run_inference(&mut session, &self.tokenizer, text)
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    // ── MockEmbedder tests ────────────────────────────────────────────────────

    #[test]
    fn mock_determinism() {
        let e = MockEmbedder::default();
        let a = e.embed("hello world foo bar").unwrap();
        let b = e.embed("hello world foo bar").unwrap();
        assert_eq!(a, b, "same text must produce identical vectors");
    }

    #[test]
    fn mock_dim() {
        let e = MockEmbedder::new(256);
        let v = e.embed("some text").unwrap();
        assert_eq!(v.len(), 256, "output dim must match constructor");
        assert_eq!(e.dim(), 256);
    }

    #[test]
    fn mock_l2_norm() {
        let e = MockEmbedder::default();
        let v = e.embed("the quick brown fox jumps over the lazy dog").unwrap();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "L2 norm must be ~1.0, got {norm}"
        );
    }

    #[test]
    fn mock_overlap_higher_cosine() {
        let e = MockEmbedder::default();
        // a and b share "hello world"; c shares nothing with a
        let a = e.embed("hello world rust lang").unwrap();
        let b = e.embed("hello world python code").unwrap();
        let c = e.embed("elephant banana xylophone").unwrap();

        let sim_ab = cosine(&a, &b);
        let sim_ac = cosine(&a, &c);

        assert!(
            sim_ab > sim_ac,
            "overlapping text ({sim_ab:.4}) should score higher than non-overlapping ({sim_ac:.4})"
        );
    }

    #[test]
    fn mock_different_texts_differ() {
        let e = MockEmbedder::default();
        let a = e.embed("alpha beta gamma").unwrap();
        let b = e.embed("delta epsilon zeta").unwrap();
        // Vectors should differ (non-overlapping tokens → different buckets)
        assert_ne!(a, b, "different texts must produce different vectors");
    }

    // ── model_manager tests ───────────────────────────────────────────────────

    #[test]
    fn model_manager_not_downloaded_empty_dir() {
        let dir = std::env::temp_dir().join("salon_test_mm_empty");
        // Ensure it's empty / doesn't have our files
        let _ = std::fs::remove_file(dir.join("model.onnx"));
        let _ = std::fs::remove_file(dir.join("tokenizer.json"));
        assert!(!model_manager::is_downloaded(&dir));
    }

    #[test]
    fn default_model_path_contains_bge() {
        let p = model_manager::default_model_path();
        let s = p.to_str().unwrap();
        assert!(
            s.contains("bge-m3"),
            "default path should reference bge-m3, got: {s}"
        );
    }

    // ── OrtEmbedder ignore tests ──────────────────────────────────────────────

    /// Real model test: requires model files at default_model_path().
    /// Run manually: cargo test --features "friend-engine-semantic coreml" -- --ignored ort_embed
    #[test]
    #[ignore]
    fn ort_embed_basic() {
        let model_dir = model_manager::default_model_path();
        assert!(
            model_manager::is_downloaded(&model_dir),
            "model not found at {}; download first",
            model_dir.display()
        );

        let t0 = std::time::Instant::now();
        let embedder = OrtEmbedder::new(&model_dir).expect("OrtEmbedder::new");
        let load_ms = t0.elapsed().as_millis();

        let t1 = std::time::Instant::now();
        let v = embedder.embed("hello world 안녕하세요").expect("embed");
        let embed_ms = t1.elapsed().as_millis();

        eprintln!(
            "\n[ort_embed_basic] load={load_ms}ms  first_embed={embed_ms}ms  dim={}",
            v.len()
        );

        assert_eq!(v.len(), 1024, "bge-m3 dim must be 1024");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "L2 norm must be ~1.0, got {norm}"
        );
    }
}
