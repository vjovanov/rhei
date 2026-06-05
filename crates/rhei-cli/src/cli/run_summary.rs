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

/// An `EventSink` recording per-task driver/duration for the console summary's
/// task tree; teed alongside the journal/frontend sinks and read after the run.
/// §FS-rhei-run-report.3.2 §FS-rhei-run-report.8
pub struct SummarySink {
    inner: Mutex<SummaryState>,
}

#[derive(Default)]
struct SummaryState {
    /// Driver of each in-flight slot, keyed by slot index, set on `SlotAssigned`.
    inflight: HashMap<u16, &'static str>,
    /// Finalized per-task activity, keyed by task id.
    tasks: HashMap<String, TaskActivity>,
}

impl SummarySink {
    pub fn new() -> Self {
        Self { inner: Mutex::new(SummaryState::default()) }
    }

    /// Snapshot the accumulated activity for rendering after the run.
    fn snapshot(&self) -> HashMap<String, TaskActivity> {
        self.inner.lock().expect("summary sink poisoned").tasks.clone()
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
            rhei_tui::RunEvent::SlotReleased { slot, task, duration_ms, .. } => {
                let driver = state.inflight.remove(&slot).unwrap_or("program");
                let entry = state.tasks.entry(task).or_default();
                entry.driver = Some(driver);
                entry.invocations += 1;
                entry.last_duration_ms = duration_ms;
            }
            _ => {}
        }
    }
}

/// Print the end-of-run console summary to stdout. Best-effort: a non-interactive
/// stdout or plan-load failure skips it, leaving the run's line-oriented output
/// as the non-TTY record. §FS-rhei-run-report.3 §FS-rhei-run-report.3.4
fn print_run_summary(
    input: &std::path::Path,
    machine: &rhei_validator::StateMachine,
    summary: &SummarySink,
    agents_spawned: u32,
    programs_spawned: u32,
    callback_only: u32,
    duration: std::time::Duration,
) {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return;
    }
    let Ok(loaded) = load_plan(input) else {
        return;
    };
    let report = RunSummaryReport::build(
        &loaded.rhei,
        machine,
        summary,
        RunStats {
            agents_spawned,
            programs_spawned,
            callback_only,
            duration: Some(duration),
            dashboard: None,
        },
    );
    // Honor NO_COLOR for users who disable ANSI globally.
    let color = std::env::var_os("NO_COLOR").is_none();
    print!("{}", report.render_tty(color));
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
}

impl Marker {
    fn glyph(self) -> char {
        match self {
            Marker::Done => '✓',
            Marker::Gate => '⏸',
            Marker::Attention => '!',
            Marker::Cancelled => '⊘',
        }
    }

    /// ANSI color for this marker class. Only `Attention` and `Gate` are
    /// saturated; success and cancelled rows stay calm. §FS-rhei-viz-ux.3
    fn color(self) -> &'static str {
        match self {
            Marker::Done => GREEN,
            Marker::Gate => YELLOW,
            Marker::Attention => RED,
            Marker::Cancelled => DIM,
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
}

/// The fully resolved console summary, ready to render.
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

        let total_tasks = rows.len();

        // Counts in canonical order: success, gate, attention, cancelled.
        let mut state_counts: Vec<(String, usize, Marker)> =
            counts.into_iter().map(|(state, (n, marker))| (state, n, marker)).collect();
        state_counts.sort_by_key(|(_, _, marker)| marker_order(*marker));

        let result = result_phrase(&attention, &rows);
        let work = format_work(stats.agents_spawned, stats.programs_spawned, stats.callback_only);

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

        // Pointers.
        if let Some(dashboard) = &self.dashboard {
            out.push_str(&format!("\nDashboard  {dashboard}\n"));
        }
        // Drop trailing spaces left by empty detail columns; keep the final newline.
        let trailing_newline = out.ends_with('\n');
        let mut trimmed = out.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
        if trailing_newline {
            trimmed.push('\n');
        }
        trimmed
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

fn result_phrase(attention: &[AttentionRow], rows: &[TaskRow]) -> String {
    if !attention.is_empty() {
        // Gated and blocked tasks both halt the run for a human; the report and
        // tree carry the per-task distinction. §FS-rhei-run-report.6
        "stopped for human attention".to_string()
    } else if rows.iter().all(|r| r.marker == Marker::Done) {
        "completed".to_string()
    } else {
        "finished".to_string()
    }
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
        RunSummaryReport::build(
            &rhei,
            &machine(),
            &SummarySink::new(),
            RunStats {
                agents_spawned: 2,
                programs_spawned: 3,
                callback_only: 0,
                duration: Some(std::time::Duration::from_secs(5)),
                dashboard: None,
            },
        )
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
}
