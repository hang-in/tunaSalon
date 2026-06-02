// smoke_v4.rs — v0.4 스모크 게이트
//
// 검증 항목:
//   INV-1  FakeBackend 결정성 (v0.4 풀/추상화 코드가 들어와도 결정 경로 불변)
//   라우팅  BackendPool::resolve 동작 (task-22)
//   폴백    BackendPool::fallback_chain 동작 (task-24)
//   배치    BackendPool::generate_batch 순서·무패닉 (task-23)
//   경량    OllamaBackend::build_request_body num_ctx (task-21)
//          OpenAIBackend::parse_response reasoning 무시 (task-27)
//
// 전체 네트워크-프리: 오프라인 백엔드 (http://127.0.0.1:1) + FakeBackend + 정적 JSON

use salon::driver;
use salon::model::{CouplingMatrix, EngineConfig, Persona};
use salon::ollama::OllamaBackend;
use salon::openai::OpenAIBackend;
use salon::pool::{BackendConfig, BackendPool};
use salon::runtime::FakeBackend;
use salon::sink::VecSink;
use std::collections::BTreeMap;
use std::time::Duration;

// ──────────────────────────────────────────────────────────────────────────────
// 공통 헬퍼
// ──────────────────────────────────────────────────────────────────────────────

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

fn base_config(theta: f64) -> EngineConfig {
    EngineConfig {
        beta: 0.5,
        theta,
        k: 60.0,
        tick_interval: 1.0,
        alpha: CouplingMatrix::default(),
        forbid_self_repeat: false,
    }
}

/// 테스트용 오프라인 BackendConfig: 포트 1은 즉시 연결 거부 → 네트워크 없이 라우팅/폴백만 검증.
fn offline_config(name: &str) -> BackendConfig {
    BackendConfig::new(
        name,
        "fake-model",
        "http://127.0.0.1:1", // 즉시 연결 거부 — 빠른 타임아웃
        None,
        1,
        None,
        Duration::from_millis(1),
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// INV-1: FakeBackend 결정성 게이트 (v0.4 헤드라인)
//
// v0.4 풀·추상화 코드가 존재해도 FakeBackend 직접 경로(--llm 없음)가
// 동일 seed에서 바이트 동일 records를 생성해야 한다.
// (a) 두 번 실행 → 동일 records
// (b) 모든 record의 utterance == None (FakeBackend는 텍스트를 생성하지 않음)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn inv1_fake_backend_determinism_preserved_with_v04_code() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let seed = 42u64;
    let ticks = 80u64; // 스펙 기준: seed 42, theta 0.65, 80 ticks

    // (a) 동일 seed 두 번 실행 → records 바이트 동일
    let mut sink_a = VecSink::default();
    let mut sink_b = VecSink::default();
    driver::run(&config, &personas, seed, ticks, &mut sink_a, &mut FakeBackend);
    driver::run(&config, &personas, seed, ticks, &mut sink_b, &mut FakeBackend);

    assert_eq!(
        sink_a.records, sink_b.records,
        "INV-1 위반: 동일 seed({seed}) 두 번 실행이 다른 records를 생성함 \
         — v0.4 코드가 결정 경로를 오염시키고 있음"
    );

    // (b) FakeBackend는 발화 텍스트를 생성하지 않으므로 모든 utterance == None
    for record in &sink_a.records {
        assert_eq!(
            record.utterance, None,
            "tick {}: FakeBackend 경로인데 utterance가 None이 아님 \
             — FakeBackend가 텍스트를 반환하거나 직렬화 생략 설정이 깨짐",
            record.tick
        );
    }

    // records가 실제로 존재해야 단언이 의미 있음
    assert_eq!(
        sink_a.records.len(), ticks as usize,
        "tick 수({ticks})와 records 수({}) 불일치",
        sink_a.records.len()
    );
}

// 여러 seed로 결정성을 추가 확인 (회귀 방지)
#[test]
fn inv1_determinism_multiple_seeds() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let ticks = 80u64;

    for seed in [7u64, 99u64] {
        let mut sink_x = VecSink::default();
        let mut sink_y = VecSink::default();
        driver::run(&config, &personas, seed, ticks, &mut sink_x, &mut FakeBackend);
        driver::run(&config, &personas, seed, ticks, &mut sink_y, &mut FakeBackend);

        assert_eq!(
            sink_x.records, sink_y.records,
            "INV-1 위반: seed={seed}에서 두 실행 결과가 다름"
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 라우팅 (task-22): BackendPool::resolve
//
// 오프라인 백엔드로 풀을 구성하고 resolve가 라우팅 규칙을 올바르게 적용하는지 검증.
// 네트워크 접근 없음 — 이름 해석 로직만 테스트한다.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn routing_resolve_returns_correct_backend() {
    let mut pool = BackendPool::new();
    pool.add(offline_config("cloud"), BTreeMap::new());
    pool.add(offline_config("friend"), BTreeMap::new());
    pool.set_default("cloud");
    // "summarizer" → "friend" 로 명시 라우팅
    pool.add_route("summarizer", "friend");

    // 1) routing 맵에 등록된 페르소나 → 지정 백엔드 이름
    assert_eq!(
        pool.resolve("summarizer"),
        Some("friend"),
        "summarizer는 명시 라우팅에 따라 'friend' 백엔드여야 함"
    );

    // 2) 미지정 페르소나 → default 백엔드 이름
    assert_eq!(
        pool.resolve("chaos"),
        Some("cloud"),
        "라우팅 미지정 페르소나는 default 'cloud'여야 함"
    );
    assert_eq!(
        pool.resolve("friend_persona"), // 이름이 달라 routing 미등록
        Some("cloud"),
        "routing 미등록 페르소나는 default로 폴백해야 함"
    );

    // 3) default도 없고 routing도 없으면 None (panic 없음)
    let empty_pool = BackendPool::new();
    assert_eq!(
        empty_pool.resolve("any"),
        None,
        "default·routing 모두 없으면 None이어야 함"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 폴백 체인 (task-24): BackendPool::fallback_chain
//
// 체인 길이·순서, 폴백 없음, 사이클 안전(유한)을 검증한다.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn fallback_chain_order_and_cycle_safety() {
    // 케이스 1: primary "cloud" → fallback "friend" 체인
    {
        let mut pool = BackendPool::new();
        pool.add(offline_config("cloud"), BTreeMap::new());
        pool.add(offline_config("friend"), BTreeMap::new());
        pool.set_default("cloud");
        pool.set_fallback("cloud", "friend");

        let chain = pool.fallback_chain("any-speaker");
        assert_eq!(
            chain,
            vec!["cloud".to_string(), "friend".to_string()],
            "체인이 [primary, fallback] 순서여야 함"
        );
    }

    // 케이스 2: fallback 없음 → [primary] 만
    {
        let mut pool = BackendPool::new();
        pool.add(offline_config("cloud"), BTreeMap::new());
        pool.set_default("cloud");
        // set_fallback 없음

        let chain = pool.fallback_chain("any-speaker");
        assert_eq!(
            chain,
            vec!["cloud".to_string()],
            "폴백 미설정 시 [primary] 만 반환되어야 함"
        );
    }

    // 케이스 3: 사이클(a→b→a) — 체인이 유한하게 끝나야 함 (무한 루프·panic 없음)
    {
        let mut pool = BackendPool::new();
        pool.add(offline_config("a"), BTreeMap::new());
        pool.add(offline_config("b"), BTreeMap::new());
        pool.set_default("a");
        pool.set_fallback("a", "b");
        pool.set_fallback("b", "a"); // 사이클

        let chain = pool.fallback_chain("any-speaker");
        assert_eq!(chain.len(), 2, "사이클 a→b→a에서 체인 길이는 2여야 함: {:?}", chain);
        assert_eq!(chain[0], "a");
        assert_eq!(chain[1], "b");
    }

    // 케이스 4: resolve 불가 → 빈 체인
    {
        let pool = BackendPool::new(); // default 없음
        let chain = pool.fallback_chain("any-speaker");
        assert!(chain.is_empty(), "resolve 불가 시 빈 Vec이어야 함");
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 배치 순서·무패닉 (task-23): BackendPool::generate_batch
//
// 오프라인 백엔드: 모든 결과가 None이고 입력 순서가 보존되어야 함.
// panic 없이 완료되어야 함.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn batch_preserves_order_all_none_no_panic() {
    let mut pool = BackendPool::new();
    // 두 오프라인 백엔드: cloud(primary) + friend(fallback) → 둘 다 None
    pool.add(offline_config("cloud"), BTreeMap::new());
    pool.add(offline_config("friend"), BTreeMap::new());
    pool.set_default("cloud");
    pool.set_fallback("cloud", "friend");

    let jobs = vec![
        ("alice".to_string(), vec![]),
        ("bob".to_string(), vec![]),
        ("carol".to_string(), vec![]),
        ("dave".to_string(), vec![]),
    ];

    // generate_batch는 panic 없이 완료되어야 한다
    let results = pool.generate_batch(&jobs, 0);

    // 결과 수 = 입력 수
    assert_eq!(
        results.len(),
        jobs.len(),
        "결과 수({})가 입력 수({})와 달라야 함",
        results.len(),
        jobs.len()
    );

    // 입력 순서 보존 + 전부 None (오프라인 백엔드)
    for (i, (speaker, text)) in results.iter().enumerate() {
        assert_eq!(
            speaker, &jobs[i].0,
            "순서 불일치: idx={i}, expected={}, got={speaker}",
            jobs[i].0
        );
        assert_eq!(
            *text, None,
            "오프라인 primary+fallback이면 모두 None이어야 함(idx={i}, speaker={speaker})"
        );
    }
}

// 라우팅 대상이 없는 경우도 패닉 없이 None 반환
#[test]
fn batch_no_backend_returns_none_no_panic() {
    let pool = BackendPool::new(); // 백엔드·default 없음

    let jobs = vec![
        ("friend".to_string(), vec![]),
        ("chaos".to_string(), vec![]),
    ];

    let results = pool.generate_batch(&jobs, 99);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "friend");
    assert_eq!(results[0].1, None);
    assert_eq!(results[1].0, "chaos");
    assert_eq!(results[1].1, None);
}

// ──────────────────────────────────────────────────────────────────────────────
// 경량: num_ctx (task-21) — OllamaBackend::build_request_body
//
// None이면 options.num_ctx 생략, Some(n)이면 n으로 설정.
// 기존 단위 테스트(ollama::tests)와 동일 로직 — 통합 smoke에서 대표 확인.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn num_ctx_none_omitted_some_included() {
    // num_ctx=None → options 키 자체가 body에 없어야 함
    let body_no_ctx = OllamaBackend::build_request_body("m", "p", None, None);
    assert!(
        body_no_ctx.get("options").is_none(),
        "num_ctx=None이면 options 키가 body에 없어야 함: {:?}",
        body_no_ctx
    );

    // num_ctx=Some(8192) → options.num_ctx == 8192
    let body_with_ctx = OllamaBackend::build_request_body("m", "p", None, Some(8192));
    let num_ctx_val = body_with_ctx
        .get("options")
        .and_then(|o| o.get("num_ctx"))
        .and_then(|v| v.as_u64());
    assert_eq!(
        num_ctx_val,
        Some(8192),
        "num_ctx=Some(8192)이면 options.num_ctx가 8192여야 함"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 경량: Backend 분기 (task-27) — OpenAIBackend::parse_response
//
// choices[0].message.content를 추출하고 reasoning 필드를 무시해야 함.
// 기존 단위 테스트(openai::tests)와 동일 로직 — 통합 smoke에서 대표 확인.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn openai_parse_response_extracts_content_ignores_reasoning() {
    // reasoning 필드가 있어도 content만 추출한다
    let json_with_reasoning = r#"{
        "choices": [{
            "message": {
                "role": "assistant",
                "reasoning": "Let me think... CoT goes here",
                "content": "The answer is here"
            }
        }]
    }"#;
    let result = OpenAIBackend::parse_response(json_with_reasoning);
    assert_eq!(
        result,
        Some("The answer is here".to_string()),
        "reasoning 필드가 있어도 content만 추출해야 함"
    );

    // reasoning 텍스트가 결과에 섞이면 안 됨
    let text = result.unwrap();
    assert!(
        !text.contains("CoT"),
        "reasoning 내용이 parse_response 결과에 포함되면 안 됨: {text}"
    );

    // choices 없으면 None (오염·파싱 오류 처리)
    let json_no_choices = r#"{"id": "abc"}"#;
    assert_eq!(
        OpenAIBackend::parse_response(json_no_choices),
        None,
        "choices 없으면 None이어야 함"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 라이브(cloud/friend) 통합 — 네트워크 필요, CI에서는 skip
// ──────────────────────────────────────────────────────────────────────────────

/// 실제 Ollama cloud 백엔드 라이브 generate — `--ignored`로 수동 실행.
#[test]
#[ignore]
fn live_pool_cloud_generate() {
    use rand::SeedableRng;
    use salon::runtime::PersonaRuntime;

    let cloud_endpoint =
        std::env::var("OLLAMA_CLOUD_ENDPOINT").unwrap_or_else(|_| "https://api.ollama.ai".to_string());
    let cloud_key = std::env::var("OLLAMA_CLOUD_API_KEY").ok();
    let model =
        std::env::var("SALON_CLOUD_MODEL").unwrap_or_else(|_| "gemma4:e4b".to_string());

    let mut pool = BackendPool::new();
    let cfg = BackendConfig::new(
        "cloud",
        model,
        cloud_endpoint,
        cloud_key,
        3,
        None,
        Duration::from_secs(30),
    );
    pool.add(cfg, BTreeMap::new());
    pool.set_default("cloud");

    let speaker = "friend".to_string();
    let history = vec![];
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

    let result = pool.generate(&speaker, &history, 0, &mut rng);
    println!("live cloud result: {result:?}");
    // 서버 응답 시 Some, 오프라인이면 None — panic 없어야 함
}
