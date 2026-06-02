use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::model::{Event, PersonaId};

const TOPIC_TAGS: &[&str] = &["art", "music", "food", "travel", "books"];
const DEFAULT_MARK: f64 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub event: Event,
    pub topic_tag: Option<String>,
}

pub fn make_utterance(
    speaker: &PersonaId,
    tick: u64,
    tick_interval: f64,
    with_topic_tag: bool,
    rng: &mut ChaCha8Rng,
) -> Utterance {
    let event = Event {
        ts: tick as f64 * tick_interval,
        speaker: speaker.clone(),
        mark: DEFAULT_MARK,
        content: None,
    };

    // Topic tags mimic the future v0.3 interest signal; v0.1 validation does not need them.
    let topic_tag = if with_topic_tag {
        let index = rng.gen_range(0..TOPIC_TAGS.len());
        Some(TOPIC_TAGS[index].to_string())
    } else {
        None
    };

    Utterance { event, topic_tag }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn builds_event_for_speaker_at_logical_time() {
        let speaker = "p1".to_string();
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let utterance = make_utterance(&speaker, 12, 0.5, false, &mut rng);

        assert_eq!(utterance.event.speaker, speaker);
        assert_eq!(utterance.event.ts, 6.0);
        assert_eq!(utterance.event.mark, DEFAULT_MARK);
    }

    #[test]
    fn topic_tag_flag_controls_optional_deterministic_tag() {
        let speaker = "p2".to_string();
        let mut off_rng = ChaCha8Rng::seed_from_u64(11);
        let off = make_utterance(&speaker, 1, 1.0, false, &mut off_rng);

        let mut first_rng = ChaCha8Rng::seed_from_u64(11);
        let first = make_utterance(&speaker, 1, 1.0, true, &mut first_rng);
        let mut second_rng = ChaCha8Rng::seed_from_u64(11);
        let second = make_utterance(&speaker, 1, 1.0, true, &mut second_rng);

        assert_eq!(off.topic_tag, None);
        assert_eq!(first.topic_tag, second.topic_tag);
        assert!(first
            .topic_tag
            .as_deref()
            .is_some_and(|tag| TOPIC_TAGS.contains(&tag)));
    }
}
