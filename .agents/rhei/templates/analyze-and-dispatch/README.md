# analyze-and-dispatch

A coordinator task **analyzes a subject and then creates the follow-up tasks
itself**, at run time. The number and shape of the spawned tasks is decided by
the analyzing agent — not fixed at instantiation — so this is the canonical
reference for *dynamic (agent-driven) task creation*: the "fan out, but the
count is a judgment call" pattern.

## What it does

The shipped workspace contains exactly one task, `analyze`. Its agent inspects
`subject` against `analysis_brief`, picks a set of independent work items (up to
`max_tasks`), and for each one writes a new `tasks/NN-<slug>.md` file into the
workspace. It also writes a `report` task whose `**Prior:**` lists every work
item. Then it completes. On the next `rhei run` pass the new tasks are ready and
each `address` task is handled by `worker_agent`; once all are done the `report`
task runs.

## How a task creates tasks (the rhei "API")

There is **no `rhei add` command and no programmatic add-task call**. Rhei
re-reads the `tasks/` directory of a directory workspace on every `rhei run`
pass, so a running agent adds work simply by **writing a conforming task file**:

```
tasks/NN-<slug>.md
  ### Task <slug>: <title>
  **State:** address
  **Prior:** Task <coordinator-id>

  <what this work item requires>
```

The conforming markdown file *is* the API. The coordinator copies its own
`{task_id}` (already substituted by the runtime) into each new task's
`**Prior:**` so the spawned tasks wait for the coordinator to finish. This is
the same mechanism `spec-implementation` and `changeset-review` use; this
template isolates it so the pattern is easy to read and copy.

## When to use it

- The number of follow-up tasks depends on what an agent finds (audit findings,
  files matching a condition, backlog items, sub-problems of a spike).
- For a fan-out whose count is known at instantiation, use `parallel-worktrees`
  (a `{% raw %}{% for %}{% endraw %}` over an array input) instead — it needs no
  coordinator.

## Inputs

| Input | Type | Default | What it does |
|---|---|---|---|
| `subject` | string | *(required)* | What the coordinator analyzes (a path, document, dataset, backlog). |
| `analysis_brief` | string | *(required)* | How to analyze and what counts as one spawned task. Pass long text with `--set-file` or `--values`. |
| `plan_title` | string | `Analyze & Dispatch` | Workspace index title. |
| `coordinator_agent` | string | `claude-code[yolo]:…opus-4-7` | Agent that analyzes and writes the task files. |
| `worker_agent` | string | `claude-code[yolo]:…opus-4-7` | Agent that handles each work item and the report. |
| `max_tasks` | number | `8` | Upper bound on dispatched tasks, so a bad analysis can't spawn unbounded work. |

## Per-task paths

| Task | Created | Path through the machine |
|---|---|---|
| `analyze` (coordinator) | shipped | `analyze → completed` |
| work item (`<slug>`) | dynamically, by `analyze` | `address → completed` |
| `report` | dynamically, by `analyze` | `report → completed` |

The full state machine diagram is in the top comment of
[`states.yaml`](./states.yaml).

## Quick start

```bash
rhei instantiate analyze-and-dispatch \
  --set subject=docs/functional-spec \
  --set-file analysis_brief=./brief.md \
  --output .agents/scratchpad/dispatch/

rhei run .agents/scratchpad/dispatch/          # coordinator runs, writes tasks
rhei run .agents/scratchpad/dispatch/ --parallel 4   # work items run in parallel
```

## What's bundled

- `template.yaml` — the inputs.
- `states.yaml` — the `analyze-and-dispatch` machine (diagram + the task-creation
  contract live in the `analyze` state's instructions). `address` is
  `concurrent: true` so dispatched items run under `--parallel`.
- `settings.json` — project defaults with `agent_timeout: 20m`, so the quick
  start runs without requiring a global timeout setting.
- `index.rhei.md` — workspace index.
- `tasks/01-analyze.md` — the single shipped coordinator task; everything else is
  created at run time.

## Example

A pre-rendered example lives at
[`examples/analyze-and-dispatch-example/`](../../../../examples/analyze-and-dispatch-example/)
and passes `rhei validate` as shipped. Its README shows a simulated post-analysis
pass so you can see the dispatched tasks become ready.
