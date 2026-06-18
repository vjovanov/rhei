# FS-rhei-run-tui: `rhei run` TUI and Run Event Journal

This document specifies a live visualization layer for parallel agent execution under `rhei run` and the persistent run-event journal that backs it. The design extracts a reusable frontend crate (`rhei-tui`) that can be driven by any parallel `rhei` subcommand — not only `rhei run` — and preserves the current plain-stdout behavior for non-interactive use.

For the surrounding `rhei run` behavior see [Rhei Usage](rhei-usage.spec.md) and [Agents Specification](rhei-agents.spec.md).

## Goals

1. **Navigate the whole plan, lead with live work.** When `rhei run` is running in an interactive terminal, the user sees the whole plan as a navigable list and can select any task — running or not — to inspect its surroundings, while live work is foregrounded. Each live task shows its current state, elapsed time, captured agent output, and cost. The terminal surface mirrors the browser Flow view (§FS-rhei-viz) under one visual language (§FS-rhei-viz-ux).
2. **Keep a light run-event log.** Each task slot assignment/release produces one line in a persistent journal. State-transition history is centralized separately in `runtime/state-transitions.log` and is surfaced in the Flow surroundings inspector. §FS-rhei-viz.4
3. **Remain CI-friendly.** When stdout is not a TTY (piped, redirected, CI runners), `rhei run` produces the same line-oriented output as today. The journal is written identically in both modes.
4. **Be reusable.** Any future parallel `rhei` subcommand reuses the same event surface and frontend.

## Non-Goals

- Replacing the current plain-stdout mode. The TUI is an additional view, not a replacement.
- Streaming agent stdout to a central log aggregator. Agents continue to write per-task log files; the TUI tails those files.
- Remote visualization. The TUI renders to the local terminal only.

## 1. Architecture

A single `rhei run` process decomposes into three concerns:

1. **Execution engine** — the existing `run_agent_mode` / `run_callback_mode` logic, refactored to emit events through an `EventSink` instead of calling `println!` directly.
2. **Sinks** — implementations of `EventSink` that consume events. The engine always writes through a `Tee` that fans out to a journal sink and a frontend sink.
3. **Frontend** — either a plain stdout writer (non-TTY) or a TUI renderer (TTY). Frontend selection is decided once at startup based on `stdout.is_terminal()`, with `--tui` and `--no-tui` overrides. The TUI renderer is the terminal sibling of the browser dashboard: both render one run model (§FS-rhei-viz.8) — plan rows, the resolved machine, and the runtime overlay — so either surface is recognizable from the other (§FS-rhei-viz-ux).

```
engine ──► Tee ──┬──► JournalSink   (runtime/transitions.log, always on)
                 └──► FrontendSink  (TuiSink if TTY, else StdoutSink)
```

Slot-oriented events (see below) mean the renderer updates exactly one slot per event. The engine assigns a `Slot` when it spawns an agent or program and releases it when that invocation exits. `Slot` is a `u16`, not a byte-sized value, so very large `--parallel` values cannot silently collide after slot 255.

### 1.1. Event Surface

```rust
// crates/rhei-tui/src/event.rs
pub type Slot = u16;

pub enum TaskOutcome {
    Completed,
    Failed(String),
    Cancelled,
    TimedOut,
}

pub struct RunSummary {
    pub agents_spawned: u32,
    pub programs_spawned: u32,
    pub terminal_tasks: usize,
    pub total_tasks: usize,
    pub accounting: Option<AccountingRunSummary>,
}

pub enum MessageLevel {
    Info,
    Warn,
    Error,
}

pub enum AgentStream {
    Stdout,
    Stderr,
}

pub enum DimensionStatus {
    Measured,
    Partial,
    Unsupported,
    Omitted,
    Unknown,
}

pub struct DimensionSummary {
    pub value: Option<u64>,
    pub status: DimensionStatus,
    pub missing_count: u64,
    pub measured_count: u64,
}

pub enum UsageCoverage {
    Complete,
    Partial,
    Unpriced,
    None,
}

pub enum UsageStatus {
    Measured,
    UnsupportedAgent,
    ExtractorUnavailable,
    ExtractorFailed,
    NoUsageEmitted,
}

pub enum PricingStatus {
    Priced,
    PartialPrice,
    Unpriced,
    NotApplicable,
}

pub struct UsageSummary {
    pub invocation_id: String,
    pub state: String,
    pub agent: String,
    pub provider: Option<String>,
    pub model: Option<String>,
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
    SlotAssigned {
        slot: Slot,
        task: String,
        from: String,
        to: String,
        agent: Option<String>,
        log_path: PathBuf,
        started_at: Instant,
        wall_clock: SystemTime,
    },
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
    UsageReported {
        slot: Option<Slot>,
        task: String,
        invocation_id: String,
        usage: UsageSummary,
    },
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: RunEvent);
}
```

`RunStarted` is emitted once per run with the workspace root, resolved
parallelism, and total task count. The total task count includes child and
grandchild task nodes, not only root tasks. `PassStarted` and `PassEnded`
bracket each scheduler pass; `PassStarted.ready` is the current ready set in
source-order task ids.

`SlotAssigned` is emitted at spawn time; `SlotReleased` is emitted when the spawned agent or program exits. Both events carry the slot index so the renderer can update the right slot without reconciliation. Both events also carry `from` and `to`: when `from == to`, the worker started or ended in the same autonomous state and renderers must not present that as a real self-transition.

`SlotAssigned.agent` identifies the resolved agent or target label when the invocation is agent-backed; it is `None` for program-backed work. `SlotReleased.exit_code` is the subprocess exit status when one is available, and `duration_ms` is the invocation duration in milliseconds.

`AgentOutput` is emitted for live agent subprocess traffic after the slot is assigned and before it is released. The event is line-oriented and identifies stdout vs stderr with `AgentStream`. Lines are ordered per stream; interleaving between stdout and stderr is best-effort because the two streams are read concurrently. The per-task log file remains the complete durable transcript.

`UsageReported` is emitted after a `runtime/accounting/invocations/` record is
durably written. It may arrive after `SlotReleased`; renderers update the
matching task, slot history, and run totals without assuming the slot is still
active. §FS-rhei-cost-accounting

`TasksDeferred` is emitted when tasks were ready in the current pass but not scheduled because another task in the same non-`concurrent` state consumed the available same-state slot. Deferred tasks remain eligible for later passes.

`Message` carries human-oriented engine diagnostics with `info`, `warn`, or `error` severity. `RunLink` carries URLs or file links produced by the run process, such as dashboard links or callback-emitted artifacts. Terminal frontends render messages in the Journal view and links in the shared links strip; neither represents a task state change.

`RunFinished` is emitted once with aggregate counts for spawned agents, spawned programs, terminal tasks, total tasks, and accounting totals when available.

`Tee` is a composite sink implementing `EventSink` by forwarding each event to a fixed list of inner sinks.

### 1.2. Live Agent Traffic

`rhei run` intercepts stdout and stderr for built-in autonomous agents through a shared subprocess capture path:

| Agent id | Prompt transport | Output capture |
|----------|------------------|----------------|
| `claude-code` | `-p <prompt>` | stdout/stderr are piped, logged, and emitted as `AgentOutput` |
| `codex` | stdin, followed by `--` separator | stdout/stderr are piped, logged, and emitted as `AgentOutput` |
| `pi` | `-p <prompt>` | stdout/stderr are piped, logged, and emitted as `AgentOutput` |

Agent-specific behavior belongs only to command construction and prompt delivery. Traffic interception is transport-agnostic once the child process has been spawned.

The TUI keeps a bounded recent traffic buffer per active slot and may drop display events if the render channel is full. Dropped display events do not affect `runtime/logs/*`: the log writer remains the durable sink and receives every captured line unless the filesystem write itself fails. Long or control-sequence-heavy lines may be sanitized and truncated for rendering, but the log preserves the raw bytes captured from the subprocess stream.

### 1.3. Sink Implementations

- **`JournalSink`** — opens `runtime/transitions.log` in append mode at construction and writes one line per `SlotAssigned` and one line per `SlotReleased`. Line format is fixed-column and tail-friendly (see below). The journal is always written, in every mode. State transitions themselves are recorded by command paths in `runtime/state-transitions.log`. §FS-rhei-viz.4
- **`StdoutSink`** — reproduces the current `println!` output exactly. It is the default frontend when stdout is not a TTY.
- **`TuiSink`** — owns a bounded `crossbeam_channel` and a render thread. It implements `EventSink` by pushing events onto the channel; the render thread consumes events and updates the UI. The render thread maintains the shared run model — plan rows and the resolved machine supplied by the host, overlaid with runtime state from the event stream — and draws the Flow surface defined in §1.5.

### 1.4. Frontend Selection

At the entry of `run_plan`, the frontend is decided once:

| Condition                                         | Frontend   |
|--------------------------------------------------|------------|
| `--no-tui`, or `stdout` is not a TTY              | StdoutSink |
| `--tui`, regardless of TTY detection              | TuiSink    |
| Default: `stdout.is_terminal()` is true            | TuiSink    |

Auto-detection uses `std::io::IsTerminal`. The `--tui` override exists for edge cases where detection is wrong (nested shells, certain tmux configurations). The `--no-tui` override is for scripted demos and debugging.

### 1.5. TUI Surface

The TUI is the terminal renderer of the same Flow model the browser dashboard
serves (§FS-rhei-viz.8): plan task rows, the resolved state machine, and the
runtime overlay. It is a tabbed, keyboard-first console surface whose default
view leads with live work while letting the operator select and inspect *any*
task, running or not. The TUI and the dashboard are two renderers of one model
under one visual language, so recognition transfers between them
(§FS-rhei-viz-ux). This section defines the terminal realization; the view
*content* is defined once in §FS-rhei-viz and is not repeated here.

The terminal surface diverges from the browser in three deliberate ways:

- **No dependency-graph (DAG) mode.** The prerequisite graph (§FS-rhei-viz.3) is
  not drawn in the terminal; per-task prerequisites remain visible in the
  inspector (§FS-rhei-viz.4).
- **The state machine renders as a grouped list, not a drawn graph.** The Machine
  view presents the resolved machine (§FS-rhei-viz.6) as a state list grouped by
  disjoint workflow, with a state-detail panel, rather than a layered graph.
- **Running-now and per-slot worker output fold into Flow.** The browser's
  running-now panel and Slots surface (§FS-rhei-viz.5) are not separate terminal
  views; live workers are marked in the plan list, agent processes use the live
  spinner, program processes use a yellow dot, and captured output appears in
  the selected task's inspector.

#### 1.5.1. Shell and shared chrome

Every view shares one frame: a header, tab bar, active body, persistent links
strip, and action bar. The header shows the plan title, derived `plan_state`,
category counts, running count, compact run cost when usage exists, and the live
run status (§FS-rhei-viz.1.2 §FS-rhei-viz.9 §FS-rhei-cost-accounting). The tab
bar exposes the terminal views, the links strip shows the dashboard URL,
workspace, and run-emitted links, and the action bar shows only keys that
currently apply. Run-event lines are shown only in the dedicated Journal view.

A single selected task is shared across views: Flow and Cost move it, Machine
marks its current state, and Cost highlights its rollup. State category, glyph,
and color come from the same map as scrollback and the browser
(§FS-rhei-viz.1.1, §FS-rhei-viz-ux.3.2).

#### 1.5.2. Navigation, selection, and keys

The surface is keyboard-driven and does not capture the mouse, so terminal text
selection still works on ids, paths, and journal lines (§FS-rhei-viz-ux.7).
Selection is two-level: the selected task is global, while local focus belongs to
the active view. The selected task is tracked by id and survives refreshes and
reordering without scroll jumps (§FS-rhei-viz-ux.4).

Flow has local focus for the outline or inspector. Inspector focus lands on
section headers first so small terminals can scroll by `depends on / unblocks`,
`state history`, `next states`, `prompt`, live agent, artifacts, and children.
`Enter` opens a section's items; opening `prompt` gives the prompt the full
surroundings pane until `Esc` returns to section headers. `Enter` on a navigable
item selects a neighbor task or marks a target state in Machine, matching the
surroundings model of §FS-rhei-viz.4.

| Key | Action |
| --- | --- |
| `j` / `k`, `↓` / `↑` | move focus down / up in the active view |
| `1`–`4` | jump to Flow, Machine, Cost, Journal |
| `h` / `l`, `←` / `→` | previous / next view |
| `Tab` | (Flow) toggle focus between outline and inspector |
| `PgUp` / `PgDn` | scroll the focused pane |
| `Enter` | (inspector header) open its items; (inspector item) activate it; (Flow outline on gating task) open gate choices |
| `Esc` | close an open inspector section or clear the active modal/filter |
| `/` | filter the active view (`Flow`/task-cost rows by id, title, or state; `Machine` by state/task text; `Journal` by line text); `Esc` clears |
| `g` | (Cost) cycle grouping: task → agent → model → state |
| `f` | (Journal) cycle severity/kind filter |
| `m` | message the selected running agent when an intervention channel is available (§1.5.5) |
| `?` | toggle the key-help overlay |
| `q` | quit once the run has finished; during a live run, stop with `Ctrl+C` |
| `Ctrl+C` | restore the terminal and re-raise `SIGINT` (§1.8) |

#### 1.5.3. Flow view (default)

Flow is the default view. It renders the plan outline and the selected task's
surroundings inspector using the browser Flow content order (§FS-rhei-viz.2
§FS-rhei-viz.4). Running tasks are marked live even if their persisted state is
idle: agent processes use the animated live marker, while program processes use
a static yellow dot. The selected live task shows captured output, elapsed time,
and latest usage/cost in the inspector (§FS-rhei-viz.5 §FS-rhei-cost-accounting).

On load, the TUI auto-selects the first running task, then the first
state-derived active task, then the first task. The only animated element is the
live spinner, which becomes static under reduced motion (§FS-rhei-viz-ux.4).

#### 1.5.4. Machine, Cost, and Journal views

Beyond Flow, the tab bar offers three compact views over the same model:

- **Machine** — grouped state list plus the focused state's details; the selected
  task's current state is labeled separately from the keyboard focus row, and
  authored agent/program process kind colors the state glyph and state name
  directly. A global Machine legend explains focus, selected-task, process-kind,
  and state-category markers (§FS-rhei-viz.6).
- **Cost** — run totals and grouped rollups by task, agent, model, or state;
  coverage gaps carry a glyph, never color alone (§FS-rhei-cost-accounting).
- **Journal** — full run-event journal with severity and text filtering (§1.7).
  Links remain in the shared links strip rather than consuming Journal body
  space.

#### 1.5.5. Live actions: intervene and human gate

The TUI exposes the dashboard's live actions through the same sinks as the
browser, not separate mutation paths (§FS-rhei-viz.5 §AR-rhei-viz-flow.7).

- **Intervene** — `m` opens a one-line composer only for a selected live task
  whose agent is reachable through the intervention sink. `Enter` sends, `Esc`
  cancels, and delivery/failure is echoed in the journal. Intervene never edits
  or transitions the plan.
- **Human gate** — `Enter` on a selected live task in a `gating` state opens the
  state's explicit outgoing transitions as digit choices and submits the selected
  `from`/`to` transition through the gate sink. Frozen/static surfaces offer no
  working controls. Interactive runs stay alive for non-terminal human gates
  when the remaining work is gate-blocked or poll-blocked; a pending gate remains
  responsive instead of being hidden behind a later poll deadline.

#### 1.5.6. Responsive degradation

Layout is recomputed on resize and degrades from side-by-side Flow panes, to
stacked panes, to a compact one-line task list when the terminal is too small
(§FS-rhei-viz-ux.8). Empty views render quiet monochrome placeholders, never a
blank panel or crash (§FS-rhei-viz-ux.7).

#### 1.5.7. Liveness, color, and lifecycle

The render thread redraws on a periodic tick so elapsed timers, output, and
counts advance without input, and a failed plan reload keeps the last-good model
visible (§FS-rhei-viz-ux.4 §FS-rhei-viz.7.1). `NO_COLOR` makes chrome and state
markers monochrome and also selects reduced motion; meaning always rides on
glyphs and labels, not color alone (§FS-rhei-viz-ux.3.3).

Interactive TUI runs stay live for a pending human gate only when gates, or work
blocked by those gates or future poll deadlines, are the remaining blockers. The
operator can resolve the gate in the UI or stop with `Ctrl+C`. Non-interactive
runs do not wait. After `RunFinished`, live actions are disabled but the final
surface remains navigable until `q`; non-TTY and `--no-tui` output remains
line-oriented (§1.4, §3).

### 1.6. Browser Dashboard

When the TUI frontend is selected, `rhei run` also serves the loopback browser
dashboard unless `--no-dashboard` is set. `--dashboard` force-enables the
dashboard outside TUI mode, while `--no-dashboard` disables it. The dashboard is
a power-user view for both live execution monitoring and static plan-shape
inspection. §FS-rhei-viz

When the live dashboard is available, the TUI header keeps the dashboard URL
visible at the top of the screen so users do not have to find it in the
scrolling journal.

The dashboard's primary surface is the **Flow view**: a single page that leads
with the work running now, presents plan shape as a navigable list or dependency
graph, opens any node's surroundings (dependencies, transitions, prompt,
artifacts, children), and draws the resolved state machine as one graph per
disjoint workflow. A live task exposes its streaming agent output and a way to
intervene. §FS-rhei-viz §FS-rhei-viz.5 The TUI renders this same Flow surface and
a terminal-appropriate subset of these views (§1.5).

Supplementary surfaces share the same `/snapshot` data and console-first
language:

- **Gantt / Cube / Sankey** — dense chart overviews for scanning many nodes at
  once. §FS-rhei-viz.12
- **Tasks** — all tasks with state, assignee, dependencies, readiness, and
  current worker slot.
- **Slots** — worker cards and live captured output.
- **Cost** — run, task, subtree, invocation, agent, provider, model, and state
  accounting views. §FS-rhei-cost-accounting
- **Journal** — recent run events.
- **Links** — workspace shortcuts and run-emitted links.

The dashboard obtains plan data from the lazily reloaded `/snapshot` payload.
That payload includes `plan_state`, derived from top-level task states only, and
the flattened task rows used by all dashboard views. The dashboard remains
self-contained: no external scripts, stylesheets, fonts, or network assets.
Compact accounting rollups live in `/snapshot`; invocation-level accounting
detail is served from a separate loopback dashboard endpoint so polling remains
lightweight. §FS-rhei-cost-accounting

### 1.7. Journal Format

`runtime/transitions.log` is a UTF-8, append-only, newline-delimited text file. Each line is one run event. Columns are space-separated; columns 1–3 are fixed-width, column 4 is a path, and optional trailing fields are comma-separated key=value pairs.

```
2026-04-21T14:03:22Z  task-042  start@pending           runtime/logs/task-042-pending.log
2026-04-21T14:07:11Z  task-042  end@pending             runtime/logs/task-042-pending.log  exit=0,duration=3m49s,outcome=completed
```

Rules:
- Timestamps are UTC, RFC 3339, second precision.
- The event column uses `start@<state>` for `SlotAssigned` and `end@<state>` for `SlotReleased`.
- Paths are workspace-relative if inside the workspace, otherwise absolute.
- Trailing metadata is only added on `SlotReleased` events (`exit`, `duration`, `outcome`).
- The file is safe to `tail -f` from other shells while `rhei run` is active.

A `SlotAssigned` produces one line; its paired `SlotReleased` produces a second line on the same state (recording exit status and duration). For multi-invocation states (`all_targets`), each invocation is a distinct pair of lines with the target suffix visible in the log path.

### 1.8. Failure Modes

- **Panic in the execution engine** — a panic hook registered by `TuiSink` calls `ratatui::restore()` before re-raising, so the terminal is never left in raw mode.
- **Ctrl+C** — because the TUI runs the terminal in raw mode, Ctrl+C arrives as a key event rather than an automatic `SIGINT`. `TuiSink` restores the terminal, explicitly re-raises `SIGINT` for the process, and then exits its render loop.
- **Terminal too small for two panes** — auto-degrade to the compact list of §1.5.6; never crash.
- **Slow log file growth** — the log tailer uses a bounded 50-line ring buffer and never blocks the engine thread.
- **Journal write failure** — log a warning to stderr and continue; journal errors never abort a run.

### 1.9. Reuse

`rhei-tui` is a standalone crate with no dependency on `rhei-cli`. Any future subcommand that fans out to a worker pool constructs:

```rust
rhei_tui::run_with_frontend(
    engine_fn,
    RunParams { parallel, workspace_root, total_tasks },
);
```

and receives an `EventSink` it writes to. The frontend choice (TUI vs stdout) and the journal are handled by the helper. `rhei-cli` only depends on `rhei-tui` for the event types and the helper; it does not see `ratatui` or `crossterm` directly.

## 2. CLI Changes

Two new flags on `rhei run`:

| Flag        | Description                                                              |
|-------------|--------------------------------------------------------------------------|
| `--tui`     | Force TUI mode even when stdout is not detected as a TTY.                |
| `--no-tui`  | Force plain stdout output even when stdout is a TTY.                     |

The two flags are mutually exclusive. When neither is given, the frontend is auto-selected from `IsTerminal`.

Existing flags (`--parallel`, `--dry-run`, `--continue-on-error`, `--agent`, etc.) retain their current semantics.

## 3. Backward Compatibility

- Without `--tui` and in any non-TTY context, output matches the current line-oriented format byte-for-byte. Existing integration tests that grep stdout continue to pass.
- The journal file is new. Runs that never produced a journal before will now produce one at `runtime/transitions.log`. This file is additive and does not alter plan state.
- No existing flags change meaning.
- The interactive TUI now stays open after the run finishes until the operator
  quits (§1.5.7); non-TTY and `--no-tui` runs still return when the run ends.

## 4. Implementation Surface

The engine refactor replaces direct `println!` sites in `run_agent_mode` (currently around `crates/rhei-cli/src/main.rs` lines 5007–5700) with `sink.emit(...)` calls. The call sites themselves do not change otherwise; the loop structure, task resolution, and spawn logic are preserved. The behavior of `StdoutSink` is defined to match today's formatted output, so this refactor is observably a no-op for all non-TTY users.

## 5. Dependencies

The new `rhei-tui` crate adds:

- `ratatui` — TUI widgets and layout.
- `crossterm` — terminal backend, input handling.
- `crossbeam-channel` — event channel between engine and render thread.

All three are pure Rust with no C dependencies. `notify` is already a workspace dependency and is reused for log tailing.

## Related Specifications

- [Console-First Visualization UX](rhei-viz-ux.spec.md) — the shared look-and-feel
  this TUI and the browser dashboard both follow. §FS-rhei-viz-ux
- [Rhei Usage](rhei-usage.spec.md) — `rhei run` execution modes and roles.
- [Agents Specification](rhei-agents.spec.md) — agent log capture and `runtime/logs/` layout.
- [Program States Specification](rhei-programs.spec.md) — program execution and exit-code transitions.
