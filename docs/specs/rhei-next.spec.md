# `rhei next`

Select and optionally claim the next eligible task from a plan.

## Usage

```bash
rhei next <RHEI_PLAN> [--peek]
```

## Options

| Flag     | Required | Default | Description                                            |
|----------|----------|---------|--------------------------------------------------------|
| `--peek` | No       | false   | Print the next claimable task without transitioning it |

## Default Behavior (Claim Mode)

Without `--peek`, `rhei next` atomically claims the next claimable task: it assigns the task to the current agent and prints the task instructions. The task's state is **not** advanced — the agent works in the current state and uses `rhei transition` or `rhei complete` to advance when ready. This is the standard entry point for agents beginning work.

A task is *claimable* when:

1. All tasks listed in its `**Prior:**` field are in terminal states (`completed` or `cancelled`).
2. The task has no `**Assignee:**` field (not already claimed by another agent).
3. Its current state is not terminal (`final: true`) and not gating (`gating: true`).
4. All required `inputs` declared on the task's current state exist.

### Behavior

1. Load the state machine and plan. Validate.
2. Scan all tasks in plan order. For each task that satisfies dependency,
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
7. Set `**Assignee:** <current-agent>` on the task.
8. Write the task file atomically (temp file + rename), release lock.
9. Resolve template variables in the state's `instructions` and `personality`
   fields (see [Template Variables](rhei-states.spec.md#template-variables-in-instructions-and-personality)).
10. Print the task id, title, current state, and resolved instructions to stdout.

If no claimable task exists, print a status summary (see [No Tasks Ready](#no-tasks-ready)).

### Output (claim mode)

Template variables in `instructions` and `personality` are resolved before output. See [Template Variables](rhei-states.spec.md#template-variables-in-instructions-and-personality) for the full variable namespace and resolution rules.

```text
Task <ID>: <title>
State: <current-state>

<resolved instructions from state definition>
```

### Missing Artifact Error

If the task that would otherwise be claimed is missing one or more required
input artifacts for its current state, `rhei next` fails and prints an explicit
error instead of silently skipping the task.

Example:

```text
Error: Task review-cache-key cannot be claimed in state agent-review-fix.
Missing required input artifact: findings (runtime/findings/review-cache-key.md)
```

## Peek Mode (`--peek`)

With `--peek`, `rhei next` performs a read-only scan and prints the next task that *would* be claimed, without modifying the plan or acquiring a lock. This is safe for PM-style navigation, scripting, and inspection.

Peek mode does **not**:

- Acquire a file lock
- Modify any state
- Append to result files
- Set or clear `**Assignee:**`

Peek mode still resolves required `inputs` for the first otherwise-claimable
task. If any are missing, `--peek` fails with the same missing-artifact error as
claim mode.

### Output (peek mode)

```text
Next: Task <ID>: <title>
State: <current-state>
```

If no claimable task exists, the same status summary is printed as in claim mode.

## No Tasks Ready

When no claimable task is found, `rhei next` (with or without `--peek`) prints one of three status messages depending on the plan state:

| Condition                                        | Message                                                                                    |
|--------------------------------------------------|--------------------------------------------------------------------------------------------|
| All tasks in terminal states                     | `Plan complete. All <N> tasks are in terminal states.`                                     |
| One or more tasks in a gating state              | `Blocked: <N> task(s) waiting on human action: Task <ID> (<state>), ...`                   |
| All non-terminal tasks are claimed (in-flight)   | `No tasks available to claim. <N> task(s) are currently in progress: Task <ID> (<state>), ...` |

These distinct messages allow a PM or orchestrator to tell apart a finished plan, a blocked plan, and a fully in-flight plan. Gating states are identified by the `gating: true` field in the state machine definition (e.g., `human-review` in the default machine). Custom machines may define additional gating states such as `security-review` or `legal-review`.

## Relationship to Other Commands

| Command            | What it does                                                              |
|--------------------|---------------------------------------------------------------------------|
| `rhei next`        | Claims the next ready task (assigns without transitioning), prints instructions |
| `rhei next --peek` | Read-only: prints the next claimable task without claiming it             |
| `rhei transition`  | Atomically changes a task's state; appends entry to result file           |
| `rhei complete`    | Transitions to terminal, appends result entry, links file, unassigns      |
| `rhei reset`       | Returns each task to its resolved profile's `initial` state, removes `runtime/` |

The typical agent loop is: `next` (claim) → work → `transition` (advance as needed) → `complete` (finish, record result, release).

## Agent Context

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

- [Plan Language Specification](../rhei.spec.md) — grammar and semantic constraints
- [How Rhei Is Used](rhei-usage.spec.md) — roles and coordination patterns
- [States Specification](rhei-states.spec.md) — state machine format
- [Agents Specification](rhei-agents.spec.md) — agent configuration, invocation, and timeout
- [Transitions Specification](rhei-transitions.spec.md) — state transition system
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
