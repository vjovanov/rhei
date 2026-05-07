use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::event::{AgentStream, EventSink, MessageLevel, RunEvent, RunSummary, Slot, TaskOutcome};

const RECENT_LIMIT: usize = 200;
const SLOT_TRAFFIC_LIMIT: usize = 60;

/// One task entry surfaced in the Tasks tab. Constructed by the caller's
/// plan loader closure on demand so the dashboard never has to depend on
/// `rhei-core` directly.
#[derive(Clone, Debug, Serialize)]
pub struct DashboardTask {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub parent: Option<String>,
    pub depth: u8,
    pub state: String,
    pub assignee: Option<String>,
    pub prior: Vec<String>,
    pub result_link: Option<String>,
}

/// Snapshot returned by the plan loader: plan title plus the flattened task
/// list. Loader can return `None` if the plan cannot be re-read (file moved,
/// transient parse error during a write); the dashboard keeps the previous
/// snapshot in that case.
#[derive(Clone, Debug)]
pub struct PlanSnapshot {
    pub title: String,
    pub tasks: Vec<DashboardTask>,
}

/// Closure invoked on every `/snapshot` request to refresh the Tasks tab.
///
/// Wrapped in `Arc` rather than `Box` so the snapshot handler can keep a
/// reference without holding a mutex during the call (which would serialise
/// concurrent dashboard tabs behind a slow plan parse).
pub type PlanLoader = Arc<dyn Fn() -> Option<PlanSnapshot> + Send + Sync>;

pub struct DashboardSink {
    url: String,
    state: Arc<Mutex<DashboardState>>,
    stop: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl DashboardSink {
    /// Backwards-compatible constructor: no plan loader, Tasks tab will show
    /// "(no plan loaded)".
    pub fn start(workspace: PathBuf, parallel: u16, total_tasks: usize) -> io::Result<Self> {
        Self::start_with_plan(workspace, parallel, total_tasks, None)
    }

    /// Constructor with an optional plan loader. The loader is called on every
    /// `/snapshot` request and must be cheap (it parses a markdown file). Pass
    /// `None` to opt out of the Tasks tab.
    pub fn start_with_plan(
        workspace: PathBuf,
        parallel: u16,
        total_tasks: usize,
        plan_loader: Option<PlanLoader>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let state =
            Arc::new(Mutex::new(DashboardState::new(workspace, parallel.max(1), total_tasks)));
        let stop = Arc::new(AtomicBool::new(false));
        // The loader is set once at construction and never mutated, so no
        // mutex is required around it. `last_plan` does need a mutex because
        // it caches a value that's overwritten each request.
        let plan = plan_loader;
        let last_plan = Arc::new(Mutex::new(None));

        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let handle =
            thread::spawn(move || serve(listener, thread_state, plan, last_plan, thread_stop));

        Ok(Self { url, state, stop, join: Mutex::new(Some(handle)) })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn finish(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let mut guard = match self.join.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(handle) = guard.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for DashboardSink {
    fn drop(&mut self) {
        self.finish();
    }
}

impl EventSink for DashboardSink {
    fn emit(&self, event: RunEvent) {
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        state.apply(&event);
    }
}

#[derive(Clone, Serialize)]
struct DashboardState {
    workspace: String,
    parallel: u16,
    total_tasks: usize,
    pass: u32,
    ready: Vec<String>,
    /// Task ids deferred during the *current* pass. Cleared on `PassStarted`.
    deferred: Vec<String>,
    slots: Vec<DashboardSlot>,
    recent: Vec<JournalLine>,
    links: Vec<DashboardLink>,
    finished: bool,
    summary: Option<DashboardSummary>,
    started_at_ms: u128,
    updated_at_ms: u128,
}

#[derive(Clone, Serialize)]
struct JournalLine {
    level: &'static str,
    text: String,
    ts_ms: u128,
}

impl DashboardState {
    fn new(workspace: PathBuf, parallel: u16, total_tasks: usize) -> Self {
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

    fn apply(&mut self, event: &RunEvent) {
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
                slot_state.transition = if from == to { None } else { Some(format!("{from}->{to}")) };
                slot_state.log_path = Some(log_path.display().to_string());
                slot_state.started_at_ms = Some(system_time_ms(*wall_clock));
                slot_state.finished_at_ms = None;
                slot_state.duration_ms = None;
                slot_state.exit_code = None;
                slot_state.outcome = None;
                slot_state.traffic.clear();
                if from == to {
                    self.push_recent(
                        "info",
                        format!("slot {slot}: task {task} started in {to}"),
                    );
                } else {
                    self.push_recent(
                        "info",
                        format!("slot {slot}: task {task} {from}->{to}"),
                    );
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
                slot, task, from, to, outcome, wall_clock, duration_ms, exit_code, ..
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
                    format!(
                        "pass {pass} deferred {} task(s): {}",
                        tasks.len(),
                        tasks.join(", ")
                    ),
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
struct DashboardSlot {
    active: bool,
    task: Option<String>,
    agent: Option<String>,
    state: Option<String>,
    transition: Option<String>,
    log_path: Option<String>,
    started_at_ms: Option<u128>,
    finished_at_ms: Option<u128>,
    duration_ms: Option<u64>,
    exit_code: Option<i32>,
    outcome: Option<String>,
    traffic: Vec<DashboardTraffic>,
}

#[derive(Clone, Serialize)]
struct DashboardTraffic {
    stream: &'static str,
    text: String,
    ts_ms: u128,
    repeat: u32,
}

#[derive(Clone, Serialize)]
struct DashboardLink {
    label: String,
    url: String,
    /// `"callback"` for links emitted by the run process; `"workspace"` for
    /// the fixed entries the dashboard injects (workspace dir, runtime/logs,
    /// runtime/results). The frontend renders this string as-is in the
    /// source-chip column.
    source: &'static str,
}

#[derive(Clone, Serialize)]
struct DashboardSummary {
    agents_spawned: u32,
    programs_spawned: u32,
    terminal_tasks: usize,
    total_tasks: usize,
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
struct SnapshotPayload<'a> {
    #[serde(flatten)]
    state: &'a DashboardState,
    plan_title: Option<String>,
    tasks: Vec<TaskRow>,
    auto_links: Vec<DashboardLink>,
}

#[derive(Serialize)]
struct TaskRow {
    #[serde(flatten)]
    task: DashboardTask,
    /// `Some(slot_index)` if a worker is currently running this task.
    in_slot: Option<u16>,
    /// `true` if this task was ready this pass but was held back by
    /// non-`concurrent` scheduling. Cleared at `PassStarted`.
    deferred_this_pass: bool,
}

fn serve(
    listener: TcpListener,
    state: Arc<Mutex<DashboardState>>,
    plan: Option<PlanLoader>,
    last_plan: Arc<Mutex<Option<PlanSnapshot>>>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => handle_client(stream, &state, plan.as_ref(), &last_plan),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
}

fn handle_client(
    mut stream: TcpStream,
    state: &Arc<Mutex<DashboardState>>,
    plan: Option<&PlanLoader>,
    last_plan: &Arc<Mutex<Option<PlanSnapshot>>>,
) {
    let mut buf = [0u8; 2048];
    let Ok(n) = stream.read(&mut buf) else {
        return;
    };
    let request = String::from_utf8_lossy(&buf[..n]);
    let path =
        request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/");

    match path {
        "/" => write_response(&mut stream, "text/html; charset=utf-8", DASHBOARD_HTML.as_bytes()),
        "/snapshot" => {
            let snapshot_state = match state.lock() {
                Ok(s) => s.clone(),
                Err(p) => p.into_inner().clone(),
            };
            // Refresh the plan view. If the loader is missing or returns
            // None (e.g. transient parse error during a write), reuse the
            // last good snapshot rather than blanking the Tasks tab.
            let fresh = plan.and_then(|loader| loader());
            let plan_snapshot = if let Some(s) = fresh {
                if let Ok(mut last) = last_plan.lock() {
                    *last = Some(s.clone());
                }
                Some(s)
            } else {
                last_plan.lock().ok().and_then(|g| g.clone())
            };

            let auto_links = derive_auto_links(&snapshot_state.workspace);
            let (plan_title, tasks) = match plan_snapshot {
                Some(p) => {
                    let active_tasks: HashSet<&str> = snapshot_state
                        .slots
                        .iter()
                        .filter_map(|s| s.task.as_deref())
                        .collect();
                    let deferred_set: HashSet<&str> =
                        snapshot_state.deferred.iter().map(|s| s.as_str()).collect();
                    let rows = p
                        .tasks
                        .into_iter()
                        .map(|task| {
                            let in_slot = snapshot_state.slots.iter().enumerate().find_map(
                                |(i, slot)| {
                                    slot.task
                                        .as_deref()
                                        .filter(|t| *t == task.id && active_tasks.contains(t))
                                        .map(|_| i as u16)
                                },
                            );
                            let deferred_this_pass = deferred_set.contains(task.id.as_str());
                            TaskRow { task, in_slot, deferred_this_pass }
                        })
                        .collect();
                    (Some(p.title), rows)
                }
                None => (None, Vec::new()),
            };

            let payload =
                SnapshotPayload { state: &snapshot_state, plan_title, tasks, auto_links };
            match serde_json::to_vec(&payload) {
                Ok(body) => write_response(&mut stream, "application/json", &body),
                Err(err) => write_response(
                    &mut stream,
                    "application/json",
                    format!(r#"{{"error":"{}"}}"#, escape_json_string(&err.to_string())).as_bytes(),
                ),
            }
        }
        _ => write_not_found(&mut stream),
    }
}

fn derive_auto_links(workspace: &str) -> Vec<DashboardLink> {
    let mut out = Vec::new();
    let encoded_root = encode_url_path(workspace);
    let push = |out: &mut Vec<DashboardLink>, label: &str, suffix: &str| {
        let url = if suffix.is_empty() {
            format!("file://{encoded_root}")
        } else {
            // `suffix` is a fixed ASCII relative path; encoding is a no-op
            // but keeps the construction symmetric.
            format!("file://{encoded_root}/{}", encode_url_path(suffix))
        };
        out.push(DashboardLink { label: label.to_string(), url, source: "workspace" });
    };
    push(&mut out, "Workspace", "");
    push(&mut out, "Runtime logs", "runtime/logs");
    push(&mut out, "Runtime results", "runtime/results");
    out
}

/// Percent-encode a filesystem path for embedding in a `file://` URL.
///
/// Preserves RFC 3986 path-safe bytes (unreserved + sub-delims + `:` `@` `/`)
/// and percent-encodes everything else, including spaces, `#`, `?`, and any
/// non-ASCII byte. Operates on raw bytes so non-UTF-8 paths are still
/// representable.
fn encode_url_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        let safe = matches!(
            byte,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
                | b'/'
        );
        if safe {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", byte));
        }
    }
    out
}

fn write_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn write_not_found(stream: &mut TcpStream) {
    let body = b"not found";
    let _ = write!(
        stream,
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn now_ms() -> u128 {
    system_time_ms(SystemTime::now())
}

fn system_time_ms(value: SystemTime) -> u128 {
    value.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

const DASHBOARD_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Rhei Run Dashboard</title>
<style>
  :root {
    --bg: #0b1020;
    --panel: #131a2e;
    --panel-2: #1a2440;
    --ink: #e7ecf5;
    --muted: #8ea0c5;
    --line: #263259;
    --accent: #93c5fd;
    --warn: #f59e0b;
    --error: #ef4444;
    --ok: #10b981;
  }
  * { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; background: var(--bg); color: var(--ink);
    font: 13px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }
  header { padding: 14px 20px; background: linear-gradient(90deg, #0b1020, #131a2e);
    border-bottom: 1px solid var(--line); display: grid;
    grid-template-columns: 1fr auto; gap: 12px 16px; align-items: baseline; }
  header h1 { margin: 0; font-size: 17px; font-weight: 600; letter-spacing: .2px; }
  header .ws { grid-column: 1 / -1; color: var(--muted); font-size: 12px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer; }
  header .ws:hover { color: var(--ink); }
  .status-pill { color: var(--muted); font-size: 12px; }
  .status-pill .seg { padding: 2px 8px; border-radius: 999px; background: #1a2440;
    margin-right: 6px; }
  .status-pill .seg.ok { background: #062e21; color: #6ee7b7; }
  .status-pill .seg.warn { background: #3f2d05; color: #fde68a; }
  .status-pill .seg.busy { background: #102a4f; color: #93c5fd; }
  .banner { display: none; padding: 8px 20px; font-size: 12px; }
  .banner.show { display: block; }
  .banner.error { background: #3a0d12; color: #fecaca; border-bottom: 1px solid #7f1d1d; }
  .banner.done  { background: #062e21; color: #6ee7b7; border-bottom: 1px solid #047857; }
  .tabs { display: flex; gap: 4px; padding: 0 20px; border-bottom: 1px solid var(--line);
    background: var(--panel); position: sticky; top: 0; z-index: 10; }
  button.tab { background: transparent; color: var(--muted); border: 0;
    border-bottom: 2px solid transparent; border-radius: 0; padding: 10px 14px;
    font: inherit; cursor: pointer; }
  button.tab.active { color: var(--ink); border-bottom-color: var(--accent); }
  button.tab .count { color: var(--muted); margin-left: 6px; font-size: 11px; }
  main { padding: 18px 20px; }
  .legend { display: flex; gap: 14px; flex-wrap: wrap; color: var(--muted);
    font-size: 12px; margin-bottom: 12px; }
  .legend .sw { display: inline-block; width: 10px; height: 10px; border-radius: 3px;
    vertical-align: -1px; margin-right: 6px; }
  .view { display: none; }
  .view.active { display: block; }
  .caption { color: var(--muted); font-size: 12px; margin: 6px 2px 14px; }
  .panel { background: var(--panel); border: 1px solid var(--line); border-radius: 10px;
    overflow: hidden; }
  .panel + .panel { margin-top: 10px; }
  table { width: 100%; border-collapse: collapse; }
  th { text-align: left; color: var(--muted); font-weight: 500; font-size: 11px;
    padding: 8px 12px; border-bottom: 1px solid var(--line); background: #0f1530;
    text-transform: uppercase; letter-spacing: .04em; }
  td { border-bottom: 1px solid var(--line); padding: 8px 12px; vertical-align: top;
    color: var(--ink); }
  tr:last-child td { border-bottom: 0; }
  tr.row-running td { background: rgba(59, 130, 246, 0.08); }
  tr.row-done td { color: var(--muted); }
  .mono { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--muted); }
  .chip { display: inline-block; padding: 2px 8px; border-radius: 999px; font-size: 11px;
    font-weight: 600; color: #0b1020; background: #475569; }
  .chip.outline { background: transparent; border: 1px solid currentColor; color: inherit; }
  .chip.muted { background: #1a2440; color: var(--muted); }
  .chip.ok { background: var(--ok); }
  .chip.warn { background: var(--warn); }
  .chip.busy { background: #3b82f6; color: #0b1020; }
  .filters { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 12px; }
  .filters button { background: #1a2440; color: var(--muted); border: 0; padding: 5px 10px;
    border-radius: 999px; font: inherit; font-size: 12px; cursor: pointer; }
  .filters button.active { background: var(--accent); color: #0b1020; }
  .slot-card { padding: 10px 12px; }
  .slot-card .head { font-weight: 600; display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .slot-card .meta { color: var(--muted); font-size: 12px; margin-top: 2px;
    display: flex; gap: 12px; flex-wrap: wrap; }
  .slot-card .meta a { color: var(--accent); }
  .slot-card .out { margin-top: 8px; max-height: 180px; overflow: auto;
    background: #0a0f23; border: 1px solid var(--line); border-radius: 6px; padding: 6px 8px; }
  .slot-card .out .line { display: flex; gap: 8px; align-items: baseline;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 11.5px; }
  .slot-card .out .ts { color: var(--muted); width: 56px; flex: 0 0 auto; }
  .slot-card .out .stream { width: 50px; flex: 0 0 auto; font-weight: 600; }
  .slot-card .out .stream.stdout { color: var(--accent); }
  .slot-card .out .stream.stderr { color: var(--warn); }
  .slot-card .out .text { white-space: pre-wrap; word-break: break-word; flex: 1; }
  .slot-card .out .repeat { color: var(--muted); }
  .slot-card.idle { opacity: .55; }
  .toggle { display: inline-flex; gap: 6px; align-items: center; color: var(--muted);
    font-size: 12px; cursor: pointer; user-select: none; }
  .journal-line { display: flex; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--line);
    font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; }
  .journal-line:last-child { border-bottom: 0; }
  .journal-line .ts { color: var(--muted); width: 80px; flex: 0 0 auto; }
  .journal-line .lvl { width: 56px; flex: 0 0 auto; font-weight: 600; }
  .journal-line.info .lvl { color: var(--muted); }
  .journal-line.warn .lvl { color: var(--warn); }
  .journal-line.error .lvl { color: var(--error); }
  .journal-line .text { white-space: pre-wrap; word-break: break-word; flex: 1; color: var(--ink); }
  a { color: var(--accent); text-decoration: none; }
  a:hover { text-decoration: underline; }
  .empty { padding: 14px; color: var(--muted); font-size: 12px; }
</style>
</head>
<body>
<header>
  <h1 id="plan-title">Rhei Run Dashboard</h1>
  <div class="status-pill" id="status"><span class="seg">connecting…</span></div>
  <div class="ws" id="workspace" title="click to copy">live execution</div>
</header>
<div class="banner" id="banner"></div>
<div class="tabs">
  <button class="tab active" data-view="tasks">Tasks <span class="count" id="count-tasks"></span></button>
  <button class="tab" data-view="slots">Slots <span class="count" id="count-slots"></span></button>
  <button class="tab" data-view="journal">Journal <span class="count" id="count-journal"></span></button>
  <button class="tab" data-view="links">Links <span class="count" id="count-links"></span></button>
</div>
<main>
  <div class="legend" id="legend"></div>

  <div class="view active" id="view-tasks">
    <div class="caption">Every task in the plan. Re-read on each refresh.</div>
    <div class="filters" id="task-filters"></div>
    <div class="panel">
      <table>
        <thead>
          <tr>
            <th>ID</th><th>Title</th><th>State</th><th>Assignee</th><th>Prior</th><th>Now</th>
          </tr>
        </thead>
        <tbody id="tasks"></tbody>
      </table>
    </div>
  </div>

  <div class="view" id="view-slots">
    <div class="caption">One card per worker. Idle workers are dimmed.</div>
    <label class="toggle"><input type="checkbox" id="hide-stderr"> Hide stderr lines</label>
    <div id="slots-detail"></div>
  </div>

  <div class="view" id="view-journal">
    <div class="caption">Run-event lines, oldest first. Auto-scrolls to bottom while you're at the bottom.</div>
    <div class="panel" id="journal-panel" style="max-height: 600px; overflow: auto;">
      <div id="journal"></div>
    </div>
  </div>

  <div class="view" id="view-links">
    <div class="caption">Workspace shortcuts plus links emitted by the run process.</div>
    <div class="panel"><table><tbody id="links"></tbody></table></div>
  </div>
</main>
<script>
const STATE_ORDER = [
  "draft", "pending", "in-progress", "in_progress", "needs-review",
  "review", "prove", "consolidate", "fix", "agent-review",
  "agent-review-fix", "human-review", "completed", "blocked", "failed",
  "cancelled", "archived",
];
const STATE_COLOR = {
  "draft":            "#64748b",
  "pending":          "#94a3b8",
  "in-progress":      "#3b82f6",
  "in_progress":      "#3b82f6",
  "needs-review":     "#f59e0b",
  "review":           "#a855f7",
  "prove":            "#06b6d4",
  "consolidate":      "#14b8a6",
  "fix":              "#f97316",
  "agent-review":     "#8b5cf6",
  "agent-review-fix": "#ec4899",
  "human-review":     "#22c55e",
  "completed":        "#10b981",
  "blocked":          "#ef4444",
  "failed":           "#ef4444",
  "cancelled":        "#475569",
  "archived":         "#334155",
};
function stateColor(s) { return STATE_COLOR[s] || "#475569"; }
function stateIndex(s) {
  const i = STATE_ORDER.indexOf(s);
  return i >= 0 ? i : STATE_ORDER.length;
}
function escapeHtml(s) {
  return String(s == null ? "" : s).replace(/[&<>"']/g, c =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}
function fmtDuration(ms) {
  if (ms == null || ms < 0) return "";
  if (ms < 1000) return ms + "ms";
  const s = Math.floor(ms / 1000);
  if (s < 60) return s + "s";
  const m = Math.floor(s / 60);
  const rem = s % 60;
  if (m < 60) return rem ? `${m}m${rem}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const remM = m % 60;
  return remM ? `${h}h${remM}m` : `${h}h`;
}
function fmtClock(ms) {
  if (!ms) return "";
  const d = new Date(Number(ms));
  const pad = n => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

let UI = {
  taskFilter: "all",
  hideStderr: false,
};

function setBanner(kind, text) {
  const el = document.getElementById("banner");
  if (!text) { el.classList.remove("show"); return; }
  el.className = "banner show " + kind;
  el.textContent = text;
}

function renderHeader(data) {
  const title = data.plan_title || "Rhei Run Dashboard";
  document.getElementById("plan-title").textContent = title;
  document.title = data.plan_title ? `Rhei: ${title}` : "Rhei Run Dashboard";
  const ws = document.getElementById("workspace");
  ws.textContent = data.workspace || "";
  ws.onclick = () => navigator.clipboard?.writeText(data.workspace || "");

  const slots = data.slots || [];
  const running = slots.filter(s => s.active).length;
  const deferred = (data.deferred || []).length;
  const total = data.total_tasks || (data.tasks || []).length;
  const done = (data.tasks || []).filter(t => isTerminal(t.state)).length;
  const elapsedMs = data.started_at_ms ? (data.updated_at_ms - data.started_at_ms) : 0;
  const segs = [
    `<span class="seg">Pass ${data.pass || 0}</span>`,
    `<span class="seg busy">${running} running</span>`,
  ];
  if (deferred > 0) segs.push(`<span class="seg warn">${deferred} deferred</span>`);
  segs.push(`<span class="seg ${done === total && total > 0 ? "ok" : ""}">${done}/${total} done</span>`);
  if (data.finished) {
    segs.push(`<span class="seg ok">finished</span>`);
  } else {
    segs.push(`<span class="seg">${fmtDuration(elapsedMs)} elapsed</span>`);
  }
  document.getElementById("status").innerHTML = segs.join("");
}

function isTerminal(state) {
  return state === "completed" || state === "cancelled" || state === "archived" || state === "failed";
}

function renderLegend(data) {
  const box = document.getElementById("legend");
  const states = new Set();
  for (const t of data.tasks || []) if (t.state) states.add(t.state);
  for (const s of data.slots || []) if (s.state) states.add(s.state);
  const ordered = [...states].sort((a, b) => stateIndex(a) - stateIndex(b));
  box.innerHTML = ordered.map(s =>
    `<span><span class="sw" style="background:${stateColor(s)}"></span>${escapeHtml(s)}</span>`
  ).join("") || `<span class="mono">(no states yet)</span>`;
}

function setCounts(data) {
  document.getElementById("count-tasks").textContent = (data.tasks || []).length || "";
  const running = (data.slots || []).filter(s => s.active).length;
  document.getElementById("count-slots").textContent = running ? `${running}/${(data.slots || []).length}` : "";
  document.getElementById("count-journal").textContent = (data.recent || []).length || "";
  const links = (data.auto_links || []).length + (data.links || []).length;
  document.getElementById("count-links").textContent = links || "";
}

function renderTaskFilters(data) {
  const tasks = data.tasks || [];
  const buckets = {
    all: tasks.length,
    running: tasks.filter(t => t.in_slot != null).length,
    ready: (data.ready || []).filter(id => !(data.deferred || []).includes(id)).length,
    deferred: (data.deferred || []).length,
    blocked: tasks.filter(t => !isTerminal(t.state) && t.in_slot == null
      && !(data.ready || []).includes(t.id)).length,
    done: tasks.filter(t => isTerminal(t.state)).length,
  };
  const box = document.getElementById("task-filters");
  box.innerHTML = ["all","running","ready","deferred","blocked","done"].map(k => {
    const active = UI.taskFilter === k ? " active" : "";
    return `<button data-filter="${k}" class="${active.trim()}">${k} (${buckets[k]})</button>`;
  }).join("");
  box.querySelectorAll("button").forEach(b => b.addEventListener("click", () => {
    UI.taskFilter = b.dataset.filter;
    render(window.__lastData);
  }));
}

function nowKindForTask(t, data) {
  if (t.in_slot != null) return { cls: "busy", text: `slot ${t.in_slot} · running` };
  if ((data.deferred || []).includes(t.id)) return { cls: "warn", text: "deferred · next pass" };
  if ((data.ready || []).includes(t.id)) return { cls: "outline", text: "ready" };
  if (isTerminal(t.state)) return { cls: "muted", text: t.state };
  return { cls: "muted", text: "blocked" };
}

function renderTasks(data) {
  const tbody = document.getElementById("tasks");
  const tasks = data.tasks || [];
  if (!tasks.length) {
    tbody.innerHTML = `<tr><td colspan="6" class="empty">No plan loaded.</td></tr>`;
    return;
  }
  const filtered = tasks.filter(t => {
    switch (UI.taskFilter) {
      case "running":  return t.in_slot != null;
      case "ready":    return (data.ready || []).includes(t.id) && !(data.deferred || []).includes(t.id);
      case "deferred": return (data.deferred || []).includes(t.id);
      case "blocked":  return !isTerminal(t.state) && t.in_slot == null
                                && !(data.ready || []).includes(t.id);
      case "done":     return isTerminal(t.state);
      default:         return true;
    }
  });
  if (!filtered.length) {
    tbody.innerHTML = `<tr><td colspan="6" class="empty">No tasks match this filter.</td></tr>`;
    return;
  }
  tbody.innerHTML = filtered.map(t => {
    const now = nowKindForTask(t, data);
    const indent = t.depth > 1 ? "&nbsp;".repeat((t.depth - 1) * 2) + "↳ " : "";
    const stateChip = `<span class="chip" style="background:${stateColor(t.state)}">${escapeHtml(t.state)}</span>`;
    const prior = (t.prior || []).length
      ? (t.prior || []).map(p => `<span class="mono">${escapeHtml(p)}</span>`).join(", ")
      : `<span class="mono">—</span>`;
    // Result link is workspace-relative; resolve against the workspace
    // file:// URL so the browser can open it directly.
    const resultLink = t.result_link
      ? ` <a href="file://${encodeURI(data.workspace || "")}/${encodeURI(t.result_link)}" target="_blank" rel="noreferrer" title="open result file">→ result</a>`
      : "";
    const cls = t.in_slot != null ? "row-running" : (isTerminal(t.state) ? "row-done" : "");
    return `<tr class="${cls}">
      <td class="mono">${indent}${escapeHtml(t.id)}</td>
      <td>${escapeHtml(t.title)}${resultLink}</td>
      <td>${stateChip}</td>
      <td class="mono">${escapeHtml(t.assignee || "—")}</td>
      <td>${prior}</td>
      <td><span class="chip ${now.cls}">${escapeHtml(now.text)}</span></td>
    </tr>`;
  }).join("");
}

function renderSlots(data) {
  const box = document.getElementById("slots-detail");
  const slots = data.slots || [];
  if (!slots.length) { box.innerHTML = `<div class="empty">No worker slots.</div>`; return; }
  // Preserve scroll positions of existing output panels keyed by slot index.
  const scrollPositions = {};
  box.querySelectorAll(".out[data-slot]").forEach(o => {
    scrollPositions[o.dataset.slot] = { top: o.scrollTop, atBottom: o.scrollHeight - o.scrollTop - o.clientHeight < 4 };
  });
  box.innerHTML = slots.map((slot, i) => renderSlotCard(slot, i, data)).join("");
  box.querySelectorAll(".out[data-slot]").forEach(o => {
    const prev = scrollPositions[o.dataset.slot];
    if (prev) {
      o.scrollTop = prev.atBottom ? o.scrollHeight : prev.top;
    } else {
      o.scrollTop = o.scrollHeight;
    }
  });
}

function renderSlotCard(slot, i, data) {
  const idle = !slot.active && !slot.task;
  if (idle) {
    return `<div class="panel slot-card idle">
      <div class="head">Worker ${i + 1} <span class="chip muted">idle</span></div>
    </div>`;
  }
  const elapsed = slot.active && slot.started_at_ms
    ? fmtDuration(data.updated_at_ms - slot.started_at_ms)
    : (slot.duration_ms != null ? fmtDuration(slot.duration_ms) : "");
  const stateChip = slot.state
    ? `<span class="chip" style="background:${stateColor(slot.state)}">${escapeHtml(slot.state)}</span>`
    : "";
  // `slot.transition` is only set for real cross-state transitions; for a
  // worker that's just running in its current state we don't show an arrow.
  const transition = slot.transition
    ? `<span class="mono">transitioned ${escapeHtml(slot.transition)}</span>`
    : (slot.state ? `<span class="mono">running in ${escapeHtml(slot.state)}</span>` : "");
  const outcome = slot.outcome
    ? `<span class="chip ${slot.outcome === "completed" ? "ok" : "warn"}">${escapeHtml(slot.outcome)}</span>`
    : "";
  const meta = [
    slot.agent ? `agent: <span class="mono">${escapeHtml(slot.agent)}</span>` : null,
    transition || null,
    elapsed ? `${slot.active ? "running" : "ran"} ${escapeHtml(elapsed)}` : null,
    slot.exit_code != null ? `exit ${slot.exit_code}` : null,
    slot.log_path ? `<a href="file://${encodeURI(slot.log_path)}" target="_blank" rel="noreferrer">view full log</a>` : null,
  ].filter(Boolean).join(" · ");
  const traffic = (slot.traffic || []).filter(t => !UI.hideStderr || t.stream !== "stderr");
  const lines = traffic.length
    ? traffic.map(t => {
        const repeat = t.repeat > 1 ? ` <span class="repeat">×${t.repeat}</span>` : "";
        return `<div class="line"><span class="ts">${fmtClock(t.ts_ms)}</span>` +
               `<span class="stream ${t.stream}">${t.stream}</span>` +
               `<span class="text">${escapeHtml(t.text)}${repeat}</span></div>`;
      }).join("")
    : `<div class="empty">(no output yet)</div>`;
  return `<div class="panel slot-card">
    <div class="head">Worker ${i + 1} · ${escapeHtml(slot.task || "—")} ${stateChip} ${outcome}</div>
    <div class="meta">${meta}</div>
    <div class="out" data-slot="${i}">${lines}</div>
  </div>`;
}

function renderJournal(data) {
  const box = document.getElementById("journal-panel");
  const list = document.getElementById("journal");
  const lines = data.recent || [];
  const atBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 4;
  list.innerHTML = lines.length
    ? lines.map(l => `<div class="journal-line ${l.level}">` +
        `<span class="ts">${fmtClock(l.ts_ms)}</span>` +
        `<span class="lvl">${l.level}</span>` +
        `<span class="text">${escapeHtml(l.text)}</span></div>`).join("")
    : `<div class="empty">(no events yet)</div>`;
  if (atBottom) box.scrollTop = box.scrollHeight;
}

function renderLinks(data) {
  const tbody = document.getElementById("links");
  const all = [...(data.auto_links || []), ...(data.links || [])];
  tbody.innerHTML = all.length
    ? all.map(l => `<tr>
        <td class="mono"><span class="chip muted">${escapeHtml(l.source)}</span></td>
        <td>${escapeHtml(l.label)}</td>
        <td><a href="${escapeHtml(l.url)}" target="_blank" rel="noreferrer">${escapeHtml(l.url)}</a></td>
      </tr>`).join("")
    : `<tr><td colspan="3" class="empty">No links yet.</td></tr>`;
}

function render(data) {
  window.__lastData = data;
  if (data.finished) setBanner("done", "Run finished. Snapshot is frozen at the final state.");
  renderHeader(data);
  renderLegend(data);
  setCounts(data);
  renderTaskFilters(data);
  renderTasks(data);
  renderSlots(data);
  renderJournal(data);
  renderLinks(data);
}

let consecutiveErrors = 0;
async function tick() {
  try {
    const res = await fetch("/snapshot", { cache: "no-store" });
    if (!res.ok) throw new Error("HTTP " + res.status);
    const data = await res.json();
    consecutiveErrors = 0;
    if (!data.finished) setBanner("", "");
    render(data);
  } catch (err) {
    consecutiveErrors++;
    // Two strikes: distinguish a transient blip from a real disconnect so a
    // fast reload doesn't flash a scary banner on every refresh.
    if (consecutiveErrors >= 2) {
      setBanner("error", "Disconnected from rhei: " + err + ". The run process may have exited.");
    }
  }
}

document.querySelectorAll(".tab").forEach(b => {
  b.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach(x => x.classList.remove("active"));
    document.querySelectorAll(".view").forEach(x => x.classList.remove("active"));
    b.classList.add("active");
    document.getElementById("view-" + b.dataset.view).classList.add("active");
  });
});

document.getElementById("hide-stderr").addEventListener("change", e => {
  UI.hideStderr = e.target.checked;
  if (window.__lastData) renderSlots(window.__lastData);
});

tick();
setInterval(tick, 1000);
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Instant, SystemTime};

    fn empty_state() -> DashboardState {
        DashboardState::new(PathBuf::from("/tmp/ws"), 1, 1)
    }

    fn assigned(from: &str, to: &str) -> RunEvent {
        RunEvent::SlotAssigned {
            slot: 0,
            task: "1".to_string(),
            from: from.to_string(),
            to: to.to_string(),
            agent: None,
            log_path: PathBuf::from("/tmp/ws/runtime/logs/1.log"),
            started_at: Instant::now(),
            wall_clock: SystemTime::now(),
        }
    }

    /// Per `RunEvent::SlotAssigned`'s contract, `from == to` means the
    /// engine started a worker in an autonomous state — not a transition.
    /// `slot.transition` must stay `None` so renderers don't paint a phantom
    /// state→state arrow.
    #[test]
    fn same_state_assignment_records_no_transition() {
        let mut state = empty_state();
        state.apply(&assigned("fetch", "fetch"));

        assert!(state.slots[0].active);
        assert_eq!(state.slots[0].state.as_deref(), Some("fetch"));
        assert!(
            state.slots[0].transition.is_none(),
            "from == to must not produce a transition; got {:?}",
            state.slots[0].transition
        );
        let last = state.recent.last().expect("recent line");
        assert!(
            last.text.contains("started in fetch"),
            "expected 'started in fetch'; got {:?}",
            last.text
        );
        assert!(
            !last.text.contains("fetch->fetch") && !last.text.contains("fetch→fetch"),
            "must not render a same-state arrow; got {:?}",
            last.text
        );
    }

    /// A real cross-state assignment must record both the `transition`
    /// string and a `from->to` recent line.
    #[test]
    fn cross_state_assignment_records_arrow_transition() {
        let mut state = empty_state();
        state.apply(&assigned("draft", "pending"));

        assert_eq!(state.slots[0].transition.as_deref(), Some("draft->pending"));
        let last = state.recent.last().expect("recent line");
        assert!(
            last.text.contains("draft->pending"),
            "expected 'draft->pending' in recent; got {:?}",
            last.text
        );
    }

    #[test]
    fn url_path_encodes_unsafe_bytes_and_preserves_slashes() {
        // Slashes, `:`, and unreserved chars stay verbatim; spaces and `#`
        // get percent-encoded; non-ASCII bytes are encoded byte-by-byte.
        assert_eq!(encode_url_path("/Users/me/project"), "/Users/me/project");
        assert_eq!(encode_url_path("/path with spaces/x"), "/path%20with%20spaces/x");
        assert_eq!(encode_url_path("/has#hash?and"), "/has%23hash%3Fand");
        // Two UTF-8 bytes for `é`.
        assert_eq!(encode_url_path("/caf\u{00e9}"), "/caf%C3%A9");
    }
}
