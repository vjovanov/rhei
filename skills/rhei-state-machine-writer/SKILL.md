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
- Transition callbacks or conditional routing

Do not use this skill when the default `rhei` machine is enough:

- Standard agent workflow with straightforward implementation and review
- No specialized phases, approval gates, or branching beyond the default flow

## Required Inputs

Gather both inputs before designing the machine. If either is missing, ask the user and stop.

### 1. Project Specification

Extract from the spec:

- Distinct workflow phases
- Phase ordering and branching
- Quality gates and checkpoints
- Failure modes and recovery paths
- Places where callbacks or automation should run

Sources can include requirements docs, READMEs, `AGENTS.md`, existing plans, or the user's description.

### 2. Team / Actor Structure

Map the involved actors to workflow behavior:

- Teams or humans who must approve work
- Teams or agents who perform work
- Handoff points between owners
- States that must not advance autonomously
- Model-specific execution, if different models own different states

Do not invent approval authorities or team structure.

## Output Contract

Emit one YAML file that conforms to the current Rhei state-machine format.

### Root fields

Required:

- `name`
- `version`
- `states`
- `transitions`

Optional:

- `models`
- `callbacks`
- `error_handling`

### State fields

Common:

- `description`
- `instructions` — strongly recommended for every non-terminal state
- `initial: true` on exactly one state
- `final: true` on terminal states

Optional workflow controls:

- `gating: true` for states that require explicit human action to exit
- `personality` for state-specific role framing
- `visits: <integer>` for counted loop re-entry limits
- `all_models: [<model>, ...]` to run the state once per listed declared model
- `model: <model>` to bind the state to one declared model

### Transition fields

Required:

- `from`
- `to`
- `description`

Optional:

- `on_leave`
- `on_enter`
- `condition`
- `timeout`

Example callback style:

```yaml
on_leave: "cli:bash ./workflow.sh handoff-review"
```

## Current Schema Pattern

```yaml
name: <project-derived-name>
version: 1.0
models:
  - <model-name>
  - <model-name>

states:
  <state-name>:
    description: <what this phase means>
    personality: |
      <optional state-specific framing — supports template variables>
    instructions: |
      <what the actor does here and when to transition out>
      Template variables like {task_id}, {task_title}, {visit_count}, {visits},
      {model}, {input.<name>.path}, {output.<name>.path}, {meta.<key>}
      are resolved by `rhei next` before output.
    initial: true
    final: true
    gating: true
    visits: 2
    all_models: [<model-name>, ...]
    model: <model-name>
    inputs:
      - name: <artifact-name>
        path: <workspace-relative path with {task_id}, {state}, {model}>
        format: <markdown|json|text>
    outputs:
      - name: <artifact-name>
        path: <workspace-relative path with {task_id}, {state}, {model}>
        format: <markdown|json|text>

transitions:
  - from: <source>
    to: <target>
    description: <when and why this transition occurs>
    on_leave: <callback-name>
    on_enter: <callback-name>
    condition: <expression>
    timeout: <duration>
```

When `models` is present, a state may either:

- omit model selectors entirely
- set `all_models: [<name>, ...]`
- set `model: <name>`

Never set both `all_models` and `model` on the same state.

## Design Rules

### States

1. Create one state per distinct workflow phase.
2. Name states after the phase, not the team. Prefer `security-review` over `security-team`.
3. Mark exactly one state as `initial: true`.
4. Mark at least one state as `final: true`. In practice, include both a success terminal and a cancellation terminal unless the workflow clearly does not need both.
5. Mark human gates explicitly with `gating: true`, and also say in `instructions` that the state must not transition out autonomously.
6. Keep the machine proportional to the workflow. If it grows past roughly 15 states, consider splitting workflows.
7. Use `visits` only when the workflow intentionally loops through the same state and should eventually escalate or take another exit.

### Transitions

1. Declare every legal transition explicitly. Unlisted transitions are forbidden.
2. Model only real workflow paths.
3. Multiple outgoing transitions represent distinct outcomes; document each clearly.
4. Every non-initial state must be reachable from the initial state.
5. Every non-terminal state must have a path to a terminal state.
6. Provide a cancellation path from every non-final state, usually with `from: "*"`.
7. Do not define outgoing transitions from final states.

### Instructions

1. Write instructions for the actor in that state.
2. State the exit condition for every non-terminal state.
3. Reference concrete artifacts and checks, not vague review language.
4. For gating states, say who decides and what decision they are making.
5. Use template variables (`{task_id}`, `{task_title}`, `{visit_count}`, `{visits}`, `{model}`) instead of prose placeholders like `<id>`. When a state declares artifact contracts, reference them via `{input.<name>.path}` and `{output.<name>.path}` instead of repeating raw paths. Unknown variables are left verbatim, so free-form braces in prose are safe.

## Workflow

1. Read the project specification and actor structure.
2. Decide whether the default `rhei` machine is sufficient. If yes, do not author a custom machine.
3. List the real phases, gates, retries, and handoffs.
4. Draft state names, descriptions, and instructions.
5. Add `gating`, state `personality`, `visits`, and model selectors only where they solve a real workflow need.
6. Draft transitions, including cancellation and recovery paths.
7. Add callbacks only where the workflow truly integrates with external automation.
8. Write the YAML file to `docs/states.yaml` or `docs/states/<name>.yaml`.
9. Validate the result with the CLI when available.

## Validation Checklist

Before returning the machine, verify:

- Exactly one state has `initial: true`
- At least one state has `final: true`
- No final state has outgoing transitions
- Every non-final state has a path to a final state
- Every non-initial state is reachable from the initial state
- Each transition has `from`, `to`, and `description`
- Gating states are marked with `gating: true` and their instructions forbid autonomous exit
- If `models` is present, every `model` and `all_models` entry is declared there
- No state declares both `all_models` and `model`
- `visits`, when present, is at least `1`
- State names are lowercase hyphenated identifiers
- The `name` field is a meaningful project-derived identifier

If the `rhei` CLI is available, validate with:

```bash
rhei states --state-machine <path>
```

Use `--json` when you need machine-readable output.

## File Placement

Use one of these conventional locations:

- `.agents/rhei/states.yaml`
- `.agents/rhei/states/<name>.yaml`

The plan writer then references the machine by name via `**States:** <name>`.
