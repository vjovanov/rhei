# How Rhei Is Used

This document describes how Rhei plans are consumed by agents, humans, and programs. It covers the distinct roles that interact with a plan, the coordination patterns they follow, and how the state machine governs the workflow.

For the formal grammar see the [Plan Language Specification](../rhei.spec.md). For authoring patterns see the [Usage Guide](rhei-authoring.spec.md).

## Roles

A Rhei plan is a shared artifact read and written by several distinct roles. Each role has a narrow mandate — what it may read, what it may change, and when it must stop.

### Plan Writer

The plan writer creates and restructures plans. It translates a goal into a dependency graph of tasks with states, subtasks, and prose context.

Responsibilities:
- Decompose work into independently completable tasks.
- Assign dependencies (`**Prior:**`) to encode sequencing.
- Set initial states (typically `draft` or `pending`).
- Maintain the DAG invariant — no cycles, no dangling references.
- Add or reorganize context sections (`## Overview`, `## Requirements`, etc.) before `## Tasks`.

The plan writer does **not** execute tasks or advance states during implementation. Structural edits and implementation progress are separate concerns.

### Plan Worker

The plan worker picks up an existing plan and makes progress on it. It is driven entirely by the plan: the state machine defines what transitions are legal, `**Prior:**` edges define what is ready, and state instructions define what to do.

Responsibilities:
- Claim the next eligible task via `rhei next` (all priors completed, unassigned, not terminal or gating, and all required current-state inputs present).
- Work in the current state following the state's `instructions`.
- Advance the state when exit conditions are met (e.g., `draft` → `pending`, `pending` → `agent-review`) or finish directly with `rhei complete` when review is not required.
- Stop at gating states (`human-review`) and terminal states (`completed`, `cancelled`).

The plan worker does **not** add tasks, reorder tasks, change dependencies, or delete sections. Its edits are limited to `**State:**` values and subtask progress logs.

### Reviewer

The reviewer inspects completed work before it advances to the next state. In the default Rhei state machine, this role appears in two forms:

- **Agent reviewer** (`agent-review`): An automated pass that checks the implementation against the task description, subtasks, and repository conventions (lint, format, tests). It records concrete findings in the task body or in a required findings artifact when the active state machine declares one, then transitions to `agent-review-fix` (fail), `human-review` (needs human), or `completed` (pass).

- **Human reviewer** (`human-review`): A human inspects the work. No agent may transition out of this state autonomously. The human decides: return to `pending` (rework), `completed` (approve), or `cancelled` (abandon).

### Human Operator

The human operator has full authority over the plan. They can:

- Transition tasks out of `human-review`.
- Cancel tasks at any point.
- Edit the plan structure (add tasks, change dependencies) — usually by re-invoking the plan writer.
- Override any state, including terminal states, when explicitly needed.

The human is the only role that can unblock `human-review` gates. This is by design: certain decisions (ship/no-ship, scope changes, external approvals) require human judgment.

## Coordination Through the State Machine

The state machine is the single coordination protocol between all roles. It defines:

1. **What states exist** and what each one means (via `description` and `instructions` fields).
2. **What transitions are legal** — any transition not declared is forbidden.
3. **Who acts in each state** — the `instructions` field tells the current role what to do and when to hand off.

This means agents do not need to communicate with each other directly. They communicate through the plan file: one agent writes a state, the next agent reads it and acts accordingly.

### State Flow (Default Machine)

```
draft --> pending --> agent-review --> completed
  |         |  \          |               ^
  v         |   +-------->+               |
cancelled   |             v               |
            |        agent-review-fix ----+
            |             |
            v             v
        human-review   cancelled
            | 
            +--> pending
            +--> completed
            +--> cancelled
```

The `pending → completed` direct transition allows agents to finish simple tasks that do not require a separate review pass.

The `human-review` state is deliberately isolated: agents can send work into it (from `pending` or `agent-review`), but only a human can transition out of it. This separation ensures that human judgment gates are never bypassed by automated workflows.

Each arrow is a declared transition. Agents follow the `instructions` field on each state to know when to fire each transition.

## Usage Patterns

### Pattern 0: Zero-Config Agent Execution

The simplest way to use Rhei. No callbacks, no `workflow.sh`, no glue code.

1. Configure your default agent once:

   ```bash
   mkdir -p ~/.config/rhei
   echo '{"agent": "claude-code"}' > ~/.config/rhei/settings.json
   ```

2. Create a plan (manually or via `rhei-plan-writer`).

3. Run it:

   ```bash
   rhei run plan.rhei.md
   ```

Rhei spawns the configured agent for each task, composing a prompt from the state machine instructions and the task content. The agent does the work and calls `rhei complete` when done. No scaffolding required.

For state-specific agents (e.g., a different agent or model for review):

```yaml
# states.yaml
states:
  pending:
    model: impl-fast
    agent: claude-code
    agent_timeout: 30m
  agent-review:
    model: review-deep
    agent: codex
    agent_timeout: 20m
```

For parallel execution of independent tasks:

```bash
rhei run plan.rhei.md --parallel 4
```

See [Agents Specification](rhei-agents.spec.md) for configuration, resolution order, timeout handling, and log capture.

### Pattern 1: Single Agent, Start to Finish

One agent session acts as both writer and worker. This is useful when the agent session itself drives the workflow rather than `rhei run`.

1. User describes the goal.
2. Agent invokes the plan writer to produce a `.rhei.md` file.
3. Agent invokes the plan worker on the same file.
4. Worker iterates through tasks, advancing states, implementing, and logging progress.
5. Worker stops when all tasks are `completed` or blocked on `human-review`.

This is the default for small, well-scoped work within a single agent session. The plan still provides value: it gives the human a structured view of progress, and the agent a memory of what it has and hasn't done. For larger work or multi-agent workflows, Pattern 0 (`rhei run`) is preferred.

### Pattern 2: Writer and Worker as Separate Sessions

For larger work, the plan writer and plan worker run in separate agent sessions — possibly days apart.

1. **Session 1 (planning):** Human describes a project. Agent produces a plan with tasks in `draft` or `pending`.
2. **Human review:** Human reads the plan, adjusts scope, promotes drafts to `pending`, reorders priorities.
3. **Session 2 (execution):** A new agent session invokes the plan worker. It reads the plan fresh, selects the first eligible task, and begins work.
4. **Subsequent sessions:** If the worker stops (human gate, end of session, ambiguity), a new session picks up where it left off by re-reading the plan.

The plan file is the handoff mechanism. No session state needs to survive between sessions — the plan captures everything.

### Pattern 3: Parallel Workers on Independent Branches

When a plan's DAG has independent branches (tasks with no shared dependencies), multiple workers can operate in parallel.

```markdown
### Task 1: Set up database schema
**State:** pending

### Task 2: Build API endpoints
**State:** pending
**Prior:** Task 1

### Task 3: Write frontend components
**State:** pending

### Task 4: Integration tests
**State:** pending
**Prior:** Task 2, Task 3
```

Here, Task 1 and Task 3 have no dependency relationship. Two workers can implement them concurrently. Task 4 remains blocked until both Task 2 and Task 3 are completed.

Coordination happens through `rhei next`, `rhei transition`, and `rhei complete`. A worker claims a task, implements it, and then completes it:

```bash
# Inspect what is next without claiming (read-only, safe for PM browsing)
rhei next --peek plan.rhei.md

# Claim the next ready task (assigns without transitioning, prints instructions)
rhei next plan.rhei.md

# ... agent does the work ...

# Advance state when ready (e.g., draft → pending, or pending → agent-review)
rhei transition plan.rhei.md --task 3 --from draft --to pending

# Complete: transition to terminal state, write result file, release assignment
rhei complete plan.rhei.md --task 3 --result "Schema migration applied successfully"
```

For finer-grained control, workers can use `rhei transition` directly with a compare-and-swap:

```bash
rhei transition plan.rhei.md --task 3 --from pending --to agent-review
```

The `--from` flag is the key: the command acquires a file lock, verifies the task is still in the expected state, and only then writes the new state. If another worker already claimed the task, the command fails with a conflict error — the losing worker re-reads the plan and picks a different task.

This eliminates last-write-wins races without requiring an external scheduler. The plan file plus its lock file are the only coordination primitives needed.

### Pattern 3b: Highly Distributed Swarms (Directory Workspaces)

If parallel workers are distributed across multiple branches or machines, the single-file lock approach breaks down (leading to Git merge conflicts). In these highly concurrent scenarios, agents use **Directory Workspaces**.

Instead of a single `plan.rhei.md`, the plan is hosted as a directory with tasks separated into a `tasks/` directory (`tasks/db-schema.md`, `tasks/integration.md`). 
Because tasks are isolated in distinct files, Git effortlessly merges cross-branch progress without text collisions, mirroring the resilience of database-backed trackers like Beads.

### Pattern 4: Human-in-the-Loop Checkpoints

Plans that require human approval use the `human-review` state as a gate.

```markdown
### Task 3: Deploy to production
**State:** `human-review`
**Prior:** Task 2

Review: All tests pass. Staging smoke tests succeeded. Ready for production deploy.
```

When a task reaches `human-review`:
- All agents stop work on that task.
- The human inspects the implementation, reads review notes, and decides.
- The human transitions the task to `pending` (rework), `completed` (approve), or `cancelled`.

Meanwhile, agents continue working on other branches of the DAG that are not blocked by this gate. The plan stays productive even when one branch is waiting on human input.

### Pattern 5: Draft Expansion

For exploratory or long-running projects, tasks start as `draft` — placeholder titles that are not yet ready for execution.

1. Plan writer creates tasks in `draft` with rough titles but minimal descriptions.
2. When all prior tasks are completed, `rhei next` claims the draft task (sets `**Assignee:**` without changing its state).
3. The agent works in `draft`: it analyzes the current state of the project, determines the most elegant approach, and writes a concrete task description.
4. The agent transitions `draft` → `pending` via `rhei transition`.
5. The agent continues working in `pending`, implementing the task per the now-concrete description.

This prevents agents from planning against stale or incomplete project state. The `draft` state is a signal: "this task exists in the plan but needs analysis before it can be specified and executed." Because `rhei next` claims without transitioning, the agent has a dedicated phase to do the analysis work before advancing the state.

### Pattern 6: Programmatic State Transitions

Beyond agent-driven workflows, Rhei plans can be advanced programmatically through the transition callback system. State machines declare `on_leave` and `on_enter` callbacks that fire during transitions, enabling integration with external systems.

**CLI (bash callbacks):**
```bash
rhei-cli run my-plan.rhei.md \
    --state-machine states.yaml \
    --handlers ./workflow-handlers.sh
```

**JavaScript (NAPI bindings):**
```typescript
const rhei = new Rhei({
  stateMachine: './states.yaml',
  rheiPath: './my-plan.rhei.md'
});

rhei.onLeave('pending', 'in-progress', async (ctx) => {
  // Validate preconditions before allowing transition
  return { success: true };
});

await rhei.run();
```

**Python (PyO3 bindings):**
```python
rhei = Rhei(
    state_machine="./states.yaml",
    rhei_path="./my-plan.rhei.md"
)

@rhei.on_leave("pending", "in-progress")
def check_ready(ctx):
    return TransitionResult(success=True)

rhei.run()
```

**Java (JNI bindings):**
```java
Rhei rhei = new Rhei(RheiConfig.builder()
    .stateMachine("./states.yaml")
    .rheiPath("./my-plan.rhei.md")
    .build());
rhei.run();
```

Callbacks can approve, reject, or redirect transitions — turning the plan into an executable workflow engine. See [Transitions Specification](rhei-transitions.spec.md) for the formal callback API and [Transition Callback Examples](rhei-callbacks.spec.md) for practical implementations.

### Pattern 7: Living Workspace Expansion

A directory workspace can stay intentionally incomplete at authoring time and be
expanded by the orchestrator while `rhei run` is executing. This is useful when
follow-up work should only exist after a concrete artifact is produced.

One example is a review loop:

1. A seed task runs three reviewers (`claude`, `codex`, and `antigravity`) and writes a shared findings file.
   When done, the orchestrator calls `rhei complete` to record the result
   (written to `runtime/results/review-seed.md`), transition to
   `completed`, and release the assignment.
2. A `codex` coordinator appends one verification task per consolidated review
   point.
3. Each verification task runs on `codex`, records whether the issue is reproducible and
   relevant, then completes with its own result file.
4. Only relevant findings cause new fix tasks to be appended to the workspace.

The workspace starts small:

```markdown
# Rhei: Living Review Loop
**States:** living-review-loop

## Overview
The orchestrator expands this workspace as review artifacts arrive.
```

```markdown
### Task review-seed: Run three-model review and seed the living workspace
**State:** review

Write a shared findings file and append verification tasks for each review
point.
```

After `review-seed` completes, the orchestrator may append task files such as:

```markdown
### Task verify-cache-key: Verify and reproduce finding F-001
**State:** prove
**Prior:** Task review-seed
```

And after verification, only the relevant issues become fix tasks:

```markdown
### Task fix-cache-key: Fix finding F-001 after verified reproduction
**State:** prove
**Prior:** Task verify-cache-key
```

This keeps the Rhei truthful: speculative fixes do not appear in the task graph
until the review artifact justifies them. The checked-in example lives in
[`examples/living-review-loop`](../../examples/living-review-loop/README.md).

### Pattern 8: Program States for Deterministic Steps

Program states execute a fixed command instead of spawning an AI agent. Use them for deterministic workflow steps — builds, tests, linting, deployment — where an agent would add latency and cost without value.

Programs communicate outcomes through exit codes. Transitions from program states can declare an `exit_code` condition that routes automatically based on the result:

```yaml
states:
  build:
    program: "make build"
    program_timeout: 10m
  test:
    program: "make test"
    program_timeout: 15m

transitions:
  - from: build
    to: test
    exit_code: 0
  - from: build
    to: failed
    exit_code: nonzero
```

Program and agent states coexist in the same state machine. A common pattern is an agent-program feedback loop: the agent implements code, deterministic build/test steps verify the result, and failures return control to the agent for fixes:

```yaml
transitions:
  - from: pending
    to: build
    description: Implementation complete, verify it builds
  - from: build
    to: test
    exit_code: 0
  - from: build
    to: pending
    description: Build failed, agent must fix the code
    exit_code: nonzero
```

See [Program States Specification](rhei-programs.spec.md) for the full exit-code evaluation algorithm, timeout handling, and validation rules.

### Pattern 9: CI/CD Pipeline as a Plan

A Rhei plan can model a CI/CD pipeline where each task is a pipeline stage. The state machine encodes the pipeline's control flow, and callbacks integrate with build systems, test runners, and deployment tools.

```markdown
# Rhei: Release Pipeline
**States:** ci-pipeline

## Tasks

### Task 1: Lint and type-check
**State:** pending

### Task 2: Unit tests
**State:** pending
**Prior:** Task 1

### Task 3: Integration tests
**State:** pending
**Prior:** Task 2

### Task 4: Deploy to staging
**State:** pending
**Prior:** Task 3

### Task 5: Production deploy
**State:** draft
**Prior:** Task 4
```

Task 5 starts as `draft` — it will only be promoted to `pending` after staging is verified. The pipeline advances automatically through callbacks, but human gates can be inserted at any stage.

## The Plan as Shared Memory

The central design principle: **the plan file is the single source of truth**. Agents do not maintain internal state about what has been done or what comes next. They read the plan, act on it, write back to it, and validate. This has several consequences:

- **Resumability.** A new agent session can pick up any plan mid-execution. No context beyond the file is needed.
- **Auditability.** The plan's git history shows every state transition, every progress log entry, and who (or what) made each change.
- **Human legibility.** The plan is standard markdown. Humans read it directly — no dashboards, no query languages, no special tooling required.
- **Composability.** Different agents (writer, worker, reviewer) interact with the same file through well-defined, non-overlapping edits. The state machine prevents conflicts by making illegal transitions impossible.

## Related Specifications

- [Plan Language Specification](../rhei.spec.md) — formal grammar and semantic constraints
- [Plan Language Usage Guide](rhei-authoring.spec.md) — authoring patterns and walkthroughs
- [States Specification](rhei-states.spec.md) — state machine format and default states
- [Agents Specification](rhei-agents.spec.md) — agent configuration, invocation, timeout, and log capture
- [Program States Specification](rhei-programs.spec.md) — deterministic program execution, exit-code transitions
- [Transitions Specification](rhei-transitions.spec.md) — formal state transition system, callbacks, and YAML schema
- [Transition Callback Examples](rhei-callbacks.spec.md) — callback implementations across languages
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
- [State Machine Writer](rhei-state-machine-writer.spec.md) — designing custom state machines from project specs and teams
- [Install Skills](rhei-install-skills.spec.md) — `rhei install-skills` command for agent integration
