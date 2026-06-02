//! chat_demo: 비인터랙티브 라이브 루프 데모 (v0.5 task-31).
//!
//! 동일한 데모 룸 풀(cloud + friend + 라우팅 + 폴백)에 LiveSession을 연결해
//! 실제 LLM 발화가 도착할 때마다 stdout에 전사 출력한다.
//!
//! - TTY 불요 (비인터랙티브).
//! - 최대 ~30초 또는 8발화 도착 시 자동 종료.
//! - 3번째 발화 이후 스크립트된 사람 턴 1회 → 페르소나 반응 관찰.
//! - 서버 미가동이면 "(…)" 표시, panic 없음.
//!
//! 백엔드 구성 (환경변수 오버라이드 가능):
//!   SALON_CLOUD_MODEL      기본 "gemma4:31b-cloud"
//!   SALON_FRIEND_MODEL     기본 "qwen3.6-35b-fast"
//!   SALON_FRIEND_ENDPOINT  기본 "http://yongseek.iptime.org:8008"
//!
//! 실행:
//!   cargo run --example chat_demo

use salon::live::LiveSession;
use salon::model::{CouplingMatrix, EngineConfig, Persona, PersonaId};
use salon::pool::{BackendConfig, BackendPool};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

// 종료 조건.
const MAX_UTTERANCES: usize = 8;
const MAX_WALL_SECS: u64 = 30;

// 사람 턴을 삽입할 발화 번호 (0-based: 3번째 발화 도착 후).
const HUMAN_TURN_AFTER: usize = 2;
const HUMAN_TEXT: &str = "안녕, 다들 비 와서 뭐해?";

fn demo_personas() -> Vec<Persona> {
    vec![
        Persona { id: "friend".to_string(), name: "친구".to_string(), base_rate: 0.80 },
        Persona { id: "chaos".to_string(), name: "혼돈".to_string(), base_rate: 0.70 },
        Persona { id: "summarizer".to_string(), name: "정리자".to_string(), base_rate: 0.25 },
    ]
}

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

fn build_pool(
    cloud_model: &str,
    friend_model: &str,
    friend_endpoint: &str,
) -> BackendPool {
    let mut pool = BackendPool::new();

    // cloud 백엔드: Ollama, cap=3, num_ctx=None(원격 auto-max).
    pool.add(
        BackendConfig::new(
            "cloud",
            cloud_model,
            "http://localhost:11434",
            None,
            3,
            None,
            Duration::from_secs(60),
        ),
        demo_persona_system_prompts(),
    );

    // friend 백엔드: OpenAI 호환(vLLM), cap=1, max_tokens=256.
    pool.add(
        BackendConfig::new_openai(
            "friend",
            friend_model,
            friend_endpoint,
            None,
            1,
            Some(256),
            Duration::from_secs(60),
        ),
        demo_persona_system_prompts(),
    );

    pool.set_default("cloud");
    pool.add_route("summarizer", "friend");
    // friend 서버 다운 시 cloud로 폴백.
    pool.set_fallback("friend", "cloud");

    pool
}

fn display_name(personas: &[Persona], id: &str) -> String {
    personas
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn main() {
    // 환경변수 또는 기본값으로 모델/엔드포인트 결정.
    let cloud_model = std::env::var("SALON_CLOUD_MODEL")
        .unwrap_or_else(|_| "gemma4:31b-cloud".to_string());
    let friend_model = std::env::var("SALON_FRIEND_MODEL")
        .unwrap_or_else(|_| "qwen3.6-35b-fast".to_string());
    let friend_endpoint = std::env::var("SALON_FRIEND_ENDPOINT")
        .unwrap_or_else(|_| "http://yongseek.iptime.org:8008".to_string());

    // .env 파일이 있으면 환경 변수로 로드한다.
    dotenvy::dotenv().ok();

    println!("=== chat_demo: 데모 룸 라이브 루프 ===");
    println!("cloud  : {cloud_model} @ localhost:11434 (cap=3)");
    println!("friend : {friend_model} @ {friend_endpoint} (cap=1)");
    println!("라우팅 : summarizer → friend(→cloud 폴백), 나머지 → cloud");
    println!("종료   : {}발화 도착 또는 {}초 경과", MAX_UTTERANCES, MAX_WALL_SECS);
    println!();

    let personas = demo_personas();
    let pool = Arc::new(build_pool(&cloud_model, &friend_model, &friend_endpoint));

    let config = EngineConfig {
        beta: 0.5,
        theta: 0.65,
        k: 60.0,
        tick_interval: 1.0,
        alpha: CouplingMatrix::default(),
        forbid_self_repeat: false,
    };

    let mut session = LiveSession::new(config, personas.clone(), 42, pool, "나");

    let deadline = Instant::now() + Duration::from_secs(MAX_WALL_SECS);
    let mut utterance_count = 0usize;
    let mut human_sent = false;

    // 루프: 틱을 전진하고 도착한 발화를 전사한다.
    loop {
        // 종료 조건: 발화 수 또는 wall-clock.
        if utterance_count >= MAX_UTTERANCES {
            println!("\n[chat_demo] {}개 발화 도착 → 종료", utterance_count);
            break;
        }
        if Instant::now() >= deadline {
            println!("\n[chat_demo] {}초 경과 → 종료 ({}개 발화 도착)", MAX_WALL_SECS, utterance_count);
            break;
        }

        // 엔진 틱 전진.
        session.tick();

        // 생성 결과 drain: 도착한 발화마다 출력.
        while let Some(ev) = session.poll_generation() {
            let name = display_name(&personas, &ev.speaker);
            match &ev.content {
                Some(text) => println!("{name}: {text}"),
                None => println!("{name}: (…)"),
            }
            utterance_count += 1;

            // HUMAN_TURN_AFTER번째 발화 도착 후 사람 턴 1회 삽입.
            if utterance_count > HUMAN_TURN_AFTER && !human_sent {
                println!("나: {HUMAN_TEXT}");
                session.submit_human(HUMAN_TEXT.to_string());
                human_sent = true;
            }
        }

        // 틱 사이 짧은 대기: 폴링 주기.
        std::thread::sleep(Duration::from_millis(200));
    }
}
