use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::event::{AgentStream, MessageLevel, RunEvent, RunSummary, Slot, TaskOutcome};

use super::{now_ms, system_time_ms, DashboardTask, RECENT_LIMIT, SLOT_TRAFFIC_LIMIT};

#[derive(Clone, Serialize)]
pub(super) struct DashboardState {
    pub(super) workspace: String,
    pub(super) parallel: u16,
    pub(super) total_tasks: usize,
    pub(super) pass: u32,
    pub(super) ready: Vec<String>,
    /// Task ids deferred during the *current* pass. Cleared on `PassStarted`.
    pub(super) deferred: Vec<String>,
    pub(super) slots: Vec<DashboardSlot>,
    pub(super) recent: Vec<JournalLine>,
    pub(super) links: Vec<DashboardLink>,
    pub(super) finished: bool,
    pub(super) summary: Option<DashboardSummary>,
    pub(super) started_at_ms: u128,
    pub(super) updated_at_ms: u128,
}

#[derive(Clone, Serialize)]
pub(super) struct JournalLine {
    pub(super) level: &'static str,
    pub(super) text: String,
    pub(super) ts_ms: u128,
}

impl DashboardState {
    pub(super) fn new(workspace: PathBuf, parallel: u16, total_tasks: usize) -> Self {
        let now = now_ms();
        Self {
            workspace: workspace.display().to_string(),
            parallel,
            total_tasks,
            pass: 0,
            ready: Vec::new(),
            deferred: Vec::new(),
            slots: vec![DashboardSlot::default(); parallel as usize],
            recent: Vec::new(),
            links: Vec::new(),
            finished: false,
            summary: None,
            started_at_ms: now,
            updated_at_ms: now,
        }
    }

    pub(super) fn apply(&mut self, event: &RunEvent) {
        let now = now_ms();
        self.updated_at_ms = now;
        match event {
            RunEvent::RunStarted { workspace, parallel, total_tasks } => {
                self.workspace = workspace.display().to_string();
                self.parallel = (*parallel).max(1);
                self.total_tasks = *total_tasks;
                self.slots = vec![DashboardSlot::default(); self.parallel as usize];
                self.started_at_ms = now;
                self.push_recent(
                    "info",
                    format!("run started: parallel={} total={}", self.parallel, self.total_tasks),
                );
            }
            RunEvent::PassStarted { pass, ready } => {
                self.pass = *pass;
                self.ready = ready.clone();
                self.deferred.clear();
                self.push_recent("info", format!("pass {pass}: {} ready", ready.len()));
            }
            RunEvent::SlotAssigned {
                slot, task, from, to, agent, log_path, wall_clock, ..
            } => {
                let slot_state = self.slot_mut(*slot);
                slot_state.active = true;
                slot_state.task = Some(task.clone());
                slot_state.agent = agent.clone();
                slot_state.state = Some(to.clone());
                // Only record a transition when the worker actually moved
                // states. `from == to` means the engine started a worker in
                // an autonomous state — there was no transition.
                slot_state.transition =
                    if from == to { None } else { Some(format!("{from}->{to}")) };
                slot_state.log_path = Some(log_path.display().to_string());
                slot_state.started_at_ms = Some(system_time_ms(*wall_clock));
                slot_state.finished_at_ms = None;
                slot_state.duration_ms = None;
                slot_state.exit_code = None;
                slot_state.outcome = None;
                slot_state.traffic.clear();
                if from == to {
                    self.push_recent("info", format!("slot {slot}: task {task} started in {to}"));
                } else {
                    self.push_recent("info", format!("slot {slot}: task {task} {from}->{to}"));
                }
            }
            RunEvent::AgentOutput { slot, stream, line, wall_clock, .. } => {
                let slot_state = self.slot_mut(*slot);
                let stream_name = match stream {
                    AgentStream::Stdout => "stdout",
                    AgentStream::Stderr => "stderr",
                };
                let ts = system_time_ms(*wall_clock);
                // Dedup consecutive identical lines: bump a counter on the
                // last entry instead of pushing a duplicate.
                if let Some(last) = slot_state.traffic.last_mut() {
                    if last.stream == stream_name && last.text == *line {
                        last.repeat += 1;
                        last.ts_ms = ts;
                        return;
                    }
                }
                if slot_state.traffic.len() == SLOT_TRAFFIC_LIMIT {
                    slot_state.traffic.remove(0);
                }
                slot_state.traffic.push(DashboardTraffic {
                    stream: stream_name,
                    text: line.clone(),
                    ts_ms: ts,
                    repeat: 1,
                });
            }
            RunEvent::SlotReleased {
                slot,
                task,
                from,
                to,
                outcome,
                wall_clock,
                duration_ms,
                exit_code,
                ..
            } => {
                let slot_state = self.slot_mut(*slot);
                slot_state.active = false;
                slot_state.finished_at_ms = Some(system_time_ms(*wall_clock));
                slot_state.duration_ms = Some(*duration_ms);
                slot_state.exit_code = *exit_code;
                slot_state.outcome = Some(match outcome {
                    TaskOutcome::Completed => "completed".to_string(),
                    TaskOutcome::Failed(reason) => format!("failed: {reason}"),
                    TaskOutcome::Cancelled => "cancelled".to_string(),
                    TaskOutcome::TimedOut => "timed out".to_string(),
                });
                if from != to {
                    slot_state.transition = Some(format!("{from}->{to}"));
                }
                let outcome_label = slot_state.outcome.as_deref().unwrap_or("unknown").to_string();
                let where_label =
                    if from == to { format!("in {to}") } else { format!("{from}->{to}") };
                self.push_recent(
                    "info",
                    format!("slot {slot}: task {task} finished {where_label} ({outcome_label})"),
                );
            }
            RunEvent::PassEnded { pass, progressed } => {
                self.push_recent("info", format!("pass {pass} ended: progressed={progressed}"));
            }
            RunEvent::TasksDeferred { pass, tasks } => {
                let mut seen: HashSet<String> = self.deferred.iter().cloned().collect();
                for t in tasks {
                    if seen.insert(t.clone()) {
                        self.deferred.push(t.clone());
                    }
                }
                self.push_recent(
                    "info",
                    format!("pass {pass} deferred {} task(s): {}", tasks.len(), tasks.join(", ")),
                );
            }
            RunEvent::RunFinished { summary } => {
                self.finished = true;
                self.summary = Some(DashboardSummary::from(summary));
                self.push_recent(
                    "info",
                    format!(
                        "run finished: terminal={}/{}",
                        summary.terminal_tasks, summary.total_tasks
                    ),
                );
            }
            RunEvent::Message { level, text } => {
                let level = match level {
                    MessageLevel::Info => "info",
                    MessageLevel::Warn => "warn",
                    MessageLevel::Error => "error",
                };
                self.push_recent(level, text.clone());
            }
            RunEvent::RunLink { label, url } => {
                if !self.links.iter().any(|link| link.url == *url) {
                    self.links.push(DashboardLink {
                        label: label.clone(),
                        url: url.clone(),
                        source: "callback",
                    });
                }
                self.push_recent("info", format!("{label}: {url}"));
            }
        }
    }

    fn slot_mut(&mut self, slot: Slot) -> &mut DashboardSlot {
        let idx = slot as usize;
        if idx >= self.slots.len() {
            self.slots.resize_with(idx + 1, DashboardSlot::default);
        }
        &mut self.slots[idx]
    }

    fn push_recent(&mut self, level: &'static str, text: String) {
        if self.recent.len() == RECENT_LIMIT {
            self.recent.remove(0);
        }
        self.recent.push(JournalLine { level, text, ts_ms: now_ms() });
    }
}

#[derive(Clone, Default, Serialize)]
pub(super) struct DashboardSlot {
    pub(super) active: bool,
    pub(super) task: Option<String>,
    pub(super) agent: Option<String>,
    pub(super) state: Option<String>,
    pub(super) transition: Option<String>,
    pub(super) log_path: Option<String>,
    pub(super) started_at_ms: Option<u128>,
    pub(super) finished_at_ms: Option<u128>,
    pub(super) duration_ms: Option<u64>,
    pub(super) exit_code: Option<i32>,
    pub(super) outcome: Option<String>,
    pub(super) traffic: Vec<DashboardTraffic>,
}

#[derive(Clone, Serialize)]
pub(super) struct DashboardTraffic {
    pub(super) stream: &'static str,
    pub(super) text: String,
    pub(super) ts_ms: u128,
    pub(super) repeat: u32,
}

#[derive(Clone, Serialize)]
pub(super) struct DashboardLink {
    pub(super) label: String,
    pub(super) url: String,
    /// `"callback"` for links emitted by the run process; `"workspace"` for
    /// the fixed entries the dashboard injects (workspace dir, runtime/logs,
    /// runtime/results). The frontend renders this string as-is in the
    /// source-chip column.
    pub(super) source: &'static str,
}

#[derive(Clone, Serialize)]
pub(super) struct DashboardSummary {
    pub(super) agents_spawned: u32,
    pub(super) programs_spawned: u32,
    pub(super) terminal_tasks: usize,
    pub(super) total_tasks: usize,
}

impl From<&RunSummary> for DashboardSummary {
    fn from(summary: &RunSummary) -> Self {
        Self {
            agents_spawned: summary.agents_spawned,
            programs_spawned: summary.programs_spawned,
            terminal_tasks: summary.terminal_tasks,
            total_tasks: summary.total_tasks,
        }
    }
}

/// Composite snapshot served at `/snapshot`. Built per request from the
/// event-driven `DashboardState` plus the lazily-loaded plan view.
#[derive(Serialize)]
pub(super) struct SnapshotPayload<'a> {
    #[serde(flatten)]
    pub(super) state: &'a DashboardState,
    pub(super) plan_title: Option<String>,
    /// Derived from top-level task state for the dashboard visualization tabs.
    /// §FS-rhei-viz.3
    pub(super) plan_state: Option<String>,
    pub(super) tasks: Vec<TaskRow>,
    pub(super) auto_links: Vec<DashboardLink>,
}

#[derive(Serialize)]
pub(super) struct TaskRow {
    #[serde(flatten)]
    pub(super) task: DashboardTask,
    /// `Some(slot_index)` if a worker is currently running this task.
    pub(super) in_slot: Option<u16>,
    /// `true` if this task was ready this pass but was held back by
    /// non-`concurrent` scheduling. Cleared at `PassStarted`.
    pub(super) deferred_this_pass: bool,
}

pub(super) fn derive_plan_state(tasks: &[DashboardTask]) -> String {
    let root_states: Vec<&str> = tasks
        .iter()
        .filter(|task| task.parent.is_none() || task.depth == 1)
        .map(|task| task.state.as_str())
        .collect();

    if root_states.is_empty() {
        return "pending".to_string();
    }
    if root_states.iter().all(|state| *state == "draft") {
        return "draft".to_string();
    }
    if root_states.iter().all(|state| *state == "completed") {
        return "completed".to_string();
    }
    if root_states.iter().all(|state| is_dashboard_terminal(state)) {
        return "archived".to_string();
    }
    if root_states.iter().any(|state| is_dashboard_active_like(state)) {
        return "active".to_string();
    }
    "pending".to_string()
}

fn is_dashboard_terminal(state: &str) -> bool {
    matches!(state, "completed" | "cancelled" | "archived" | "failed")
}

fn is_dashboard_active_like(state: &str) -> bool {
    matches!(
        state,
        "in_progress"
            | "in-progress"
            | "needs-review"
            | "review"
            | "prove"
            | "consolidate"
            | "agent-review"
            | "agent-review-fix"
    )
}
