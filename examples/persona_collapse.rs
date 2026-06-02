//! persona collapse 관전 도구.
//!
//! 같은 작은 모델에 서로 다른 페르소나 system prompt를 주고, **같은 맥락**에 답하게 해서
//! 출력 톤이 페르소나마다 다른지(유지) 비슷하게 무너지는지(collapse)를 사람이 비교한다.
//!
//! 실제 Ollama 호출이 필요하다(로컬 데몬 + 모델). 예:
//!   ollama pull gemma4:e4b   # 또는 cloud 모델: ollama pull <model>:cloud
//!   cargo run --example persona_collapse                 # 기본 gemma4:e4b
//!   cargo run --example persona_collapse -- glm-5.1:cloud # cloud 모델
//!
//! Ollama가 안 떠 있으면 각 줄이 "(no response)"로 나온다(panic 없음).

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use salon::model::Event;
use salon::ollama::OllamaBackend;
use salon::runtime::PersonaRuntime;
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
    // 기본은 cloud(gemma4:31b-cloud) — 로컬 ollama는 맥북 랙으로 금지. 인자로 다른 모델 지정 가능.
    let model = std::env::args().nth(1).unwrap_or_else(|| "gemma4:31b-cloud".to_string());
    let endpoint = "http://localhost:11434".to_string();

    let prompts = demo_prompts();
    // cloud(:cloud) 모델은 num_ctx None(원격 auto-max), 로컬 모델만 RAM 상한 8192.
    let num_ctx = if model.ends_with(":cloud") { None } else { Some(8192) };
    let mut backend = OllamaBackend::new(
        model.clone(),
        endpoint,
        None,
        prompts,
        Duration::from_secs(60),
        num_ctx,
    );

    // 세 페르소나가 똑같이 답할 공통 맥락 한 줄.
    let opening = "오늘 비 와서 다들 약속 취소했대. 좀 심심하네.";
    let history = vec![Event {
        ts: 0.0,
        speaker: "사람".to_string(),
        mark: 1.0,
        content: Some(opening.to_string()),
    }];
    // rng는 OllamaBackend가 소비하지 않지만 시그니처상 필요하다.
    let mut rng = ChaCha8Rng::seed_from_u64(0);

    println!("model: {model}");
    println!("opening> {opening}\n");

    for persona in ["friend", "chaos", "summarizer"] {
        let out = backend
            .generate(&persona.to_string(), &history, 1, &mut rng)
            .unwrap_or_else(|| "(no response / Ollama 안 떠 있거나 모델 없음)".to_string());
        println!("[{persona}] {out}\n");
    }

    println!("세 줄의 톤이 서로 다르면 페르소나 유지, 비슷하면 collapse.");
}
