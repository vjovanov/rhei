---
name: rhei-state-machine-writer
description: Design and generate custom Rhei state machine YAML files from project specifications and team structures. Use when users need a workflow that doesn't fit the default rhei state machine — domain-specific phases, multi-team handoffs, approval gates, counted review loops, model-specific states, or automated callback integrations.
---

# Rhei State Machine Writer

Design a custom Rhei state machine only when the built-in `rhei` workflow is too generic. Produce a YAML file matching the project's real phases, actors, gates, retries, and automation hooks, ready to be referenced by a plan's `**States:**` declaration. The state machine writer runs before the plan writer: it defines the workflow the plan later uses.

## When To Use This Skill

Use this skill when the project needs any of:

- Domain-specific phases (`ingesting`, `qa`, `staging`, `release-ready`)
- Human approval gates (`security-review`, `legal-review`, `manager-approval`)
- Multi-team handoffs with explicit transitions between owners
- Counted retry/review loops using `visits`
- Model-specific or multi-model execution using `models` / `model` / `all_models`
- State-level artifact contracts (`inputs:` / `outputs:`) enforced by the runtime
- Transition callbacks or conditional routing
- Distinct flows per node kind (e.g. `task` goes through review but `bug` skips it)

Do not use it when the default `rhei` machine is enough: a simple pending → completed flow with no specialized phases, gates, or branching.

## Required Inputs

Gather both before designing. If either is missing, ask and stop.

### 1. Project Specification

Extract: distinct phases; their ordering and branching; quality gates and checkpoints; failure modes and recovery paths; per-state artifact contracts (files required before entering or after leaving a state); places where callbacks/automation should run. Sources: requirements docs, READMEs, `AGENTS.md`, existing plans, or the user's description.

### 2. Team / Actor Structure

Map actors to behavior: who must approve work; who performs it; handoff points between owners; states that must not advance autonomously; different flows for different node kinds (e.g. `bug` bypasses design); model-specific execution if different models own different states. Do not invent approval authorities or team structure.

## Output Contract

Emit one YAML file conforming to the current Rhei state-machine format. Sections: `name`/`version`, optional `models`, `states`, `transitions`, `profiles`, `node_policy`, and optional `callbacks` / `error_handling`.

When invoked from `rhei-template-writer`, emit template-ready YAML for `<template>/states.yaml`: the full machine, with any needed `{{...}}` instantiation variables preserved and the template-required ASCII diagram comment block before `name:`. Do not return only a transition sketch or prose description.

### Root fields

Required: `name` (meaningful project-derived identifier), `version`, `states`, `transitions`, `profiles`, `node_policy`.

Optional: `models`; `personality` (machine-level role-framing default); `callbacks` (platform-specific mappings — `cli`, `nodejs`, ...); `error_handling`.

### State fields

Common: `description`; `instructions` (strongly recommended for every non-terminal state); `final: true` on terminal states.

> `initial: true` is **not** a state field. Initial states are declared per profile under `profiles`; `node_policy` routes each node to a profile.

Optional workflow controls:

- `gating: true` — state requires explicit human action to exit
- `personality` — state-specific role framing (overrides the machine-level default)
- `visits: <integer>` — counted loop re-entry limit (minimum `1`)
- `all_models: [<model>, ...]` — run the state once per listed declared model
- `model: <model>` — bind the state to one declared model (never set both `all_models` and `model`)
- `inputs:` / `outputs:` — artifact contracts (see *Artifact Contracts*)
- `agent:` — override the agent id `rhei next` assigns in this state

### Transition fields

Required: `from` (source state name, or `"*"` wildcard — typically a global cancellation edge); `to`; `description`.

Optional: `on_leave` (callback on the source before state change); `on_enter` (callback on the target after); `condition` (expression `rhei run` uses to choose among outgoing edges); `exit_code` (routes by subprocess exit code under `rhei run`); `timeout`.

Callback style: `on_leave: "cli:bash ./workflow.sh handoff-review"`.

### Profiles

Every machine declares at least one profile. A profile is the *policy* applied to a node: where it starts and which states it may ever hold.

```yaml
profiles:
  default:
    initial: <state-name>
    allowed: [<state-name>, <state-name>, ...]
```

Rules:

- Every profile declares `initial` and a non-empty `allowed`, and `initial` is a member of `allowed`.
- `allowed` includes at least one `final: true` state.
- **Reachability:** for every non-final state in `allowed`, a path to a final state in `allowed` exists using only transitions whose `to` is also in `allowed`; and every non-initial state in `allowed` is reachable from `initial` the same way. This is the core constraint the Transitions rules below build on.

### Node policy

```yaml
node_policy:
  root: <profile-name>       # required — the plan root (always kind `rhei`)
  default: <profile-name>    # required — fallback for kinds not matched below
  by_type:                   # optional — per-kind overrides
    <kind>: <profile-name>
  overrides:                 # optional — ordered, first-match-wins
    - match: { type: <kind>, level: <n> }
      profile: <profile-name>
```

Rules: `root` and `default` are required and must name defined profiles. Every `by_type` key must be a declared non-root node kind; `rhei` is reserved and must never be a `by_type` key. Resolution order for a non-root node is `overrides` → `by_type` → `default`.

## Schema Skeleton

```yaml
name: <project-derived-name>
version: 1.0
personality: |
  <optional machine-level role framing>
models: [<model-name>, ...]            # optional

states:
  <state-name>:
    description: <what this phase means>
    personality: |                     # optional, overrides machine default
      <state-specific framing — supports template variables>
    instructions: |
      <what the actor does here; template variables like {task_id},
      {task_title}, {visit_count}, {visits}, {model}, {input.<name>.path},
      {output.<name>.path}, {meta.<key>} are resolved by `rhei next`>
    final: true                        # terminal states only
    gating: true                       # human-exit states only
    visits: 2                          # counted-loop states only
    all_models: [<model-name>, ...]    # OR model: <name> — never both
    inputs:
      - { name: <artifact>, path: <root-relative path with {task_id}/{state}/{model}>, format: markdown }
    outputs:
      - { name: <artifact>, path: <root-relative path>, format: markdown }

transitions:
  - from: <source>
    to: <target>
    description: <when and why>
    on_leave: <callback>               # optional
    on_enter: <callback>               # optional
    condition: <expression>            # optional
    exit_code: <integer>               # optional
    timeout: <duration>                # optional

profiles:
  default: { initial: <state-name>, allowed: [<state-name>, ...] }

node_policy:
  root: default
  default: default
  by_type: { bug: <other-profile> }    # optional
```

When `models` is present, a state may omit model selectors, set `all_models`, or set `model` — never both selectors on one state.

### Artifact Contracts

`inputs:` and `outputs:` make files first-class transition prerequisites — they are execution semantics, not markdown syntax:

- `rhei next` refuses to claim a task if any `inputs:` file for the current state is missing, with an explicit missing-artifact error.
- `rhei transition` / `rhei complete` refuse to leave a state until every `outputs:` file is written.

Paths resolve relative to the plan's execution root (the directory containing the `.rhei.md` plan for a Single-File Plan; the directory containing `index.rhei.md` for a Directory Workspace). Template variables (`{task_id}`, `{state}`, `{model}`) resolve at runtime. Prefer artifact contracts over prose like "write your review to foo.md and transition" — the runtime enforces them structurally, and `instructions` can reference them via `{input.<name>.path}` / `{output.<name>.path}`.

## Design Rules

### States

1. One state per distinct workflow phase.
2. Name states after the phase, not the team (`security-review`, not `security-team`).
3. Never set `initial: true` on a state — initial lives on profiles.
4. Mark at least one state `final: true`. Include both a success terminal and a cancellation terminal unless the workflow clearly needs only one.
5. Mark human gates `gating: true` and say in `instructions` that the state must not transition out autonomously. `rhei next` refuses to claim gating tasks; `rhei complete` refuses to exit them.
6. Keep the machine proportional. Past roughly 15 states, consider splitting workflows.
7. Use `visits` only when the workflow intentionally loops through the same state and should eventually escalate or take another exit. Visit 1 renders as the unsuffixed name; later visits render as `<name>-<n>`. (Never combine `visits` with `all_models`/`all_targets` — see the Worked Pattern gotcha.)

### Transitions

1. Declare every legal transition explicitly. Unlisted transitions are forbidden — this is the core safety property.
2. Model only real workflow paths.
3. Multiple outgoing transitions are distinct outcomes; document each in `description`. Under `rhei run`, route among them with `condition` / `exit_code`, not `instructions` prose.
4. Honor the profile reachability constraint (see *Profiles*) when choosing edges: every allowed non-final state must keep a path to a terminal inside `allowed`.
5. Provide a cancellation path from every non-final state, usually `from: "*"` to a `cancelled` terminal.
6. No outgoing transitions from final states.
7. Avoid prose-only gates for machine-critical decisions. If a decision changes what the machine may do, encode it as a gating state, artifact contract, transition `condition`, `exit_code` route, or callback result the runtime can observe.

### Profiles and Node Policy

1. Start with a single default profile. Split only when kinds have genuinely different flows (e.g. `bug` skips design review).
2. Name profiles for the policy (`light-review`, `reviewed`, `simple`), not the kind they apply to.
3. Each profile's `allowed` is wholesale — profiles are referenced by name, never merged.
4. Use `overrides` only when `by_type` cannot express the rule (e.g. "leaf-level tasks skip review").

### Instructions

1. Write for the actor in that state, describing the domain work only — exit conditions are encoded structurally (`outputs:` artifacts, transition `condition` / `exit_code`), not in prose. Gating states are the exception: their instructions address a human reader and must explicitly say "do not transition out of this state autonomously," naming who decides and what decision.
2. Reference concrete artifacts and checks, not vague review language.
3. Use template variables (`{task_id}`, `{task_title}`, `{visit_count}`, `{visits}`, `{model}`) instead of prose placeholders; reference contracts via `{input.<name>.path}` / `{output.<name>.path}`. Unknown variables are left verbatim, so free-form braces in prose are safe.

### Automation, queues, and publish states

1. **Idempotency for recurring sweeps.** A state or callback that repeatedly scans for work must name the durable marker, artifact, external id, or de-duplication key that prevents reprocessing the same item next run.
2. **Queue drain condition.** Queue-processing flows need an explicit "no ready items remain" condition, terminal path, or parking gate, or `rhei run` finds the same work forever.
3. **Publish states re-check approvals.** A state that releases, publishes, merges, deploys, or notifies externally must consume the review/approval artifacts as `inputs:` and verify the latest result is still affirmative immediately before the side effect.
4. **Publish states need a negative path.** If the required artifact is missing, stale, rejected, or inconsistent, route to review/blocked/cancelled/failed instead of publishing.
5. **Keep side effects idempotent.** Publish and callback states must be safe to retry after a crash — use external ids, lock files, status artifacts, or read-before-write checks.

## Worked Pattern: Multi-Round Discussion / Deliberation

When the workflow needs several agents to **deliberate** — take each other's positions into account across rounds and then converge or escalate — rather than just hand a task state to state, start from the checked-in, `rhei validate`-passing reference `examples/agent-discussion/` (state machine `discussion-states.yaml`, driven by `workflow.sh` callbacks). Its shape:

- **A `collect ↔ judge` loop.** `collect` declares `all_models: [...]` so the position callback fans out once per participant; `judge` is a single-model synthesizer. The loop is driven by the **judge's callback `nextState` redirect**, not by `visits`: the judge returns `converged` (consensus — records the decision artifact), `escalated` (a `gating: true` human handoff once the round budget is spent), or no redirect to fall through the default `judge → collect` transition for another round. Declare `converged` and `escalated` as transitions from `judge` so the redirects are legal targets.
- **Participants take each other into account** because round 2+ reads the previous round's digest artifact, and **argue from distinct stances** assigned per participant (here, competing project goals).
- **The decision gates real work:** a downstream task declares `**Prior:**` on the discussion task, so it stays blocked until the discussion reaches a terminal decision (`converged`) — otherwise it's just chatter.

**Critical gotcha — never combine `all_models` (or `all_targets`) with `visits` on the same state.** The engine runs such a state per-target *per-visit* and spins on a `state → state-2` self-loop that never advances. Counted loops (`visits`) are for **single-target** states (see `examples/review-fix-visits`); fan-out loops must be bounded in the callback (a `CAP`) and driven by a redirect.

## Workflow

1. Read the project specification and actor structure.
2. Decide whether the default `rhei` machine suffices. If yes, do not author a custom machine.
3. List the real phases, gates, retries, handoffs, and per-state artifacts.
4. Draft state names, descriptions, and instructions.
5. Add `gating`, state `personality`, `visits`, model selectors, and `inputs` / `outputs` only where they solve a real need.
6. Draft transitions, including cancellation and recovery paths.
7. Draft profiles and `node_policy` — start with one default profile; add `by_type` / `overrides` only when kinds need different flows.
8. Add callbacks only where the workflow truly integrates with external automation.
9. Write the YAML to a conventional location (see *File Placement*). For a template, write to `<template>/states.yaml` and ensure the plan skeleton declares the same machine name via `**States:**`.
10. Validate with the CLI when available.

## Validation Checklist

Before returning the machine, verify:

- No state carries `initial: true`; at least one state is `final: true`; no final state has outgoing transitions.
- Every transition has `from`, `to`, `description`.
- Gating states are marked `gating: true` and their instructions forbid autonomous exit.
- Machine-critical decisions live in gating states / artifacts / conditions / exit-code routes / callback results, not prose.
- If `models` is present, every `model` / `all_models` entry is declared there; no state sets both `all_models` and `model`.
- `visits`, when present, is ≥ `1`, and never appears alongside `all_models` / `all_targets`.
- `inputs:` / `outputs:` paths are execution-root-relative and use template variables, not hard-coded task ids.
- Recurring sweeps define an idempotency marker; queue flows define a no-ready-items drain path.
- Publish/merge/deploy/release/notify states re-check approval artifacts via `inputs:` and have a non-publish path for missing/stale/negative results.
- State names are lowercase hyphenated `IDENTIFIER`s (names with spaces/punctuation are legal but must be backticked in markdown).
- `name` is a meaningful project-derived identifier.
- `profiles` present; every profile declares `initial` and a non-empty `allowed`; `initial` ∈ `allowed`; `allowed` contains ≥1 final; reachability holds (every allowed non-final reaches a final via `to`-in-`allowed` transitions).
- `node_policy` has `root` and `default`, both naming defined profiles; every `by_type` key is a declared non-root kind; `rhei` is never a `by_type` key.
- No orphan states (defined but unreferenced by any profile `allowed`, transition, or override).

When the CLI is available, validate with `rhei states --state-machine <path>` (add `--json` for machine-readable output). For template-authored machines still containing `{{...}}` placeholders, do not validate the raw `<template>/states.yaml` — instantiate first, then validate the rendered workspace's state machine.

## File Placement

- `docs/states.yaml` — single machine for the project, auto-discovered by a sibling or workspace-root plan.
- `docs/states/<name>.yaml` — multiple machines in the project.
- `.agents/rhei/states.yaml` or `.agents/rhei/states/<name>.yaml` — for projects keeping agent config under `.agents/`.

A plan picks up a sibling or workspace-root `states.yaml` automatically when it declares `**States:** <name>`; the YAML's `name` must match. Use `--state-machine <path>` to override the auto-discovered file.
