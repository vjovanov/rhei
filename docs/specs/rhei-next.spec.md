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

Without `--peek`, `rhei next` atomically claims the next claimable task: it transitions the task forward from its ready-to-start state, prints the task instructions, and returns. This is the standard entry point for agents beginning work.

A task is *claimable* when:

1. All tasks listed in its `**Prior:**` field are in terminal states (`completed` or `cancelled`).
2. Its current state is the machine's initial state (e.g., `draft` or `pending`).
3. No other agent has already claimed it (enforced via file lock and compare-and-swap).

### Behavior

1. Load the state machine and plan. Validate.
2. Scan all tasks to find claimable candidates (criteria above).
3. Select the first candidate in plan order.
4. Acquire a file lock on the plan file.
5. Re-read and re-validate the task's state under the lock (guards against concurrent claims).
6. Transition the task to its first non-initial state (the first declared transition out of the initial state).
7. Append a `## <from> → <to>` entry with no message body to `runtime/results/<task-id>.md`.
8. Set `**Assignee:** <current-agent>` on the task.
9. Write the task file atomically (temp file + rename), release lock.
10. Print the task id, title, new state, and instructions to stdout.

If no claimable task exists, print a status summary (see [No Tasks Ready](#no-tasks-ready)).

### Output (claim mode)

```text
Task <ID>: <title>
State: <new-state>

<instructions from state definition>
```

## Peek Mode (`--peek`)

With `--peek`, `rhei next` performs a read-only scan and prints the next task that *would* be claimed, without modifying the plan or acquiring a lock. This is safe for PM-style navigation, scripting, and inspection.

Peek mode does **not**:

- Acquire a file lock
- Modify any state
- Append to result files
- Set or clear `**Assignee:**`

### Output (peek mode)

```text
Next: Task <ID>: <title>
State: <current-state>  (would transition to: <next-state>)
```

If no claimable task exists, the same status summary is printed as in claim mode.

## No Tasks Ready

When no claimable task is found, `rhei next` (with or without `--peek`) prints one of three status messages depending on the plan state:

| Condition                                        | Message                                                                                    |
|--------------------------------------------------|--------------------------------------------------------------------------------------------|
| All tasks in terminal states                     | `Plan complete. All <N> tasks are in terminal states.`                                     |
| One or more tasks in `human-review`              | `Blocked: <N> task(s) waiting on human review: Task <ID>, ...`                             |
| All non-terminal tasks are claimed (in-flight)   | `No tasks available to claim. <N> task(s) are currently in progress: Task <ID> (<state>), ...` |

These distinct messages allow a PM or orchestrator to tell apart a finished plan, a blocked plan, and a fully in-flight plan.

## Relationship to Other Commands

| Command            | What it does                                                              |
|--------------------|---------------------------------------------------------------------------|
| `rhei next`        | Claims the next ready task; transitions it forward, prints instructions   |
| `rhei next --peek` | Read-only: prints the next claimable task without transitioning it        |
| `rhei transition`  | Atomically changes a task's state; appends entry to result file           |
| `rhei complete`    | Transitions to terminal, appends result entry, links file, unassigns      |
| `rhei reset`       | Returns all tasks to initial state, removes `runtime/`                    |

The typical agent loop is: `next` (claim) → work → `complete` (finish, record result, release).

## Related Specifications

- [Plan Language Specification](../rhei.spec.md) — grammar and semantic constraints
- [How Rhei Is Used](rhei-usage.spec.md) — roles and coordination patterns
- [States Specification](rhei-states.spec.md) — state machine format
- [Transitions Specification](rhei-transitions.spec.md) — state transition system
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
