use crate::model::{Event, PersonaId};
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub chosen: PersonaId,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signal {
    Intensity,
    Balance,
    Random,
}

impl Signal {
    fn name(self) -> &'static str {
        match self {
            Signal::Intensity => "intensity",
            Signal::Balance => "balance",
            Signal::Random => "random",
        }
    }
}

pub fn select(
    candidates: &[PersonaId],
    intensities: &BTreeMap<PersonaId, f64>,
    history: &[Event],
    k: f64,
    rng: &mut ChaCha8Rng,
) -> Selection {
    if candidates.is_empty() {
        return Selection {
            chosen: PersonaId::new(),
            reason: Signal::Random.name().to_string(),
        };
    }

    let mut ordered = candidates.to_vec();
    ordered.sort();
    ordered.dedup();

    let intensity_ranks = rank_by_intensity(&ordered, intensities);
    let balance_ranks = rank_by_balance(&ordered, history);
    let random_ranks = rank_by_random(&ordered, rng);

    ordered
        .iter()
        .map(|candidate| {
            let ranks = ranks_for(candidate, &intensity_ranks, &balance_ranks, &random_ranks);
            let score = rrf_score(k, ranks);
            (candidate, score, best_signal(ranks))
        })
        .max_by(|(left_id, left_score, _), (right_id, right_score, _)| {
            compare_f64(*left_score, *right_score).then_with(|| right_id.cmp(left_id))
        })
        .map(|(chosen, _, reason)| Selection {
            chosen: chosen.clone(),
            reason: reason.name().to_string(),
        })
        .unwrap_or(Selection {
            chosen: PersonaId::new(),
            reason: Signal::Random.name().to_string(),
        })
}

fn rank_by_intensity(
    candidates: &[PersonaId],
    intensities: &BTreeMap<PersonaId, f64>,
) -> BTreeMap<PersonaId, usize> {
    rank_by(candidates, |left, right| {
        let left_value = intensities.get(left).copied().unwrap_or(0.0);
        let right_value = intensities.get(right).copied().unwrap_or(0.0);

        compare_f64(right_value, left_value)
    })
}

fn rank_by_balance(candidates: &[PersonaId], history: &[Event]) -> BTreeMap<PersonaId, usize> {
    rank_by(candidates, |left, right| {
        let left_count = utterance_count(left, history);
        let right_count = utterance_count(right, history);

        left_count.cmp(&right_count)
    })
}

fn rank_by_random(candidates: &[PersonaId], rng: &mut ChaCha8Rng) -> BTreeMap<PersonaId, usize> {
    let mut shuffled = candidates.to_vec();
    shuffled.shuffle(rng);

    shuffled
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| (candidate, index + 1))
        .collect()
}

fn rank_by<F>(candidates: &[PersonaId], compare: F) -> BTreeMap<PersonaId, usize>
where
    F: Fn(&PersonaId, &PersonaId) -> Ordering,
{
    let mut ranked = candidates.to_vec();
    ranked.sort_by(|left, right| compare(left, right).then_with(|| left.cmp(right)));

    ranked
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| (candidate, index + 1))
        .collect()
}

fn utterance_count(candidate: &PersonaId, history: &[Event]) -> usize {
    history
        .iter()
        .filter(|event| &event.speaker == candidate)
        .count()
}

fn ranks_for(
    candidate: &PersonaId,
    intensity_ranks: &BTreeMap<PersonaId, usize>,
    balance_ranks: &BTreeMap<PersonaId, usize>,
    random_ranks: &BTreeMap<PersonaId, usize>,
) -> [(Signal, usize); 3] {
    [
        (
            Signal::Intensity,
            intensity_ranks
                .get(candidate)
                .copied()
                .unwrap_or(usize::MAX),
        ),
        (
            Signal::Balance,
            balance_ranks.get(candidate).copied().unwrap_or(usize::MAX),
        ),
        (
            Signal::Random,
            random_ranks.get(candidate).copied().unwrap_or(usize::MAX),
        ),
    ]
}

fn best_signal(ranks: [(Signal, usize); 3]) -> Signal {
    ranks
        .into_iter()
        .min_by(|(left_signal, left_rank), (right_signal, right_rank)| {
            left_rank
                .cmp(right_rank)
                .then_with(|| signal_order(*left_signal).cmp(&signal_order(*right_signal)))
        })
        .map(|(signal, _)| signal)
        .unwrap_or(Signal::Intensity)
}

fn signal_order(signal: Signal) -> usize {
    match signal {
        Signal::Intensity => 0,
        Signal::Balance => 1,
        Signal::Random => 2,
    }
}

fn rrf_score(k: f64, ranks: [(Signal, usize); 3]) -> f64 {
    ranks
        .into_iter()
        .map(|(_, rank)| 1.0 / (k + rank as f64))
        .sum()
}

fn compare_f64(left: f64, right: f64) -> Ordering {
    left.partial_cmp(&right).unwrap_or(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn ids(values: &[&str]) -> Vec<PersonaId> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn intensities(values: &[(&str, f64)]) -> BTreeMap<PersonaId, f64> {
        values
            .iter()
            .map(|(id, value)| (id.to_string(), *value))
            .collect()
    }

    fn event(speaker: &str, ts: f64) -> Event {
        Event {
            ts,
            speaker: speaker.to_string(),
            mark: 1.0,
        }
    }

    #[test]
    fn rrf_score_formula_matches_known_ranks() {
        let ranks = [
            (Signal::Intensity, 1),
            (Signal::Balance, 3),
            (Signal::Random, 2),
        ];
        let k = 60.0;
        let expected = (1.0 / 61.0) + (1.0 / 63.0) + (1.0 / 62.0);

        assert_eq!(rrf_score(k, ranks), expected);
    }

    #[test]
    fn balance_ranks_more_frequent_speaker_worse() {
        let candidates = ids(&["a", "b", "c"]);
        let history = vec![event("a", 1.0), event("a", 2.0), event("b", 3.0)];

        let ranks = rank_by_balance(&candidates, &history);

        assert!(ranks["c"] < ranks["b"]);
        assert!(ranks["b"] < ranks["a"]);
    }

    #[test]
    fn same_seed_and_inputs_select_identically() {
        let candidates = ids(&["a", "b", "c"]);
        let intensities = intensities(&[("a", 0.9), ("b", 0.8), ("c", 0.1)]);
        let history = vec![event("a", 1.0), event("b", 2.0)];
        let mut left_rng = ChaCha8Rng::seed_from_u64(42);
        let mut right_rng = ChaCha8Rng::seed_from_u64(42);

        let left = select(&candidates, &intensities, &history, 60.0, &mut left_rng);
        let right = select(&candidates, &intensities, &history, 60.0, &mut right_rng);

        assert_eq!(left, right);
    }

    #[test]
    fn large_k_lets_random_signal_flip_non_leader_more_often() {
        let candidates = ids(&["a", "b", "c", "d"]);
        let intensities = intensities(&[("a", 1.0), ("b", 0.9), ("c", 0.8), ("d", 0.7)]);
        let history = vec![
            event("b", 1.0),
            event("c", 2.0),
            event("c", 3.0),
            event("d", 4.0),
            event("d", 5.0),
            event("d", 6.0),
        ];

        let small_k_non_leader_wins = non_leader_wins(0.01, &candidates, &intensities, &history);
        let large_k_non_leader_wins = non_leader_wins(1_000.0, &candidates, &intensities, &history);

        assert!(large_k_non_leader_wins > small_k_non_leader_wins);
    }

    fn non_leader_wins(
        k: f64,
        candidates: &[PersonaId],
        intensities: &BTreeMap<PersonaId, f64>,
        history: &[Event],
    ) -> usize {
        (0..200)
            .filter(|seed| {
                let mut rng = ChaCha8Rng::seed_from_u64(*seed);
                select(candidates, intensities, history, k, &mut rng).chosen == "b"
            })
            .count()
    }
}
