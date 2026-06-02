use std::collections::BTreeSet;

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
/// 규칙:
/// 1. 소문자화.
/// 2. 공백으로 분리.
/// 3. 각 토큰의 양끝 ASCII 구두점(`.` `,` `!` `?` `'` `"` `(` `)` `:` `;`) 제거.
/// 4. 빈 문자열이 된 토큰은 버린다.
///
/// 한국어는 공백 분리 근사(v0.6 목표는 정밀도보다 빠른 측정).
pub(crate) fn tokenize(utterance: &str) -> BTreeSet<String> {
    const STRIP: &[char] = &['.', ',', '!', '?', '\'', '"', '(', ')', ':', ';'];
    utterance
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
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// (1) 동일/거의 동일한 발화 여러 개 → convergence 높음 (> 0.5).
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

    /// (2) 전혀 다른 토큰으로만 구성된 발화들 → convergence 낮음 (< 0.2).
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

    /// (3) 유효 발화 1개 이하 → None.
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

    /// (4) 같은 입력 두 번 measure → 동일 값(결정성).
    #[test]
    fn measure_is_deterministic() {
        let utterances = ["안녕 세계", "안녕 친구", "세계 평화"];
        let r1 = measure(&utterances);
        let r2 = measure(&utterances);
        assert_eq!(r1, r2, "동일 입력에 대한 두 번의 호출이 같아야 한다");
    }

    /// (5) Jaccard 손계산 검증: ["a b", "a c"] → {a,b} vs {a,c}.
    /// 교집합 = {a}, 합집합 = {a,b,c} → 1/3 ≈ 0.3333...
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

    /// (6) 구두점 트리밍 확인: "hello," "hello." → 동일 토큰 "hello" → Jaccard=1.
    #[test]
    fn punctuation_is_stripped_from_tokens() {
        let result = measure(&["hello,", "hello."]).expect("측정 가능해야 한다");
        assert!(
            (result.convergence - 1.0).abs() < 1e-9,
            "구두점 제거 후 동일 토큰 → convergence=1.0 기대, 실제: {}",
            result.convergence
        );
    }

    /// (7) 대소문자 정규화: "Apple" vs "apple" → 동일 토큰 → Jaccard=1.
    #[test]
    fn case_normalization_works() {
        let result = measure(&["Apple Banana", "apple banana"]).expect("측정 가능해야 한다");
        assert!(
            (result.convergence - 1.0).abs() < 1e-9,
            "대소문자 정규화 후 동일 → convergence=1.0 기대, 실제: {}",
            result.convergence
        );
    }
}
