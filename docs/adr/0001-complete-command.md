# 0001 - Introduce `complete` command

## Status

accepted; points 3–5 superseded

Steps 3–5 below described `rhei complete` as the owner of the result file, the
result link, and the assignee removal. They are now properties of entering a
`final: true` state, enforced identically by every verb on the shared
transition path — including the requirement that the result be **non-empty**,
which was the last piece still bound to this command
(§FS-rhei-states.3.3, §FS-rhei-transition-cmd.3.2). Point 1 is what remains
genuinely `rhei complete`'s own (§FS-rhei-complete.4.1). The "Decision" text is
kept as written for the record.

## Context

Agents and humans completing a task must currently perform three separate
steps: transition the state to a terminal value (`rhei transition`), remove
the assignee (manual edit), and record the outcome (manual edit). This is
error-prone and requires knowledge of which terminal state is reachable
from the current state.

## Decision

Add a single `rhei complete` subcommand that atomically:

1. **Finds the completion target** — scans declared transitions for a
   non-cancelled terminal state reachable in one hop from the task's
   current state. If no such terminal state exists, the command fails;
   it never treats `cancelled` as successful completion.
2. **Executes the state transition** — reuses the existing
   compare-and-swap `execute_transition` path, including file locking
   and on_leave/on_enter callbacks.
3. **Writes the result file** — writes the mandatory `--result` message
   to `runtime/results/<task-id>.md`, creating directories as needed.
4. **Links the result from the task** — appends a
   `> **Result:** [<task-id>](runtime/results/<task-id>.md)` line to the
   task body (after content, before any child task nodes).
5. **Removes the assignee** — strips any `**Assignee:**` line from the
   task in a post-transition text rewrite.

The result is stored in a separate file under `runtime/` rather than
inlined because: (a) it keeps task files concise, (b) it is consistent
with how other runtime artifacts (findings, verifications, fixes) are
stored in directory workspaces, and (c) `rhei reset` already removes
`runtime/`, so results are cleaned up naturally.

## Consequences

- Agents can complete a task with a single CLI invocation instead of
  three coordinated operations.
- The `--result` flag is mandatory — every completed task has a recorded
  outcome, which keeps the plan auditable.
- Result detail lives in `runtime/results/`, not in the plan file. The
  task body contains only a link. This avoids bloating task files with
  long result messages.
- `--no-callbacks` is supported, consistent with `transition` and `run`.
- See [Complete Command Specification](../functional-spec/rhei-complete.spec.md)
  for the full behavioral contract.
