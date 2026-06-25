//! 채팅방(`--chat`) 엔진 config의 구조적 보장을 결정적으로 검증한다(FakeBackend, LLM 없음).
//!
//! 사용자 관찰(2026-06-03 라이브) + "이건 스모크로 되는 거 아닌가?" 제안에서 출발:
//! 발화 *내용*(gemma가 따르는지)은 비결정이라 라이브 영역이지만, **누가/언제 말하나(구조)**는
//! 결정적이라 여기서 박는다.
//!
//! 검증 대상 = `main.rs`의 `--chat` config를 그대로 재구성:
//!   chat_personas(μ 0.70/0.62/0.55) + RoomPreset::Pub 교차자극 + theta 0.60 + forbid_self_repeat.
//! 보장:
//!   1. **자기연속 0**: 같은 화자가 2연속으로 선택되지 않는다(자기 말 받아치기 구조적 차단).
//!   2. **3-way 참여**: 세 페르소나 모두 발화하며 friend > realist > summarizer 순(quiet하지만 참여).
//!
//! 이 config는 반복 헤드리스 측정으로 튜닝됨: seed42/300틱 → friend~110·chaos~77·summarizer~59.

use salon::driver;
use salon::model::{Persona, PersonaId, PersonaModifier};
use salon::preset::RoomPreset;
use salon::runtime::FakeBackend;
use salon::sink::VecSink;
use std::collections::BTreeMap;

const SEED: u64 = 42;
const TICKS: u64 = 300;

/// `main.rs::chat_personas`와 동일(μ 0.70/0.62/0.55). 드리프트 방지를 위해 값 일치 유지.
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

/// `main.rs::demo_persona_modifiers`와 동일.
fn chat_modifiers() -> BTreeMap<PersonaId, PersonaModifier> {
    BTreeMap::from([
        (
            "friend".to_string(),
            PersonaModifier {
                reactivity: 2.0,
                provocativeness: 1.0,
            },
        ),
        (
            "chaos".to_string(),
            PersonaModifier {
                reactivity: 1.0,
                provocativeness: 0.8,
            },
        ),
        (
            "summarizer".to_string(),
            PersonaModifier {
                reactivity: 1.0,
                provocativeness: 0.5,
            },
        ),
    ])
}

/// `main.rs`의 `--chat` 엔진 config를 재구성한다.
fn chat_config(personas: &[Persona]) -> salon::model::EngineConfig {
    let mut c = RoomPreset::Pub.build_config_with_modifiers(personas, &chat_modifiers());
    c.theta = 0.60;
    c.forbid_self_repeat = true;
    c
}

fn run() -> VecSink {
    let personas = chat_personas();
    let config = chat_config(&personas);
    let mut sink = VecSink::default();
    driver::run(&config, &personas, SEED, TICKS, &mut sink, &mut FakeBackend);
    sink
}

/// 발화한 화자 시퀀스(침묵 제외).
fn spoken(sink: &VecSink) -> Vec<&PersonaId> {
    sink.records
        .iter()
        .filter_map(|r| r.chosen.as_ref())
        .collect()
}

// ── (1) 자기연속 0: 같은 화자가 연속으로 선택되지 않는다 ──────────────────────────
#[test]
fn chat_config_never_picks_same_speaker_twice_in_a_row() {
    let sink = run();
    let seq = spoken(&sink);
    let self_repeats = seq.windows(2).filter(|w| w[0] == w[1]).count();
    assert_eq!(
        self_repeats, 0,
        "채팅 config는 같은 화자 2연속을 허용하면 안 된다(자기 말 받아치기). 발견: {self_repeats}건"
    );
}

// ── (2) 3-way 참여: 세 페르소나 모두 발화, friend > realist > summarizer ──────────
#[test]
fn chat_config_gives_three_way_participation() {
    let sink = run();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for s in spoken(&sink) {
        *counts.entry(s.as_str()).or_default() += 1;
    }
    let friend = *counts.get("friend").unwrap_or(&0);
    let realist = *counts.get("chaos").unwrap_or(&0); // id는 "chaos"(realist 역할)
    let summarizer = *counts.get("summarizer").unwrap_or(&0);

    // 세 명 모두 참여
    assert!(
        friend > 0 && realist > 0 && summarizer > 0,
        "세 페르소나 모두 발화해야 한다. friend={friend} realist={realist} summarizer={summarizer}"
    );
    // summarizer가 "quiet하지만 분명히" 참여(측정상 ~59/300, 보수적으로 ≥30)
    assert!(
        summarizer >= 30,
        "summarizer가 충분히 참여해야 한다(독점 방지). 실제={summarizer} (기대 ≥30)"
    );
    // 순서: friend ≥ realist ≥ summarizer (friend가 가장 활발, summarizer가 가장 조용)
    assert!(friend >= realist && realist >= summarizer,
        "참여 순서 friend ≥ realist ≥ summarizer 기대. friend={friend} realist={realist} summarizer={summarizer}");
}
