//! 한국어 형태소 토크나이저 (task-43, friend-engine feature).
//!
//! seCall `crates/secall-core/src/search/tokenizer.rs`의 `LinderaKoTokenizer` 경로만
//! lift했다. Kiwi / `create_tokenizer` factory / seCall `Tokenizer` trait는 미포함.
//!
//! 공개 인터페이스: `morphological_tokens(text) -> Vec<String>`.
//! OnceLock으로 1회 초기화(임베딩 사전). 오류 시 whitespace fallback.

#![cfg(feature = "morphology")]

use std::collections::HashSet;
use std::sync::OnceLock;

use lindera::{
    dictionary::{load_embedded_dictionary, DictionaryKind},
    mode::Mode,
    segmenter::Segmenter,
    token_filter::{korean_keep_tags::KoreanKeepTagsTokenFilter, BoxTokenFilter},
    tokenizer::Tokenizer as LinderaInner,
};

// ─── LinderaKoTokenizer ───────────────────────────────────────────────────────

struct LinderaKoTokenizer {
    inner: LinderaInner,
}

impl LinderaKoTokenizer {
    fn new() -> Result<Self, String> {
        let dictionary = load_embedded_dictionary(DictionaryKind::KoDic)
            .map_err(|e| format!("lindera ko-dic load failed: {e}"))?;
        let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
        let mut tokenizer = LinderaInner::new(segmenter);

        // keep-tags: NNG(일반명사) NNP(고유명사) NNB(의존명사) VV(동사) VA(형용사) SL(외국어)
        let tags: HashSet<String> = ["NNG", "NNP", "NNB", "VV", "VA", "SL"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let keep_filter = KoreanKeepTagsTokenFilter::new(tags);
        tokenizer.append_token_filter(BoxTokenFilter::from(keep_filter));

        Ok(Self { inner: tokenizer })
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        let tokens = match self.inner.tokenize(text) {
            Ok(t) => t,
            Err(_) => return tokenize_fallback(text),
        };

        let mut result: Vec<String> = Vec::new();
        for token in tokens {
            let surface = token.surface.to_lowercase();
            if surface.chars().count() > 1 {
                result.push(surface);
            }
        }

        if result.is_empty() {
            tokenize_fallback(text)
        } else {
            result
        }
    }
}

// ─── 전역 OnceLock ────────────────────────────────────────────────────────────

/// `None` = init 실패(eprintln 1회 경고 후 fallback 경로 사용).
static TOKENIZER: OnceLock<Option<LinderaKoTokenizer>> = OnceLock::new();

fn get_tokenizer() -> Option<&'static LinderaKoTokenizer> {
    TOKENIZER
        .get_or_init(|| match LinderaKoTokenizer::new() {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!(
                    "[tunaSalon] friend-engine: lindera ko-dic init failed — \
                     falling back to whitespace tokenizer. error: {e}"
                );
                None
            }
        })
        .as_ref()
}

// ─── 공개 API ─────────────────────────────────────────────────────────────────

/// 텍스트를 한국어 형태소 토큰 리스트로 변환한다.
///
/// keep-tags: NNG/NNP/NNB/VV/VA/SL, surface 소문자, 1글자 토큰 제외.
/// lindera init 실패 / 토큰 오류 / 빈 결과 시 whitespace fallback.
pub fn morphological_tokens(text: &str) -> Vec<String> {
    match get_tokenizer() {
        Some(tok) => tok.tokenize(text),
        None => tokenize_fallback(text),
    }
}

// ─── Fallback ─────────────────────────────────────────────────────────────────

/// seCall `tokenize_fallback`과 동형: 공백+ASCII 구두점 분리, 소문자, 1글자 이하 제외.
fn tokenize_fallback(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .filter(|s| s.chars().count() > 1)
        .collect()
}

// ─── 단위 테스트 ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// 한국어 형태소 분해: 조사 분리 확인.
    /// "아키텍처를 설계한다" → "아키텍처"(NNG?) 또는 "설계"(VV?) 포함.
    /// 사전 의존이므로 느슨한 단언(포함 여부만).
    #[test]
    fn korean_morphology_contains_meaningful_tokens() {
        let tokens = morphological_tokens("아키텍처를 설계한다");
        assert!(!tokens.is_empty(), "형태소 결과가 비어 있으면 안 된다");
        let joined = tokens.join(" ");
        // "아키텍처" 또는 "설계" 중 하나 이상 포함해야 한다
        assert!(
            joined.contains("아키텍처") || joined.contains("설계"),
            "조사 분리 실패 — 의미 토큰이 없다. 실제: {joined:?}"
        );
    }

    /// 빈 입력 → 빈 결과, 패닉 없음.
    #[test]
    fn empty_input_returns_empty_no_panic() {
        let tokens = morphological_tokens("");
        assert!(tokens.is_empty(), "빈 입력은 빈 결과여야 한다");
    }

    /// 특수문자만 → 빈 결과 또는 fallback 결과, 패닉 없음.
    #[test]
    fn special_chars_only_no_panic() {
        let tokens = morphological_tokens("!@#$%^&*()");
        // 패닉 없이 반환만 되면 통과
        let _ = tokens;
    }

    /// fallback 직접 테스트: 공백 분리 + 1글자 제외.
    #[test]
    fn tokenize_fallback_splits_and_filters() {
        let tokens = tokenize_fallback("hello world ab");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        // "ab"는 2글자이므로 포함
        assert!(tokens.contains(&"ab".to_string()));
    }

    /// fallback: 구두점 분리 확인.
    #[test]
    fn tokenize_fallback_strips_punctuation_boundaries() {
        let tokens = tokenize_fallback("hello,world");
        // 쉼표가 분리자이므로 "hello"와 "world"가 각각 나와야 한다
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
    }
}
