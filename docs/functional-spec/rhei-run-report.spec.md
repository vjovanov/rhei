# FS-rhei-run-report: Per-Run Report

`rhei run` writes a durable Markdown report at the end of every run so an
operator can understand what happened without replaying the browser dashboard,
reading raw logs, or knowing the internal artifact-reuse rules. The report is a
commit-friendly UI: plain Markdown first, with enough structure for dashboards
and future JSON export to render the same facts. It complements the live TUI and
Flow dashboard; it does not replace either surface. §GOAL-rhei-outcomes

The report answers four questions:

1. What final state did every task reach?
2. Which transitions were taken, and why were those transitions allowed?
3. Did Rhei spawn an agent or program, or did it advance because existing
   artifacts already satisfied the state contract?
4. Which tasks did not advance, and what concrete condition kept them there?

## 1. Report Artifact

At the end of `rhei run`, Rhei writes:

- `runtime/run-report.md` - latest report for quick inspection.
- `runtime/run-reports/<timestamp>-<run-id>.md` - immutable history entry.

The latest report may be overwritten by the next run. Timestamped reports are
append-only unless `rhei reset` removes `runtime/`, matching the existing runtime
artifact lifecycle. A failed run still writes a report with the information
available up to the failure point. A `--dry-run` is the one exception: it is a
side-effect-free preview and writes no report file (§3.5).

The report uses relative links for logs and artifacts so it remains useful after
the workspace is moved, committed, archived, or pasted into an issue.

## 2. Markdown UI

The report layout is optimized for scan-first reading:

1. **Header** - plan title, workspace path, run id, start/end time, duration,
   command mode, parallelism, dashboard artifact path, and overall result.
2. **Outcome Strip** - small summary table for final task states, transitions,
   spawned agents, spawned programs, callback-only advances, reused-output
   advances, dry-run transitions, terminal-at-start tasks, and blocked tasks.
   When accounting exists, the strip also shows run cost, input tokens, cached
   input tokens, output tokens, cached output tokens, and coverage.
3. **Attention** - the shortest path to action: blocked tasks, gating tasks, and
   halted tasks with the exact reason.
4. **Transition Ledger** - one row per transition attempt or no-op observation,
   in chronological order.
5. **Task Final States** - source-order task tree with final state, start state,
   terminal-at-start marker, and last transition.
6. **Artifacts Checked** - grouped artifact evidence for readers investigating
   reuse, missing inputs, or missing outputs.
7. **Invocations** - spawned agents/programs with command labels, targets,
   models, exit status, duration, logs, accounting record if present, and output
   paths.
8. **Task Costs** - direct task cost and token totals when accounting was
   reported. The task cost is the sum of agent states spawned directly for that
   task; subtree accounting remains available through `rhei cost`. §FS-rhei-cost-accounting

The top of the report must expose reuse counts and blocked counts without
scrolling. A run that performs no spawned work because every required output
already existed must be visibly different from a run that spawned work quickly.

## 3. End-of-Run Console Summary

When `rhei run` exits, the last thing it prints is a compact summary block, then
the path to the durable report. This is the at-a-glance sibling of the Markdown
report: it shows the same Outcome Strip counts and the head of the Attention
list so an operator learns the result and the next action without opening a
file, then points at the report and dashboard for the full forensic read. The
file is the durable explanation; the console summary is the pointer to it.

On an interactive terminal the summary supersedes the flat `Run complete: … /
Final states: … / one line per task` dump. It obeys the console-first visual
language: monospace, dark, near-zero chrome, and saturated color reserved for
states that need attention, so the terminal, the TUI tiles, and the dashboard
name the same state the same way. §FS-rhei-viz-ux When stdout is not a TTY, the
existing line-oriented output is preserved verbatim so scripts and tests that
match it keep working (§3.4).

### 3.1. Layout

After the TUI restores the terminal - or after plain-stdout execution finishes -
the summary prints five stacked groups:

1. **Result line** - plan title, run id, duration, and overall result, using the
   same result vocabulary as the report header.

   A run the operator interrupted (§FS-rhei-run.3.2) reads as its own outcome
   and not as a halt: the result line is `interrupted — re-run to continue`,
   taking precedence over every other phrase, and the Attention row of a ticket
   whose worker was interrupted names the interruption as the blocker with
   `re-run to continue` as the next action. Neither surface tells the operator
   to inspect logs or to cancel the task: nothing about the ticket failed, the
   run was stopped, and re-running it is the whole recovery. The console prints
   no `Run complete:` line for such a run.
2. **Counts** - two dense lines: final task states, then run activity (agents,
   programs, reused-output, callback-only, terminal-at-start, could-not-advance).
   The states line is preceded by a static state-distribution bar whose segments
   are sized by count and colored by state hue; the labeled counts beside it are
   the authoritative, non-color readout. Reuse and blocked counts are always on
   screen, never below a fold. The bar is drawn once and never animates
   §FS-rhei-viz-ux.4.
3. **Attention** - up to five highest-priority halted tasks (gated first, then
   blocked, then halted), each with its state, the proven blocker, and the next
   action, ordered exactly as the report's Attention section. A trailing
   `… N more in the report` line appears when the list is truncated. The group is
   omitted entirely when no task is halted, rather than printing an empty heading.

   The blocker and next action come from a **plan-wide classification** of why
   each non-terminal task node is not moving, in this order: an open descendant
   subtree; a gating state awaiting a decision; a live `**Assignee:**`; an
   unsatisfied `**Prior:**`; a worker interrupted mid-flight by an *operator's*
   shutdown (§FS-rhei-run.3.2) — a run that tore its own workers down while
   failing describes their tickets by the failure instead, never by "re-run to
   continue"; a manual-only initial state; a worker that ran and
   left a required artifact unwritten — agent or program alike, since both are
   workers and both stall the same way; a ticket the run never scheduled; no
   declared outgoing transition; anything else. Interruption is classified
   ahead of the missing-artifact class because it explains it: a worker the run
   killed had no chance to write what it owed, and telling the operator to
   write those files by hand would be advice about a stall that did not
   happen. Non-leaf tasks are classified alongside leaves
   because a non-leaf task is a task in its own right
   (§FS-rhei-plan-language.3); a parent whose subtree is still open reads as
   waiting on that subtree, which is a structural consequence rather than
   something a human must fix, so — like a gate — it does not by itself make
   the run exit non-zero. The descendants named in it do that on their own
   account.

   The artifacts a missing-artifact row names are the ones the ticket's most
   recent worker left unwritten **in the state the ticket is still sitting
   in**. A ticket that was spawned again, or that moved on and later halted for
   an unrelated reason, is classified from scratch: an old stall never explains
   a new one.

   A parent held open **only** by its own subtree is therefore not halted work
   and takes **no Attention row**: it is counted in neither the `N gated ·
   M blocked` header, nor `could not advance` (§2), nor the Transition Ledger
   (§4). The open descendant is the halted work and is already counted there.
   Counting the parent as well multiplied one gated leaf by the height of the
   tree above it — three ancestors made four Attention rows, `4 gated`, and
   four blocked ledger entries for a single human decision, with the topmost
   parent's reason text repeating the whole transitive subtree. The parent
   stays visible in the task tree instead, with its own calm marker and its own
   `waiting on open descendant …` detail (§3.2). A parent that is itself
   gating, `blocked`, or `failed` keeps its Attention row: those are things to
   act on whatever its children are doing, even though the open subtree
   outranks them in the classification order above.

   A **gating** parent that still carries a `held` supervision block
   (§FS-rhei-supervision.3.1) is classified ahead of the open-subtree reading,
   because it is not waiting on its descendants — it is holding them. Its row
   says so and names them, and the next action is the human's: move it back into
   its supervising state to resume supervision, or anywhere else to release the
   subtree. The held descendants keep their own held rows, and those rows name
   the human rather than a supervisor visit that is not coming.

   The run's exit status and `--dry-run`'s are one judgment, not two readings
   of this classification: a run ends non-zero when work remains that is
   waiting on neither a human gate, a poll backoff, nor a `**Prior:**` that is
   itself waiting on one of those (§FS-rhei-run.4). Deriving the prediction
   from the per-ticket causes instead made `--dry-run` exit non-zero on plans
   the run itself exits zero on — a ticket whose prior chain ends in a gate is
   blocked by that gate, however its own line reads.
   Each names the command that clears it — `rhei release <id>` for a claim,
   `rhei next` then `rhei complete <id>` for manual-only work, finishing the
   prior for a dependency.

   A ticket the run actually spawned work for is a different kind of halt: its
   problem is the work, not the scheduling. When that work exited `0` and the
   run knows *which* required artifacts were missing — including the ticket's
   terminal result, reported under the artifact name `result` like any other
   (§FS-rhei-agents.3.2.1) — the row names them: `worker exited 0 without
   <name> (<path>), …`, and the next action is to write those files, or to
   record the outcome by hand with `rhei transition <id> --from … --to …
   --result …`. That is the whole difference between a report an operator can
   act on and one that says the task "stalled" and suggests reading logs that
   name nothing the operator did not already know. A ticket that ran and left
   nothing identifiable behind keeps the generic stalled reading; a ticket the
   run never scheduled says so, rather than borrowing it.

   The same classification feeds the live halt message and `--dry-run`
   (§FS-rhei-run.4), so the durable report and the terminal never disagree
   about one run. They did: the report described a claimed ticket, a
   prior-blocked ticket, and a manual-only ticket alike as `stalled in
   non-terminal state <state>` with `inspect logs or mark the task cancelled`,
   while stderr named the real cause precisely. That advice was wrong three
   ways — it proposed cancelling work that only needed its claim released or
   its prior finished, and it pointed at logs that a run spawning nothing never
   wrote. The report is what survives after the terminal scrolls away, so it is
   the copy that has to be right.
4. **Tasks** - the source-order task tree, every task on one aligned row, so the
   operator sees the whole plan's outcome, not only the halted tail. This mirrors
   the report's Task Final States section (§5). Tree and collapse rules are in
   §3.2.
5. **Pointers** - the latest report path, the timestamped history entry, and the
   dashboard artifact when one was written.

```text
Run Report  Rhei UI Canonical Test                       9f24c6 · 12m04s
  stopped for human attention

  States    █████████ ██ █████ █     9 completed · 2 human-gate · 5 blocked · 1 cancelled
  Work      7 agents · 15 programs · 0 reused · 0 callback-only · 2 terminal-at-start
  Cost      $1.23 · In 2.4M · In cached 1.5M · Out 180k · Out cached - · Coverage partial

Attention  3 gated · 4 blocked
  ! ui-test-canonical-example.full-pipeline              human-gate  counted fix loop finished
        → inspect runtime/fixes/ui-test-canonical-example.full-pipeline-visit-2.md and transition manually
  ! ui-test-canonical-example.poll-exhaustion            blocked     pollAttempts reached pollMaxAttempts
        → inspect runtime/logs/task-ui-test-canonical-example.poll-exhaustion-poll-exhaust.log
  … 5 more in the report

Tasks   17 tasks · source order
  ✓ ui-test-canonical-example.collect-inputs             completed    agent     3.4s
  ⏸ ui-test-canonical-example.full-pipeline              human-gate   counted fix loop finished
  │ ✓ ui-test-canonical-example.script-normalize         completed    program   0.2s
  │ ✓ ui-test-canonical-example.mock-implement           completed    agent     8.1s
  │ ✓ ui-test-canonical-example.parallel-review          completed    agent×2   5.0s
  │ ! ui-test-canonical-example.live-failure-blocked     blocked      program exited 42
  ✓ ui-test-canonical-example.snapshot-child             completed    agent     2.7s
  · ui-test-canonical-example.terminal-completed         completed    —         terminal at start
  ⏸ ui-test-canonical-example.human-gate                 human-gate   seeded gating state
  ! ui-test-canonical-example.poll-exhaustion            blocked      pollAttempts reached pollMaxAttempts
  ! ui-test-canonical-example.skill-unavailable-blocked  blocked      required skill absent-lens unavailable
  ⊘ ui-test-canonical-example.cancelled-task             cancelled    —
  … 3 completed tasks collapsed

Report     runtime/run-report.md
History    runtime/run-reports/2026-06-05T09-22-31Z-9f24c6.md
Dashboard  runtime/dashboard.html
```

Only the result line, the `!` attention rows, and each task's state column carry
saturated color, and the hue is the state color defined in §FS-rhei-viz-ux.
Every other line is plain chrome.

### 3.2. Task tree

The Tasks group lists every task in source order, preserving hierarchy with a
leading `│` gutter for child rows, so a reader recognizes the same shape the
report, TUI, and dashboard show. Each row is four aligned columns:

`<marker> <task-id>   <state>   <driver-or-detail>`

- **marker** - a fast scan glyph that degrades gracefully; color and the state
  label remain the primary signal §FS-rhei-viz-ux.3, so the marker is never the
  only cue and an ASCII fallback is used where the glyph is unavailable.

  | Marker | Fallback | Meaning |
  | --- | --- | --- |
  | `✓` | `+` | terminal-success state (`completed` or a custom terminal-success state) |
  | `⏸` | `=` | deliberately paused - a gating state awaiting a human (`human-gate`), or a non-leaf task held open only by its own subtree (§3.1) |
  | `!` | `!` | blocked or failed - needs attention |
  | `⊘` | `~` | `cancelled` |
  | `·` | `.` | terminal at the start of the run |

  A non-leaf task is halted for as long as any descendant is open, which is the
  eligibility rule working (§FS-rhei-plan-language.3), not a fault: one gated
  leaf otherwise turns every ancestor above it into its own attention row. Such
  a parent takes the paused marker, and the task tree is the only place it
  appears — it is counted in no Attention, header, or ledger tally (§3.1). Its
  detail column still reads `waiting on open descendant …`, so the tree shows
  the shape of the wait without inflating any count. A parent that is itself
  `blocked` or `failed` keeps the `!` marker: that is wrong independently of
  its children.

- **state** - the final state label, colored by its state hue.
- **driver-or-detail** - for advanced tasks, the driver and timing
  (`agent 3.4s`, `program 0.2s`, `agent×2 5.0s`, `reused`); for halted tasks, the
  same proven blocker shown in Attention; for terminal-at-start rows, `terminal
  at start`.

Only `!` rows take saturated attention color. `✓`, `·`, and `⊘` rows are calm
chrome so a healthy run reads as quiet.

To keep large plans scannable, a subtree whose every descendant is in a
terminal-success state collapses to a single trailing `… N completed tasks
collapsed` line. Every halted, gated, cancelled, or otherwise non-terminal task
is always shown, never collapsed, so nothing that needs a human disappears. The
report's Task Final States section (§5) is the un-collapsed source of truth.

### 3.3. Reused-output runs look different

The central UI requirement of this spec applies to the console too: a run that
spawned no work because every required output already existed must not look like
a fast successful run. When `agents == 0 && programs == 0` and at least one
transition was driven by `reused-output`, the result line reads `completed — no
work spawned` and a dedicated Reuse line names the cause:

```text
Run Report  Rhei UI Canonical Test                       a13f02 · 0.4s
  completed — no work spawned

  States    █████████████████████   22 completed
  Work      0 agents · 0 programs · 22 reused · 2 terminal-at-start
  Reuse     every required output already existed; no agent or program ran

Tasks   22 tasks · source order
  ✓ ui-test-canonical-example.collect-inputs             completed    reused
  ✓ ui-test-canonical-example.full-pipeline              completed    reused
  │ ✓ ui-test-canonical-example.script-normalize         completed    reused
  │ ✓ ui-test-canonical-example.mock-implement           completed    reused
  · ui-test-canonical-example.terminal-completed         completed    terminal at start
  … 17 completed tasks collapsed

Report     runtime/run-report.md
```

The `reused` detail on every row, not a duration, is what tells the operator no
subprocess ran. A reused run never collapses to a bare `✓` with a fast timing.

### 3.4. Plain / non-TTY mode

When stdout is not a TTY (piped, redirected, CI), the rich summary is suppressed
and `rhei run` keeps its existing line-oriented output verbatim, so scripts and
tests that match it keep working. §FS-rhei-run-tui.1.4 The frontend's TTY
detection decides this once, the same way the TUI/stdout frontend is chosen. The
preserved output keeps the greppable `Final states: <state>=<count>` prefix, the
`N/N tasks in terminal state` summary line, and one `- Task <id>: <title>
[<state>]` line per task:

```text
Run complete: 7 agent(s), 15 program(s) spawned, 9/24 tasks in terminal state.
Final states: blocked=5, cancelled=1, completed=9, human-gate=2
  - Task ui-test-canonical-example.full-pipeline: Full pipeline [human-gate]
  - Task ui-test-canonical-example.polling: Polling loop [completed]
  - Task ui-test-canonical-example.poll-exhaustion: Poll exhaustion [blocked]
  …
```

The durable Markdown report (§1, §4) now backs the non-TTY path: after the
preserved line-oriented output, `rhei run` prints a greppable `Report:
runtime/run-report.md` pointer to the report it just wrote.

### 3.5. Dry runs and empty runs

- Under `--dry-run`, the run is side-effect-free: it writes no report file and
  leaves `runtime/` untouched. The summary keeps the existing terminal phrase
  `Dry run complete - no agents were spawned.` and, on a TTY, shows the rich
  summary's counts for the simulated transitions (every output status
  `not-checked`). It prints no `Report:` pointer, since no report was written.
- When nothing advanced and nothing was reused, the result line reads `no tasks
  could be advanced` and the Attention group lists the blockers - the
  explanation the current bare `No tasks could be advanced.` line cannot give.

## 4. Transition Ledger

Each ledger row represents one task-state decision made by the scheduler. The
columns are:

| Column | Meaning |
| --- | --- |
| `task` | Task id, preserving hierarchy in source order. |
| `from` | State before the decision. |
| `to` | Selected destination state, or `-` when no transition was taken. |
| `driver` | `agent`, `program`, `callback-only`, `reused-output`, `dry-run`, `terminal-at-start`, or `blocked`. |
| `inputs` | Compact `name: ok/missing/optional-missing` list with resolved paths in the detail block. |
| `outputs` | Compact `name: created/reused/missing/not-checked` list with resolved paths in the detail block. |
| `invocation` | Agent/program label and log path, or `none`. |
| `reason` | The condition that selected the row: exit code, callback, output reuse, prior dependency, gate, poll delay, missing input, missing output, missing skill/MCP, terminal state, interruption, or run option. |

`driver: reused-output` means the current state's required outputs existed
before any subprocess was spawned for that decision, so Rhei was able to evaluate
the outgoing transition without running the autonomous state. This is the
high-signal label for artifact-reuse investigations. The row must still list
the checked outputs and mark them `reused`.

`driver: callback-only` means the transition was made through callbacks or
transition rules while autonomous spawning was disabled or not applicable. It is
not used for existing-output reuse, because that case needs its own visual
treatment.

An invocation the operator interrupted (§FS-rhei-run.3.2) reports `interrupted`
as both its exit status in **Invocations** and its ledger `reason`: the run
stopped the worker before it could finish, so the row records the state it ran
in and no transition out of it. It is deliberately distinct from `timed out`
and from a failing exit code — nothing about the ticket is known to be wrong,
and the marker used for it in live surfaces (`⏹`) is calm chrome rather than
attention color, for the same reason. The ticket's own row keeps whatever
marker its unchanged state earns.

`driver: blocked` is used for non-terminal tasks that remain in place at run end.
The row's `reason` must name the first concrete blocker Rhei can prove, such as
`waiting for prior ui-test-canonical-example.polling`, `gating state human-gate`, `missing input
runtime/build/ui-test-canonical-example.full-pipeline-report.md`, `poll next attempt at ...`, or
`required skill absent-lens unavailable`.

## 5. Artifact Evidence

The report records both sides of artifact checking:

- **Inputs checked before work** - every declared input for the state, the
  resolved path, whether it existed, and whether it was optional.
- **Outputs checked for transition** - every declared output for the source
  state, the resolved path, whether it existed before work, whether it existed
  after work, and whether it was required.

The UI uses four output statuses:

| Status | Meaning |
| --- | --- |
| `created` | Missing before invocation, present after invocation. |
| `reused` | Present before invocation or before reuse-based transition. |
| `missing` | Required after invocation or transition check and not present. |
| `not-checked` | No output check applied, such as terminal-at-start or dry-run. |

This makes accidental artifact reuse visible. `{task_id}` renders the
project-qualified ticket id, so two rheis sharing an execution root no longer
collide on paths like `runtime/milestones/{task_id}.md` (§AR-rhei-panta.5); the
remaining reuse case is an artifact left behind by an earlier run of the same
ticket. Such a run shows `driver: reused-output` and `output: milestone reused`
instead of implying that an agent completed the work.

**Implementation status.** The durable report (§1, §2) and the Transition Ledger
(§4) are emitted from the run event stream and the run-start state snapshot: every
spawned agent/program transition carries its real driver, exit, log path, and the
synthesized callback-only, terminal-at-start, and blocked rows complete the
picture. A report is written for every run except a `--dry-run` preview (§3.5),
including runs that abort with an error mid-execution — the latter via a
best-effort write of the data available up to the failure (§1). The per-artifact
Inputs/Outputs evidence columns (§5) and the `reused-output` driver are not yet
captured — distinguishing reuse from a callback advance needs a pre-run
output-existence snapshot from the engine, so no-spawn advances are reported as
`callback-only`. Until that lands, the report makes the artifact-reuse case
visible the coarse way: a run whose Outcome Strip shows `agent invocations: 0 ·
program invocations: 0` while tasks advanced is flagged in the report body as "no
agent or program ran," which is the signal the motivating issue asked for.

## 6. Canonical Rhei Example

The canonical fixture `examples/ui-test-canonical-example` should be the first
design target. It exercises agent states, program states, polling, failures,
gates, terminal-at-start rows, nested tasks, snapshot inheritance, generated
follow-up tasks, and fan-out review targets.

A completed canonical report starts like this:

```markdown
# Run Report: Rhei UI Canonical Test

Run: 2026-06-05T09:22:31Z / 9f24c6
Workspace: examples/ui-test-canonical-example
Command: rhei run . --parallel 4 --dashboard
Result: stopped for human attention
Latest dashboard: runtime/dashboard.html
Latest report: runtime/run-report.md

| Final states | Count |
| --- | ---: |
| completed | 9 |
| human-gate | 2 |
| blocked | 5 |
| cancelled | 1 |

| Activity | Count |
| --- | ---: |
| agent invocations | 7 |
| program invocations | 15 |
| callback-only transitions | 0 |
| reused-output transitions | 0 |
| terminal at start | 2 |
| could not advance | 7 |
```

The attention section should make the current stopping condition obvious:

```markdown
## Attention

| Task | State | Reason | Next action |
| --- | --- | --- | --- |
| ui-test-canonical-example.full-pipeline | human-gate | counted fix loop finished | inspect runtime/fixes/ui-test-canonical-example.full-pipeline-visit-2.md and transition manually |
| ui-test-canonical-example.human-gate | human-gate | seeded gating state | transition manually when reviewed |
| ui-test-canonical-example.poll-exhaustion | blocked | pollAttempts reached pollMaxAttempts | inspect runtime/logs/task-ui-test-canonical-example.poll-exhaustion-poll-exhaust.log |
| ui-test-canonical-example.live-failure-blocked | blocked | program exited 42 and matched blocked transition | inspect runtime/failures/ui-test-canonical-example.live-failure-blocked.md |
| ui-test-canonical-example.skill-unavailable-blocked | blocked | required skill absent-lens unavailable | install skill or mark task cancelled |
| ui-test-canonical-example.mcp-unavailable-blocked | blocked | required MCP server mock-mcp unavailable | start server or mark task cancelled |
| ui-test-canonical-example.blocked-seeded | blocked | seeded blocked state remained non-terminal | inspect task owner |
```

The transition ledger should make spawned work and artifact checks compact:

```markdown
## Transition Ledger

| Task | From | To | Driver | Inputs | Outputs | Invocation | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ui-test-canonical-example.full-pipeline | collect-inputs | script-normalize | agent | - | raw-inputs: created, raw-notes: created | mock-agent[yolo]:mock:ui-implementer / runtime/logs/task-ui-test-canonical-example.full-pipeline-collect-inputs-mock-agent-yolo-mock-ui-implementer.log | exit 0, outputs present |
| ui-test-canonical-example.full-pipeline | script-normalize | mock-implement | program | raw-inputs: ok, raw-notes: ok | normalized-inputs: created, io-map: created | bash ./bin/mock-program.sh normalize / runtime/logs/task-ui-test-canonical-example.full-pipeline-script-normalize.log | exit 0 |
| ui-test-canonical-example.full-pipeline | parallel-review | aggregate | agent | build-report: ok | review-findings: created x2 | 2 targets / runtime/logs/task-ui-test-canonical-example.full-pipeline-parallel-review-*.log | all target outputs present |
| ui-test-canonical-example.polling | script-poll | script-poll | program | - | poll-ready: missing | bash ./bin/mock-program.sh poll / runtime/logs/task-ui-test-canonical-example.polling-script-poll.log | exit 75, retry scheduled |
| ui-test-canonical-example.poll-exhaustion | poll-exhaust | blocked | program | - | poll-pending: created | bash ./bin/mock-program.sh poll-exhaust / runtime/logs/task-ui-test-canonical-example.poll-exhaustion-poll-exhaust.log | pollAttempts >= pollMaxAttempts |
| ui-test-canonical-example.terminal-completed | completed | - | terminal-at-start | - | not-checked | none | already terminal |
```

To model the motivating artifact-reuse case, the canonical fixture should
also have a report test variant with pre-created outputs. The ledger would show:

```markdown
| Task | From | To | Driver | Inputs | Outputs | Invocation | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ui-test-canonical-example.full-pipeline | collect-inputs | script-normalize | reused-output | - | raw-inputs: reused, raw-notes: reused | none | required outputs existed before run |
| ui-test-canonical-example.full-pipeline | script-normalize | mock-implement | reused-output | raw-inputs: ok, raw-notes: ok | normalized-inputs: reused, io-map: reused | none | required outputs existed before run |
```

This is the central UI requirement: no reader should confuse reused artifacts
with fast agent work.

## 7. Dashboard Affordance

The live and frozen Flow dashboard may link to the latest report, but the report
is the durable source for run explanation. The dashboard should add a small
`Report` action near the existing dashboard/run links:

- During a run, the action is disabled or points to a draft only if the draft is
  already durable.
- After the run finishes, it opens `runtime/run-report.md` through the
  same local open route used for artifacts and logs.
- In a frozen dashboard, it points to the relative report path if the report
  exists beside the dashboard artifact.

The dashboard should not duplicate the full Markdown report. It should surface
the same counts as a compact run summary and keep detailed forensic reading in
the report file.

## 8. Data Requirements

The execution engine must retain enough per-run facts to render the report:

- run id, command line, workspace root, start/end timestamps, duration, options,
  and final exit category;
- task ids, titles, hierarchy, state at run start, state at run end, and whether
  the task was terminal at start;
- every selected transition with source state, destination state, trigger
  reason, callbacks, and whether the transition was committed;
- every input/output artifact resolved for each decision, with pre/post
  existence status and optionality;
- every spawned agent/program, including resolved agent/target/model label,
  command label, exit status, duration, log path, accounting path, and output
  paths;
- every halted task with the most concrete blocker known to the scheduler.

These facts already overlap heavily with the run event surface and transition
journal, but the report should not be reconstructed only from
`runtime/transitions.log`. The journal is a tail-friendly event log; the report
is an end-of-run explanation with artifact and blocker context. §FS-rhei-run-tui

## 9. Non-Goals

- No interactive controls in the Markdown report.
- No remote report server.
- No hidden dependence on the browser dashboard being enabled.
- No content validation of artifacts beyond the existing file-existence
  contract.
- No attempt to summarize complete agent transcripts; the report links to logs.

## Related Specifications

- [Rhei Run](rhei-run.spec.md) - scheduler, transition, and completion behavior
- [Run TUI](rhei-run-tui.spec.md) - live event surface and transition journal
- [Flow Visualization](rhei-viz.spec.md) - browser dashboard that may link to the report
- [Agents](rhei-agents.spec.md) - completion authority and output contracts
- [Program States](rhei-programs.spec.md) - deterministic subprocess states
