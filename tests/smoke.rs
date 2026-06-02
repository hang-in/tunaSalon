use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use salon::driver;
use salon::model::{EngineConfig, Persona, PersonaId};
use salon::rrf;
use salon::sink::VecSink;
use std::collections::BTreeMap;

const SEED: u64 = 42;
const TICKS: u64 = 200;

fn base_config(theta: f64, k: f64) -> EngineConfig {
    EngineConfig {
        beta: 0.5,
        theta,
        k,
        tick_interval: 1.0,
    }
}

fn demo_personas() -> Vec<Persona> {
    vec![
        Persona {
            id: "friend".to_string(),
            name: "Friendly Regular".to_string(),
            base_rate: 0.80,
        },
        Persona {
            id: "chaos".to_string(),
            name: "Chaos Guest".to_string(),
            base_rate: 0.70,
        },
        Persona {
            id: "summarizer".to_string(),
            name: "Quiet Summarizer".to_string(),
            base_rate: 0.25,
        },
    ]
}

fn run_to_sink(config: &EngineConfig, seed: u64, ticks: u64) -> VecSink {
    let personas = demo_personas();
    let mut sink = VecSink::default();
    driver::run(config, &personas, seed, ticks, &mut sink);
    sink
}

fn speak_counts(sink: &VecSink) -> BTreeMap<PersonaId, u64> {
    let mut counts = BTreeMap::from([
        ("friend".to_string(), 0),
        ("chaos".to_string(), 0),
        ("summarizer".to_string(), 0),
    ]);

    for record in &sink.records {
        if let Some(chosen) = &record.chosen {
            if let Some(count) = counts.get_mut(chosen) {
                *count += 1;
            }
        }
    }

    counts
}

fn final_silence_count(sink: &VecSink) -> u64 {
    sink.records.last().map_or(0, |record| record.silence_count)
}

#[test]
fn higher_mu_personas_speak_more_often() {
    let sink = run_to_sink(&base_config(0.65, 60.0), SEED, TICKS);
    let counts = speak_counts(&sink);

    assert!(counts["friend"] > counts["chaos"]);
    assert!(counts["chaos"] > counts["summarizer"]);
}

#[test]
fn higher_theta_produces_more_silence() {
    let low_theta = run_to_sink(&base_config(0.40, 60.0), SEED, TICKS);
    let high_theta = run_to_sink(&base_config(0.78, 60.0), SEED, TICKS);

    assert!(final_silence_count(&high_theta) > final_silence_count(&low_theta));
}

#[test]
fn large_k_allows_more_non_leader_wins_when_balance_is_isolated() {
    let candidates = ["a", "b", "c", "d"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<PersonaId>>();
    let intensities = BTreeMap::from([
        ("a".to_string(), 1.0),
        ("b".to_string(), 0.9),
        ("c".to_string(), 0.8),
        ("d".to_string(), 0.7),
    ]);
    let history = Vec::new();

    // 분산은 balance가 담당, k는 순간 미세조정 (플랜 §2 기준 c).
    let small_k = nonleader_wins(0.5, &candidates, &intensities, &history);
    let large_k = nonleader_wins(1_000.0, &candidates, &intensities, &history);

    assert!(large_k > small_k);
}

#[test]
fn rrf_reason_is_populated_for_every_gate_passed_record() {
    let sink = run_to_sink(&base_config(0.65, 60.0), SEED, TICKS);
    let known_reasons = ["intensity", "balance", "random"];

    for record in sink.records.iter().filter(|record| record.gate_passed) {
        assert!(record.chosen.is_some());
        assert!(record
            .rrf_reason
            .as_deref()
            .is_some_and(|reason| known_reasons.contains(&reason)));
    }
}

// Criterion 4 is intentionally not asserted in v0.1: alpha=0 keeps the
// length/seed distribution near-deterministic, so it would be flaky or weak.

fn nonleader_wins(
    k: f64,
    candidates: &[PersonaId],
    intensities: &BTreeMap<PersonaId, f64>,
    history: &[salon::model::Event],
) -> usize {
    (0..200)
        .filter(|seed| {
            let mut rng = ChaCha8Rng::seed_from_u64(*seed);
            rrf::select(candidates, intensities, history, k, &mut rng).chosen != "a"
        })
        .count()
}
