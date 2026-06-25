use rand_chacha::ChaCha8Rng;
use salon::driver;
use salon::model::{CouplingMatrix, EngineConfig, Event, Persona, PersonaId};
use salon::runtime::FakeBackend;
use salon::runtime::PersonaRuntime;
use salon::sink::VecSink;

// ──────────────────────────────────────────────────────────────────────────────
// StubBackend: 결정적, 고정 텍스트 반환, rng 미소비
// ──────────────────────────────────────────────────────────────────────────────

struct StubBackend {
    reply: String,
}

impl PersonaRuntime for StubBackend {
    fn generate(
        &mut self,
        _speaker: &PersonaId,
        _history: &[Event],
        _tick: u64,
        _rng: &mut ChaCha8Rng,
    ) -> Option<String> {
        Some(self.reply.clone())
    }
}

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

fn speak_count_for(sink: &VecSink, persona_id: &str) -> u64 {
    sink.records
        .iter()
        .filter(|r| r.chosen.as_deref() == Some(persona_id))
        .count() as u64
}

// ──────────────────────────────────────────────────────────────────────────────
// 기준 1: 기본 결정성(FakeBackend) + μ→빈도
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn fake_backend_is_deterministic_and_keeps_mu_frequency() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let seed = 42u64;
    let ticks = 200u64;

    // 동일 seed 두 번 → 동일 records
    let mut sink_a = VecSink::default();
    let mut sink_b = VecSink::default();
    driver::run(
        &config,
        &personas,
        seed,
        ticks,
        &mut sink_a,
        &mut FakeBackend,
    );
    driver::run(
        &config,
        &personas,
        seed,
        ticks,
        &mut sink_b,
        &mut FakeBackend,
    );

    assert_eq!(
        sink_a.records, sink_b.records,
        "FakeBackend: 동일 seed가 동일 결과를 내지 않음 — 결정성 위반"
    );

    // μ → 빈도: friend > chaos > summarizer
    let friend = speak_count_for(&sink_a, "friend");
    let chaos = speak_count_for(&sink_a, "chaos");
    let summarizer = speak_count_for(&sink_a, "summarizer");

    assert!(
        friend > chaos,
        "friend({friend}) > chaos({chaos}) 이어야 함 — μ→빈도 위반"
    );
    assert!(
        chaos > summarizer,
        "chaos({chaos}) > summarizer({summarizer}) 이어야 함 — μ→빈도 위반"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 기준 2: content 배선 — gate_passed record의 utterance 유무
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn stub_backend_fills_utterance_for_speaking_records() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let seed = 42u64;
    let ticks = 200u64;
    let reply = "그렇구나".to_string();

    // StubBackend: gate_passed인 record → utterance == Some(reply),
    //              gate_passed=false(침묵) → utterance == None
    let mut stub = StubBackend {
        reply: reply.clone(),
    };
    let mut sink_stub = VecSink::default();
    driver::run(&config, &personas, seed, ticks, &mut sink_stub, &mut stub);

    for record in &sink_stub.records {
        if record.gate_passed {
            assert_eq!(
                record.utterance.as_deref(),
                Some(reply.as_str()),
                "tick {}: gate_passed인 record의 utterance가 Some(reply)가 아님",
                record.tick
            );
        } else {
            assert_eq!(
                record.utterance, None,
                "tick {}: 침묵 record의 utterance가 None이 아님",
                record.tick
            );
        }
    }

    // FakeBackend: 모든 utterance == None
    let mut sink_fake = VecSink::default();
    driver::run(
        &config,
        &personas,
        seed,
        ticks,
        &mut sink_fake,
        &mut FakeBackend,
    );

    for record in &sink_fake.records {
        assert_eq!(
            record.utterance, None,
            "tick {}: FakeBackend인데 utterance가 None이 아님",
            record.tick
        );
    }

    // gate_passed record가 실제로 존재해야 단언이 의미 있음
    let spoken_count = sink_stub.records.iter().filter(|r| r.gate_passed).count();
    assert!(
        spoken_count > 0,
        "gate_passed record가 하나도 없음 — 단언이 무의미"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 기준 3: StubBackend 결정성 — 동일 seed 두 번 → 동일 records
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn stub_backend_is_deterministic() {
    let personas = demo_personas();
    let config = base_config(0.65);
    let seed = 99u64;
    let ticks = 200u64;
    let reply = "안녕하세요".to_string();

    let mut stub_a = StubBackend {
        reply: reply.clone(),
    };
    let mut stub_b = StubBackend {
        reply: reply.clone(),
    };
    let mut sink_a = VecSink::default();
    let mut sink_b = VecSink::default();

    driver::run(&config, &personas, seed, ticks, &mut sink_a, &mut stub_a);
    driver::run(&config, &personas, seed, ticks, &mut sink_b, &mut stub_b);

    assert_eq!(
        sink_a.records, sink_b.records,
        "StubBackend: 동일 seed가 동일 결과를 내지 않음 — 결정성 위반"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 기준 4: content가 결정에 영향 — "friend"를 호명하면 friend 발화 수가 증가한다
//
// Interest 신호: 가장 최근 발화 content에 candidate id가 포함되면 rank 1 획득.
// StubBackend(reply="friend 말이 맞아")를 쓰면 friend가 매 발화마다 호명되어
// interest 신호에서 rank 1을 얻는다.
// FakeBackend(content=None)는 interest/echo 신호가 전혀 활성화되지 않는다.
//
// alpha=0.5 균일(off-diagonal) + theta=0.5: chaos/summarizer에게 cross-excitation burst가
// 발생하여 intensity가 friend를 일시적으로 역전하는 구간이 생긴다.
// 그 구간에서 interest 신호(friend 호명)가 friend를 다시 1위로 올린다.
// alpha=0(기본 base_config)에서는 friend가 항상 intensity 1위라 interest 효과가 없음.
// ──────────────────────────────────────────────────────────────────────────────

fn alpha_active_config(theta: f64) -> EngineConfig {
    // 균일 off-diagonal alpha=0.5: 누군가 발화하면 모든 다른 페르소나에게 자극이 전파됨.
    // → chaos/summarizer가 burst를 받아 intensity가 friend를 역전하는 순간이 생김.
    // → 그 순간 interest 신호(friend 호명)가 friend를 다시 선택하게 함.
    let mut alpha = CouplingMatrix::default();
    let ids = ["friend", "chaos", "summarizer"];
    for &p in &ids {
        for &j in &ids {
            if p != j {
                alpha.values.insert((p.to_string(), j.to_string()), 0.5);
            }
        }
    }
    EngineConfig {
        beta: 0.5,
        theta,
        k: 60.0,
        tick_interval: 1.0,
        alpha,
        forbid_self_repeat: false,
    }
}

#[test]
fn content_naming_a_persona_steers_selection() {
    let personas = demo_personas();
    // alpha=0.5 균일 + theta=0.5: alpha burst로 실제 경쟁이 발생하는 설정.
    // friend(0.80) vs chaos(0.70)는 burst 구간에서 intensity가 역전되므로
    // interest 신호(friend 호명)가 그 구간마다 friend를 회복시킨다.
    let config = alpha_active_config(0.5);
    let seed = 42u64;
    let ticks = 300u64;

    // StubBackend: 항상 "friend"를 포함한 텍스트 반환
    // → interest 신호가 매 선택마다 friend에게 rank 1 부여
    let mut stub = StubBackend {
        reply: "friend 말이 맞아".to_string(),
    };
    let mut sink_stub = VecSink::default();
    driver::run(&config, &personas, seed, ticks, &mut sink_stub, &mut stub);

    // FakeBackend: content=None, interest 신호 비활성
    let mut sink_fake = VecSink::default();
    driver::run(
        &config,
        &personas,
        seed,
        ticks,
        &mut sink_fake,
        &mut FakeBackend,
    );

    let friend_stub = speak_count_for(&sink_stub, "friend");
    let friend_fake = speak_count_for(&sink_fake, "friend");

    // 경쟁이 실제로 일어났는지 확인 — 다른 화자도 발화해야 진정한 경쟁
    let chaos_stub = speak_count_for(&sink_stub, "chaos");
    let summarizer_stub = speak_count_for(&sink_stub, "summarizer");

    assert!(
        chaos_stub > 0 || summarizer_stub > 0,
        "stub 실행에서 friend 외 다른 화자가 없음 — 경쟁이 없어 interest 효과 측정 불가"
    );

    assert!(
        friend_stub > friend_fake,
        "friend 발화 수: stub({friend_stub}) > fake({friend_fake}) 이어야 함 \
         — interest content-signal이 호명된 후보를 띄워야 함 \
         (alpha burst 구간에서 interest가 friend를 역전 회복)"
    );
}
