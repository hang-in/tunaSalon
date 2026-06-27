use crate::model::Event;
use std::collections::BTreeSet;

/// content 있는 최근 발화 최대 N개를 flow 계산에 사용한다.
///
/// driver(headless 골든 경로)와 live(채팅 사이드바)가 같은 윈도우를 쓰도록
/// 단일 정의한다. 값이 두 곳으로 갈리면 엔진 결정성/측정 윈도우가 분기한다.
pub const FLOW_WINDOW: usize = 6;

/// 대화 수렴/발산 측정 결과.
///
/// `convergence ∈ [0, 1]`:
/// - **1.0** = 최근 발화들이 서로 거의 같음(토큰 반복/수렴, 대화가 식고 있음).
/// - **0.0** = 모든 발화에 새 토큰(발산, 대화가 살아있음).
///
/// v0.6은 키워드 중복도 근사(BGE-M3 임베딩은 이후 단계에서 `measure` 인터페이스를
/// 유지한 채 내부만 교체 예정). `serde::Serialize`는 task-34 record 직렬화용.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct FlowMetric {
    pub convergence: f64,
}

/// 단일 발화 문자열을 토큰 집합으로 변환한다.
///
/// `morphology` feature ON: 한국어 형태소(어간/명사) 기반 토크나이저.
///   "날씨가"/"날씨를" 모두 "날씨"로 통일돼 한국어 수렴도 측정이 개선된다.
///   영어/비한글은 `morphological_tokens`가 SL 태그로 처리(fallback 포함).
///
/// `morphology` feature OFF(기본): 공백+ASCII 구두점 분리.
///   1. 소문자화.
///   2. 공백으로 분리.
///   3. 각 토큰의 양끝 ASCII 구두점(`.` `,` `!` `?` `'` `"` `(` `)` `:` `;`) 제거.
///   4. 빈 문자열이 된 토큰은 버린다.
pub(crate) fn tokenize(utterance: &str) -> BTreeSet<String> {
    #[cfg(feature = "morphology")]
    {
        return crate::tokenize_ko::morphological_tokens(utterance)
            .into_iter()
            .map(|t| t.to_lowercase())
            .collect();
    }
    #[cfg(not(feature = "morphology"))]
    {
        const STRIP: &[char] = &['.', ',', '!', '?', '\'', '"', '(', ')', ':', ';'];
        return utterance
            .to_lowercase()
            .split_whitespace()
            .filter_map(|tok| {
                let trimmed = tok.trim_matches(STRIP);
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();
    }
}

/// 두 토큰 집합의 Jaccard 유사도를 계산한다.
///
/// `|A ∩ B| / |A ∪ B|`. 합집합이 비어 있으면 `None`을 반환해 0-division을 방어한다.
fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> Option<f64> {
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        None
    } else {
        Some(intersection as f64 / union as f64)
    }
}

/// 최근 발화 슬라이스를 받아 수렴/발산 지표를 계산한다.
///
/// - 각 발화를 토큰화하고 빈 토큰셋은 제외한다.
/// - 유효 토큰셋이 2개 미만이면 `None`(측정 불가).
/// - convergence = 모든 distinct 쌍의 Jaccard 유사도 평균.
///   합집합이 빈 쌍은 스킵; 유효 쌍이 하나도 없으면 `None`.
///
/// 결정적: rng·네트워크·시간 없음. BTreeSet 사용으로 순회 순서 고정.
pub fn measure(recent: &[&str]) -> Option<FlowMetric> {
    // 각 발화를 토큰 집합으로 변환, 빈 집합 제외
    let sets: Vec<BTreeSet<String>> = recent
        .iter()
        .map(|s| tokenize(s))
        .filter(|s| !s.is_empty())
        .collect();

    // 유효 토큰셋 2개 미만 → 측정 불가
    if sets.len() < 2 {
        return None;
    }

    // 모든 distinct 쌍(i < j)의 Jaccard 합산
    let mut total = 0.0_f64;
    let mut count = 0_usize;

    for i in 0..sets.len() {
        for j in (i + 1)..sets.len() {
            if let Some(j_sim) = jaccard(&sets[i], &sets[j]) {
                total += j_sim;
                count += 1;
            }
        }
    }

    // 유효 쌍이 하나도 없으면 None
    if count == 0 {
        return None;
    }

    Some(FlowMetric {
        convergence: total / count as f64,
    })
}

/// Event 히스토리에서 content 있는 최근 `FLOW_WINDOW`개 발화로 수렴/발산을 측정한다.
///
/// content 없는 발화(FakeBackend/placeholder)는 제외한다. driver(headless 골든 경로)와
/// live(채팅 사이드바)가 같은 윈도우·같은 로직을 쓰도록 단일화한다(인라인 3곳 통합).
/// 골든 보존: content 필터 → 마지막 `FLOW_WINDOW`개 → `measure`, 기존 인라인과 동일.
pub fn measure_recent(history: &[Event]) -> Option<FlowMetric> {
    let content_utterances: Vec<&str> = history
        .iter()
        .filter_map(|e| e.content.as_deref())
        .collect();
    let window_start = content_utterances.len().saturating_sub(FLOW_WINDOW);
    measure(&content_utterances[window_start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (1, 어절 경로 전용) 동일/거의 동일한 발화 여러 개 → convergence 높음 (> 0.5).
    ///
    /// morphology ON에서 형태소 분해 결과가 달라져 임계값이 다를 수 있으므로
    /// 어절 경로(morphology off)에서만 실행.
    #[cfg(not(feature = "morphology"))]
    #[test]
    fn near_identical_utterances_yield_high_convergence() {
        let utterances = ["비 온다 심심해", "비 온다 심심해", "비 온다 그래"];
        let result = measure(&utterances).expect("측정 가능해야 한다");
        assert!(
            result.convergence > 0.5,
            "유사 발화는 convergence > 0.5 기대, 실제: {}",
            result.convergence
        );
    }

    /// (2, 어절 경로 전용) 전혀 다른 토큰으로만 구성된 발화들 → convergence 낮음 (< 0.2).
    ///
    /// morphology ON에서 Lindera가 영어(SL)를 다르게 쪼갤 수 있어 어절 경로 전용.
    #[cfg(not(feature = "morphology"))]
    #[test]
    fn all_distinct_tokens_yield_low_convergence() {
        let utterances = ["apple banana", "cat dog", "echo foxtrot"];
        let result = measure(&utterances).expect("측정 가능해야 한다");
        assert!(
            result.convergence < 0.2,
            "완전 다른 토큰은 convergence < 0.2 기대, 실제: {}",
            result.convergence
        );
    }

    /// (3) 유효 발화 1개 이하 → None (경로 무관).
    #[test]
    fn insufficient_utterances_return_none() {
        // 빈 슬라이스
        assert!(measure(&[]).is_none(), "빈 입력은 None이어야 한다");

        // 발화 1개
        assert!(
            measure(&["only one"]).is_none(),
            "발화 1개는 None이어야 한다"
        );

        // 빈 문자열들만 → 유효 토큰셋 0개
        assert!(
            measure(&["", "  "]).is_none(),
            "빈 문자열들만 있으면 None이어야 한다"
        );
    }

    /// (4) 같은 입력 두 번 measure → 동일 값(결정성, 경로 무관).
    #[test]
    fn measure_is_deterministic() {
        let utterances = ["안녕 세계", "안녕 친구", "세계 평화"];
        let r1 = measure(&utterances);
        let r2 = measure(&utterances);
        assert_eq!(r1, r2, "동일 입력에 대한 두 번의 호출이 같아야 한다");
    }

    /// measure_recent: content=None 발화 제외 + 유효 발화 부족 시 None (경로 무관).
    #[test]
    fn measure_recent_excludes_none_content() {
        let ev = |ts: f64, content: Option<&str>| Event {
            ts,
            speaker: "a".to_string(),
            mark: 0.0,
            content: content.map(str::to_string),
        };
        // content 2개(+ None 1개 제외) → Some
        let h = vec![
            ev(0.0, Some("alpha beta")),
            ev(1.0, Some("alpha gamma")),
            ev(2.0, None),
        ];
        assert!(
            measure_recent(&h).is_some(),
            "content 발화 2개 이상이면 Some(None 발화 제외)"
        );
        // content 1개 + None → 유효 1개 → None
        let h1 = vec![ev(0.0, Some("solo")), ev(1.0, None)];
        assert!(measure_recent(&h1).is_none(), "유효 발화 1개는 None");
        // 빈 히스토리 → None
        assert!(measure_recent(&[]).is_none(), "빈 히스토리는 None");
    }

    /// (5, 어절 경로 전용) Jaccard 손계산 검증: ["a b", "a c"] → {a,b} vs {a,c}.
    /// 교집합 = {a}, 합집합 = {a,b,c} → 1/3 ≈ 0.3333...
    ///
    /// morphology ON에서 morphological_tokens는 1글자 토큰("a")을 제거하므로
    /// 빈 집합이 되어 measure()가 None을 반환한다. 어절 경로 전용.
    #[cfg(not(feature = "morphology"))]
    #[test]
    fn jaccard_hand_computed_verification() {
        let result = measure(&["a b", "a c"]).expect("측정 가능해야 한다");
        let expected = 1.0_f64 / 3.0_f64;
        assert!(
            (result.convergence - expected).abs() < 1e-9,
            "Jaccard({{a,b}}, {{a,c}}) = 1/3 기대, 실제: {}",
            result.convergence
        );
    }

    /// (6, 어절 경로 전용) 구두점 트리밍 확인: "hello," "hello." → 동일 토큰 "hello" → Jaccard=1.
    ///
    /// morphology ON에서 Lindera SL 처리 결과가 다를 수 있어 어절 경로 전용.
    #[cfg(not(feature = "morphology"))]
    #[test]
    fn punctuation_is_stripped_from_tokens() {
        let result = measure(&["hello,", "hello."]).expect("측정 가능해야 한다");
        assert!(
            (result.convergence - 1.0).abs() < 1e-9,
            "구두점 제거 후 동일 토큰 → convergence=1.0 기대, 실제: {}",
            result.convergence
        );
    }

    /// (7, 어절 경로 전용) 대소문자 정규화: "Apple" vs "apple" → 동일 토큰 → Jaccard=1.
    ///
    /// morphology ON에서 Lindera SL이 영어를 어떻게 처리하는지 불확실하여 어절 경로 전용.
    #[cfg(not(feature = "morphology"))]
    #[test]
    fn case_normalization_works() {
        let result = measure(&["Apple Banana", "apple banana"]).expect("측정 가능해야 한다");
        assert!(
            (result.convergence - 1.0).abs() < 1e-9,
            "대소문자 정규화 후 동일 → convergence=1.0 기대, 실제: {}",
            result.convergence
        );
    }

    /// (8, morphology 전용) 조사만 다른 한국어 발화 → convergence 개선 확인.
    ///
    /// "오늘 날씨가 정말 맑다" vs "오늘 날씨를 다시 봤다".
    /// 어절 기반: "날씨가"/"날씨를"이 겹치지 않아 convergence 거의 0.
    /// 형태소 기반: "오늘"(NNG)/"날씨"(NNG) 공유 → convergence 확연히 높음.
    #[cfg(feature = "morphology")]
    #[test]
    fn korean_josa_difference_yields_higher_convergence_with_morphology() {
        let result = measure(&["오늘 날씨가 정말 맑다", "오늘 날씨를 다시 봤다"])
            .expect("측정 가능해야 한다");

        assert!(
            result.convergence > 0.1,
            "형태소 분석 시 조사 제거로 convergence > 0.1 기대, 실제: {}",
            result.convergence
        );
    }

    /// (9, morphology 전용) 같은 발화 두 번 → convergence 높음 (형태소 경로에서도).
    #[cfg(feature = "morphology")]
    #[test]
    fn morphology_identical_utterances_yield_high_convergence() {
        let utterances = [
            "오늘 날씨 정말 좋아",
            "오늘 날씨 정말 좋아",
            "오늘 날씨 맑네",
        ];
        let result = measure(&utterances).expect("측정 가능해야 한다");
        assert!(
            result.convergence > 0.3,
            "동일 한국어 발화 반복 시 convergence > 0.3 기대, 실제: {}",
            result.convergence
        );
    }
}
