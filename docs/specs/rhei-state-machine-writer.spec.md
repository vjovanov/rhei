# Rhei State Machine Writer

This document specifies a new role in the Rhei ecosystem: the **State Machine Writer**. It designs custom state machines (YAML files) tailored to a project's specification, team structure, and workflow requirements.

For the YAML schema and transition callback system see the [Transitions Specification](rhei-transitions.spec.md). For the default state machine see the [States Specification](rhei-states.spec.md).

## Purpose

The default `rhei` state machine covers a generic agent workflow (draft, pending, review, completed, with optional human review). Many projects need domain-specific workflows:

- A compliance-heavy project may require multiple approval gates from distinct teams.
- A data pipeline project may model stages like `ingesting`, `transforming`, `validating`, `published`.
- A release process may need `staging`, `canary`, `rollout`, `stable`.

The state machine writer analyzes two inputs — the **project specification** and the **project teams** — and produces a state machine YAML file that encodes the project's real workflow as Rhei states and transitions.

## Inputs

### 1. Project Specification

The project specification describes what the project does, its deliverables, phases, quality gates, and constraints. This can come from:

- A requirements document, PRD, or design doc.
- A README or AGENTS.md in the repository.
- Verbal description from the user.
- An existing Rhei plan whose workflow doesn't match the default machine.

The state machine writer extracts from the specification:

| Extract | Maps to |
|---------|---------|
| Distinct workflow phases | States |
| Phase ordering and branching | Transitions |
| Quality gates and checkpoints | Gating states (no autonomous exit) |
| Failure modes and recovery paths | Error/retry states and transitions |
| Automated checks | Callback-eligible transitions |

### 2. Project Teams

The team structure describes who is involved and what authority each team or role has. This can come from:

- An explicit team list with roles.
- Organizational context described by the user.
- Existing approval workflows (PR reviewers, sign-off chains).

The state machine writer maps teams to workflow elements:

| Team aspect | Maps to |
|-------------|---------|
| Teams that must approve before proceeding | Gating states with team name (e.g., `security-review`, `legal-review`) |
| Teams that perform work | Work states with team-specific instructions |
| Handoff points between teams | Transitions between team-owned states |
| Autonomous agents | Non-gating states with agent instructions |
| Human decision-makers | Gating states (no autonomous exit) |

## Output

The state machine writer produces a single YAML file conforming to the [YAML State Machine Format](rhei-transitions.spec.md#yaml-state-machine-format-specification). The file is ready to be referenced by a Rhei plan's `**States:**` declaration.

### Output Structure

```yaml
name: <project-derived-name>
version: 1.0
models:
  - <model-name>
  - <model-name>

states:
  <state-name>:
    description: <what this state means in the project context>
    instructions: |
      <agent-facing guidance: what to do in this state, when to transition out>
    initial: true|false  # exactly one state must be initial
    final: true|false    # at least one state must be final
    all_models: [<model-name>, ...] # optional: run once for each listed declared model
    model: <model-name>    # optional: exactly one declared model may work this state

transitions:
  - from: <source-state>
    to: <target-state>
    description: <when and why this transition occurs>
    # Optional callback fields when automation is needed:
    # on_leave: <callback-name>
    # on_enter: <callback-name>
    # condition: <expression>
    # timeout: <duration>
```

When a machine declares `models`, each state may either:

- omit model selectors entirely
- set `all_models: [<model-name>, ...]`
- set `model: <model-name>`

It must not set both `all_models` and `model` on the same state.

## Design Rules

The state machine writer follows these rules when designing a state machine:

### State Design

1. **Every distinct workflow phase gets its own state.** Don't overload a single state with multiple meanings. If two phases have different instructions or different exit conditions, they are different states.

2. **Name states after what is happening, not who is doing it.** Prefer `security-review` over `security-team`. The state describes the phase; the instructions describe the actor.

3. **Mark exactly one state as `initial: true`.** This is the entry point. Tasks start here. In most workflows, this is a "ready" or "pending" state — not an active work state.

4. **Mark at least one state as `final: true`.** Terminal states are absorbing — no outgoing transitions. Every workflow needs at least a success terminal (`completed`, `deployed`, `published`) and typically a cancellation terminal (`cancelled`, `abandoned`).

5. **Make human approval gates explicit.** Any state where an agent must stop and wait for human judgment should have instructions that say "do not transition out of this state autonomously." These are gating states. Name them to reflect the approver: `legal-review`, `manager-approval`, `security-sign-off`.

6. **Keep state count proportional to workflow complexity.** A simple project needs 4-6 states. A complex multi-team pipeline might need 8-12. More than 15 states suggests the workflow should be split into separate state machines for different plan types.

7. **Include recovery states when failure is expected.** If a phase can fail and be retried, model the retry path explicitly (e.g., `validation-failed` → `fixing` → `validating`). Don't rely on implicit "go back to the previous state."

### Transition Design

1. **Every transition must be explicitly declared.** Unlisted transitions are forbidden. This is the core safety property of Rhei state machines.

2. **Transitions encode the project's real workflow, not hypothetical paths.** If a transition never happens in practice, don't declare it. If a transition is rare but valid, declare it.

3. **Fan-out from a state represents decisions.** When a state has multiple outgoing transitions, each target represents a different outcome. Document in the transition's `description` when each path is taken.

4. **The transition graph must be connected.** Every non-initial state must be reachable from the initial state. Every non-terminal state must have a path to at least one terminal state. Unreachable or dead-end states are design errors.

5. **Provide a cancellation path.** Use a wildcard transition (`from: "*"`) to a `cancelled` terminal state, or declare explicit cancellation transitions from each non-terminal state. Every task must be cancellable.

6. **Team handoffs are transitions.** When work passes from one team to another, model it as a transition between team-owned states. The `on_leave` callback on the source state packages the deliverable; the `on_enter` callback on the target state notifies the receiving team.

### Instructions Design

1. **Write instructions for the actor in that state.** If the state is for an agent, write what the agent should do. If the state is a human gate, write what the human is expected to decide.

2. **State when to transition out.** Every non-terminal state's instructions must describe the exit condition: "transition to X when Y is true."

3. **Reference concrete artifacts.** Don't write "review the work." Write "review the implementation against the task description and subtasks. Check that tests pass and lint is clean."

## Workflow

The state machine writer follows this process:

1. **Gather inputs.** Read the project specification and team structure. If either is missing or incomplete, ask the user before proceeding.

2. **Identify phases.** List the distinct workflow phases from the specification. Each phase becomes a candidate state.

3. **Identify actors.** Map each phase to its actor: which team or agent type performs work in that phase, and who approves the exit.

4. **Draft states.** For each phase, write a state with a description and instructions. Mark initial and final states.

5. **Draft transitions.** For each pair of phases that have a direct handoff, declare a transition. Add a description explaining when the transition fires.

6. **Add safety paths.** Ensure every state has a path to a terminal state. Add cancellation transitions. Add recovery/retry paths for phases that can fail.

7. **Add callbacks (optional).** If the project uses programmatic transitions, assign `on_leave` and `on_enter` callback names to transitions that integrate with external systems.

8. **Validate the design.**
   - Exactly one initial state.
   - At least one final state (typically two: success and cancellation).
   - Every non-initial state reachable from the initial state.
   - Every non-terminal state has a path to at least one terminal state.
   - No orphan states (states with no incoming or outgoing transitions, except initial/terminal).
   - Transition graph is connected.
   - State names are lowercase, hyphenated identifiers (matching the `IDENTIFIER` grammar production).
   - Instructions describe exit conditions for every non-terminal state.

9. **Write the YAML file.** Emit the file conforming to the YAML State Machine Format.

10. **Validate with the CLI.** If the `rhei` CLI is available, run `rhei states --state-machine <path>` to verify the file parses correctly.

## Examples

### Example 1: Data Pipeline Project

**Input — specification:** "We process customer data through ingestion, transformation, validation, and publication stages. Bad data must be quarantined for manual review."

**Input — teams:** "Data engineering builds and runs the pipeline. Data quality team reviews quarantined records. Product team signs off on publication to production."

**Output:**

```yaml
name: data-pipeline
version: 1.0

states:
  queued:
    description: Data batch is registered and waiting to be processed.
    instructions: |
      Pick up when all Prior tasks are completed. Transition to ingesting
      to begin processing.
    initial: true

  ingesting:
    description: Raw data is being loaded from source systems.
    instructions: |
      Load data from the configured source. Validate schema on ingestion.
      On success, transition to transforming. On schema failure, transition
      to quarantined.

  transforming:
    description: Data is being cleaned, enriched, and reshaped.
    instructions: |
      Apply transformation rules. Log row counts before and after.
      On success, transition to validating.

  validating:
    description: Transformed data is checked against quality rules.
    instructions: |
      Run data quality checks (null rates, range checks, referential
      integrity). On pass, transition to publication-review. On fail,
      transition to quarantined.

  quarantined:
    description: Data failed quality checks and requires manual review.
    instructions: |
      Do not transition out of this state autonomously. Wait for the
      data quality team to investigate, fix, and either return to
      ingesting (reprocess) or cancel.

  publication-review:
    description: Product team reviews data before production publication.
    instructions: |
      Do not transition out of this state autonomously. Wait for
      product team sign-off. On approval, transition to published.
      On rejection, transition to transforming for rework.

  published:
    description: Data is live in production.
    instructions: |
      Terminal state. Do not modify.
    final: true

  cancelled:
    description: Processing abandoned.
    instructions: |
      Terminal state. Skip entirely.
    final: true

transitions:
  - from: queued
    to: ingesting
    description: Begin data processing
  - from: ingesting
    to: transforming
    description: Ingestion succeeded, proceed to transformation
  - from: ingesting
    to: quarantined
    description: Ingestion failed schema validation
  - from: transforming
    to: validating
    description: Transformation complete, run quality checks
  - from: validating
    to: publication-review
    description: Quality checks passed, ready for product review
  - from: validating
    to: quarantined
    description: Quality checks failed
  - from: quarantined
    to: ingesting
    description: Data quality team approved reprocessing
  - from: publication-review
    to: published
    description: Product team approved publication
  - from: publication-review
    to: transforming
    description: Product team requested rework
  - from: "*"
    to: cancelled
    description: Processing abandoned at any stage
```

### Example 2: Multi-Team Feature Delivery

**Input — specification:** "Features go through design, implementation, security review, QA, and release. Security issues block release."

**Input — teams:** "Product designs features. Engineering implements. Security team reviews for vulnerabilities. QA validates functionality. Release engineering manages deployments."

**Output:**

```yaml
name: feature-delivery
version: 1.0

states:
  design:
    description: Feature is being specified by product team.
    instructions: |
      Write the feature specification including acceptance criteria.
      Transition to ready-for-dev when the spec is complete and reviewed.
    initial: true

  ready-for-dev:
    description: Feature spec is approved, waiting for engineering pickup.
    instructions: |
      Pick up when all Prior tasks are completed. Transition to
      implementing to begin development.

  implementing:
    description: Engineering is building the feature.
    instructions: |
      Implement the feature per the specification. Write tests.
      When implementation is complete and self-tested, transition
      to security-review.

  security-review:
    description: Security team inspects the implementation for vulnerabilities.
    instructions: |
      Do not transition out of this state autonomously. Wait for
      the security team to review. On pass, transition to qa.
      On fail with findings, transition to security-fix.

  security-fix:
    description: Engineering addresses security findings.
    instructions: |
      Fix only the issues identified by the security team. No scope
      expansion. Transition back to security-review when fixes are applied.

  qa:
    description: QA team validates the feature against acceptance criteria.
    instructions: |
      Do not transition out of this state autonomously. Wait for
      QA validation. On pass, transition to release-ready.
      On fail, transition to implementing for rework.

  release-ready:
    description: Feature is approved and waiting for release window.
    instructions: |
      Release engineering picks up and transitions to released
      during the next deployment window.

  released:
    description: Feature is deployed to production.
    instructions: |
      Terminal state. Do not modify.
    final: true

  cancelled:
    description: Feature is abandoned.
    instructions: |
      Terminal state. Skip entirely.
    final: true

transitions:
  - from: design
    to: ready-for-dev
    description: Feature spec complete and approved
  - from: ready-for-dev
    to: implementing
    description: Engineering picks up the feature
  - from: implementing
    to: security-review
    description: Implementation complete, ready for security review
  - from: security-review
    to: qa
    description: Security review passed
  - from: security-review
    to: security-fix
    description: Security issues found, needs fixes
  - from: security-fix
    to: security-review
    description: Security fixes applied, ready for re-review
  - from: qa
    to: release-ready
    description: QA validation passed
  - from: qa
    to: implementing
    description: QA found defects, needs rework
  - from: release-ready
    to: released
    description: Deployed to production
  - from: "*"
    to: cancelled
    description: Feature abandoned at any stage
```

## Relationship to Other Roles

| Role | Relationship |
|------|-------------|
| **Plan Writer** | Uses the state machine produced by this role. References it via `**States:** <name>` in the plan header. |
| **Plan Worker** | Executes tasks using the states and transitions defined by this role. Follows the `instructions` field on each state. |
| **Reviewer** | Review states in the machine define what the reviewer checks and how they advance work. |
| **Human Operator** | Gating states designed by this role define where human judgment is required. |

The state machine writer runs **before** the plan writer. The plan writer consumes the YAML file; it does not design workflows.

## When to Use a Custom State Machine

Use the default `rhei` state machine when:
- The project follows a standard agent workflow (implement, review, complete).
- There are no domain-specific phases or team handoffs.
- The plan is small and does not need specialized gates.

Use a custom state machine when:
- The project has domain-specific phases (data stages, compliance gates, deployment tiers).
- Multiple teams are involved with distinct approval authorities.
- The workflow has non-trivial branching (retry loops, conditional paths, escalation chains).
- Automated callbacks should fire on specific transitions.
- The default states don't capture the project's real workflow.

## File Placement

State machine YAML files should be placed in the project at a conventional location:

- `docs/states.yaml` — for projects with a single state machine.
- `docs/states/<name>.yaml` — for projects with multiple state machines.

The file path is then referenced by the plan's `**States:**` declaration, and by the `rhei` CLI via `--state-machine <path>`.

## Related Specifications

- [Plan Language Specification](../rhei.spec.md) — formal grammar and semantic constraints
- [States Specification](rhei-states.spec.md) — default state machine format
- [Transitions Specification](rhei-transitions.spec.md) — YAML schema, callbacks, and transition system
- [How Rhei Is Used](rhei-usage.spec.md) — roles, coordination patterns, and agent workflows
