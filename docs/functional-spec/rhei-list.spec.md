# FS-rhei-list: `rhei list`

Read-only listing of tasks in a plan, with filters for state, assignee, kind,
dependency, hierarchy, free-text, and readiness. Modeled after `bd list` from
beads, restricted to fields Rhei stores in markdown (no priority, labels, or
timestamps).

## 1. Usage

```bash
rhei list <RHEI_PLAN> [FILTERS] [--rhei <RHEI_ID>] [--limit N] [--json]
```

`<RHEI_PLAN>` is a single-file plan, a Directory Workspace, or a Panta project
directory. Listing is project-wide: filters apply across every rhei, and
`--rhei` narrows the listing to named rheis (§FS-rhei-panta.6.4).

## 2. Options

| Flag                     | Description                                                                       |
|--------------------------|-----------------------------------------------------------------------------------|
| `--rhei <RHEI_ID>`       | Only tickets in the named rheis. Repeatable. An id that names no rhei in the project is an error listing the available ids. §FS-rhei-panta.6.4 |
| `--state <STATE>`        | Filter by state. Repeatable; comma-separated also accepted. Aliases are normalized per owning machine. A state no loaded machine declares is an error (§2.1). |
| `--assignee <ASSIGNEE>`  | Exact `**Assignee:**` match. Mutually exclusive with `--no-assignee`.            |
| `--no-assignee`          | Only tasks with no `**Assignee:**` field.                                         |
| `--kind <KIND>`          | Filter by node kind (e.g. `task`, `bug`, `spec`). Case-insensitive.               |
| `--has-prior <TASK_ID>`  | Only tasks that list `<TASK_ID>` in their `**Prior:**` dependencies.              |
| `--parent <TASK_ID>`     | Only direct children of `<TASK_ID>`. Mutually exclusive with `--root`.            |
| `--root`                 | Only top-level tasks (no parent).                                                 |
| `--contains <TEXT>`      | Case-insensitive substring match against task title and content body.             |
| `--terminal`             | Only tasks whose state is terminal in the resolved state machine.                 |
| `--non-terminal`         | Only tasks whose state is non-terminal. Mutually exclusive with `--terminal`.     |
| `--ready`                | Only tasks whose descendants are all terminal, whose `**Prior:**` set is satisfied, and whose state is non-terminal and non-gating — the claimable set (§3.1). Mutually exclusive with `--blocked`. |
| `--blocked`              | Only non-terminal tasks with at least one unsatisfied prerequisite.               |
| `--limit <N>`            | Cap the number of printed tasks. `0` means no limit (default).                    |
| `--json`                 | Emit a JSON array instead of human-readable text.                                 |

Filters combine with logical AND. Empty result sets are not an error.

### 2.1. Filter values that name nothing

A filter *value* that cannot exist is a different thing from a filter that
matches nothing, and the two must not look alike. `--rhei` already draws that
line: an id naming no rhei is an error listing the available ids "rather than a
silently empty scope" (§FS-rhei-panta.6). `--state` follows the same rule — a
value no loaded machine declares is an error naming the states that do exist:

```text
unknown state 'in-reveiw'; states in this project: cancelled, completed, fix, pending, review
```

A silent `(no tasks match the given filters)` reads as "no work is in that
state", which is exactly the wrong conclusion to hand someone whose state was
renamed out from under a script, or who typed it slightly wrong.

Validation is against every machine the project loads, not the `--rhei` scope.
A project runs one machine per rhei (§AR-rhei-panta.4), so `--rhei billing
--state review` names a state that genuinely exists while no in-scope ticket
can hold it. That is an honest empty result, not a mistake.

`--assignee` takes no such check: any string is a legitimate assignee, and
there is no declared set to check it against.

## 3. Behavior

1. Load the plan and resolve each rhei's state machine the same way
   `rhei validate` does (auto-discovery, `**States:**` field,
   `--state-machine` override). A project resolves one machine per rhei
   (§AR-rhei-panta.4), and every state judgment below — normalization,
   terminality, gating — uses the machine of the rhei that owns the ticket.
2. Walk the task tree in source order, recording each task with its parent id.
3. Apply filters in order; normalize `--state` values and the task's own state
   through that ticket's machine so aliases match.
4. For `--ready` / `--blocked`, evaluate prerequisites against the current
   plan state using the same dependency rule as `rhei next` (terminal,
   non-cancelled).
5. Apply `--limit` after filtering.

### 3.1. What `--ready` means

`--ready` answers "what work could be picked up", so it lists exactly the set
`rhei next` draws from — including the descendant rule that command applies
(§FS-rhei-next.3). A ticket whose subtree is still open is not work anyone can
be handed: its children are. Once every descendant is terminal the parent is
ordinary claimable work and is listed like any other ticket, because a non-leaf
ticket is a task in its own right (§FS-rhei-plan-language.3).

`--ready` reports *readiness*, not *availability*: a ready ticket that already
carries an `**Assignee:**` is still listed, because whether someone has claimed
it is a separate question with its own filters. Compose them for the narrower
answer — `rhei list --ready --no-assignee` is the unclaimed ready work.
6. Emit the result. The plan file is **not** modified and no lock is acquired.

## 4. Output

### 4.1. Text (default)

One task per line, indented two spaces per depth level *within its rhei* — the
Panta qualification segment adds no indentation, so top-level tickets stay
flush-left — in source order:

```text
Task release.1: Define pipeline contracts [pending]
  Task release.1.1: Capture deployment events [pending]
Task release.2: Bootstrap environments [pending] (prior: release.1)
Task release.3: Roll out release bot [in-progress] (prior: release.1, release.2) @claude-code
```

Ticket ids are project-qualified, including for a bare rhei loaded directly:
`release.rhei.md` is the single rhei of an implicit Panta with the id `release`
(§FS-rhei-panta.6, §AR-rhei-panta.3).

Tickets print in the merged graph's source order: discovered rheis in
deterministic discovery order, with the `basin` rhei's tickets last because the
basin loads after every discovered rhei (§FS-rhei-panta.4). No rhei-level
headings or visual de-emphasis are applied; rhei-level grouping is deferred
(§FS-rhei-panta.3).

The `(prior: …)` suffix is omitted when the task has no prerequisites; the
`@<assignee>` suffix is omitted when the task is unclaimed.

A rhei holding no tickets is named after the ticket lines, in the wording
`rhei render --format progress` already uses for it:

```text
Task auth.1: Rotate signing keys [pending]

Billing (billing): (no tickets yet)
```

`rhei init` ends by telling the reader to run `rhei new "<title>"`, which makes
the very next `rhei list` the moment a project holds one rhei and no tickets —
and a listing that showed nothing at all would read as though the create had
not landed. Only the text output names them, and only when no filter is active:
a filter asks a question about tickets, and a rhei with none has no answer to
give. The JSON array is unchanged, because its shape is a contract and an empty
rhei is not a ticket. This is not rhei-level grouping, which stays deferred
(§FS-rhei-panta.3).

When no task matches, `rhei list` prints `(no tasks match the given filters)`
and exits 0.

### 4.2. JSON (`--json`)

A flat array of objects (no hierarchy nesting); the `parent` field carries the
parent id when present.

```json
[
  {
    "id": "release.2",
    "kind": "task",
    "title": "Bootstrap environments",
    "state": "draft",
    "assignee": null,
    "prior": ["release.1"],
    "parent": null,
    "depth": 1
  }
]
```

Fields are stable: `id`, `kind`, `title`, `state` (raw, as authored), `assignee`
(string or `null`), `prior` (array of id strings), `parent` (string or `null`),
`depth` (1-based depth within the owning rhei — a top-level ticket is `1`; the
Panta qualification segment does not count).

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
