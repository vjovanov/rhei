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

use crate::event::{EventSink, MessageLevel, RunEvent, Slot, TaskOutcome};

const CHANNEL_CAPACITY: usize = 1024;
const JOURNAL_BUFFER: usize = 200;

#[derive(Clone, Default)]
struct SlotState {
    task: Option<String>,
    state: String,
    started_at: Option<Instant>,
    log_path: Option<PathBuf>,
    last_event_display: Option<String>,
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
            RunEvent::SlotAssigned { slot, task, from, to, log_path, started_at, .. } => {
                if let Some(s) = self.slots.get_mut(*slot as usize) {
                    s.task = Some(task.clone());
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
        // Best effort — a full channel means the render thread is behind,
        // and we'd rather drop an event than block the engine.
        let _ = self.tx.try_send(Msg::Event(event));
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

    let mut lines: Vec<Line> = Vec::new();
    for (i, s) in snapshot.slots.iter().enumerate() {
        let i = i as Slot;
        if let Some(task) = &s.task {
            let elapsed = s.started_at.map(|t| t.elapsed()).unwrap_or_default();
            let elapsed_s = elapsed.as_secs();
            let transition = s.last_event_display.as_deref().unwrap_or(&s.state);
            lines.push(Line::from(vec![
                Span::styled(format!("[{i:>2}] "), Style::default().fg(Color::Cyan)),
                Span::styled(format!("{task:<28}"), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::styled(transition.to_string(), Style::default().fg(Color::Green)),
                Span::raw(format!("  {:>4}s", elapsed_s)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("[{i:>2}] "), Style::default().fg(Color::DarkGray)),
                Span::styled("— idle —", Style::default().fg(Color::DarkGray)),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
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
    use super::{handle_key_event, InputAction, UiState};
    use crossterm::event::{KeyCode, KeyModifiers};

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
}
