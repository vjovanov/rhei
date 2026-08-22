use std::io::IsTerminal;
use std::path::Path;
use std::sync::Arc;

use crate::rhei_tui::event::{EventSink, NullSink, Tee};
use crate::rhei_tui::event_log::EventLogSink;
use crate::rhei_tui::journal::JournalSink;
use crate::rhei_tui::json::JsonSink;
use crate::rhei_tui::stdout::StdoutSink;
use crate::rhei_tui::tui::{TuiContext, TuiSink};

/// Caller-selected frontend override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendKind {
    /// Force TUI mode.
    Tui,
    /// Force plain stdout mode.
    Stdout,
    /// Force the JSONL event stream on stdout. Decided before TTY detection:
    /// a stream a program parses is never also a screen. §FS-rhei-run-json.1
    Json {
        /// Inline `agent_output` records instead of leaving the traffic to the
        /// per-task logs. §FS-rhei-run-json.2.3
        agent_output: bool,
    },
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

/// Choose a frontend and compose it with the always-on sinks into a single
/// `EventSink`. The transition journal and the durable event log are written in
/// every mode; the frontend is a `JsonSink`, a `TuiSink` (interactive), or a
/// `StdoutSink`.
///
/// `parallel` and `total_tasks` are passed to the TUI for its initial layout.
/// When TUI construction fails (e.g., the backend cannot enter raw mode),
/// this falls back to `StdoutSink` and logs a warning to stderr.
// §FS-rhei-run-tui.1.7 §FS-rhei-run-json.3
pub fn select_frontend(
    workspace_root: &Path,
    kind: FrontendKind,
    parallel: u16,
    total_tasks: usize,
    tui_context: TuiContext,
) -> Frontend {
    let want_tui = match kind {
        FrontendKind::Tui => true,
        FrontendKind::Stdout | FrontendKind::Json { .. } => false,
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

    // Written in every mode so a run is followable whichever surface drives
    // it, and before the frontend so the head of the stream is in the file.
    // §FS-rhei-run-json.3
    let event_log: Arc<dyn EventSink> = match EventLogSink::create(workspace_root) {
        Ok(log) => Arc::new(log),
        Err(err) => {
            eprintln!(
                "warning: could not open the run event log at {}: {err}\n\
                 `rhei attach` will not be able to follow this run.",
                crate::rhei_tui::event_log::event_log_path(workspace_root).display()
            );
            Arc::new(NullSink)
        }
    };

    if let FrontendKind::Json { agent_output } = kind {
        let json: Arc<dyn EventSink> = Arc::new(JsonSink::new(agent_output, workspace_root));
        let sink = Arc::new(Tee::new(vec![journal, event_log, json]));
        return Frontend { sink, is_tui: false, _tui: None };
    }

    if want_tui {
        match TuiSink::start(parallel.max(1), total_tasks, tui_context) {
            Ok(tui) => {
                let tui = Arc::new(tui);
                let frontend: Arc<dyn EventSink> = tui.clone();
                let sink = Arc::new(Tee::new(vec![journal, event_log, frontend]));
                return Frontend { sink, is_tui: true, _tui: Some(tui) };
            }
            Err(err) => {
                eprintln!("warning: could not start TUI ({}); falling back to stdout", err);
            }
        }
    }

    let stdout: Arc<dyn EventSink> = Arc::new(StdoutSink::new());
    let sink = Arc::new(Tee::new(vec![journal, event_log, stdout]));
    Frontend { sink, is_tui: false, _tui: None }
}
