use crate::driver;
use crate::model::{CouplingMatrix, EngineConfig, Persona, PersonaId};
use crate::sink::VecSink;
use std::collections::BTreeMap;

const DEFAULT_BETA: f64 = 0.5;
const TICK_INTERVAL: f64 = 1.0;

pub fn run(seed: u64, ticks: u64) {
    let personas = demo_personas();

    for theta in [0.40, 0.65, 0.78] {
        for k in [2.0, 60.0, 500.0] {
            let config = EngineConfig {
                beta: DEFAULT_BETA,
                theta,
                k,
                tick_interval: TICK_INTERVAL,
                alpha: CouplingMatrix::default(),
            };
            let mut sink = VecSink::default();

            driver::run(&config, &personas, seed, ticks, &mut sink);

            let silence_count = sink.records.last().map_or(0, |record| record.silence_count);
            let counts = speak_counts(&sink, &personas);
            println!(
                "theta={theta:.2} k={k:.1} silence_count={silence_count} friend={} chaos={} summarizer={}",
                counts.get("friend").copied().unwrap_or(0),
                counts.get("chaos").copied().unwrap_or(0),
                counts.get("summarizer").copied().unwrap_or(0),
            );
        }
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

fn speak_counts(sink: &VecSink, personas: &[Persona]) -> BTreeMap<PersonaId, u64> {
    let mut counts = personas
        .iter()
        .map(|persona| (persona.id.clone(), 0))
        .collect::<BTreeMap<PersonaId, u64>>();

    for record in &sink.records {
        if let Some(chosen) = &record.chosen {
            if let Some(count) = counts.get_mut(chosen) {
                *count += 1;
            }
        }
    }

    counts
}
