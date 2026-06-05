// End-of-run console summary: after `rhei run` exits and the TUI restores the
// terminal, print a compact, scan-first view — result line, distribution bar,
// counts, attention, and a source-order task tree — without opening a file.

// §FS-rhei-run-report.3: the renderer here is pure; `SummarySink` collects the
// per-task data during the run.

// `HashMap` and `Mutex` are already imported at the crate root (this file is
// `include!`-ed), so they are referenced unqualified without a local `use`.

/// Per-task activity accumulated from the run event stream. The tree shows the
/// driver and timing of the work that advanced each task. §FS-rhei-run-report.3.2
#[derive(Debug, Clone, Default)]
struct TaskActivity {
    /// `"agent"` or `"program"` — the driver of the last invocation for the task.
    driver: Option<&'static str>,
    /// Number of invocations spawned for the task (fan-out targets count > 1).
    invocations: u32,
    /// Duration of the last invocation, milliseconds.
    last_duration_ms: u64,
}

/// One spawned transition from the run event stream, rendered into the report's
/// ledger and invocations (agent/program only; callback and terminal-at-start
/// rows are synthesized at build time). §FS-rhei-run-report.4 §FS-rhei-run-report.7
#[derive(Debug, Clone)]
struct LedgerRecord {
    task: String,
    from: String,
    to: String,
    /// `"agent"` or `"program"`.
    driver: &'static str,
    log_path: std::path::PathBuf,
    exit_code: Option<i32>,
    duration_ms: u64,
    outcome: LedgerOutcome,
}

/// The terminal disposition of a spawned invocation, mirrored from
/// [`rhei_tui::TaskOutcome`] so the renderer does not depend on the TUI enum.
#[derive(Debug, Clone)]
enum LedgerOutcome {
    Completed,
    Failed(String),
    Cancelled,
    TimedOut,
}

/// `EventSink` recording per-task driver/duration for the console task tree and
/// the spawned-transition ledger for the durable report; teed alongside the
/// journal/frontend sinks and read post-run. §FS-rhei-run-report.3.2 §FS-rhei-run-report.8
pub struct SummarySink {
    inner: Mutex<SummaryState>,
}

#[derive(Default)]
struct SummaryState {
    /// Driver of each in-flight slot, keyed by slot index, set on `SlotAssigned`.
    inflight: HashMap<u16, &'static str>,
    /// Finalized per-task activity, keyed by task id.
    tasks: HashMap<String, TaskActivity>,
    /// Spawned transitions in chronological order, for the durable ledger.
    ledger: Vec<LedgerRecord>,
}

impl SummarySink {
    pub fn new() -> Self {
        Self { inner: Mutex::new(SummaryState::default()) }
    }

    /// Snapshot the accumulated activity for rendering after the run. A poisoned
    /// lock (a worker panicked mid-run) degrades to empty rather than panicking
    /// the report — a partial report still beats none.
    fn snapshot(&self) -> HashMap<String, TaskActivity> {
        self.inner.lock().map(|state| state.tasks.clone()).unwrap_or_default()
    }

    /// The spawned-transition ledger in chronological order; empty on a poisoned
    /// lock, for the same best-effort reason as [`snapshot`](Self::snapshot).
    fn ledger(&self) -> Vec<LedgerRecord> {
        self.inner.lock().map(|state| state.ledger.clone()).unwrap_or_default()
    }
}

impl Default for SummarySink {
    fn default() -> Self {
        Self::new()
    }
}

impl rhei_tui::EventSink for SummarySink {
    fn emit(&self, event: rhei_tui::RunEvent) {
        let mut state = match self.inner.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        match event {
            // `agent` is `Some` for agent-backed work, `None` for programs.
            rhei_tui::RunEvent::SlotAssigned { slot, agent, .. } => {
                let driver = if agent.is_some() { "agent" } else { "program" };
                state.inflight.insert(slot, driver);
            }
            rhei_tui::RunEvent::SlotReleased {
                slot,
                task,
                from,
                to,
                log_path,
                outcome,
                exit_code,
                duration_ms,
                ..
            } => {
                let driver = state.inflight.remove(&slot).unwrap_or("program");
                let entry = state.tasks.entry(task.clone()).or_default();
                entry.driver = Some(driver);
                entry.invocations += 1;
                entry.last_duration_ms = duration_ms;
                let outcome = match outcome {
                    rhei_tui::TaskOutcome::Completed => LedgerOutcome::Completed,
                    rhei_tui::TaskOutcome::Failed(msg) => LedgerOutcome::Failed(msg),
                    rhei_tui::TaskOutcome::Cancelled => LedgerOutcome::Cancelled,
                    rhei_tui::TaskOutcome::TimedOut => LedgerOutcome::TimedOut,
                };
                state.ledger.push(LedgerRecord {
                    task,
                    from,
                    to,
                    driver,
                    log_path,
                    exit_code,
                    duration_ms,
                    outcome,
                });
            }
            _ => {}
        }
    }
}

/// Build the report, write the durable Markdown files, then print the run's
/// end-of-run surface: rich console summary on a TTY, else a `Report:` pointer.
/// Best-effort — a load or write failure must not mask the result. §FS-rhei-run-report.1 §FS-rhei-run-report.3
fn emit_run_report(
    input: &std::path::Path,
    machine: &rhei_validator::StateMachine,
    summary: &SummarySink,
    runtime_dir: &std::path::Path,
    stats: RunStats,
) {
    use std::io::IsTerminal;
    let Ok(loaded) = load_plan(input) else {
        return;
    };
    let mut report = RunSummaryReport::build(&loaded.rhei, machine, summary, stats);
    // The durable report is the point of this surface; the console view is its
    // pointer. Write it even when stdout is piped, so CI runs leave the artifact.
    if let Err(err) = report.write_to_runtime(runtime_dir) {
        eprintln!("warning: could not write run report: {err}");
    }
    if std::io::stdout().is_terminal() {
        // Honor NO_COLOR for users who disable ANSI globally.
        let color = std::env::var_os("NO_COLOR").is_none();
        print!("{}", report.render_tty(color));
    } else if let Some(report_path) = &report.report_path {
        println!("Report: {report_path}");
    }
}

/// A short, stable run identifier derived from the run's wall-clock start. FNV-1a
/// over the start nanoseconds folded to six hex digits — enough to disambiguate
/// history entries without a random-number dependency. §FS-rhei-run-report.2
fn short_run_id(started_at: std::time::SystemTime) -> String {
    let nanos =
        started_at.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in nanos.to_le_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:06x}", hash & 0xff_ffff)
}

/// The relative path to the frozen dashboard artifact when one was written this
/// run, for the report's Dashboard pointer. Gated on `enabled_this_run` so a
/// stale `dashboard.html` left by an earlier run is never linked. §FS-rhei-run-report.2
fn frozen_dashboard_relative_path(
    enabled_this_run: bool,
    runtime_dir: &std::path::Path,
    workspace_root: &std::path::Path,
) -> Option<String> {
    if !enabled_this_run {
        return None;
    }
    let path = runtime_dir.join("dashboard.html");
    path.exists().then(|| relativize(&path, workspace_root))
}

/// The current process command line with `argv[0]` normalized to `rhei`, so the
/// report header records the real flags the operator ran. §FS-rhei-run-report.2
fn current_command_line() -> String {
    let mut args: Vec<String> = std::env::args().collect();
    if let Some(first) = args.first_mut() {
        *first = "rhei".to_string();
    }
    args.join(" ")
}

/// Snapshot each task's normalized state at run start, keyed by task id, so the
/// report can mark terminal-at-start tasks and reconcile callback advances that
/// emit no slot events. §FS-rhei-run-report.8
fn collect_initial_states(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> HashMap<String, String> {
    fn walk(
        tasks: &[rhei_core::ast::Task],
        machine: &rhei_validator::StateMachine,
        out: &mut HashMap<String, String>,
    ) {
        for task in tasks {
            out.insert(task.id.to_string(), normalized_state_name(task.state.as_str(), machine));
            walk(&task.children, machine, out);
        }
    }
    let mut out = HashMap::new();
    walk(&rhei.tasks, machine, &mut out);
    out
}

/// Writes a best-effort report if `rhei run` returns early with an error.
/// Declared before the frontend so it drops after the terminal is restored; the
/// happy path disarms it after the full report is written. §FS-rhei-run-report.1
struct RunReportGuard<'a> {
    input: &'a std::path::Path,
    machine: &'a rhei_validator::StateMachine,
    runtime_dir: std::path::PathBuf,
    run_started: std::time::Instant,
    run_started_wall: std::time::SystemTime,
    run_id: String,
    workspace_root: std::path::PathBuf,
    command: String,
    parallel: usize,
    mode: &'static str,
    initial_states: HashMap<String, String>,
    /// Set once the frontend exists; without it there is nothing to report from.
    summary: Option<std::sync::Arc<SummarySink>>,
    /// Cleared by the happy path after the authoritative report is written.
    armed: bool,
}

impl RunReportGuard<'_> {
    /// The run wrote its own report; suppress the best-effort fallback.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RunReportGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Some(summary) = self.summary.clone() else {
            return;
        };
        // Best-effort from the data captured before the failure: spawn counts come
        // from the ledger, callbacks and dashboard are unknown on an aborted run.
        let ledger = summary.ledger();
        let agents = ledger.iter().filter(|r| r.driver == "agent").count() as u32;
        let programs = ledger.iter().filter(|r| r.driver == "program").count() as u32;
        emit_run_report(
            self.input,
            self.machine,
            &summary,
            &self.runtime_dir,
            RunStats {
                agents_spawned: agents,
                programs_spawned: programs,
                callback_only: 0,
                duration: Some(self.run_started.elapsed()),
                dashboard: None,
                run_id: self.run_id.clone(),
                started_at: Some(self.run_started_wall),
                workspace_root: self.workspace_root.clone(),
                command: self.command.clone(),
                parallel: self.parallel,
                mode: self.mode,
                initial_states: self.initial_states.clone(),
                dry_run: false,
            },
        );
    }
}

/// The scan glyph for a task's final state. Color and the state label remain the
/// primary signal; the marker degrades to an ASCII fallback. §FS-rhei-run-report.3.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    /// Terminal-success state.
    Done,
    /// Gating state awaiting a human.
    Gate,
    /// Blocked or failed — needs attention.
    Attention,
    /// Cancelled.
    Cancelled,
    /// Terminal at the start of the run — no work was attempted. §FS-rhei-run-report.3.2
    TerminalAtStart,
}

impl Marker {
    fn glyph(self) -> char {
        match self {
            Marker::Done => '✓',
            Marker::Gate => '⏸',
            Marker::Attention => '!',
            Marker::Cancelled => '⊘',
            Marker::TerminalAtStart => '·',
        }
    }

    /// ANSI color for this marker class. Only `Attention` and `Gate` are
    /// saturated; success, cancelled, and terminal-at-start rows stay calm.
    /// §FS-rhei-viz-ux.3
    fn color(self) -> &'static str {
        match self {
            Marker::Done => GREEN,
            Marker::Gate => YELLOW,
            Marker::Attention => RED,
            Marker::Cancelled => DIM,
            Marker::TerminalAtStart => DIM,
        }
    }

    /// Whether this marker represents a task a human must still act on.
    fn needs_attention(self) -> bool {
        matches!(self, Marker::Gate | Marker::Attention)
    }
}

/// Classify a state into a marker. Failure/cancel state names win over the
/// `gating` flag: a machine may park a `blocked` task in a gating state, but it
/// still reads as attention, not a calm gate. §FS-rhei-run-report.3.2
fn classify_marker(state: &str, machine: &rhei_validator::StateMachine) -> Marker {
    match state {
        "cancelled" | "canceled" => return Marker::Cancelled,
        "blocked" | "failed" => return Marker::Attention,
        _ => {}
    }
    let def = machine.states.get(state);
    if def.map(|d| d.gating).unwrap_or(false) {
        Marker::Gate
    } else if def.map(|d| d.terminal).unwrap_or(false) {
        Marker::Done
    } else {
        Marker::Attention
    }
}

/// One row of the source-order task tree.
struct TaskRow {
    depth: usize,
    id: String,
    state: String,
    marker: Marker,
    /// Driver + timing for advanced tasks, or a short reason for halted ones.
    detail: Option<String>,
}

/// A halted task surfaced in the Attention group, with its proven blocker and
/// the next action. §FS-rhei-run-report.3.1
struct AttentionRow {
    id: String,
    state: String,
    reason: String,
    next: String,
    /// True for a gating state awaiting a human; false for a blocked/failed task.
    is_gate: bool,
}

/// Run-level facts the summary needs beyond the plan itself. §FS-rhei-run-report.8
pub struct RunStats {
    pub agents_spawned: u32,
    pub programs_spawned: u32,
    pub callback_only: u32,
    pub duration: Option<std::time::Duration>,
    pub dashboard: Option<String>,
    /// Short run identifier shown in the header and history filename.
    pub run_id: String,
    /// Wall-clock start, rendered in the report header. `None` falls back to
    /// the run id alone.
    pub started_at: Option<std::time::SystemTime>,
    /// Workspace root, used to render relative artifact links. §FS-rhei-run-report.1
    pub workspace_root: std::path::PathBuf,
    /// The command label shown in the header (`rhei run …`).
    pub command: String,
    /// Worker parallelism for the run.
    pub parallel: usize,
    /// `"agent"` or `"callback"` execution mode.
    pub mode: &'static str,
    /// Task id → normalized state at run start, for terminal-at-start detection
    /// and reconciling callback advances that emit no slot events.
    /// §FS-rhei-run-report.8
    pub initial_states: HashMap<String, String>,
    /// True under `--dry-run`: the report records a simulated run that applied no
    /// changes, so its result line and counts read as a preview. §FS-rhei-run-report.3.5
    pub dry_run: bool,
}

/// One rendered Transition Ledger row. §FS-rhei-run-report.4
struct LedgerEntry {
    task: String,
    from: String,
    /// Destination state, or `-` when no transition was taken.
    to: String,
    /// `agent`, `program`, `callback-only`, `terminal-at-start`, or `blocked`.
    driver: &'static str,
    /// Invocation label + relative log link, or `none`.
    invocation: String,
    reason: String,
}

/// One spawned agent/program for the Invocations section. §FS-rhei-run-report.7
struct InvocationRow {
    driver: &'static str,
    task: String,
    /// `exit 0`, `exit 42`, `cancelled`, `timed out`, or `—`.
    exit: String,
    duration_ms: u64,
    /// Relative log path.
    log: String,
}

/// The fully resolved run report, ready to render to the console or to Markdown.
pub struct RunSummaryReport {
    title: String,
    result: String,
    duration: Option<std::time::Duration>,
    /// State label, count, and marker class, in canonical count order.
    state_counts: Vec<(String, usize, Marker)>,
    total_tasks: usize,
    work: String,
    attention: Vec<AttentionRow>,
    rows: Vec<TaskRow>,
    dashboard: Option<String>,
    // ── Durable-report fields (§FS-rhei-run-report.1, .2, .4, .7) ────────────
    run_id: String,
    started_at: Option<std::time::SystemTime>,
    workspace: String,
    command: String,
    parallel: usize,
    mode: &'static str,
    agents_spawned: u32,
    programs_spawned: u32,
    callback_only: u32,
    terminal_at_start: usize,
    ledger: Vec<LedgerEntry>,
    invocations: Vec<InvocationRow>,
    /// Relative paths to the written report files, filled by [`write_to_runtime`].
    report_path: Option<String>,
    history_path: Option<String>,
}

// ANSI codes; emitted only when color is enabled.
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";

/// Width of the static state-distribution bar, in cells.
const BAR_WIDTH: usize = 24;
/// Maximum task rows printed before fully-completed subtrees collapse.
const MAX_TASK_ROWS: usize = 40;
/// Maximum attention rows printed before the rest defer to the report.
const MAX_ATTENTION_ROWS: usize = 5;

impl RunSummaryReport {
    /// Build the report from the on-disk plan, the run's spawn counts, and the
    /// per-task activity captured by [`SummarySink`]. §FS-rhei-run-report.8
    pub fn build(
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        summary: &SummarySink,
        stats: RunStats,
    ) -> Self {
        let activity = summary.snapshot();

        // Source-order walk that preserves hierarchy depth.
        let mut rows = Vec::new();
        let mut attention = Vec::new();
        let mut counts: std::collections::BTreeMap<String, (usize, Marker)> =
            std::collections::BTreeMap::new();
        collect_rows(
            &rhei.tasks,
            0,
            machine,
            &activity,
            &mut rows,
            &mut attention,
            &mut counts,
        );

        // Terminal-at-start: same terminal state at run start as now, so no work
        // was attempted. The row keeps its state count but flips to the calm `·`
        // marker so it reads apart from work that just ran. §FS-rhei-run-report.3.2
        let mut terminal_at_start = 0usize;
        for row in &mut rows {
            let was = stats.initial_states.get(&row.id).map(String::as_str);
            let unchanged_terminal =
                was == Some(row.state.as_str()) && is_terminal_state(&row.state, machine);
            if unchanged_terminal {
                terminal_at_start += 1;
                // A success state flips to the calm `·` marker; a cancelled task
                // keeps its own `⊘` marker but still counts as terminal-at-start.
                if row.marker == Marker::Done {
                    row.marker = Marker::TerminalAtStart;
                    row.detail = Some("terminal at start".to_string());
                }
            }
        }

        let total_tasks = rows.len();

        // Counts in canonical order: success, gate, attention, cancelled.
        let mut state_counts: Vec<(String, usize, Marker)> =
            counts.into_iter().map(|(state, (n, marker))| (state, n, marker)).collect();
        state_counts.sort_by_key(|(_, _, marker)| marker_order(*marker));

        let no_work = stats.agents_spawned == 0 && stats.programs_spawned == 0;
        let advanced_without_work = rows.iter().any(|r| {
            r.marker == Marker::Done
                && stats.initial_states.get(&r.id).map(String::as_str) != Some(r.state.as_str())
        });
        // A dry run simulated transitions but applied nothing, so its result
        // reads as a preview rather than an outcome. §FS-rhei-run-report.3.5
        let result = if stats.dry_run {
            "dry run — no changes applied".to_string()
        } else {
            result_phrase(&attention, &rows, no_work, advanced_without_work)
        };
        let work = format_work(stats.agents_spawned, stats.programs_spawned, stats.callback_only);

        let ledger = build_ledger(
            &rows,
            &attention,
            &summary.ledger(),
            &stats.initial_states,
            machine,
            &stats.workspace_root,
        );
        let invocations = build_invocations(&summary.ledger(), &stats.workspace_root);

        Self {
            title: rhei.title.clone(),
            result,
            duration: stats.duration,
            state_counts,
            total_tasks,
            work,
            attention,
            rows,
            dashboard: stats.dashboard,
            run_id: stats.run_id,
            started_at: stats.started_at,
            workspace: stats.workspace_root.display().to_string(),
            command: stats.command,
            parallel: stats.parallel,
            mode: stats.mode,
            agents_spawned: stats.agents_spawned,
            programs_spawned: stats.programs_spawned,
            callback_only: stats.callback_only,
            terminal_at_start,
            ledger,
            invocations,
            report_path: None,
            history_path: None,
        }
    }

    /// Render the rich, colored summary for an interactive terminal.
    /// §FS-rhei-run-report.3.1
    pub fn render_tty(&self, color: bool) -> String {
        let c = Palette::new(color);
        let mut out = String::new();

        // Header: title + duration, then the result line.
        let dur = self.duration.map(format_duration_long).unwrap_or_default();
        out.push_str(&format!(
            "\n{}Run Report{}  {}{}{}",
            c.bold, c.reset, c.bold, self.title, c.reset
        ));
        if !dur.is_empty() {
            out.push_str(&format!("   {}{}{}", c.dim, dur, c.reset));
        }
        out.push('\n');
        out.push_str(&format!("  {}{}{}\n\n", c.result_color(&self.result), self.result, c.reset));

        // Counts: distribution bar + labeled states, then work.
        out.push_str("  States    ");
        out.push_str(&self.render_bar(&c));
        out.push_str("   ");
        out.push_str(&self.render_state_labels(&c));
        out.push('\n');
        out.push_str(&format!("  Work      {}\n", self.work));

        // Attention.
        if !self.attention.is_empty() {
            let gated = self.attention.iter().filter(|a| a.is_gate).count();
            let blocked = self.attention.len() - gated;
            out.push_str(&format!(
                "\n{}Attention{}  {} gated · {} blocked\n",
                c.bold, c.reset, gated, blocked
            ));
            for row in self.attention.iter().take(MAX_ATTENTION_ROWS) {
                out.push_str(&format!(
                    "  {}!{} {:<26} {}{:<11}{} {}\n",
                    c.red, c.reset, row.id, c.dim, row.state, c.reset, row.reason
                ));
                out.push_str(&format!("        {}→ {}{}\n", c.dim, row.next, c.reset));
            }
            if self.attention.len() > MAX_ATTENTION_ROWS {
                out.push_str(&format!(
                    "  {}… {} more in the report{}\n",
                    c.dim,
                    self.attention.len() - MAX_ATTENTION_ROWS,
                    c.reset
                ));
            }
        }

        // Task tree.
        out.push_str(&format!(
            "\n{}Tasks{}   {} tasks · source order\n",
            c.bold, c.reset, self.total_tasks
        ));
        out.push_str(&self.render_tree(&c));

        // Pointers: the durable report is the at-a-glance summary's companion;
        // the console points at it for the full forensic read. §FS-rhei-run-report.3.1
        out.push('\n');
        if let Some(report) = &self.report_path {
            out.push_str(&format!("Report     {report}\n"));
        }
        if let Some(history) = &self.history_path {
            out.push_str(&format!("History    {history}\n"));
        }
        if let Some(dashboard) = &self.dashboard {
            out.push_str(&format!("Dashboard  {dashboard}\n"));
        }
        // Drop trailing spaces left by empty detail columns; keep the final newline.
        let trailing_newline = out.ends_with('\n');
        let mut trimmed = out.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
        if trailing_newline {
            trimmed.push('\n');
        }
        trimmed
    }

    /// Render the durable Markdown report — header, outcome strip, attention,
    /// ledger, task final states, invocations: the commit-friendly explanation
    /// an operator can read without the dashboard. §FS-rhei-run-report.1 §FS-rhei-run-report.2
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();

        // 1. Header.
        out.push_str(&format!("# Run Report: {}\n\n", self.title));
        let when = self
            .started_at
            .map(format_iso8601_utc)
            .map(|ts| format!("{ts} / {}", self.run_id))
            .unwrap_or_else(|| self.run_id.clone());
        out.push_str(&format!("Run: {when}\n"));
        out.push_str(&format!("Workspace: {}\n", self.workspace));
        out.push_str(&format!("Command: {}\n", self.command));
        out.push_str(&format!("Mode: {} · parallel {}\n", self.mode, self.parallel));
        if let Some(dur) = self.duration {
            out.push_str(&format!("Duration: {}\n", format_duration_long(dur)));
        }
        out.push_str(&format!("Result: {}\n", self.result));
        if let Some(dashboard) = &self.dashboard {
            out.push_str(&format!("Dashboard: {dashboard}\n"));
        }
        out.push('\n');

        // 2. Outcome strip — final states and run activity. The reuse/blocked
        // signal sits at the top of the report, never below a fold.
        out.push_str("| Final states | Count |\n| --- | ---: |\n");
        for (state, n, _) in &self.state_counts {
            out.push_str(&format!("| {state} | {n} |\n"));
        }
        out.push('\n');
        let could_not_advance = self.attention.len();
        out.push_str("| Activity | Count |\n| --- | ---: |\n");
        out.push_str(&format!("| agent invocations | {} |\n", self.agents_spawned));
        out.push_str(&format!("| program invocations | {} |\n", self.programs_spawned));
        out.push_str(&format!("| callback-only transitions | {} |\n", self.callback_only));
        out.push_str(&format!("| terminal at start | {} |\n", self.terminal_at_start));
        out.push_str(&format!("| could not advance | {could_not_advance} |\n"));
        out.push('\n');
        if self.agents_spawned == 0 && self.programs_spawned == 0 {
            out.push_str(
                "> No agent or program ran this run. Any task that advanced did so through \
                 callbacks, transition rules, or outputs that already existed — inspect the \
                 ledger below before assuming work was performed.\n\n",
            );
        }

        // 3. Attention.
        if !self.attention.is_empty() {
            out.push_str("## Attention\n\n");
            out.push_str("| Task | State | Reason | Next action |\n| --- | --- | --- | --- |\n");
            for a in &self.attention {
                out.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    md_cell(&a.id),
                    md_cell(&a.state),
                    md_cell(&a.reason),
                    md_cell(&a.next),
                ));
            }
            out.push('\n');
        }

        // 4. Transition ledger.
        out.push_str("## Transition Ledger\n\n");
        out.push_str(
            "| Task | From | To | Driver | Invocation | Reason |\n\
             | --- | --- | --- | --- | --- | --- |\n",
        );
        for e in &self.ledger {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                e.task,
                md_cell(&e.from),
                md_cell(&e.to),
                e.driver,
                md_link_or_text(&e.invocation),
                md_cell(&e.reason),
            ));
        }
        out.push('\n');

        // 5. Task final states.
        out.push_str("## Task Final States\n\n");
        for row in &self.rows {
            let indent = "  ".repeat(row.depth);
            let detail = row.detail.as_deref().unwrap_or("");
            let detail = if detail.is_empty() {
                String::new()
            } else {
                format!(" — {detail}")
            };
            out.push_str(&format!(
                "{indent}- {} `{}` ({}){detail}\n",
                row.marker.glyph(),
                row.id,
                row.state,
            ));
        }
        out.push('\n');

        // 6. Invocations.
        if !self.invocations.is_empty() {
            out.push_str("## Invocations\n\n");
            out.push_str(
                "| Task | Driver | Exit | Duration | Log |\n| --- | --- | --- | --- | --- |\n",
            );
            for inv in &self.invocations {
                out.push_str(&format!(
                    "| {} | {} | {} | {} | [{}]({}) |\n",
                    inv.task,
                    inv.driver,
                    inv.exit,
                    format_duration_short(inv.duration_ms),
                    inv.log,
                    inv.log,
                ));
            }
            out.push('\n');
        }

        out
    }

    /// Write the durable report to `runtime/run-report.md` and a timestamped
    /// history entry, recording the relative paths for the console pointer.
    /// Best-effort. §FS-rhei-run-report.1
    pub fn write_to_runtime(&mut self, runtime_dir: &std::path::Path) -> std::io::Result<()> {
        let body = self.render_markdown();
        let latest = runtime_dir.join("run-report.md");
        let history_dir = runtime_dir.join("run-reports");
        std::fs::create_dir_all(&history_dir)?;
        let stamp = self
            .started_at
            .map(format_iso8601_utc)
            .map(|ts| ts.replace(':', "-"))
            .unwrap_or_else(|| "unknown".to_string());
        let history = history_dir.join(format!("{stamp}-{}.md", self.run_id));
        std::fs::write(&latest, &body)?;
        std::fs::write(&history, &body)?;
        self.report_path = Some(relativize(&latest, &self.workspace_root_path()));
        self.history_path = Some(relativize(&history, &self.workspace_root_path()));
        Ok(())
    }

    /// The workspace root reconstructed from its display string, for link bases.
    fn workspace_root_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.workspace)
    }

    /// The static state-distribution bar, sized by count and colored by class.
    /// Drawn once; never animates. §FS-rhei-run-report.3.1 §FS-rhei-viz-ux.4
    fn render_bar(&self, c: &Palette) -> String {
        if self.total_tasks == 0 {
            return String::new();
        }
        // Proportional widths, with at least one cell per non-empty state.
        let mut widths: Vec<usize> = self
            .state_counts
            .iter()
            .map(|(_, n, _)| {
                let w = (*n * BAR_WIDTH) / self.total_tasks;
                if *n > 0 {
                    w.max(1)
                } else {
                    0
                }
            })
            .collect();
        // Trim overflow from the largest segment so total == BAR_WIDTH.
        let mut total: usize = widths.iter().sum();
        while total > BAR_WIDTH {
            if let Some((idx, _)) =
                widths.iter().enumerate().filter(|(_, w)| **w > 1).max_by_key(|(_, w)| **w)
            {
                widths[idx] -= 1;
                total -= 1;
            } else {
                break;
            }
        }
        let mut bar = String::new();
        for ((_, _, marker), w) in self.state_counts.iter().zip(widths) {
            if w == 0 {
                continue;
            }
            bar.push_str(c.color(marker.color()));
            bar.push_str(&"█".repeat(w));
            bar.push_str(c.reset);
        }
        bar
    }

    fn render_state_labels(&self, c: &Palette) -> String {
        self.state_counts
            .iter()
            .map(|(state, n, marker)| {
                format!("{}{} {}{}", c.color(marker.color()), n, state, c.reset)
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn render_tree(&self, c: &Palette) -> String {
        let mut out = String::new();
        let mut collapsed = 0usize;
        let mut shown = 0usize;
        for row in &self.rows {
            // Collapse calm completed leaf rows once the tree grows long, but
            // never hide anything that needs a human. §FS-rhei-run-report.3.2
            if shown >= MAX_TASK_ROWS && row.marker == Marker::Done {
                collapsed += 1;
                continue;
            }
            shown += 1;
            let gutter = if row.depth > 0 { "│ ".repeat(row.depth) } else { String::new() };
            let detail = row.detail.as_deref().unwrap_or("");
            // Pad the state column *outside* the color codes so that empty-detail
            // rows can have their trailing padding trimmed away.
            let state_cell = c.colored(row.marker.color(), &row.state);
            let state_pad = " ".repeat(11usize.saturating_sub(row.state.chars().count()));
            out.push_str(&format!(
                "  {}{}{}{} {:<width$} {}{} {}\n",
                c.dim,
                gutter,
                c.reset,
                c.colored(row.marker.color(), &row.marker.glyph().to_string()),
                row.id,
                state_cell,
                state_pad,
                detail,
                width = 26usize.saturating_sub(row.depth * 2),
            ));
        }
        if collapsed > 0 {
            out.push_str(&format!(
                "  {}… {collapsed} completed tasks collapsed{}\n",
                c.dim, c.reset
            ));
        }
        out
    }
}

/// Recursive source-order walk capturing depth, markers, detail, counts, and
/// the attention list.
#[allow(clippy::too_many_arguments)]
fn collect_rows(
    tasks: &[rhei_core::ast::Task],
    depth: usize,
    machine: &rhei_validator::StateMachine,
    activity: &HashMap<String, TaskActivity>,
    rows: &mut Vec<TaskRow>,
    attention: &mut Vec<AttentionRow>,
    counts: &mut std::collections::BTreeMap<String, (usize, Marker)>,
) {
    for task in tasks {
        let state = normalized_state_name(task.state.as_str(), machine);
        let marker = classify_marker(&state, machine);
        let id = task.id.to_string();

        let entry = counts.entry(state.clone()).or_insert((0, marker));
        entry.0 += 1;

        let detail = task_detail(&id, &state, marker, activity);
        if marker.needs_attention() {
            let (reason, next) = attention_reason(marker, &state);
            attention.push(AttentionRow {
                id: id.clone(),
                state: state.clone(),
                reason,
                next,
                is_gate: marker == Marker::Gate,
            });
        }

        rows.push(TaskRow { depth, id, state, marker, detail });
        collect_rows(&task.children, depth + 1, machine, activity, rows, attention, counts);
    }
}

/// Build the detail column for a task row: driver + timing when the run spawned
/// work, otherwise a short reason for halted tasks. §FS-rhei-run-report.3.2
fn task_detail(
    id: &str,
    state: &str,
    marker: Marker,
    activity: &HashMap<String, TaskActivity>,
) -> Option<String> {
    if let Some(act) = activity.get(id) {
        if let Some(driver) = act.driver {
            let label = if act.invocations > 1 {
                format!("{driver}×{}", act.invocations)
            } else {
                driver.to_string()
            };
            return Some(format!("{label}  {}", format_duration_short(act.last_duration_ms)));
        }
    }
    match marker {
        Marker::Gate | Marker::Attention => Some(attention_reason(marker, state).0),
        _ => None,
    }
}

/// A generic, honest reason and next action for a halted task. Precise blockers
/// (exit codes, poll counts) depend on scheduler tracking and are filled in by
/// the durable report. §FS-rhei-run-report.3.1
fn attention_reason(marker: Marker, state: &str) -> (String, String) {
    match marker {
        Marker::Gate => (
            "gating state awaiting review".to_string(),
            "transition manually when reviewed".to_string(),
        ),
        _ => {
            let reason = if state.is_empty() {
                "no forward transition available".to_string()
            } else {
                format!("stalled in non-terminal state {state}")
            };
            (reason, "inspect logs or mark the task cancelled".to_string())
        }
    }
}

fn result_phrase(
    attention: &[AttentionRow],
    rows: &[TaskRow],
    no_work: bool,
    advanced_without_work: bool,
) -> String {
    let all_terminal_success =
        rows.iter().all(|r| matches!(r.marker, Marker::Done | Marker::TerminalAtStart));
    if !attention.is_empty() {
        // Gated and blocked tasks both halt the run for a human; the report and
        // tree carry the per-task distinction. §FS-rhei-run-report.6
        "stopped for human attention".to_string()
    } else if all_terminal_success && no_work && advanced_without_work {
        // A run that advanced tasks while spawning nothing must not read like a
        // fast successful run — name the absence of work. §FS-rhei-run-report.3.3
        "completed — no work spawned".to_string()
    } else if all_terminal_success {
        "completed".to_string()
    } else {
        "finished".to_string()
    }
}

/// Escape a value for a Markdown table cell: pipes would split the column and
/// newlines would break the row, so both are neutralized.
fn md_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

/// Render an invocation cell. `"<driver> / <log>"` becomes `<driver> / [log](log)`
/// so the log is a relative link; anything else (notably `none`) is escaped text.
/// §FS-rhei-run-report.7
fn md_link_or_text(value: &str) -> String {
    match value.split_once(" / ") {
        Some((label, path)) => format!("{} / [{}]({})", md_cell(label), path, path),
        None => md_cell(value),
    }
}

/// Render a path relative to the workspace root with forward slashes, so report
/// links survive the workspace being moved, committed, or pasted into an issue.
/// §FS-rhei-run-report.1
fn relativize(path: &std::path::Path, root: &std::path::Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A short reason string for a spawned invocation, from its outcome and exit.
fn ledger_outcome_reason(outcome: &LedgerOutcome, exit_code: Option<i32>) -> String {
    match outcome {
        LedgerOutcome::Completed => match exit_code {
            Some(0) | None => "exit 0".to_string(),
            Some(code) => format!("exit {code}"),
        },
        LedgerOutcome::Failed(msg) => {
            let msg = msg.lines().next().unwrap_or("").trim();
            match exit_code {
                Some(code) if msg.is_empty() => format!("failed, exit {code}"),
                Some(code) => format!("exit {code}: {msg}"),
                None if msg.is_empty() => "failed".to_string(),
                None => format!("failed: {msg}"),
            }
        }
        LedgerOutcome::Cancelled => "cancelled".to_string(),
        LedgerOutcome::TimedOut => "timed out".to_string(),
    }
}

/// Assemble the Transition Ledger in source order: spawned rows from the event
/// stream, plus synthesized callback / terminal-at-start / blocked rows for tasks
/// that emit no slot events. §FS-rhei-run-report.4
fn build_ledger(
    rows: &[TaskRow],
    attention: &[AttentionRow],
    records: &[LedgerRecord],
    initial_states: &HashMap<String, String>,
    machine: &rhei_validator::StateMachine,
    workspace_root: &std::path::Path,
) -> Vec<LedgerEntry> {
    let attention_by_id: HashMap<&str, &AttentionRow> =
        attention.iter().map(|a| (a.id.as_str(), a)).collect();
    let mut ledger = Vec::new();
    for row in rows {
        let task_records: Vec<&LedgerRecord> =
            records.iter().filter(|r| r.task == row.id).collect();
        if !task_records.is_empty() {
            for rec in &task_records {
                let log = relativize(&rec.log_path, workspace_root);
                ledger.push(LedgerEntry {
                    task: row.id.clone(),
                    from: rec.from.clone(),
                    to: rec.to.clone(),
                    driver: rec.driver,
                    invocation: format!("{} / {}", rec.driver, log),
                    reason: ledger_outcome_reason(&rec.outcome, rec.exit_code),
                });
            }
            // If the task ended in a terminal-success state past the last spawned
            // transition, a callback or transition rule carried it the rest of the
            // way — record that advance so the ledger reaches the final state.
            let last_to = task_records.last().map(|r| r.to.as_str());
            if matches!(row.marker, Marker::Done | Marker::TerminalAtStart)
                && last_to != Some(row.state.as_str())
            {
                ledger.push(LedgerEntry {
                    task: row.id.clone(),
                    from: last_to.unwrap_or("").to_string(),
                    to: row.state.clone(),
                    driver: "callback-only",
                    invocation: "none".to_string(),
                    reason: "advanced without spawning work".to_string(),
                });
            }
            continue;
        }

        // No invocation ran for this task this run — classify why it sits where
        // it does from the plan and the initial-state snapshot.
        let initial = initial_states.get(&row.id).map(String::as_str);
        if row.marker == Marker::TerminalAtStart {
            ledger.push(LedgerEntry {
                task: row.id.clone(),
                from: row.state.clone(),
                to: "-".to_string(),
                driver: "terminal-at-start",
                invocation: "none".to_string(),
                reason: "already terminal".to_string(),
            });
        } else if matches!(row.marker, Marker::Attention | Marker::Gate) {
            let reason = attention_by_id
                .get(row.id.as_str())
                .map(|a| a.reason.clone())
                .unwrap_or_else(|| format!("stalled in non-terminal state {}", row.state));
            ledger.push(LedgerEntry {
                task: row.id.clone(),
                from: row.state.clone(),
                to: "-".to_string(),
                driver: "blocked",
                invocation: "none".to_string(),
                reason,
            });
        } else if initial != Some(row.state.as_str()) {
            // Advanced to a new state without spawning a subprocess: callbacks,
            // transition rules, or already-present outputs carried it forward.
            ledger.push(LedgerEntry {
                task: row.id.clone(),
                from: initial.unwrap_or("").to_string(),
                to: row.state.clone(),
                driver: "callback-only",
                invocation: "none".to_string(),
                reason: "advanced without spawning work".to_string(),
            });
        } else if is_terminal_state(&row.state, machine) {
            ledger.push(LedgerEntry {
                task: row.id.clone(),
                from: row.state.clone(),
                to: "-".to_string(),
                driver: "terminal-at-start",
                invocation: "none".to_string(),
                reason: "already terminal".to_string(),
            });
        }
    }
    ledger
}

/// Collect spawned agents/programs for the Invocations section. §FS-rhei-run-report.7
fn build_invocations(
    records: &[LedgerRecord],
    workspace_root: &std::path::Path,
) -> Vec<InvocationRow> {
    records
        .iter()
        .map(|rec| InvocationRow {
            driver: rec.driver,
            task: rec.task.clone(),
            exit: match (&rec.outcome, rec.exit_code) {
                (LedgerOutcome::Cancelled, _) => "cancelled".to_string(),
                (LedgerOutcome::TimedOut, _) => "timed out".to_string(),
                (_, Some(code)) => format!("exit {code}"),
                (_, None) => "—".to_string(),
            },
            duration_ms: rec.duration_ms,
            log: relativize(&rec.log_path, workspace_root),
        })
        .collect()
}

fn format_work(agents: u32, programs: u32, callback_only: u32) -> String {
    let mut parts = vec![format!("{agents} agents"), format!("{programs} programs")];
    if callback_only > 0 {
        parts.push(format!("{callback_only} callback-only"));
    }
    parts.join(" · ")
}

fn marker_order(marker: Marker) -> u8 {
    match marker {
        Marker::Done => 0,
        Marker::Gate => 1,
        Marker::Attention => 2,
        Marker::Cancelled => 3,
        Marker::TerminalAtStart => 4,
    }
}

fn format_duration_short(ms: u64) -> String {
    if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}m{:02}s", ms / 60_000, (ms % 60_000) / 1000)
    }
}

fn format_duration_long(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

/// ANSI palette gated by a single `color` flag, so the renderer stays one code
/// path for both colored and plain output.
struct Palette {
    color: bool,
    reset: &'static str,
    bold: &'static str,
    dim: &'static str,
    red: &'static str,
}

impl Palette {
    fn new(color: bool) -> Self {
        Self {
            color,
            reset: if color { RESET } else { "" },
            bold: if color { BOLD } else { "" },
            dim: if color { DIM } else { "" },
            red: if color { RED } else { "" },
        }
    }

    fn color(&self, code: &'static str) -> &'static str {
        if self.color {
            code
        } else {
            ""
        }
    }

    fn colored(&self, code: &'static str, text: &str) -> String {
        if self.color {
            format!("{code}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    fn result_color(&self, result: &str) -> &'static str {
        if !self.color {
            return "";
        }
        if result.starts_with("stopped — ") {
            RED
        } else if result.starts_with("stopped") {
            YELLOW
        } else if result == "completed" {
            GREEN
        } else {
            ""
        }
    }
}

#[cfg(test)]
mod run_summary_tests {
    use super::*;

    fn machine() -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::builtin_default()
    }

    /// Parse a tiny plan whose tasks carry the given `(id, state)` pairs.
    fn report(tasks: &[(&str, &str)]) -> RunSummaryReport {
        let mut md = String::from("# Rhei: Test Plan\n\n## Tasks\n\n");
        for (id, state) in tasks {
            md.push_str(&format!("### Task {id}: Task {id}\n**State:** {state}\n\n"));
        }
        let rhei = rhei_core::parse(&md).expect("plan parses");
        RunSummaryReport::build(&rhei, &machine(), &SummarySink::new(), test_stats())
    }

    /// `RunStats` with non-zero spawn counts and empty run metadata, for the
    /// renderer tests that do not exercise the durable header.
    fn test_stats() -> RunStats {
        RunStats {
            agents_spawned: 2,
            programs_spawned: 3,
            callback_only: 0,
            duration: Some(std::time::Duration::from_secs(5)),
            dashboard: None,
            run_id: "abc123".to_string(),
            started_at: Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_749_115_351)),
            workspace_root: std::path::PathBuf::from("examples/test"),
            command: "rhei run .".to_string(),
            parallel: 4,
            mode: "agent",
            initial_states: HashMap::new(),
            dry_run: false,
        }
    }

    #[test]
    fn markers_classify_by_state_class() {
        let m = machine();
        assert_eq!(classify_marker("completed", &m), Marker::Done);
        assert_eq!(classify_marker("blocked", &m), Marker::Attention);
        assert_eq!(classify_marker("cancelled", &m), Marker::Cancelled);
    }

    #[test]
    fn plain_render_lists_every_task_with_state() {
        let r = report(&[("1", "completed"), ("2", "blocked")]);
        let out = r.render_tty(false);
        assert!(out.contains("Run Report"), "{out}");
        assert!(out.contains("Test Plan"), "{out}");
        assert!(out.contains("completed"), "{out}");
        assert!(out.contains("blocked"), "{out}");
        // No ANSI escapes when color is disabled.
        assert!(!out.contains('\x1b'), "{out}");
    }

    #[test]
    fn attention_block_surfaces_blocked_tasks() {
        let r = report(&[("1", "completed"), ("2", "blocked")]);
        let out = r.render_tty(false);
        assert!(out.contains("Attention"), "{out}");
        assert!(out.contains("1 blocked"), "{out}");
        assert!(out.contains("stopped for human attention"), "{out}");
    }

    #[test]
    fn all_completed_reads_as_completed() {
        let r = report(&[("1", "completed"), ("2", "completed")]);
        let out = r.render_tty(false);
        assert!(out.contains("completed"), "{out}");
        assert!(!out.contains("Attention"), "{out}");
    }

    #[test]
    fn color_render_emits_ansi() {
        let r = report(&[("1", "blocked")]);
        let out = r.render_tty(true);
        assert!(out.contains('\x1b'), "expected ANSI escapes");
    }

    #[test]
    fn duration_formats_short_and_long() {
        assert_eq!(format_duration_short(200), "0.2s");
        assert_eq!(format_duration_short(8_100), "8.1s");
        assert_eq!(format_duration_short(65_000), "1m05s");
        assert_eq!(format_duration_long(std::time::Duration::from_secs(724)), "12m04s");
    }

    /// Build a report from `(id, state)` pairs and a custom `RunStats`, used by
    /// the durable-report tests that vary spawn counts and initial states.
    fn report_with(tasks: &[(&str, &str)], stats: RunStats) -> RunSummaryReport {
        let mut md = String::from("# Rhei: Test Plan\n\n## Tasks\n\n");
        for (id, state) in tasks {
            md.push_str(&format!("### Task {id}: Task {id}\n**State:** {state}\n\n"));
        }
        let rhei = rhei_core::parse(&md).expect("plan parses");
        RunSummaryReport::build(&rhei, &machine(), &SummarySink::new(), stats)
    }

    #[test]
    fn markdown_report_has_all_sections() {
        let r = report(&[("1", "completed"), ("2", "blocked")]);
        let md = r.render_markdown();
        assert!(md.starts_with("# Run Report: Test Plan"), "{md}");
        assert!(md.contains("Run: 2025-"), "header carries the ISO start: {md}");
        assert!(md.contains("| Final states | Count |"), "{md}");
        assert!(md.contains("| Activity | Count |"), "{md}");
        assert!(md.contains("## Attention"), "{md}");
        assert!(md.contains("## Transition Ledger"), "{md}");
        assert!(md.contains("## Task Final States"), "{md}");
    }

    #[test]
    fn run_id_is_stable_for_a_given_start() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_nanos(1_749_115_351_123_456);
        assert_eq!(short_run_id(t), short_run_id(t));
        assert_eq!(short_run_id(t).len(), 6);
    }

    #[test]
    fn no_work_run_that_advanced_reads_differently() {
        // Every task ended completed, nothing spawned, and a task moved off its
        // non-terminal start — the report must not look like fast agent work.
        // §FS-rhei-run-report.3.3
        let mut initial = HashMap::new();
        initial.insert("1".to_string(), "queued".to_string());
        let stats = RunStats {
            agents_spawned: 0,
            programs_spawned: 0,
            callback_only: 1,
            initial_states: initial,
            ..test_stats()
        };
        let r = report_with(&[("1", "completed")], stats);
        assert_eq!(r.result, "completed — no work spawned");
        let md = r.render_markdown();
        assert!(md.contains("No agent or program ran"), "{md}");
        // The advance with no invocation is a callback-only ledger row.
        assert!(md.contains("| 1 | queued | completed | callback-only |"), "{md}");
    }

    #[test]
    fn terminal_at_start_task_is_marked_calm() {
        let mut initial = HashMap::new();
        initial.insert("done".to_string(), "completed".to_string());
        let stats = RunStats { initial_states: initial, ..test_stats() };
        let r = report_with(&[("done", "completed")], stats);
        assert_eq!(r.terminal_at_start, 1);
        let md = r.render_markdown();
        assert!(md.contains("terminal at start"), "{md}");
        // It is a terminal-at-start ledger row, not an invocation.
        assert!(md.contains("| done | completed | - | terminal-at-start |"), "{md}");
    }

    #[test]
    fn write_to_runtime_emits_latest_and_history() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        let stats =
            RunStats { workspace_root: dir.path().to_path_buf(), ..test_stats() };
        let mut r = report_with(&[("1", "completed")], stats);
        r.write_to_runtime(&runtime).expect("write report");
        assert!(runtime.join("run-report.md").exists());
        assert_eq!(r.report_path.as_deref(), Some("runtime/run-report.md"));
        let history = std::fs::read_dir(runtime.join("run-reports"))
            .expect("history dir")
            .filter_map(Result::ok)
            .count();
        assert_eq!(history, 1, "one timestamped history entry written");
    }

    #[test]
    fn dry_run_result_reads_as_preview() {
        let stats = RunStats { dry_run: true, ..test_stats() };
        let r = report_with(&[("1", "completed")], stats);
        assert_eq!(r.result, "dry run — no changes applied");
        assert!(r.render_markdown().contains("Result: dry run — no changes applied"));
    }

    #[test]
    fn dashboard_pointer_gated_on_enabled_this_run() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let runtime = dir.path().join("runtime");
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::write(runtime.join("dashboard.html"), "<html>").unwrap();
        // A stale dashboard from an earlier run must not be linked when the
        // dashboard was off this run.
        assert_eq!(frozen_dashboard_relative_path(false, &runtime, dir.path()), None);
        assert_eq!(
            frozen_dashboard_relative_path(true, &runtime, dir.path()).as_deref(),
            Some("runtime/dashboard.html"),
        );
    }

    #[test]
    fn md_cell_escapes_pipes_and_newlines() {
        assert_eq!(md_cell("a|b"), "a\\|b");
        assert_eq!(md_cell("line1\nline2"), "line1 line2");
    }

    /// A `SummarySink` carrying one spawned transition `from`→`to`.
    fn summary_with_spawn(task: &str, from: &str, to: &str, agent: bool) -> SummarySink {
        use rhei_tui::EventSink;
        let s = SummarySink::new();
        let log = std::path::PathBuf::from("runtime/logs/x.log");
        s.emit(rhei_tui::RunEvent::SlotAssigned {
            slot: 0,
            task: task.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            agent: agent.then(|| "mock".to_string()),
            template_context: None,
            log_path: log.clone(),
            started_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
        });
        s.emit(rhei_tui::RunEvent::SlotReleased {
            slot: 0,
            task: task.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            log_path: log,
            outcome: rhei_tui::TaskOutcome::Completed,
            finished_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
            exit_code: Some(0),
            duration_ms: 1_200,
        });
        s
    }

    #[test]
    fn ledger_records_trailing_callback_advance_after_spawn() {
        // An agent ran build->review, then a callback carried review->completed
        // with no further spawn. The ledger must reach the final state.
        let summary = summary_with_spawn("1", "build", "review", true);
        let stats = RunStats { initial_states: HashMap::new(), ..test_stats() };
        let mut md = String::from("# Rhei: Test Plan\n\n## Tasks\n\n");
        md.push_str("### Task 1: Task 1\n**State:** completed\n\n");
        let rhei = rhei_core::parse(&md).expect("plan parses");
        let report = RunSummaryReport::build(&rhei, &machine(), &summary, stats);
        let md = report.render_markdown();
        // The spawned agent row and the synthesized callback advance both appear.
        assert!(md.contains("| 1 | build | review | agent |"), "{md}");
        assert!(md.contains("| 1 | review | completed | callback-only |"), "{md}");
    }
}
