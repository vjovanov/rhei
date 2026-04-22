# `rhei run`

Drive a plan end-to-end by repeatedly claiming the next ready task, spawning the state's agent or program, waiting for completion, and performing the resulting transition. `rhei run` operates under `orchestrator` authority: the orchestrator — not the spawned subprocess — owns every state transition. See [Agents Specification — Completion Authority](rhei-agents.spec.md#completion-authority) for the full authority contract.

This document specifies the command contract and execution loop. The live terminal UI is specified separately in [rhei-run-tui.spec.md](rhei-run-tui.spec.md).

## Usage

```bash
rhei run <RHEI_PLAN_OR_WORKSPACE> [flags]
```

## Options

Flags are grouped by concern:

### Standalone

| Flag                     | Default | Description                                                                |
|--------------------------|---------|----------------------------------------------------------------------------|
| `--dry-run`              | false   | Print the sequence of transitions that would be made without executing them |
| `--no-callbacks`         | false   | Skip execution of `on_leave` / `on_enter` callbacks                        |
| `--continue-on-error`    | false   | Continue to the next task when an agent or program exits non-zero          |
| `--parallel <N>`         | 1       | Maximum number of agents or programs to run concurrently (0 = unlimited)   |
| `--tui`                  | auto    | Force TUI mode even when stdout is not detected as a TTY                   |
| `--no-tui`               | auto    | Force plain stdout output even when stdout is a TTY                        |

### Agent Execution

| Flag                    | Description                                                             |
|-------------------------|-------------------------------------------------------------------------|
| `--no-agent`            | Disable agent spawning; use callback-only advancement                   |
| `--agent <AGENT>`       | Override the agent for this run                                         |
| `--agent-mode <MODE>`   | Override the agent mode (named flag set) for this run                   |
| `--model <MODEL>`       | Override the model for this run                                         |

### Program Execution

| Flag                           | Description                                                                      |
|--------------------------------|----------------------------------------------------------------------------------|
| `--no-program`                 | Disable program spawning; use callback-only advancement for program states       |
| `--program-timeout <DURATION>` | Override the program timeout for this run (applied per program state)            |

## Execution Loop

`rhei run` runs passes until no further forward progress is possible:

1. Load the state machine and plan. Validate.
2. Scan all tasks and compute the *ready set*: tasks whose `**Prior:**` are all in terminal states, whose current state is non-terminal and non-gating, and whose current state's required `inputs:` all exist. See [Next Command](rhei-next.spec.md#default-behavior-claim-mode) for the full claimability rule.
3. Up to `--parallel` tasks from the ready set are executed concurrently. For each task:
   - Resolve the state's target: either an agent subprocess (`agent` or resolved target selector) or a program (`program`).
   - Spawn the subprocess with the state's resolved instructions, environment (`RHEI_*` variables defined in [Agents Specification — Environment Variables](rhei-agents.spec.md#environment-variables)), and timeout.
   - Wait for the subprocess to exit or for the timeout to fire. On timeout, send `SIGTERM`, grace 10 s, then `SIGKILL`.
4. On subprocess exit, evaluate the state's [Completion Condition](rhei-agents.spec.md#completion-condition): exit code `0` plus every required `outputs:` artifact present on disk.
5. If the condition holds, select the first declared transition whose `condition` / `exit_code` matches and execute it. The subprocess **must not** call `rhei transition` or `rhei complete`; the orchestrator owns the transition.
6. If the condition fails (non-zero exit or missing outputs), route through the state's error or timeout transition per [Agents Specification — Execution Loop](rhei-agents.spec.md#execution-loop). When no error transition is declared and `--continue-on-error` is unset, `rhei run` aborts with a non-zero exit code.
7. Repeat until no pass makes progress. Exit `0` when the plan reaches a state where every task is terminal. Exit non-zero when progress halts with non-terminal tasks remaining and no further advancement is possible.

`rhei run` does not transition out of [gating states](rhei-states.spec.md#per-state-fields) — exiting one requires an explicit human-initiated `rhei transition` call.

## Dry Run

With `--dry-run`, `rhei run` performs the same scan and selection logic but prints each planned transition instead of executing subprocesses or callbacks. Output format:

```text
would transition: Task <ID>  <from> -> <to>
```

No file lock is acquired, no markdown is rewritten, and no runtime artifacts are created.

## Parallel Execution

With `--parallel N`, up to `N` subprocesses run concurrently. The orchestrator:

- Assigns each spawn a slot index.
- Writes one line to `runtime/transitions.log` per `SlotAssigned` and one per `SlotReleased`; see [Run TUI Specification — Transition Journal](rhei-run-tui.spec.md#transition-journal).
- Serializes every state write through its own file lock, so two agents completing at once cannot corrupt the plan.

Tasks whose transitions would race on the same task node are never scheduled in parallel: scheduling is driven by the ready set, which excludes tasks already in flight.

## Relationship to Other Commands

`rhei run` drives the full plan forward under orchestrator authority. It is mutually exclusive per execution with the manual-worker flow (`next` / `transition` / `complete`) — they never overlap on the same task because `rhei run` holds transition responsibility for the states it drives.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#command-surface) for the full table comparing all five coordination commands.

## Related Specifications

- [Agents Specification](rhei-agents.spec.md) — completion authority, completion condition, timeout handling, environment variables
- [Program States Specification](rhei-programs.spec.md) — exit-code transitions and program-specific semantics
- [Run TUI Specification](rhei-run-tui.spec.md) — live terminal UI and transition journal
- [Transitions Specification](rhei-transitions.spec.md) — transition YAML schema and callbacks
- [Next Command](rhei-next.spec.md), [Complete Command](rhei-complete.spec.md), [Transition Command](rhei-transition-cmd.spec.md) — manual-worker counterparts
