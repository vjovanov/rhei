# Rhei Plan Language Usage Guide

This guide shows how to author Rhei Plan documents that conform to the
[Rhei Language Specification](../rhei.spec.md). It focuses on practical
patterns rather than formal grammar — consult the spec for precise rules.

## A Minimal Plan

The smallest valid plan declares a title, a `## Tasks` section, and at least
one task with a `**State:**` field:

```markdown
# Rhei: First Plan

## Tasks

### Task 1: Set up the repository
**State:** pending
```

Save this file with a `.rhei.md` extension, then validate it:

```bash
cargo run -p rhei-cli -- validate my-plan.rhei.md
```

When no `**States:**` field is declared, the plan uses the built-in `rhei`
state machine from [`docs/states.yaml`](states.yaml).

## Authoring Workflow

A typical plan grows in three passes:

1. **Outline** — write the `# Rhei:` title and draft `## Overview`,
   `## Context`, or similar prose sections that motivate the work.
2. **Break down** — add a `## Tasks` section and enumerate tasks with
   numeric or named ids. Give every task a `**State:**` (usually `draft`
   or `pending`) and a short description.
3. **Refine** — add `**Prior:**` dependencies, split tasks into subtasks,
   and transition states as work progresses.

Keep content sections before `## Tasks`. Everything after `## Tasks` is
parsed as task structure.

## Tasks and Subtasks

### Numeric vs named tasks

Numeric task ids (`### Task 1:`) are ordered and may contain subtasks.
Named task ids (`### Task setup:`) are useful for conceptual anchors that
other tasks depend on — they must not declare subtasks.

```markdown
### Task infra: Provision cloud resources
**State:** pending

### Task 1: Deploy application
**State:** pending
**Prior:** Task infra
```

### Subtasks

Subtasks belong to the task with the matching first number. They track
fine-grained progress inside a larger task but do not carry their own
`**State:**` metadata.

```markdown
### Task 2: Implement login flow
**State:** in-progress

#### Subtask 2.1: Wire OAuth callback
#### Subtask 2.2: Persist session tokens
```

## Metadata

`**State:**` is mandatory and must be the first line after the task header.
`**Prior:**` is optional and, when present, must immediately follow
`**State:**`.

```markdown
### Task 3: Ship release notes
**State:** agent-review
**Prior:** Task 1, Task 2
```

### State values with spaces

Single-word states are written bare. Multi-word states must be wrapped in
backticks:

```markdown
**State:** `in review`
```

The state value must be defined in the active states file — multi-word
states typically require a custom states file such as
[`examples/states-with-spaces.yaml`](../../examples/states-with-spaces.yaml).

### Dependencies

List prerequisites by id, separated by commas. References must resolve to
tasks defined in the same document, and the dependency graph must stay
acyclic.

```markdown
**Prior:** Task 1, Task design, Task 4
```

## Using a Custom State Machine

To reuse one state machine across plans, declare it on the line directly
after the `# Rhei:` title:

```markdown
# Rhei: Content Refresh
**States:** content-workflow
```

The `name` field in the referenced YAML file must match this value. Pass
the states file to the CLI with `--state-machine`:

```bash
cargo run -p rhei-cli -- --state-machine ./content-workflow.yaml \
    validate plans/content-refresh.rhei.md
```

See the [States Specification](rhei-states.spec.md) for the states file format.

## Common Pitfalls

- **Missing `**State:**`** — every task header must be followed by a
  `**State:**` line.
- **Metadata out of order** — `**Prior:**` must come after `**State:**`,
  not before.
- **Subtask under a named task** — only numeric task ids may own
  subtasks.
- **Cross-plan references** — `**Prior:**` only resolves within one
  document; to model cross-plan dependencies, keep those tasks in the
  same file.
- **Unknown state** — validation fails if a `**State:**` value is not
  declared in the active states file.
- **Duplicate ids** — two tasks with the same id (numeric or named) are
  rejected, as are duplicate subtask numbers under one parent.

## Worked Examples

The [`examples/`](../../examples/) directory contains end-to-end plans that
exercise the patterns above:

- [`release-automation.rhei.md`](../../examples/release-automation.rhei.md) —
  mixed numeric and named task ids with fenced code inside a subtask.
- [`human-review-loop.rhei.md`](../../examples/human-review-loop.rhei.md) —
  multi-state workflow with chained dependencies.
- [`escaped-state-values.rhei.md`](../../examples/escaped-state-values.rhei.md) —
  multi-word state values paired with a custom states file.

## Advancing Task States with `rhei transition`

While `**State:**` values can be edited by hand, the `rhei transition`
command provides an atomic, validated way to advance a task's state:

```bash
rhei transition my-plan.rhei.md --task 1 --from pending --to in-progress
```

The command:

1. **Acquires a file lock** on the plan to prevent concurrent writes.
2. **Reads the current state** of the specified task.
3. **Compare-and-swap** — if the task's current state does not match
   `--from`, the command fails with a conflict error. This prevents two
   agents from claiming the same task.
4. **Validates the transition** against the state machine — illegal
   transitions are rejected before any write occurs.
5. **Writes the new state** to the markdown and releases the lock.

### Flags

| Flag             | Required | Description                                     |
| ---------------- | -------- | ----------------------------------------------- |
| `--task <id>`    | Yes      | Task id (numeric or named) to transition        |
| `--from <state>` | Yes      | Expected current state (compare-and-swap guard) |
| `--to <state>`   | Yes      | Target state                                    |
| `--json`         | No       | Emit result as JSON instead of plain text       |

On success the command prints the updated state. On conflict (another
agent already transitioned the task) it exits non-zero with a message
indicating the actual current state. Agents should re-read the plan and
re-select when this happens.

### Parallel safety

When multiple agents work on the same plan, `rhei transition` is the
coordination primitive. Because the `--from` flag acts as a
compare-and-swap guard, only one agent can win a race on the same task —
the loser gets a clean error and picks a different task. See
[How Rhei Is Used — Pattern 3](rhei-usage.spec.md) for the full
parallel-workers pattern.

## Next Steps

- Read the [Plan Language Specification](../rhei.spec.md) for the
  formal grammar and semantic constraints.
- Browse the [States Specification](rhei-states.spec.md) to define project-
  specific workflows.
- Use `rhei render --format github` to produce review-friendly views of
  a plan, or `--format progress` for a terminal overview.
