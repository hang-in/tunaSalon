use crate::gate::{self, GateResult};
use crate::hawkes::HawkesEngine;
use crate::model::{EngineConfig, EngineState, Persona, PersonaId};
use crate::rrf;
use crate::sink::{ObservationRecord, ObservationSink};
use crate::utterance;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

pub fn run(
    config: &EngineConfig,
    personas: &[Persona],
    seed: u64,
    ticks: u64,
    sink: &mut dyn ObservationSink,
) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut state = initial_state(personas, seed);
    let mut silence_count = 0;
    let mut speak_count = 0;

    for tick in 0..ticks {
        state.intensities = HawkesEngine::update_intensities(&state, 1, config, personas);
        state.excitations = HawkesEngine::decay_excitations(
            &state.excitations,
            1,
            config.beta,
            config.tick_interval,
        );
        let combined_intensities =
            HawkesEngine::combined_intensities(&state.intensities, &state.excitations, personas);
        let intensity_snapshot = combined_intensities.clone();

        let (gate_passed, candidates, chosen, rrf_reason) =
            match gate::evaluate(&combined_intensities, config.theta) {
                GateResult::Candidates(candidates) => {
                    let selection = rrf::select(
                        &candidates,
                        &combined_intensities,
                        &state.history,
                        config.k,
                        &mut rng,
                    );
                    let utterance = utterance::make_utterance(
                        &selection.chosen,
                        tick,
                        config.tick_interval,
                        false,
                        &mut rng,
                    );

                    state.history.push(utterance.event);
                    suppress_chosen(&mut state, personas, &selection.chosen);
                    HawkesEngine::apply_excitation_on_speak(
                        &mut state.excitations,
                        &config.alpha,
                        &selection.chosen,
                        personas,
                    );
                    state.last_speaker = Some(selection.chosen.clone());
                    speak_count += 1;

                    (
                        true,
                        candidates,
                        Some(selection.chosen),
                        Some(selection.reason),
                    )
                }
                GateResult::Silent => {
                    silence_count += 1;
                    (false, Vec::new(), None, None)
                }
            };

        let conversation_len = speak_count + silence_count;
        let record = ObservationRecord {
            tick,
            ts: tick as f64 * config.tick_interval,
            intensities: intensity_snapshot,
            gate_passed,
            candidates,
            chosen,
            rrf_reason,
            silence_count,
            speak_count,
            conversation_len,
        };

        sink.emit(&record);
    }

    sink.finish();
}

fn initial_state(personas: &[Persona], seed: u64) -> EngineState {
    let intensities = personas
        .iter()
        .map(|persona| (persona.id.clone(), persona.base_rate))
        .collect::<BTreeMap<PersonaId, f64>>();

    EngineState {
        intensities,
        excitations: BTreeMap::new(),
        history: Vec::new(),
        last_speaker: None,
        rng_seed: seed,
    }
}

fn suppress_chosen(state: &mut EngineState, personas: &[Persona], chosen: &PersonaId) {
    if let Some(persona) = personas.iter().find(|persona| &persona.id == chosen) {
        state.intensities.insert(
            chosen.clone(),
            HawkesEngine::suppressed_after_speak(persona.base_rate),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::VecSink;

    fn config() -> EngineConfig {
        EngineConfig {
            beta: 0.5,
            theta: 0.5,
            k: 60.0,
            tick_interval: 1.0,
            alpha: crate::model::CouplingMatrix::default(),
        }
    }

    fn personas() -> Vec<Persona> {
        vec![
            Persona {
                id: "friend".to_string(),
                name: "Friend".to_string(),
                base_rate: 0.8,
            },
            Persona {
                id: "chaos".to_string(),
                name: "Chaos".to_string(),
                base_rate: 0.7,
            },
            Persona {
                id: "summarizer".to_string(),
                name: "Summarizer".to_string(),
                base_rate: 0.25,
            },
        ]
    }

    #[test]
    fn fixed_seed_produces_identical_records() {
        let config = config();
        let personas = personas();
        let mut left = VecSink::default();
        let mut right = VecSink::default();

        run(&config, &personas, 42, 100, &mut left);
        run(&config, &personas, 42, 100, &mut right);

        assert_eq!(left.records, right.records);
        assert_eq!(left.records.len(), 100);
    }
}
