use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

/// Slot index assigned to a running task invocation.
///
/// The engine allocates one slot per concurrent agent/program and releases the
/// slot when the invocation exits. The renderer uses the slot to update the
/// correct tile without reconciling task ids on every frame. The type is
/// wider than a byte so callers with very large `--parallel` values cannot
/// silently collide into slot 255.
pub type Slot = u16;

/// Outcome of a released slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    Completed,
    Failed(String),
    Cancelled,
    TimedOut,
}

/// Aggregate statistics emitted with `RunFinished`.
#[derive(Debug, Clone, Default)]
pub struct RunSummary {
    pub agents_spawned: u32,
    pub programs_spawned: u32,
    pub terminal_tasks: usize,
    pub total_tasks: usize,
    pub accounting: Option<AccountingRunSummary>,
}

/// Severity of an engine log message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLevel {
    Info,
    Warn,
    Error,
}

/// Agent subprocess stream that produced a live output line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DimensionStatus {
    Measured,
    Partial,
    Unsupported,
    Omitted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DimensionSummary {
    pub value: Option<u64>,
    pub status: DimensionStatus,
    pub missing_count: u64,
    pub measured_count: u64,
}

impl Default for DimensionSummary {
    fn default() -> Self {
        Self { value: None, status: DimensionStatus::Unknown, missing_count: 0, measured_count: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageCoverage {
    Complete,
    Partial,
    Unpriced,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageStatus {
    Measured,
    UnsupportedAgent,
    ExtractorUnavailable,
    ExtractorFailed,
    NoUsageEmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PricingStatus {
    Priced,
    PartialPrice,
    Unpriced,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsageSummary {
    pub invocation_id: String,
    pub state: String,
    pub agent: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub total: DimensionSummary,
    pub input_total: DimensionSummary,
    pub input_cached_read: DimensionSummary,
    pub input_cache_write: DimensionSummary,
    pub output_total: DimensionSummary,
    pub output_cached_read: DimensionSummary,
    pub output_cache_write: DimensionSummary,
    pub cost_micro: Option<u64>,
    pub priced_cost_micro: Option<u64>,
    pub currency: Option<String>,
    pub coverage: UsageCoverage,
    pub status: UsageStatus,
    pub pricing_status: PricingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AccountingRunSummary {
    pub total: DimensionSummary,
    pub input_total: DimensionSummary,
    pub input_cached_read: DimensionSummary,
    pub input_cache_write: DimensionSummary,
    pub output_total: DimensionSummary,
    pub output_cached_read: DimensionSummary,
    pub output_cache_write: DimensionSummary,
    pub cost_micro: Option<u64>,
    pub priced_cost_micro: Option<u64>,
    pub currency: Option<String>,
    pub coverage: UsageCoverage,
    pub pricing_status: PricingStatus,
    pub invocation_count: u64,
    pub measured_invocation_count: u64,
    pub missing_invocation_count: u64,
}

/// Events emitted by the execution engine.
///
/// The shape follows the TUI event surface. `Message` is an additional variant
/// used while the stdout path still emits humanized strings; a TUI frontend can
/// surface these in its journal pane.
// §FS-rhei-run-tui.1.1: Run event surface.
#[derive(Debug, Clone)]
pub enum RunEvent {
    RunStarted {
        workspace: PathBuf,
        parallel: u16,
        total_tasks: usize,
    },
    PassStarted {
        pass: u32,
        ready: Vec<String>,
    },
    /// A worker has been assigned to a task.
    ///
    /// `from` is the task's persisted state at the moment of claim; `to` is
    /// the state the worker is operating in. When `from == to`, the worker
    /// is running an *autonomous* state that the engine did not transition
    /// into as part of the claim — it is "starting work in `to`," not
    /// "moving from `from` to `to`." Renderers must distinguish the two
    /// cases so the UI does not show a phantom `state→state` self-loop.
    SlotAssigned {
        slot: Slot,
        task: String,
        from: String,
        to: String,
        agent: Option<String>,
        template_context: Option<rhei_viz_model::TemplateContext>,
        log_path: PathBuf,
        started_at: Instant,
        wall_clock: SystemTime,
    },
    /// A worker slot has been released.
    ///
    /// `from` is the state at assignment; `to` is the state the task ended
    /// up in. When `from == to`, the worker exited without changing state
    /// (typical for autonomous states that hand control back to the run loop
    /// for re-evaluation) — render as "ended in `to`," not as a transition.
    SlotReleased {
        slot: Slot,
        task: String,
        from: String,
        to: String,
        log_path: PathBuf,
        outcome: TaskOutcome,
        finished_at: Instant,
        wall_clock: SystemTime,
        exit_code: Option<i32>,
        duration_ms: u64,
    },
    PassEnded {
        pass: u32,
        progressed: bool,
    },
    /// Tasks that were eligible this pass but yielded their slot to a same-state
    /// claimant (non-`concurrent` state). They are reconsidered next pass.
    TasksDeferred {
        pass: u32,
        tasks: Vec<String>,
    },
    RunFinished {
        summary: RunSummary,
    },
    Message {
        level: MessageLevel,
        text: String,
    },
    RunLink {
        label: String,
        url: String,
    },
    AgentOutput {
        slot: Slot,
        task: String,
        stream: AgentStream,
        line: String,
        wall_clock: SystemTime,
    },
    /// Accounting event emitted after the durable invocation record is written.
    /// §FS-rhei-cost-accounting.7
    UsageReported {
        slot: Option<Slot>,
        task: String,
        invocation_id: String,
        usage: UsageSummary,
    },
}

/// Sink that consumes `RunEvent`s. Implementations must be cheap to clone and
/// safe to share across threads (the engine spawns parallel workers).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: RunEvent);
}

/// Composite sink that forwards every event to each inner sink in order.
#[derive(Clone)]
pub struct Tee {
    inners: Arc<Vec<Arc<dyn EventSink>>>,
}

impl Tee {
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self { inners: Arc::new(sinks) }
    }
}

impl EventSink for Tee {
    fn emit(&self, event: RunEvent) {
        for sink in self.inners.iter() {
            sink.emit(event.clone());
        }
    }
}

/// Sink that discards every event. Useful as the default frontend when the
/// engine is responsible for producing stdout (backward-compatible mode).
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: RunEvent) {}
}
