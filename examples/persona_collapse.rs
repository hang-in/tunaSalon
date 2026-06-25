//! persona collapse 관전 도구 (v0.4 병렬 버전).
//!
//! 같은 모델에 서로 다른 페르소나 system prompt를 주고, **같은 맥락**에 동시에 답하게 해서
//! 출력 톤이 페르소나마다 다른지(유지) 비슷하게 무너지는지(collapse)를 사람이 비교한다.
//!
//! v0.4: BackendPool + generate_batch로 3 페르소나를 동시 호출한다(순차 → 병렬).
//! cap=3이므로 세 요청이 동시에 in-flight로 나간다.
//!
//! 실제 Ollama cloud 호출이 필요하다. 예:
//!   cargo run --example persona_collapse                         # 기본 gemma4:31b-cloud
//!   cargo run --example persona_collapse -- gemma4:31b-cloud     # 동일
//!   SALON_CLOUD_MODEL=gemma4:31b-cloud cargo run --example persona_collapse
//!
//! Ollama가 안 떠 있거나 모델이 없으면 각 줄이 "(no response)"로 나온다(panic 없음).

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use salon::model::Event;
use salon::pool::{BackendConfig, BackendPool};
use std::collections::BTreeMap;
use std::time::Duration;

const COMMON: &str =
    " 실제 상담가처럼 굴지 말 것. 과한 사과나 칭찬 금지. 앞사람 말을 그대로 반복하지 말 것. 짧게.";

fn demo_prompts() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "friend".to_string(),
            format!("너는 따뜻하고 편한 단골이다. 감정과 분위기에 먼저 반응한다. 1~2문장, 가볍게.{COMMON}"),
        ),
        (
            "chaos".to_string(),
            format!("너는 장난스러운 분위기 메이커다. 살짝 엉뚱한 한마디를 던지고 빠진다. 1문장.{COMMON}"),
        ),
        (
            "summarizer".to_string(),
            format!("너는 조용한 정리자다. 흐름이 쌓였을 때만 한 문장으로 짚어준다.{COMMON}"),
        ),
    ])
}

fn main() {
    // 기본은 cloud(gemma4:31b-cloud) — 로컬 ollama는 맥북 랙으로 금지.
    // 인자1 또는 SALON_CLOUD_MODEL 환경변수로 모델 지정 가능.
    let model = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("SALON_CLOUD_MODEL").ok())
        .unwrap_or_else(|| "gemma4:31b-cloud".to_string());

    // cloud 모델은 num_ctx None(원격 auto-max), 로컬 모델만 RAM 상한 8192.
    let num_ctx = if model.ends_with(":cloud") {
        None
    } else {
        Some(8192)
    };

    // BackendPool: 단일 cloud 백엔드, cap=3으로 3 페르소나 동시 가능.
    let mut pool = BackendPool::new();
    pool.add(
        BackendConfig::new(
            "cloud",
            model.clone(),
            "http://localhost:11434",
            None, // api_key 없음
            3,    // cloud cap: 동시 3
            num_ctx,
            Duration::from_secs(60),
        ),
        demo_prompts(),
    );
    pool.set_default("cloud");

    // 세 페르소나가 똑같이 답할 공통 맥락 한 줄.
    let opening = "오늘 비 와서 다들 약속 취소했대. 좀 심심하네.";
    let opening_event = Event {
        ts: 0.0,
        speaker: "사람".to_string(),
        mark: 1.0,
        content: Some(opening.to_string()),
    };

    // 각 페르소나에 같은 opening history를 배정한다.
    let jobs: Vec<(String, Vec<Event>)> = vec![
        ("friend".to_string(), vec![opening_event.clone()]),
        ("chaos".to_string(), vec![opening_event.clone()]),
        ("summarizer".to_string(), vec![opening_event.clone()]),
    ];

    println!("model: {model}");
    println!("opening> {opening}\n");
    println!("(3 페르소나 동시 호출 중 — cap=3, BackendPool::generate_batch)\n");

    // generate_batch: 세 요청이 동시에 나가고 입력 순서대로 결과가 반환된다.
    let results = pool.generate_batch(&jobs, 1);

    for (persona, text) in &results {
        let out = text
            .as_deref()
            .unwrap_or("(no response / 서버 미가동·모델 없음)");
        println!("[{persona}] {out}\n");
    }

    println!("세 줄의 톤이 서로 다르면 페르소나 유지, 비슷하면 collapse.");

    // rng: PersonaRuntime::generate 시그니처용으로 예약. generate_batch는 사용하지 않는다.
    let _rng = ChaCha8Rng::seed_from_u64(0);
}
