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

/// 틱 주기 기본값 (wall-clock). 생성은 off-thread이므로 UI는 이 주기마다 엔진을 1틱 전진.
/// 생성이 이 주기보다 빠르면 발화 간격 ≈ 이 주기 → 읽을 틈이 일정하게 보장된다.
/// 페이스는 주관적이라 `SALON_CHAT_TICK_MS` 환경변수로 런타임 튜닝 가능(아래 run()).
const TICK_PERIOD: Duration = Duration::from_millis(2000);
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
/// - `pending`  : true면 생성 진행 중 → 사이드바 하단에 "· 생각 중…" 표시(입력창은 건드리지 않음).
/// - `flow`     : 수렴/발산 지표. None이면 "흐름 -", Some이면 게이지 막대 + 값 표시.
/// - `mu_scale` : MetaController 식히기 비율. 1.00=식힘 없음, 낮을수록 강하게 식힘.
/// - `topics`   : 활성 화제 태그(최대 5개). 비어있으면 제목 "chat", 있으면 "chat · 화제: a · b".
pub fn render_chat(
    frame: &mut Frame,
    history: &[Event],
    intensities: &BTreeMap<PersonaId, f64>,
    names: &BTreeMap<PersonaId, String>,
    theta: f64,
    input: &str,
    pending: bool,
    flow: Option<crate::flow::FlowMetric>,
    mu_scale: f64,
    topics: &[String],
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
    // 채팅 pane 제목: 토픽 있으면 "chat · 화제: a · b", 없으면 "chat".
    let chat_title = if topics.is_empty() {
        "chat".to_string()
    } else {
        format!("chat · 화제: {}", topics.join(" · "))
    };
    frame.render_widget(
        Paragraph::new(chat_text).block(Block::default().title(chat_title).borders(Borders::ALL)),
        columns[0],
    );

    // ── 사이드바 (우측) ───────────────────────────────────────────────
    // 참여자 수: 페르소나(λ 게이지 있음) + 사람 "나"(외부 이벤트라 λ 없음).
    let persona_count = intensities.len();
    let mut gauge_lines: Vec<Line> = Vec::new();
    gauge_lines.push(Line::from(format!("나 + 페르소나 {persona_count}")));
    gauge_lines.push(Line::from(""));
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
    // 생성 진행 중 표시는 사이드바 하단에. 입력창은 타이핑에 방해되지 않게 건드리지 않는다.
    if pending {
        gauge_lines.push(Line::from(""));
        gauge_lines.push(Line::from("· 생각 중…"));
    }

    // ── 흐름 게이지 (persona-ui §5 "수렴 ▓░ 발산 ▓▓") ──────────────────
    // convergence ∈ [0,1]: 1.0=수렴(식는 중), 0.0=발산(살아있음).
    gauge_lines.push(Line::from(""));
    match flow {
        Some(m) => {
            // 너비 10 채움 막대: convergence 비율로 '#' 채우기.
            const FLOW_BAR_WIDTH: usize = 10;
            let filled = (m.convergence * FLOW_BAR_WIDTH as f64)
                .round()
                .clamp(0.0, FLOW_BAR_WIDTH as f64) as usize;
            let bar: String = (0..FLOW_BAR_WIDTH)
                .map(|i| if i < filled { '#' } else { '.' })
                .collect();
            gauge_lines.push(Line::from(vec![
                Span::raw(format!("흐름 수렴 {bar} {:.2}", m.convergence)),
            ]));
        }
        None => {
            gauge_lines.push(Line::from("흐름 -"));
        }
    }

    // ── 식힘 미터 (task-38) ────────────────────────────────────────────
    // mu_scale ∈ [floor, 1.0]: 1.00=식힘 없음, 낮을수록 MetaController가 강하게 식힘.
    gauge_lines.push(Line::from(format!("식힘 x{mu_scale:.2}")));

    frame.render_widget(
        Paragraph::new(gauge_lines).block(
            Block::default()
                .title(format!("참여자 {}명", persona_count + 1))
                .borders(Borders::ALL),
        ),
        columns[1],
    );

    // ── 입력창 (하단) ─────────────────────────────────────────────────
    // 입력창은 항상 깨끗하게 — "생각 중" 표시는 사이드바에만(입력 방해 방지).
    let input_text = format!("> {}", input);
    frame.render_widget(
        Paragraph::new(input_text).block(Block::default().borders(Borders::ALL)),
        root[1],
    );
}

// ─────────────────────────────────────────────
// 명령 파싱 유틸리티
// ─────────────────────────────────────────────

/// `/topic` 명령의 인자 부분을 파싱한다.
///
/// `rest`: `/topic` 이후의 문자열(앞뒤 공백 포함 가능).
/// - 쉼표로 분리 → 각 토큰 trim → 빈 문자열 제거 → 최대 5개.
/// - `rest`가 공백뿐이거나 비어있으면 빈 Vec 반환(clear 의미).
pub fn parse_topic_args(rest: &str) -> Vec<String> {
    rest.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .take(5)
        .collect()
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

        // 틱 주기: 기본 TICK_PERIOD(2초). SALON_CHAT_TICK_MS로 재컴파일 없이 조절.
        let tick_period = std::env::var("SALON_CHAT_TICK_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(TICK_PERIOD);

        loop {
            // ── 틱 전진 ──────────────────────────────────────────────
            if last_tick.elapsed() >= tick_period {
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
            // 수렴/발산 지표: 매 루프 갱신 (content 쌓이면 자동으로 움직임).
            let flow = self.session.flow();
            // 식힘 비율: MetaController가 현재 수렴도에서 계산. 사이드바 표시용.
            let mu_scale = self.session.mu_scale();
            // 활성 화제 태그: 채팅 pane 제목 + 생성 컨텍스트(live.rs 스냅샷 주입).
            let topics_snapshot = self.session.topics().to_vec();

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
                            flow,
                            mu_scale,
                            &topics_snapshot,
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

                                // Enter: `/`로 시작하면 명령, 아니면 human submit
                                KeyCode::Enter => {
                                    let line = input_buf.trim().to_string();
                                    if line.starts_with('/') {
                                        // 명령 라우팅: `/topic [args]` 처리.
                                        // 미인식 명령은 무시(입력 버퍼만 비움).
                                        if let Some(rest) = line.strip_prefix("/topic") {
                                            let topics = parse_topic_args(rest);
                                            self.session.set_topics(topics);
                                        }
                                        // 그 외 `/...` → 무시(버퍼는 아래에서 비움)
                                        input_buf.clear();
                                    } else if !line.is_empty() {
                                        self.session.submit_human(line);
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
            .draw(|f| render_chat(f, &history, &intensities(), &names(), 0.65, "", false, None, 1.0, &[]))
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
            .draw(|f| render_chat(f, &history, &intensities(), &names(), 0.65, "", false, None, 1.0, &[]))
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
                    None,
                    1.0,
                    &[],
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
            .draw(|f| render_chat(f, &history, &intensities(), &names(), 0.65, "", true, None, 1.0, &[]))
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

    // ── 테스트 5: parse_topic_args 단위 테스트 ──────────────────────────
    #[test]
    fn parse_topic_args_basic_split() {
        let result = parse_topic_args("rust, ai, 주말");
        assert_eq!(result, vec!["rust", "ai", "주말"]);
    }

    #[test]
    fn parse_topic_args_trims_and_drops_empty() {
        let result = parse_topic_args("  rust  ,, ,ai,");
        assert_eq!(result, vec!["rust", "ai"], "빈 항목·공백 제거");
    }

    #[test]
    fn parse_topic_args_caps_at_5() {
        let result = parse_topic_args("a,b,c,d,e,f");
        assert_eq!(result.len(), 5, "6개 → 5개 cap");
        assert_eq!(result[4], "e");
    }

    #[test]
    fn parse_topic_args_empty_returns_empty() {
        assert!(parse_topic_args("").is_empty());
        assert!(parse_topic_args("   ").is_empty());
    }

    // ── 테스트 6: render_chat 토픽 있을 때 채팅 pane 제목에 화제 표시 ──
    #[test]
    fn render_chat_title_shows_topics_when_set() {
        let history: Vec<Event> = Vec::new();
        let topics = vec!["rust".to_string(), "ai".to_string()];

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|f| {
                render_chat(
                    f,
                    &history,
                    &intensities(),
                    &names(),
                    0.65,
                    "",
                    false,
                    None,
                    1.0,
                    &topics,
                )
            })
            .expect("render ok");

        let text = buffer_text(&terminal);
        assert!(
            text.contains("rust"),
            "채팅 pane 제목에 화제 'rust'가 나타나야 한다. 실제: {text:?}"
        );
        assert!(
            text.contains("ai"),
            "채팅 pane 제목에 화제 'ai'가 나타나야 한다"
        );
    }

    // ── 테스트 7: render_chat 토픽 없을 때 제목은 "chat" ───────────────
    #[test]
    fn render_chat_title_is_chat_when_no_topics() {
        let history: Vec<Event> = Vec::new();

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|f| render_chat(f, &history, &intensities(), &names(), 0.65, "", false, None, 1.0, &[]))
            .expect("render ok");

        let text = buffer_text(&terminal);
        assert!(
            text.contains("chat"),
            "토픽 없을 때 채팅 pane 제목에 'chat'이 있어야 한다"
        );
    }
}
