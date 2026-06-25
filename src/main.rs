use salon::chat::ChatApp;
use salon::driver;
use salon::headless::HeadlessSink;
use salon::live::{LiveSession, PersonaMeta};
use salon::model::{CouplingMatrix, EngineConfig, Persona, PersonaId, PersonaModifier};
use salon::persona_kit::{assemble, Blood, Mbti, Role, Zodiac};
use salon::pool::{BackendConfig, BackendPool};
use salon::preset::RoomPreset;
use salon::runtime::FakeBackend;
use salon::sweep;
use salon::tui::TuiSink;
use std::collections::BTreeMap;
use std::env;
use std::io;
use std::process;
use std::sync::{Arc, OnceLock};
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
const DEFAULT_ROOM_ID: &str = "salon";
const FRIEND_ENDPOINT: &str = "http://yongseek.iptime.org:8008";

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
    // 인터랙티브 채팅 TUI 모드 (기본 false; 실제 터미널 필요)
    chat: bool,
    // web 프런트엔드 모드 (axum WebSocket 서버)
    web: bool,
    port: u16,
    host: String,
    room_id: String,
    topic: Vec<String>,
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

    // --web: axum WebSocket 서버(엔진 이벤트 push + 사람 입력 수신).
    // web feature off 빌드에서도 main.rs가 컴파일되도록 cfg 분기 필수.
    if cli.web {
        #[cfg(feature = "web")]
        {
            use salon::roomstore::RoomStore;
            use std::sync::Arc;

            let chat_personas = chat_personas();
            let mut chat_config = RoomPreset::Pub
                .build_config_with_modifiers(&chat_personas, &demo_persona_modifiers());
            chat_config.theta = cli.theta.unwrap_or(0.60);
            if let Some(beta) = cli.beta {
                chat_config.beta = beta;
            }
            chat_config.forbid_self_repeat = true;
            let pool = Arc::new(build_demo_room_pool());
            let rooms_db_path = RoomStore::default_rooms_db_path();
            let session_factory: salon::web::WebSessionFactory = {
                let chat_config = chat_config.clone();
                let chat_personas = chat_personas.clone();
                let pool = pool.clone();
                Arc::new(move |room_id: String| {
                    let store = rooms_db_path
                        .as_ref()
                        .and_then(|p| match RoomStore::open(p) {
                            Ok(s) => Some(s),
                            Err(e) => {
                                eprintln!("[tunaSalon] rooms.db 열기 실패(영속 off): {e}");
                                None
                            }
                        });

                    let snap_opt = store.as_ref().and_then(|s| s.load(&room_id).ok().flatten());

                    let session =
                        if let Some(snap) = snap_opt.filter(|s| !s.participants.is_empty()) {
                            let n_personas = snap.participants.len();
                            let n_messages = snap.messages.len();
                            let mut sess = LiveSession::with_store_for_room(
                                chat_config.clone(),
                                vec![],
                                cli.seed,
                                pool.clone(),
                                "나",
                                salon::memory::live_store(),
                                room_id.clone(),
                            )
                            .with_target_rho(RoomPreset::Pub.target_rho());
                            for (mut p, mut m) in snap.participants {
                                apply_default_persona_profile(&mut p, &mut m);
                                sess.add_persona(p, m);
                            }
                            sess.restore_history(snap.messages, snap.tick_count);
                            sess.set_topics(snap.topics);
                            eprintln!(
                                "[tunaSalon] 방 '{}' 복원: {n_personas}명, {n_messages}발화",
                                room_id
                            );
                            sess
                        } else {
                            LiveSession::with_store_for_room(
                                chat_config.clone(),
                                chat_personas.clone(),
                                cli.seed,
                                pool.clone(),
                                "나",
                                salon::memory::live_store(),
                                room_id,
                            )
                            .with_target_rho(RoomPreset::Pub.target_rho())
                            .with_persona_meta(build_demo_persona_meta())
                        };
                    (session, store)
                })
            };

            let startup = salon::web::WebStartup::debate(cli.topic.clone());
            salon::web::serve_multi(
                &cli.host,
                cli.port,
                cli.room_id.clone(),
                startup,
                session_factory,
            );
        }
        #[cfg(not(feature = "web"))]
        {
            eprintln!("--web은 `cargo run --features web -- --web`로 빌드/실행해야 합니다.");
            eprintln!("(web 프런트는 먼저 `cd web && pnpm install && pnpm build`로 web/dist 생성)");
        }
        return;
    }

    // --chat: 데모 룸 풀(cloud + friend) + LiveSession + ChatApp.
    // headless/--llm/기본 경로와 완전히 분리된 모드이므로 최우선 처리.
    if cli.chat {
        // 채팅방 = "생동감 있는 3-way". 헤드리스/골든 경로(위 config/personas)와 분리된
        // 자체 페르소나(chat_personas, μ 0.70/0.62/0.55)와 엔진 config를 쓴다.
        // 기본 = Pub 교차자극 + theta 0.60 + 같은 화자 2연속 금지(자기 말 받아치기 방지).
        // --theta/--beta는 존중. 반복 테스트 튜닝: friend~43%/realist~32%/summarizer~23%, self-repeat 0.
        let chat_personas = chat_personas();
        let mut chat_config =
            RoomPreset::Pub.build_config_with_modifiers(&chat_personas, &demo_persona_modifiers());
        chat_config.theta = cli.theta.unwrap_or(0.60);
        if let Some(beta) = cli.beta {
            chat_config.beta = beta;
        }
        chat_config.forbid_self_repeat = true;
        let theta = chat_config.theta;
        let names = persona_names(&chat_personas);
        let pool = Arc::new(build_demo_room_pool());
        let session = LiveSession::with_store(
            chat_config,
            chat_personas,
            cli.seed,
            pool,
            "나",
            salon::memory::live_store(),
        )
        .with_target_rho(RoomPreset::Pub.target_rho())
        .with_persona_meta(build_demo_persona_meta());
        match ChatApp::new(session, names, theta) {
            Ok(mut app) => {
                let _ = app.run();
            }
            Err(e) => {
                eprintln!("채팅 TUI를 시작할 수 없습니다: {e}");
                eprintln!(
                    "실제 터미널에서 실행하세요. (비대화형이면 cargo run --example chat_demo)"
                );
                process::exit(1);
            }
        }
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

        // 로컬 모델 적재 금지(맥북 RAM/Metal 랙). localhost 데몬은 ":cloud" 아닌 모델을
        // 로컬로 적재하므로 거부한다. cloud(:cloud, 원격 프록시) 또는 원격 --ollama-host(https)만 허용.
        let local_daemon = endpoint.contains("localhost") || endpoint.contains("127.0.0.1");
        if local_daemon && !cli.model.ends_with(":cloud") {
            eprintln!(
                "error: 로컬 모델 '{}'은 localhost 데몬이 RAM/Metal에 적재해 랙을 유발합니다(로컬 ollama 금지).",
                cli.model
            );
            eprintln!("cloud 모델(예: --model gemma4:31b-cloud)을 쓰거나 원격 --ollama-host를 지정하세요.");
            process::exit(1);
        }

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
            driver::run(
                &config, &personas, cli.seed, cli.ticks, &mut sink, &mut pool,
            );
            return;
        }

        let names = persona_names(&personas);
        let mut sink = match TuiSink::new(names, config.theta, Duration::from_millis(cli.delay_ms))
        {
            Ok(sink) => sink,
            Err(error) => {
                eprintln!("failed to start TUI: {error}");
                eprintln!("Try `salon --headless` for non-interactive NDJSON output.");
                process::exit(1);
            }
        };
        driver::run(
            &config, &personas, cli.seed, cli.ticks, &mut sink, &mut pool,
        );
        return;
    }

    // FakeBackend 경로 (기본, --llm 없음)
    if cli.headless {
        let stdout = io::stdout();
        let mut sink = HeadlessSink::new(stdout.lock());
        driver::run(
            &config,
            &personas,
            cli.seed,
            cli.ticks,
            &mut sink,
            &mut FakeBackend,
        );
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
    driver::run(
        &config,
        &personas,
        cli.seed,
        cli.ticks,
        &mut sink,
        &mut FakeBackend,
    );
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
        // 기본은 cloud 모델(원격 프록시, 로컬 RAM 0). 로컬 ollama는 맥북 랙으로 금지.
        model: "gemma4:31b-cloud".to_string(),
        ollama_host: None,
        chat: false,
        web: false,
        port: 8080,
        host: "0.0.0.0".to_string(),
        room_id: DEFAULT_ROOM_ID.to_string(),
        topic: vec![],
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
                let raw = args
                    .next()
                    .ok_or_else(|| "missing value for --room".to_string())?;
                cli.room = Some(raw);
            }
            "--llm" => cli.llm = true,
            "--chat" => cli.chat = true,
            "--web" => cli.web = true,
            "--port" => cli.port = parse_u64_arg("--port", args.next())? as u16,
            "--host" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "missing value for --host".to_string())?;
                cli.host = raw;
            }
            "--room-id" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "missing value for --room-id".to_string())?;
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Err("invalid value for --room-id: empty".to_string());
                }
                cli.room_id = trimmed.to_string();
            }
            "--topic" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "missing value for --topic".to_string())?;
                let parsed = parse_topic_arg(&raw)?;
                cli.topic.extend(parsed);
                if cli.topic.len() > 5 {
                    cli.topic.truncate(5);
                }
            }
            "--model" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "missing value for --model".to_string())?;
                cli.model = raw;
            }
            "--ollama-host" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "missing value for --ollama-host".to_string())?;
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

fn parse_topic_arg(raw: &str) -> Result<Vec<String>, String> {
    let topics: Vec<String> = raw
        .split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect();
    if topics.is_empty() {
        return Err("invalid value for --topic: empty".to_string());
    }
    Ok(topics)
}

fn demo_personas() -> Vec<Persona> {
    vec![
        Persona {
            id: "friend".to_string(),
            name: "Friendly Regular".to_string(),
            base_rate: 0.80,
        },
        // id "chaos"는 골든 보존 위해 유지. 역할/표시는 realist로 교체(2026-06-03).
        Persona {
            id: "chaos".to_string(),
            name: "Grounded Realist".to_string(),
            base_rate: 0.70,
        },
        Persona {
            id: "summarizer".to_string(),
            name: "Quiet Summarizer".to_string(),
            base_rate: 0.25,
        },
    ]
}

#[derive(Debug, Clone, Copy)]
struct DefaultPersonaSeed {
    id: &'static str,
    role: Role,
    mbti: Mbti,
    blood: Blood,
    zodiac: Zodiac,
}

fn default_persona_seeds() -> [DefaultPersonaSeed; 3] {
    [
        DefaultPersonaSeed {
            id: "friend",
            role: Role::Friend,
            mbti: Mbti::Enfp,
            blood: Blood::B,
            zodiac: Zodiac::Leo,
        },
        // 내부 id는 기존 저장/라우팅 호환을 위해 "chaos"로 유지하되, 역할과 표시명은 realist 축을 쓴다.
        DefaultPersonaSeed {
            id: "chaos",
            role: Role::Realist,
            mbti: Mbti::Intj,
            blood: Blood::O,
            zodiac: Zodiac::Capricorn,
        },
        DefaultPersonaSeed {
            id: "summarizer",
            role: Role::Summarizer,
            mbti: Mbti::Isfj,
            blood: Blood::Ab,
            zodiac: Zodiac::Pisces,
        },
    ]
}

fn default_persona_seed(id: &str) -> Option<DefaultPersonaSeed> {
    default_persona_seeds()
        .into_iter()
        .find(|seed| seed.id == id)
}

fn default_persona_name(seed: DefaultPersonaSeed) -> String {
    assemble(seed.role, seed.mbti, seed.blood, seed.zodiac, "")
        .persona
        .name
}

fn default_persona_axes(seed: DefaultPersonaSeed) -> salon::live::PersonaAxes {
    salon::live::PersonaAxes {
        blood: seed.blood.code().to_string(),
        mbti: seed.mbti.code().to_string(),
        zodiac: seed.zodiac.abbreviation().to_string(),
        role: seed.role.key().to_string(),
    }
}

fn apply_default_persona_profile(persona: &mut Persona, meta: &mut PersonaMeta) {
    let Some(seed) = default_persona_seed(&persona.id) else {
        return;
    };
    let prompts = demo_persona_system_prompts();
    let modifiers = demo_persona_modifiers();
    persona.name = default_persona_name(seed);
    meta.system_prompt = prompts.get(&persona.id).cloned().unwrap_or_default();
    meta.modifier = modifiers.get(&persona.id).cloned().unwrap_or_default();
    meta.axes = Some(default_persona_axes(seed));
}

/// 채팅방(`--chat`) 전용 페르소나. 헤드리스/골든은 `demo_personas`(canonical, dev 회귀용)를
/// 그대로 쓰고, 채팅은 "생동감 있는 3-way"를 위해 μ 간격을 좁힌 이 세트를 쓴다(골든 불침투).
///
/// 반복 테스트 튜닝(Pub 교차자극 + theta 0.60 + forbid_self_repeat 기준):
/// friend~43% / realist~32% / summarizer~23%, 침묵 리듬 유지(maxsil~3), 자기연속 0.
/// id/이름은 demo와 동일(프롬프트·모디파이어 공유), base_rate만 다르다.
fn chat_personas() -> Vec<Persona> {
    let base_rates = BTreeMap::from([("friend", 0.70), ("chaos", 0.62), ("summarizer", 0.55)]);
    default_persona_seeds()
        .into_iter()
        .map(|seed| Persona {
            id: seed.id.to_string(),
            name: default_persona_name(seed),
            base_rate: base_rates.get(seed.id).copied().unwrap_or(0.55),
        })
        .collect()
}

/// 데모 3인(friend / chaos / summarizer)의 역할 기반 system prompt를 반환한다.
/// 웹/채팅 경로는 이제 잡담방이 아니라 토론방으로 다룬다.
/// 응답 언어는 시스템 로케일(`$LANG`)에서 감지, 기본 한국어(salon::locale).
fn demo_persona_system_prompts() -> BTreeMap<PersonaId, String> {
    let lang = salon::locale::reply_language();
    let friend_name = default_persona_seed("friend")
        .map(default_persona_name)
        .unwrap_or_else(|| "Friendly Regular".to_string());
    let realist_name = default_persona_seed("chaos")
        .map(default_persona_name)
        .unwrap_or_else(|| "Grounded Realist".to_string());
    let summarizer_name = default_persona_seed("summarizer")
        .map(default_persona_name)
        .unwrap_or_else(|| "Quiet Summarizer".to_string());
    // 공통 꼬리말: 언어 지시 + 토론 가드레일.
    let common = format!(
        " You are in a live debate room, not casual small talk. A real person participates as \"나\". Always respond in {lang}, even if others write in another language. Take a clear position on the current topic only, and explicitly connect to at least one real participant by nickname when agreeing, rebutting, or refining their point. Do not write \"나님\"; refer to the human as \"사용자님\" only when direct address is necessary. Do not address Moderator, system-like progress lines, or 나 unless you are answering a fresh user message from 나. Use prior memory only when it directly matches the current topic; never import old topic terms just because a broad word overlaps. Write 3-6 substantial sentences: claim, reason, and consequence or counterexample. If you are repeating an earlier position, add a new concrete example, metric, legal threshold, implementation mechanism, or compromise test. Use real-world cases or standards only when they are genuinely relevant to the current topic. Do not expose chain-of-thought or hidden reasoning; give only the final argument. Avoid generic greetings, seasonal chatter, therapy language, excessive praise, emojis, and laughter. Some recent lines may be YOUR OWN earlier messages; never praise or react to your own line as if someone else said it."
    );
    let mut m = BTreeMap::new();
    m.insert(
        "friend".to_string(),
        format!("You are {friend_name}, a collaborative civic-benefit advocate. You tend to defend human agency, accessibility, transparency, due process, and practical community governance, while still acknowledging real risks. Push the debate forward by asking what design or policy would protect people without freezing participation or innovation.{common}"),
    );
    // id는 "chaos"로 유지(골든 보존)하되 역할은 realist로 교체(사용자 결정 2026-06-03).
    m.insert(
        "chaos".to_string(),
        format!("You are {realist_name}, a skeptical implementation-first realist. You emphasize incentives, enforcement cost, liability, abuse cases, institutional capture, operational failure modes, and concrete thresholds. Challenge vague optimism and ask what can actually be audited, funded, or enforced for this topic.{common}"),
    );
    m.insert(
        "summarizer".to_string(),
        format!("You are {summarizer_name}, a synthesis-focused debate participant. You are not passive: identify the strongest point from each side, name the hidden disagreement, and propose a sharper framing or compromise test. When needed, call out where someone skipped evidence or changed the premise.{common}"),
    );
    m
}

fn env_flag(name: &str, default: bool) -> bool {
    env::var(name)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            !(value.is_empty()
                || value == "0"
                || value == "false"
                || value == "off"
                || value == "no")
        })
        .unwrap_or(default)
}

fn env_duration_secs(name: &str, default: u64) -> Duration {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(default))
}

fn friend_backend_available(endpoint: &str) -> bool {
    if env_flag("SALON_SKIP_FRIEND_HEALTHCHECK", false) {
        return true;
    }
    let timeout = env_duration_secs("SALON_FRIEND_HEALTHCHECK_SECS", 5);
    let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
    let client = match reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            eprintln!("[tunaSalon] friend backend healthcheck client failed: {e}");
            return false;
        }
    };
    match client.get(&url).send() {
        Ok(resp) if resp.status().is_success() => true,
        Ok(resp) => {
            eprintln!(
                "[tunaSalon] friend backend healthcheck failed: {url} -> {}",
                resp.status()
            );
            false
        }
        Err(e) => {
            eprintln!("[tunaSalon] friend backend unavailable; using cloud-only routing: {e}");
            false
        }
    }
}

fn use_friend_backend() -> bool {
    static FRIEND_ENABLED: OnceLock<bool> = OnceLock::new();
    *FRIEND_ENABLED.get_or_init(|| {
        if env_flag("SALON_CLOUD_ONLY", false) {
            return false;
        }
        friend_backend_available(FRIEND_ENDPOINT)
    })
}

/// --room 사용 시 적용되는 데모 모디파이어.
fn demo_persona_modifiers() -> BTreeMap<PersonaId, PersonaModifier> {
    let mut m = BTreeMap::new();
    // realist: 과장·비현실에 반응(중간 reactivity), 도발은 낮게. (id는 "chaos" 유지)
    m.insert(
        "chaos".to_string(),
        PersonaModifier {
            reactivity: 1.0,
            provocativeness: 0.8,
        },
    );
    m.insert(
        "friend".to_string(),
        PersonaModifier {
            reactivity: 2.0,
            provocativeness: 1.0,
        },
    );
    m.insert(
        "summarizer".to_string(),
        PersonaModifier {
            reactivity: 1.0,
            provocativeness: 0.5,
        },
    );
    m
}

/// --chat 및 chat_demo 공용 데모 룸 풀을 빌드한다.
///
/// 구성:
///   - cloud  : Ollama(gemma4:31b-cloud, localhost:11434, cap=1, num_ctx=None, thinking=true)
///   - friend : OpenAI(qwen3.6-35b-fast, yongseek.iptime.org:8008, cap=2, max_tokens=2048, thinking=true)
///   - 양쪽에 demo_persona_system_prompts() 적용.
///   - default = "friend"(qwen, 2명: friend/chaos), summarizer → "cloud"(gemma, 1명) 라우팅, 상호 폴백.
///   - `SALON_CLOUD_ONLY` 설정 시: friend 백엔드/라우팅/폴백을 건너뛰고 cloud(cap=1)만.
///     지인 vLLM 서버가 죽었을 때 라이브 테스트용(비파괴적 — 토글만 끄면 원복).
///
/// 토론 모드(2026-06-25 사용자): 속도보다 논증 품질을 우선해 reasoning/thinking을 기본 활성화한다.
/// `SALON_DEBATE_THINKING=0`이면 빠른 검증용으로 끌 수 있다.
///
/// SECURITY: api_key 없음(cloud는 localhost 프록시, friend는 내부망 서버).
fn build_demo_room_pool() -> BackendPool {
    let mut pool = BackendPool::new();
    let debate_thinking = env_flag("SALON_DEBATE_THINKING", true);

    // cloud 백엔드: Ollama, gemma4:31b-cloud, cap=1, num_ctx=None(원격 auto-max).
    // 동시성 1(사용자 결정 2026-06-03): cloud rate 보수적, 부하는 friend(qwen)로.
    let mut cloud_cfg = BackendConfig::new(
        "cloud",
        "gemma4:31b-cloud",
        "http://localhost:11434",
        None,
        1,
        None,
        Duration::from_secs(180),
    );
    cloud_cfg.thinking = debate_thinking;
    pool.add(cloud_cfg, demo_persona_system_prompts());

    // SALON_CLOUD_ONLY: 지인(friend) vLLM 서버가 죽었을 때 cloud만으로 라이브 테스트.
    // friend 백엔드/라우팅/폴백을 통째로 건너뛴다(서버 복구 시 토글만 끄면 원복).
    if !use_friend_backend() {
        pool.set_default("cloud");
        return pool;
    }

    // friend 백엔드: OpenAI 호환(vLLM), qwen3.6-35b-fast, cap=2, max_tokens=2048.
    // 동시성 2(사용자 결정 2026-06-03): 지인 vLLM 서버에 더 많은 부하 배분.
    // 주의: 지인 vLLM은 vllm-swap(한 번에 한 모델). 모델 전환 시 첫 발화가 swap-in으로 ~2.5분
    //   걸릴 수 있어 timeout 240s로 견딘다(swap 후 발화는 빠름). 다른 용도와 공유 시 재swap 가능.
    let mut friend_cfg = BackendConfig::new_openai(
        "friend",
        "qwen3.6-35b-fast",
        FRIEND_ENDPOINT,
        None,
        2,
        Some(2048),
        Duration::from_secs(240),
    );
    friend_cfg.thinking = debate_thinking;
    pool.add(friend_cfg, demo_persona_system_prompts());

    // 라우팅(사용자 결정 2026-06-03): cloud(gemma, cap 1) = 1명(조용한 summarizer),
    // friend(qwen, cap 2) = 2명(friend/chaos). cap 설정과 일관.
    pool.set_default("friend");
    pool.add_route("summarizer", "cloud");
    // 상호 폴백: 한쪽 서버 다운 시 다른 쪽으로.
    pool.set_fallback("friend", "cloud");
    pool.set_fallback("cloud", "friend");

    pool
}

/// `--chat` / `--web` 경로에서 `LiveSession::with_persona_meta`에 전달하는 맵을 빌드한다.
///
/// 라우팅은 `build_demo_room_pool`과 일관성을 유지한다:
///   - summarizer -> "cloud" (gemma4:31b-cloud)
///   - friend / chaos  -> "friend" (qwen3.6-35b)
///
/// SALON_CLOUD_ONLY 환경변수가 설정되면 세 persona 모두 "cloud"로 라우팅한다.
/// `build_demo_room_pool`이 cloud_only 분기에서 friend 백엔드를 통째로 건너뛰는 것과 동일 의도.
///
/// system_prompt는 `demo_persona_system_prompts()`와 동일 값.
/// modifier는 `demo_persona_modifiers()`와 동일 값(없으면 default).
fn build_demo_persona_meta() -> BTreeMap<PersonaId, PersonaMeta> {
    let use_friend = use_friend_backend();

    let prompts = demo_persona_system_prompts();
    let modifiers = demo_persona_modifiers();

    // 라우팅 결정: SALON_CLOUD_ONLY면 전부 cloud, 아니면 build_demo_room_pool 일치.
    let backend_for = |id: &str| -> String {
        if use_friend {
            match id {
                "summarizer" => "cloud".to_string(),
                _ => "friend".to_string(),
            }
        } else {
            "cloud".to_string()
        }
    };

    let ids = ["friend", "chaos", "summarizer"];
    ids.iter()
        .map(|id| {
            let system_prompt = prompts.get(*id).cloned().unwrap_or_default();
            let modifier = modifiers.get(*id).cloned().unwrap_or_default();
            let axes = default_persona_seed(id).map(default_persona_axes);
            (
                id.to_string(),
                PersonaMeta {
                    backend: backend_for(id),
                    system_prompt,
                    modifier,
                    axes,
                },
            )
        })
        .collect()
}

fn persona_names(personas: &[Persona]) -> BTreeMap<PersonaId, String> {
    personas
        .iter()
        .map(|persona| (persona.id.clone(), persona.name.clone()))
        .collect()
}

fn usage() -> &'static str {
    "Usage: salon [--headless] [--sweep] [--fsm] [--seed <u64>] [--ticks <u64>] [--theta <f64>] [--k <f64>] [--beta <f64>] [--delay-ms <u64>] [--room <calm|pub|argument|chaos>] [--llm] [--model <name>] [--ollama-host <url>] [--chat] [--web] [--port <u16>] [--host <addr>] [--room-id <id>] [--topic <text>[,<text>...]]"
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
                model: "gemma4:31b-cloud".to_string(),
                ollama_host: None,
                chat: false,
                web: false,
                port: 8080,
                host: "0.0.0.0".to_string(),
                room_id: DEFAULT_ROOM_ID.to_string(),
                topic: vec![],
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
                model: "gemma4:31b-cloud".to_string(),
                ollama_host: None,
                chat: false,
                web: false,
                port: 8080,
                host: "0.0.0.0".to_string(),
                room_id: DEFAULT_ROOM_ID.to_string(),
                topic: vec![],
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
    fn parses_room_id_flag() {
        let args = vec![
            "--web".to_string(),
            "--room-id".to_string(),
            "debate-alpha".to_string(),
        ];

        let cli = parse_args(args).expect("valid args");
        assert!(cli.web);
        assert_eq!(cli.room_id, "debate-alpha");
    }

    #[test]
    fn parses_topic_flag() {
        let args = vec![
            "--web".to_string(),
            "--topic".to_string(),
            "AI safety, open source".to_string(),
            "--topic".to_string(),
            "education".to_string(),
        ];

        let cli = parse_args(args).expect("valid args");
        assert_eq!(
            cli.topic,
            vec![
                "AI safety".to_string(),
                "open source".to_string(),
                "education".to_string()
            ]
        );
    }

    #[test]
    fn rejects_empty_topic_value() {
        let args = vec![
            "--web".to_string(),
            "--topic".to_string(),
            " , ".to_string(),
        ];
        assert!(parse_args(args).is_err());
    }

    #[test]
    fn caps_topics_at_five() {
        let args = vec![
            "--web".to_string(),
            "--topic".to_string(),
            "a,b,c,d,e,f".to_string(),
        ];

        let cli = parse_args(args).expect("valid args");
        assert_eq!(cli.topic, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn rejects_missing_room_value() {
        let args = vec!["--headless".to_string(), "--room".to_string()];
        assert!(parse_args(args).is_err());
    }

    #[test]
    fn rejects_empty_room_id_value() {
        let args = vec![
            "--web".to_string(),
            "--room-id".to_string(),
            "   ".to_string(),
        ];
        assert!(parse_args(args).is_err());
    }
}
