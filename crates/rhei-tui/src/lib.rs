//! Live visualization and transition journal for `rhei run`.
//!
//! See `docs/functional-spec/rhei-run-tui.spec.md` for the normative specification.

mod dashboard;
mod event;
mod frontend;
mod journal;
mod stdout;
mod tui;

pub use dashboard::{DashboardSink, DashboardTask, PlanLoader, PlanSnapshot};
pub use event::{
    AgentStream, EventSink, MessageLevel, NullSink, RunEvent, RunSummary, Slot, TaskOutcome, Tee,
};
pub use frontend::{select_frontend, Frontend, FrontendKind};
pub use journal::JournalSink;
pub use stdout::StdoutSink;
pub use tui::TuiSink;
