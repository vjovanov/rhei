# FS-rhei-memory: Mid-Term Memory

A Rhei project is the **mid-term memory** of the work it governs: longer-lived
than any agent session, shorter-lived than the repository's specs and code. It
holds what was decided, what was produced, what failed, and what is still open
— in plan files, result files, exports, briefs, logs, and the transition
ledger. §GND-rhei-purpose §FS-rhei-usage.4

This document specifies how an invocation of a task **reads** that memory. Under
`rhei run` every invocation is cold: the agent knows nothing but its prompt. So
the prompt must *reconstitute* the memory — tell the agent where it stands,
what happened before it, what happened to it, and how to find anything the
prompt left out — and it must do so by a fixed algorithm, at a bounded cost in
tokens. §GOAL-rhei-outcomes

This spec extends the prompt of §FS-rhei-agents.3 with four sections. The
sections are graph- and runtime-level context, not configured in `states.yaml`,
and `rhei next` renders them for a manual worker exactly as `rhei run` does
(§5). It depends on:

- §FS-rhei-plan-language for the task hierarchy, plan formats, and content sections
- §FS-rhei-panta and §AR-rhei-panta.5 for rheis, execution roots, and qualified ids
- §FS-rhei-complete.3 for result files and the transition ledger
- §FS-rhei-agents.3 for the prompt this spec extends, and §FS-rhei-agents.8.1 for log paths
- §FS-rhei-supervision.5 for the sections a supervisor already gets

## 1. Requirements

### 1.1. Everything Before Is Reachable

From any invocation, the agent can determine — without guessing, and without
help from anything outside the prompt and the files it names — for **every
task in the Panta** that is terminal when the prompt is composed: its qualified
id, its title, its final state, and its result. Tasks in the invocation's own
rhei and every transitive prior are listed in the prompt itself (§3.2); every
other rhei is reachable through the map in §3.4, which names each rhei's
execution root, so no terminal task in the project is unreachable from any
other. The ledger of each execution root (§FS-rhei-complete.3.1) gives the
order in which they finished.

### 1.2. Composition Is Algorithmic

The prompt is a **pure function** of: the merged project graph, the `runtime/`
tree of every execution root, the resolved state machines, the resolved
settings, and the invocation identity (task, state, visit count, execution
identity). Composition performs no summarization, ranking, or selection beyond
the rules written in §4: a summary is a fixed slice of a file, an order is a
stated order, a cap is a stated number, and a truncation leaves a stated
overflow line. The same inputs produce the same bytes. Nothing that varies per
run — a run id, a timestamp, a pid — appears in the prompt; those travel in the
environment (§FS-rhei-agents.4).

### 1.3. Bounded and Progressive

Every section is omitted when it has nothing to say, so a one-task plan under
the built-in machine gains a few lines, not a page. Every section that can
grow has a cap, and every cap is paired with an overflow line naming the
command or file that holds the rest. Detail is pasted once: a result already
pasted in full elsewhere in the prompt is referred to, not repeated. The prompt
gives the agent the map and the nearest detail; the agent fetches the rest.

## 2. The Store

The memory is whatever the project already writes; this spec adds no store.

| Memory | Where | Written by |
|---|---|---|
| Intent, decomposition, progress notes | plan files: task bodies, `**Prior:**`, `**Consumes:**`/`**Provides:**` | plan writer, workers, supervisors |
| Plan-level context | content sections of the rhei (`index.rhei.md`, or the H2 sections before `## Tasks`) and of `index.panta.md` | plan writer |
| Outcome of a task | `runtime/results/<task-id>.md` (§FS-rhei-complete.3.2) | `rhei complete`, `rhei transition --result`, workers, the engine's failure routes |
| Order of events | `runtime/state-transitions.log` (§FS-rhei-complete.3.1) | every transition |
| Published outputs | `runtime/exports/<task-id>/<name>.md` (§FS-rhei-plan-language.3.12) | the producing task |
| Direction from above | `runtime/supervise/<task-id>[/<state>].md` (§FS-rhei-supervision.5.2) | supervisors |
| Same-task state handoffs | declared `outputs:` of an earlier state (§FS-rhei-states.3.2) | the earlier state |
| Raw transcripts | `runtime/logs/task-…log` (§FS-rhei-agents.8.1) | `rhei run` |

Each path is relative to the **execution root** of the rhei that owns the
task (§AR-rhei-panta.5): the workspace directory of a Directory Workspace rhei,
the project directory for single-file rheis.

## 3. The Sections

The four sections below join the prompt of §FS-rhei-agents.3 in this order:
`## Position` directly after `## State:` and the personality, before
`## Instructions`; `## Plan History` and `## Previous Visits` after
`## Supervisor Brief`; and two sub-sections inside `## Rhei Commands`.
Orientation comes before the instructions so the instructions are read with
the goal in mind; the broader memory comes after the task's own inputs because
the inputs are what the task acts on and the history is what it acts *within*.

### 3.1. `## Position`

Where this invocation sits in the project, top down.

```
## Position

Panta: {panta-title} › rhei `{rhei-id}`: {rhei-title} › Task {ancestor-id}: {title} [{state}] › …
› **Task {task_id}: {title} [{state}]** ← this invocation (visit {visit_count})

### Siblings

- Task {id}: {title} [{state}]
- Task {id}: {title} [{state}] — waits on this task

### Parent: Task {parent-id}: {title}

    ```markdown
    {the parent's body}
    ```

### Rhei Context

{content sections of the owning rhei, verbatim}

### Project Context

{content sections of index.panta.md, verbatim}
```

- The chain line names every ancestor, root first, each with its state. A
  root task's chain is the Panta and the rhei alone.
- `### Siblings` renders only when the task has a parent: the parent's other
  children, in plan order. A sibling that lists this task in `**Prior:**` or
  consumes one of its exports carries ` — waits on this task`. A root task has
  no sibling list; `## Plan History` and `### In Flight` cover the rest of the
  rhei.
- `### Parent` pastes the nearest ancestor's body in full, fenced. Higher
  ancestors contribute one line each to the chain and nothing more — a
  four-level tree does not paste four bodies. The parent's body is the memory
  that matters most to a leaf: it is where the decomposition was decided and
  where the acceptance for the whole subtree is written.
- `### Rhei Context` and `### Project Context` paste the content sections of
  the owning rhei and of the Panta manifest, verbatim and in authored order.
  These are the plan writer's standing notes, and until now only a worker that
  opened the file read them. A bare rhei with no Panta manifest has no
  `### Project Context`.

### 3.2. `## Plan History`

What finished before this invocation, one line per task.

```
## Plan History

Finished work, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root.

- Task {id}: {title} — {state} — {summary}
- Task {id}: {title} — {state} — see above
- Task {rhei}.{id}: {title} — {state} — {summary}   (rhei `{rhei}`, prior)
… {n} earlier tasks not shown — `rhei list --rhei {rhei-id} --terminal`

### In Flight

- Task {id}: {title} [{state}] — {assignee}

### Dependents

- Task {id}: {title} [{state}] — prior
- Task {id}: {title} [{state}] — consumes `{export}`
```

- The list covers every terminal task of the **owning rhei** and every
  **transitive prior** of this task in any rhei, including cancelled tasks —
  why something was not done is memory too. Tasks outside the owning rhei
  carry a trailing `(rhei `<id>`, prior)`.
- `{summary}` is derived by the rule in §4.3, never written by a model. A task
  whose result is already pasted in full in this prompt — under `## Prior Task
  Results`, `## Child Task Results`, or `## Checkpoints` — shows `see above`
  instead.
- `### In Flight` lists every non-terminal task in the Panta, other than this
  one, that carries an `**Assignee:**` or is spawned by the same `rhei run`
  pass: what other agents are touching right now. Omitted when there is none.
- `### Dependents` lists every task in the Panta that names this task in
  `**Prior:**` or names one of its exports in `**Consumes:**`, with the
  relation. This is who reads what this task writes; omitted when there is
  none.

### 3.3. `## Previous Visits`

What already happened to this task — rendered only when the task has at least
one ledger line or a result file, i.e. never on a task's first invocation.

```
## Previous Visits

Trail for this task: {from} → {to} → … → {state} (this visit, visit {visit_count}).

Result entries so far:

    ```markdown
    {runtime/results/<task-id>.md}
    ```

Previous log: `runtime/logs/{log file of the previous visit of this state}`
```

- The trail is this task's lines from the ledger, in order, with the current
  state appended.
- The result file is pasted because it is where every earlier verdict on this
  task landed: a worker's `--result` message, and the engine's own entries when
  an earlier visit timed out or exited without its required outputs
  (§FS-rhei-agents.3.2.1). An agent retrying a state that stalled must know
  why it stalled.
- `Previous log:` names the log file of the previous visit of this same state
  by the naming rule of §FS-rhei-agents.8.1, only if that file exists. The
  log is not pasted; it is a transcript, and the path is enough.

### 3.4. `## Rhei Commands` Additions

Two fixed sub-sections follow the existing authority text and transition list.

```
### Reading the rhei

- This rhei: `{execution root}` — plan `{plan or index path}`, this task's file `{task file path}`
- Every rhei in this project and its execution root:
  - `{rhei-id}` — `{execution root}`
- Under each execution root: `runtime/results/<task-id>.md` (results),
  `runtime/exports/<task-id>/<name>.md` (exports), `runtime/supervise/<task-id>[/<state>].md` (briefs),
  `runtime/state-transitions.log` (order of events), `runtime/logs/` (agent transcripts)
- Read-only commands, always safe: `rhei list [--rhei <id>] [--terminal] [--has-prior <id>] [--parent <id>]`,
  `rhei render <plan> --format json --pretty`

### Leaving a trail

What you write is what the next agent and the human see.
- `runtime/results/<task-id>.md`: the first line is the one-line summary every later Plan History shows; detail below it.
- You may append progress paragraphs to your own task body — files touched, commands run, decisions made — and append child tasks under your own task. Do not edit `**State:**` lines or any other task's body.
```

`Reading the rhei` is the map that makes §1.1 true across rheis: it names
every execution root in the project, so the results of a rhei the prompt does
not list are one path away. Paths are rendered relative to `RHEI_ROOT`, or
absolute when `RHEI_CHECKOUT_ROOT` differs from it, by the same rule
`{output.<name>.path}` follows (§FS-rhei-states.4). `Leaving a trail`
describes artifacts and permitted edits; it says nothing about when to stop or
how completion is detected, which stay with the completion condition
(§FS-rhei-agents.3.1).

## 4. Composition Algorithm

Given an invocation `I = (task, state, visit_count, identity)`:

### 4.1. Inputs

1. `G` — the merged project graph in plan order (§FS-rhei-plan-language.1.2);
   for a bare rhei, its implicit Panta (§FS-rhei-panta).
2. For every rhei `R` in `G`: its execution root `root(R)` and, under it, the
   ledger `L(R)` and the `runtime/` tree.
3. The owning rhei `R₀ = rhei(task)`.

### 4.2. Position

1. `chain` = ancestors of `task` from the root down; render each as
   `Task <id>: <title> [<state>]`, joined by ` › `, prefixed by the Panta
   title and `rhei <R₀>: <title>`.
2. If `task` has a parent `P`: `siblings` = children of `P` in plan order
   minus `task`; cap 30, overflow line
   `… {n} more — rhei list --parent <P>`. Mark a sibling `— waits on this task`
   when `task ∈ Prior(sibling)` or `Consumes(sibling)` names an export of
   `task`. Paste `body(P)`, fenced (§4.5); cap 200 lines, overflow line
   `… truncated; read <task file path of P>`.
3. `### Rhei Context` = the content sections of `R₀`'s index (or the H2
   sections before `## Tasks` of its single-file plan), verbatim, in authored
   order; `### Project Context` = the content sections of `index.panta.md`.
   Each capped at 1000 lines with the overflow line
   `… truncated; read <path>`. Omit either when empty.

### 4.3. Plan History

1. `own` = terminal tasks of `R₀` other than `task` and its descendants
   already rendered under `## Child Task Results` or `## Checkpoints`.
   Order: by the position in `L(R₀)` of each task's last line entering a
   terminal state; tasks with no ledger line (imported plans) come first, in
   plan order.
2. `priors` = the transitive closure of `Prior(task)` minus `own`, in plan
   order, each tagged `(rhei <R>, prior)`.
3. `summary(T)` = the first non-blank line of the **last** `## Result` entry
   of `runtime/results/<T>.md` under `root(rhei(T))`, excluding the heading
   line, cut to 120 columns with a trailing `…`; `(no result)` when the file
   is missing or empty. If `T`'s result is pasted in full elsewhere in this
   prompt, `see above` replaces the summary.
4. Cap: 40 lines. Entries in `priors` are never dropped; entries in `own` are
   dropped **oldest first** until the cap holds, and the overflow line
   `… {n} earlier tasks not shown — rhei list --rhei <R₀> --terminal` is
   emitted once, first.
5. `### In Flight` = non-terminal tasks of `G` other than `task` with an
   `**Assignee:**`, or spawned by the current `rhei run` pass; plan order;
   cap 20, overflow `… {n} more — rhei list --non-terminal`.
6. `### Dependents` = tasks of `G` with `task ∈ Prior(·)` or consuming an
   export of `task`; plan order; each with `— prior` and/or
   `— consumes <export>`; cap 30, overflow
   `… {n} more — rhei list --has-prior <task>`.
7. Omit `## Plan History` entirely when `own ∪ priors` is empty and neither
   sub-section renders.

### 4.4. Previous Visits

1. `trail` = lines of `L(R₀)` for `task`, in order. Render when `trail` is
   non-empty or `runtime/results/<task>.md` exists; otherwise omit the section.
2. Paste `runtime/results/<task>.md`, fenced; cap 100 lines, keeping the
   **last** 100 with the overflow line `… earlier entries omitted; read <path>`
   first.
3. `prev_log` = the path of §FS-rhei-agents.8.1 for `(task, state, identity,
   visit_count − 1)`; emit the `Previous log:` line only when that file exists.

### 4.5. Fencing and Rendering

1. Every pasted body is fenced with a backtick run one longer than the longest
   run it contains, so a pasted `## Result` never becomes a prompt heading
   (§FS-rhei-supervision.5.1). `## Prior Task Results`, `## Consumed Exports`,
   and the handoff sections adopt the same fence.
2. Ids are **qualified** everywhere (§FS-rhei-panta.6.3) — the form
   `rhei list` prints and `rhei transition` accepts.
3. Truncation is by whole lines, never mid-line, and the overflow line is a
   literal of this spec so tests can match it.
4. Caps are the numbers in this section. A settings surface may later override
   a cap with another number; it cannot introduce selection by relevance, and
   a cap of `0` removes the section rather than the overflow line.

## 5. Surfaces

- `rhei run` composes the sections for every spawned agent, in every mode.
- `rhei next` text output renders the same sections in the same order after
  the instructions, as it does for the supervision sections today
  (§FS-rhei-supervision.3.4); JSON output carries each as a string field named
  after the section: `position`, `plan_history`, `previous_visits`,
  `navigation`.
- `rhei run --dry-run` keeps its `<prompt...>` placeholder (§FS-rhei-agents.9).
  There is no command yet that prints the full composed prompt without
  claiming a ticket; claim-mode `rhei next` output (§FS-rhei-next.3.2) is the
  way to see it today, and a dry-run form is tracked on the roadmap.

## 6. Example

In the supervised-delivery example, the second invocation of
`deliver.fix-1` in state `fix`, after round-1 reviews finished and the first
`fix` visit timed out:

```
## Position

Panta: Rhei › rhei `delivery`: Supervised delivery › Task delivery.deliver: Deliver subtree supervision [supervising]
› **Task delivery.deliver.fix-1: Fix round 1 [fix]** ← this invocation (visit 2)

### Siblings

- Task delivery.deliver.implement: Implement docs/functional-spec/rhei-supervision.spec.md [completed]
- Task delivery.deliver.review-1: Code review round 1 [completed]
- Task delivery.deliver.pm-1: Product review round 1 [completed]
- Task delivery.deliver.review-2: Code review round 2 [review] — waits on this task
…

## Plan History

Finished work, oldest first. Full text: `runtime/results/<id>.md` under the owning rhei's execution root.

- Task delivery.deliver.implement: Implement … — completed — Landed execute_on, hold/release, checkpoints; 41 tests
- Task delivery.deliver.review-1: Code review round 1 — completed — see above
- Task delivery.deliver.pm-1: Product review round 1 — completed — see above

### Dependents

- Task delivery.deliver.review-2: Code review round 2 [review] — prior, consumes `resolutions`
- Task delivery.deliver.pm-2: Product review round 2 [pm-review] — prior, consumes `resolutions`

## Previous Visits

Trail for this task: pending → fix → fix (this visit, visit 2).

Result entries so far:

    ```markdown
    ## Result

    agent timed out in state 'fix' after 30m
    ```

Previous log: `runtime/logs/task-delivery.deliver.fix-1-fix-1.log`
```

The two reviews are `see above` because they are this task's direct priors and
already pasted in full under `## Prior Task Results`; the agent pays for each
result once.
