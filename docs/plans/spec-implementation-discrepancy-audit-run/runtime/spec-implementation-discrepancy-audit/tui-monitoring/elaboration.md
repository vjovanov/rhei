# Discrepancy Elaboration: TUI Monitoring

Task: `tui-monitoring`

Source discrepancy file: `runtime/spec-implementation-discrepancy-audit/tui-monitoring/discrepancies.md`

This elaboration consolidates duplicate findings, marks weaker findings as
tentative, and records areas where the audit found no discrepancy. It does not
choose a reconciliation strategy.

## Duplicate Merges

- TM-009 and TM-017 are one log-tailing mismatch: the TUI does not tail
  `log_path` via `notify`, and the dependency claim about reusing `notify` is
  therefore not implemented in the TUI crate.
- TM-012 contains two separate outcomes: the frontend flags and selection path
  exist, but parser/help/frontend-selection coverage for those flags is thin.
  The implemented behavior is recorded under no-discrepancy areas, and the
  coverage gap is elaborated below.
- TM-019 overlaps with TM-003 and TM-004 because end-to-end journal tests would
  catch dry-run artifact creation and invocation-vs-transition journal lines.
  It is kept separate as a test-coverage discrepancy because it affects the
  durability of multiple monitoring guarantees.

## Elaborated Discrepancies

### TM-E001: Callback-only runs bypass the TUI and journal event surface

Source findings: TM-002. Classification: `implementation-diverges`.

Exact mismatch: The TUI spec says both `run_agent_mode` and
`run_callback_mode` are refactored to emit `RunEvent`s through an `EventSink`,
and that the engine always writes through a `Tee` to `JournalSink` plus the
selected frontend. The implementation selects a frontend only in
`run_agent_mode`; callback-only runs print directly with `println!` /
`eprintln!` and do not open the journal.

Why it matters: Monitoring behavior depends on run mode. A callback-only plan
can advance state without producing `runtime/transitions.log`, without honoring
the `--tui` / `--no-tui` frontend path, and without the same event lifecycle as
autonomous runs.

Affected: Users of callback-only state machines, workflows run with spawning
disabled, non-TTY automation expecting a journal, and any tooling tailing
`runtime/transitions.log` as the run audit trail.

Risk: User-facing and internal. Users lose a promised runtime artifact, and
internal monitoring consumers cannot treat the event stream as universal.

Verification currently exists: The discrepancy file records a targeted CLI
observation where a callback-only run transitioned a task and created no
`runtime/` directory or transition journal. The audit did not identify tests
that require callback-only mode to use `select_frontend`, emit events, or write
the journal.

### TM-E002: Autonomous dry-run creates runtime artifacts

Source findings: TM-003. Classification: `implementation-diverges`.

Exact mismatch: The run spec says `--dry-run` creates no runtime artifacts.
Autonomous mode initializes the frontend before dry-run branching, and
`select_frontend` opens `JournalSink`, which creates `runtime/` and opens
`runtime/transitions.log` in append/create mode.

Why it matters: Dry-run is documented as a no-artifact preview. Creating
runtime files makes dry-run unsuitable as a clean planning/check command and
can dirty repositories, CI workspaces, or fixture directories.

Affected: Users running dry-run in source-controlled workspaces, CI checks,
tests that expect no filesystem mutation, and scripts using dry-run as a safe
preview.

Risk: User-facing.

Verification currently exists: The discrepancy file records an observed
autonomous dry-run that exited `0` and left `runtime/` plus an empty
`runtime/transitions.log`. Existing dry-run tests check stdout and plan-file
stability, but not absence of runtime artifacts.

### TM-E003: Journal lines describe spawn invocations rather than actual transitions

Source findings: TM-004. Classification: `implementation-diverges`.

Exact mismatch: The spec describes `runtime/transitions.log` as a transition
journal where lines carry the transition, such as `agent-review -> completed`.
The implementation emits `SlotAssigned` before spawn with `from` and `to` both
derived from the current state, then reuses those same values for
`SlotReleased`. A task that actually transitions from `build` to `completed`
can therefore journal `build->build` for both assignment and release.

Why it matters: The journal cannot be used to reconstruct state changes. It is
an invocation log with exit metadata, not the transition journal described by
the spec.

Affected: Users inspecting `runtime/transitions.log`, tools tailing or parsing
the journal, debugging workflows, and any future monitoring feature that treats
the journal as state-transition evidence.

Risk: User-facing and internal.

Verification currently exists: The discrepancy file records a targeted CLI
observation where a successful `build -> completed` program wrote two
`build->build` journal lines while the plan ended in `completed`. Journal unit
tests assert assignment/release line shape, transition substrings, relative
paths, and release metadata, but do not assert that the journal records the
eventual committed transition.

### TM-E004: Journal columns are not fixed-width

Source findings: TM-005. Classification: `implementation-diverges`.

Exact mismatch: The journal format says columns 1-3 are fixed-width. The
implementation writes fields separated by two spaces, without padding the task
or transition columns to fixed widths.

Why it matters: Fixed-width columns are part of the tail-friendly operator
contract. Without alignment, human scanning and simple column-oriented shell
processing are less reliable, especially with mixed task id and transition
lengths.

Affected: Users reading `tail -f runtime/transitions.log`, terminal monitoring
workflows, and parsers that expect stable columns.

Risk: Mostly user-facing.

Verification currently exists: `JournalSink` unit tests cover line count,
transition substrings, relative path suffixes, release metadata, and append
behavior. They do not assert fixed-width alignment.

### TM-E005: Journal path requirements conflict internally

Source findings: TM-006. Classification: `ambiguous-spec`.

Status: Tentative.

Exact mismatch: The goals section says every journal line carries the absolute
path to the detailed log. The journal-format rules later say paths are
workspace-relative when inside the workspace and absolute otherwise. The
implementation follows the later, more specific rule by stripping the workspace
root for in-workspace paths.

Why it matters: The spec gives two incompatible path contracts. Consumers
cannot know whether they should expect absolute paths universally or
workspace-relative paths for normal run logs.

Affected: Journal readers, shell tooling, documentation, and any tests or
integrations validating journal path format.

Risk: User-facing and internal. The implementation appears consistent with one
section of the spec, so this is not a pure implementation bug.

Verification currently exists: Journal unit tests expect a relative path ending
in `runtime/logs/task-1-pending.log`. The audit did not identify a test or
normative clarification resolving the goals-vs-format conflict.

### TM-E006: TUI log tailing via `notify` is not implemented

Source findings: TM-009, TM-017. Classification: `implementation-diverges`.

Exact mismatch: The spec says each tile shows the last five lines of the log
file at `log_path`, tailed via `notify` with a bounded 50-line ring buffer. The
TUI stores `log_path` on assignment, but does not open, watch, or tail that
file. The displayed tail comes only from in-memory `AgentOutput` events, and
`rhei-tui` does not depend on `notify`.

Why it matters: The TUI misses output that is written directly to per-task log
files. Program stdout/stderr are redirected to the program log file without
emitting `AgentOutput`, and agent log headers/footers written directly to the
log are not necessarily displayed.

Affected: Users watching program states in the TUI, users expecting complete
per-task log tails, operators relying on the tile view during long runs, and
the dependency/API boundary promised by the TUI spec.

Risk: User-facing.

Verification currently exists: TUI unit tests cover in-memory `AgentOutput`
retention, unknown-slot safety, sanitization, truncation, and row reservation.
The audit did not identify tests that write to `log_path` and require the TUI
to tail those file changes, nor any `notify`-backed TUI implementation.

### TM-E007: TUI slot layout and display rules are mostly unimplemented

Source findings: TM-010. Classification: `implementation-diverges`.

Exact mismatch: The spec requires single-pane, 2x2, 3x3, and compact-list
layout modes based on `--parallel N` and terminal size; a persistent bottom
transition journal pane; resize-event handling; task id plus short title;
current state from `SlotAssigned.to`; elapsed time; log tail; and idle slots.
The implementation renders all slots as a vertical list in one pane, ignores
explicit resize events while relying on redraw frame size, shows an in-memory
mixed UI journal rather than a persistent transition-only journal pane, cannot
show task title because `SlotAssigned` lacks it, and displays a `from->to`
string instead of just the current `to` state.

Why it matters: The interactive surface is materially different from the
specified dashboard. It is less useful for parallel monitoring, less aligned
with the journal semantics, and cannot show all specified task context.

Affected: Interactive terminal users, demos, operators watching parallel runs,
and future reusable TUI consumers.

Risk: User-facing.

Verification currently exists: Unit tests cover several narrow TUI state and
rendering helpers, including bounded traffic retention, unknown-slot handling,
control-sequence sanitization, truncation, and row reservation. Idle-slot
display and the too-small terminal warning path are implemented. The audit did
not identify tests for 2x2/3x3 layout selection, compact-list mode, explicit
resize events, persistent transition-only journal pane, task title display, or
`SlotAssigned.to` state display.

### TM-E008: `--parallel 0` has unclear TUI slot semantics

Source findings: TM-011. Classification: `ambiguous-spec`.

Status: Tentative.

Exact mismatch: The run spec defines `--parallel 0` as unlimited, while the TUI
spec says the renderer allocates a fixed pool of `N` slots matching
`--parallel N`. A fixed pool of zero slots is unusable, but the spec does not
say whether unlimited mode should size slots from the current ready batch or
grow dynamically. The implementation reports at least one frontend slot for
`--parallel 0`, while the scheduler can assign slot indices from the whole
ready batch, potentially beyond the initialized TUI slot vector.

Why it matters: Unlimited concurrency can produce slot events the TUI cannot
display. The right behavior cannot be classified cleanly until the spec defines
how an unlimited scheduler maps to a fixed-slot UI.

Affected: Users running `rhei run --parallel 0`, TUI rendering, journal slot
metadata, and any monitoring code assuming slot indices are bounded by the
frontend's reported parallel count.

Risk: User-facing, but tentative because the spec is underspecified.

Verification currently exists: The discrepancy file records implementation
evidence for minimum-one frontend slot reporting and scheduler slot assignment
by enumeration. The audit did not identify tests for `--parallel 0` with the
TUI or journal slot indexing.

### TM-E009: Frontend flag behavior is implemented, but CLI coverage is thin

Source findings: TM-012. Classification: `missing-test`.

Exact mismatch: The implementation defines `--tui` and `--no-tui`, makes them
mutually exclusive, maps them to `FrontendKind`, and uses terminal detection in
`select_frontend`. The test suite does not appear to assert parser handling for
those flags, help output inclusion, mutual exclusion, or frontend-selection
behavior.

Why it matters: Frontend selection is a user-visible compatibility boundary.
Thin coverage makes it easier to regress the override flags, conflict rules, or
TTY/non-TTY default behavior without detection.

Affected: CLI users, scripted demos, CI/non-TTY users, and maintainers changing
the run command parser or frontend wiring.

Risk: Mostly internal until a regression ships; then user-facing.

Verification currently exists: Parser tests cover several other run flags, and
`select_frontend` implementation evidence shows the intended mapping. The audit
did not identify direct tests for `--tui`, `--no-tui`, mutual exclusion, help
text, or frontend selection.

### TM-E010: Non-TTY stdout byte-for-byte compatibility is only partially tested

Source findings: TM-013. Classification: `missing-test`.

Exact mismatch: The spec requires non-TTY output without `--tui` to match the
previous line-oriented format byte-for-byte. The implementation routes
human-readable autonomous output through `RunEvent::Message`, and `StdoutSink`
prints only message events. Existing tests are substring-oriented rather than
golden byte-for-byte compatibility tests.

Why it matters: Substring tests can miss ordering, spacing, lifecycle-message,
and newline regressions. The implementation may be compatible, but the
strongest compatibility claim is not verified at the required precision.

Affected: CI users, scripts grepping or diffing stdout, users piping `rhei run`
output, and maintainers refactoring event emission.

Risk: Internal coverage risk with user-facing consequences if stdout format
regresses.

Verification currently exists: Dry-run and e2e run tests assert selected stdout
fragments and file states. The audit did not identify byte-for-byte golden
tests for non-TTY autonomous stdout.

### TM-E011: Timeout outcomes are misclassified when a timeout is merely configured

Source findings: TM-014. Classification: `implementation-diverges`.

Exact mismatch: The event surface distinguishes `TaskOutcome::Failed(String)`
from `TaskOutcome::TimedOut`. The implementation maps any non-success exit from
a state with a configured timeout to `TimedOut`, even if the process exited
before the timeout with a normal nonzero status. The spawn helpers return only
an `ExitStatus`, not a flag indicating whether timeout handling actually fired.

Why it matters: Monitoring can report ordinary failures as timeouts. TUI
symbols, journal `outcome=timeout` metadata, failure summaries, and debugging
signals can all point to the wrong cause.

Affected: Users debugging failed agent/program states, workflows with
configured timeouts, journal consumers, and failure/timeout transition
monitoring.

Risk: User-facing.

Verification currently exists: Tests cover that timed-out fake agents retain
output and write a footer, plus inherited output pipe behavior. The audit did
not identify tests that distinguish a configured-timeout nonzero exit from an
actual timeout in emitted `TaskOutcome` or journal metadata.

### TM-E012: The reusable helper API shown in the spec does not exist

Source findings: TM-016. Classification: `spec-stale`.

Status: Tentative / spec-stale.

Exact mismatch: The spec shows future parallel subcommands using
`rhei_tui::run_with_frontend` and `RunParams`. The crate exports
`select_frontend`, `Frontend`, and `FrontendKind`, and repository search found
no `run_with_frontend` or `RunParams` implementation or call sites. The broader
crate boundary is implemented: `rhei-tui` is standalone and `rhei-cli` depends
on it without directly depending on terminal UI crates.

Why it matters: Reuse guidance in the spec does not match the public API.
Future subcommand authors cannot follow the documented helper pattern.

Affected: Rhei maintainers, future parallel subcommands, API docs, and tests or
examples that would exercise the reusable frontend boundary.

Risk: Mostly internal/API-facing. It is not evidence that current `rhei run`
monitoring fails, so the finding is weaker than runtime mismatches.

Verification currently exists: Crate manifests and `rhei-tui` exports support
the standalone boundary. The discrepancy file records that repository search
found no `run_with_frontend` or `RunParams` symbols.

### TM-E013: End-to-end journal behavior lacks coverage

Source findings: TM-019. Classification: `missing-test`.

Exact mismatch: The spec makes the journal a persistent, append-only,
tail-friendly runtime artifact with specific creation, formatting, and dry-run
absence semantics. The journal sink has unit tests, but `rhei run`
end-to-end tests do not assert journal creation, append behavior, line format,
transition correctness, or absence during dry-run.

Why it matters: Several journal mismatches are integration-level failures: dry
run opens the journal before the dry-run branch, and run-mode code emits
current-state invocation data instead of final transitions. Unit tests for the
sink alone cannot catch those wiring errors.

Affected: CLI integration behavior, CI coverage, maintainers refactoring
`rhei run`, and users relying on `runtime/transitions.log`.

Risk: Internal coverage risk with direct user-facing consequences.

Verification currently exists: `JournalSink` unit tests cover assignment and
release lines plus appending across opens. E2e run tests cover stdout fragments,
final task states, program artifacts, and fixture logs. Dry-run tests check
stdout and plan immutability. The audit did not identify e2e assertions for
`runtime/transitions.log`.

## Areas With No Discrepancy Found

- Autonomous run event surface exists. `rhei-tui` defines `RunEvent`,
  `TaskOutcome`, `AgentStream`, `EventSink`, `Tee`, and autonomous
  `run_agent_mode` emits run/pass/slot/output/finish events.
- Journal append and error-handling behavior exists. `JournalSink` creates
  missing parent directories, opens `runtime/transitions.log` in append mode,
  writes UTF-8 newline-delimited lines, flushes each line, includes release
  metadata, and warns rather than aborting on write/flush failure.
- Built-in agent traffic capture matches the scoped shared-pipe requirement for
  `claude-code`, `codex`, and `pi`: prompt transport is agent-specific, while
  stdout/stderr capture is shared, logged, flushed, and emitted as
  line-oriented `AgentOutput`.
- Frontend flags are present and wired. `--tui` and `--no-tui` exist,
  conflict with each other, map to `FrontendKind`, and `select_frontend` uses
  `std::io::IsTerminal` for auto mode while composing the chosen frontend with
  the journal sink. The coverage gap for these flags is recorded separately.
- TUI lifecycle preservation is mostly implemented. Lifecycle events use
  blocking sends while high-volume `AgentOutput` is best-effort, raw mode and
  alternate-screen setup have cleanup paths, panic cleanup performs equivalent
  crossterm restoration, Ctrl+C restores the terminal and re-raises `SIGINT`,
  and unit tests cover Ctrl+C handling.
- Target/model fanout log suffixes are reflected in log paths. The log path
  builder accepts suffixes, target slug is preferred before model id, and
  agent fanout call sites pass the suffix through so multi-invocation states
  can produce distinct log paths.
- Some TUI display safeguards are present even though the full layout spec is
  not implemented: idle slots render, too-small terminals avoid crashing by
  drawing a warning, display traffic is bounded, control sequences are
  sanitized, and long lines are truncated for rendering.
