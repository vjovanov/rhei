//! Live visualization and transition journal for `rhei run`.
//!
//! §FS-rhei-run-tui: Live visualization and transition journal behavior.

mod dashboard;
mod event;
mod frontend;
mod journal;
mod stdout;
mod tui;

pub use dashboard::{DashboardSink, DashboardTask, PlanLoader, PlanSnapshot};
pub use event::{
    AccountingRunSummary, AgentStream, DimensionStatus, DimensionSummary, EventSink, MessageLevel,
    NullSink, PricingStatus, RunEvent, RunSummary, Slot, TaskOutcome, Tee, UsageCoverage,
    UsageStatus, UsageSummary,
};
pub use frontend::{select_frontend, Frontend, FrontendKind};
pub use journal::JournalSink;
pub use stdout::StdoutSink;
pub use tui::TuiSink;
