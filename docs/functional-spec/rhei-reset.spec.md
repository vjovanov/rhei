# FS-rhei-reset: `rhei reset`

Reset every task in a plan to its resolved profile's `initial` state and remove runtime output. This is the inverse of a forward run: it restores the plan to a clean, pre-execution state so the same plan can be re-executed from scratch.

## 1. Usage

```bash
rhei reset <RHEI_PLAN_OR_WORKSPACE> [--rhei <RHEI_ID>]
```

`<RHEI_PLAN_OR_WORKSPACE>` may be a `.rhei.md` file (single-file plan), a directory workspace root (containing `index.rhei.md` and `tasks/`), or a Panta project directory.

### 1.1. Options

| Flag               | Default | Description                                                                    |
|--------------------|---------|--------------------------------------------------------------------------------|
| `--rhei <RHEI_ID>` | all     | Narrow the reset to the named rheis (repeatable). See §2.1. §FS-rhei-panta.6.4 |

An id that names no rhei in the project is an error listing the available rhei
ids.

## 2. Behavior

1. Load the state machine and plan. Validate the plan (reset refuses to operate on an invalid plan).
2. Acquire a file lock on the plan file (single-file) or on `index.rhei.md` (workspace).
3. For every task node in the merged task graph (including all descendants):
   - Resolve the task's profile through `node_policy`.
   - Rewrite the task's `**State:**` line to the profile's `initial` state.
   - Remove the `**Assignee:**` line if present.
   - Remove the `> **Result:**` link block from the task body if present.
   - Clear any counted-visit suffix; `stateVisits` entries for the task in frontmatter `metadata.tasks.<id>.stateVisits` are deleted.
4. For a directory workspace, delete the `runtime/` directory at the workspace root if it exists. For a single-file plan, delete the `runtime/` directory next to the plan file if it exists. This removes result files, findings, logs, and journaled transition records.
5. Write each modified task file atomically (temp file + rename). Release the lock.

Reset does **not**:

- Modify the `# Rhei:` title, content sections, `**Prior:**` lines, or task descriptions.
- Remove user-authored files outside of `runtime/`.
- Alter the state machine or template source of the plan.

Reset is project-wide by default. Because it destroys runtime state across
every in-scope rhei, it reports its resolved scope and the affected rheis
before acting; a one-rhei project has no fan-out to report and stays quiet
(§FS-rhei-panta.6.4).

### 2.1. Narrowed Reset (`--rhei`)

A narrowed reset must never delete whole `runtime/` trees: sibling single-file
rheis share one execution root, so removing the tree would destroy an
out-of-scope rhei's state. Instead it removes exactly what is **keyed by an
in-scope ticket id**, under that ticket's owning rhei execution root — and,
when they differ, under the project execution root as well, because
run-orchestrated logs and captures land there even for tickets owned by a
subdirectory rhei:

- `runtime/results/<ticket-id>.md`
- `runtime/logs/task-<ticket-id>-*`
- every artifact the resolved state machine declares as an `inputs:`/`outputs:`
  path containing `{task_id}` — a stale output left behind would otherwise
  satisfy a required input on the next run
- `runtime/snapshot-sessions/<ticket-id>-*`
- `runtime/worktree-refs/<ticket-id>.yaml`
- `runtime/accounting/captures/<ticket-id>-*` and
  `runtime/accounting/tasks/<ticket-id>.json`
- the ticket's lines in `runtime/state-transitions.log`, so a reset ticket's
  recorded history cannot claim a completion its plan no longer holds

Artifact paths that still carry unresolved placeholders after `{task_id}` is
substituted (`{state}`, `{visit_count}`, `{model}`, …) are matched by the
literal prefix up to the first remaining placeholder, so `auth.1` never matches
`auth.10`.

Run-scoped output is **not** ticket-owned — the run report, the dashboard, and
the accounting rollups describe a run, not a ticket — so a narrowed reset keeps
it and says so on stdout. Reset without `--rhei` to clear it.

## 3. Safety

Reset is destructive with respect to runtime state. It does not prompt and has no `--dry-run` flag; callers that need a preview should inspect `runtime/` and the current `**State:**` values before invoking it.

Because reset operates under a file lock, it is safe against concurrent `rhei next` / `rhei transition` / `rhei complete` calls: those calls either run before the reset acquires the lock or after it releases.

## 4. Output

On success, two lines are printed:

```text
Reset <N> task(s) to initial state '<initial>'.
Removed runtime output.
```

When the task graph contains child tasks, the first line also reports the
descendant count:

```text
Reset <N> task(s) (and <M> descendant task(s)) to initial state '<initial>'.
```

The second line is `No runtime output was present.` when the `runtime/`
directory did not exist.

A project-scoped invocation prints its resolved scope first:

```text
Scope: `rhei reset` operates project-wide across <N> rheis: <ids>
```

and a narrowed one reports what it deliberately kept after the two summary
lines:

```text
Scope: `rhei reset` narrowed to <N> rheis: <ids>
Reset <N> task(s) to initial state '<initial>'.
Removed runtime output.
Kept run-scoped output not owned by any ticket (run report, dashboard, accounting rollups). Reset without `--rhei` to clear it.
```

## Relationship to Other Commands

`rhei reset` inverts the forward commands (`next`, `transition`, `complete`, `run`): it returns every task to its profile's `initial` state and removes the `runtime/` directory.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#22-command-surface) for the full table comparing all five coordination commands.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — plan formats and semantic constraints
- [States Specification](rhei-states.spec.md) — profile resolution and `initial` state rules
- [Next Command](rhei-next.spec.md), [Complete Command](rhei-complete.spec.md), [Transition Command](rhei-transition-cmd.spec.md) — forward commands that reset inverts
