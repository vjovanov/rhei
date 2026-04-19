# 0001 - Introduce `complete` command

## Status

accepted

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
   current state. Falls back to `cancelled` only when no other terminal
   state is reachable.
2. **Executes the state transition** — reuses the existing
   compare-and-swap `execute_transition` path, including file locking
   and on_leave/on_enter callbacks.
3. **Removes the assignee** — strips any `**Assignee:**` line from the
   task in a post-transition text rewrite.
4. **Appends an optional result** — inserts a `> **Result:** <msg>`
   blockquote into the task body (after content, before subtasks) so the
   outcome is visible in rendered plans.

The result format uses a Markdown blockquote with a bold `**Result:**`
prefix rather than a metadata field because results are task content, not
structural metadata. This keeps the parser and spec unchanged.

## Consequences

- Agents can complete a task with a single CLI invocation instead of
  three coordinated operations.
- The `> **Result:**` format is greppable and renders well in GitHub
  markdown, but it is not parsed by rhei-core — it remains free-form
  content. A future ADR could promote it to a structured field if needed.
- `--no-callbacks` is supported, consistent with `transition` and `run`.
