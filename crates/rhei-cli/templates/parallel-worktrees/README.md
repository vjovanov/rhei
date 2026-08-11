# parallel-worktrees

Apply one shared task across many independent targets **at the same time**.
Each target becomes its own task in its own git worktree on its own branch, so
`rhei run --parallel N` advances them concurrently without several agents
fighting over a single checkout. This is the canonical reference for the
**parallel-execution** and **per-task git-worktree** patterns.

## What it does

The `tasks/01-targets.md` file loops over the `targets` array and emits one
top-level task per target, with **no `**Prior:**` between them** — so every
task is ready at once. Each task walks the same four-state machine:

```
prepare-worktree [initial]  -->  work  -->  integrate  -->  completed [final]
   create <worktree_root>/        do the      commit on
   <task_id> on branch            edits in    branch, report
   <branch_prefix>/<task_id>      the worktree
```

A wildcard `* → cancelled` lets a human abort any target. Per-task path (every
target is identical and independent):

| Task | Path through the machine |
|---|---|
| `<target.id>` | `prepare-worktree → work → integrate → completed` |

## What makes it parallel

Two things must **both** hold — this template does both:

1. **Independent ready tasks.** The fan-out emits sibling tasks with no
   prerequisites linking them, so all are claimable immediately.
2. **`concurrent: true` on the working states.** Without it, `rhei run`
   schedules at most one task per state per pass and the fan-out serializes —
   regardless of `--parallel`. With it, `rhei run --parallel N` works up to `N`
   tasks in the same state together.

`--parallel` only takes effect on **directory workspaces** (it is ignored on
single-file `plan.rhei.md` plans), which is why this template ships an
`index.rhei.md` + `tasks/` layout rather than a single file.

## What makes it safe under concurrency

Each task derives its worktree path and branch from its task id
(`<worktree_root>/<task_id>`, branch `<branch_prefix>/<task_id>`), so
parallel agents edit disjoint checkouts and never collide. Runtime artifacts
(`{output.*.path}`) are written back to the scratchpad workspace, not inside
the worktree. Branches are left for human review — the template never merges.

> Worktree creation here is an **instruction pattern**, not a Rhei feature: the
> `prepare-worktree` state tells the agent to run `git worktree add`. If you
> want it deterministic, make `prepare-worktree` a `program:` state that runs
> git directly; the rest of the machine is unchanged.

## Inputs

| Input | Type | Default | What it does |
|---|---|---|---|
| `task` | string | *(required)* | The instruction applied to every target. |
| `targets` | object[] | 2 illustrative entries | The fan-out set. Each `{ id, path }` becomes one parallel task; `id` must be a branch/dir-safe slug (validated against `[a-z][a-z0-9-]*`) and unique across targets, since it names the branch, worktree dir, and task; `path` scopes the work. |
| `batch_title` | string | `Parallel Worktree Batch` | Workspace index title. |
| `agent` | string | `claude-code[yolo]:…opus-4-7` | Agent that sets up the worktree, edits, and commits. |
| `branch_prefix` | string | `batch` | Branch is `<branch_prefix>/<task_id>`. |
| `worktree_root` | string | `runtime/worktrees` | Where each task's worktree is created. |

## Quick start

`targets` is an object array, so pass it via a values file:

```yaml
# batch.yaml
task: |
  Add a crate-level //! doc comment summarizing the crate. Do not change code.
targets:
  - { id: cli,  path: crates/rhei-cli }
  - { id: core, path: crates/rhei-core }
branch_prefix: docs-pass
```

```bash
rhei instantiate parallel-worktrees \
  --values batch.yaml \
  --output .agents/scratchpad/parallel-worktrees/

rhei run .agents/scratchpad/parallel-worktrees/ --parallel 3
```

## What's bundled

- `template.yaml` — the inputs.
- `states.yaml` — the `parallel-worktrees` machine (diagram + the `concurrent`
  note in the top comment).
- `settings.json` — project defaults with `agent_timeout: 20m`, so the quick
  start runs without requiring a global timeout setting.
- `index.rhei.md` — workspace index listing the targets and branches.
- `tasks/01-targets.md` — the per-target `for`-loop fan-out that emits one task per target.

## Example

A pre-rendered example lives at
[`examples/parallel-worktrees-example/`](../../../../examples/parallel-worktrees-example/)
and passes `rhei validate` as shipped. Its README shows the `--parallel`
dry-run side by side with the sequential one.
