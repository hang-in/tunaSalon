//! TUI 미터를 TTY 없이 정적으로 미리보는 dev 도구.
//! `cargo run --example tui_preview` — 같은 `tui::render`를 TestBackend에 그려 텍스트로 덤프한다.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use salon::sink::ObservationRecord;
use salon::tui::render;
use std::collections::BTreeMap;

const W: u16 = 74;
const H: u16 = 15;

fn names() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("chaos".to_string(), "Chaos Guest".to_string()),
        ("friend".to_string(), "Friendly Regular".to_string()),
        ("summarizer".to_string(), "Quiet Summarizer".to_string()),
    ])
}

fn show(title: &str, record: &ObservationRecord, log: &[String]) {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).expect("terminal");
    terminal
        .draw(|frame| render(frame, record, &names(), 0.65, log))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    println!("### {title}");
    for y in 0..H {
        let mut line = String::new();
        for x in 0..W {
            line.push_str(buffer[(x, y)].symbol());
        }
        println!("{}", line.trim_end());
    }
    println!();
}

fn main() {
    let log = vec![
        "t6 Chaos Guest".to_string(),
        "t7 Friendly Regular".to_string(),
        "t8 (silence)".to_string(),
        "t9 (silence)".to_string(),
        "t10 Friendly Regular".to_string(),
    ];

    let speaking = ObservationRecord {
        tick: 10,
        ts: 10.0,
        intensities: BTreeMap::from([
            ("chaos".to_string(), 0.629),
            ("friend".to_string(), 0.666),
            ("summarizer".to_string(), 0.25),
        ]),
        gate_passed: true,
        candidates: vec!["chaos".to_string(), "friend".to_string()],
        chosen: Some("friend".to_string()),
        rrf_reason: Some("intensity".to_string()),
        silence_count: 6,
        speak_count: 11,
        conversation_len: 17,
    };

    let silence = ObservationRecord {
        tick: 11,
        ts: 11.0,
        intensities: BTreeMap::from([
            ("chaos".to_string(), 0.507),
            ("friend".to_string(), 0.436),
            ("summarizer".to_string(), 0.25),
        ]),
        gate_passed: false,
        candidates: Vec::new(),
        chosen: None,
        rrf_reason: None,
        silence_count: 7,
        speak_count: 11,
        conversation_len: 18,
    };

    show("발화 틱 (friend 당선)", &speaking, &log);
    show("침묵 틱", &silence, &log);
}
