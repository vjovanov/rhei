---
name: rhei-state-machine-writer
description: Design and generate custom Rhei state machine YAML files from project specifications and team structures. Use when users need a workflow that doesn't fit the default rhei state machine — domain-specific phases, multi-team handoffs, approval gates, counted review loops, model-specific states, or automated callback integrations.
---

# Rhei State Machine Writer

Design a custom Rhei state machine only when the built-in `rhei` workflow is too generic. Produce a YAML file that matches the project's real phases, actors, gates, retries, and automation hooks, ready to be referenced by a plan's `**States:**` declaration.

The state machine writer runs before the plan writer. It defines the workflow; the plan writer later uses that workflow.

## When To Use This Skill

Use this skill when the project needs any of the following:

- Domain-specific phases such as `ingesting`, `qa`, `staging`, or `release-ready`
- Human approval gates such as `security-review`, `legal-review`, or `manager-approval`
- Multi-team handoffs with explicit transitions between owners
- Counted retry/review loops using `visits`
- Model-specific or multi-model execution using `models`, `model`, or `all_models`
- State-level artifact contracts (`inputs:` / `outputs:`) that the runtime should enforce
- Transition callbacks or conditional routing
- Distinct flows for different node kinds (for example, `task` goes through review but `bug` skips it)

Do not use this skill when the default `rhei` machine is enough:

- Standard agent workflow with draft → pending → review → completed
- No specialized phases, approval gates, or branching beyond the default flow

## Required Inputs

Gather both inputs before designing the machine. If either is missing, ask the user and stop.

### 1. Project Specification

Extract from the spec:

- Distinct workflow phases
- Phase ordering and branching
- Quality gates and checkpoints
- Failure modes and recovery paths
- Per-state artifact contracts (files that must exist before entering or after leaving a state)
- Places where callbacks or automation should run

Sources can include requirements docs, READMEs, `AGENTS.md`, existing plans, or the user's description.

### 2. Team / Actor Structure

Map the involved actors to workflow behavior:

- Teams or humans who must approve work
- Teams or agents who perform work
- Handoff points between owners
- States that must not advance autonomously
- Different flows for different node kinds (for example `bug` tasks bypass design)
- Model-specific execution, if different models own different states

Do not invent approval authorities or team structure.

## Output Contract

Emit one YAML file that conforms to the current Rhei state-machine format. The file has five sections: `name`/`version`, optional `models`, `states`, `transitions`, `profiles`, `node_policy`, and optional `callbacks` / `error_handling`.

When this skill is used from `rhei-template-writer`, emit template-ready YAML for `<template>/states.yaml`: include the full machine, preserve any needed `{{...}}` instantiation variables, and add the template-required ASCII diagram comment block before `name:`. Do not return only a transition sketch or prose description.

### Root fields

Required:

- `name` — a meaningful project-derived identifier
- `version`
- `states`
- `transitions`
- `profiles`
- `node_policy`

Optional:

- `models`
- `personality` — machine-level role framing default
- `callbacks` — platform-specific callback mappings (`cli`, `nodejs`, ...)
- `error_handling`

### State fields

Common:

- `description`
- `instructions` — strongly recommended for every non-terminal state
- `final: true` on terminal states

> `initial: true` is **not** a state-level field. Initial states are declared on each profile under the top-level `profiles` block; `node_policy` routes each node to a profile.

Optional workflow controls:

- `gating: true` for states that require explicit human action to exit
- `personality` for state-specific role framing (overrides the machine-level default)
- `visits: <integer>` for counted loop re-entry limits (minimum `1`)
- `all_models: [<model>, ...]` to run the state once per listed declared model
- `model: <model>` to bind the state to one declared model
- `inputs:` / `outputs:` — artifact contracts (see *Artifact Contracts*)
- `agent:` — overrides the agent id assigned by `rhei next` in this state

### Transition fields

Required:

- `from` — a source state name, or `"*"` for a wildcard (typically used for a global cancellation edge)
- `to`
- `description`

Optional:

- `on_leave` — callback invoked on the source state before state change
- `on_enter` — callback invoked on the target state after state change
- `condition` — expression used by `rhei run` to select among multiple outgoing edges
- `exit_code` — routes by subprocess exit code under `rhei run` orchestrator authority
- `timeout`

Example callback style:

```yaml
on_leave: "cli:bash ./workflow.sh handoff-review"
```

### Profiles

Every machine declares at least one profile. A profile defines the *policy* applied to a node: where it starts and which states it may ever hold.

```yaml
profiles:
  default:
    initial: <state-name>
    allowed:
      - <state-name>
      - <state-name>
      - ...
```

Rules:

- Every profile must declare `initial` and a non-empty `allowed` list.
- The `initial` state must be a member of `allowed`.
- `allowed` must include at least one state with `final: true`.
- For every non-final state in `allowed`, a path to a final state in `allowed` must exist using only transitions whose `to` is also in `allowed`.

### Node policy

```yaml
node_policy:
  root: <profile-name>       # required — the plan root (always kind `rhei`) uses this profile
  default: <profile-name>    # required — fallback for non-root kinds not in `by_type` or `overrides`
  by_type:                   # optional — per-kind overrides
    <kind>: <profile-name>
  overrides:                 # optional — ordered, first-match-wins
    - match: { type: <kind>, level: <n> }
      profile: <profile-name>
```

Rules:

- `root` and `default` are required and must name defined profiles.
- Every `by_type` key must be a declared non-root node kind. `rhei` is reserved and must never appear as a `by_type` key.
- `overrides` is evaluated in order; the first match wins. Resolution order for a non-root node is `overrides` → `by_type` → `default`.

## Current Schema Pattern

```yaml
name: <project-derived-name>
version: 1.0
personality: |
  <optional machine-level role framing>
models:
  - <model-name>
  - <model-name>

states:
  <state-name>:
    description: <what this phase means>
    personality: |
      <optional state-specific framing — supports template variables>
    instructions: |
      <what the actor does here>
      Template variables like {task_id}, {task_title}, {visit_count}, {visits},
      {model}, {input.<name>.path}, {output.<name>.path}, {meta.<key>}
      are resolved by `rhei next` before output.
    final: true
    gating: true
    visits: 2
    all_models: [<model-name>, ...]
    model: <model-name>
    inputs:
      - name: <artifact-name>
        path: <execution-root-relative path with {task_id}, {state}, {model}>
        format: <markdown|json|text>
    outputs:
      - name: <artifact-name>
        path: <execution-root-relative path with {task_id}, {state}, {model}>
        format: <markdown|json|text>

transitions:
  - from: <source>
    to: <target>
    description: <when and why this transition occurs>
    on_leave: <callback-name>
    on_enter: <callback-name>
    condition: <expression>
    exit_code: <integer>
    timeout: <duration>

profiles:
  default:
    initial: <state-name>
    allowed:
      - <state-name>
      - <state-name>

node_policy:
  root: default
  default: default
  by_type:
    bug: <other-profile>
```

When `models` is present, a state may either:

- omit model selectors entirely
- set `all_models: [<name>, ...]`
- set `model: <name>`

Never set both `all_models` and `model` on the same state.

### Artifact Contracts

`inputs:` and `outputs:` turn files into first-class transition prerequisites. They are part of execution semantics, not markdown syntax:

- `rhei next` will refuse to claim a task if any `inputs:` file for the current state is missing, and prints an explicit missing-artifact error.
- `rhei transition` and `rhei complete` will refuse to leave a state if any `outputs:` file has not been written.

Paths are resolved relative to the plan's execution root (the directory containing the `.rhei.md` plan for a Single-File Plan; the directory containing `index.rhei.md` for a Directory Workspace). Template variables (`{task_id}`, `{state}`, `{model}`) are resolved at runtime.

Use artifact contracts in preference to prose like "write your review to foo.md and transition" — the runtime enforces them structurally and `instructions` can reference them via `{input.<name>.path}` / `{output.<name>.path}`.

## Design Rules

### States

1. Create one state per distinct workflow phase.
2. Name states after the phase, not the team. Prefer `security-review` over `security-team`.
3. Do **not** set `initial: true` on any state. Initial states live on profiles.
4. Mark at least one state as `final: true`. In practice, include both a success terminal and a cancellation terminal unless the workflow clearly does not need both.
5. Mark human gates explicitly with `gating: true`, and also say in `instructions` that the state must not transition out autonomously. `rhei next` will refuse to claim tasks in gating states; `rhei complete` will refuse to exit them.
6. Keep the machine proportional to the workflow. If it grows past roughly 15 states, consider splitting workflows.
7. Use `visits` only when the workflow intentionally loops through the same state and should eventually escalate or take another exit. Visit 1 renders as the unsuffixed name; later visits render as `<name>-<n>` in markdown.

### Transitions

1. Declare every legal transition explicitly. Unlisted transitions are forbidden — this is the core safety property.
2. Model only real workflow paths.
3. Multiple outgoing transitions represent distinct outcomes; document each in `description`. Under `rhei run` orchestrator authority, use `condition` / `exit_code` to route among them rather than relying on `instructions` prose.
4. Every non-initial state in a profile's `allowed` must be reachable from that profile's `initial` using transitions whose `to` also lies in `allowed`.
5. Every non-terminal state in a profile's `allowed` must have a path to a terminal state inside `allowed`.
6. Provide a cancellation path from every non-final state, usually with `from: "*"` to a `cancelled` terminal.
7. Do not define outgoing transitions from final states.

### Profiles and Node Policy

1. Start with a single default profile. Only split when different kinds have genuinely different flows (for example, `bug` skips design review).
2. Name profiles for the policy (`light-review`, `reviewed`, `simple`), not the kind they apply to.
3. Each profile's `allowed` is wholesale — profiles are referenced by name, never merged.
4. `node_policy.root` and `node_policy.default` are both required and must name defined profiles.
5. Use `overrides` only when `by_type` cannot express the rule (e.g., "leaf-level tasks skip review"). `rhei` is reserved and cannot appear as a `by_type` key.

### Instructions

1. Write instructions for the actor in that state.
2. Describe the domain work only. Under `rhei run` orchestrator authority, exit conditions are encoded structurally — via `outputs:` artifacts and transition `condition` / `exit_code` — not in prose. Gating states are the exception: their instructions address a human reader and should explicitly say "do not transition out of this state autonomously."
3. Reference concrete artifacts and checks, not vague review language.
4. For gating states, say who decides and what decision they are making.
5. Use template variables (`{task_id}`, `{task_title}`, `{visit_count}`, `{visits}`, `{model}`) instead of prose placeholders like `<id>`. When a state declares artifact contracts, reference them via `{input.<name>.path}` and `{output.<name>.path}` instead of repeating raw paths. Unknown variables are left verbatim, so free-form braces in prose are safe.

## Workflow

1. Read the project specification and actor structure.
2. Decide whether the default `rhei` machine is sufficient. If yes, do not author a custom machine.
3. List the real phases, gates, retries, handoffs, and per-state artifacts.
4. Draft state names, descriptions, and instructions.
5. Add `gating`, state `personality`, `visits`, model selectors, and `inputs` / `outputs` only where they solve a real workflow need.
6. Draft transitions, including cancellation and recovery paths.
7. Draft profiles and `node_policy`. Start with one default profile. Add `by_type` / `overrides` only when different kinds need different flows.
8. Add callbacks only where the workflow truly integrates with external automation.
9. Write the YAML file to `docs/states.yaml` or `docs/states/<name>.yaml` (or the `.agents/rhei/` paths — see *File Placement*).
10. Validate the result with the CLI when available.

If authoring for a Rhei Template, write the YAML to `<template>/states.yaml` instead and ensure the template's plan skeleton declares the same machine name with `**States:** <name>`.

## Validation Checklist

Before returning the machine, verify:

- No state carries `initial: true` (initial is a profile field, not a state field).
- At least one state has `final: true`.
- No final state has outgoing transitions.
- Every transition has `from`, `to`, and `description`.
- Gating states are marked with `gating: true` and their instructions forbid autonomous exit.
- If `models` is present, every `model` and `all_models` entry is declared there.
- No state declares both `all_models` and `model`.
- `visits`, when present, is at least `1`.
- `inputs:` / `outputs:` paths are execution-root-relative and use template variables rather than hard-coded task ids.
- State names are lowercase hyphenated identifiers (matching the `IDENTIFIER` grammar production). States whose names contain spaces or punctuation are legal but must be referenced in markdown via backticks.
- The `name` field is a meaningful project-derived identifier.
- `profiles` is present and every profile declares `initial` and a non-empty `allowed`.
- Every profile's `initial` is in its `allowed` set.
- Every profile's `allowed` contains at least one final state.
- For every profile, every non-final state in `allowed` has a path to a final state in `allowed`, using only transitions whose `to` is also in `allowed`.
- `node_policy` is present with both `root` and `default`, both referencing defined profiles.
- Every `by_type` key is a declared non-root node kind. `rhei` does not appear as a `by_type` key.
- No orphan states (defined in `states` but not referenced by any profile's `allowed`, transition, or override).

If the `rhei` CLI is available, validate with:

```bash
rhei states --state-machine <path>
```

Use `--json` when you need machine-readable output.

## File Placement

Use one of these conventional locations:

- `docs/states.yaml` — single machine for the project, auto-discovered by a sibling or workspace-root plan.
- `docs/states/<name>.yaml` — multiple machines in the project.
- `.agents/rhei/states.yaml` / `.agents/rhei/states/<name>.yaml` — for projects that keep agent configuration under `.agents/`.

Plans normally pick up a sibling or workspace-root `states.yaml` automatically when they declare `**States:** <name>`. The YAML file's `name` must match the declaration. Use `--state-machine <path>` when overriding the conventional auto-discovered file.
