//! mixed-model 벤치 (v0.4 신규).
//!
//! 한 방에 두 백엔드(cloud=Ollama, friend=OpenAI/vLLM)를 라우팅해
//! 같은 맥락에 동시 생성하고 모델별 출력을 나란히 비교한다.
//!
//! 백엔드 구성:
//!   - cloud  : BackendConfig::new  (Ollama, gemma4:31b-cloud, cap=3)
//!   - friend : BackendConfig::new_openai (qwen3.6-35b-fast, http://yongseek.iptime.org:8008, cap=1)
//!
//! 라우팅: summarizer → friend, 나머지 → cloud(default).
//!
//! 추가 측정: 배치 후 cloud 백엔드 대상으로 K=4 순차 generate 지연 측정.
//!   첫 번째(콜드) 제외 avg/max 출력. burst 결정 근거.
//!
//! 환경변수:
//!   SALON_CLOUD_MODEL    기본 "gemma4:31b-cloud"
//!   SALON_FRIEND_MODEL   기본 "qwen3.6-35b-fast"
//!   SALON_FRIEND_ENDPOINT 기본 "http://yongseek.iptime.org:8008"
//!
//! 실행 예:
//!   cargo run --example mixed_bench
//!   SALON_CLOUD_MODEL=gemma4:31b-cloud SALON_FRIEND_ENDPOINT=http://... cargo run --example mixed_bench
//!
//! 서버가 없으면 해당 슬롯이 "(no response)"로 나온다(panic 없음).

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use salon::model::Event;
use salon::pool::{BackendConfig, BackendPool};
use salon::runtime::PersonaRuntime;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

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
    // 환경변수 또는 기본값으로 모델/엔드포인트 결정.
    let cloud_model =
        std::env::var("SALON_CLOUD_MODEL").unwrap_or_else(|_| "gemma4:31b-cloud".to_string());
    let friend_model =
        std::env::var("SALON_FRIEND_MODEL").unwrap_or_else(|_| "qwen3.6-35b-fast".to_string());
    let friend_endpoint = std::env::var("SALON_FRIEND_ENDPOINT")
        .unwrap_or_else(|_| "http://yongseek.iptime.org:8008".to_string());

    // cloud 모델은 num_ctx None(원격 auto-max), 로컬이면 8192.
    let cloud_num_ctx = if cloud_model.ends_with(":cloud") {
        None
    } else {
        Some(8192)
    };

    // -------------------------------------------------------------------------
    // BackendPool: cloud(Ollama) + friend(OpenAI/vLLM) 두 백엔드 등록.
    // -------------------------------------------------------------------------
    let mut pool = BackendPool::new();

    // cloud 백엔드: Ollama 프로토콜, cap=3(동시 3 in-flight).
    // SALON_THINK=1 이면 reasoning(thinking) 켜고 max_tokens 상향(CoT 여유 확보).
    let think = std::env::var("SALON_THINK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    let mut cloud_cfg = BackendConfig::new(
        "cloud",
        cloud_model.clone(),
        "http://localhost:11434",
        None, // api_key 없음
        3,    // cloud cap
        cloud_num_ctx,
        Duration::from_secs(60),
    );
    cloud_cfg.thinking = think;
    pool.add(cloud_cfg, demo_prompts());

    // friend 백엔드: OpenAI 호환 프로토콜, cap=1(max-num-seqs 1).
    let mut friend_cfg = BackendConfig::new_openai(
        "friend",
        friend_model.clone(),
        friend_endpoint.clone(),
        None, // api_key 없음(내부망 서버)
        1,    // cap=1(직렬)
        Some(if think { 4096 } else { 256 }),
        Duration::from_secs(if think { 120 } else { 60 }),
    );
    friend_cfg.thinking = think;
    pool.add(friend_cfg, demo_prompts());

    // 기본 백엔드: cloud. summarizer만 friend로 라우팅.
    pool.set_default("cloud");
    pool.add_route("summarizer", "friend");

    // -------------------------------------------------------------------------
    // 공통 opening.
    // -------------------------------------------------------------------------
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

    // 모델명 조회용 맵: backend_name -> model_string (api_key 노출 없음).
    let backend_model_map: BTreeMap<&str, &str> = BTreeMap::from([
        ("cloud", cloud_model.as_str()),
        ("friend", friend_model.as_str()),
    ]);

    println!("=== mixed_bench: cloud + friend 동시 생성 ===");
    println!("cloud  : {cloud_model} @ localhost:11434 (cap=3)");
    println!("friend : {friend_model} @ {friend_endpoint} (cap=1)");
    println!("라우팅 : summarizer → friend, 나머지 → cloud");
    println!(
        "thinking: {}",
        if think {
            "ON (reasoning, max_tokens 1024)"
        } else {
            "off"
        }
    );
    println!("opening> {opening}\n");

    // -------------------------------------------------------------------------
    // generate_batch: 동시 생성. 입력 순서 보존.
    // -------------------------------------------------------------------------
    let results = pool.generate_batch(&jobs, 1);

    for (persona, text) in &results {
        // pool.resolve(persona)로 실제 사용 백엔드 이름 조회.
        let backend_name = pool.resolve(persona).unwrap_or("(unknown)");
        let model_str = backend_model_map
            .get(backend_name)
            .copied()
            .unwrap_or("(unknown)");
        let out = text
            .as_deref()
            .unwrap_or("(no response / 서버 미가동·모델 없음)");
        println!("[{persona} via {backend_name}/{model_str}] {out}\n");
    }

    // -------------------------------------------------------------------------
    // 지연 micro-측정: cloud 백엔드 대상, K=4 순차 generate.
    // PersonaRuntime::generate(&mut pool, ...) — 폴백 체인 포함, rng 소비 없음.
    // 첫 번째(콜드) 제외 avg/max 출력.
    // -------------------------------------------------------------------------
    const K: usize = 4;
    // 측정 대상: cloud로 라우팅되는 "friend" 페르소나(직접 naming 혼동을 피하기 위해
    // routing에 없는 "chaos"를 사용 — default=cloud로 향한다).
    let bench_persona = "chaos".to_string();
    let bench_history = vec![opening_event.clone()];
    // rng: PersonaRuntime::generate 시그니처 요구사항. BackendPool은 소비하지 않는다.
    let mut rng = ChaCha8Rng::seed_from_u64(0);

    println!("=== 지연 측정: cloud 백엔드 순차 {K}회 (첫 콜드 제외 avg/max) ===");
    println!("대상 페르소나: {bench_persona} (→ cloud/{cloud_model})");

    let mut latencies_ms: Vec<f64> = Vec::with_capacity(K);
    for i in 0..K {
        let start = Instant::now();
        let text = pool.generate(&bench_persona, &bench_history, (i + 2) as u64, &mut rng);
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        latencies_ms.push(elapsed_ms);

        let label = if i == 0 { " [cold]" } else { "" };
        let preview = text
            .as_deref()
            .unwrap_or("(no response)")
            .chars()
            .take(40)
            .collect::<String>();
        println!("  [{i}]{label} {elapsed_ms:.0}ms | {preview}");
    }

    // 첫 콜드 호출 제외하고 avg/max 계산.
    if K > 1 {
        let warm: &[f64] = &latencies_ms[1..];
        let avg = warm.iter().sum::<f64>() / warm.len() as f64;
        let max = warm.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        println!(
            "\n  warm avg: {avg:.0}ms  max: {max:.0}ms (n={})",
            warm.len()
        );
        println!("  → burst 필요성: 라이브 순차 틱이 1발화당 ~{avg:.0}ms 블록");
    } else {
        println!("  (K=1이므로 warm 통계 없음)");
    }
}
