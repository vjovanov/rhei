# FS-rhei-list: `rhei list`

Read-only listing of tasks in a plan, with filters for state, assignee, kind,
dependency, hierarchy, free-text, and readiness. Modeled after `bd list` from
beads, restricted to fields Rhei stores in markdown (no priority, labels, or
timestamps).

## 1. Usage

```bash
rhei list <RHEI_PLAN> [FILTERS] [--limit N] [--json]
```

`<RHEI_PLAN>` is a single-file plan or a Directory Workspace path.

## 2. Options

| Flag                     | Description                                                                       |
|--------------------------|-----------------------------------------------------------------------------------|
| `--state <STATE>`        | Filter by state. Repeatable; comma-separated also accepted. Aliases are normalized through the resolved state machine. |
| `--assignee <ASSIGNEE>`  | Exact `**Assignee:**` match. Mutually exclusive with `--no-assignee`.            |
| `--no-assignee`          | Only tasks with no `**Assignee:**` field.                                         |
| `--kind <KIND>`          | Filter by node kind (e.g. `task`, `bug`, `spec`). Case-insensitive.               |
| `--has-prior <TASK_ID>`  | Only tasks that list `<TASK_ID>` in their `**Prior:**` dependencies.              |
| `--parent <TASK_ID>`     | Only direct children of `<TASK_ID>`. Mutually exclusive with `--root`.            |
| `--root`                 | Only top-level tasks (no parent).                                                 |
| `--contains <TEXT>`      | Case-insensitive substring match against task title and content body.             |
| `--terminal`             | Only tasks whose state is terminal in the resolved state machine.                 |
| `--non-terminal`         | Only tasks whose state is non-terminal. Mutually exclusive with `--terminal`.     |
| `--ready`                | Only tasks whose `**Prior:**` set is satisfied and whose state is non-terminal and non-gating. Mutually exclusive with `--blocked`. |
| `--blocked`              | Only non-terminal tasks with at least one unsatisfied prerequisite.               |
| `--limit <N>`            | Cap the number of printed tasks. `0` means no limit (default).                    |
| `--json`                 | Emit a JSON array instead of human-readable text.                                 |

Filters combine with logical AND. Empty result sets are not an error.

## 3. Behavior

1. Load the plan and resolve the state machine the same way `rhei validate` does
   (auto-discovery, `**States:**` field, `--state-machine` override).
2. Walk the task tree in source order, recording each task with its parent id.
3. Apply filters in order; normalize `--state` values and the task's own state
   through the state machine so aliases match.
4. For `--ready` / `--blocked`, evaluate prerequisites against the current
   plan state using the same dependency rule as `rhei next` (terminal,
   non-cancelled).
5. Apply `--limit` after filtering.
6. Emit the result. The plan file is **not** modified and no lock is acquired.

## 4. Output

### 4.1. Text (default)

One task per line, indented two spaces per depth level, in source order:

```text
Task 1: Define pipeline contracts [pending]
  Task 1.1: Capture deployment events [pending]
Task 2: Bootstrap environments [pending] (prior: 1)
Task 3: Roll out release bot [in-progress] (prior: 1, 2) @claude-code
```

The `(prior: …)` suffix is omitted when the task has no prerequisites; the
`@<assignee>` suffix is omitted when the task is unclaimed.

When no task matches, `rhei list` prints `(no tasks match the given filters)`
and exits 0.

### 4.2. JSON (`--json`)

A flat array of objects (no hierarchy nesting); the `parent` field carries the
parent id when present.

```json
[
  {
    "id": "2",
    "kind": "task",
    "title": "Bootstrap environments",
    "state": "draft",
    "assignee": null,
    "prior": ["1"],
    "parent": null,
    "depth": 1
  }
]
```

Fields are stable: `id`, `kind`, `title`, `state` (raw, as authored), `assignee`
(string or `null`), `prior` (array of id strings), `parent` (string or `null`),
`depth` (1-based segment count).

## Relationship to Other Commands

- `rhei list --ready` lists *all* currently ready tasks; `rhei next --peek`
  selects the *single* task that would be claimed next.
- `rhei list` never mutates plan state; for state changes use `rhei transition`,
  `rhei next`, or `rhei complete`.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — grammar and semantic constraints
- [States Specification](rhei-states.spec.md) — state machine format and terminal/gating semantics
- [Next Command](rhei-next.spec.md) — single-task claim with `--peek`
- [Transition Command](rhei-transition-cmd.spec.md) — atomic state change
