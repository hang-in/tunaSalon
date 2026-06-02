use crate::model::{EngineConfig, EngineState, Persona, PersonaId};
use std::collections::BTreeMap;

const SPEAKER_SUPPRESSION_FACTOR: f64 = 0.25;

pub struct HawkesEngine;

impl HawkesEngine {
    pub fn update_intensities(
        state: &EngineState,
        elapsed_ticks: u64,
        config: &EngineConfig,
        personas: &[Persona],
    ) -> BTreeMap<PersonaId, f64> {
        let elapsed = (elapsed_ticks as f64) * config.tick_interval;
        let decay = (-config.beta * elapsed).exp();
        let mut updated = BTreeMap::new();

        for persona in personas {
            let base_rate = persona.base_rate;
            let previous = match state.intensities.get(&persona.id) {
                Some(value) => *value,
                None => base_rate,
            };
            let intensity = base_rate + (previous - base_rate) * decay;

            updated.insert(persona.id.clone(), intensity);
        }

        updated
    }

    /// 발화 직후 1회 억제값. 드라이버(task-06)가 발화 시점에 해당 페르소나의 저장 강도에
    /// 한 번만 적용한다. last_speaker로 매 틱 재적용하면 마지막 화자가 μ 아래에 고착되어
    /// idle 회복(침묵이 길어지면 base rate가 강도를 밀어올리는 동역학)이 깨진다.
    pub fn suppressed_after_speak(base_rate: f64) -> f64 {
        base_rate * SPEAKER_SUPPRESSION_FACTOR
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> EngineConfig {
        EngineConfig {
            beta: 1.0,
            theta: 0.7,
            k: 60.0,
            tick_interval: 1.0,
        }
    }

    fn personas() -> Vec<Persona> {
        vec![
            Persona {
                id: "quiet".to_string(),
                name: "Quiet".to_string(),
                base_rate: 0.3,
            },
            Persona {
                id: "active".to_string(),
                name: "Active".to_string(),
                base_rate: 0.9,
            },
        ]
    }

    fn state(
        intensities: BTreeMap<PersonaId, f64>,
        last_speaker: Option<PersonaId>,
    ) -> EngineState {
        EngineState {
            intensities,
            history: Vec::new(),
            last_speaker,
            rng_seed: 42,
        }
    }

    fn intensity_at(intensities: &BTreeMap<PersonaId, f64>, id: &str) -> f64 {
        match intensities.get(id) {
            Some(value) => *value,
            None => f64::NAN,
        }
    }

    #[test]
    fn higher_base_rate_has_higher_steady_state_after_recovery() {
        let personas = personas();
        let mut initial = BTreeMap::new();
        initial.insert("quiet".to_string(), 4.0);
        initial.insert("active".to_string(), -2.0);
        let state = state(initial, None);

        let recovered = HawkesEngine::update_intensities(&state, 100, &config(), &personas);

        assert!(intensity_at(&recovered, "active") > intensity_at(&recovered, "quiet"));
    }

    #[test]
    fn speaker_intensity_drops_then_recovers_monotonically_toward_base_rate() {
        let personas = personas();
        let base = 0.9; // "active" base_rate

        // 발화 직후 1회 억제 적용 (드라이버가 하는 일). 이후엔 last_speaker 무관하게 순수 회복.
        let dropped_active = HawkesEngine::suppressed_after_speak(base);
        let mut suppressed = BTreeMap::new();
        suppressed.insert("active".to_string(), dropped_active);

        let recovering_once = state(suppressed, Some("active".to_string()));
        let recovered_one =
            HawkesEngine::update_intensities(&recovering_once, 1, &config(), &personas);
        let recovering_twice = state(recovered_one.clone(), Some("active".to_string()));
        let recovered_two =
            HawkesEngine::update_intensities(&recovering_twice, 1, &config(), &personas);

        let recovered_one_active = intensity_at(&recovered_one, "active");
        let recovered_two_active = intensity_at(&recovered_two, "active");

        // last_speaker가 계속 active여도 고착되지 않고 μ 쪽으로 단조 회복해야 한다.
        assert!(dropped_active < base);
        assert!(dropped_active < recovered_one_active);
        assert!(recovered_one_active < recovered_two_active);
        assert!(recovered_two_active < base);
    }

    #[test]
    fn update_is_deterministic_for_same_inputs() {
        let personas = personas();
        let mut initial = BTreeMap::new();
        initial.insert("quiet".to_string(), 0.1);
        initial.insert("active".to_string(), 1.2);
        let state = state(initial, Some("quiet".to_string()));
        let config = config();

        let first = HawkesEngine::update_intensities(&state, 7, &config, &personas);
        let second = HawkesEngine::update_intensities(&state, 7, &config, &personas);

        assert_eq!(first, second);
    }
}
