# Rhei: `rhei run` Agent Traffic in the TUI

## Context

`rhei run` already emits lifecycle events into `rhei-tui`, writes per-task logs under `runtime/logs/`, and shows run messages in the TUI journal. Agent subprocess output is still redirected directly to log files inside `spawn_and_wait_agent`, so the TUI cannot show live agent traffic until after the fact.

The implementation should intercept live stdout and stderr for the supported built-in agents requested here:

- `claude-code`: command `claude`, prompt via `-p <prompt>`.
- `codex`: command `codex exec`, prompt via stdin, with `--` appended.
- `pi`: command `pi`, prompt via `-p <prompt>`.

The solution should use one shared capture path for all three agents. Agent-specific differences stay in command construction and prompt delivery; output interception should not fork into three separate implementations.

## Design Direction

Spawn agent subprocesses with piped stdout and stderr, read both streams on background threads, tee each line to the existing task log, and emit a structured TUI event for each observed line. Keep the log file as the durable source of truth, but let the TUI receive traffic directly instead of polling or tailing the file.

Use bounded buffers in the UI. The engine must never block because the TUI is slow; if the render channel is full, dropping live display events is acceptable because the log still contains the full transcript.

## Tasks

### Task 1: Specify live agent traffic events
**State:** completed

Define the contract for streaming agent output through `rhei-tui`.

Add a short section to `docs/specs/rhei-run-tui.spec.md` describing live traffic events, retention, and failure behavior. The spec should make clear that per-task logs remain complete, while the TUI keeps only a bounded recent view.


> **Result:** [1](runtime/results/1.md)
#### Task 1.1: Add event shape to the spec
**State:** completed

Document a new event variant such as `AgentOutput { slot, task, stream, line, wall_clock }`, where `stream` distinguishes stdout and stderr. Include ordering expectations: lines are ordered per stream, but stdout and stderr interleaving is best-effort because they are read concurrently.

Added `AgentOutput` and `AgentStream` to `docs/specs/rhei-run-tui.spec.md`, including line ordering and durable log behavior.

#### Task 1.2: Document supported-agent behavior
**State:** completed

Call out `claude-code`, `codex`, and `pi` explicitly. The document should state that the same output interception path applies to all three; Codex is special only for stdin prompt delivery, not for traffic capture.

Documented the shared traffic capture path for `claude-code`, `codex`, and `pi`, with prompt transport differences isolated to command construction.

#### Task 1.3: Document truncation and backpressure
**State:** completed

Specify that TUI panes retain a bounded number of recent lines per slot and may drop display events under pressure. The task log must remain complete unless the underlying filesystem write fails.

Documented bounded TUI retention, best-effort display delivery, and complete per-task log preservation.

### Task 2: Extend the TUI event API
**State:** completed
**Prior:** Task 1

Add the live traffic event types to `crates/rhei-tui`.

Update `crates/rhei-tui/src/event.rs` with an `AgentStream` enum and a `RunEvent::AgentOutput` variant. Keep the event cheap to clone because `Tee` forwards it to multiple sinks.


> **Result:** [2](runtime/results/2.md)
#### Task 2.1: Add `AgentStream`
**State:** completed

Define `AgentStream::{Stdout, Stderr}` with `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.

Added `AgentStream` to `crates/rhei-tui/src/event.rs` and re-exported it from `crates/rhei-tui/src/lib.rs`.

#### Task 2.2: Add `RunEvent::AgentOutput`
**State:** completed

Include `slot`, `task`, `stream`, `line`, and `wall_clock`. Do not include raw byte buffers in the first version; line-oriented traffic is sufficient for the current TUI and journal.

Added `RunEvent::AgentOutput` with slot, task, stream, line, and wall clock fields.

#### Task 2.3: Keep non-TUI behavior stable
**State:** completed

Ensure `StdoutSink` ignores `AgentOutput` so non-TTY runs do not start echoing agent logs to the terminal. Ensure `JournalSink` ignores it unless a later task intentionally adds a separate transcript journal.

Left `StdoutSink` and `JournalSink` behavior unchanged for non-message and non-transition events, so `AgentOutput` is ignored outside the TUI.

### Task 3: Teach the TUI to display live traffic
**State:** completed
**Prior:** Task 2

Update `crates/rhei-tui/src/tui.rs` so each active slot stores recent agent output and renders it in the slot area and journal.

The UI should remain useful when there is only one slot, as in the screenshot. In compact terminals, prioritize the most recent traffic lines over decorative detail.


> **Result:** [3](runtime/results/3.md)
#### Task 3.1: Store per-slot traffic
**State:** completed

Add a bounded `VecDeque` of recent traffic lines to `SlotState`. Reset it on `SlotReleased`. A capacity around 50 lines per slot is enough for context without growing memory unbounded.

Added bounded per-slot traffic storage in `crates/rhei-tui/src/tui.rs`; slot reset continues to clear traffic on release.

#### Task 3.2: Render traffic in active slots
**State:** completed

When a slot is active, show task id, state, elapsed time, and a small tail of stdout/stderr lines. Prefix stderr lines distinctly, but keep the visual treatment restrained so errors are visible without overwhelming the layout.

Rendered a compact live traffic tail under each active slot, with distinct stdout and stderr labels.

#### Task 3.3: Mirror concise traffic into the journal
**State:** completed

For `AgentOutput`, append short journal entries such as `· [slot 0 stdout] ...` or `! [slot 0 stderr] ...`. Apply width-safe truncation so a single long agent line does not destroy the journal layout.

Mirrored sanitized and truncated traffic lines into the journal pane with stdout/stderr prefixes.

#### Task 3.4: Add TUI state tests
**State:** completed

Unit test `UiState::apply` for `AgentOutput`: traffic is appended to the right slot, bounded retention drops oldest lines, and output for an unknown slot is ignored or journaled as a warning without panicking.

Added TUI state tests for traffic insertion, bounded retention, unknown slots, sanitization, and truncation.

### Task 4: Replace direct log redirection with an output tee
**State:** completed
**Prior:** Task 2

Refactor `spawn_and_wait_agent` in `crates/rhei-cli/src/main.rs` so stdout and stderr are piped, read, written to the task log, and emitted as `AgentOutput`.

This is the core interception work. It should preserve existing log headers, footers, timeout handling, and exit-code behavior.


> **Result:** [4](runtime/results/4.md)
#### Task 4.1: Introduce an agent output capture helper
**State:** completed

Create a small helper in `main.rs`, or a focused module if the file becomes too noisy, that owns a cloned log writer, a stream label, the slot, task id, and `Arc<dyn EventSink>`. It reads bytes line-by-line from a child stream, writes every line to the log, and emits `RunEvent::AgentOutput`.

Added `spawn_agent_output_reader` in `crates/rhei-cli/src/main.rs` to tee stream lines to the log and `AgentOutput`.

#### Task 4.2: Make log writes thread-safe
**State:** completed

Replace cloned `File` stdout/stderr redirection with an `Arc<Mutex<File>>` or `Arc<Mutex<BufWriter<File>>>` shared by header, stdout reader, stderr reader, and footer writer. Keep line writes atomic enough that stdout and stderr lines do not interleave inside one line.

Replaced direct file redirection with a shared `Arc<Mutex<File>>` used by header, stream readers, and footer writes.

#### Task 4.3: Preserve timeout and kill behavior
**State:** completed

Keep the existing `try_wait` loop, SIGTERM, grace period, and kill fallback. After the child exits, join the stdout and stderr reader threads before writing the exit footer so the log footer remains last.

Kept the existing timeout loop and kill behavior; stdout and stderr reader threads are joined before the exit footer is written.

#### Task 4.4: Handle partial final lines
**State:** completed

Ensure output without a trailing newline is still written to the log and emitted once. Avoid silently losing the final agent status line from CLIs that flush without newline.

Handled partial final lines through `read_until`, with unit coverage proving both complete and partial lines are emitted.

### Task 5: Thread slot and sink context into agent spawning
**State:** completed
**Prior:** Task 4

Update every call to `spawn_and_wait_agent` so it receives enough context to emit traffic events.

Sequential and parallel paths already emit `SlotAssigned` before spawning. Reuse that slot id for all `AgentOutput` events until `SlotReleased`.


> **Result:** [5](runtime/results/5.md)
#### Task 5.1: Update the function signature
**State:** completed

Add parameters for `slot: rhei_tui::Slot` and `sink: Arc<dyn rhei_tui::EventSink>` to `spawn_and_wait_agent`. Keep the rest of the signature stable unless the capture helper makes a small context struct cleaner.

Extended `spawn_and_wait_agent` with the active slot and event sink needed by the output reader.

#### Task 5.2: Update the sequential path
**State:** completed

Pass slot `0` and the active sink from the single-agent branch. Verify the screenshot-style `--parallel 1` case shows agent output below the existing `▶ slot 0` entry.

Updated the sequential branch to pass slot `0` and the active sink into the agent spawn path.

#### Task 5.3: Update the parallel path
**State:** completed

Move each acquired slot and a cloned sink into the worker thread. Ensure traffic from concurrent agents stays associated with the correct slot and task id.

Updated the parallel branch to move each slot and cloned sink into its worker thread, keeping emitted traffic associated with the correct task.

### Task 6: Verify Claude Code, Codex, and Pi with fake agents
**State:** completed
**Prior:** Task 5

Add deterministic tests using fake executables so CI does not need real agent CLIs installed.

Each fake executable should emit stdout and stderr over time, optionally omit a trailing newline, and exit with a controlled status. Tests should assert both the task log and emitted TUI events.


> **Result:** [6](runtime/results/6.md)
#### Task 6.1: Add a fake `claude` test profile
**State:** completed

Exercise the `claude-code` shape: prompt passed via `-p`, model flag if present, stdout/stderr captured live, and task log complete.

Added a fake executable test that exercises the `claude-code` prompt-flag shape and verifies live output events and log contents.

#### Task 6.2: Add a fake `codex` test profile
**State:** completed

Exercise the Codex shape: prompt written to stdin, `--` appended, stdout/stderr captured live, and no prompt bytes are mistaken for agent output.

Added a fake executable test that exercises Codex stdin prompt delivery through the same output capture path.

#### Task 6.3: Add a fake `pi` test profile
**State:** completed

Exercise the Pi shape: prompt passed via `-p`, no permission-mode assumptions, stdout/stderr captured live, and task log complete.

Added a fake executable test that exercises the `pi` prompt-flag shape and verifies live output events.

#### Task 6.4: Add timeout coverage
**State:** completed

Use a fake agent that emits a line, sleeps past the timeout, and is killed. Assert the emitted line appears in the TUI event recorder and the log footer still records the timeout exit path.

Added timeout coverage with a sleeping fake agent, a test-only short terminate grace period, and assertions for emitted output and log footer preservation.

### Task 7: Polish display ergonomics
**State:** completed
**Prior:** Task 3, Task 6

Tune the TUI so live traffic improves observability without making the screen noisy.


> **Result:** [7](runtime/results/7.md)
#### Task 7.1: Truncate long lines safely
**State:** completed

Add a helper for width-aware truncation of traffic lines. Avoid breaking the UI when agents emit long JSON, stack traces, or terminal control sequences.

Added TUI truncation helpers for journal and slot traffic lines, with tests.

#### Task 7.2: Strip or neutralize control sequences
**State:** completed

Sanitize ANSI control sequences before inserting text into TUI widgets. Keep plain logs complete; sanitize only the rendered line stored in `AgentOutput` or the UI state.

Added display-only ANSI/control character sanitization in `crates/rhei-tui/src/tui.rs`; raw task logs remain unchanged.

#### Task 7.3: Improve slot labels
**State:** completed

Include the agent id in the active slot label when available, so concurrent `claude-code`, `codex`, and `pi` traffic is easy to distinguish. If the existing `SlotAssigned` event lacks agent id, add it only if that can be done without disrupting existing journal format.

Added an optional `agent` field to `SlotAssigned` and rendered it in active slot labels for agent invocations; program slots use `None`, and journal formatting remains unchanged.

### Task 8: Run CI-level verification
**State:** completed
**Prior:** Task 7

Run the repository verification commands from `AGENTS.md` and fix issues found by formatting, clippy, build, or tests.

Commands:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets
cargo test --workspace --all-targets --no-fail-fast
```

If the full suite is too slow during iteration, run focused package tests first, then finish with the full commands above before marking the task complete.

Ran the full verification sequence successfully: formatting check, workspace clippy with warnings denied, workspace build for all targets, and workspace tests with `--no-fail-fast`.

> **Result:** [8](runtime/results/8.md)
