# Discrepancy Audit: TUI Monitoring

Partition: `tui-monitoring`

Audit target: `rhei run` monitoring, TUI, transition journal, stdout compatibility,
agent traffic capture, slot lifecycle, log tailing, non-TTY behavior, and
failure/timeout display, scoped by
`runtime/spec-implementation-discrepancy-audit/tui-monitoring/scope.md`.

## TM-001: TUI/Journal Event Surface Exists for Autonomous Runs

Classification: no-discrepancy

The implementation has a standalone `rhei-tui` crate in the workspace and exposes
the core event/sink types required by the spec. `RunEvent` includes run/pass
lifecycle events, `SlotAssigned`, `SlotReleased`, `AgentOutput`, and
`RunFinished`; `TaskOutcome` distinguishes `Completed`, `Failed`, `Cancelled`,
and `TimedOut`; `AgentStream` distinguishes stdout/stderr; and `EventSink` is
`Send + Sync` (`crates/rhei-tui/src/event.rs:14`,
`crates/rhei-tui/src/event.rs:40`, `crates/rhei-tui/src/event.rs:53`,
`crates/rhei-tui/src/event.rs:105`). `Tee` forwards each event to fixed inner
sinks in order (`crates/rhei-tui/src/event.rs:111`). `run_agent_mode` constructs
the frontend once at startup and emits `RunStarted`, `PassStarted`,
`PassEnded`, and `RunFinished` around the execution loop
(`crates/rhei-cli/src/main.rs:7374`, `crates/rhei-cli/src/main.rs:7382`,
`crates/rhei-cli/src/main.rs:7450`, `crates/rhei-cli/src/main.rs:8366`,
`crates/rhei-cli/src/main.rs:8423`).

## TM-002: Callback-Only `rhei run` Bypasses the Event Surface

Classification: implementation-diverges

The TUI spec says the existing `run_agent_mode` / `run_callback_mode` logic is
refactored to emit events through `EventSink`, and that the engine always writes
through a `Tee` to `JournalSink` plus a frontend sink
(`docs/specs/rhei-run-tui.spec.md:24`, `docs/specs/rhei-run-tui.spec.md:25`).
The implementation only selects a frontend in `run_agent_mode`
(`crates/rhei-cli/src/main.rs:7374`). If the machine has no autonomous execution
states, `run_command` prints the header directly and calls `run_callback_mode`
without `select_frontend` (`crates/rhei-cli/src/main.rs:7334`,
`crates/rhei-cli/src/main.rs:7343`, `crates/rhei-cli/src/main.rs:7351`).
`run_callback_mode` still uses direct `println!` / `eprintln!` calls for pass
output, dry-run output, transitions, and summaries
(`crates/rhei-cli/src/main.rs:8461`, `crates/rhei-cli/src/main.rs:8484`,
`crates/rhei-cli/src/main.rs:8510`, `crates/rhei-cli/src/main.rs:8552`).

Practical CLI behavior matches the code path: a callback-only run can transition
a task to `completed` without creating `runtime/transitions.log`, so `--tui`,
`--no-tui`, and the journal surface do not apply to that mode. This conflicts
with the spec's "always writes through a Tee" and "journal is always written"
language (`docs/specs/rhei-run-tui.spec.md:29`,
`docs/specs/rhei-run-tui.spec.md:82`).

## TM-003: Dry Run Creates Runtime Artifacts in Autonomous Mode

Classification: implementation-diverges

The run spec says `--dry-run` creates no runtime artifacts
(`docs/specs/rhei-run.spec.md:73`, `docs/specs/rhei-run.spec.md:79`).
Autonomous mode initializes the frontend before the dry-run branches
(`crates/rhei-cli/src/main.rs:7374`), and `select_frontend` opens the journal
unconditionally (`crates/rhei-tui/src/frontend.rs:51`). `JournalSink::open`
creates `<workspace>/runtime` and opens `<workspace>/runtime/transitions.log` in
append/create mode (`crates/rhei-tui/src/journal.rs:24`,
`crates/rhei-tui/src/journal.rs:29`, `crates/rhei-tui/src/journal.rs:32`).

Observed behavior: running `target/debug/rhei run <program-plan> --dry-run`
against a program-driven plan exited `0` and left both `runtime/` and an empty
`runtime/transitions.log`. The stdout showed only planned spawn information, but
the filesystem still gained runtime artifacts.

## TM-004: Journal Records Spawn Invocations, Not Actual State Transitions

Classification: implementation-diverges

The journal is specified as a transition journal: each line carries the
transition (`from -> to`) and the detailed log path
(`docs/specs/rhei-run-tui.spec.md:10`, `docs/specs/rhei-run-tui.spec.md:120`).
The example release line records the final transition into `completed`
(`docs/specs/rhei-run-tui.spec.md:127`). Implementation emits `SlotAssigned`
before spawning with `from` equal to the task's current state and `to` equal to
the normalized current state, not the eventual transition target
(`crates/rhei-cli/src/main.rs:7673`, `crates/rhei-cli/src/main.rs:7676`,
`crates/rhei-cli/src/main.rs:7677`, `crates/rhei-cli/src/main.rs:7959`,
`crates/rhei-cli/src/main.rs:7962`, `crates/rhei-cli/src/main.rs:7963`,
`crates/rhei-cli/src/main.rs:8186`, `crates/rhei-cli/src/main.rs:8189`,
`crates/rhei-cli/src/main.rs:8190`). `SlotReleased` uses the same `from` and
`to` values captured before spawn (`crates/rhei-cli/src/main.rs:7702`,
`crates/rhei-cli/src/main.rs:7705`, `crates/rhei-cli/src/main.rs:7706`,
`crates/rhei-cli/src/main.rs:8000`, `crates/rhei-cli/src/main.rs:8003`,
`crates/rhei-cli/src/main.rs:8004`, `crates/rhei-cli/src/main.rs:8249`,
`crates/rhei-cli/src/main.rs:8252`, `crates/rhei-cli/src/main.rs:8253`).

Observed behavior for a successful program state `build -> completed`:

```text
2026-05-03T19:40:24Z  1  build→build  runtime/logs/task-1-build.log
2026-05-03T19:40:24Z  1  build→build  runtime/logs/task-1-build.log  exit=0,duration=3ms,outcome=completed
```

The plan state changed to `completed`, but the journal did not contain
`build→completed`. This makes `runtime/transitions.log` an invocation log rather
than the transition journal described by the spec.

## TM-005: Journal Line Format Is Not Fixed-Width

Classification: implementation-diverges

The spec says columns 1-3 are fixed-width, column 4 is a path, and optional
metadata follows (`docs/specs/rhei-run-tui.spec.md:122`). `JournalSink` formats
lines with two spaces between fields but no fixed-width padding for timestamp,
task, or transition columns (`crates/rhei-tui/src/journal.rs:78`,
`crates/rhei-tui/src/journal.rs:81`, `crates/rhei-tui/src/journal.rs:84`,
`crates/rhei-tui/src/journal.rs:110`). Unit tests only assert line count,
transition substring, relative-path suffix, and release metadata; they do not
assert fixed-width alignment (`crates/rhei-tui/src/journal.rs:176`,
`crates/rhei-tui/src/journal.rs:205`).

## TM-006: Journal Path Requirements Conflict Internally

Classification: ambiguous-spec

The goals section says every journal line carries the absolute path to the
detailed log (`docs/specs/rhei-run-tui.spec.md:10`). The journal format rules
later say paths are workspace-relative if inside the workspace, otherwise
absolute (`docs/specs/rhei-run-tui.spec.md:133`). Implementation follows the
later rule by stripping `workspace_root` when possible
(`crates/rhei-tui/src/journal.rs:57`, `crates/rhei-tui/src/journal.rs:59`), and
the journal unit test expects a relative path ending in
`runtime/logs/task-1-pending.log` (`crates/rhei-tui/src/journal.rs:181`,
`crates/rhei-tui/src/journal.rs:209`).

## TM-007: Journal Append, Flush, Metadata, and Error Handling

Classification: no-discrepancy

The append/tail-friendly parts of the journal spec are implemented. The sink
creates missing parent directories, opens `runtime/transitions.log` in append
mode, writes UTF-8 text lines, flushes each line, and converts write/flush
failures into warnings instead of aborting the run
(`crates/rhei-tui/src/journal.rs:24`, `crates/rhei-tui/src/journal.rs:30`,
`crates/rhei-tui/src/journal.rs:32`, `crates/rhei-tui/src/journal.rs:39`,
`crates/rhei-tui/src/journal.rs:45`, `crates/rhei-tui/src/journal.rs:50`).
Release lines include comma-separated `exit`, `duration`, and `outcome`
metadata when available (`crates/rhei-tui/src/journal.rs:103`,
`crates/rhei-tui/src/journal.rs:107`, `crates/rhei-tui/src/journal.rs:108`).
Unit tests cover assignment/release output and appending across multiple opens
(`crates/rhei-tui/src/journal.rs:176`, `crates/rhei-tui/src/journal.rs:216`).

## TM-008: Agent Traffic Capture Matches the Shared-Pipe Requirement

Classification: no-discrepancy

Built-in prompt transports match the scoped spec for `claude-code`, `codex`, and
`pi`: `claude-code` and `pi` use `-p`, while `codex` uses stdin prompt delivery
with no prompt flag (`crates/rhei-cli/src/main.rs:5592`,
`crates/rhei-cli/src/main.rs:5596`, `crates/rhei-cli/src/main.rs:5598`,
`crates/rhei-cli/src/main.rs:5610`, `crates/rhei-cli/src/main.rs:5613`,
`crates/rhei-cli/src/main.rs:5615`, `crates/rhei-cli/src/main.rs:5676`,
`crates/rhei-cli/src/main.rs:5679`, `crates/rhei-cli/src/main.rs:5681`).
`build_agent_command` pipes stdin for stdin-prompt agents and appends `--`
(`crates/rhei-cli/src/main.rs:6651`, `crates/rhei-cli/src/main.rs:6661`);
`spawn_and_wait_agent` pipes stdout/stderr for every resolved agent and starts a
shared reader path for both streams (`crates/rhei-cli/src/main.rs:6924`,
`crates/rhei-cli/src/main.rs:6929`, `crates/rhei-cli/src/main.rs:6939`).

Each reader writes the raw bytes it read to the per-task log, flushes, and emits
a line-oriented `AgentOutput` event with the stream id
(`crates/rhei-cli/src/main.rs:6787`, `crates/rhei-cli/src/main.rs:6803`,
`crates/rhei-cli/src/main.rs:6808`, `crates/rhei-cli/src/main.rs:6813`). Tests
cover complete and partial lines, prompt transports, timeout output retention,
and inherited pipe handles (`crates/rhei-cli/src/main.rs:11989`,
`crates/rhei-cli/src/main.rs:12029`, `crates/rhei-cli/src/main.rs:12161`,
`crates/rhei-cli/src/main.rs:12187`, `crates/rhei-cli/src/main.rs:12210`,
`crates/rhei-cli/src/main.rs:12236`, `crates/rhei-cli/src/main.rs:12286`).

## TM-009: TUI Does Not Tail Log Files via `notify`

Classification: implementation-diverges

The TUI spec says each tile shows the last 5 lines of the log file at
`log_path`, tailed via the `notify` crate with a bounded 50-line ring buffer
(`docs/specs/rhei-run-tui.spec.md:112`, `docs/specs/rhei-run-tui.spec.md:116`,
`docs/specs/rhei-run-tui.spec.md:144`). The TUI state stores `log_path` on slot
assignment, but the renderer never opens or watches that path
(`crates/rhei-tui/src/tui.rs:35`, `crates/rhei-tui/src/tui.rs:87`,
`crates/rhei-tui/src/tui.rs:95`). The only displayed tail comes from
`AgentOutput` events retained in `SlotState.traffic`
(`crates/rhei-tui/src/tui.rs:112`, `crates/rhei-tui/src/tui.rs:119`,
`crates/rhei-tui/src/tui.rs:407`). `rhei-tui` does not depend on `notify` at
all (`crates/rhei-tui/Cargo.toml:7`); `notify` is currently a direct
`rhei-cli` dependency (`crates/rhei-cli/Cargo.toml:20`).

Consequences in scoped behavior:

- Program stdout/stderr are redirected directly to the program log file and do
  not emit `AgentOutput` (`crates/rhei-cli/src/main.rs:7153`,
  `crates/rhei-cli/src/main.rs:7157`), so the TUI has no live program log tail.
- Agent log headers/footers written directly to the log file are also not shown
  unless mirrored as `AgentOutput` events (`crates/rhei-cli/src/main.rs:6878`,
  `crates/rhei-cli/src/main.rs:7000`).

## TM-010: TUI Layout Rules Are Mostly Unimplemented

Classification: implementation-diverges

The spec requires layout modes for N=1, 2x2 grids for N=2-4, 3x3 grids for
N=5-9, compact list mode when rows-per-tile is too small or N>=10, a bottom
journal pane showing recent transitions, explicit resize handling, task id plus
short title, current state from `SlotAssigned.to`, elapsed time, and idle slots
(`docs/specs/rhei-run-tui.spec.md:98`, `docs/specs/rhei-run-tui.spec.md:102`,
`docs/specs/rhei-run-tui.spec.md:110`, `docs/specs/rhei-run-tui.spec.md:112`).

Implementation allocates a fixed slot vector, but renders all slots as a single
vertical list inside one `slots` pane regardless of N
(`crates/rhei-tui/src/tui.rs:55`, `crates/rhei-tui/src/tui.rs:60`,
`crates/rhei-tui/src/tui.rs:364`, `crates/rhei-tui/src/tui.rs:376`). The input
loop only handles key events and ignores `crossterm` resize events, though each
draw uses the current frame size (`crates/rhei-tui/src/tui.rs:273`,
`crates/rhei-tui/src/tui.rs:274`, `crates/rhei-tui/src/tui.rs:323`). The bottom
pane is an in-memory UI journal containing run messages and agent traffic, not a
persistent transition-only journal (`crates/rhei-tui/src/tui.rs:125`,
`crates/rhei-tui/src/tui.rs:130`, `crates/rhei-tui/src/tui.rs:143`,
`crates/rhei-tui/src/tui.rs:483`). The slot label contains task id plus agent id
when present, but no task title because `SlotAssigned` does not carry one
(`crates/rhei-tui/src/event.rs:63`, `crates/rhei-tui/src/tui.rs:390`,
`crates/rhei-tui/src/tui.rs:393`). Active slots display the `from→to` transition
string rather than just the current `to` state (`crates/rhei-tui/src/tui.rs:96`,
`crates/rhei-tui/src/tui.rs:389`, `crates/rhei-tui/src/tui.rs:402`).

Idle-slot display is implemented (`crates/rhei-tui/src/tui.rs:421`), and the
too-small terminal path avoids crashing by drawing a single warning line
(`crates/rhei-tui/src/tui.rs:323`, `crates/rhei-tui/src/tui.rs:325`).

## TM-011: `--parallel 0` Has No Clear TUI Slot Semantics

Classification: ambiguous-spec

The run spec defines `--parallel 0` as unlimited
(`docs/specs/rhei-run.spec.md:24`), while the TUI spec says the renderer
allocates a fixed pool of N slots matching `--parallel N`
(`docs/specs/rhei-run-tui.spec.md:100`). A fixed pool of zero slots is not
useful, and the spec does not say whether unlimited mode should allocate slots
from the current ready batch or grow dynamically.

The implementation reports at least one frontend slot for `--parallel 0`
(`crates/rhei-cli/src/main.rs:7374`), while the agent scheduler may spawn the
entire ready batch and assign slots by `enumerate()` (`crates/rhei-cli/src/main.rs:7880`,
`crates/rhei-cli/src/main.rs:8136`, `crates/rhei-cli/src/main.rs:8182`). Since
`UiState` initializes its slot vector from the reported parallel count
(`crates/rhei-tui/src/tui.rs:55`, `crates/rhei-tui/src/tui.rs:60`), unlimited
mode can emit slot indices that have no corresponding TUI tile. The spec needs a
clear rule before this can be classified as a pure implementation bug.

## TM-012: Frontend Flags Are Implemented, but CLI Coverage Is Thin

Classification: no-discrepancy

`--tui` and `--no-tui` are present on `rhei run`, and Clap marks `--tui` as
conflicting with `no_tui` (`crates/rhei-cli/src/main.rs:5414`,
`crates/rhei-cli/src/main.rs:5415`, `crates/rhei-cli/src/main.rs:5417`).
`RunOptions::frontend_kind` maps `--tui` to `FrontendKind::Tui`, `--no-tui` to
`FrontendKind::Stdout`, and neither flag to auto mode
(`crates/rhei-cli/src/main.rs:5476`). `select_frontend` uses
`std::io::stdout().is_terminal()` for auto mode and composes the chosen frontend
with the journal sink (`crates/rhei-tui/src/frontend.rs:45`,
`crates/rhei-tui/src/frontend.rs:51`, `crates/rhei-tui/src/frontend.rs:63`,
`crates/rhei-tui/src/frontend.rs:77`).

Classification: missing-test

The CLI parser test exercises `--dry-run`, `--no-callbacks`,
`--continue-on-error`, `--parallel`, `--no-agent`, `--agent`, and `--model`, but
not `--tui` or `--no-tui` (`crates/rhei-cli/src/main.rs:11254`,
`crates/rhei-cli/src/main.rs:11260`, `crates/rhei-cli/src/main.rs:11265`).
The help test checks `--dry-run` and `--parallel`, but does not assert the TUI
flags are present (`crates/rhei-cli/src/main.rs:11290`,
`crates/rhei-cli/src/main.rs:11298`). No CLI test in
`crates/rhei-cli/tests` asserts mutual exclusion or frontend selection.

## TM-013: Non-TTY Stdout Compatibility Is Only Partially Tested

Classification: missing-test

The spec requires non-TTY output without `--tui` to match the current
line-oriented format byte-for-byte (`docs/specs/rhei-run-tui.spec.md:175`,
`docs/specs/rhei-run-tui.spec.md:181`). In autonomous mode, the implementation
routes human-readable output through `RunEvent::Message`; `StdoutSink` only
prints `Message` events and ignores lifecycle events
(`crates/rhei-cli/src/main.rs:7388`, `crates/rhei-tui/src/stdout.rs:25`,
`crates/rhei-tui/src/stdout.rs:27`). This design can preserve the old text, but
the existing tests are substring-oriented rather than byte-for-byte golden
assertions. Examples include dry-run assertions that check selected substrings
and file content only (`crates/rhei-cli/tests/integration_markdown_plans.rs:2027`,
`crates/rhei-cli/tests/integration_markdown_plans.rs:2033`,
`crates/rhei-cli/tests/integration_markdown_plans.rs:2039`) and e2e run tests
that check selected stdout fragments (`crates/rhei-cli/tests/e2e/run_tests.rs:13`,
`crates/rhei-cli/tests/e2e/run_tests.rs:18`,
`crates/rhei-cli/tests/e2e/run_tests.rs:23`).

## TM-014: Timeout Outcomes Are Misclassified When a Timeout Is Merely Configured

Classification: implementation-diverges

The event surface requires `TaskOutcome::Failed(String)` and
`TaskOutcome::TimedOut` to be distinguishable (`docs/specs/rhei-run-tui.spec.md:52`,
`docs/specs/rhei-run-tui.spec.md:53`). Implementation maps any non-success exit
from a state with `timeout_secs.is_some()` to `TaskOutcome::TimedOut`, even when
the process exited before the timeout with a normal nonzero status
(`crates/rhei-cli/src/main.rs:7687`, `crates/rhei-cli/src/main.rs:7692`,
`crates/rhei-cli/src/main.rs:7985`, `crates/rhei-cli/src/main.rs:7990`,
`crates/rhei-cli/src/main.rs:8233`, `crates/rhei-cli/src/main.rs:8239`).
`spawn_and_wait_agent` and `spawn_and_wait_program` return only `ExitStatus`,
not a timeout flag, so the monitoring layer cannot accurately distinguish
configured timeout from actual timeout (`crates/rhei-cli/src/main.rs:6854`,
`crates/rhei-cli/src/main.rs:7117`). This affects TUI symbols and journal
`outcome=timeout` metadata.

## TM-015: TUI Failure-Mode Lifecycle Preservation Is Mostly Implemented

Classification: no-discrepancy

The TUI preserves lifecycle events better than high-volume agent output:
`AgentOutput` uses `try_send`, while non-output events use blocking `send`
(`crates/rhei-tui/src/tui.rs:222`, `crates/rhei-tui/src/tui.rs:224`,
`crates/rhei-tui/src/tui.rs:229`). `TuiSink::start` enables raw mode, enters the
alternate screen, and installs a panic hook that disables raw mode and leaves the
alternate screen before delegating to the previous hook
(`crates/rhei-tui/src/tui.rs:174`, `crates/rhei-tui/src/tui.rs:176`,
`crates/rhei-tui/src/tui.rs:178`, `crates/rhei-tui/src/tui.rs:182`). `finish`
and `Drop` send shutdown and join the render thread (`crates/rhei-tui/src/tui.rs:203`,
`crates/rhei-tui/src/tui.rs:216`). Ctrl+C in raw mode is handled by restoring
the terminal and raising `SIGINT` on the process (`crates/rhei-tui/src/tui.rs:270`,
`crates/rhei-tui/src/tui.rs:282`, `crates/rhei-tui/src/tui.rs:298`,
`crates/rhei-tui/src/tui.rs:313`). Unit tests cover Ctrl+C handling and ignoring
non-Ctrl+C input (`crates/rhei-tui/src/tui.rs:528`,
`crates/rhei-tui/src/tui.rs:541`).

The implementation does not call a literal `ratatui::restore()` as the spec
phrases it (`docs/specs/rhei-run-tui.spec.md:141`), but it performs the
equivalent crossterm cleanup operations.

## TM-016: Reusable Crate Boundary Exists, but the Specified Helper API Does Not

Classification: spec-stale

The boundary requirement that `rhei-tui` not depend on `rhei-cli` is satisfied:
`rhei-tui` exposes event, frontend, journal, stdout, and TUI APIs without a
`rhei-cli` dependency (`crates/rhei-tui/Cargo.toml:7`,
`crates/rhei-tui/src/lib.rs:11`), and `rhei-cli` depends on `rhei-tui`
(`crates/rhei-cli/Cargo.toml:11`). `rhei-cli` does not directly depend on
`ratatui` or `crossterm` (`crates/rhei-cli/Cargo.toml:7`).

The exact helper API in the spec is stale relative to the implementation. The
spec shows future subcommands calling `rhei_tui::run_with_frontend` with
`RunParams` (`docs/specs/rhei-run-tui.spec.md:149`,
`docs/specs/rhei-run-tui.spec.md:151`). The crate exports `select_frontend`,
`Frontend`, and `FrontendKind`, but has no `run_with_frontend` or `RunParams`
symbols (`crates/rhei-tui/src/lib.rs:14`). A repository search for
`run_with_frontend` and `RunParams` found no implementation or call sites.

## TM-017: Dependency Claims Are Partially Implemented

Classification: implementation-diverges

The specified TUI dependencies are present for `ratatui`, `crossterm`, and
`crossbeam-channel` (`docs/specs/rhei-run-tui.spec.md:183`,
`docs/specs/rhei-run-tui.spec.md:187`; `crates/rhei-tui/Cargo.toml:8`,
`crates/rhei-tui/Cargo.toml:9`, `crates/rhei-tui/Cargo.toml:11`). The spec also
says `notify` is reused for log tailing (`docs/specs/rhei-run-tui.spec.md:191`).
That part is not implemented: `rhei-tui` does not depend on `notify`, and there
is no log-tail implementation in the TUI crate (`crates/rhei-tui/Cargo.toml:7`,
`crates/rhei-tui/src/tui.rs:1`). `notify` remains a `rhei-cli` dependency
(`crates/rhei-cli/Cargo.toml:20`), but it is not used for the TUI tile tail.

## TM-018: Target/Model Fanout Log Suffixes Are Reflected in Log Paths

Classification: no-discrepancy

The spec says multi-invocation states should produce distinct journal/log pairs
with target suffixes visible in the log path
(`docs/specs/rhei-run-tui.spec.md:137`). The log path builder supports an
optional suffix (`crates/rhei-cli/src/main.rs:6742`), and
`resolved_agent_log_suffix` uses the target slug first, then a non-empty model
string (`crates/rhei-cli/src/main.rs:6305`). All agent spawn paths pass that
suffix into `agent_log_path` (`crates/rhei-cli/src/main.rs:7895`,
`crates/rhei-cli/src/main.rs:7939`, `crates/rhei-cli/src/main.rs:8162`).
Shipped examples exercise the relevant declaration surfaces: `all_targets` in
the changeset review workflow (`examples/changeset-review-example/states.yaml:188`)
and `all_models` in the living review loop (`examples/living-review-loop/team-states.yaml:13`).

## TM-019: End-to-End Journal Behavior Is Missing Test Coverage

Classification: missing-test

The journal sink has unit coverage, but `rhei run` end-to-end tests do not assert
that `runtime/transitions.log` is created, appended, formatted correctly, or
omitted during dry-run. Existing e2e tests verify stdout fragments, final task
states, program artifacts, and fixture logs (`crates/rhei-cli/tests/e2e/run_tests.rs:13`,
`crates/rhei-cli/tests/e2e/run_tests.rs:147`,
`crates/rhei-cli/tests/e2e/run_tests.rs:154`,
`crates/rhei-cli/tests/e2e/run_tests.rs:279`). The dry-run integration test
checks stdout and that the plan file is unchanged, but not that runtime artifacts
are absent (`crates/rhei-cli/tests/integration_markdown_plans.rs:2018`,
`crates/rhei-cli/tests/integration_markdown_plans.rs:2027`,
`crates/rhei-cli/tests/integration_markdown_plans.rs:2039`).

This missing coverage would have caught TM-003 and TM-004.

## Targeted CLI Observations

- `target/debug/rhei run <program-plan> --dry-run` exited `0` and created
  `runtime/transitions.log`, contrary to the dry-run runtime-artifact claim.
- `target/debug/rhei run <program-plan> --no-tui` on a state with transition
  `build -> completed` wrote two journal lines with `build→build`; the plan
  ended in `completed`.
- `target/debug/rhei run <callback-only-plan> --no-tui` transitioned the task and
  created no `runtime/` directory or transition journal.
