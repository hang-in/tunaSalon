use crate::model::{CouplingMatrix, EngineConfig, EngineState, Persona, PersonaId};
use std::collections::BTreeMap;

const SPEAKER_SUPPRESSION_FACTOR: f64 = 0.25;

pub struct HawkesEngine;

impl HawkesEngine {
    /// Hawkes 기저 강도를 1틱 회복시킨다.
    ///
    /// `mu_scale ∈ [0, 1]`: 회복 목표(effective μ)를 `base_rate * mu_scale`로 낮춘다.
    /// - `mu_scale == 1.0` → `mu = base_rate` → 기존 공식과 비트 동일(골든 보존).
    /// - `mu_scale < 1.0` → 회복 목표가 낮아져 강도가 점차 내려감(MetaController 식히기).
    pub fn update_intensities(
        state: &EngineState,
        elapsed_ticks: u64,
        config: &EngineConfig,
        personas: &[Persona],
        mu_scale: f64,
    ) -> BTreeMap<PersonaId, f64> {
        let elapsed = (elapsed_ticks as f64) * config.tick_interval;
        let decay = (-config.beta * elapsed).exp();
        let mut updated = BTreeMap::new();

        for persona in personas {
            let base_rate = persona.base_rate;
            // mu_scale=1.0이면 mu == base_rate → 기존 공식과 완전 동일.
            let mu = base_rate * mu_scale;
            let previous = match state.intensities.get(&persona.id) {
                Some(value) => *value,
                None => base_rate,
            };
            let intensity = mu + (previous - mu) * decay;

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

    pub fn decay_excitations(
        excitations: &BTreeMap<PersonaId, f64>,
        elapsed_ticks: u64,
        beta: f64,
        tick_interval: f64,
    ) -> BTreeMap<PersonaId, f64> {
        let elapsed = (elapsed_ticks as f64) * tick_interval;
        let decay = (-beta * elapsed).exp();

        excitations
            .iter()
            .map(|(persona_id, excitation)| (persona_id.clone(), excitation * decay))
            .collect()
    }

    pub fn apply_excitation_on_speak(
        excitations: &mut BTreeMap<PersonaId, f64>,
        alpha: &CouplingMatrix,
        speaker: &PersonaId,
        personas: &[Persona],
    ) {
        for persona in personas {
            let excitation = excitations.entry(persona.id.clone()).or_insert(0.0);
            *excitation += alpha.get(&persona.id, speaker);
        }
    }

    pub fn combined_intensities(
        base: &BTreeMap<PersonaId, f64>,
        excitations: &BTreeMap<PersonaId, f64>,
        personas: &[Persona],
    ) -> BTreeMap<PersonaId, f64> {
        let mut combined = BTreeMap::new();

        for persona in personas {
            let base_value = match base.get(&persona.id) {
                Some(value) => *value,
                None => persona.base_rate,
            };
            let excitation = match excitations.get(&persona.id) {
                Some(value) => *value,
                None => 0.0,
            };
            combined.insert(persona.id.clone(), base_value + excitation);
        }

        combined
    }

    pub fn branching_spectral_radius(
        alpha: &CouplingMatrix,
        personas: &[Persona],
        beta: f64,
    ) -> f64 {
        if personas.is_empty() || beta <= 0.0 || !beta.is_finite() {
            return 0.0;
        }

        let n = personas.len();
        let mut vector = vec![1.0 / n as f64; n];
        let mut log_scale_sum = 0.0;
        let mut scale_count = 0_u64;

        for iteration in 0..288 {
            let mut next = vec![0.0; n];

            for (row_index, persona) in personas.iter().enumerate() {
                let mut value = 0.0;
                for (column_index, speaker) in personas.iter().enumerate() {
                    let entry = alpha.get(&persona.id, &speaker.id) / beta;
                    if entry.is_finite() && entry > 0.0 {
                        value += entry * vector[column_index];
                    }
                }
                next[row_index] = value;
            }

            let norm = next.iter().copied().fold(0.0, f64::max);
            if norm <= 0.0 || !norm.is_finite() {
                return 0.0;
            }
            if iteration >= 32 {
                log_scale_sum += norm.ln();
                scale_count += 1;
            }

            for value in &mut next {
                *value /= norm;
            }
            vector = next;
        }

        if scale_count > 0 {
            let radius = (log_scale_sum / scale_count as f64).exp();
            if radius.is_finite() {
                return radius;
            }
            0.0
        } else {
            0.0
        }
    }

    pub fn is_stable(alpha: &CouplingMatrix, personas: &[Persona], beta: f64) -> bool {
        Self::branching_spectral_radius(alpha, personas, beta) < 1.0
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
            alpha: CouplingMatrix::default(),
            forbid_self_repeat: false,
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
            excitations: BTreeMap::new(),
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

        let recovered = HawkesEngine::update_intensities(&state, 100, &config(), &personas, 1.0);

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
            HawkesEngine::update_intensities(&recovering_once, 1, &config(), &personas, 1.0);
        let recovering_twice = state(recovered_one.clone(), Some("active".to_string()));
        let recovered_two =
            HawkesEngine::update_intensities(&recovering_twice, 1, &config(), &personas, 1.0);

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

        let first = HawkesEngine::update_intensities(&state, 7, &config, &personas, 1.0);
        let second = HawkesEngine::update_intensities(&state, 7, &config, &personas, 1.0);

        assert_eq!(first, second);
    }

    #[test]
    fn cross_excitation_raises_combined_intensity_after_speaker_event() {
        let personas = personas();
        let base = BTreeMap::from([("quiet".to_string(), 0.3), ("active".to_string(), 0.9)]);
        let mut alpha = CouplingMatrix::default();
        alpha
            .values
            .insert(("quiet".to_string(), "active".to_string()), 0.4);
        let mut excitations = BTreeMap::new();

        HawkesEngine::apply_excitation_on_speak(
            &mut excitations,
            &alpha,
            &"active".to_string(),
            &personas,
        );
        let combined = HawkesEngine::combined_intensities(&base, &excitations, &personas);

        assert!(intensity_at(&combined, "quiet") > intensity_at(&base, "quiet"));
        assert_eq!(
            intensity_at(&combined, "active"),
            intensity_at(&base, "active")
        );
    }

    #[test]
    fn excitation_decays_monotonically_toward_zero_without_events() {
        let mut excitations = BTreeMap::from([("quiet".to_string(), 1.0)]);
        let first = HawkesEngine::decay_excitations(&excitations, 1, 0.5, 1.0);
        excitations = first.clone();
        let second = HawkesEngine::decay_excitations(&excitations, 1, 0.5, 1.0);

        assert!(intensity_at(&first, "quiet") < 1.0);
        assert!(intensity_at(&second, "quiet") < intensity_at(&first, "quiet"));
        assert!(intensity_at(&second, "quiet") > 0.0);
    }

    #[test]
    fn empty_alpha_keeps_combined_intensities_equal_to_base_sequence() {
        let personas = personas();
        let config = config();
        let alpha = CouplingMatrix::default();
        let mut state = state(BTreeMap::new(), None);
        let mut excitations = BTreeMap::new();

        for _ in 0..12 {
            let base = HawkesEngine::update_intensities(&state, 1, &config, &personas, 1.0);
            excitations =
                HawkesEngine::decay_excitations(&excitations, 1, config.beta, config.tick_interval);
            HawkesEngine::apply_excitation_on_speak(
                &mut excitations,
                &alpha,
                &"active".to_string(),
                &personas,
            );
            let combined = HawkesEngine::combined_intensities(&base, &excitations, &personas);

            assert_eq!(combined, base);
            state.intensities = base;
        }
    }

    #[test]
    fn spectral_radius_matches_known_two_by_two_and_stability_boundary() {
        let personas = vec![
            Persona {
                id: "a".to_string(),
                name: "A".to_string(),
                base_rate: 0.5,
            },
            Persona {
                id: "b".to_string(),
                name: "B".to_string(),
                base_rate: 0.5,
            },
        ];
        let mut alpha = CouplingMatrix::default();
        alpha.values.insert(("a".to_string(), "b".to_string()), 0.2);
        alpha.values.insert(("b".to_string(), "a".to_string()), 0.8);

        let radius = HawkesEngine::branching_spectral_radius(&alpha, &personas, 1.0);
        let hand_value = (0.2_f64 * 0.8_f64).sqrt();

        assert!((radius - hand_value).abs() < 1e-9);
        assert!(HawkesEngine::is_stable(&alpha, &personas, 1.0));

        alpha.values.insert(("a".to_string(), "b".to_string()), 1.0);
        alpha.values.insert(("b".to_string(), "a".to_string()), 1.0);

        assert!(!HawkesEngine::is_stable(&alpha, &personas, 1.0));
    }

    #[test]
    fn stable_excitation_stays_bounded_while_unstable_excitation_grows() {
        let personas = vec![Persona {
            id: "solo".to_string(),
            name: "Solo".to_string(),
            base_rate: 0.5,
        }];
        let base = BTreeMap::from([("solo".to_string(), 0.5)]);
        let mut stable_alpha = CouplingMatrix::default();
        stable_alpha
            .values
            .insert(("solo".to_string(), "solo".to_string()), 0.4);
        let mut unstable_alpha = CouplingMatrix::default();
        unstable_alpha
            .values
            .insert(("solo".to_string(), "solo".to_string()), 1.2);
        let mut stable_excitations = BTreeMap::new();
        let mut unstable_excitations = BTreeMap::new();

        for _ in 0..40 {
            stable_excitations = HawkesEngine::decay_excitations(&stable_excitations, 1, 1.0, 1.0);
            unstable_excitations =
                HawkesEngine::decay_excitations(&unstable_excitations, 1, 1.0, 1.0);
            HawkesEngine::apply_excitation_on_speak(
                &mut stable_excitations,
                &stable_alpha,
                &"solo".to_string(),
                &personas,
            );
            HawkesEngine::apply_excitation_on_speak(
                &mut unstable_excitations,
                &unstable_alpha,
                &"solo".to_string(),
                &personas,
            );
        }

        let stable = HawkesEngine::combined_intensities(&base, &stable_excitations, &personas);
        let unstable = HawkesEngine::combined_intensities(&base, &unstable_excitations, &personas);

        assert!(HawkesEngine::is_stable(&stable_alpha, &personas, 1.0));
        assert!(!HawkesEngine::is_stable(&unstable_alpha, &personas, 1.0));
        assert!(intensity_at(&stable, "solo") < 1.2);
        assert!(intensity_at(&unstable, "solo") > intensity_at(&stable, "solo") * 2.0);
    }

    // -------------------------------------------------------------------------
    // task-37 신규 테스트: mu_scale 동작 검증
    // -------------------------------------------------------------------------

    /// (task-37-1) mu_scale=1.0은 기존 동작과 동일한 결과를 반환해야 한다.
    ///
    /// mu=base_rate*1.0 == base_rate → 공식이 완전히 동일 → 비트 동일.
    #[test]
    fn mu_scale_one_matches_pre_change_behavior() {
        let personas = personas();
        let config = config();

        // 기존 공식 직접 계산: intensity = base_rate + (prev - base_rate)*decay
        let elapsed = 1.0 * config.tick_interval;
        let decay = (-config.beta * elapsed).exp();

        let prev_quiet = 0.1_f64;
        let prev_active = 1.5_f64;

        let mut initial = BTreeMap::new();
        initial.insert("quiet".to_string(), prev_quiet);
        initial.insert("active".to_string(), prev_active);
        let state = state(initial, None);

        // mu_scale=1.0으로 호출
        let result = HawkesEngine::update_intensities(&state, 1, &config, &personas, 1.0);

        // 손계산으로 기대값
        let expected_quiet = 0.3 + (prev_quiet - 0.3) * decay;
        let expected_active = 0.9 + (prev_active - 0.9) * decay;

        let got_quiet = intensity_at(&result, "quiet");
        let got_active = intensity_at(&result, "active");

        assert!(
            (got_quiet - expected_quiet).abs() < 1e-15,
            "mu_scale=1.0: quiet 기대={expected_quiet}, 실제={got_quiet}"
        );
        assert!(
            (got_active - expected_active).abs() < 1e-15,
            "mu_scale=1.0: active 기대={expected_active}, 실제={got_active}"
        );
    }

    /// (task-37-2) mu_scale<1.0이면 회복 목표가 낮아져 강도가 더 낮게 수렴한다.
    ///
    /// base_rate=0.9 페르소나가 0.1에서 회복 중: mu_scale=0.5이면 목표가 0.45로 낮아짐.
    /// → intensity(scale=0.5) < intensity(scale=1.0) (초기값이 목표보다 낮을 때).
    #[test]
    fn mu_scale_below_one_recovers_toward_lower_target() {
        let personas = personas();
        let config = config();

        // active(base_rate=0.9)가 0.1(목표보다 낮음)에서 회복 중
        let mut initial = BTreeMap::new();
        initial.insert("quiet".to_string(), 0.3_f64); // 기준점
        initial.insert("active".to_string(), 0.1_f64); // 목표(0.9) 아래
        let s = state(initial, None);

        let result_scale1 = HawkesEngine::update_intensities(&s, 1, &config, &personas, 1.0);
        let result_scale_half = HawkesEngine::update_intensities(&s, 1, &config, &personas, 0.5);

        let active_scale1 = intensity_at(&result_scale1, "active");
        let active_scale_half = intensity_at(&result_scale_half, "active");

        // mu_scale=0.5 목표는 0.45; 초기값 0.1에서 0.45로 회복.
        // mu_scale=1.0 목표는 0.9; 초기값 0.1에서 0.9로 회복.
        // 회복 방향은 같지만 scale=0.5 목표가 낮으므로 최종값도 낮다.
        assert!(
            active_scale_half < active_scale1,
            "mu_scale=0.5 강도({active_scale_half}) < mu_scale=1.0 강도({active_scale1}) 이어야 한다"
        );
    }
}
