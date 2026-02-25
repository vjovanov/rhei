use std::io::IsTerminal;
use std::path::Path;
use std::sync::Arc;

use crate::event::{EventSink, NullSink, Tee};
use crate::journal::JournalSink;
use crate::stdout::StdoutSink;
use crate::tui::TuiSink;

/// Caller-selected frontend override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendKind {
    /// Force TUI mode.
    Tui,
    /// Force plain stdout mode.
    Stdout,
    /// Auto-detect from `stdout.is_terminal()`.
    Auto,
}

/// Result of selecting a frontend: an event sink and a flag describing which
/// frontend was picked (so the engine can suppress stdout when a TUI is in
/// charge of the terminal).
pub struct Frontend {
    pub sink: Arc<dyn EventSink>,
    /// True when a `TuiSink` is the active frontend. The engine uses this to
    /// decide whether its own `println!` output should be suppressed.
    pub is_tui: bool,
    _tui: Option<Arc<TuiSink>>,
}

/// Choose a frontend and compose it with a `JournalSink` into a single
/// `EventSink`. The journal is always written; the frontend is either a
/// `TuiSink` (interactive) or `StdoutSink` (backward-compatible).
///
/// `parallel` and `total_tasks` are passed to the TUI for its initial layout.
/// When TUI construction fails (e.g., the backend cannot enter raw mode),
/// this falls back to `StdoutSink` and logs a warning to stderr.
pub fn select_frontend(
    workspace_root: &Path,
    kind: FrontendKind,
    parallel: u16,
    total_tasks: usize,
) -> Frontend {
    let want_tui = match kind {
        FrontendKind::Tui => true,
        FrontendKind::Stdout => false,
        FrontendKind::Auto => std::io::stdout().is_terminal(),
    };

    let journal: Arc<dyn EventSink> = match JournalSink::open(workspace_root) {
        Ok(j) => Arc::new(j),
        Err(err) => {
            eprintln!(
                "warning: could not open transition journal at {}/runtime/transitions.log: {}",
                workspace_root.display(),
                err
            );
            Arc::new(NullSink)
        }
    };

    if want_tui {
        match TuiSink::start(parallel.max(1), total_tasks) {
            Ok(tui) => {
                let tui = Arc::new(tui);
                let frontend: Arc<dyn EventSink> = tui.clone();
                let sink = Arc::new(Tee::new(vec![journal, frontend]));
                return Frontend { sink, is_tui: true, _tui: Some(tui) };
            }
            Err(err) => {
                eprintln!("warning: could not start TUI ({}); falling back to stdout", err);
            }
        }
    }

    let stdout: Arc<dyn EventSink> = Arc::new(StdoutSink::new());
    let sink = Arc::new(Tee::new(vec![journal, stdout]));
    Frontend { sink, is_tui: false, _tui: None }
}
