use salon::driver;
use salon::headless::HeadlessSink;
use salon::model::{EngineConfig, Persona, PersonaId};
use salon::sweep;
use salon::tui::TuiSink;
use std::collections::BTreeMap;
use std::env;
use std::io;
use std::process;
use std::time::Duration;

const DEFAULT_SEED: u64 = 42;
const DEFAULT_TICKS: u64 = 200;
// Tunable starting points for v0.1 observation, not fixed product defaults.
// theta는 두 talker(μ 0.80, 0.70)는 통과시키고 quiet(μ 0.25)은 막되,
// 발화 직후 억제로 둘이 동시에 theta 아래로 떨어지는 침묵 구간이 생기도록 잡았다.
const DEFAULT_BETA: f64 = 0.5;
const DEFAULT_THETA: f64 = 0.65;
const DEFAULT_K: f64 = 60.0;
const DEFAULT_DELAY_MS: u64 = 150;
const TICK_INTERVAL: f64 = 1.0;

#[derive(Debug, Clone, PartialEq)]
struct Cli {
    headless: bool,
    sweep: bool,
    seed: u64,
    ticks: u64,
    beta: f64,
    theta: f64,
    k: f64,
    delay_ms: u64,
}

fn main() {
    let cli = match parse_args(env::args().skip(1)) {
        Ok(cli) => cli,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{}", usage());
            process::exit(1);
        }
    };

    let config = EngineConfig {
        beta: cli.beta,
        theta: cli.theta,
        k: cli.k,
        tick_interval: TICK_INTERVAL,
    };
    let personas = demo_personas();

    if cli.sweep {
        sweep::run(cli.seed, cli.ticks);
        return;
    }

    if cli.headless {
        let stdout = io::stdout();
        let mut sink = HeadlessSink::new(stdout.lock());
        driver::run(&config, &personas, cli.seed, cli.ticks, &mut sink);
        return;
    }

    let names = persona_names(&personas);
    let mut sink = match TuiSink::new(names, config.theta, Duration::from_millis(cli.delay_ms)) {
        Ok(sink) => sink,
        Err(error) => {
            eprintln!("failed to start TUI: {error}");
            eprintln!("Try `salon --headless` for non-interactive NDJSON output.");
            process::exit(1);
        }
    };
    driver::run(&config, &personas, cli.seed, cli.ticks, &mut sink);
}

fn parse_args<I>(args: I) -> Result<Cli, String>
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli {
        headless: false,
        sweep: false,
        seed: DEFAULT_SEED,
        ticks: DEFAULT_TICKS,
        beta: DEFAULT_BETA,
        theta: DEFAULT_THETA,
        k: DEFAULT_K,
        delay_ms: DEFAULT_DELAY_MS,
    };
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => cli.headless = true,
            "--sweep" => cli.sweep = true,
            "--seed" => cli.seed = parse_u64_arg("--seed", args.next())?,
            "--ticks" => cli.ticks = parse_u64_arg("--ticks", args.next())?,
            "--beta" => cli.beta = parse_f64_arg("--beta", args.next())?,
            "--theta" => cli.theta = parse_f64_arg("--theta", args.next())?,
            "--k" => cli.k = parse_f64_arg("--k", args.next())?,
            "--delay-ms" => cli.delay_ms = parse_u64_arg("--delay-ms", args.next())?,
            "-h" | "--help" => return Err(usage().to_string()),
            unknown => return Err(format!("unknown argument: {unknown}")),
        }
    }

    Ok(cli)
}

fn parse_u64_arg(flag: &str, value: Option<String>) -> Result<u64, String> {
    let raw = value.ok_or_else(|| format!("missing value for {flag}"))?;
    raw.parse::<u64>()
        .map_err(|_| format!("invalid value for {flag}: {raw}"))
}

fn parse_f64_arg(flag: &str, value: Option<String>) -> Result<f64, String> {
    let raw = value.ok_or_else(|| format!("missing value for {flag}"))?;
    raw.parse::<f64>()
        .map_err(|_| format!("invalid value for {flag}: {raw}"))
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

fn persona_names(personas: &[Persona]) -> BTreeMap<PersonaId, String> {
    personas
        .iter()
        .map(|persona| (persona.id.clone(), persona.name.clone()))
        .collect()
}

fn usage() -> &'static str {
    "Usage: salon [--headless] [--sweep] [--seed <u64>] [--ticks <u64>] [--theta <f64>] [--k <f64>] [--beta <f64>] [--delay-ms <u64>]"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headless_defaults() {
        let args = vec!["--headless".to_string()];

        assert_eq!(
            parse_args(args),
            Ok(Cli {
                headless: true,
                sweep: false,
                seed: DEFAULT_SEED,
                ticks: DEFAULT_TICKS,
                beta: DEFAULT_BETA,
                theta: DEFAULT_THETA,
                k: DEFAULT_K,
                delay_ms: DEFAULT_DELAY_MS,
            })
        );
    }

    #[test]
    fn parses_seed_and_ticks() {
        let args = vec![
            "--headless".to_string(),
            "--seed".to_string(),
            "7".to_string(),
            "--ticks".to_string(),
            "12".to_string(),
        ];

        assert_eq!(
            parse_args(args),
            Ok(Cli {
                headless: true,
                sweep: false,
                seed: 7,
                ticks: 12,
                beta: DEFAULT_BETA,
                theta: DEFAULT_THETA,
                k: DEFAULT_K,
                delay_ms: DEFAULT_DELAY_MS,
            })
        );
    }

    #[test]
    fn parses_tuning_knob_overrides() {
        let args = vec![
            "--headless".to_string(),
            "--theta".to_string(),
            "0.4".to_string(),
            "--k".to_string(),
            "30".to_string(),
            "--beta".to_string(),
            "0.3".to_string(),
            "--delay-ms".to_string(),
            "25".to_string(),
        ];

        let cli = parse_args(args).expect("valid args");
        assert!(!cli.sweep);
        assert_eq!(cli.theta, 0.4);
        assert_eq!(cli.k, 30.0);
        assert_eq!(cli.beta, 0.3);
        assert_eq!(cli.delay_ms, 25);
    }

    #[test]
    fn rejects_invalid_float_value() {
        let args = vec![
            "--headless".to_string(),
            "--theta".to_string(),
            "abc".to_string(),
        ];

        assert!(parse_args(args).is_err());
    }

    #[test]
    fn parses_sweep_flag() {
        let args = vec!["--sweep".to_string(), "--seed".to_string(), "9".to_string()];

        let cli = parse_args(args).expect("valid args");
        assert!(cli.sweep);
        assert!(!cli.headless);
        assert_eq!(cli.seed, 9);
    }
}
