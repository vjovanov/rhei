# FS-rhei-next: `rhei next`

Select and optionally claim the next eligible task from a plan.

## 1. Usage

```bash
rhei next <RHEI_PLAN> [--peek]
```

## 2. Options

| Flag     | Required | Default | Description                                            |
|----------|----------|---------|--------------------------------------------------------|
| `--peek` | No       | false   | Print the next claimable task without transitioning it |

## 3. Default Behavior (Claim Mode)

Without `--peek`, `rhei next` atomically claims the next claimable task: it assigns the task to the current agent and prints the task instructions. The task's state is **not** advanced — the agent works in the current state and uses `rhei transition` or `rhei complete` to advance when ready. This is the standard entry point for agents beginning work.

Initial states are not all treated the same: an initial state that declares
runnable autonomous work (`program`, `agent`, `target`, `all_targets`, `model`,
or `all_models`) is claimed and presented in place. A non-runnable initial
state is auto-advanced only when its first applicable forward transition targets
another non-terminal state. If its first applicable forward transition targets
a terminal state, `rhei next` claims and presents the initial state in place so
the agent can do the work before `rhei complete` finishes it. This keeps the
built-in `pending` -> `completed` machine claimable without completing work at
claim time.

A task is *claimable* when:

1. It is a leaf task node with no child task nodes.
2. All tasks listed in its `**Prior:**` field are in successful terminal
   states (`final: true` and not the normalized `cancelled` state).
3. The task has no `**Assignee:**` field (not already claimed by another agent).
4. Its current state is not terminal (`final: true`) and not gating (`gating: true`).
5. All required `inputs` declared on the task's current state exist.

Non-leaf task nodes are structural rollups and result anchors. `rhei next`
must exclude them from claim selection even when their dependencies and state
would otherwise make them ready.

### 3.1. Behavior

1. Load the state machine and plan. Validate.
2. Scan leaf tasks in plan order. For each task that satisfies dependency,
   assignee, and state eligibility, resolve the current state's required
   `inputs`.
3. If any required input file for the first otherwise-claimable task is
   missing, stop immediately and fail with an explicit missing-artifact error.
   Do not skip ahead to later tasks.
4. Select the first candidate in plan order.
5. Acquire a file lock on the plan file.
6. Re-read and re-validate the task's claimability under the lock, including
   re-checking required `inputs` (guards against concurrent claims and moved
   files).
7. If the selected task is in a non-runnable initial state whose first
   applicable forward transition targets another non-terminal state, apply that
   transition before rendering. Otherwise keep the task in its current state.
8. Set `**Assignee:** <current-agent>` on the task, where `<current-agent>` is the agent id resolved for the rendered state via the [agent resolution order](rhei-agents.spec.md) (state `agent:` field → project settings → global settings). When no agent is configured, write the reserved assignee value `manual` so the task still leaves the claimable set durably and concurrent `rhei next` calls cannot claim it twice.
9. Write the task file atomically (temp file + rename), release lock.
10. Resolve template variables in the state's `instructions` and `personality`
   fields (see [Template Variables](rhei-states.spec.md#4-template-variables-in-instructions-and-personality)).
11. Print the task id, title, current state, and resolved instructions to stdout.

If no claimable task exists, print a status summary (see [No Tasks Ready](#5-no-tasks-ready)).

### 3.2. Output (claim mode)

Template variables in `instructions` and `personality` are resolved before output. See [Template Variables](rhei-states.spec.md#4-template-variables-in-instructions-and-personality) for the full variable namespace and resolution rules.

```text
Task <ID>: <title>
State: <current-state>

<resolved instructions from state definition>
```

### 3.3. Missing Artifact Error

If the task that would otherwise be claimed is missing one or more required
input artifacts for its current state, `rhei next` fails and prints an explicit
error instead of silently skipping the task.

Example:

```text
Error: Task review-cache-key cannot be claimed in state agent-review-fix.
Missing required input artifact: findings (runtime/findings/review-cache-key.md)
```

## 4. Peek Mode (`--peek`)

With `--peek`, `rhei next` performs a read-only scan and prints the next task that *would* be claimed, without modifying the plan or acquiring a lock. This is safe for PM-style navigation, scripting, and inspection.

Peek mode does **not**:

- Acquire a file lock
- Modify any state
- Append to result files
- Set or clear `**Assignee:**`

Peek mode still resolves required `inputs` for the first otherwise-claimable
task. If any are missing, `--peek` fails with the same missing-artifact error as
claim mode.

### 4.1. Output (peek mode)

```text
Next: Task <ID>: <title>
State: <current-state>
```

If no claimable task exists, the same status summary is printed as in claim mode.

## 5. No Tasks Ready

When no claimable task is found, `rhei next` (with or without `--peek`) prints a
status message that explains why no claim was possible and what the next human
action is:

| Condition | Message |
|-----------|---------|
| All tasks in terminal states | `Plan complete. All <N> task(s) are in terminal states.` |
| All leaf tasks are terminal but one or more non-leaf rollups remain non-terminal | `Leaf work complete. <N> rollup task(s) can be completed after descendants are terminal: Task <ID> (<state>), ...` |
| One or more otherwise-ready tasks are in a gating state | `Blocked: <N> task(s) waiting on human action: Task <ID> (<state>), ...` |
| All otherwise-ready non-terminal tasks are claimed | `No tasks available to claim. <N> task(s) are currently in progress: Task <ID> (<state>, assignee <ASSIGNEE>), ...` |
| A ready task is mid-workflow rather than in its profile's initial state | `No tasks can be auto-claimed: Task <ID> is mid-workflow in state '<state>'. Pick one of its outgoing transitions explicitly.` followed by one `rhei [--state-machine=<states>] transition <plan> --task <ID> --from=<state> --to=<target>` command per currently applicable outgoing transition, with shell quoting applied to copied arguments |
| Non-terminal tasks are blocked by prerequisites | `no tasks are ready to claim: Task <ID> waiting on Task <PRIOR> (<state>) blocked by incomplete prerequisites.` |

These distinct messages allow a PM or orchestrator to tell apart a finished
plan, a human gate, fully in-flight work, manual transition selection, and
ordinary prerequisite blocking. See [States Specification — State
Definition](rhei-states.spec.md#12-per-state-fields) for the `gating: true`
field (e.g., `human-review` in the default machine; custom machines may define
additional gating states such as `security-review` or `legal-review`).

## Relationship to Other Commands

`rhei next` is the claim step of the manual-worker loop: `next` (claim) → work → `transition` (advance as needed) → `complete` (finish, record result, release). `--peek` is the read-only variant that inspects the next claimable task without taking it.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#22-command-surface) for the full table comparing all five coordination commands.

## 6. Agent Context

When a state declares an `agent` field (or an agent is resolved from project/global settings), `rhei next` includes the agent identifier in its JSON output:

```json
{
  "task_id": "3",
  "title": "Implement caching layer",
  "state": "pending",
  "agent": "claude-code",
  "model": "impl-fast",
  "model_provider": "anthropic",
  "model_name": "claude-sonnet-4-6",
  "instructions": "..."
}
```

The `agent`, `model`, `model_provider`, and `model_name` fields are omitted
from JSON output when no agent or model is configured.

In text output mode, the agent is shown after the state line:

```text
Task 3: Implement caching layer
State: pending
Agent: claude-code (impl-fast = anthropic/claude-sonnet-4-6)

<resolved instructions>
```

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — grammar and semantic constraints
- [How Rhei Is Used](rhei-usage.spec.md) — roles and coordination patterns
- [States Specification](rhei-states.spec.md) — state machine format
- [Agents Specification](rhei-agents.spec.md) — agent configuration, invocation, and timeout
- [Transitions Specification](rhei-transitions.spec.md) — state transition system
- [Transition Command](rhei-transition-cmd.spec.md) — `rhei transition` behavioral contract
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
- [Run Command](rhei-run.spec.md) — `rhei run` behavioral contract
- [Reset Command](rhei-reset.spec.md) — `rhei reset` behavioral contract
