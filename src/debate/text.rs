//! 발화 텍스트 정규화 + 페르소나 멘션/요약자 식별(순수).

use crate::model::{Persona, PersonaId};

/// 생성 결과 앞에 모델이 echo한 화자 라벨(`이름:` / `나:`)을 1회 제거한다.
/// `labels`에 매칭되는 짧은 라벨일 때만 제거한다(과잉 strip 방지).
pub(crate) fn strip_speaker_prefix(
    text: &str,
    labels: &std::collections::BTreeSet<String>,
) -> String {
    let trimmed = text.trim_start();
    if let Some(colon) = trimmed.find(':') {
        let label = trimmed[..colon].trim();
        if !label.is_empty()
            && label.chars().count() <= 20
            && labels.contains(&label.to_lowercase())
        {
            return trimmed[colon + 1..].trim_start().to_string();
        }
    }
    text.to_string()
}

pub(crate) fn sanitize_generated_text(text: &str) -> String {
    text.replace("나님", "사용자님")
        .replace("나 님", "사용자님")
}

fn normalize_mention_text(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub(crate) fn mentioned_persona_id(text: &str, personas: &[Persona]) -> Option<PersonaId> {
    let normalized = normalize_mention_text(text);
    personas.iter().find_map(|persona| {
        let name = normalize_mention_text(&persona.name);
        let id = normalize_mention_text(&persona.id);
        if (!name.is_empty() && normalized.contains(&name))
            || (!id.is_empty() && normalized.contains(&id))
        {
            Some(persona.id.clone())
        } else {
            None
        }
    })
}

pub(crate) fn summary_persona_id(personas: &[Persona]) -> Option<PersonaId> {
    personas
        .iter()
        .find(|p| p.id == "summarizer")
        .or_else(|| {
            personas
                .iter()
                .find(|p| p.name.contains("요약") || p.name.contains("정리"))
        })
        .map(|p| p.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_speaker_prefix_removes_echoed_label() {
        let mut labels = std::collections::BTreeSet::new();
        labels.insert("grounded realist".to_string());
        labels.insert("realist".to_string());
        labels.insert("나".to_string());
        assert_eq!(
            strip_speaker_prefix("Realist: 집착을 버려", &labels),
            "집착을 버려"
        );
        assert_eq!(
            strip_speaker_prefix("나: 근데 말이야", &labels),
            "근데 말이야"
        );
        // 라벨 매칭 안 됨 → 그대로(과잉 strip 방지)
        assert_eq!(
            strip_speaker_prefix("오늘 날씨 좋다", &labels),
            "오늘 날씨 좋다"
        );
        assert_eq!(
            strip_speaker_prefix("넷플릭스: 추천", &labels),
            "넷플릭스: 추천"
        );
    }

    #[test]
    fn mention_detection_and_sanitize_work() {
        let personas = vec![Persona {
            id: "summarizer".to_string(),
            name: "날카로운별의노래".to_string(),
            base_rate: 0.5,
        }];
        assert_eq!(
            mentioned_persona_id("날카로운별의노래는 왜 조용해?", &personas),
            Some("summarizer".to_string())
        );
        assert_eq!(
            sanitize_generated_text("나님, 제 생각은"),
            "사용자님, 제 생각은"
        );
    }
}
