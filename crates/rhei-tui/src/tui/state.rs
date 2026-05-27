use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use crate::event::{AgentStream, MessageLevel, RunEvent, Slot, TaskOutcome};

use super::text::{sanitize_terminal_text, stream_label, truncate_chars};
use super::{JOURNAL_BUFFER, JOURNAL_TRAFFIC_WIDTH, SLOT_TRAFFIC_BUFFER};

#[derive(Clone, Default)]
pub(super) struct SlotState {
    pub(super) task: Option<String>,
    pub(super) agent: Option<String>,
    pub(super) state: String,
    pub(super) started_at: Option<Instant>,
    pub(super) log_path: Option<PathBuf>,
    pub(super) last_event_display: Option<String>,
    pub(super) traffic: VecDeque<TrafficLine>,
}

#[derive(Clone)]
pub(super) struct TrafficLine {
    pub(super) stream: AgentStream,
    pub(super) text: String,
}

#[derive(Clone)]
pub(super) struct TaskRow {
    pub(super) task: String,
    pub(super) status: String,
    pub(super) slot: Option<Slot>,
    pub(super) log_path: Option<PathBuf>,
    pub(super) dashboard_url: Option<String>,
}

pub(super) struct UiState {
    pub(super) parallel: u16,
    pub(super) total_tasks: usize,
    pub(super) slots: Vec<SlotState>,
    pub(super) task_rows: Vec<TaskRow>,
    pub(super) journal: VecDeque<String>,
    pub(super) dashboard_url: Option<String>,
    pub(super) finished: bool,
}

impl UiState {
    pub(super) fn new(parallel: u16, total_tasks: usize) -> Self {
        let parallel = parallel.max(1);
        Self {
            parallel,
            total_tasks,
            slots: vec![SlotState::default(); parallel as usize],
            task_rows: Vec::new(),
            journal: VecDeque::with_capacity(JOURNAL_BUFFER),
            dashboard_url: None,
            finished: false,
        }
    }

    pub(super) fn push_journal(&mut self, line: String) {
        if self.journal.len() == JOURNAL_BUFFER {
            self.journal.pop_front();
        }
        self.journal.push_back(line);
    }

    pub(super) fn apply(&mut self, event: &RunEvent) {
        match event {
            RunEvent::RunStarted { parallel, total_tasks, .. } => {
                self.parallel = (*parallel).max(1);
                self.total_tasks = *total_tasks;
                self.slots = vec![SlotState::default(); self.parallel as usize];
                self.task_rows.clear();
                self.dashboard_url = None;
                self.push_journal(format!(
                    "run started — parallel={} total={}",
                    self.parallel, self.total_tasks
                ));
            }
            RunEvent::PassStarted { pass, ready } => {
                for task in ready {
                    let row = self.upsert_task_row(task);
                    if row.slot.is_none() && !is_terminal_status(&row.status) {
                        row.status = "pending".to_string();
                    }
                }
                self.push_journal(format!("pass {}: {} ready", pass, ready.len()));
            }
            RunEvent::SlotAssigned {
                slot, task, from, to, agent, log_path, started_at, ..
            } => {
                let same_state = from == to;
                let display =
                    if same_state { format!("in {to}") } else { format!("{from}→{to}") };
                if let Some(s) = self.slots.get_mut(*slot as usize) {
                    s.task = Some(task.clone());
                    s.agent = agent.clone();
                    s.state = to.clone();
                    s.started_at = Some(*started_at);
                    s.log_path = Some(log_path.clone());
                    s.last_event_display = Some(display.clone());
                }
                let row = self.upsert_task_row(task);
                row.status = "running".to_string();
                row.slot = Some(*slot);
                row.log_path = Some(log_path.clone());
                let line = if same_state {
                    format!("▶ slot {slot}: {task} started in {to}")
                } else {
                    format!("▶ slot {slot}: {task} {from}→{to}")
                };
                self.push_journal(line);
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
                let row = self.upsert_task_row(task);
                row.status = match outcome {
                    TaskOutcome::Completed => "succeeded".to_string(),
                    TaskOutcome::Failed(_) => "failed".to_string(),
                    TaskOutcome::Cancelled => "skipped".to_string(),
                    TaskOutcome::TimedOut => "timed out".to_string(),
                };
                row.slot = None;
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
            RunEvent::TasksDeferred { pass, tasks } => {
                for task in tasks {
                    let row = self.upsert_task_row(task);
                    if row.slot.is_none() && !is_terminal_status(&row.status) {
                        row.status = "deferred".to_string();
                    }
                }
                self.push_journal(format!(
                    "pass {} deferred {} task(s): {}",
                    pass,
                    tasks.len(),
                    tasks.join(", ")
                ));
            }
            RunEvent::RunFinished { summary } => {
                self.finished = true;
                let mut line = format!(
                    "run finished — agents={} programs={} terminal={}/{}",
                    summary.agents_spawned,
                    summary.programs_spawned,
                    summary.terminal_tasks,
                    summary.total_tasks
                );
                if let Some(accounting) = summary.accounting.as_ref() {
                    if let Some(cost) = accounting.cost_micro.or(accounting.priced_cost_micro) {
                        line.push_str(&format!(" cost={}", format_cost_micro(cost)));
                    }
                }
                self.push_journal(line);
            }
            RunEvent::Message { level, text } => {
                let prefix = match level {
                    MessageLevel::Info => "·",
                    MessageLevel::Warn => "!",
                    MessageLevel::Error => "✗",
                };
                self.push_journal(format!("{prefix} {text}"));
            }
            RunEvent::RunLink { label, url } => {
                if label == "Dashboard" {
                    // §FS-rhei-run-tui.1.6: keep the live dashboard URL visible in the TUI header.
                    self.dashboard_url = Some(url.clone());
                } else if let Some(task) = label.strip_suffix(" dashboard") {
                    let row = self.upsert_task_row(task);
                    // §FS-rhei-batch-run.3.2: nested run dashboard URLs are attached to
                    // their plan row so batch-run remains terminal-first.
                    row.dashboard_url = Some(url.clone());
                }
                self.push_journal(format!("{label}: {url}"));
            }
            RunEvent::UsageReported { task, usage, .. } => {
                let cost = usage
                    .cost_micro
                    .or(usage.priced_cost_micro)
                    .map(format_cost_micro)
                    .unwrap_or_else(|| "unpriced".to_string());
                self.push_journal(format!(
                    "usage task {task}: {} {cost} {:?}",
                    usage.agent, usage.coverage
                ));
            }
        }
    }

    fn upsert_task_row(&mut self, task: &str) -> &mut TaskRow {
        if let Some(index) = self.task_rows.iter().position(|row| row.task == task) {
            return &mut self.task_rows[index];
        }
        self.task_rows.push(TaskRow {
            task: task.to_string(),
            status: "pending".to_string(),
            slot: None,
            log_path: None,
            dashboard_url: None,
        });
        self.task_rows.last_mut().expect("task row was just pushed")
    }
}

fn format_cost_micro(value: u64) -> String {
    let units = value / 1_000_000;
    let cents = (value % 1_000_000) / 10_000;
    format!("${units}.{cents:02}")
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "succeeded" | "failed" | "skipped" | "timed out")
}

pub(super) struct UiStateSnapshot {
    pub(super) parallel: u16,
    pub(super) total_tasks: usize,
    pub(super) slots: Vec<SlotState>,
    pub(super) task_rows: Vec<TaskRow>,
    pub(super) journal: Vec<String>,
    pub(super) dashboard_url: Option<String>,
    pub(super) finished: bool,
}

impl UiState {
    pub(super) fn clone_snapshot(&self) -> UiStateSnapshot {
        UiStateSnapshot {
            parallel: self.parallel,
            total_tasks: self.total_tasks,
            slots: self.slots.clone(),
            task_rows: self.task_rows.clone(),
            journal: self.journal.iter().cloned().collect(),
            dashboard_url: self.dashboard_url.clone(),
            finished: self.finished,
        }
    }
}
