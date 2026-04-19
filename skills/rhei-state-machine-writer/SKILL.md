---
name: rhei-state-machine-writer
description: Design and generate custom Rhei state machine YAML files from project specifications and team structures. Use when users need a workflow that doesn't fit the default rhei state machine — domain-specific phases, multi-team handoffs, approval gates, or automated callback integrations.
---

# Rhei State Machine Writer

Produce a YAML state machine file that encodes a project's real workflow as Rhei states and transitions, ready to be referenced by a plan's `**States:**` declaration.

## Output Contract

- Emit a single YAML file conforming to the Rhei state machine format.
- Include `name`, `version`, `states`, and `transitions` at the root level.
- Mark exactly one state as `initial: true`.
- Mark at least one state as `final: true` (typically a success terminal and a cancellation terminal).
- Every state must have `description` and `instructions` fields.
- Every transition must have `from`, `to`, and `description` fields.
- Only declare transitions that represent real workflow paths — unlisted transitions are forbidden at runtime.

### YAML Structure

```yaml
name: <project-derived-name>
version: 1.0
models:
  - <model-name>
  - <model-name>

states:
  <state-name>:
    description: <what this state means>
    instructions: |
      <agent-facing guidance: what to do, when to transition out>
    initial: true   # exactly one
    final: true     # at least one
    all_models: [<model-name>, ...]  # optional: run once for each listed declared model
    model: <model-name> # optional: exactly one declared model may work this state

transitions:
  - from: <source>
    to: <target>
    description: <when and why>
    # Optional:
    # on_leave: <callback-name>
    # on_enter: <callback-name>
    # condition: <expression>
    # timeout: <duration>
```

If the machine declares `models`, each state may either omit model selectors,
set `all_models: [<model-name>, ...]`, or set `model: <model-name>`. Never set
both on the same state.

## Inputs

Gather two inputs before designing the state machine. If either is missing, ask the user.

### 1. Project Specification

What the project does, its deliverables, phases, quality gates, and constraints. Extract:

- **Distinct workflow phases** → states
- **Phase ordering and branching** → transitions
- **Quality gates and checkpoints** → gating states (no autonomous exit)
- **Failure modes and recovery** → error/retry states and transitions
- **Automated checks** → callback-eligible transitions

Sources: requirements docs, PRDs, READMEs, AGENTS.md, verbal description, or an existing plan whose default states don't fit.

### 2. Project Teams

Who is involved and what authority each team or role has. Map:

- **Teams that must approve** → gating states named after the review (e.g., `security-review`, `legal-review`)
- **Teams that perform work** → work states with team-specific instructions
- **Handoff points between teams** → transitions between team-owned states
- **Autonomous agents** → non-gating states with agent instructions
- **Human decision-makers** → gating states with "do not transition autonomously" instructions

Sources: explicit team lists, organizational context, existing approval workflows.

## State Design Rules

1. **One state per distinct phase.** If two phases have different instructions or different exit conditions, they are different states.

2. **Name states after what is happening.** Prefer `security-review` over `security-team`. The state describes the phase; the instructions describe the actor.

3. **Exactly one `initial: true` state.** The entry point. Usually a "ready" or "queued" state, not an active work state.

4. **At least two `final: true` states.** A success terminal (`completed`, `deployed`, `published`) and a cancellation terminal (`cancelled`, `abandoned`).

5. **Make human gates explicit.** Instructions must say "do not transition out of this state autonomously." Name the state to reflect the approver: `legal-review`, `manager-approval`, `security-sign-off`.

6. **Keep state count proportional.** Simple projects: 4-6 states. Complex multi-team pipelines: 8-12. More than 15 suggests splitting into separate state machines.

7. **Model recovery explicitly.** If a phase can fail and retry, declare the retry path as states and transitions (e.g., `validation-failed` -> `fixing` -> `validating`).

## Transition Design Rules

1. **Every transition is explicitly declared.** Unlisted = forbidden.

2. **Only model real paths.** If a transition never happens in practice, don't declare it.

3. **Fan-out = decisions.** Multiple outgoing transitions from one state represent different outcomes. Document when each fires.

4. **The graph must be connected.** Every non-initial state reachable from initial. Every non-terminal state has a path to a terminal.

5. **Provide a cancellation path.** Use `from: "*"` wildcard to a `cancelled` state, or declare explicit cancellation transitions from each non-terminal state. Wildcard excludes final states.

6. **Team handoffs are transitions.** When work passes between teams, model it as a transition. Use `on_leave`/`on_enter` callbacks for notification and packaging.

## Instructions Design Rules

1. **Write for the actor in that state.** Agent states get implementation guidance. Human gates describe the decision to make.

2. **State exit conditions.** Every non-terminal state's instructions must say "transition to X when Y."

3. **Reference concrete artifacts.** Not "review the work" but "review the implementation against task description and subtasks, check tests pass."

## Design Workflow

1. **Gather inputs.** Read project spec and team structure. Ask if incomplete.
2. **List phases.** Each distinct workflow phase is a candidate state.
3. **Map actors.** Which team or agent acts in each phase? Who approves exit?
4. **Draft states.** Write description and instructions for each. Mark initial/final.
5. **Draft transitions.** Declare handoffs between phases. Add descriptions.
6. **Add safety paths.** Cancellation from every non-terminal state. Recovery/retry where failure is expected.
7. **Add callbacks (optional).** Assign `on_leave`/`on_enter` names for transitions that integrate with external systems.
8. **Validate the design** (see checklist below).
9. **Write the YAML file.**
10. **CLI validation.** If available, run `rhei states --state-machine <path>` to verify.

## Validation Checklist

Before returning output, verify:

- Exactly one state has `initial: true`.
- At least one state has `final: true` (typically two: success + cancellation).
- Every state has `description` and `instructions`.
- Every non-terminal state's instructions describe exit conditions.
- Every non-initial state is reachable from the initial state.
- Every non-terminal state has a path to at least one terminal state.
- No orphan states (no incoming and no outgoing transitions, except initial/terminal).
- Every transition has `from`, `to`, and `description`.
- State names are lowercase hyphenated identifiers (`[a-z][a-z0-9-]*`).
- The `name` field is a meaningful project-derived identifier.
- Cancellation is possible from every non-terminal state (via wildcard or explicit transitions).
- Gating states have "do not transition autonomously" in instructions.
- No transitions originate from final states.

## File Placement

Save the output YAML to a conventional location:

- `docs/states.yaml` — single state machine per project.
- `docs/states/<name>.yaml` — multiple state machines per project.

The plan writer references this file via `**States:** <name>` in the plan header.

## When NOT to Use This Skill

Use the default `rhei` state machine (don't create a custom one) when:

- The project follows a standard implement/review/complete workflow.
- There are no domain-specific phases or team handoffs.
- The plan is small and doesn't need specialized gates.

## Missing Information Handling

If required input is missing:

- Ask the user to describe the project's workflow phases and team structure.
- If the user points to existing documentation, read it and extract the workflow.
- Do not guess team structures or approval requirements — these must come from the user.
