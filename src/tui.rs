use crate::model::PersonaId;
use crate::sink::{ObservationRecord, ObservationSink};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, IsTerminal, Stdout, Write};
use std::time::Duration;

const LOG_LIMIT: usize = 50;
const BAR_WIDTH: usize = 12;

type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

pub fn render(
    frame: &mut Frame<'_>,
    record: &ObservationRecord,
    names: &BTreeMap<PersonaId, String>,
    theta: f64,
    log: &[String],
) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(frame.area());
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
        .split(root[0]);

    let event_log = log
        .iter()
        .rev()
        .take(columns[0].height.saturating_sub(2) as usize)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    frame.render_widget(
        Paragraph::new(event_log).block(Block::default().title("events").borders(Borders::ALL)),
        columns[0],
    );

    let mut gauge_lines = Vec::new();
    for (id, lambda) in &record.intensities {
        let name = names.get(id).map(String::as_str).unwrap_or(id.as_str());
        gauge_lines.push(Line::from(name.to_string()));
        let excitation_suffix = match record.excitations.get(id) {
            Some(&e) if e != 0.0 => format!(" +{e:.2}"),
            _ => String::new(),
        };
        gauge_lines.push(Line::from(vec![
            Span::raw(lambda_bar(*lambda, theta)),
            Span::raw(format!(" {lambda:.2}{excitation_suffix}")),
        ]));
    }
    gauge_lines.push(Line::from(""));
    gauge_lines.push(Line::from(format!(
        "speak {}  silence {}",
        record.speak_count, record.silence_count
    )));
    if record.chosen.is_none() {
        gauge_lines.push(Line::from("침묵 (silence)"));
    }

    frame.render_widget(
        Paragraph::new(gauge_lines).block(Block::default().title("gauges").borders(Borders::ALL)),
        columns[1],
    );

    let chosen = record
        .chosen
        .as_ref()
        .map(|id| names.get(id).map(String::as_str).unwrap_or(id.as_str()))
        .unwrap_or("침묵");
    let reason = record.rrf_reason.as_deref().unwrap_or("-");
    let status = format!(
        "tick {} | len {} | {} | reason: {}    [q] quit  [space] pause",
        record.tick, record.conversation_len, chosen, reason
    );
    frame.render_widget(
        Paragraph::new(status).block(Block::default().borders(Borders::ALL)),
        root[1],
    );
}

pub struct TuiSink {
    terminal: Option<TuiTerminal>,
    names: BTreeMap<PersonaId, String>,
    theta: f64,
    delay: Duration,
    log: VecDeque<String>,
    quit: bool,
    paused: bool,
    restored: bool,
}

impl TuiSink {
    pub fn new(
        names: BTreeMap<PersonaId, String>,
        theta: f64,
        delay: Duration,
    ) -> io::Result<Self> {
        let mut stdout = io::stdout();
        if !stdout.is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "stdout is not an interactive terminal",
            ));
        }

        enable_raw_mode()?;
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(error);
            }
        };

        Ok(Self {
            terminal: Some(terminal),
            names,
            theta,
            delay,
            log: VecDeque::with_capacity(LOG_LIMIT),
            quit: false,
            paused: false,
            restored: false,
        })
    }

    fn append_log(&mut self, record: &ObservationRecord) {
        if self.log.len() == LOG_LIMIT {
            self.log.pop_front();
        }
        let speaker = record
            .chosen
            .as_ref()
            .map(|id| {
                self.names
                    .get(id)
                    .map(String::as_str)
                    .unwrap_or(id.as_str())
            })
            .unwrap_or("(silence)");
        self.log.push_back(format!("t{} {}", record.tick, speaker));
    }

    fn poll_input(&mut self, delay: Duration) {
        match event::poll(delay) {
            Ok(true) => self.handle_next_event(),
            Ok(false) => {}
            Err(_) => self.quit = true,
        }
    }

    fn handle_next_event(&mut self) {
        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.quit = true;
                    let _ = self.restore();
                }
                KeyCode::Char(' ') => self.paused = !self.paused,
                _ => {}
            },
            Ok(_) => {}
            Err(_) => self.quit = true,
        }
    }

    fn wait_while_paused(&mut self) {
        while self.paused && !self.quit {
            self.poll_input(Duration::from_millis(100));
        }
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }

        let mut first_error = None;
        if let Some(mut terminal) = self.terminal.take() {
            if let Err(error) = terminal.show_cursor() {
                first_error = Some(error);
            }
            if let Err(error) = execute!(terminal.backend_mut(), LeaveAlternateScreen) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
            if let Err(error) = terminal.backend_mut().flush() {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        } else {
            let mut stdout = io::stdout();
            if let Err(error) = execute!(stdout, LeaveAlternateScreen) {
                first_error = Some(error);
            }
            if let Err(error) = stdout.flush() {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        if let Err(error) = disable_raw_mode() {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        self.restored = true;
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl ObservationSink for TuiSink {
    fn emit(&mut self, record: &ObservationRecord) {
        if self.quit {
            return;
        }

        self.append_log(record);
        let log = self.log.iter().cloned().collect::<Vec<_>>();
        if let Some(terminal) = self.terminal.as_mut() {
            if terminal
                .draw(|frame| render(frame, record, &self.names, self.theta, &log))
                .is_err()
            {
                self.quit = true;
                let _ = self.restore();
                return;
            }
        }

        self.poll_input(self.delay);
        self.wait_while_paused();
    }

    fn finish(&mut self) {
        let _ = self.restore();
    }
}

impl Drop for TuiSink {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub(crate) fn lambda_bar(lambda: f64, theta: f64) -> String {
    let scale = theta.max(lambda).max(1.0);
    let filled = ((lambda / scale) * BAR_WIDTH as f64)
        .round()
        .clamp(0.0, BAR_WIDTH as f64) as usize;
    let theta_pos = ((theta / scale) * BAR_WIDTH as f64)
        .round()
        .clamp(0.0, BAR_WIDTH as f64) as usize;

    let mut chars = vec!['.'; BAR_WIDTH + 1];
    for ch in chars.iter_mut().take(filled) {
        *ch = '#';
    }
    if let Some(ch) = chars.get_mut(theta_pos) {
        *ch = '|';
    }
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn names() -> BTreeMap<PersonaId, String> {
        BTreeMap::from([
            ("chaos".to_string(), "Chaos Guest".to_string()),
            ("friend".to_string(), "Friendly Regular".to_string()),
        ])
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn renders_speaker_record_with_gauges_counts_and_reason() {
        let mut intensities = BTreeMap::new();
        intensities.insert("chaos".to_string(), 0.73);
        intensities.insert("friend".to_string(), 0.81);
        let record = ObservationRecord {
            tick: 12,
            ts: 12.0,
            intensities,
            gate_passed: true,
            candidates: vec!["chaos".to_string(), "friend".to_string()],
            chosen: Some("friend".to_string()),
            rrf_reason: Some("rrf intensity rank won".to_string()),
            silence_count: 3,
            speak_count: 9,
            conversation_len: 12,
            excitations: BTreeMap::new(),
            utterance: None,
        };
        let names = names();
        let log = vec![
            "t11 Chaos Guest".to_string(),
            "t12 Friendly Regular".to_string(),
        ];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend terminal");

        terminal
            .draw(|frame| render(frame, &record, &names, 0.65, &log))
            .expect("render succeeds");

        let text = buffer_text(&terminal);
        assert!(text.contains("Chaos Guest"));
        assert!(text.contains("Friendly Regular"));
        assert!(text.contains("0.73"));
        assert!(text.contains("0.81"));
        assert!(text.contains("speak 9  silence 3"));
        assert!(text.contains("rrf intensity rank won"));
    }

    #[test]
    fn renders_silence_record_without_crashing() {
        let mut intensities = BTreeMap::new();
        intensities.insert("chaos".to_string(), 0.22);
        intensities.insert("friend".to_string(), 0.31);
        let record = ObservationRecord {
            tick: 13,
            ts: 13.0,
            intensities,
            gate_passed: false,
            candidates: Vec::new(),
            chosen: None,
            rrf_reason: None,
            silence_count: 4,
            speak_count: 9,
            conversation_len: 13,
            excitations: BTreeMap::new(),
            utterance: None,
        };
        let names = names();
        let log = vec!["t13 (silence)".to_string()];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend terminal");

        terminal
            .draw(|frame| render(frame, &record, &names, 0.65, &log))
            .expect("render succeeds");

        let text = buffer_text(&terminal);
        assert!(text.contains("(silence)"));
        assert!(text.contains("speak 9  silence 4"));
    }

    #[test]
    fn renders_excitation_annotation_when_present() {
        let mut intensities = BTreeMap::new();
        intensities.insert("chaos".to_string(), 0.73);
        intensities.insert("friend".to_string(), 0.81);
        let mut excitations = BTreeMap::new();
        excitations.insert("chaos".to_string(), 0.09);
        let record = ObservationRecord {
            tick: 20,
            ts: 20.0,
            intensities,
            gate_passed: true,
            candidates: vec!["chaos".to_string()],
            chosen: Some("chaos".to_string()),
            rrf_reason: Some("intensity".to_string()),
            silence_count: 1,
            speak_count: 5,
            conversation_len: 6,
            excitations,
            utterance: None,
        };
        let names = names();
        let log = vec!["t20 Chaos Guest".to_string()];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend terminal");

        terminal
            .draw(|frame| render(frame, &record, &names, 0.65, &log))
            .expect("render succeeds");

        let text = buffer_text(&terminal);
        // chaos 게이지에 자극 표시 +0.09가 나타나야 한다
        assert!(text.contains("+0.09"), "excitation annotation '+0.09' should appear for chaos");
        // friend는 excitations에 없으므로 friend의 lambda에 + 표시가 없어야 한다
        assert!(text.contains("0.81"), "friend lambda 0.81 should appear");
    }
}
