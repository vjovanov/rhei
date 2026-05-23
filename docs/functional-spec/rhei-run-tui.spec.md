# FS-rhei-run-tui: `rhei run` TUI and Transition Journal

This document specifies a live visualization layer for parallel agent execution under `rhei run` and the persistent transition journal that backs it. The design extracts a reusable frontend crate (`rhei-tui`) that can be driven by any parallel `rhei` subcommand — not only `rhei run` — and preserves the current plain-stdout behavior for non-interactive use.

For the surrounding `rhei run` behavior see [Rhei Usage](rhei-usage.spec.md) and [Agents Specification](rhei-agents.spec.md).

## Goals

1. **Visualize parallel agent activity.** When `rhei run --parallel N` is running in an interactive terminal, the user sees a live view of up to N agents, each with its task id, current state, elapsed time, and a short tail of its log.
2. **Keep a light transition log.** Each state transition produces exactly one line in a persistent journal. Every line carries both the transition (`from → to`) and the absolute path to the detailed log for that state.
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
3. **Frontend** — either a plain stdout writer (non-TTY) or a TUI renderer (TTY). Frontend selection is decided once at startup based on `stdout.is_terminal()`, with `--tui` and `--no-tui` overrides.

```
engine ──► Tee ──┬──► JournalSink   (runtime/transitions.log, always on)
                 └──► FrontendSink  (TuiSink if TTY, else StdoutSink)
```

Slot-oriented events (see below) mean the renderer updates exactly one tile per event. The engine assigns a `Slot` when it spawns an agent or program and releases it when that invocation exits. `Slot` is a `u16`, not a byte-sized value, so very large `--parallel` values cannot silently collide after slot 255.

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

`SlotAssigned` is emitted at spawn time; `SlotReleased` is emitted when the spawned agent or program exits. Both events carry the slot index so the renderer can update the right tile without reconciliation. Both events also carry `from` and `to`: when `from == to`, the worker started or ended in the same autonomous state and renderers must not present that as a real self-transition.

`SlotAssigned.agent` identifies the resolved agent or target label when the invocation is agent-backed; it is `None` for program-backed work. `SlotReleased.exit_code` is the subprocess exit status when one is available, and `duration_ms` is the invocation duration in milliseconds.

`AgentOutput` is emitted for live agent subprocess traffic after the slot is assigned and before it is released. The event is line-oriented and identifies stdout vs stderr with `AgentStream`. Lines are ordered per stream; interleaving between stdout and stderr is best-effort because the two streams are read concurrently. The per-task log file remains the complete durable transcript.

`UsageReported` is emitted after a `runtime/accounting/invocations/` record is
durably written. It may arrive after `SlotReleased`; renderers update the
matching task, slot history, and run totals without assuming the slot is still
active. §FS-rhei-cost-accounting

`TasksDeferred` is emitted when tasks were ready in the current pass but not scheduled because another task in the same non-`concurrent` state consumed the available same-state slot. Deferred tasks remain eligible for later passes.

`Message` carries human-oriented engine diagnostics with `info`, `warn`, or `error` severity. `RunLink` carries URLs or file links produced by the run process, such as dashboard links or callback-emitted artifacts. Frontends may render both in a journal pane; they do not represent task state changes.

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

- **`JournalSink`** — opens `runtime/transitions.log` in append mode at construction and writes one line per `SlotAssigned` and one line per `SlotReleased`. Line format is fixed-column and tail-friendly (see below). The journal is always written, in every mode.
- **`StdoutSink`** — reproduces the current `println!` output exactly. It is the default frontend when stdout is not a TTY.
- **`TuiSink`** — owns a bounded `crossbeam_channel` and a render thread. It implements `EventSink` by pushing events onto the channel; the render thread consumes events and updates the UI.

### 1.4. Frontend Selection

At the entry of `run_plan`, the frontend is decided once:

| Condition                                         | Frontend   |
|--------------------------------------------------|------------|
| `--no-tui`, or `stdout` is not a TTY              | StdoutSink |
| `--tui`, regardless of TTY detection              | TuiSink    |
| Default: `stdout.is_terminal()` is true            | TuiSink    |

Auto-detection uses `std::io::IsTerminal`. The `--tui` override exists for edge cases where detection is wrong (nested shells, certain tmux configurations). The `--no-tui` override is for scripted demos and debugging.

### 1.5. Layout Rules (TuiSink)

The renderer allocates a fixed pool of N slots matching `--parallel N`. Slots are reused as tasks complete — the grid does not grow unbounded.

| N    | Terminal constraint                | Layout                                    |
|------|------------------------------------|-------------------------------------------|
| 1    | any                                | Single full-width pane with log tail      |
| 2–4  | rows-per-tile ≥ 6                  | 2×2 grid with log tail                    |
| 5–9  | rows-per-tile ≥ 6                  | 3×3 grid, shorter tiles, log tail         |
| any  | rows-per-tile < 6                  | Compact list: one row per slot, no tail   |
| ≥ 10 | any                                | Compact list mode                         |

A persistent journal pane at the bottom shows the most recent transitions regardless of mode. Layout is recomputed on terminal resize (`crossterm::event::Event::Resize`).

Each tile shows:
- task id + short title
- current state (the `to` field of its `SlotAssigned` event)
- elapsed time (updated once per second)
- latest known invocation cost, input/output tokens, cached tokens, and
  accounting coverage when available
- last 5 lines of the log file at `log_path`, tailed via the `notify` crate with a bounded 50-line ring buffer

Idle slots show `— idle —`.

When any accounting data is available, the TUI header includes a compact
run-level cost strip with total cost, input tokens, output tokens, cache-hit
ratio, and accounting coverage. §FS-rhei-cost-accounting

### 1.6. Browser Dashboard

When the TUI frontend is selected, `rhei run` also serves the loopback browser
dashboard unless `--no-dashboard` is set. `--dashboard` force-enables the
dashboard outside TUI mode, while `--no-dashboard` disables it. The dashboard is
a power-user view for both live execution monitoring and static plan-shape
inspection. §FS-rhei-viz

The dashboard's primary surface is the **Flow view**: a single page that leads
with the work running now, presents plan shape as a navigable list or dependency
graph, opens any node's surroundings (dependencies, transitions, prompt,
artifacts, children), and draws the resolved state machine as one graph per
disjoint workflow. A live task exposes its streaming agent output and a way to
intervene. §FS-rhei-viz §FS-rhei-viz.5

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

`runtime/transitions.log` is a UTF-8, append-only, newline-delimited text file. Each line is one event. Columns are space-separated; columns 1–3 are fixed-width, column 4 is a path, and optional trailing fields are comma-separated key=value pairs.

```
2026-04-21T14:03:22Z  task-042  draft→pending           runtime/logs/task-042-pending.log
2026-04-21T14:03:22Z  task-042  pending→agent-review    runtime/logs/task-042-agent-review.log
2026-04-21T14:07:11Z  task-042  agent-review→completed  runtime/logs/task-042-agent-review.log  exit=0,duration=3m49s
```

Rules:
- Timestamps are UTC, RFC 3339, second precision.
- The transition column uses the UTF-8 arrow `→` (U+2192).
- Paths are workspace-relative if inside the workspace, otherwise absolute.
- Trailing metadata is only added on `SlotReleased` events (`exit`, `duration`, `outcome`).
- The file is safe to `tail -f` from other shells while `rhei run` is active.

A `SlotAssigned` produces one line; its paired `SlotReleased` produces a second line on the same state (recording exit status and duration). For multi-invocation states (`all_targets`), each invocation is a distinct pair of lines with the target suffix visible in the log path.

### 1.8. Failure Modes

- **Panic in the execution engine** — a panic hook registered by `TuiSink` calls `ratatui::restore()` before re-raising, so the terminal is never left in raw mode.
- **Ctrl+C** — because the TUI runs the terminal in raw mode, Ctrl+C arrives as a key event rather than an automatic `SIGINT`. `TuiSink` restores the terminal, explicitly re-raises `SIGINT` for the process, and then exits its render loop.
- **Terminal too small for any tile** — auto-degrade to compact list mode; never crash.
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
