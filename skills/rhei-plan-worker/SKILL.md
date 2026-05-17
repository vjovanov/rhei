---
name: rhei-plan-worker
description: Execute tasks in a Rhei Plan markdown document without an orchestrator. Takes one required argument `<plan>` — the path to a `.rhei.md` file or a Directory Workspace `index.rhei.md` (e.g. `rhei-plan-worker docs/my-plan.rhei.md`). Use when the user asks to work, implement, advance, drive, or make progress on a specific `.rhei.md` plan — the worker self-selects the next eligible task, works in its current state, logs subtask progress, advances state, finalizes with a result, and respects human-review gates.
argument-hint: <plan>
---

# Rhei Plan Worker (Unorchestrated)

Pick up a Rhei Plan and make progress on it without any external scheduler. The worker is driven by the plan itself: the state machine defines what is legal, `**Prior:**` edges define what is ready, and each state's `instructions` field defines what to do.

Do not repurpose this skill for plan authoring — use `rhei-plan-writer` for that. Structural edits to the plan are limited to logging subtask progress and updating child task states. All root-task state changes and assignments go through the CLI.

## Parameters

The skill takes a single required parameter:

- `<plan>` — path to the `.rhei.md` file for a Single-File Plan, or the `index.rhei.md` at the root of a Directory Workspace.

Invoke as `rhei-plan-worker <plan>` (e.g., `rhei-plan-worker examples/ci-heal/index.rhei.md`). If the caller does not supply `<plan>`, ask for it and stop — do not guess, do not scan the working directory, do not pick from multiple candidates. The plan path is the worker's only source of truth for which plan to drive.

All other inputs (state machine, task selection, transitions) are derived from the plan file and the state machine it declares.

## Operating Loop

Execute this loop until no eligible task remains or a human gate stops you:

1. **Validate the plan.** Run `rhei validate <plan>`. If it fails, stop and report — do not work a broken plan.
2. **Load the state machine.** Run `rhei states` against the plan (or `rhei states --state-machine <path>`) to read allowed states, their `instructions`, and the declared transition graph. The CLI resolves the state machine from the plan's `**States:**` field (or a sibling/workspace `states.yaml`). Add `--json` for structured data. Fall back to reading the YAML directly, or [default-states.md](../rhei-plan-writer/references/default-states.md), if the CLI is unavailable.
3. **Read the plan.** Prefer `rhei render <plan> --format json --pretty` for structured access. Read the raw markdown too — you will edit it in place to log subtask progress.
4. **Claim the next task.** Run `rhei next <plan>`. The command atomically selects the next claimable task (all priors terminal, no current `**Assignee:**`, state not terminal and not gating, required `inputs` present), writes `**Assignee:**` on it, and prints the task id, current state, and resolved instructions. If no task is claimable, stop — the plan is either done or blocked.
    - Use `rhei next <plan> --peek` first when you want a read-only look at what would be claimed.
    - If `rhei next` fails with a missing-artifact error, the current state requires an input file that does not exist — surface it; do not try to skip ahead.
5. **Work in the current state.** Follow the printed instructions verbatim. The state you are handed is where the work happens — `rhei next` does **not** advance state. Implement child task nodes in order, logging per child (see *Progress Logging*).
6. **Advance state only when the workflow demands it.** Use `rhei transition` for intermediate hops (for example `draft` → `pending`, `agent-review` → `agent-review-fix`). For terminal completion, use `rhei complete` — do not hand-craft a transition into a final state.
7. **Finalize with `rhei complete`.** When the task is finished, run `rhei complete <plan> --task <id> --result "<one-line summary>"`. The command transitions to the first reachable non-cancelled terminal, appends a `## <from> → <to>` entry plus the message to `runtime/results/<task-id>.md`, links that file via `> **Result:**`, and removes the `**Assignee:**` line.
8. **Stop at terminal or gating states.** `completed` and `cancelled` are final. Any state with `gating: true` (typically `human-review`) halts the worker — do not transition out of it autonomously, and do not try to `rhei complete` through it.
9. **Loop.** Return to step 4 and claim the next task. Re-read the plan on every pass; the markdown file is the single source of truth.

Never skip validation. A failed `rhei validate` after a direct edit means the edit is wrong — fix it before moving on.

## Task Selection

Selection is owned by `rhei next` — do not re-implement it in prose. A task is claimable when:

1. Every task referenced in its `**Prior:**` list is in a terminal state (`final: true`; in the default machine that means `completed` or `cancelled`).
2. The task has no `**Assignee:**` field.
3. The current state is neither terminal nor gating.
4. Every required `inputs` artifact declared on the current state exists.

Validation rejects plans where a child task lists its parent or another
ancestor as `**Prior:**`. If you see that failure, do not try to work around it
by manually claiming the child; ask for or make a structural fix so the
follow-up task is a sibling when it must wait for the parent.

When multiple tasks are claimable, `rhei next` picks the first one in plan order. Do not try to pre-rank tasks by descendant count or other heuristics — trust the CLI.

A resumable task (already carrying your own `**Assignee:**` from a prior session that was interrupted) is not re-claimable via `rhei next`. Resume it directly: read the current state, follow its instructions, and advance with `rhei transition` / `rhei complete` as usual.

## State Transitions

All root-task state transitions go through the CLI. Never edit the root `**State:**` line by hand.

```bash
rhei transition <plan> --task <id> --from <current-state> --to <target-state>
```

The CLI provides:

- **File locking** — prevents concurrent writes from corrupting the plan.
- **Compare-and-swap** — `--from` guards against racing workers. If another agent already transitioned the task, the command fails and prints the actual state.
- **Transition validation** — illegal edges are rejected before any write.
- **Artifact enforcement** — required `outputs:` on the source state must exist before the transition succeeds.
- **Callbacks** — `on_leave` / `on_enter` callbacks fire on the edge unless `--no-callbacks` is passed.
- **Result file trail** — each transition appends a `## <from> → <to>` entry to `runtime/results/<task-id>.md`.

On conflict, re-read the plan and re-claim with `rhei next`. Do not retry the same transition blindly.

### Typical transitions for the default `rhei` machine

- `draft` → `pending` — description is finalized and the task is ready to be implemented.
- `pending` → `agent-review` — implementation is complete and self-tested; route to review.
- `agent-review` → `agent-review-fix` — review found issues; record findings first (see *Agent Review*).
- `agent-review` → `human-review` — review passed but a human gate is required before completion.
- `agent-review-fix` → `agent-review` — fixes applied, re-submit for review.
- `human-review` → ... — worker does **not** perform these; a human does.

For terminal completion (`pending` → `completed`, `agent-review` → `completed`, etc.) use `rhei complete`, not `rhei transition`. `rhei complete` picks the first reachable non-cancelled terminal, writes the result file, and unassigns in one atomic step.

If the loaded machine differs from the default, trust it over this list.

## Assignee Discipline

`**Assignee:**` is owned by the CLI. Never edit it by hand:

- `rhei next` writes `**Assignee:**` when claiming.
- `rhei complete` removes it when finalizing.
- `rhei transition` leaves it untouched. A long-running task therefore keeps the same assignee across intermediate transitions (for example `pending` → `agent-review` → `agent-review-fix`).

## Progress Logging

Log implementation progress by appending to each task node's body — do not invent new metadata fields.

- One short paragraph per leaf task node, written as you complete it (not in a batch at the end).
- State the concrete change: files touched, functions added, commands run to verify.
- Do not restate the task title; extend the description.
- For tasks without child nodes, append a single paragraph to the task description.

When a task re-enters an earlier state (for example `agent-review` → `agent-review-fix` → `agent-review`), append a new paragraph describing the rework rather than rewriting history. If the machine uses counted visits, the re-rendered `**State:** <name>-<n>` line makes the visit explicit — do not edit that suffix by hand.

## Agent Review

When a task enters `agent-review`, the reviewer is a *different* mental mode, not a different person. The reviewer:

- Reads the task description, its child task nodes, and the diff actually produced.
- Checks repository conventions (lint, format, test commands listed in [AGENTS.md](../../AGENTS.md) or the project's equivalent).
- Records concrete findings as a new paragraph in the task body, prefixed with `Review:` — one bullet per finding.
- Chooses the next edge: `agent-review-fix` (rework), `human-review` (needs human), or straight to terminal via `rhei complete` (pass).

Never finalize a task whose tests or build fail.

## Editing Discipline

State transitions and progress logging are separate concerns:

- **Root task state transitions** — always use `rhei transition` or `rhei complete`. Never edit a root task `**State:**` line in the markdown directly.
- **Assignee** — never edit `**Assignee:**` by hand; let `rhei next` and `rhei complete` manage it.
- **Result block** — the `> **Result:**` line is written by `rhei complete`. Do not author it by hand.
- **Child task state transitions** — child task nodes carry their own mandatory `**State:**` field. For child-only flows in the default machine you may update child states directly in the markdown as you finish each one. After every direct edit, run `rhei validate <plan>` to confirm the file is still well-formed. If the active machine's `node_policy` routes children through a stateful profile that uses the CLI, prefer `rhei transition` / `rhei complete` for children too.
- **Progress logging** — edit the markdown directly to append per-child progress (see *Progress Logging*). Re-run `rhei validate <plan>` after any direct edit.

General rules:

- Preserve IDs, titles, `**Prior:**` edges, and frontmatter. Structural changes belong to the plan writer.
- Do not reorder tasks. Do not delete completed or cancelled tasks. Do not remove `> **Result:**` links.
- Do not reformat unrelated sections.

## Stopping Conditions

Stop the loop and report to the user when any of these is true:

- `rhei next` prints no claimable task (everything is in a terminal state, blocked on a gating state, still in `draft` with unmet analysis, or awaiting priors).
- A task reaches a gating state (typically `human-review`) — stop *that* task but keep working on independent branches of the DAG by re-running `rhei next`.
- `rhei validate` fails after an edit you cannot explain — stop and show the user.
- A CAS conflict on `rhei transition` or `rhei complete` cannot be resolved by re-claiming (for example another worker is actively driving the same plan and there is no other eligible work) — stop and report.
- The task requires information or access the worker does not have (credentials, external decisions, missing input artifact) — stop and ask.

When stopping, print a short summary: which tasks advanced, which task is blocked and why, and what the next human action is.

## Unorchestrated Mode Notes

"Unorchestrated" means **no** external process tells the worker what to do next. Consequences:

- The worker reads the plan on every pass; treat the markdown file(s) as the single source of truth.
- Do not cache state across passes beyond the current conversation.
- Multiple workers can safely operate on the same plan. `rhei next` acquires a file lock and `rhei transition` / `rhei complete` use compare-and-swap semantics — the loser of a race re-reads and re-claims.
- Do not batch multiple task transitions into one command — one task, one transition, then re-read.
- Directory Workspaces behave the same, except the lock scope is per-task-file rather than per-plan, which is why the format exists.

This worker flow is distinct from `rhei run` (agent mode). Under `rhei run`, the orchestrator spawns a subprocess for each claimed state and performs the transition after the subprocess exits; the worker skill does the work manually instead.

## Missing Information Handling

If the plan path, state machine, or a task description is ambiguous or missing required detail:

- Ask the user before editing.
- Never invent prerequisites, states, or transitions to unblock selection.
- If a task description is too thin to implement, surface it and ask for clarification — do not silently expand scope. (In the default machine, thin descriptions normally mean the task is still in `draft` and needs analysis before it reaches `pending`.)
