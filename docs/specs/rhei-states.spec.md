# Rhei States Specification

This document defines the default states configuration for tasks in the Rhei plan compiler. The authoritative machine-readable form lives in [states.yaml](states.yaml); the writer-skill mirror is [default-states.md](../../skills/rhei-plan-writer/references/default-states.md).

The state-machine schema also permits these optional fields for richer workflows:

- Per-state `personality: <string>` to inject role framing into `rhei next` for that specific state (supports template variables)
- Template variables in `instructions` and `personality` fields, resolved by `rhei next` at output time
- Top-level `models: [<model-name>, ...]` to declare the model identifiers available to the machine
- Per-state `all_models: [<model-name>, ...]` to declare the full model set that may execute that state
- Per-state `model: <model-name>` to bind a state to exactly one declared model
- Per-state `agent: <agent-id>` to bind a state to a specific coding agent CLI
- Per-state `agent_timeout: <duration>` to set the maximum time an agent may work in this state
- Per-state `program: <string|object>` to bind a state to a deterministic program command (mutually exclusive with `agent`)
- Per-state `program_timeout: <duration>` to set the maximum time a program may run in this state
- Per-state `visits: <integer>` to cap total counted visits for that state
- Per-state `inputs:` / `outputs:` artifact contracts to require workspace files on entry/exit

When `models` is omitted, the machine behaves as it does today and states are not model-constrained. When `models` is present, a state may either omit both selector fields, set `all_models: [<name>, ...]`, or set `model: <name>`. Setting both `all_models` and `model` on the same state is invalid. `visits` is orthogonal to model selection and may be used together with either `all_models` or `model`.

When `agent` is set on a state, `rhei run` uses that agent to execute work in the state instead of relying on callbacks alone. The agent/model pair together define who does the work: the agent is the CLI tool, the model is the specific model variant within that tool. See [Agents Specification](rhei-agents.spec.md) for configuration, resolution order, and invocation details.

## Schema Additions

### Top-level fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `models` | string array | No | The complete set of model identifiers available to the machine |

### Per-state fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `personality` | string | No | State-specific role framing printed by `rhei next` for that state |
| `gating` | boolean | No | When `true`, autonomous commands (`rhei next`, `rhei complete`, engine-triggered transitions) must not transition out of this state. Only explicit human-initiated transitions are allowed. |
| `visits` | integer | No | Maximum number of visits permitted for this state before the workflow must take a non-loop exit |
| `all_models` | string array | No | The complete set of declared model identifiers allowed to work this state |
| `model` | string | No | A single model identifier from the machine-level `models` list |
| `agent` | string or object | No | The coding agent CLI that executes work in this state. String form references a known agent ID (e.g., `claude-code`, `codex`). Object form defines a custom agent profile. See [Agents Specification](rhei-agents.spec.md). |
| `agent_timeout` | string | No | Maximum time an agent may work in this state before being killed (e.g., `30m`, `1h`). See [Agents Specification — Timeout Handling](rhei-agents.spec.md#timeout-handling). |
| `program` | string or object | No | The program command to execute in this state. String form runs via shell. Object form specifies `command`, `env`, `working_directory`, and `shell`. Mutually exclusive with `agent`. See [Program States Specification](rhei-programs.spec.md). |
| `program_timeout` | string | No | Maximum time the program may run before being killed (e.g., `10m`, `1h`). Same duration format and timeout handling as `agent_timeout`. See [Program States Specification](rhei-programs.spec.md#timeout-handling). |
| `inputs` | artifact array | No | Required artifacts that must exist before the task can enter or continue in this state |
| `outputs` | artifact array | No | Required artifacts that must exist before the task can leave this state |

### Validation Rules

- `models`, when present, must be a list of unique non-empty strings.
- `state.model`, when present, must match an entry from the machine-level `models` list.
- `state.all_models`, when present, must be a list of unique non-empty strings drawn from the machine-level `models` list.
- A state must not declare both `all_models: [..]` and `model: <name>`.
- `state.all_models: []` is treated the same as omitting the field.
- `state.visits`, when present, must be an integer greater than or equal to `1`.
- `state.agent`, when present, must be a non-empty string (known agent ID) or a valid agent profile object with at least `id` and `command` fields.
- `state.agent` on a `final: true` state is a validation error (terminal states have no work to execute).
- `state.agent` on a `gating: true` state is a validation warning (gating states are human-only; the agent will never be invoked by `rhei run`).
- `state.agent_timeout`, when present, must be a valid duration string (e.g., `30s`, `5m`, `1h`, `2h30m`).
- A state must not declare both `agent` and `program`.
- `state.program`, when present, must be a non-empty string or a valid program object with at least a `command` field. See [Program States Specification](rhei-programs.spec.md).
- `state.program` on a `final: true` state is a validation error (terminal states have no work to execute).
- `state.program` on a `gating: true` state is a validation error (gating states require human action; programs execute autonomously).
- `state.program_timeout`, when present, must be a valid duration string (e.g., `30s`, `5m`, `1h`, `2h30m`).
- `state.inputs` / `state.outputs`, when present, must be arrays of unique artifact definitions keyed by `name`.
- Artifact `path` values must be relative to the plan root (single-file plan) or workspace root (directory workspace) and must not escape that root after template expansion.

Counted-loop counters are task-instance data, not state-definition data. The state machine declares the cap with `visits`; runtimes persist the current per-task counts in task metadata and mirror the active visit in markdown by appending `-<n>` to `**State:**` for visits greater than `1`.

When a state declares both `all_models` and `visits`, the engine runs the state once per listed model and each model-specific execution tracks its own visit budget.

## Artifact Contracts

States may declare required file artifacts as explicit contracts. This lets a
workflow say "review must produce findings" or "fix cannot begin until findings
exist" in machine-readable form rather than relying on prose instructions.

Each artifact definition has this shape:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Stable identifier for the artifact within that state |
| `path` | string | Yes | Workspace-relative file path template |
| `description` | string | No | Human-readable explanation of what the artifact contains |

Supported path template variables:

- `{task_id}` - the current task id as rendered in the plan
- `{state}` - the canonical unsuffixed state name
- `{visit_count}` - the current visit number for counted-loop states (only available when the state declares `visits`)
- `{model}` - the current model identifier (only available when the state declares `model` or `all_models`)

Runtime semantics:

- `inputs` are checked before entering the state and before work begins in that
  state. If any required input file is missing, the transition is rejected.
- `outputs` are checked after callbacks complete and before the transition out
  of the state is committed. If any required output file is missing, the
  transition is rejected.
- Artifact contracts are file-existence contracts in v1. They do not yet define
  JSON schemas, required headings, or content-level validation.

Example:

```yaml
states:
  agent-review:
    description: Review the implementation and record concrete findings
    inputs:
      - name: implementation-summary
        path: runtime/results/{task_id}.md
        description: Result written by the implementation step
    outputs:
      - name: findings
        path: runtime/findings/{task_id}.md
        description: Review findings for the task

  agent-review-fix:
    description: Address reviewer findings without changing scope
    inputs:
      - name: findings
        path: runtime/findings/{task_id}.md
        description: Findings produced by `agent-review`
```

## Template Variables in Instructions and Personality

The `instructions` and `personality` fields support template variable substitution. Variables use the same `{variable}` syntax as artifact path templates. `rhei next` resolves all template variables before printing output, so agents receive fully expanded prompts with no manual variable resolution required.

### Variable Namespace

| Variable | Source | Description | Example Value |
|----------|--------|-------------|---------------|
| `{task_id}` | claimed task | Task identifier as rendered in the plan | `3`, `setup` |
| `{task_title}` | claimed task | Task title text | `Implement caching layer` |
| `{state}` | state machine | Canonical unsuffixed state name | `review` |
| `{visit_count}` | runtime counter | Current visit number for counted-loop states | `2` |
| `{visits}` | state definition | Configured loop budget for the state | `3` |
| `{model}` | model selector | Current model identifier (requires `model` or `all_models`) | `claude-sonnet` |
| `{agent}` | agent selector | Current agent identifier (requires `agent` on the state or in settings) | `claude-code` |
| `{plan_title}` | plan header | Title from the `# Rhei: <title>` header | `Feature Branch CI Pipeline` |
| `{plan_path}` | filesystem | Path to the plan file | `./ci-pipeline.rhei.md` |
| `{input.<name>.path}` | artifact contract | Resolved path of a declared input artifact | `runtime/results/3.md` |
| `{output.<name>.path}` | artifact contract | Resolved path of a declared output artifact | `runtime/findings/3.md` |
| `{meta.<key>}` | task metadata | Value from the task's YAML metadata section | `alice`, `2` |

### Resolution Rules

- **Resolve at output time, not load time.** Template variables are expanded by `rhei next` when printing instructions to an agent. The state machine YAML remains portable — the same `states.yaml` works across different plans.
- **Fail-open on unknown variables.** An unrecognized variable like `{foo}` is left verbatim in the output. This avoids breaking existing instructions that happen to contain braces and makes templates forward-compatible with future variables.
- **Pure substitution, no expressions.** Templates produce text, not decisions. Conditional logic belongs in transition `condition` fields, not in instructions. The resolved text tells the agent "you are on pass 2 of 3" — the agent reads that to decide what to do.
- **Artifact references create a single source of truth.** Using `{input.<name>.path}` or `{output.<name>.path}` instead of repeating raw paths means the artifact contract defines the path once. If the path changes, instructions stay correct automatically.
- **`{visit_count}` and `{visits}` are only meaningful for counted-loop states.** For states without a `visits` declaration, `{visits}` is left unresolved and `{visit_count}` resolves to `1`.

### Example

```yaml
states:
  review:
    description: Review pass that appends findings to a shared artifact.
    instructions: |
      You are on review pass {visit_count} of {visits} for Task {task_id}: {task_title}.

      Review the current task output and append one numbered review pass to
      `{output.review-notes.path}`.

      After each review pass, transition to `fix`.
    initial: true
    visits: 2
    outputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}-review-{visit_count}.md

  fix:
    description: Fix step that consumes the review artifact.
    instructions: |
      Fix pass {visit_count} of {visits} for Task {task_id}: {task_title}.

      Read `{input.review-notes.path}`, extract the accumulated review
      findings, and update `{output.fix-notes.path}`.

      Transition back to `review` if {visit_count} < {visits}.
      Otherwise, transition to `completed`.
    visits: 2
    inputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}-review-{visit_count}.md
    outputs:
      - name: fix-notes
        path: runtime/fixes/task-{task_id}-fix-{visit_count}.md
```

When `rhei next` claims Task 3 ("Implement caching layer") during the second visit to `fix`, the agent receives:

```text
Fix pass 2 of 2 for Task 3: Implement caching layer.

Read `runtime/reviews/task-3-review-2.md`, extract the accumulated review
findings, and update `runtime/fixes/task-3-fix-2.md`.

Transition back to `review` if 2 < 2.
Otherwise, transition to `completed`.
```

### Multi-Model Example

```yaml
models:
  - claude
  - codex

states:
  review:
    description: Independent review by each model
    personality: |
      You are {model}. Provide a review from your perspective.
      Do not attempt to emulate or defer to another model's style.
    instructions: |
      Review the implementation for Task {task_id}.
      Read `{input.implementation.path}` and write your findings to
      `{output.findings.path}`.
    all_models: [claude, codex]
    inputs:
      - name: implementation
        path: runtime/results/{task_id}.md
    outputs:
      - name: findings
        path: runtime/findings/{task_id}-{model}.md
```

Here `{model}` appears in both the artifact path and the instructions. The artifact contract defines the per-model output path once; instructions reference it by name.

## Agent Field

States can declare which coding agent executes work in that state. The `agent` field names a known agent ID (matching the IDs used by `rhei install-skills --agent`) or provides a custom agent profile object.

```yaml
states:
  draft:
    description: Task requires analysis before execution.
    agent: claude-code
    model: claude-opus-4-6
    agent_timeout: 15m
    initial: true

  pending:
    description: Task is ready for implementation.
    agent: claude-code
    model: claude-sonnet-4-6
    agent_timeout: 30m

  agent-review:
    description: A separate reviewing agent inspects the result.
    agent: codex
    model: o3
    agent_timeout: 20m

  agent-review-fix:
    description: The implementing agent addresses reviewer findings.
    agent: claude-code
    model: claude-sonnet-4-6
    agent_timeout: 30m

  human-review:
    gating: true
    # No agent - humans act here

  completed:
    final: true
  cancelled:
    final: true
```

When `agent` is set on a state, `rhei run` spawns that agent to execute work. When `agent` is omitted, `rhei run` falls back to project-level or global-level settings. See [Agents Specification](rhei-agents.spec.md) for full resolution order, invocation profiles, and timeout handling.

The `agent` and `model` fields form a pair: the agent is the CLI tool, the model is the variant within that tool. Either can be set independently at any configuration level, and the resolution merges across levels.

## Program States

States can declare a deterministic program to execute instead of spawning an AI agent. The `program` field names a command (string form) or provides a structured command definition (object form). Program states are the right choice for build, test, lint, deploy, and other steps where the behavior is fixed and an AI agent adds no value.

```yaml
states:
  build:
    description: Build the project
    program: "make build"
    program_timeout: 10m

  test:
    description: Run the test suite
    program:
      command: ["npm", "test", "--", "--coverage"]
      env:
        NODE_ENV: test
    program_timeout: 15m
    outputs:
      - name: coverage
        path: coverage/lcov.info

  deploy:
    description: Deploy to staging
    program:
      command: ["./scripts/deploy.sh", "{meta.deploy_env}"]
      working_directory: ./infra
    program_timeout: 20m
```

When `program` is set, `rhei run` spawns the command as a subprocess instead of an agent. The program communicates its outcome via exit code, and transitions from program states can declare `exit_code` conditions for automatic routing:

```yaml
transitions:
  - from: build
    to: test
    exit_code: 0
  - from: build
    to: failed
    exit_code: nonzero
```

A state must not declare both `agent` and `program` — they are mutually exclusive. The `program` field is also incompatible with `gating: true` (programs run autonomously).

See [Program States Specification](rhei-programs.spec.md) for the complete specification including program declaration forms, exit-code transitions, environment variables, timeout handling, and validation rules.

## States

| State | Description | Initial | Final | Gating |
|-------|-------------|---------|-------|--------|
| `draft` | Task is still being shaped; description not ready for execution | Yes | No | No |
| `pending` | Task ready for implementation once prerequisites are `completed` | No | No | No |
| `agent-review` | A separate reviewing agent inspects the result | No | No | No |
| `agent-review-fix` | Implementing agent applies reviewer findings, no scope change | No | No | No |
| `human-review` | Work paused pending human inspection; no autonomous exit | No | No | Yes |
| `completed` | Task finished successfully; immutable | No | Yes | No |
| `cancelled` | Task no longer needed; skip entirely | No | Yes | No |

## Transitions

See [states.yaml](states.yaml) for the enforced transition table. Summary:

- `draft` → `pending` | `cancelled`
- `pending` → `agent-review` | `human-review` | `completed` | `cancelled`
- `agent-review` → `agent-review-fix` (fail) | `human-review` (pass, gated) | `completed` (pass, ungated)
- `agent-review-fix` → `agent-review` | `cancelled`
- `human-review` → `pending` | `completed` | `cancelled`

Any transition not listed in `states.yaml` is forbidden.

### Completion paths

Not every state can be completed directly via `rhei complete`. The command requires a non-cancelled terminal state reachable in one hop:

- From `pending`, `agent-review`: direct completion to `completed` is available.
- From `agent-review-fix`: no direct path to `completed` exists. The agent must transition to `agent-review` first, then complete from there.
- From `human-review`: completion is blocked because the state is gating (`gating: true`). Only a human-initiated `rhei transition` can exit this state.

## Related Documentation

- [Plan Language Specification](../rhei.spec.md) - Formal grammar and semantic constraints
- [Agents Specification](rhei-agents.spec.md) - Agent configuration, invocation profiles, timeout, and log capture
- [Program States Specification](rhei-programs.spec.md) - Deterministic program execution, exit-code transitions
- [Transitions Specification](rhei-transitions.spec.md) - Formal state transition system, callbacks, and YAML schema
- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Usage Guide](rhei-authoring.spec.md) - Practical authoring patterns and walkthroughs
- [Transition Callback Examples](rhei-callbacks.spec.md) - Callback implementations across languages
- [State Machine Writer](rhei-state-machine-writer.spec.md) - Designing custom state machines from project specs and teams
