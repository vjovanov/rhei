# Reconciliation Proposal: TUI Monitoring

Source elaboration: `runtime/spec-implementation-discrepancy-audit/tui-monitoring/elaboration.md`

This proposal names a primary human decision option for each elaborated
discrepancy and records a credible alternative. Decision options use the audit
vocabulary: `update-spec`, `update-implementation`, `update-both`,
`defer-follow-up`, `no-change`.

## TM-E001: Callback-only runs bypass the TUI and journal event surface

- Primary decision: `update-both`.
- Next edits: update `docs/specs/rhei-run-tui.spec.md` so the event surface can
  represent committed transitions independently from spawn slots, for example
  with a `TransitionCommitted` event carrying task id, title when available,
  from/to states, log path if one exists, wall-clock time, and result metadata.
  Then refactor `run_callback_mode` to accept an `EventSink`, emit run/pass
  lifecycle and transition events, and route user-facing lines through the same
  stdout frontend path used by autonomous mode. Keep dry-run on a no-artifact
  path as described in TM-E002.
- Expected tests: add callback-only e2e coverage for non-dry-run journal
  creation, actual transition lines, `--no-tui` stdout compatibility, and
  absence of runtime artifacts under callback-only `--dry-run`.
- Reason preferable: the universal journal is valuable, but callback-only
  transitions do not map cleanly to the current slot-assignment API. Updating
  both the spec and implementation avoids inventing fake slots or fake log
  files while preserving one monitoring surface for all run modes.
- Alternative: `update-spec` to scope the TUI and journal only to autonomous
  subprocess runs. This is credible if callback-only monitoring is deliberately
  out of scope, but it weakens the audit trail and contradicts the current
  "always writes through a Tee" language.

## TM-E002: Autonomous dry-run creates runtime artifacts

- Primary decision: `update-implementation`.
- Next edits: move autonomous dry-run handling before `select_frontend`, or pass
  a dry-run frontend that never opens `JournalSink` or `TuiSink`. Ensure the
  dry-run path emits only the documented preview text and performs no
  `runtime/` directory creation, log creation, journal creation, locks, or
  markdown writes.
- Expected tests: add e2e tests for autonomous agent and program dry-runs that
  assert exit status, stdout preview text, unchanged plan files, and no
  `runtime/` directory or `runtime/transitions.log`.
- Reason preferable: the run spec is explicit that dry-run creates no runtime
  artifacts, and this behavior is important for clean CI and preview commands.
- Alternative: `update-spec` to allow an empty dry-run journal. This would
  match the current wiring but would make dry-run a mutating command.

## TM-E003: Journal lines describe spawn invocations rather than actual transitions

- Primary decision: `update-both`.
- Next edits: resolve the spec tension between "one line per state transition"
  and "one line per slot event" by defining `runtime/transitions.log` as a
  committed-transition journal. Add a transition event to `rhei-tui`, emit it
  only after `execute_transition` or callback advancement succeeds, and have
  `JournalSink` write the final `from -> to` state change. If invocation start
  and release records remain useful, document them as TUI-only events or write
  them to a separate invocation log rather than `transitions.log`.
- Expected tests: add e2e tests where a program or fake agent transitions
  `build -> completed` and the journal contains that transition, not
  `build -> build`. Include a no-transition failure case to assert no committed
  transition line is written.
- Reason preferable: users and tools need the journal to reconstruct actual
  plan movement. A separate transition event is the cleanest boundary between
  process monitoring and state history.
- Alternative: `update-implementation` to mutate `SlotReleased` so it carries
  the eventual transition target and keep the two-line slot journal. This is
  smaller but preserves the confusing mix of invocation and transition
  semantics.

## TM-E004: Journal columns are not fixed-width

- Primary decision: `update-implementation`.
- Next edits: update `JournalSink` formatting so timestamp, task id, and
  transition columns are padded to documented widths, with the path column
  starting at a stable offset. Keep timestamps at RFC 3339 second precision and
  keep metadata as trailing comma-separated key/value pairs.
- Expected tests: add `JournalSink` unit tests with short and long task ids and
  transitions that assert the path column offset and padding. Add an e2e
  journal-format assertion once TM-E013 coverage is added.
- Reason preferable: fixed-width output is a small implementation change and is
  part of the tail-friendly operator contract.
- Alternative: `update-spec` to define the current two-space separated format
  as normative. This is credible if machine parsing matters more than visual
  alignment, but it should be an explicit compatibility choice.

## TM-E005: Journal path requirements conflict internally

- Primary decision: `update-spec`.
- Next edits: update the TUI spec goals section to match the detailed journal
  rules: paths are workspace-relative when the log is inside the workspace and
  absolute otherwise. Mention that this keeps normal journals portable while
  still preserving absolute paths for external logs.
- Expected tests: retain existing `JournalSink` unit tests for relative
  in-workspace paths and add one unit test for an out-of-workspace absolute
  path if it is not already covered.
- Reason preferable: the implementation follows the more precise format rule,
  and relative in-workspace paths are better for moved workspaces, checked
  artifacts, and readable tail output.
- Alternative: `update-implementation` to always write absolute paths. This
  would satisfy the goals sentence but would contradict the current format
  section and make journals less portable.

## TM-E006: TUI log tailing via `notify` is not implemented

- Primary decision: `update-implementation`.
- Next edits: add file-backed log tailing to `rhei-tui`: depend on `notify`,
  start or retarget a watcher when a slot receives `log_path`, seed a bounded
  50-line ring buffer from the existing file contents, append newly written
  lines as change notifications arrive, and render the last five lines from
  that buffer. Keep `AgentOutput` as an optional low-latency supplement, but
  make the log file the source of truth shown in the tile.
- Expected tests: add TUI state/tailer tests that assign a slot to a temp log,
  append lines after assignment, and assert the displayed tail updates and
  remains bounded. Add program-run coverage showing program stdout/stderr
  appears in the TUI tail source even though programs do not emit
  `AgentOutput`.
- Reason preferable: the durable per-task log is the only complete transcript,
  especially for programs and direct log headers/footers. Tailing it makes the
  UI match what operators can inspect after the run.
- Alternative: `update-spec` to define `AgentOutput` as the only live TUI
  source and require all subprocess/log writers to mirror output into events.
  This avoids file watching but makes the durable log and displayed log easier
  to diverge.

## TM-E007: TUI slot layout and display rules are mostly unimplemented

- Primary decision: `update-both`.
- Next edits: update the event spec to include the data the renderer is
  required to show, especially a short task title on slot assignment or on a
  separate task metadata event. Then implement a layout strategy in
  `rhei-tui` for single-pane, 2x2, 3x3, and compact-list modes; reserve a
  persistent bottom pane for recent committed transitions; handle
  `crossterm::event::Event::Resize`; show current state from `SlotAssigned.to`;
  and keep idle-slot rendering.
- Expected tests: add renderer unit tests or snapshot-style buffer tests for
  N=1, N=4, N=9, N>=10, too-small terminals, resize recomputation, bottom
  transition pane content, task title display, current-state display, and idle
  slots.
- Reason preferable: the current vertical list is materially less useful for
  parallel monitoring, and the spec cannot be fully implemented until the event
  model carries the task title and transition-pane events.
- Alternative: `update-spec` to document the current vertical-list UI as the
  intended first implementation. This would reduce near-term work but would
  lower the UX target for the main monitoring surface.

## TM-E008: `--parallel 0` has unclear TUI slot semantics

- Primary decision: `update-both`.
- Next edits: define unlimited mode in the spec as a dynamically sized frontend
  pool: `--parallel 0` may schedule every ready task in a pass, the frontend
  grows to the maximum simultaneous in-flight slot index for that run, and the
  renderer uses compact-list mode once the active or allocated slot count is
  ten or more. Update the scheduler/frontend handshake so reported parallelism
  and slot assignment use the same effective slot count for each pass.
- Expected tests: add run/TUI tests for `--parallel 0` with multiple ready
  tasks, asserting all assigned slot indices are displayable, no unknown-slot
  events are dropped for valid scheduler output, and compact-list mode is used
  for large ready batches.
- Reason preferable: unlimited concurrency is already a documented scheduler
  mode, so the monitoring model should define how an unbounded ready batch maps
  to a bounded terminal surface.
- Alternative: `update-spec` to say `--parallel 0` disables the TUI or forces a
  single aggregate slot. This is simpler, but it makes unlimited mode the least
  observable mode.

## TM-E009: Frontend flag behavior is implemented, but CLI coverage is thin

- Primary decision: `update-implementation`.
- Next edits: add parser/help/frontend-selection tests for `--tui`,
  `--no-tui`, their mutual exclusion, help text inclusion, and the mapping from
  run options to `FrontendKind`. If needed, make terminal detection injectable
  in tests so auto mode can be asserted without depending on the test runner's
  TTY.
- Expected tests: new clap tests for flag parsing and conflict diagnostics,
  help-output assertions, and small `select_frontend` or run-command tests for
  forced TUI, forced stdout, and auto selection.
- Reason preferable: the behavior appears implemented; regression tests are the
  missing reconciliation.
- Alternative: `no-change` if maintainers consider existing manual coverage of
  the flags enough. This leaves a user-visible compatibility boundary mostly
  unprotected.

## TM-E010: Non-TTY stdout byte-for-byte compatibility is only partially tested

- Primary decision: `update-implementation`.
- Next edits: add golden-output tests for representative non-TTY autonomous
  runs using deterministic fixtures. Normalize only inherently variable data
  that was not part of the old line-oriented contract, or split such data into
  explicit wildcard assertions so spacing, ordering, and newline behavior stay
  byte-for-byte checked.
- Expected tests: golden tests for a successful program run, successful fake
  agent run, failure path, no-ready path, and dry-run path under `--no-tui` or
  non-TTY execution.
- Reason preferable: the spec makes a precise compatibility promise, and
  substring tests cannot catch whitespace, ordering, or lifecycle-message
  regressions.
- Alternative: `update-spec` to relax byte-for-byte compatibility to semantic
  compatibility. This is credible if existing output was never stable enough to
  freeze, but it should be a deliberate public contract change.

## TM-E011: Timeout outcomes are misclassified when a timeout is merely configured

- Primary decision: `update-implementation`.
- Next edits: change agent and program spawn helpers to return a structured
  outcome such as `Exited(ExitStatus)`, `TimedOut`, and `SpawnError`. Map only
  the actual timeout path to `TaskOutcome::TimedOut` and timeout transitions;
  map ordinary nonzero exits from timeout-configured states to
  `TaskOutcome::Failed` and normal error/exit-code routing.
- Expected tests: add fake agent and program tests where a state has a timeout
  configured but the subprocess exits nonzero before the deadline. Assert the
  TUI/journal outcome is failed, the timeout transition is not fired, and an
  actual sleep-past-timeout case still reports timeout.
- Reason preferable: timeout is an observed runtime outcome, not a property of
  the state configuration. Correct classification is critical for debugging and
  for routing through the right transitions.
- Alternative: `update-spec` to define any nonzero exit from a
  timeout-configured state as timeout. This would match the current shortcut
  but would make normal failures misleading.

## TM-E012: The reusable helper API shown in the spec does not exist

- Primary decision: `update-spec`.
- Next edits: replace the normative `run_with_frontend` / `RunParams` example
  with the currently exported `select_frontend`, `Frontend`, `FrontendKind`,
  `EventSink`, and event types, or mark the helper as a future convenience API
  rather than an implemented contract. Revisit the helper after the transition
  event model from TM-E001/TM-E003 is settled.
- Expected tests: no runtime behavior test is required for a spec-only cleanup.
  Add a docs/API smoke check later if the project starts testing public
  snippets.
- Reason preferable: the existing crate boundary already keeps terminal UI
  dependencies out of `rhei-cli`; the stale helper name is misleading but not
  blocking current monitoring behavior.
- Alternative: `update-implementation` to add `run_with_frontend` and
  `RunParams` now. This may be useful eventually, but adding a helper before
  the event semantics are reconciled risks freezing the wrong API.

## TM-E013: End-to-end journal behavior lacks coverage

- Primary decision: `update-implementation`.
- Next edits: add CLI e2e journal assertions around `rhei run` rather than only
  `JournalSink` unit tests. Cover journal creation, append behavior across two
  runs, fixed-width line shape after TM-E004, relative path formatting, actual
  committed transitions after TM-E003, release/outcome metadata if retained,
  callback-only mode after TM-E001, and absence under dry-run after TM-E002.
- Expected tests: new e2e fixtures for successful program transition,
  successful fake-agent transition, failing subprocess, callback-only
  transition, repeated append run, and dry-run no-artifact behavior.
- Reason preferable: the known journal failures happen in `rhei run` wiring,
  not inside the sink's isolated formatting logic. Integration coverage is the
  right guard for this contract.
- Alternative: `no-change` if maintainers treat journal unit tests as
  sufficient until the event model is redesigned. This leaves the highest-risk
  user-facing monitoring artifact without end-to-end protection.
