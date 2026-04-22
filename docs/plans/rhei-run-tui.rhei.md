# Rhei: `rhei run` TUI and Transition Journal

## Context

Implement the spec at [`docs/specs/rhei-run-tui.spec.md`](rhei-run-tui.spec.md) — a new `rhei-tui` crate that provides an event surface, a persistent transition journal, and a TUI frontend for parallel `rhei run` execution. The work ships as three independently reviewable PRs.

The existing `rhei run` implementation lives in [`crates/rhei-cli/src/main.rs`](../../crates/rhei-cli/src/main.rs), with the main agent-mode loop at `run_agent_mode` (lines 5007+) and the parallel spawn path around lines 5537–5720. Per-task log paths are produced by `agent_log_path` (line 4508) and `program_log_path` (line 4661). `--parallel` gating for workspaces is at lines 4951–4961. `notify` and `nix` are already workspace dependencies.

PR 1 introduces the event surface and the journal without changing user-visible output. PR 2 adds the TUI. PR 3 adds log tailing inside tiles. Each PR is independently valuable: PR 1 alone gives users the transition journal they asked for; PR 2 adds the live view; PR 3 polishes the view with log previews.

## PR Map

- **PR 1: Journal.** Tasks 1–5. Introduces the `rhei-tui` crate, the event surface, the sinks, and refactors `run_agent_mode` to emit events. `runtime/transitions.log` starts being written. No user-visible stdout changes.
- **PR 2: TUI.** Tasks 6–9, 11. Adds log tailing, TuiSink, layout, slot grid, and wires `Frontend::Tui` with TTY auto-detect. Documentation updated.
- **PR 3: Polished tiles.** Task 10. Log previews in tiles. Smallest PR; lands after PR 2 has stabilized.

Task 12 is explicitly a follow-up and should not land in PRs 1–3.

## Tasks

### Task 1: Create `rhei-tui` crate skeleton with event types and `EventSink` trait
**State:** pending

Add a new crate at `crates/rhei-tui/` with only the event surface and no UI dependencies yet. This crate will grow in Task 6 and Task 7; keeping it empty-but-real up front lets PR 1 depend on it without dragging in ratatui.

Define in `crates/rhei-tui/src/event.rs`:

- `pub enum RunEvent` with variants `RunStarted`, `PassStarted`, `SlotAssigned`, `SlotReleased`, `PassEnded`, `RunFinished` matching the spec.
- `pub enum TaskOutcome` with variants `Completed`, `Failed(String)`, `Cancelled`, `TimedOut`.
- `pub trait EventSink: Send + Sync { fn emit(&self, event: RunEvent); }`.
- A `pub struct Tee { sinks: Vec<Arc<dyn EventSink>> }` with a constructor and `EventSink` impl that forwards to each inner sink.

Wire the crate into the workspace `Cargo.toml` and add `rhei-tui = { path = "../rhei-tui" }` as a dependency of `rhei-cli`. Nothing in `rhei-cli` uses it yet.

#### Task 1.1: Create crate and add to workspace
**State:** pending

Create `crates/rhei-tui/Cargo.toml` (edition 2021, MIT OR Apache-2.0). No external deps at this point — just `std`. Add the crate to the workspace members list.

#### Task 1.2: Define `RunEvent`, `TaskOutcome`, `EventSink`
**State:** pending

Write `crates/rhei-tui/src/event.rs` with the types from the spec. Use `std::path::PathBuf`, `std::time::Instant`, and a `TaskId` newtype (`pub struct TaskId(pub String)`) and `StateName` newtype (`pub struct StateName(pub String)`) local to the crate. Derive `Debug` and `Clone` on everything except `EventSink`.

#### Task 1.3: Define `Tee` composite sink
**State:** pending

Add `pub struct Tee { sinks: Vec<std::sync::Arc<dyn EventSink>> }` with `Tee::new(sinks: Vec<Arc<dyn EventSink>>) -> Self` and an `EventSink` impl that iterates and forwards. Verify with a unit test that a two-sink `Tee` delivers each event to both inner sinks in order.

#### Task 1.4: Add `rhei-tui` as dependency of `rhei-cli`
**State:** pending

Append `rhei-tui = { path = "../rhei-tui" }` to `crates/rhei-cli/Cargo.toml`. No imports yet in `main.rs` — the dependency edge is established but unused. Confirm the workspace still builds.

### Task 2: Add `JournalSink` writing the transition log
**State:** pending
**Prior:** Task 1

Implement `crates/rhei-tui/src/journal.rs` with `pub struct JournalSink` that opens `runtime/transitions.log` in append mode and writes one line per `SlotAssigned` and one per `SlotReleased`, following the format in the spec.

Format rules:
- UTC RFC 3339 timestamp, second precision.
- Arrow glyph `→` (U+2192) between `from` and `to`.
- Path column is the `log_path` from the event, rendered workspace-relative when inside the workspace.
- `SlotReleased` lines append `  exit=<code>,duration=<human>` (two spaces before the key=value block). Duration is computed from `started_at` to `finished_at` and formatted as `HmM` / `MmSs` / `Ns`.
- Journal write errors produce a single `eprintln!` warning and are otherwise ignored — they never abort a run.

Other event variants (`RunStarted`, `PassStarted`, `PassEnded`, `RunFinished`) are no-ops in the journal.

#### Task 2.1: Open and append to `runtime/transitions.log`
**State:** pending

Constructor `JournalSink::open(workspace_root: &Path) -> io::Result<Self>`. Creates `runtime/` if missing, opens the file in append+create mode, stores the file handle behind a `Mutex<BufWriter<File>>`. Store `workspace_root` for path rewriting.

#### Task 2.2: Implement `EventSink` for `JournalSink`
**State:** pending

Match on `RunEvent`. For `SlotAssigned`, format the line and flush. For `SlotReleased`, format including exit/duration and flush. Use a small helper `format_relative_path(root, path)` that returns the relative path if `path.starts_with(root)`, otherwise the absolute.

#### Task 2.3: Unit test the line format
**State:** pending

Test emits scripted `SlotAssigned` and `SlotReleased` events into a temp-dir journal, then asserts the file contents match expected column positions and arrow glyph. Include a case where the log path is outside the workspace (absolute path in output).

### Task 3: Add `StdoutSink` preserving current `println!` behavior
**State:** pending
**Prior:** Task 1

Implement `crates/rhei-tui/src/stdout.rs` with `pub struct StdoutSink` implementing `EventSink`. Each event variant maps to the exact `println!` / `eprintln!` calls already present in `main.rs::run_agent_mode`, so that replacing the inline prints in Task 5 is observably a no-op on stdout.

Map events to current output strings:
- `RunStarted` → `"Running {plan|workspace} '{title}' with N task(s) (K terminal at start)."` and `"Initial states: ..."`.
- `PassStarted` → `"\nPass {n}: {ready} ready, {terminal} terminal, {total} total."` and `"Ready: ..."`.
- `SlotAssigned` → `"\nSpawning agent '{agent}' for Task {id}: {title}"` with `Model:` and `Log:` lines, matching the existing sequential and parallel variants.
- `SlotReleased` → `"  Task {id} advanced: '{from}' -> '{to}'"` on success-advance, `"  error: agent exited with code N for task M"` on failure, `"  warning: agent exited 0 but task N did not advance from '...'"` on no-op, etc. — each matching the current code paths.
- `PassEnded` → no output (the existing code prints nothing per pass end).
- `RunFinished` → the final summary block.

Because the engine refactor in Task 5 is the *only* way engine output is produced after PR 1, `StdoutSink` must reproduce every existing message character-for-character (and to the correct stream — `println!` vs `eprintln!`).

#### Task 3.1: Implement `StdoutSink` struct and `EventSink` impl
**State:** pending

Stateless struct. Match on `RunEvent`, writing to `std::io::stdout()` or `stderr()` as appropriate.

#### Task 3.2: Snapshot test against current output
**State:** pending

Pick one existing integration test in `crates/rhei-cli/tests/e2e/` that exercises `rhei run` and capture its current stdout. Add a unit test that feeds a scripted event stream to `StdoutSink` and asserts byte-equal output. This will be the regression check for Task 5.

### Task 4: Build the sink pipeline helper
**State:** pending
**Prior:** Task 2, Task 3

Add `pub fn build_sink(workspace_root: &Path, frontend: Frontend) -> Arc<dyn EventSink>` in `crates/rhei-tui/src/lib.rs`. It constructs:

```
Tee { sinks: [Arc<JournalSink>, Arc<frontend_sink>] }
```

where `frontend_sink` is `StdoutSink` for now (the TuiSink arrives in PR 2). Introduce a `pub enum Frontend { Stdout, Tui }` stub — the `Tui` variant falls back to `Stdout` until PR 2 lands. This keeps the signature stable across PRs.

Expose via `pub use event::*`, `pub use journal::JournalSink`, `pub use stdout::StdoutSink`.

#### Task 4.1: Add `Frontend` enum and `build_sink`
**State:** pending

Match on `Frontend`; construct the journal first, then the frontend, wrap both in a `Tee`. Return `Arc<dyn EventSink>`.

#### Task 4.2: Integration test for sink fan-out
**State:** pending

Construct `build_sink(temp_dir, Frontend::Stdout)`, emit a scripted sequence, assert the journal file has the expected number of lines and stdout (captured via a test harness or a custom `StdoutSink`-style sink injected via a feature hook) received the expected calls.

### Task 5: Refactor `run_agent_mode` to emit events instead of printing directly
**State:** pending
**Prior:** Task 4

Replace every `println!` / `eprintln!` in `run_agent_mode` (and the dry-run / program / callback branches it calls) with an `sink.emit(RunEvent::...)` call against a `sink: &Arc<dyn EventSink>` plumbed in from `run_plan`. The control flow is unchanged; only the output path moves.

Key sites in [`crates/rhei-cli/src/main.rs`](../../crates/rhei-cli/src/main.rs):

- Lines 4976–4983: `RunStarted` emission.
- Lines 5036–5043: `PassStarted` emission.
- Lines 5058–5062: `SlotAssigned` (for gating / callback-only tasks that still transition).
- Lines 5162–5165, 5229–5232, 5256–5259, 5432–5435, 5483–5486, 5671–5675: `SlotReleased` with outcome `Completed` (state advanced).
- Lines 5168–5169, 5490–5499, 5677–5688: `SlotReleased` with outcome `Failed` (no advance).
- Lines 5503–5527: `SlotReleased` with outcome `Failed` + exit code.
- Lines 5402–5411, 5577–5583: `SlotAssigned` for sequential and parallel agent spawns.
- Lines 5184–5218: `SlotAssigned` / dry-run description for programs.

Slot indices: in sequential mode, always slot 0. In parallel mode, assign slots 0..=N-1 as handles are collected into `handles`; reuse freed slots across passes.

Introduce a lightweight `SlotPool` helper (in `main.rs`, not `rhei-tui`) that hands out and returns slot numbers to avoid passing them around by index math.

Add `--tui` and `--no-tui` flags to the `RunOptions` struct but keep them as no-ops for now — they just influence `Frontend::Stdout` vs `Frontend::Tui`, and since `Tui` still falls back to `Stdout` in PR 1, user-visible behavior is unchanged.

#### Task 5.1: Add `--tui` / `--no-tui` flags to `RunOptions`
**State:** pending

Extend the clap struct. Enforce mutual exclusion with a clap `conflicts_with` attribute. Add `fn frontend(&self) -> Frontend` that resolves to `Tui`, `Stdout`, or auto (based on `std::io::IsTerminal`).

#### Task 5.2: Build the sink at the top of `run_plan` and thread it
**State:** pending

In `run_plan`, call `rhei_tui::build_sink(&workspace_root, opts.frontend())` once and pass `Arc<dyn EventSink>` into `run_agent_mode` / `run_callback_mode`. All downstream helpers that currently print receive the sink.

#### Task 5.3: Replace `RunStarted` and `PassStarted` prints
**State:** pending

Remove the two `println!` calls at lines 4976–4983 and emit `RunStarted`. Replace lines 5036–5043 with `PassStarted`. Do not yet touch per-spawn prints.

#### Task 5.4: Add `SlotPool` and replace sequential spawn prints
**State:** pending

Add a small `SlotPool::with_capacity(n)` struct in `main.rs`. Replace the sequential spawn messages (lines 5402–5411 and friends) with `SlotAssigned` via `pool.acquire()` and the completion messages with `SlotReleased` via `pool.release(slot)`.

#### Task 5.5: Replace parallel spawn prints
**State:** pending

In the parallel branch (lines 5537–5720), acquire slots before spawning each thread, move the slot index into the closure alongside the other captured state, and emit `SlotAssigned` / `SlotReleased` pairs. Preserve result collection order semantics.

#### Task 5.6: Replace program and callback-task prints
**State:** pending

Update lines 5127–5171 (callback-only), 5173–5340 (program spawns and dry-run) to emit `SlotAssigned` / `SlotReleased` for each. Programs use a separate slot pool allocation to avoid mixing with agent slots in the TUI.

#### Task 5.7: Replace `RunFinished` summary print
**State:** pending

At the end of `run_plan`, emit `RunFinished` with counts from the accumulated `agents_spawned`, `programs_spawned`, etc.

#### Task 5.8: Integration test for stdout regression
**State:** pending

Re-run the existing `crates/rhei-cli/tests/e2e/` suite; no diffs in stdout. Add an explicit test that reads `runtime/transitions.log` after a small scripted run and verifies it has the expected transition lines.

### Task 6: Extract log tailer as a standalone module in `rhei-tui`
**State:** pending
**Prior:** Task 1

Add `crates/rhei-tui/src/tail.rs` with `pub struct LogTail` backed by `notify` and a bounded 50-line ring buffer. This module is independent of ratatui and is unit-testable on its own.

API:

```rust
pub struct LogTail { /* ring buffer, notify watcher */ }

impl LogTail {
    pub fn open(path: &Path, capacity: usize) -> io::Result<Self>;
    pub fn snapshot(&self) -> Vec<String>;  // last N lines, oldest first
    pub fn close(self);
}
```

Internally, spawn a small reader thread that watches the file and pushes new lines into a `Mutex<VecDeque<String>>`. Drop the watcher and thread in `close()`.

Unit test: write to a temp file, assert snapshot contains the appended lines; overflow past capacity drops oldest lines.

This task lives under PR 2 but is self-contained and has no dependency on any TUI widgets.

#### Task 6.1: `LogTail` struct and reader thread
**State:** pending

Use `notify::RecommendedWatcher` to watch the parent directory (the file may not yet exist at `open` time; handle the create event). On each modify, seek to last-read offset, read new bytes, split on newlines, push onto the ring buffer.

#### Task 6.2: Bounded ring buffer and snapshot
**State:** pending

Back by `VecDeque<String>` behind a mutex. On push, if `len() == capacity`, `pop_front()`. `snapshot()` clones the contents into a `Vec<String>`.

#### Task 6.3: Unit test `LogTail` end to end
**State:** pending

Temp file + write loop + sleep + assert `snapshot()` contents. Also test: opening `LogTail` before the file exists, then creating it, then writing — the tail should pick up the lines.

### Task 7: Add TUI dependencies and `TuiSink` skeleton
**State:** pending
**Prior:** Task 5, Task 6

Add `ratatui`, `crossterm`, and `crossbeam-channel` to `crates/rhei-tui/Cargo.toml`. Create `crates/rhei-tui/src/tui/mod.rs` with `pub struct TuiSink` that owns a `crossbeam_channel::Sender<RunEvent>` and spawns a render thread at construction time.

The render thread:
1. Enters alternate screen and raw mode via crossterm.
2. Registers a panic hook that restores the terminal before re-raising.
3. Runs an event loop selecting on the event channel, a 100ms tick, and `crossterm::event::read`.
4. On `RunFinished`, breaks the loop, restores the terminal, and joins.

For this task, the render body is a placeholder — it drains events and prints nothing. The test goal is: constructing `TuiSink`, sending scripted events, and seeing the render thread exit cleanly after `RunFinished` without leaking terminal state.

#### Task 7.1: Add crate dependencies
**State:** pending

Append `ratatui`, `crossterm`, `crossbeam-channel` to `crates/rhei-tui/Cargo.toml`. Verify the workspace still builds.

#### Task 7.2: `TuiSink` struct and render thread scaffold
**State:** pending

`TuiSink::spawn(params: RunParams) -> Self`. Internally: create channel, spawn render thread with receiver, store sender and `JoinHandle`. Implement `EventSink` by sending on the channel (`try_send` with a warning on full — never blocks the engine).

#### Task 7.3: Install panic hook that restores the terminal
**State:** pending

On first `TuiSink::spawn`, install a panic hook (use `std::panic::set_hook`) that disables raw mode and leaves the alternate screen before calling the previous hook. Use a `std::sync::Once` so installing twice is safe.

#### Task 7.4: Render thread loop structure
**State:** pending

`select!` on: event channel (drain all available events into a local state struct), `std::time::Instant::now()` tick every 100ms, `crossterm::event::poll` for keyboard. On 'q' or 'Q' or Ctrl-C: set a quit flag and break after emitting a final render. On `RunFinished`: break.

### Task 8: Implement the slot grid layout and idle/active rendering
**State:** pending
**Prior:** Task 7

Add `crates/rhei-tui/src/tui/layout.rs` with the layout-decision function and the slot grid renderer.

```rust
pub enum Layout { Single, Grid(u8, u8), CompactList }

pub fn choose_layout(parallel: u8, term_rows: u16, term_cols: u16) -> Layout;
```

Rules follow the spec's layout table. `Grid(rows, cols)` is computed for `parallel` in 2..=9 when `term_rows / rows >= 6`; otherwise `CompactList`.

Add a `SlotState` struct tracked in the render thread:

```rust
struct SlotState {
    task: Option<TaskId>,
    title: Option<String>,
    state: Option<StateName>,
    started_at: Option<Instant>,
    log_path: Option<PathBuf>,
}
```

Update `SlotState` on `SlotAssigned`; clear on `SlotReleased`. Render each tile with: task id + truncated title (top line), current state (next line), elapsed time (right-aligned top line). For this task, do **not** yet show log tails — that comes in Task 10.

Also add a bottom journal pane: the last ~8 `SlotReleased` events formatted identically to the journal file line format.

#### Task 8.1: `choose_layout` with unit tests
**State:** pending

Cover the table rows: `parallel=1`, `parallel=4` with 24 rows (grid), `parallel=4` with 10 rows (compact), `parallel=12` (always compact).

#### Task 8.2: Slot state tracker
**State:** pending

`HashMap<u8, SlotState>`. Update on `SlotAssigned` / `SlotReleased`. On `SlotReleased`, drop the entry after a brief 1-second hold so the user sees the final state before the tile flips to idle.

#### Task 8.3: Tile widget without log tail
**State:** pending

Use `ratatui::widgets::Paragraph` inside `Block::default().borders(ALL).title(task_id)`. Render task title, state, elapsed. Idle slots show `— idle —`.

#### Task 8.4: Grid and compact-list renderers
**State:** pending

For `Layout::Grid(rows, cols)`, split the top region into rows×cols tiles. For `Layout::CompactList`, one line per slot: `slot_n  task_id  state  elapsed`.

#### Task 8.5: Journal pane at bottom
**State:** pending

Keep a `VecDeque<String>` of the last 8 released-transition lines (same format as `JournalSink`). Render in a `Paragraph` with a border titled "Transitions".

#### Task 8.6: Handle terminal resize
**State:** pending

On `crossterm::event::Event::Resize`, recompute the layout. The tile/list switch must be visibly live.

### Task 9: Wire `TuiSink` into `build_sink` and gate with `Frontend::Tui`
**State:** pending
**Prior:** Task 8

Update `rhei_tui::build_sink` from Task 4 so `Frontend::Tui` actually constructs a `TuiSink` (previously it fell back to `StdoutSink`). Keep the fallback when `TuiSink::spawn` fails (e.g., no terminal) with a single `eprintln!` warning.

Update `RunOptions::frontend` in `main.rs` to resolve the auto case via `std::io::IsTerminal`:

```rust
fn frontend(&self) -> Frontend {
    if self.no_tui { return Frontend::Stdout; }
    if self.tui { return Frontend::Tui; }
    if std::io::stdout().is_terminal() { Frontend::Tui } else { Frontend::Stdout }
}
```

After this task, running `rhei run --parallel 4 plan.rhei.md` in an interactive terminal shows the live grid; running the same command in CI or piped shows the current output. The journal is written in both cases.

#### Task 9.1: `build_sink` uses `TuiSink` for `Frontend::Tui`
**State:** pending

Match arm constructs `TuiSink::spawn(params)`; on error, warn and substitute `StdoutSink`.

#### Task 9.2: Auto-detect TTY
**State:** pending

Use `std::io::IsTerminal` (stable since Rust 1.70). Verify both `Frontend::Tui` and `Frontend::Stdout` paths with integration tests that force stdout to be piped vs a PTY.

#### Task 9.3: Smoke test with a two-task plan
**State:** pending

Add an opt-in test (gated behind `#[cfg(feature = "tui-smoke")]` or ignored by default) that runs `rhei run --tui` against a two-task plan and asserts the journal has the expected lines and the process exits cleanly within a timeout.

### Task 10: Show tailed log lines in each tile
**State:** pending
**Prior:** Task 6, Task 9

In the render thread, keep a `HashMap<u8, LogTail>` keyed by slot. Open a `LogTail` when a `SlotAssigned` arrives with a `log_path`; close it on `SlotReleased` after the 1-second hold.

Update the tile widget to show the last 5 lines from `tail.snapshot()` below the state/elapsed lines. Truncate long lines to fit the tile width. If the log file does not yet exist, show `— waiting for log —`.

In compact list mode, log tails are not shown (per the layout rules).

#### Task 10.1: Per-slot `LogTail` lifecycle
**State:** pending

Open on `SlotAssigned`, close on `SlotReleased` (after hold). Swallow I/O errors with a `log_status: Error(String)` on the `SlotState`.

#### Task 10.2: Render log preview area in tile
**State:** pending

Reserve the bottom ~5 lines of each tile for the log snapshot. Use `Paragraph` with wrap disabled; truncate each line at `tile_width - 2`.

#### Task 10.3: End-to-end smoke test for log tails
**State:** pending

Run a plan where one task's state produces deterministic log output. Assert that by the time `SlotReleased` arrives, the tile's log snapshot contained the expected lines (test via a scripted event stream and direct widget-state inspection, not a rendered frame).

### Task 11: Update `rhei run` user documentation
**State:** pending
**Prior:** Task 9

Update [`docs/specs/rhei-usage.spec.md`](rhei-usage.spec.md) Pattern 0 and Pattern 3 to mention `--tui` / `--no-tui` and the transition journal. Add a brief paragraph explaining: "by default, `rhei run` detects an interactive terminal and shows a live grid of parallel agents; the full transition history is always written to `runtime/transitions.log`".

Cross-link from [`docs/specs/rhei-agents.spec.md`](rhei-agents.spec.md) (the log-capture section) to the journal and the spec at [`docs/specs/rhei-run-tui.spec.md`](rhei-run-tui.spec.md).

#### Task 11.1: Edit `rhei-usage.spec.md`
**State:** pending

Add `--tui` / `--no-tui` to the `rhei run` flag description and mention the journal file.

#### Task 11.2: Edit `rhei-agents.spec.md`
**State:** pending

Add a sentence in the log-capture section: "In addition to per-state log files at `runtime/logs/`, `rhei run` appends one line per state transition to `runtime/transitions.log` — see the [`rhei run` TUI spec](rhei-run-tui.spec.md)."

### Task 12: Add `rhei watch` command as a follow-up
**State:** draft
**Prior:** Task 9

This task is out of scope for PRs 1–3 and is listed as a follow-up. Add a `rhei watch <workspace>` subcommand that attaches the TUI to an already-running `rhei run` by reading its journal and tailing the referenced log files. Useful for attaching from a second terminal or after a detached run.

The `draft` state signals that the concrete design should be worked out after the core three PRs land, since the reader-only TUI mode may want different lifecycle semantics (survive engine exit, replay history, etc.) that are easier to evaluate once PRs 1–3 are shipped.
