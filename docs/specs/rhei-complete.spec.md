# `rhei complete`

Atomically complete a task: transition to a terminal state, write the result to a file, link it from the task body, and remove the `**Assignee:**` line. This is the single command an agent calls when it is done with a task.

## Usage

```bash
rhei complete <RHEI_PLAN> --task <TASK_ID> --result <MESSAGE>
```

## Options

| Flag             | Required | Default | Description                                       |
|------------------|----------|---------|---------------------------------------------------|
| `--task <ID>`    | Yes      |         | Task identifier (number or name)                  |
| `--result <MSG>` | Yes      |         | Result message for the task                       |
| `--no-callbacks` | No       | false   | Skip execution of `on_leave`/`on_enter` callbacks |

## Result File

Each task has a result file at a fixed path:

```text
runtime/results/<task-id>.md
```

The `runtime/results/` directory is created if it does not exist. A markdown
link to the result file is appended to the task body (after task content,
before child nodes):

```markdown
> **Result:** [<task-id>](runtime/results/<task-id>.md)
```

This keeps task files concise — the result detail lives in a separate artifact under `runtime/`, consistent with how other runtime outputs (findings, verifications, fixes) are stored in directory workspaces.

### Result File Format

The result file contains one entry per state transition, appended by both `rhei transition` and `rhei complete`. Each entry is a markdown heading with the transition arrow followed by the message (if any):

```markdown
## <from> → <to>

<message>
```

`rhei transition` appends an entry with no message body. `rhei complete` appends an entry with the mandatory `--result` message. This gives every task a complete, ordered audit trail of its state transitions.

Example result file after a task goes `draft → pending → completed`:

```markdown
## draft → pending

## pending → completed

Added avatar_url column and migration 0042
```

## Behavior

1. Load the state machine and plan (single file or directory workspace). Validate.
2. Locate the task by ID. Fail if the task does not exist.
3. Reject if the task is already in a terminal state.
4. Reject if the task's current state is a [gating state](rhei-states.spec.md#per-state-fields) (`gating: true`) — those can only be exited by an explicit human-initiated `rhei transition`.
5. Reject if any descendant task node of the target task is still in a
   non-terminal state. A parent task must not be completed while any child,
   grandchild, or deeper descendant remains open.
6. Find the completion target: the first non-cancelled terminal state reachable via a declared transition from the current state. Fail if none exists (e.g., from `agent-review-fix` there is no direct path to a terminal state — the agent must transition to `agent-review` first). `cancelled` is never treated as a successful completion target. The order of transitions in the YAML `transitions` list is significant when selecting the target; editors and formatters should preserve declaration order.
7. Execute the state transition directly (compare-and-swap with file lock, `on_leave`/`on_enter` callbacks). This is performed inline — `rhei complete` does **not** delegate to `rhei transition`, so only one result entry is appended per invocation.
8. Append a `## <from> → <to>` entry with the `--result` message to `runtime/results/<task-id>.md` (create directories as needed).
9. Remove the `**Assignee:**` line from the task (no-op if absent).
10. If the result file does not yet have a `> **Result:**` link in the task body, append a `> **Result:** [<task-id>](runtime/results/<task-id>.md)` link to the task body.
11. Write the task file atomically (temp file + rename).

`rhei transition` also appends a `## <from> → <to>` entry (with no message body) to the same result file. This means the result file accumulates the full transition history regardless of which command performed each transition.

**Note on child nodes:** In the current hierarchical node model, child nodes
are full stateful task nodes. `rhei complete` must therefore inspect all
descendants of the target task and reject completion until every descendant is
in a terminal state.

### Completion Target Selection

The command scans declared transitions for a non-cancelled terminal state reachable in one hop from the task's current state. If multiple terminal states are reachable, the first non-cancelled one wins. If only `cancelled` is reachable, the command fails.

### Single-File Plans

The result file is written relative to the plan file's parent directory. The state change, assignee removal, and result link are applied in the plan file itself.

### Directory Workspaces

The result file is written relative to the workspace root. The state change, assignee removal, and result link are applied in the task file under `tasks/`.

## Output

```text
Task <ID> completed: '<from>' -> '<to>' (runtime/results/<ID>.md)
```

## Examples

```bash
# Agent finishes work on task 3
rhei complete plan.rhei.md --task 3 \
  --result "Added avatar_url column and migration 0042"
# State: pending -> completed
# Result: runtime/results/3.md
# Assignee: removed

# Worker in a living workspace completes a review-seed task
rhei complete ./my-workspace --task review-seed \
  --result "Wrote 4 findings to runtime/findings/consolidated.md"
# State: pending -> completed
# Result: ./my-workspace/runtime/results/review-seed.md
# Task body: > **Result:** [review-seed](runtime/results/review-seed.md)
```

## Relationship to Other Commands

`rhei complete` is the terminal step of the manual-worker loop: `next` (claim) → work → `transition` (advance as needed) → `complete` (finish, record result, release). It transitions the task to a terminal state, appends a result entry, and releases the claim.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#command-surface) for the full table comparing all five coordination commands.

## Related Specifications

- [Plan Language Specification](../rhei.spec.md) — grammar including `assignee_field` and `result_block`
- [How Rhei Is Used](rhei-usage.spec.md) — roles and coordination patterns
- [States Specification](rhei-states.spec.md) — state machine format
- [Transitions Specification](rhei-transitions.spec.md) — state transition system
- [Next Command](rhei-next.spec.md) — `rhei next` behavioral contract
- [Transition Command](rhei-transition-cmd.spec.md) — `rhei transition` behavioral contract
- [Run Command](rhei-run.spec.md) — `rhei run` behavioral contract
- [Reset Command](rhei-reset.spec.md) — `rhei reset` behavioral contract
