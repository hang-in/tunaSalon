use salon::driver;
use salon::headless::HeadlessSink;
use salon::model::{CouplingMatrix, EngineConfig, Persona, PersonaId, PersonaModifier};
use salon::pool::{BackendConfig, BackendPool};
use salon::preset::RoomPreset;
use salon::runtime::FakeBackend;
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
    fsm: bool,
    seed: u64,
    ticks: u64,
    beta: Option<f64>,
    theta: Option<f64>,
    k: Option<f64>,
    delay_ms: u64,
    room: Option<String>,
    // LLM opt-in 플래그 (기본 false → FakeBackend)
    llm: bool,
    model: String,
    ollama_host: Option<String>,
}

fn main() {
    // .env 파일이 있으면 환경 변수로 로드한다. 없거나 실패해도 무시.
    dotenvy::dotenv().ok();

    let cli = match parse_args(env::args().skip(1)) {
        Ok(cli) => cli,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{}", usage());
            process::exit(1);
        }
    };

    let mut personas = demo_personas();

    let mut config = if let Some(ref room_str) = cli.room {
        let preset = match RoomPreset::parse(room_str) {
            Ok(p) => p,
            Err(error) => {
                eprintln!("{error}");
                eprintln!("{}", usage());
                process::exit(1);
            }
        };
        let mu = preset.mu_scale();
        for p in &mut personas {
            p.base_rate *= mu;
        }
        let demo_modifiers = demo_persona_modifiers();
        preset.build_config_with_modifiers(&personas, &demo_modifiers)
    } else {
        EngineConfig {
            beta: cli.beta.unwrap_or(DEFAULT_BETA),
            theta: cli.theta.unwrap_or(DEFAULT_THETA),
            k: cli.k.unwrap_or(DEFAULT_K),
            tick_interval: TICK_INTERVAL,
            alpha: CouplingMatrix::default(),
            forbid_self_repeat: false,
        }
    };

    // 명시적 플래그가 있으면 preset 값을 덮어쓴다 (우선순위: 명시 플래그 > preset > 기본).
    if let Some(beta) = cli.beta {
        config.beta = beta;
    }
    if let Some(theta) = cli.theta {
        config.theta = theta;
    }
    if let Some(k) = cli.k {
        config.k = k;
    }
    if cli.fsm {
        config.forbid_self_repeat = true;
    }

    if cli.sweep {
        sweep::run(cli.seed, cli.ticks);
        return;
    }

    // --llm 없으면 FakeBackend (기본, 골든 보존).
    // --llm 있으면 BackendPool(단일 백엔드) 빌드. v0.3 OllamaBackend 단일 경로와 동일 동작.
    if cli.llm {
        // 앱은 항상 Ollama 데몬(기본 localhost:11434)에 요청한다.
        // cloud 모델은 ":cloud" 이름(예: glm-5.1:cloud)으로 로컬 데몬이 원격 프록시하므로
        // 로컬 RAM을 쓰지 않고, 앱이 키를 직접 다룰 필요도 없다.
        let endpoint = cli
            .ollama_host
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        // 직접 원격(https) 엔드포인트를 가리킬 때만 API 키를 첨부한다. localhost 데몬은 키 불필요.
        // SECURITY: 키 값을 로그/에러/출력에 절대 넣지 않는다.
        let api_key: Option<String> = if endpoint.starts_with("https://") {
            env::var("OLLAMA_CLOUD_API_KEY").ok()
        } else {
            None
        };

        // cloud 모델은 None → auto-max(우리가 num_ctx를 보내면 모델 최대 ctx를 오히려 깎는다).
        // cloud 판정: (1) 직접 원격(https) 엔드포인트, 또는 (2) ":cloud" 모델명(localhost 데몬이 원격 프록시).
        // 그 외 로컬 모델(e4b 등)만 RAM 상한을 위해 8192 ctx 명시.
        let is_cloud = endpoint.starts_with("https://") || cli.model.ends_with(":cloud");
        let num_ctx: Option<u64> = if is_cloud { None } else { Some(8192) };

        // BackendPool 구성: 단일 백엔드 "default" + routing 없음 = 모든 페르소나가 동일 백엔드로.
        // v0.3의 단일 OllamaBackend 경로와 동일 동작이다.
        let cfg = BackendConfig::new(
            "default",
            cli.model.clone(),
            endpoint,
            api_key,
            1, // 라이브 순차 유지(task-23에서 세마포어로 관리)
            num_ctx,
            Duration::from_secs(30),
        );
        let mut pool = BackendPool::new();
        pool.add(cfg, demo_persona_system_prompts());
        pool.set_default("default");

        if cli.headless {
            let stdout = io::stdout();
            let mut sink = HeadlessSink::new(stdout.lock());
            driver::run(&config, &personas, cli.seed, cli.ticks, &mut sink, &mut pool);
            return;
        }

        let names = persona_names(&personas);
        let mut sink =
            match TuiSink::new(names, config.theta, Duration::from_millis(cli.delay_ms)) {
                Ok(sink) => sink,
                Err(error) => {
                    eprintln!("failed to start TUI: {error}");
                    eprintln!("Try `salon --headless` for non-interactive NDJSON output.");
                    process::exit(1);
                }
            };
        driver::run(&config, &personas, cli.seed, cli.ticks, &mut sink, &mut pool);
        return;
    }

    // FakeBackend 경로 (기본, --llm 없음)
    if cli.headless {
        let stdout = io::stdout();
        let mut sink = HeadlessSink::new(stdout.lock());
        driver::run(&config, &personas, cli.seed, cli.ticks, &mut sink, &mut FakeBackend);
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
    driver::run(&config, &personas, cli.seed, cli.ticks, &mut sink, &mut FakeBackend);
}

fn parse_args<I>(args: I) -> Result<Cli, String>
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli {
        headless: false,
        sweep: false,
        fsm: false,
        seed: DEFAULT_SEED,
        ticks: DEFAULT_TICKS,
        beta: None,
        theta: None,
        k: None,
        delay_ms: DEFAULT_DELAY_MS,
        room: None,
        llm: false,
        model: "gemma4:e4b".to_string(),
        ollama_host: None,
    };
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => cli.headless = true,
            "--sweep" => cli.sweep = true,
            "--fsm" => cli.fsm = true,
            "--seed" => cli.seed = parse_u64_arg("--seed", args.next())?,
            "--ticks" => cli.ticks = parse_u64_arg("--ticks", args.next())?,
            "--beta" => cli.beta = Some(parse_f64_arg("--beta", args.next())?),
            "--theta" => cli.theta = Some(parse_f64_arg("--theta", args.next())?),
            "--k" => cli.k = Some(parse_f64_arg("--k", args.next())?),
            "--delay-ms" => cli.delay_ms = parse_u64_arg("--delay-ms", args.next())?,
            "--room" => {
                let raw = args.next().ok_or_else(|| "missing value for --room".to_string())?;
                cli.room = Some(raw);
            }
            "--llm" => cli.llm = true,
            "--model" => {
                let raw = args.next().ok_or_else(|| "missing value for --model".to_string())?;
                cli.model = raw;
            }
            "--ollama-host" => {
                let raw =
                    args.next().ok_or_else(|| "missing value for --ollama-host".to_string())?;
                cli.ollama_host = Some(raw);
            }
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

/// 데모 3인(friend / chaos / summarizer)의 역할 기반 system prompt를 반환한다.
/// 짧고 선명하게 — 작은 모델도 긴 프롬프트는 뭉개므로 1~2문장.
fn demo_persona_system_prompts() -> BTreeMap<PersonaId, String> {
    let mut m = BTreeMap::new();
    m.insert(
        "friend".to_string(),
        "You are a warm, easygoing regular in this group chat. React to the mood and feelings in the conversation with 1-2 short, light sentences. Don't act like a therapist, skip excessive apologies or praise, don't repeat the previous line, and keep it short.".to_string(),
    );
    m.insert(
        "chaos".to_string(),
        "You are a playful chaos-stirrer. Throw in one short, slightly absurd remark that provokes a reaction, then bow out. Don't act like a therapist, skip excessive apologies or praise, don't repeat the previous line, and keep it short.".to_string(),
    );
    m.insert(
        "summarizer".to_string(),
        "You are a quiet observer. Only speak up to tie loose threads together in one brief sentence. Don't act like a therapist, skip excessive apologies or praise, don't repeat the previous line, and keep it short.".to_string(),
    );
    m
}

/// --room 사용 시 적용되는 데모 모디파이어.
fn demo_persona_modifiers() -> BTreeMap<PersonaId, PersonaModifier> {
    let mut m = BTreeMap::new();
    m.insert(
        "chaos".to_string(),
        PersonaModifier { reactivity: 0.6, provocativeness: 2.0 },
    );
    m.insert(
        "friend".to_string(),
        PersonaModifier { reactivity: 2.0, provocativeness: 1.0 },
    );
    m.insert(
        "summarizer".to_string(),
        PersonaModifier { reactivity: 0.5, provocativeness: 0.5 },
    );
    m
}

fn persona_names(personas: &[Persona]) -> BTreeMap<PersonaId, String> {
    personas
        .iter()
        .map(|persona| (persona.id.clone(), persona.name.clone()))
        .collect()
}

fn usage() -> &'static str {
    "Usage: salon [--headless] [--sweep] [--fsm] [--seed <u64>] [--ticks <u64>] [--theta <f64>] [--k <f64>] [--beta <f64>] [--delay-ms <u64>] [--room <calm|pub|argument|chaos>] [--llm] [--model <name>] [--ollama-host <url>]"
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
                fsm: false,
                seed: DEFAULT_SEED,
                ticks: DEFAULT_TICKS,
                beta: None,
                theta: None,
                k: None,
                delay_ms: DEFAULT_DELAY_MS,
                room: None,
                llm: false,
                model: "gemma4:e4b".to_string(),
                ollama_host: None,
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
                fsm: false,
                seed: 7,
                ticks: 12,
                beta: None,
                theta: None,
                k: None,
                delay_ms: DEFAULT_DELAY_MS,
                room: None,
                llm: false,
                model: "gemma4:e4b".to_string(),
                ollama_host: None,
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
        assert_eq!(cli.theta, Some(0.4));
        assert_eq!(cli.k, Some(30.0));
        assert_eq!(cli.beta, Some(0.3));
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

    #[test]
    fn parses_room_flag() {
        let args = vec![
            "--headless".to_string(),
            "--room".to_string(),
            "argument".to_string(),
            "--seed".to_string(),
            "1".to_string(),
        ];
        let cli = parse_args(args).expect("valid args");
        assert_eq!(cli.room, Some("argument".to_string()));
        assert_eq!(cli.seed, 1);
    }

    #[test]
    fn rejects_missing_room_value() {
        let args = vec!["--headless".to_string(), "--room".to_string()];
        assert!(parse_args(args).is_err());
    }
}
