use std::collections::VecDeque;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Sender};
use crossterm::event::{self as ctevent, Event as CtEvent, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use nix::sys::signal::{raise, Signal};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;

use crate::event::{AgentStream, EventSink, MessageLevel, RunEvent, Slot, TaskOutcome};

const CHANNEL_CAPACITY: usize = 1024;
const JOURNAL_BUFFER: usize = 200;
const SLOT_TRAFFIC_BUFFER: usize = 50;
const JOURNAL_TRAFFIC_WIDTH: usize = 120;

#[derive(Clone, Default)]
struct SlotState {
    task: Option<String>,
    agent: Option<String>,
    state: String,
    started_at: Option<Instant>,
    log_path: Option<PathBuf>,
    last_event_display: Option<String>,
    traffic: VecDeque<TrafficLine>,
}

#[derive(Clone)]
struct TrafficLine {
    stream: AgentStream,
    text: String,
}

struct UiState {
    parallel: u16,
    total_tasks: usize,
    slots: Vec<SlotState>,
    journal: VecDeque<String>,
    finished: bool,
}

impl UiState {
    fn new(parallel: u16, total_tasks: usize) -> Self {
        let parallel = parallel.max(1);
        Self {
            parallel,
            total_tasks,
            slots: vec![SlotState::default(); parallel as usize],
            journal: VecDeque::with_capacity(JOURNAL_BUFFER),
            finished: false,
        }
    }

    fn push_journal(&mut self, line: String) {
        if self.journal.len() == JOURNAL_BUFFER {
            self.journal.pop_front();
        }
        self.journal.push_back(line);
    }

    fn apply(&mut self, event: &RunEvent) {
        match event {
            RunEvent::RunStarted { parallel, total_tasks, .. } => {
                self.parallel = (*parallel).max(1);
                self.total_tasks = *total_tasks;
                self.slots = vec![SlotState::default(); self.parallel as usize];
                self.push_journal(format!(
                    "run started — parallel={} total={}",
                    self.parallel, self.total_tasks
                ));
            }
            RunEvent::PassStarted { pass, ready } => {
                self.push_journal(format!("pass {}: {} ready", pass, ready.len()));
            }
            RunEvent::SlotAssigned {
                slot, task, from, to, agent, log_path, started_at, ..
            } => {
                if let Some(s) = self.slots.get_mut(*slot as usize) {
                    s.task = Some(task.clone());
                    s.agent = agent.clone();
                    s.state = to.clone();
                    s.started_at = Some(*started_at);
                    s.log_path = Some(log_path.clone());
                    s.last_event_display = Some(format!("{from}→{to}"));
                }
                self.push_journal(format!("▶ slot {}: {} {}→{}", slot, task, from, to));
            }
            RunEvent::SlotReleased { slot, task, outcome, duration_ms, .. } => {
                let sym = match outcome {
                    TaskOutcome::Completed => "✓",
                    TaskOutcome::Failed(_) => "✗",
                    TaskOutcome::Cancelled => "⊘",
                    TaskOutcome::TimedOut => "⏱",
                };
                if let Some(s) = self.slots.get_mut(*slot as usize) {
                    *s = SlotState::default();
                }
                self.push_journal(format!("{} slot {}: {} ({}ms)", sym, slot, task, duration_ms));
            }
            RunEvent::AgentOutput { slot, stream, line, .. } => {
                let line = sanitize_terminal_text(line);
                let stream_label = stream_label(*stream);
                let journal_prefix = match stream {
                    AgentStream::Stdout => "·",
                    AgentStream::Stderr => "!",
                };
                if let Some(s) = self.slots.get_mut(*slot as usize) {
                    if s.traffic.len() == SLOT_TRAFFIC_BUFFER {
                        s.traffic.pop_front();
                    }
                    s.traffic.push_back(TrafficLine { stream: *stream, text: line.clone() });
                }
                self.push_journal(format!(
                    "{journal_prefix} [slot {slot} {stream_label}] {}",
                    truncate_chars(&line, JOURNAL_TRAFFIC_WIDTH)
                ));
            }
            RunEvent::PassEnded { pass, progressed } => {
                self.push_journal(format!("pass {} ended — progressed={}", pass, progressed));
            }
            RunEvent::RunFinished { summary } => {
                self.finished = true;
                self.push_journal(format!(
                    "run finished — agents={} programs={} terminal={}/{}",
                    summary.agents_spawned,
                    summary.programs_spawned,
                    summary.terminal_tasks,
                    summary.total_tasks
                ));
            }
            RunEvent::Message { level, text } => {
                let prefix = match level {
                    MessageLevel::Info => "·",
                    MessageLevel::Warn => "!",
                    MessageLevel::Error => "✗",
                };
                self.push_journal(format!("{prefix} {text}"));
            }
        }
    }
}

/// Live TUI frontend backed by ratatui + crossterm.
///
/// The sink owns a bounded event channel and a render thread. Drop (or
/// `finish`) joins the render thread and restores the terminal.
pub struct TuiSink {
    tx: Sender<Msg>,
    join: Mutex<Option<JoinHandle<()>>>,
}

enum Msg {
    Event(RunEvent),
    Shutdown,
}

enum InputAction {
    Continue,
    ForwardSigint,
}

impl TuiSink {
    pub fn start(parallel: u16, total_tasks: usize) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        stdout.execute(EnterAlternateScreen)?;

        // Panic hook: if the engine panics, restore the terminal before the
        // default handler prints its message, so the user sees the panic.
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = io::stdout().execute(LeaveAlternateScreen);
            prev_hook(info);
        }));

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let (tx, rx) = bounded::<Msg>(CHANNEL_CAPACITY);
        let state = Arc::new(Mutex::new(UiState::new(parallel, total_tasks)));

        let handle = thread::spawn({
            let state = Arc::clone(&state);
            move || render_loop(terminal, rx, state)
        });

        Ok(Self { tx, join: Mutex::new(Some(handle)) })
    }

    /// Signal the render thread to exit and wait for it. Safe to call twice.
    pub fn finish(&self) {
        let _ = self.tx.send(Msg::Shutdown);
        let mut guard = match self.join.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(handle) = guard.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for TuiSink {
    fn drop(&mut self) {
        self.finish();
    }
}

impl EventSink for TuiSink {
    fn emit(&self, event: RunEvent) {
        if matches!(event, RunEvent::AgentOutput { .. }) {
            // Agent output is best-effort because the durable per-task log has
            // the full transcript. Dropping here keeps output bursts from
            // filling the shared channel indefinitely.
            let _ = self.tx.try_send(Msg::Event(event));
        } else {
            // Lifecycle events define slot state. Preserve them even during
            // output floods so the UI cannot get stuck showing stale work.
            let _ = self.tx.send(Msg::Event(event));
        }
    }
}

fn render_loop(
    mut terminal: Terminal<CrosstermBackend<Stdout>>,
    rx: crossbeam_channel::Receiver<Msg>,
    state: Arc<Mutex<UiState>>,
) {
    let tick = Duration::from_millis(250);
    let mut last_draw = Instant::now().checked_sub(tick).unwrap_or_else(Instant::now);

    loop {
        // Drain any pending events until the channel is empty or timeout.
        let deadline = Instant::now() + tick;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(Msg::Event(event)) => {
                    if let Ok(mut s) = state.lock() {
                        s.apply(&event);
                    }
                }
                Ok(Msg::Shutdown) => {
                    draw(&mut terminal, &state);
                    break_out(terminal);
                    return;
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => break,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    draw(&mut terminal, &state);
                    break_out(terminal);
                    return;
                }
            }
        }

        // Drain terminal input (non-blocking) so Ctrl+C etc. is visible in the
        // logs. In raw mode Ctrl+C no longer generates SIGINT automatically,
        // so the TUI has to forward it to the process itself.
        while ctevent::poll(Duration::from_millis(0)).unwrap_or(false) {
            if let Ok(CtEvent::Key(key)) = ctevent::read() {
                let action = match state.lock() {
                    Ok(mut s) => handle_key_event(&mut s, key.code, key.modifiers),
                    Err(p) => {
                        let mut s = p.into_inner();
                        handle_key_event(&mut s, key.code, key.modifiers)
                    }
                };
                if matches!(action, InputAction::ForwardSigint) {
                    draw(&mut terminal, &state);
                    break_out(terminal);
                    let _ = forward_sigint_to_self();
                    return;
                }
            }
        }

        if last_draw.elapsed() >= tick {
            draw(&mut terminal, &state);
            last_draw = Instant::now();
        }
    }
}

fn break_out(mut terminal: Terminal<CrosstermBackend<Stdout>>) {
    let _ = terminal.show_cursor();
    let _ = disable_raw_mode();
    let _ = io::stdout().execute(LeaveAlternateScreen);
}

fn handle_key_event(state: &mut UiState, code: KeyCode, modifiers: KeyModifiers) -> InputAction {
    if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
        state.push_journal("(ctrl+c received — forwarding SIGINT)".to_string());
        return InputAction::ForwardSigint;
    }

    InputAction::Continue
}

fn forward_sigint_to_self() -> nix::Result<()> {
    raise(Signal::SIGINT)
}

fn draw(terminal: &mut Terminal<CrosstermBackend<Stdout>>, state: &Arc<Mutex<UiState>>) {
    let snapshot = match state.lock() {
        Ok(s) => s.clone_snapshot(),
        Err(p) => p.into_inner().clone_snapshot(),
    };

    let _ = terminal.draw(|f| {
        let area = f.size();
        if area.height < 4 || area.width < 20 {
            // Terminal too small — render a single line.
            let msg = Paragraph::new("rhei run (terminal too small)")
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(msg, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(snapshot.parallel.max(1) + 2),
                Constraint::Min(5), // journal pane
            ])
            .split(area);

        render_header(f, chunks[0], &snapshot);
        render_slots(f, chunks[1], &snapshot);
        render_journal(f, chunks[2], &snapshot);
    });
}

fn render_header(f: &mut ratatui::Frame, area: Rect, snapshot: &UiStateSnapshot) {
    let active = snapshot.slots.iter().filter(|s| s.task.is_some()).count();
    let line = Line::from(vec![
        Span::styled("rhei run", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!(
            "  parallel={} active={} total_tasks={}{}",
            snapshot.parallel,
            active,
            snapshot.total_tasks,
            if snapshot.finished { "  [finished]" } else { "" },
        )),
    ]);
    let block = Block::default().borders(Borders::BOTTOM);
    f.render_widget(Paragraph::new(line).block(block), area);
}

fn render_slots(f: &mut ratatui::Frame, area: Rect, snapshot: &UiStateSnapshot) {
    let block = Block::default().title(" slots ").borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let lines = slot_lines(snapshot, inner.width, inner.height);
    f.render_widget(Paragraph::new(lines), inner);
}

fn slot_lines(snapshot: &UiStateSnapshot, width: u16, height: u16) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    for (idx, s) in snapshot.slots.iter().enumerate() {
        let remaining_slots = snapshot.slots.len().saturating_sub(idx + 1);
        let available_rows = height as usize;
        if lines.len() >= available_rows {
            break;
        }

        let i = idx as Slot;
        if let Some(task) = &s.task {
            let elapsed = s.started_at.map(|t| t.elapsed()).unwrap_or_default();
            let elapsed_s = elapsed.as_secs();
            let transition = s.last_event_display.as_deref().unwrap_or(&s.state);
            let task_label = s
                .agent
                .as_ref()
                .map(|agent| format!("{task} ({agent})"))
                .unwrap_or_else(|| task.clone());
            lines.push(Line::from(vec![
                Span::styled(format!("[{i:>2}] "), Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{task_label:<28}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(transition.to_string(), Style::default().fg(Color::Green)),
                Span::raw(format!("  {:>4}s", elapsed_s)),
            ]));
            let traffic_room =
                available_rows.saturating_sub(lines.len()).saturating_sub(remaining_slots);
            let traffic_tail = traffic_room.min(5);
            if traffic_tail > 0 {
                for traffic in s.traffic.iter().rev().take(traffic_tail).rev() {
                    let (label, style) = match traffic.stream {
                        AgentStream::Stdout => ("out", Style::default().fg(Color::Gray)),
                        AgentStream::Stderr => ("err", Style::default().fg(Color::Yellow)),
                    };
                    lines.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(format!("{label}> "), style),
                        Span::raw(truncate_chars(&traffic.text, width.saturating_sub(11) as usize)),
                    ]));
                }
            }
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("[{i:>2}] "), Style::default().fg(Color::DarkGray)),
                Span::styled("— idle —", Style::default().fg(Color::DarkGray)),
            ]));
        }
    }
    lines
}

fn stream_label(stream: AgentStream) -> &'static str {
    match stream {
        AgentStream::Stdout => "stdout",
        AgentStream::Stderr => "stderr",
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut chars = value.chars();
    let mut out = String::new();
    for _ in 0..max_chars {
        if let Some(ch) = chars.next() {
            out.push(ch);
        } else {
            return out;
        }
    }
    if chars.next().is_some() {
        if max_chars > 1 {
            out.pop();
        }
        out.push('…');
    }
    out
}

fn sanitize_terminal_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if matches!(chars.peek(), Some('[')) {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        if ch.is_control() && ch != '\t' {
            continue;
        }
        out.push(ch);
    }
    out
}

fn render_journal(f: &mut ratatui::Frame, area: Rect, snapshot: &UiStateSnapshot) {
    let block = Block::default().title(" journal ").borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let height = inner.height as usize;
    let lines: Vec<Line> =
        snapshot.journal.iter().rev().take(height).rev().map(|l| Line::from(l.clone())).collect();
    f.render_widget(Paragraph::new(lines), inner);
}

struct UiStateSnapshot {
    parallel: u16,
    total_tasks: usize,
    slots: Vec<SlotState>,
    journal: Vec<String>,
    finished: bool,
}

impl UiState {
    fn clone_snapshot(&self) -> UiStateSnapshot {
        UiStateSnapshot {
            parallel: self.parallel,
            total_tasks: self.total_tasks,
            slots: self.slots.clone(),
            journal: self.journal.iter().cloned().collect(),
            finished: self.finished,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        handle_key_event, sanitize_terminal_text, slot_lines, truncate_chars, InputAction, UiState,
        SLOT_TRAFFIC_BUFFER,
    };
    use crate::event::{AgentStream, RunEvent};
    use crossterm::event::{KeyCode, KeyModifiers};
    use std::path::PathBuf;
    use std::time::{Instant, SystemTime};

    #[test]
    fn ctrl_c_requests_sigint_forwarding() {
        let mut state = UiState::new(1, 1);

        let action = handle_key_event(&mut state, KeyCode::Char('c'), KeyModifiers::CONTROL);

        assert!(matches!(action, InputAction::ForwardSigint));
        assert_eq!(
            state.journal.back().map(String::as_str),
            Some("(ctrl+c received — forwarding SIGINT)")
        );
    }

    #[test]
    fn non_ctrl_c_input_is_ignored() {
        let mut state = UiState::new(1, 1);

        let action = handle_key_event(&mut state, KeyCode::Char('q'), KeyModifiers::NONE);

        assert!(matches!(action, InputAction::Continue));
        assert!(state.journal.is_empty());
    }

    #[test]
    fn agent_output_is_added_to_slot_and_journal() {
        let mut state = UiState::new(1, 1);
        state.apply(&RunEvent::AgentOutput {
            slot: 0,
            task: "task-1".to_string(),
            stream: AgentStream::Stdout,
            line: "hello".to_string(),
            wall_clock: SystemTime::now(),
        });

        assert_eq!(state.slots[0].traffic.len(), 1);
        assert_eq!(state.slots[0].traffic[0].text, "hello");
        assert_eq!(state.journal.back().map(String::as_str), Some("· [slot 0 stdout] hello"));
    }

    #[test]
    fn agent_output_retention_is_bounded() {
        let mut state = UiState::new(1, 1);
        for i in 0..(SLOT_TRAFFIC_BUFFER + 2) {
            state.apply(&RunEvent::AgentOutput {
                slot: 0,
                task: "task-1".to_string(),
                stream: AgentStream::Stderr,
                line: format!("line {i}"),
                wall_clock: SystemTime::now(),
            });
        }

        assert_eq!(state.slots[0].traffic.len(), SLOT_TRAFFIC_BUFFER);
        assert_eq!(state.slots[0].traffic[0].text, "line 2");
    }

    #[test]
    fn unknown_slot_output_does_not_panic() {
        let mut state = UiState::new(1, 1);
        state.apply(&RunEvent::AgentOutput {
            slot: 9,
            task: "task-1".to_string(),
            stream: AgentStream::Stdout,
            line: "orphan".to_string(),
            wall_clock: SystemTime::now(),
        });

        assert!(state.slots[0].traffic.is_empty());
        assert_eq!(state.journal.back().map(String::as_str), Some("· [slot 9 stdout] orphan"));
    }

    #[test]
    fn sanitizes_control_sequences_for_display() {
        assert_eq!(sanitize_terminal_text("\u{1b}[31mred\u{1b}[0m"), "red");
        assert_eq!(sanitize_terminal_text("a\u{7}b"), "ab");
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate_chars("abcdef", 4), "abc…");
        assert_eq!(truncate_chars("abc", 4), "abc");
    }

    #[test]
    fn slot_lines_reserve_rows_for_later_slots() {
        let mut state = UiState::new(3, 3);
        for slot in 0..3 {
            state.apply(&RunEvent::SlotAssigned {
                slot,
                task: format!("task-{slot}"),
                from: "fetch".to_string(),
                to: "fetch".to_string(),
                agent: Some("codex".to_string()),
                log_path: PathBuf::from(format!("task-{slot}.log")),
                started_at: Instant::now(),
                wall_clock: SystemTime::now(),
            });
        }
        for i in 0..10 {
            state.apply(&RunEvent::AgentOutput {
                slot: 0,
                task: "task-0".to_string(),
                stream: AgentStream::Stdout,
                line: format!("line {i}"),
                wall_clock: SystemTime::now(),
            });
        }

        let snapshot = state.clone_snapshot();
        let lines = slot_lines(&snapshot, 100, 3);
        let rendered = lines
            .iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>();

        assert_eq!(rendered.len(), 3);
        assert!(rendered.iter().any(|line| line.contains("task-0")));
        assert!(rendered.iter().any(|line| line.contains("task-1")));
        assert!(rendered.iter().any(|line| line.contains("task-2")));
    }
}
