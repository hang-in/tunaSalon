//! 생성 워커에 주입할 진행 지시(directive) + 반복 억제 + 화제 관련성 판정(순수).

use crate::memory::MemoryEvent;

pub(crate) fn repetition_guard(
    history: &[crate::model::Event],
    speaker: &str,
) -> Option<&'static str> {
    let recent = history
        .iter()
        .rev()
        .filter(|event| {
            event
                .content
                .as_ref()
                .is_some_and(|content| !content.trim().is_empty())
        })
        .take(6)
        .collect::<Vec<_>>();
    let speaker_count = recent
        .iter()
        .filter(|event| event.speaker == speaker)
        .count();
    if recent.len() >= 4 && speaker_count >= 2 {
        Some("[반복 억제] 최근 논거를 다시 말하지 마세요. 현재 주제와 직접 관련된 새 사례, 수치 기준, 법적 임계값, 검증 절차, 또는 양측을 잇는 절충안을 하나 이상 추가하세요.")
    } else {
        None
    }
}

pub(crate) fn significant_topic_tokens(topics: &[String]) -> std::collections::BTreeSet<String> {
    let joined = topics.join(" ");
    crate::flow::tokenize(&joined)
        .into_iter()
        .filter(|token| token.chars().count() > 1)
        .filter(|token| !matches!(token.as_str(), "ai" | "수" | "것" | "the" | "and" | "or"))
        .collect()
}

pub(crate) fn cross_room_memory_is_topic_relevant(
    event: &MemoryEvent,
    topic_tokens: &std::collections::BTreeSet<String>,
) -> bool {
    if topic_tokens.is_empty() {
        return false;
    }
    let content_tokens = crate::flow::tokenize(&event.content);
    topic_tokens.intersection(&content_tokens).take(2).count() >= 2
}

/// 생성 워커에 주입할 "[진행 지시]" 텍스트(순수). 우선순위: 사람 우선 > 화제 > 없음.
pub(crate) fn build_directive(
    human_msg: Option<&str>,
    human_focus_active: bool,
    topics: &[String],
    direct_call: bool,
    repetition: Option<&str>,
) -> Option<String> {
    let mut parts = Vec::new();
    if direct_call {
        parts.push("[진행 지시] 사용자가 당신을 직접 호출했습니다. 침묵 이유를 남이 해석하게 두지 말고 본인이 직접 짧게 응답한 뒤, 현재 쟁점에 대한 입장과 다음 논점을 제시하세요.".to_string());
    }
    if let Some(r) = repetition {
        parts.push(r.to_string());
    }
    if human_focus_active {
        if let Some(h) = human_msg {
            parts.push(format!(
                "[진행 지시] 사용자(나)가 \"{h}\"라고 했습니다. 지금은 이 발화를 최우선으로 삼아 토론하세요. 다른 화제로 새지 말고 사용자 말에 직접 답하되, 최근 참가자 발화나 기억과 연결해 동의/반박/보완 중 하나를 분명히 하세요."
            ));
            return Some(parts.join(" "));
        }
    }
    if !topics.is_empty() {
        parts.push(format!(
            "[진행 지시] 토론 주제는 '{}'입니다. 이 주제에서 벗어나지 말고, 최근 발언자 닉네임을 불러 동의하거나 반박하거나 보완하세요. 자기 입장을 먼저 분명히 하고 근거와 현실적 결과를 붙이세요.",
            topics.join("', '")
        ));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_directive_prioritizes_human() {
        // 사람 우선 활성 → 사람 메시지 지시
        let d = build_directive(
            Some("드라마 추천 좀"),
            true,
            &["부처님".to_string()],
            false,
            None,
        )
        .unwrap();
        assert!(d.contains("드라마 추천 좀") && d.contains("최우선"));
        // 사람 우선 비활성 + 화제 → 화제 지시(사람 메시지 미포함)
        let d = build_directive(
            Some("드라마 추천 좀"),
            false,
            &["부처님".to_string()],
            false,
            None,
        )
        .unwrap();
        assert!(d.contains("부처님") && !d.contains("드라마 추천 좀"));
        // 둘 다 없음 → None
        assert!(build_directive(None, false, &[], false, None).is_none());
    }

    #[test]
    fn directive_includes_direct_call_and_repetition_guard() {
        let d = build_directive(
            Some("날카로운별의노래는 왜 조용해?"),
            true,
            &["오픈소스 보안 책임".to_string()],
            true,
            Some("[반복 억제] 새 근거를 제시하세요."),
        )
        .unwrap();
        assert!(d.contains("직접 호출"));
        assert!(d.contains("반복 억제"));
        assert!(d.contains("날카로운별의노래"));
    }
}
