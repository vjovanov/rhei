# FS-rhei-batch-run: `rhei batch-run`

Run a directory of generated Rhei plans as one batch. This command is intended
for planner outputs such as `runtime/generated-plans/*.rhei.md`, where each
plan remains an ordinary plan executed by `rhei run`. It supports repeatable
flows and predictable execution without replacing the single-plan run loop.
§GOAL-rhei-outcomes §FS-rhei-run

## 1. Usage

```bash
rhei batch-run <PLANS_DIR> [flags]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--glob <PATTERN>` | `*.rhei.md` | Match plan files under `<PLANS_DIR>`. A pattern without `/` matches file names; a pattern with `/` matches the normalized relative path. |
| `--batch-state-machine <PATH>` | unset | State machine passed to every child `rhei run` in the batch. Overrides the global `--state-machine` for this subcommand. |
| `--batch-workflow-state-machine <PATH>` | unset | Materialize the discovered plans as a generated parent batch workspace and run that workspace with the selected state machine. |
| `--tickets-dir <DIR>` | auto | Directory containing ticket markdown files named by generated batch task id. When unset, a sibling `tickets/` directory under the same runtime root is used if present. |
| `--parallelism <N>` | `1` | Maximum number of plan runs active at once. Values greater than `1` warn because multiple plans may edit the same worktree. |
| `--inner-parallelism <N>` | `1` | Passed to each nested `rhei run <plan> --parallel <N>`. |
| `--sleep <DURATION>` | unset | Wait after a nested `rhei run` finishes before that worker starts another plan. Uses the same duration syntax as run timeouts, for example `30s`, `5m`, or `1h`. |
| `--continue-on-error` | false | Continue scheduling remaining plans after a validation or run failure. The batch still exits non-zero if any plan failed. |
| `--dry-run` | false | Print discovered order and planned nested commands without writing files, mutating plans, or spawning agents. |
| `--dashboard` | false | Enable the optional parent batch dashboard. Nested run dashboards are enabled by default. |
| `--no-dashboard` | false | Disable nested run dashboards and the optional parent batch dashboard. |
| `--tui` | auto | Force the parent batch TUI even when stdout is not detected as a TTY. |
| `--no-tui` | false | Force plain stdout for the parent batch UI. Reports are still written. |
| `--agent <AGENT>` | unset | Passed to each nested `rhei run`. |
| `--agent-mode <MODE>` | unset | Passed to each nested `rhei run`. |
| `--model <MODEL>` | unset | Passed to each nested `rhei run`. |

The global `--state-machine <PATH>` option applies to validation and is passed
through to every nested run. `--batch-state-machine <PATH>` is a batch-run local
alias for the same child-run behavior and takes precedence when both are set.
`--batch-workflow-state-machine <PATH>` controls only the generated parent batch
workspace used for post-batch states such as `create-pr`.

## 2. Discovery

`rhei batch-run` recursively discovers files under `<PLANS_DIR>` that match the
glob pattern, then sorts them deterministically by normalized relative path.
Normalized paths use `/` separators and omit `.` components. This order is the
dry-run display order and the scheduling order.

## 3. Execution

Before running a plan, the batch runner validates that plan with the same
validation behavior as `rhei validate`. If validation fails, the plan is marked
failed and `rhei run` is not invoked for that plan.

Each successful validation is followed by a nested command equivalent to:

```bash
rhei run <plan> --parallel <inner-parallelism>
```

The batch runner invokes the existing `rhei run` command behavior rather than
duplicating the run loop. Agent selection flags are appended to the nested run
only when the operator supplied them. Existing `rhei run` semantics remain
unchanged. §FS-rhei-run

By default, the nested command also receives `--dashboard --no-tui` so each
running plan exposes its own dashboard URL without competing for the terminal:

```bash
rhei run <plan> --parallel <inner-parallelism> --dashboard --no-tui
```

With `--no-dashboard`, nested runs receive `--no-dashboard --no-tui` instead:

```bash
rhei run <plan> --parallel <inner-parallelism> --no-dashboard --no-tui
```

### 3.1. Batch Workflow State Machine Mode

With `--batch-workflow-state-machine <PATH>`, `rhei batch-run` does not use the
built-in queue runner directly. Instead it writes a generated Directory
Workspace under the batch report directory. The workspace contains one task for
each discovered plan, copies each plan to
`inputs/generated-plans/{task_id}.rhei.md`, and copies matching tickets to
`inputs/tickets/{task_id}.md` when available. Ordered plan file prefixes such
as `01-` are stripped when deriving `{task_id}`.

The generated plan tasks start in the selected batch machine's default profile
initial state. If the selected batch state machine declares a `create-pr` state,
the generated workspace also includes a final `create-pr` task whose `Prior:`
list contains every generated plan task. This lets teams choose a batch workflow
that performs execution, review, publication, pull-request creation, or any
other post-batch step encoded in the state machine.

For the generated plan-execution state, batch-run writes a concrete program into
the generated state machine that validates the copied plan and runs it with the
requested nested parallelism, agent overrides, and nested dashboard setting. The
program tees nested output to both the execution report and the parent run
stream, so nested `Dashboard: ...` lines are available to the batch TUI.

When the plans directory contains `.agents/rhei/settings.json`, those settings
are copied both to the generated batch workspace root and to
`inputs/generated-plans/` so nested single-file plan runs keep the same agent
timeouts and defaults they had in the source directory.

The generated parent workspace is then executed with:

```bash
rhei --state-machine <batch-workflow-state-machine> run <generated-batch-workspace> --parallel <parallelism>
```

The batch TUI remains terminal-first in this mode. The parent browser dashboard
is still opt-in via `--dashboard`. If `--sleep` is set, the sleep occurs after
the generated batch workspace run finishes.

### 3.2. Parent Batch TUI

The parent batch UI is a terminal-first control surface. When stdout is a TTY
and `--no-tui` is not set, `rhei batch-run` uses the terminal TUI by default.
`--tui` forces the parent TUI. `--no-tui` forces plain stdout.

The TUI lists the discovered plans from the scheduler pass, not only the active
worker slots. Each row shows whether that plan is pending, running, succeeded,
failed, or skipped; active rows show their worker slot and per-plan log path.
When a nested `rhei run` prints its dashboard URL, the batch TUI attaches that
URL to the corresponding plan row so the operator can open the child run's
normal dashboard.

By default, `rhei batch-run` does not start a parent batch browser dashboard.
`--dashboard` explicitly enables one for the batch report directory. The parent
dashboard, when enabled, receives the same live events as the TUI and records
validation, nested run completion, nested dashboard links, and per-plan log
paths. `--no-dashboard` disables both the optional parent dashboard and the
nested run dashboards.

## 4. Failure Handling

By default, the first validation or nested-run failure stops scheduling new
plans. Runs already active are allowed to finish. Plans that were discovered but
not started are recorded as skipped, and the batch exits non-zero.

With `--continue-on-error`, every discovered plan is attempted. The batch exits
non-zero if any plan failed.

## 5. Dry Run

With `--dry-run`, `rhei batch-run` prints:

1. the deterministic plan order;
2. the nested `rhei run` command that would be invoked for each plan.

It does not create a batch report directory, write logs, validate plans, mutate
plan files, or spawn agents.

## 6. Reports

Non-dry batches write durable reports below a runtime directory near
`<PLANS_DIR>`. For paths already under a `runtime/` directory, reports are
written under that runtime root; otherwise they are written under
`<PLANS_DIR>/runtime/`.

Each batch run creates a timestamped directory under `batch-runs/` containing:

- a per-plan log file for validation failures, spawn failures, and nested run
  stdout/stderr;
- `plans.json`, with one record per discovered plan: path, normalized path,
  start and end time, validation result, nested run exit code, status, command
  args, and log path;
- `summary.json`, with total, succeeded, failed, skipped, and elapsed counts;
- `batch-report.json`, containing both the summary and per-plan records.

Report timestamps are UTC ISO 8601 strings.

## Related Specifications

- [`rhei run`](rhei-run.spec.md) §FS-rhei-run
- [`rhei validate`](rhei-validate.spec.md) §FS-rhei-validate
