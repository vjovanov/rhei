# `rhei reset`

Reset every task in a plan to its resolved profile's `initial` state and remove runtime output. This is the inverse of a forward run: it restores the plan to a clean, pre-execution state so the same plan can be re-executed from scratch.

## Usage

```bash
rhei reset <RHEI_PLAN_OR_WORKSPACE>
```

`<RHEI_PLAN_OR_WORKSPACE>` may be either a `.rhei.md` file (single-file plan) or a directory workspace root (containing `index.rhei.md` and `tasks/`).

## Behavior

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

## Safety

Reset is destructive with respect to runtime state. It does not prompt and has no `--dry-run` flag; callers that need a preview should inspect `runtime/` and the current `**State:**` values before invoking it.

Because reset operates under a file lock, it is safe against concurrent `rhei next` / `rhei transition` / `rhei complete` calls: those calls either run before the reset acquires the lock or after it releases.

## Output

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

## Relationship to Other Commands

`rhei reset` inverts the forward commands (`next`, `transition`, `complete`, `run`): it returns every task to its profile's `initial` state and removes the `runtime/` directory.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#command-surface) for the full table comparing all five coordination commands.

## Related Specifications

- [Plan Language Specification](../rhei.spec.md) — plan formats and semantic constraints
- [States Specification](rhei-states.spec.md) — profile resolution and `initial` state rules
- [Next Command](rhei-next.spec.md), [Complete Command](rhei-complete.spec.md), [Transition Command](rhei-transition-cmd.spec.md) — forward commands that reset inverts
