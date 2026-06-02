use crate::model::PersonaId;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateResult {
    Silent,
    Candidates(Vec<PersonaId>),
}

pub fn evaluate(intensities: &BTreeMap<PersonaId, f64>, theta: f64) -> GateResult {
    let candidates: Vec<PersonaId> = intensities
        .iter()
        .filter_map(|(persona_id, intensity)| {
            if *intensity >= theta {
                Some(persona_id.clone())
            } else {
                None
            }
        })
        .collect();

    if candidates.is_empty() {
        GateResult::Silent
    } else {
        GateResult::Candidates(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intensities(values: &[(&str, f64)]) -> BTreeMap<PersonaId, f64> {
        values
            .iter()
            .map(|(id, intensity)| (id.to_string(), *intensity))
            .collect()
    }

    #[test]
    fn all_intensities_below_theta_are_silent() {
        let intensities = intensities(&[("alice", 0.2), ("bob", 0.4), ("carol", 0.6)]);

        assert_eq!(evaluate(&intensities, 0.7), GateResult::Silent);
    }

    #[test]
    fn candidates_at_or_above_theta_are_returned_in_sorted_order() {
        let intensities = intensities(&[("carol", 0.8), ("alice", 0.9), ("bob", 0.3)]);

        assert_eq!(
            evaluate(&intensities, 0.7),
            GateResult::Candidates(vec!["alice".to_string(), "carol".to_string()])
        );
    }

    #[test]
    fn intensity_equal_to_theta_is_a_candidate() {
        let intensities = intensities(&[("alice", 0.7), ("bob", 0.69)]);

        assert_eq!(
            evaluate(&intensities, 0.7),
            GateResult::Candidates(vec!["alice".to_string()])
        );
    }
}
