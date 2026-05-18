use std::io::{self, Stdout};
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
use ratatui::Terminal;

use crate::event::{EventSink, RunEvent};

mod render;
mod state;
mod text;

use render::draw;
use state::UiState;

const CHANNEL_CAPACITY: usize = 1024;
const JOURNAL_BUFFER: usize = 200;
const SLOT_TRAFFIC_BUFFER: usize = 50;
const JOURNAL_TRAFFIC_WIDTH: usize = 120;

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

#[cfg(test)]
mod tests;
