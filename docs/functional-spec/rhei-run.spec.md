# FS-rhei-run: `rhei run`

Drive a plan end-to-end by repeatedly claiming the next ready task, spawning the state's agent or program, waiting for completion, and performing the resulting transition. `rhei run` operates under `orchestrator` authority: the orchestrator — not the spawned subprocess — owns every state transition. See [Agents Specification — Completion Authority](rhei-agents.spec.md#31-completion-authority) for the full authority contract.

This document specifies the command contract and execution loop. The live terminal UI is specified separately in [rhei-run-tui.spec.md](rhei-run-tui.spec.md).

## 1. Usage

```bash
rhei run <RHEI_PLAN_OR_WORKSPACE> [flags]
```

## 2. Options

Flags are grouped by concern:

### 2.1. Standalone

| Flag                     | Default | Description                                                                |
|--------------------------|---------|----------------------------------------------------------------------------|
| `--dry-run`              | false   | Print the sequence of transitions that would be made without executing them |
| `--no-callbacks`         | false   | Skip execution of `on_leave` / `on_enter` callbacks                        |
| `--continue-on-error`    | false   | Continue to the next task when an agent or program exits non-zero          |
| `--parallel <N>`         | 1       | Maximum number of agents or programs to run concurrently (0 = unlimited)   |
| `--tui`                  | auto    | Force TUI mode even when stdout is not detected as a TTY                   |
| `--no-tui`               | auto    | Force plain stdout output even when stdout is a TTY                        |
| `--dashboard`            | auto    | Serve the loopback browser dashboard for this run                          |
| `--no-dashboard`         | auto    | Disable the loopback browser dashboard                                     |

### 2.2. Agent Execution

| Flag                    | Description                                                             |
|-------------------------|-------------------------------------------------------------------------|
| `--no-agent`            | Disable agent spawning; use callback-only advancement                   |
| `--agent <AGENT>`       | Override the agent for this run                                         |
| `--agent-mode <MODE>`   | Override the agent mode (named flag set) for this run                   |
| `--model <MODEL>`       | Override the model for this run                                         |

### 2.3. Snapshots

| Flag | Description |
|------|-------------|
| `--from-snapshot <ref>` | Override the concrete source selected by an authored `snapshot.inherit:` after that state's constraints are applied. See [Snapshot Operations Specification — Run Override](rhei-snapshot-operations.spec.md#2-run-override). |
| `--override-inherit` | Explicitly bypass authored source-selection and compatibility constraints for an ad-hoc debug run. The target state must still declare `snapshot.inherit:`. Requires `--from-snapshot`. |
| `--task <id>` | Select the task for an ambiguous snapshot override. |
| `--target <slug>` | Select the fanout target for an ambiguous snapshot override. |

### 2.4. Program Execution

| Flag                           | Description                                                                      |
|--------------------------------|----------------------------------------------------------------------------------|
| `--no-program`                 | Disable program spawning; use callback-only advancement for program states       |
| `--program-timeout <DURATION>` | Override the program timeout for this run (applied per program state)            |

## 3. Execution Loop

`rhei run` runs passes until no further forward progress is possible:

Mode selection: `rhei run` uses orchestrated subprocess execution whenever any reachable non-terminal, non-gating state declares autonomous work via `program`, `agent`, `target`, `all_targets`, `model`, or `all_models`. Callback-only advancement is entered only when no such state exists, or when the caller explicitly disables spawning with `--no-agent` and/or `--no-program`. If a state declares model/target-driven work but no agent transport resolves, `rhei run` fails with a missing-agent configuration error; it does not silently fall back to callback-only transitions for that state.

The built-in `pending` -> `completed` machine is manual-only, not
callback-complete work. If a ready task under that built-in machine is in its
profile's initial `pending` state, `rhei run` must fail without changing the
task. The manual worker loop must claim such a task with `rhei next`, do the
work, and finish it with `rhei complete`. This prevents the built-in machine
from silently completing fresh tasks without executing them.

1. Load the state machine and plan. Validate.
2. Scan all task nodes, including child and grandchild tasks, and compute the
   *ready set*: tasks whose `**Prior:**` are all in successful terminal states
   (terminal and not `cancelled`), whose current state is non-terminal and
   non-gating, and whose current state's required `inputs:` all exist. Task
   counts, terminal counts, final state summaries, and remaining-work checks
   use the same full task tree. Tasks whose current state declares `poll:` and whose
   `metadata.tasks.<id>.pollNextAttemptAt.<state-name>` is later than the
   current wall-clock time are excluded from the ready set until the interval
   elapses. See [Next Command](rhei-next.spec.md#3-default-behavior-claim-mode)
   for the manual claimability rule and [Polling States](#51-polling-states) for
   the poll scheduling rule.
3. Up to `--parallel` tasks from the ready set are executed concurrently, subject to the [concurrent-state rule](#5-parallel-execution): at most one ready task per non-concurrent state is scheduled per pass. For each task:
   - Resolve the state's target: either an agent subprocess (`agent` or resolved target selector) or a program (`program`).
   - If the state declares `snapshot.inherit:`, resolve and preload the source snapshot before spawning the agent. Polling states reject `snapshot.inherit` in v1. See [Snapshots Specification](rhei-snapshots.spec.md).
   - Spawn the subprocess with the state's resolved instructions, environment (`RHEI_*` variables defined in [Agents Specification — Environment Variables](rhei-agents.spec.md#4-environment-variables)), checkout-root working directory, and timeout.
   - Wait for the subprocess to exit or for the timeout to fire. On timeout, send `SIGTERM`, grace 10 s, then `SIGKILL`.
4. On subprocess exit, evaluate the state's [Completion Condition](rhei-agents.spec.md#32-completion-condition): exit code `0` plus every required `outputs:` artifact present on disk.
5. Select the outgoing transition without applying it yet. If the condition
   holds, select the first declared transition whose `condition` / `exit_code`
   matches. If the condition fails (non-zero exit or missing outputs), route
   through the state's error or timeout transition per
   [Agents Specification — Execution Loop](rhei-agents.spec.md#52-execution-loop).
   When no error transition is declared and `--continue-on-error` is unset,
   `rhei run` aborts with a non-zero exit code.
6. For agent invocations, extract measured usage and write the accounting
   invocation record when the resolved agent supports accounting. Accounting
   failures affect cost coverage but do not alter transition selection. §FS-rhei-cost-accounting
7. For agent-bearing states with supported snapshot sessions, write
   auto-emitted `_state` snapshots and any matching named `snapshot.emit:`
   after transition selection and before the transition is applied. Poll
   self-loop attempts do not emit because the selected transition is known;
   terminal poll exits may emit. See
   [Snapshots Specification — Emit on Exit](rhei-snapshots.spec.md#102-emit-on-exit).
8. Apply the selected transition and append one central state-transition entry
   to `runtime/state-transitions.log` as `<task-id> <from>@<to>`. The
   subprocess **must not** call `rhei transition` or `rhei complete`; the
   orchestrator owns the transition. When the effective target is `final: true`,
   terminal result finalization is performed as defined in
   [Complete Command — Result File](rhei-complete.spec.md#3-result-file).
9. Repeat until no pass makes progress. Exit `0` when the plan reaches a state where every task is terminal. Exit non-zero when progress halts with non-terminal tasks remaining and no further advancement is possible.

`rhei run` does not transition out of [gating states](rhei-states.spec.md#12-per-state-fields) — exiting one requires an explicit human-initiated `rhei transition` call.

Gating states are a barrier, not an immediate global abort. If one task enters a
gating state while other non-gating tasks are already running, or while other
independent non-gating tasks remain ready, `rhei run` lets that remaining
non-gating work finish. The run halts for human input only when the remaining
non-terminal tasks are either themselves in gating states or blocked behind a
gating dependency. In other words: a gate waits for everyone else to complete,
then stops autonomous progress at the boundary.

### 3.1. Git Consistency After Subprocess Commits

The orchestrator-owned transition in step 8 is a durable-state write to the
authored plan or workspace task file. If a subprocess creates a Git commit
before that write, the commit cannot include the later Rhei-owned transition
without violating orchestrator authority.

When a non-dry-run execution starts inside a Git repository, `rhei run`
records the starting `HEAD`. If the final success path observes that `HEAD`
changed during the run, it must inspect tracked changes under the plan input
and `runtime/results` before returning success. The path check resolves the
actual plan or workspace path independent of the operator's current directory,
so `rhei run plan.rhei.md` from a subdirectory and `rhei run
path/to/plan.rhei.md` from the repository root are equivalent for this
postcondition.

If any of those Rhei-owned tracked paths remain dirty, `rhei run` exits
non-zero with a diagnostic that names the paths instead of silently reporting a
durable success. This check is read-only: it does not create commits, stage
files, or reject untracked runtime artifacts. Outside Git repositories, or
when `HEAD` does not move during the run, the check is a no-op.
§GOAL-rhei-outcomes

## 4. Dry Run

With `--dry-run`, `rhei run` performs the same scan and selection logic but prints each planned transition instead of executing subprocesses or callbacks. Output format:

```text
would transition: Task <ID>  <from> -> <to>
```

No file lock is acquired, no markdown is rewritten, and no runtime artifacts are created.

## 5. Parallel Execution

With `--parallel N`, up to `N` subprocesses run concurrently. The orchestrator:

- Assigns each spawn a slot index.
- Writes one line to `runtime/transitions.log` per `SlotAssigned` and one per
  `SlotReleased`; see [Run TUI Specification — Run Event Journal](rhei-run-tui.spec.md#17-journal-format).
- Serializes every state write through its own file lock, so two agents completing at once cannot corrupt the plan.
- Refills freed slots immediately: after any subprocess exits and its result is
  processed, the orchestrator re-reads the plan, recomputes the ready set, and
  starts newly ready work while the rest of the pool keeps running.

Tasks whose transitions would race on the same task node are never scheduled in
parallel: scheduling is driven by the ready set, which excludes tasks already in
flight. A dependent task only becomes schedulable after its `**Prior:**` task has
actually reached a successful terminal state; if sibling work finishes first,
the freed slot is filled only with work whose dependencies are already satisfied.

### 5.1. Polling States

States that declare a [`poll:`](rhei-states.spec.md#2-polling-states) block are time-triggered: each attempt spawns one subprocess, the engine evaluates transitions, and a self-loop transition means "retry after `poll.interval`". Between attempts, the orchestrator:

- Persists `metadata.tasks.<id>.pollNextAttemptAt.<state-name> = now() + interval` and `metadata.tasks.<id>.stateVisits.<state-name>` (the attempt counter).
- Releases the `--parallel` slot so other ready tasks may run.
- Does not hold a timer thread; the next pass re-scans and picks the task up again only once `pollNextAttemptAt` is in the past.

If, at the end of a pass, every remaining non-terminal task is either in a gating state, blocked behind a gating dependency, or blocked by a pending `pollNextAttemptAt`, `rhei run` sleeps until the earliest `pollNextAttemptAt` across all blocked poll tasks (bounded below by 1 s to avoid busy-looping) and then begins a new pass. If no poll deadline is pending and only gating remains, the run exits as it does today.

Once `stateVisits.<state-name>` reaches `poll.max_attempts`, the engine refuses to select a self-loop transition and picks the first matching non-self-loop instead. If no non-self-loop transition matches, the run halts that task with a "polling exhausted with no matching non-self-loop transition" error — `--continue-on-error` applies as with any other task failure. A non-self-loop exit at any attempt clears both `pollNextAttemptAt.<state-name>` and `stateVisits.<state-name>`.

`snapshot.inherit` is rejected on polling states in v1. Snapshot emit,
including auto-emit, is suppressed for self-loop attempts and runs only on a
terminal non-self-loop exit when the state is otherwise snapshot-capable.

### 5.2. Concurrent vs. Serial States

The [`concurrent`](rhei-states.spec.md#12-per-state-fields) flag on a `StateDef` determines whether multiple ready tasks in the same state may be scheduled together in one pass:

- `concurrent: true` — any number of ready tasks in this state may be scheduled together (bounded by `--parallel`).
- `concurrent: false` (the default) — at most one ready task in this state is scheduled per pass. Additional tasks remain ready and are picked up on the next pass.

The flag does not change state entry/exit semantics or transitions, and it does not affect within-task fanout (`all_targets` / `all_models`): every resolved invocation for a single scheduled task is still spawned together.

## Relationship to Other Commands

`rhei run` drives the full plan forward under orchestrator authority. It is mutually exclusive per execution with the manual-worker flow (`next` / `transition` / `complete`) — they never overlap on the same task because `rhei run` holds transition responsibility for the states it drives.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#22-command-surface) for the full table comparing all five coordination commands.

## Related Specifications

- [Agents Specification](rhei-agents.spec.md) — completion authority, completion condition, timeout handling, environment variables
- [Program States Specification](rhei-programs.spec.md) — exit-code transitions and program-specific semantics
- [Snapshots Specification](rhei-snapshots.spec.md) — snapshot side effects and inheritance preload
- [Snapshot Operations Specification](rhei-snapshot-operations.spec.md) — snapshot commands, settings, and `--from-snapshot`
- [Run TUI Specification](rhei-run-tui.spec.md) — live terminal UI and transition journal
- [Cost Accounting Specification](rhei-cost-accounting.spec.md) — token/cost ledger and rollups
- [Transitions Specification](rhei-transitions.spec.md) — transition YAML schema and callbacks
- [Next Command](rhei-next.spec.md), [Complete Command](rhei-complete.spec.md), [Transition Command](rhei-transition-cmd.spec.md) — manual-worker counterparts
