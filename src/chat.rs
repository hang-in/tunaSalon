//! 채팅 TUI (v0.5 task-30).
//!
//! `render_chat`: 순수 렌더 함수 (LiveSession 결합 없음 → TestBackend로 단위 테스트 가능).
//! `ChatApp`: raw-mode + 이벤트 루프. LiveSession을 구동한다.
//! TuiSink와 동일한 restore 패턴 (show_cursor + LeaveAlternateScreen + disable_raw_mode, Drop).

use crate::live::LiveSession;
use crate::model::{Event, PersonaId};
use crossterm::event::{self, Event as CxEvent, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Stdout, Write};
use std::time::{Duration, Instant};

/// 틱 주기 (wall-clock). 생성은 off-thread이므로 UI는 이 주기마다 엔진을 1틱 전진.
const TICK_PERIOD: Duration = Duration::from_millis(700);
/// event::poll 타임아웃. 짧게 유지해 입력이 즉시 반응.
const POLL_TIMEOUT: Duration = Duration::from_millis(50);
/// 게이지 막대 너비 (tui.rs BAR_WIDTH와 동일).
#[allow(dead_code)]
const BAR_WIDTH: usize = 12;

// ─────────────────────────────────────────────
// 순수 렌더 함수 (TestBackend로 단위 테스트 가능)
// ─────────────────────────────────────────────

/// 채팅 화면을 렌더링한다 (LiveSession을 직접 받지 않음).
///
/// 레이아웃 (persona-ui §5):
///   - 세로: [상단 Min(3) | 입력창 Length(3)]
///   - 상단 가로: [채팅 pane 62% | 사이드바 38%]
///
/// - `history`  : 전체 대화 기록. 화면에 들어오는 최근 N줄만 표시.
/// - `intensities`: 페르소나별 현재 강도 (게이지 렌더).
/// - `names`    : PersonaId → 표시 이름.
/// - `theta`    : 게이트 임계값 (막대에 `|` 마커 표시).
/// - `input`    : 현재 입력 버퍼.
/// - `pending`  : true면 생성 진행 중 → 입력창에 "(…생각 중)" 힌트.
pub fn render_chat(
    frame: &mut Frame,
    history: &[Event],
    intensities: &BTreeMap<PersonaId, f64>,
    names: &BTreeMap<PersonaId, String>,
    theta: f64,
    input: &str,
    pending: bool,
) {
    // 세로 분할: 상단(채팅+사이드바) | 하단(입력창)
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(frame.area());

    // 상단 가로 분할: 채팅 62% | 사이드바 38%
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(root[0]);

    // ── 채팅 pane (좌측) ─────────────────────────────────────────────
    let chat_height = columns[0].height.saturating_sub(2) as usize; // 테두리 2줄 제외
    let chat_lines: Vec<String> = history
        .iter()
        .map(|ev| {
            let name = names
                .get(&ev.speaker)
                .map(String::as_str)
                .unwrap_or(ev.speaker.as_str());
            match &ev.content {
                Some(text) => format!("{}: {}", name, text),
                None => format!("{}: (생각 중)", name), // placeholder
            }
        })
        .rev()
        .take(chat_height)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let chat_text = chat_lines.join("\n");
    frame.render_widget(
        Paragraph::new(chat_text).block(Block::default().title("chat").borders(Borders::ALL)),
        columns[0],
    );

    // ── 사이드바 (우측) ───────────────────────────────────────────────
    let mut gauge_lines: Vec<Line> = Vec::new();
    for (id, &lambda) in intensities {
        let name = names.get(id).map(String::as_str).unwrap_or(id.as_str());
        // 이름 줄
        gauge_lines.push(Line::from(name.to_string()));
        // 게이지 막대 줄: lambda_bar 재사용 + 수치
        gauge_lines.push(Line::from(vec![
            Span::raw(crate::tui::lambda_bar(lambda, theta)),
            Span::raw(format!(" {lambda:.2}")),
        ]));
    }
    frame.render_widget(
        Paragraph::new(gauge_lines)
            .block(Block::default().title("gauges").borders(Borders::ALL)),
        columns[1],
    );

    // ── 입력창 (하단) ─────────────────────────────────────────────────
    let input_text = if pending {
        format!("> {} (…생각 중)", input)
    } else {
        format!("> {}", input)
    };
    frame.render_widget(
        Paragraph::new(input_text).block(Block::default().borders(Borders::ALL)),
        root[1],
    );
}

// ─────────────────────────────────────────────
// ChatApp: raw-mode 터미널 + 이벤트 루프
// ─────────────────────────────────────────────

/// 채팅 TUI 앱. LiveSession + ratatui Terminal을 소유한다.
///
/// raw-mode / AlternateScreen 복원은 `Drop`에서도 보장된다(TuiSink 패턴).
pub struct ChatApp {
    session: LiveSession,
    terminal: Option<Terminal<CrosstermBackend<Stdout>>>,
    names: BTreeMap<PersonaId, String>,
    theta: f64,
    /// restore가 이미 완료됐는지 이중 호출 방지.
    restored: bool,
}

impl ChatApp {
    /// ChatApp을 생성하고 raw-mode + AlternateScreen을 활성화한다.
    ///
    /// stdout이 터미널이 아니면 `Err`를 반환한다 (TuiSink 패턴과 동일).
    pub fn new(
        session: LiveSession,
        names: BTreeMap<PersonaId, String>,
        theta: f64,
    ) -> io::Result<Self> {
        let mut stdout = io::stdout();
        if !stdout.is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "stdout is not an interactive terminal",
            ));
        }

        enable_raw_mode()?;
        if let Err(e) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(e);
        }

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(t) => t,
            Err(e) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(e);
            }
        };

        Ok(Self {
            session,
            terminal: Some(terminal),
            names,
            theta,
            restored: false,
        })
    }

    /// 채팅 이벤트 루프를 실행한다.
    ///
    /// 종료 조건:
    /// - `Esc`
    /// - 입력 버퍼가 빈 상태에서 `q`
    ///
    /// 루프는 논블로킹:
    /// - 틱 주기(700ms)마다 `session.tick()`.
    /// - 매 루프마다 `session.poll_generation()` drain → history 반영.
    /// - `event::poll(50ms)` 타임아웃으로 키 입력 처리.
    pub fn run(&mut self) -> io::Result<()> {
        let mut input_buf = String::new();
        let mut last_tick = Instant::now();

        loop {
            // ── 틱 전진 ──────────────────────────────────────────────
            if last_tick.elapsed() >= TICK_PERIOD {
                self.session.tick();
                last_tick = Instant::now();
            }

            // ── 생성 결과 drain ───────────────────────────────────────
            while self.session.poll_generation().is_some() {}

            // ── 렌더 ─────────────────────────────────────────────────
            let history = self.session.state().history.clone();
            let intensities = self.session.combined_intensities();
            let pending = self.session.is_pending();
            let names = self.names.clone();
            let theta = self.theta;
            let input_snapshot = input_buf.clone();

            if let Some(ref mut terminal) = self.terminal {
                if terminal
                    .draw(|f| {
                        render_chat(
                            f,
                            &history,
                            &intensities,
                            &names,
                            theta,
                            &input_snapshot,
                            pending,
                        )
                    })
                    .is_err()
                {
                    break;
                }
            }

            // ── 키 입력 처리 ─────────────────────────────────────────
            match event::poll(POLL_TIMEOUT) {
                Ok(true) => {
                    match event::read() {
                        Ok(CxEvent::Key(key)) if key.kind == KeyEventKind::Press => {
                            match key.code {
                                // Esc: 항상 종료
                                KeyCode::Esc => break,

                                // q: 입력 버퍼가 비어있을 때만 종료 (버퍼에 'q'를 타이핑할 수 있도록)
                                KeyCode::Char('q') if input_buf.is_empty() => break,

                                // Enter: 입력 버퍼가 비어있지 않으면 human submit
                                KeyCode::Enter => {
                                    if !input_buf.is_empty() {
                                        self.session.submit_human(input_buf.clone());
                                        input_buf.clear();
                                    }
                                }

                                // Backspace: 버퍼 마지막 문자 제거
                                KeyCode::Backspace => {
                                    input_buf.pop();
                                }

                                // 일반 문자: 버퍼에 추가
                                KeyCode::Char(c) => {
                                    input_buf.push(c);
                                }

                                _ => {}
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
                Ok(false) => {}
                Err(_) => break,
            }
        }

        self.restore()?;
        Ok(())
    }

    /// raw-mode + AlternateScreen 복원. 이중 호출 안전.
    fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }

        let mut first_error: Option<io::Error> = None;

        if let Some(mut terminal) = self.terminal.take() {
            if let Err(e) = terminal.show_cursor() {
                first_error = Some(e);
            }
            if let Err(e) = execute!(terminal.backend_mut(), LeaveAlternateScreen) {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
            if let Err(e) = terminal.backend_mut().flush() {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        } else {
            let mut stdout = io::stdout();
            if let Err(e) = execute!(stdout, LeaveAlternateScreen) {
                first_error = Some(e);
            }
            if let Err(e) = stdout.flush() {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }

        if let Err(e) = disable_raw_mode() {
            if first_error.is_none() {
                first_error = Some(e);
            }
        }

        self.restored = true;
        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

impl Drop for ChatApp {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

// ─────────────────────────────────────────────
// 단위 테스트 (TestBackend; LiveSession 불요)
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// TestBackend 버퍼 전체를 문자열로 변환하는 헬퍼 (tui.rs 패턴과 동일).
    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    fn names() -> BTreeMap<PersonaId, String> {
        BTreeMap::from([
            ("aria".to_string(), "Aria".to_string()),
            ("you".to_string(), "You".to_string()),
        ])
    }

    fn intensities() -> BTreeMap<PersonaId, f64> {
        BTreeMap::from([
            ("aria".to_string(), 0.72),
        ])
    }

    // ── 테스트 1: 채팅 pane에 발화(사람 + 페르소나)가 렌더된다 ─────────
    //
    // TestBackend는 2-byte 한글 문자를 셀 단위로 저장하면서 다음 셀을 공백으로 채운다.
    // 따라서 한글 콘텐츠 대신 이름("Aria:"/", "You:") 위주로 검증한다.
    #[test]
    fn render_chat_shows_utterances() {
        let history = vec![
            // 사람 발화
            Event {
                ts: 0.0,
                speaker: "you".to_string(),
                mark: 0.0,
                content: Some("hello world".to_string()),
            },
            // 페르소나 발화
            Event {
                ts: 1.0,
                speaker: "aria".to_string(),
                mark: 0.0,
                content: Some("nice to meet you".to_string()),
            },
        ];

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|f| render_chat(f, &history, &intensities(), &names(), 0.65, "", false))
            .expect("render ok");

        let text = buffer_text(&terminal);
        // 사람 발화: 이름 + 콘텐츠
        assert!(
            text.contains("You:"),
            "사람 발화 이름이 채팅 pane에 나타나야 한다. 실제: {text:?}"
        );
        assert!(
            text.contains("hello world"),
            "사람 발화 내용이 채팅 pane에 나타나야 한다"
        );
        // 페르소나 발화: 이름 + 콘텐츠
        assert!(
            text.contains("Aria:"),
            "페르소나 발화 이름이 채팅 pane에 나타나야 한다"
        );
        assert!(
            text.contains("nice to meet you"),
            "페르소나 발화 내용이 채팅 pane에 나타나야 한다"
        );
    }

    // ── 테스트 2: 사이드바에 페르소나 이름 + 게이지 막대가 나온다 ───────
    #[test]
    fn render_chat_sidebar_shows_name_and_gauge() {
        let history: Vec<Event> = Vec::new();

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|f| render_chat(f, &history, &intensities(), &names(), 0.65, "", false))
            .expect("render ok");

        let text = buffer_text(&terminal);
        // 사이드바에 페르소나 이름 표시
        assert!(
            text.contains("Aria"),
            "사이드바에 페르소나 이름 'Aria'가 있어야 한다"
        );
        // 게이지 막대 문자('#' 또는 '.' 또는 '|') 존재
        assert!(
            text.contains('#') || text.contains('|'),
            "사이드바에 게이지 막대 문자(# 또는 |)가 있어야 한다"
        );
        // 강도 값 표시
        assert!(
            text.contains("0.72"),
            "사이드바에 강도 값 0.72가 있어야 한다"
        );
    }

    // ── 테스트 3: 입력창에 `> {input}` 버퍼가 나온다 ───────────────────
    //
    // 입력 버퍼에 ASCII 문자열을 사용해 TestBackend 한글 공백 문제를 우회한다.
    #[test]
    fn render_chat_input_box_shows_buffer() {
        let history: Vec<Event> = Vec::new();

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|f| {
                render_chat(
                    f,
                    &history,
                    &intensities(),
                    &names(),
                    0.65,
                    "test-input-buffer",
                    false,
                )
            })
            .expect("render ok");

        let text = buffer_text(&terminal);
        // 입력 버퍼 내용이 표시돼야 한다
        assert!(
            text.contains("test-input-buffer"),
            "입력창에 버퍼 내용 'test-input-buffer'가 나타나야 한다. 실제: {text:?}"
        );
        // '>' 프롬프트가 있어야 한다
        assert!(
            text.contains('>'),
            "입력창에 '>' 프롬프트가 있어야 한다"
        );
    }

    // ── 테스트 4: placeholder(content None) → panic 없이 "생각 중" 표시 ─
    //
    // TestBackend는 2-byte 한글을 셀+공백으로 나눈다: "생각 중" → "생 각  중".
    // "생"(첫 글자)의 존재로 마커 렌더를 확인하고, 발화자 이름이 앞에 나타남을 검증한다.
    #[test]
    fn render_chat_placeholder_event_shows_marker_without_panic() {
        let history = vec![
            // placeholder: content = None
            Event {
                ts: 2.0,
                speaker: "aria".to_string(),
                mark: 0.0,
                content: None,
            },
        ];

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        // panic이 없어야 한다
        terminal
            .draw(|f| render_chat(f, &history, &intensities(), &names(), 0.65, "", true))
            .expect("render ok (panic 없음)");

        let text = buffer_text(&terminal);
        // "Aria:" 이름이 나타나야 한다
        assert!(
            text.contains("Aria:"),
            "placeholder event에 발화자 이름이 나타나야 한다. 실제: {text:?}"
        );
        // "생" 문자가 나타나야 한다 (TestBackend 셀 분리로 공백이 삽입되지만 첫 글자는 존재)
        assert!(
            text.contains('생'),
            "placeholder event에 '생각 중' 마커의 '생' 자가 나타나야 한다. 실제: {text:?}"
        );
    }
}
