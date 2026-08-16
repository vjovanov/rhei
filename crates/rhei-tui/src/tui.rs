use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Sender};
use crossterm::event::{self as ctevent, Event as CtEvent, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
#[cfg(unix)]
use nix::sys::signal::{raise, Signal};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::dashboard::{GateTransitionSink, InterveneSink, PlanLoader};
use crate::event::{EventSink, MessageLevel, RunEvent};

mod derive;
mod input;
mod render;
mod state;
mod text;
mod theme;
mod views;

use input::{handle_key_event, InputAction};
use state::UiState;

const CHANNEL_CAPACITY: usize = 1024;
const JOURNAL_BUFFER: usize = 400;
const SLOT_TRAFFIC_BUFFER: usize = 50;

/// Everything the Flow surface needs beyond parallelism and task count: the
/// workspace root, the plan loader (shared with the dashboard), and the two
/// live-action boundaries. §FS-rhei-run-tui.1.5
pub struct TuiContext {
    pub workspace: PathBuf,
    pub plan_loader: Option<PlanLoader>,
    pub intervene: Option<Arc<dyn InterveneSink>>,
    pub gate: Option<Arc<dyn GateTransitionSink>>,
}

pub struct TuiSink {
    tx: Sender<Msg>,
    join: Mutex<Option<JoinHandle<()>>>,
    /// Raised by the render thread the instant it leaves the alternate screen,
    /// and by the panic hook. From then on there is no journal pane to receive
    /// a message: the channel would still accept one — the receiver outlives
    /// the restore by the width of a `return` — and it would be swallowed.
    // §FS-rhei-run-tui.1.8
    screen_restored: Arc<AtomicBool>,
}

/// Where a message belongs once the screen may be gone.
///
/// Warnings and errors are the only events with a plain-text form the operator
/// can read on a bare terminal, and the only ones worth interrupting them with;
/// everything else is journal or dashboard state that a restored screen has no
/// place to show.
// §FS-rhei-run-tui.1.8
fn message_goes_to_stderr(screen_restored: bool, event: &RunEvent) -> bool {
    screen_restored
        && matches!(
            event,
            RunEvent::Message { level: MessageLevel::Warn | MessageLevel::Error, .. }
        )
}

enum Msg {
    Event(Box<RunEvent>),
    Shutdown,
}

impl TuiSink {
    /// Start the render thread. `context` carries the plan loader and live-action
    /// sinks; pass an empty context for a self-contained surface.
    pub fn start(parallel: u16, total_tasks: usize, context: TuiContext) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        stdout.execute(EnterAlternateScreen)?;

        let screen_restored = Arc::new(AtomicBool::new(false));

        // Panic hook: if the engine panics, restore the terminal before the
        // default handler prints its message, so the user sees the panic. §1.8
        let prev_hook = std::panic::take_hook();
        let panic_restored = Arc::clone(&screen_restored);
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = io::stdout().execute(LeaveAlternateScreen);
            panic_restored.store(true, Ordering::SeqCst);
            prev_hook(info);
        }));

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let (tx, rx) = bounded::<Msg>(CHANNEL_CAPACITY);
        let state = UiState::with_context(
            context.workspace,
            parallel.max(1),
            total_tasks,
            context.plan_loader,
            context.intervene,
            context.gate,
        );

        let loop_restored = Arc::clone(&screen_restored);
        let handle = thread::spawn(move || render_loop(terminal, rx, state, &loop_restored));

        Ok(Self { tx, join: Mutex::new(Some(handle)), screen_restored })
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
        // The screen is gone; stderr is the only surface left. Sending instead
        // would succeed and vanish — the run's shutdown notice was arriving in
        // exactly this window. §FS-rhei-run-tui.1.8
        if message_goes_to_stderr(self.screen_restored.load(Ordering::SeqCst), &event) {
            if let RunEvent::Message { text, .. } = event {
                eprintln!("{text}");
            }
            return;
        }
        if matches!(event, RunEvent::AgentOutput { .. }) {
            // Agent output is best-effort because the durable per-task log has
            // the full transcript. Dropping here keeps output bursts from
            // filling the shared channel indefinitely. §1.2
            let _ = self.tx.try_send(Msg::Event(Box::new(event)));
        } else {
            // Lifecycle events define slot state. Preserve them even during
            // output floods so the UI cannot get stuck showing stale work.
            let _ = self.tx.send(Msg::Event(Box::new(event)));
        }
    }
}

fn render_loop(
    mut terminal: Terminal<CrosstermBackend<Stdout>>,
    rx: crossbeam_channel::Receiver<Msg>,
    mut state: UiState,
    screen_restored: &AtomicBool,
) {
    let tick = Duration::from_millis(250);
    let mut last_draw = Instant::now().checked_sub(tick).unwrap_or_else(Instant::now);

    loop {
        // Drain pending events until the channel is empty or the tick elapses.
        let deadline = Instant::now() + tick;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(Msg::Event(event)) => state.apply(&event),
                Ok(Msg::Shutdown) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    // The run has ended. A non-TTY run returns here; an
                    // interactive run stays navigable until the operator quits.
                    state.finished = true;
                    stay_until_quit(&mut terminal, &mut state, screen_restored);
                    break_out(terminal, screen_restored);
                    return;
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => break,
            }
        }

        if drain_input(&mut terminal, &mut state, screen_restored) {
            return;
        }

        if last_draw.elapsed() >= tick {
            state.refresh_plan();
            state.tick_spinner();
            draw(&mut terminal, &state);
            last_draw = Instant::now();
        }
    }
}

/// After the run finishes, keep redrawing and accepting navigation keys until
/// the operator presses `q`. The live actions are already disabled (§1.5.7).
fn stay_until_quit(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &mut UiState,
    screen_restored: &AtomicBool,
) {
    let tick = Duration::from_millis(250);
    state.refresh_plan();
    draw(terminal, state);
    loop {
        if ctevent::poll(tick).unwrap_or(false) {
            if let Ok(CtEvent::Key(key)) = ctevent::read() {
                if key.kind != KeyEventKind::Release {
                    match handle_key_event(state, key.code, key.modifiers) {
                        InputAction::Quit => return,
                        InputAction::ForwardSigint => {
                            break_out_ref(terminal, screen_restored);
                            let _ = forward_sigint_to_self();
                            std::process::exit(130);
                        }
                        InputAction::Continue => {}
                    }
                }
            }
        }
        state.refresh_plan();
        state.tick_spinner();
        draw(terminal, state);
    }
}

/// Read terminal input (non-blocking). Returns `true` when the loop should exit
/// because Ctrl+C was pressed (the terminal is already restored).
fn drain_input(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &mut UiState,
    screen_restored: &AtomicBool,
) -> bool {
    while ctevent::poll(Duration::from_millis(0)).unwrap_or(false) {
        match ctevent::read() {
            Ok(CtEvent::Key(key)) if key.kind != KeyEventKind::Release => {
                match handle_key_event(state, key.code, key.modifiers) {
                    InputAction::ForwardSigint => {
                        draw(terminal, state);
                        break_out_ref(terminal, screen_restored);
                        // The engine's own interruption handling takes over
                        // from here, and its notices now land on the terminal
                        // this call just restored. §FS-rhei-run-tui.1.8
                        let _ = forward_sigint_to_self();
                        return true;
                    }
                    InputAction::Quit | InputAction::Continue => {}
                }
            }
            Ok(CtEvent::Resize(_, _)) => draw(terminal, state),
            _ => {}
        }
    }
    false
}

fn draw(terminal: &mut Terminal<CrosstermBackend<Stdout>>, state: &UiState) {
    let _ = terminal.draw(|f| render::draw(f, state));
}

fn break_out(mut terminal: Terminal<CrosstermBackend<Stdout>>, screen_restored: &AtomicBool) {
    break_out_ref(&mut terminal, screen_restored);
}

/// Restore the terminal and say so, before returning to any caller that might
/// go on to emit. §FS-rhei-run-tui.1.8
fn break_out_ref(terminal: &mut Terminal<CrosstermBackend<Stdout>>, screen_restored: &AtomicBool) {
    let _ = terminal.show_cursor();
    let _ = disable_raw_mode();
    let _ = io::stdout().execute(LeaveAlternateScreen);
    screen_restored.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
fn forward_sigint_to_self() -> nix::Result<()> {
    raise(Signal::SIGINT)
}

#[cfg(not(unix))]
fn forward_sigint_to_self() -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "SIGINT forwarding is Unix-only"))
}

#[cfg(test)]
mod tests;
