use salon::chat::ChatApp;
use salon::driver;
use salon::headless::HeadlessSink;
use salon::live::{LiveSession, PersonaMeta};
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
use std::sync::Arc;
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
    // 인터랙티브 채팅 TUI 모드 (기본 false; 실제 터미널 필요)
    chat: bool,
    // web 프런트엔드 모드 (axum WebSocket 서버)
    web: bool,
    port: u16,
    host: String,
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

            let chat_personas = chat_personas();
            let mut chat_config = RoomPreset::Pub
                .build_config_with_modifiers(&chat_personas, &demo_persona_modifiers());
            chat_config.theta = cli.theta.unwrap_or(0.60);
            if let Some(beta) = cli.beta {
                chat_config.beta = beta;
            }
            chat_config.forbid_self_repeat = true;
            let pool = std::sync::Arc::new(build_demo_room_pool());

            // RoomStore 열기 시도. 실패하면 None(영속 off)으로 계속 진행.
            let store = RoomStore::default_rooms_db_path()
                .and_then(|p| match RoomStore::open(&p) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        eprintln!("[tunaSalon] rooms.db 열기 실패(영속 off): {e}");
                        None
                    }
                });

            // 복원 시도
            let snap_opt = store
                .as_ref()
                .and_then(|s| s.load("salon").ok().flatten());

            let session = if let Some(snap) = snap_opt.filter(|s| !s.participants.is_empty()) {
                // 복원 세션: 빈 personas로 시작하고 add_persona로 순서 복원.
                // with_target_rho를 add_persona 루프 전에 설정해야 각 add 시 alpha가 정규화된다.
                let n_personas = snap.participants.len();
                let n_messages = snap.messages.len();
                let mut sess = LiveSession::with_store(
                    chat_config,
                    vec![],
                    cli.seed,
                    pool,
                    "나",
                    salon::memory::live_store(),
                )
                .with_target_rho(RoomPreset::Pub.target_rho());
                for (p, m) in snap.participants {
                    sess.add_persona(p, m);
                }
                sess.restore_history(snap.messages, snap.tick_count);
                sess.set_topics(snap.topics);
                eprintln!(
                    "[tunaSalon] 방 'salon' 복원: {n_personas}명, {n_messages}발화"
                );
                sess
            } else {
                // 기본 3명 세션
                LiveSession::with_store(
                    chat_config,
                    chat_personas,
                    cli.seed,
                    pool,
                    "나",
                    salon::memory::live_store(),
                )
                .with_target_rho(RoomPreset::Pub.target_rho())
                .with_persona_meta(build_demo_persona_meta())
            };

            salon::web::serve(&cli.host, cli.port, session, "나".to_string(), store);
        }
        #[cfg(not(feature = "web"))]
        {
            eprintln!("--web은 `cargo run --features web -- --web`로 빌드/실행해야 합니다.");
            eprintln!(
                "(web 프런트는 먼저 `cd web && pnpm install && pnpm build`로 web/dist 생성)"
            );
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
        let mut chat_config = RoomPreset::Pub
            .build_config_with_modifiers(&chat_personas, &demo_persona_modifiers());
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
                eprintln!("실제 터미널에서 실행하세요. (비대화형이면 cargo run --example chat_demo)");
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
        // 기본은 cloud 모델(원격 프록시, 로컬 RAM 0). 로컬 ollama는 맥북 랙으로 금지.
        model: "gemma4:31b-cloud".to_string(),
        ollama_host: None,
        chat: false,
        web: false,
        port: 8080,
        host: "0.0.0.0".to_string(),
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
            "--chat" => cli.chat = true,
            "--web" => cli.web = true,
            "--port" => cli.port = parse_u64_arg("--port", args.next())? as u16,
            "--host" => {
                let raw =
                    args.next().ok_or_else(|| "missing value for --host".to_string())?;
                cli.host = raw;
            }
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

/// 채팅방(`--chat`) 전용 페르소나. 헤드리스/골든은 `demo_personas`(canonical, dev 회귀용)를
/// 그대로 쓰고, 채팅은 "생동감 있는 3-way"를 위해 μ 간격을 좁힌 이 세트를 쓴다(골든 불침투).
///
/// 반복 테스트 튜닝(Pub 교차자극 + theta 0.60 + forbid_self_repeat 기준):
/// friend~43% / realist~32% / summarizer~23%, 침묵 리듬 유지(maxsil~3), 자기연속 0.
/// id/이름은 demo와 동일(프롬프트·모디파이어 공유), base_rate만 다르다.
fn chat_personas() -> Vec<Persona> {
    vec![
        Persona {
            id: "friend".to_string(),
            name: "Friendly Regular".to_string(),
            base_rate: 0.70,
        },
        Persona {
            id: "chaos".to_string(),
            name: "Grounded Realist".to_string(),
            base_rate: 0.62,
        },
        Persona {
            id: "summarizer".to_string(),
            name: "Quiet Summarizer".to_string(),
            base_rate: 0.55,
        },
    ]
}

/// 데모 3인(friend / chaos / summarizer)의 역할 기반 system prompt를 반환한다.
/// 2~3문장 — 한마디 툭 던지고 끝이 아니라 조금 더 대화하게(단, 독백은 금지).
/// 응답 언어는 시스템 로케일(`$LANG`)에서 감지, 기본 한국어(salon::locale).
fn demo_persona_system_prompts() -> BTreeMap<PersonaId, String> {
    let lang = salon::locale::reply_language();
    // 공통 꼬리말: 언어 지시 + 행동 가드레일.
    let common = format!(
        " A real person takes part in this chat; their messages are labelled \"나\". When 나 says or asks something, respond to 나 directly and follow their lead (answer the question, do what they ask) instead of just riffing with the other personas. Always respond in {lang}, even if others write in another language. Don't act like a therapist, skip excessive apologies or praise, don't repeat the previous line, and keep it conversational (not a monologue). Use chat-laughter like ㅋㅋㅋ/ㅎㅎ and emoji very sparingly — at most occasionally, never in every message. Some recent lines may be YOUR OWN earlier messages (same name as you) — never agree with, praise, or react to your own line as if someone else said it; instead build on what OTHERS said, answer 나, or add a genuinely new angle."
    );
    let mut m = BTreeMap::new();
    m.insert(
        "friend".to_string(),
        format!("You are a warm, easygoing regular in this group chat. React to the mood and feelings with 2-3 natural, conversational sentences.{common}"),
    );
    // id는 "chaos"로 유지(골든 보존)하되 역할은 realist로 교체(사용자 결정 2026-06-03).
    m.insert(
        "chaos".to_string(),
        format!("You are a level-headed realist in this group chat. When the talk drifts into exaggeration, hype, or wishful thinking, gently bring it back to earth with one or two grounded, practical sentences — concrete and matter-of-fact, never absurd or random.{common}"),
    );
    m.insert(
        "summarizer".to_string(),
        format!("You are a quiet observer. Speak up to tie loose threads together in two or three sentences when things have piled up.{common}"),
    );
    m
}

/// --room 사용 시 적용되는 데모 모디파이어.
fn demo_persona_modifiers() -> BTreeMap<PersonaId, PersonaModifier> {
    let mut m = BTreeMap::new();
    // realist: 과장·비현실에 반응(중간 reactivity), 도발은 낮게. (id는 "chaos" 유지)
    m.insert(
        "chaos".to_string(),
        PersonaModifier { reactivity: 1.0, provocativeness: 0.8 },
    );
    m.insert(
        "friend".to_string(),
        PersonaModifier { reactivity: 2.0, provocativeness: 1.0 },
    );
    m.insert(
        "summarizer".to_string(),
        PersonaModifier { reactivity: 1.0, provocativeness: 0.5 },
    );
    m
}

/// --chat 및 chat_demo 공용 데모 룸 풀을 빌드한다.
///
/// 구성:
///   - cloud  : Ollama(gemma4:31b-cloud, localhost:11434, cap=1, num_ctx=None, thinking=false)
///   - friend : OpenAI(qwen3.6-35b-fast, yongseek.iptime.org:8008, cap=2, max_tokens=1024, thinking=false)
///   - 양쪽에 demo_persona_system_prompts() 적용.
///   - default = "friend"(qwen, 2명: friend/chaos), summarizer → "cloud"(gemma, 1명) 라우팅, 상호 폴백.
///   - `SALON_CLOUD_ONLY` 설정 시: friend 백엔드/라우팅/폴백을 건너뛰고 cloud(cap=1)만.
///     지인 vLLM 서버가 죽었을 때 라이브 테스트용(비파괴적 — 토글만 끄면 원복).
///
/// thinking off(2026-06-03 사용자): qwen reasoning이 발화당 ~70초로 과해 thinking을 끈다.
///   - friend: qwen3.6-35b-fast(reasoning 버전 대신 fast 변종) + enable_thinking=false.
///   - cloud(gemma4:31b-cloud): thinking off.
///   서버 실측: thinking=false면 즉답(~0.3초). 단 vLLM은 vllm-swap이라 모델 전환 첫 발화는 느림(~2.5분).
///
/// SECURITY: api_key 없음(cloud는 localhost 프록시, friend는 내부망 서버).
fn build_demo_room_pool() -> BackendPool {
    let mut pool = BackendPool::new();

    // cloud 백엔드: Ollama, gemma4:31b-cloud, cap=1, num_ctx=None(원격 auto-max).
    // 동시성 1(사용자 결정 2026-06-03): cloud rate 보수적, 부하는 friend(qwen)로.
    // thinking off(2026-06-03 사용자): 빠른 발화. thinking은 발화 텀을 과하게 늘려 끈다.
    let cloud_cfg = BackendConfig::new(
        "cloud",
        "gemma4:31b-cloud",
        "http://localhost:11434",
        None,
        1,
        None,
        Duration::from_secs(60),
    );
    pool.add(cloud_cfg, demo_persona_system_prompts());

    // SALON_CLOUD_ONLY: 지인(friend) vLLM 서버가 죽었을 때 cloud만으로 라이브 테스트.
    // friend 백엔드/라우팅/폴백을 통째로 건너뛴다(서버 복구 시 토글만 끄면 원복).
    let cloud_only = std::env::var("SALON_CLOUD_ONLY")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    if cloud_only {
        pool.set_default("cloud");
        return pool;
    }

    // friend 백엔드: OpenAI 호환(vLLM), qwen3.6-35b-fast, cap=2, max_tokens=1024.
    // 동시성 2(사용자 결정 2026-06-03): 지인 vLLM 서버에 더 많은 부하 배분.
    // thinking off + fast 모델(2026-06-03 사용자): reasoning ~70초 페이싱 제거. thinking off면
    //   reasoning 0이라 max_tokens 1024로 충분(발화 2-3문장).
    // 주의: 지인 vLLM은 vllm-swap(한 번에 한 모델). 모델 전환 시 첫 발화가 swap-in으로 ~2.5분
    //   걸릴 수 있어 timeout 180s로 견딘다(swap 후 발화는 빠름). 다른 용도와 공유 시 재swap 가능.
    let friend_cfg = BackendConfig::new_openai(
        "friend",
        "qwen3.6-35b-fast",
        "http://yongseek.iptime.org:8008",
        None,
        2,
        Some(1024),
        Duration::from_secs(180),
    );
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
    let cloud_only = std::env::var("SALON_CLOUD_ONLY")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    let prompts = demo_persona_system_prompts();
    let modifiers = demo_persona_modifiers();

    // 라우팅 결정: SALON_CLOUD_ONLY면 전부 cloud, 아니면 build_demo_room_pool 일치.
    let backend_for = |id: &str| -> String {
        if cloud_only {
            "cloud".to_string()
        } else {
            match id {
                "summarizer" => "cloud".to_string(),
                _ => "friend".to_string(),
            }
        }
    };

    let ids = ["friend", "chaos", "summarizer"];
    ids.iter()
        .map(|id| {
            let system_prompt = prompts
                .get(*id)
                .cloned()
                .unwrap_or_default();
            let modifier = modifiers
                .get(*id)
                .cloned()
                .unwrap_or_default();
            (
                id.to_string(),
                PersonaMeta {
                    backend: backend_for(id),
                    system_prompt,
                    modifier,
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
    "Usage: salon [--headless] [--sweep] [--fsm] [--seed <u64>] [--ticks <u64>] [--theta <f64>] [--k <f64>] [--beta <f64>] [--delay-ms <u64>] [--room <calm|pub|argument|chaos>] [--llm] [--model <name>] [--ollama-host <url>] [--chat] [--web] [--port <u16>] [--host <addr>]"
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
