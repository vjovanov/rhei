# Rhei Program States Specification

This document specifies **program states** — states that execute a deterministic program or command instead of spawning an AI coding agent. Program states are the algorithmic complement to agent states: where agents receive prompts and make decisions, programs execute a fixed command and let exit codes and file outputs determine workflow progression.

For agent states see [Agents Specification](rhei-agents.spec.md). For the state machine format see [States Specification](rhei-states.spec.md).

## Overview

A program state runs a command as a subprocess. The command receives context through environment variables and command-line template variables. Its exit code and file outputs determine what happens next. This is the right choice for deterministic workflow steps — builds, tests, linting, deployment, data transforms, migrations — where an AI agent would add latency and cost without value.

Program states participate in the same `rhei run` execution loop as agent states. They support the same artifact contracts, counted loops, timeout handling, log capture, and callback integration. The only difference is how the work is performed: a fixed command instead of a prompted agent.

## Program Declaration

### String Form

The simplest form runs a shell command:

```yaml
states:
  build:
    description: Build the project
    program: "npm run build"
    program_timeout: 10m
```

String-form commands are executed via the system shell (`/bin/sh -c` on Unix, `cmd /c` on Windows).

### Object Form

For more control over execution:

```yaml
states:
  build:
    description: Build the project
    program:
      command: ["npm", "run", "build"]
      env:
        NODE_ENV: production
      working_directory: ./packages/core
    program_timeout: 10m
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `command` | string or string array | Yes | The command to execute. Array form bypasses the shell (exec directly). String form runs via shell. |
| `env` | object | No | Additional environment variables merged with the base set. Values support template variables. |
| `working_directory` | string | No | Override the subprocess working directory. Relative to workspace root. Supports template variables. Default: workspace root. |
| `shell` | boolean | No | Force shell execution even for array-form commands. Default: `false` for arrays, `true` for strings. |

### Template Variables in Commands

Program commands support the same template variables as `instructions` and agent prompts:

```yaml
states:
  test:
    program: "./scripts/test.sh {task_id}"
    program_timeout: 5m

  deploy:
    program:
      command: ["./scripts/deploy.sh", "--env", "{meta.deploy_env}"]
      env:
        TASK_TITLE: "{task_title}"
```

Variables are resolved by `rhei run` before spawning the process, using the same resolution rules as agent prompt composition. See [Template Variables](rhei-states.spec.md#template-variables-in-instructions-and-personality).

## Environment Variables

Program subprocesses inherit the same base environment as agent subprocesses:

| Variable | Value |
|----------|-------|
| `RHEI_PLAN_PATH` | Absolute path to the plan file or workspace directory |
| `RHEI_TASK_ID` | Current task identifier |
| `RHEI_STATE` | Current state name |
| `RHEI_VISIT_COUNT` | Current visit number (for counted-loop states) |
| `RHEI_INPUT_<NAME>_EXISTS` | `true` or `false` — whether the declared input artifact exists on disk. Set for every declared input, required or optional. `<NAME>` is the artifact `name` uppercased with hyphens and spaces replaced by underscores (e.g., `continuation-notes` → `RHEI_INPUT_CONTINUATION_NOTES_EXISTS`). |
| `RHEI_INPUT_<NAME>_PATH` | Resolved path of the declared input artifact. Set for every declared input regardless of whether the file exists. Same name transform as `RHEI_INPUT_<NAME>_EXISTS`. |

Additional variables declared in `program.env` are merged on top of this base set. When a `program.env` key collides with a base variable, the `program.env` value wins.

The working directory defaults to the workspace root (for directory workspaces) or the plan file's parent directory (for single-file plans), unless overridden by `program.working_directory`.

## Exit-Code Transitions

Programs communicate their outcome through exit codes. Transitions from program states can declare an `exit_code` condition that matches specific exit values, enabling automatic routing without the program needing to call `rhei transition` directly.

```yaml
states:
  build:
    description: Build the project
    program: "make build"

  test:
    description: Run the test suite
    program: "make test"

  build-failed:
    description: Build failed
    gating: true

  test-failed:
    description: Tests failed
    gating: true

transitions:
  - from: build
    to: test
    description: Build succeeded
    exit_code: 0

  - from: build
    to: build-failed
    description: Build failed
    exit_code: nonzero

  - from: test
    to: completed
    description: All tests passed
    exit_code: 0

  - from: test
    to: test-failed
    description: Tests failed with assertion errors
    exit_code: [1, 2]

  - from: test
    to: test-failed
    description: Test infrastructure error
    exit_code: nonzero
```

### `exit_code` Field

Added to the transition definition schema (see [Transitions Specification](rhei-transitions.spec.md)):

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `exit_code` | integer, integer array, or `"nonzero"` | No | Exit-code condition for automatic transitions from program states. |

Values:
- **integer** (`0`, `1`, `42`): matches that exact exit code.
- **integer array** (`[1, 2, 3]`): matches any listed exit code.
- **`"nonzero"`**: matches any non-zero exit code not already matched by a more specific transition from the same source state.

### Evaluation Order

When a program exits and has not already advanced the task (via `rhei transition` or `rhei complete`):

1. Collect all transitions from the current state that declare an `exit_code` field.
2. Evaluate specific matches first: integer and integer-array conditions are checked against the actual exit code.
3. If no specific match and the exit code is non-zero, evaluate `"nonzero"` transitions.
4. If exactly one transition matches, fire it (with its `on_leave`/`on_enter` callbacks).
5. If multiple transitions match at the same specificity level, also evaluate `condition` fields to disambiguate. If still ambiguous after conditions, it is a validation error.
6. If no transition matches and exit code is `0`, log a warning: `warning: program exited 0 but task {id} did not advance from '{state}'`.
7. If no transition matches and exit code is non-zero, log an error and apply the `--continue-on-error` policy.

### Mixing Exit-Code and Manual Transitions

Programs may also call `rhei transition` or `rhei complete` directly via subprocess invocation, just like agents. When a program does so, the task state changes before the program exits, and exit-code evaluation is skipped entirely — the explicit transition takes precedence.

If a transition from a program state has no `exit_code` field, it is available for manual invocation (`rhei transition`) or for the program to call directly, but it will never be selected by exit-code evaluation.

This allows hybrid approaches where a program handles the common case via exit codes and calls `rhei transition` for edge cases that need more nuanced routing.

## Timeout Handling

Program timeout uses the same mechanism as agent timeout.

### Configuration

Timeout can be set at three levels:

1. **Per-state** — `program_timeout` field on the state definition:
   ```yaml
   states:
     build:
       program: "make build"
       program_timeout: 10m
   ```

2. **Global default** — `program_timeout` in settings:
   ```json
   { "program_timeout": "15m" }
   ```

3. **CLI override** — `--program-timeout` flag on `rhei run` overrides all levels.

Resolution: CLI override > state-level > settings-level.

Duration format is the same as `agent_timeout`: `30s`, `5m`, `1h`, `2h30m`. See [Agents Specification — Duration Format](rhei-agents.spec.md#duration-format).

### Behavior

When a program exceeds its timeout:

1. `rhei run` sends `SIGTERM` to the process.
2. After a 10-second grace period, if the process has not exited, send `SIGKILL`.
3. Log to the task log: `program timed out after {duration}`.
4. Evaluate timeout transitions from the current state (same mechanism as agent timeout — see [Transitions Specification](rhei-transitions.spec.md)).
5. If a timeout transition exists, fire it.
6. If no timeout transition exists, the task remains in its current state and a warning is logged.

## Log Capture

Program stdout and stderr are captured using the same log format and naming conventions as agents. All output is written to `runtime/logs/` relative to the workspace or plan root.

### Log File Naming

| Scenario | Log file path |
|----------|---------------|
| Simple state | `runtime/logs/task-{task_id}-{state}.log` |
| Counted-loop state | `runtime/logs/task-{task_id}-{state}-{visit_count}.log` |

### Log Format

```
=== rhei program log v1 ===
program: npm run build
task: 3
state: build
started: 2026-04-20T10:30:00Z
timeout: 10m
plan: /home/user/project/plan.rhei.md
===

<raw program stdout and stderr, interleaved>

=== exit ===
code: 0
duration: 2m15s
ended: 2026-04-20T10:32:15Z
===
```

The header distinguishes program logs from agent logs (`rhei program log` vs `rhei agent log`). The `v1` suffix is the log format version — increment it when the header/footer structure changes. The body is the raw, unmodified output of the program process.

## `rhei run` Integration

### Execution Loop

Program states integrate into the same `rhei run` loop as agent states. The resolution priority when `rhei run` encounters a state is:

1. State declares `program` → spawn program.
2. State declares `agent` → spawn agent.
3. Project/global agent settings → spawn agent.
4. No program or agent configured → fail with error.

Programs take precedence because they are declared explicitly on the state and represent a deliberate choice for deterministic execution.

### Sequential Mode

Within the sequential execution loop (see [Agents Specification](rhei-agents.spec.md#sequential-mode-default---parallel-1)):

1. Find the next claimable task.
2. Check the task's current state.
3. If the state declares `program`, resolve and expand the command.
4. Log the spawn to `runtime/logs/task-{task_id}-{state}[-{visit_count}].log`.
5. Spawn the program as a subprocess.
6. Wait for exit (subject to `program_timeout`).
7. Re-read the plan from disk. Check whether the task's state changed — `rhei transition` and `rhei complete` are file-mutating operations that write the new state directly to the plan file.
8. If state changed, log the transition and continue the loop.
9. If state did not change, evaluate exit-code transitions.
10. If an exit-code transition matches, fire it and continue.
11. If no match: warning (exit 0) or error (exit non-zero).

### Parallel Mode

Programs respect the same independence rules as agents in parallel mode. Independent tasks with program states can run concurrently up to `--parallel N`.

### Flags

| Flag | Effect on Program States |
|------|--------------------------|
| `--dry-run` | Shows the command that would be spawned, without executing |
| `--no-agent` | No effect on program states (programs are not agents) |
| `--no-program` | Suppresses program spawning; fall back to callback-only advancement |
| `--parallel <N>` | Programs respect the same independence and concurrency rules as agents |
| `--program-timeout <duration>` | Override program timeout for this run |

### Dry-Run Output

```
Pass 1: 2 ready, 0 terminal, 5 total.

Would run: npm run build
  Task 1: Build the project [draft -> build]
  Timeout: 10m
  Log: runtime/logs/task-1-build.log

Would spawn: claude -p "<prompt...>" --model claude-sonnet-4-6
  Task 3: Write documentation [draft -> pending]
  Agent: claude-code, Model: impl-fast (anthropic/claude-sonnet-4-6), Timeout: 30m
  Log: runtime/logs/task-3-pending.log

Dry run complete - nothing was executed.
```

### Gating States

A state must not declare both `program` and `gating: true`. Programs execute autonomously; gating states require human action. This combination is a validation error.

## Interaction with Other Features

### Artifact Contracts

Program states support the same `inputs` and `outputs` artifact contracts as any other state. Inputs are checked before spawning the program. Outputs are checked after the program exits and before the exit-code transition is committed.

```yaml
states:
  build:
    description: Build the project
    program: "make build"
    program_timeout: 10m
    outputs:
      - name: bundle
        path: dist/bundle.js
        description: Production build artifact

  test:
    description: Run tests against the build
    program: "make test"
    program_timeout: 15m
    inputs:
      - name: bundle
        path: dist/bundle.js
        description: Build artifact from the build step
    outputs:
      - name: coverage
        path: coverage/lcov.info
        description: Test coverage report
```

### Callbacks

Programs and callbacks are complementary, the same as agents and callbacks:

1. The program runs and exits.
2. Exit-code transition evaluation determines the target state.
3. The selected transition's `on_leave` and `on_enter` callbacks execute as usual.

```yaml
transitions:
  - from: build
    to: test
    exit_code: 0
    on_leave: "cli:bash ./workflow.sh archive-build"
    on_enter: "cli:bash ./workflow.sh prepare-test-env"
```

### Counted Loops

Programs work with `visits` for retry and iteration patterns:

```yaml
states:
  deploy:
    description: Deploy to staging
    program: "./scripts/deploy.sh staging"
    program_timeout: 15m
    visits: 3

  deploy-failed:
    description: Deployment exhausted retries
    gating: true

  verify:
    description: Verify the deployment
    program: "./scripts/verify.sh staging"
    program_timeout: 5m

transitions:
  - from: deploy
    to: deploy
    description: Retry deployment
    exit_code: nonzero
    condition: visitCount < visits

  - from: deploy
    to: deploy-failed
    description: Deployment exhausted retries
    exit_code: nonzero
    condition: visitCount >= visits

  - from: deploy
    to: verify
    description: Deployment succeeded
    exit_code: 0
```

### Instructions and Personality

`instructions` on program states serve as documentation — they describe what the program does for human operators viewing `rhei next` output. They are not passed to the program.

`personality` is ignored for program states.

### Models

`model` and `all_models` are ignored for program states. Programs do not use AI models. Declaring both `program` and `model`/`all_models` on the same state is a validation warning (not an error, to allow gradual migration).

## Validation Rules

### State-Level

- `state.program`, when present, must be a non-empty string or a valid program object with at least a `command` field.
- A state must not declare both `agent` and `program`.
- `state.program` on a `final: true` state is a validation error (terminal states have no work to execute).
- `state.program` on a `gating: true` state is a validation error (gating states require human action).
- `state.program_timeout`, when present, must be a valid duration string (e.g., `30s`, `5m`, `1h`, `2h30m`).
- `program.command`, when present as an array, must be a non-empty array of strings.
- `program.env` values must be strings (after template variable resolution).
- `program.working_directory`, when present, must resolve to a path within the workspace root after template expansion.

### Transition-Level

- Transitions with `exit_code` must originate from a state that declares `program`. An `exit_code` on a transition from a non-program state is a validation error.
- Exit-code transitions from the same source state must not have overlapping values at the same specificity. Specifically:
  - Two specific integers must not be equal.
  - Two integer arrays must not share any value.
  - A specific integer must not appear in another transition's integer array.
  - At most one `"nonzero"` transition per source state (before `condition` disambiguation).
- When multiple `"nonzero"` or overlapping transitions exist from the same source, they must have mutually exclusive `condition` fields to disambiguate.

## Per-State Fields (additions to States Specification)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `program` | string or object | No | The program command to execute in this state. Mutually exclusive with `agent`. String form runs via shell. Object form specifies `command`, `env`, `working_directory`, and `shell`. |
| `program_timeout` | string | No | Maximum time the program may run before being killed (e.g., `10m`, `1h`). Same duration format and timeout handling as `agent_timeout`. |

## Transition Field (addition to Transitions Specification)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `exit_code` | integer, integer array, or `"nonzero"` | No | Exit-code condition for transitions from program states. Only evaluated when the program exits without calling `rhei transition`. |

## Example: CI Pipeline — Pure Program Workflow

A fully deterministic pipeline with no AI agents:

```yaml
name: ci-pipeline
version: 1.0

states:
  lint:
    description: Run linters and formatters
    program: "npm run lint"
    program_timeout: 5m
    initial: true

  build:
    description: Build the project
    program:
      command: ["npm", "run", "build"]
      env:
        NODE_ENV: production
    program_timeout: 10m

  test:
    description: Run the test suite
    program: "npm test -- --coverage"
    program_timeout: 15m
    outputs:
      - name: coverage
        path: coverage/lcov.info

  deploy-staging:
    description: Deploy to staging environment
    program:
      command: ["./scripts/deploy.sh", "staging"]
      env:
        DEPLOY_ENV: staging
    program_timeout: 20m
    visits: 3

  human-review:
    description: Human verifies staging deployment
    gating: true

  deploy-prod:
    description: Deploy to production
    program:
      command: ["./scripts/deploy.sh", "production"]
      env:
        DEPLOY_ENV: production
    program_timeout: 20m

  completed:
    description: Pipeline finished successfully
    final: true

  failed:
    description: Pipeline failed
    final: true

transitions:
  - from: lint
    to: build
    description: Lint passed
    exit_code: 0
  - from: lint
    to: failed
    description: Lint failed
    exit_code: nonzero

  - from: build
    to: test
    description: Build succeeded
    exit_code: 0
  - from: build
    to: failed
    description: Build failed
    exit_code: nonzero

  - from: test
    to: deploy-staging
    description: Tests passed
    exit_code: 0
  - from: test
    to: failed
    description: Tests failed
    exit_code: nonzero

  - from: deploy-staging
    to: human-review
    description: Staging deploy succeeded
    exit_code: 0
  - from: deploy-staging
    to: deploy-staging
    description: Staging deploy failed, retry
    exit_code: nonzero
    condition: visitCount < visits
  - from: deploy-staging
    to: failed
    description: Staging deploy exhausted retries
    exit_code: nonzero
    condition: visitCount >= visits

  - from: human-review
    to: deploy-prod
    description: Human approved staging
  - from: human-review
    to: failed
    description: Human rejected staging

  - from: deploy-prod
    to: completed
    description: Production deploy succeeded
    exit_code: 0
  - from: deploy-prod
    to: failed
    description: Production deploy failed
    exit_code: nonzero
```

## Example: Mixed Agent and Program Workflow

Program states and agent states coexist naturally in the same state machine. Use programs for deterministic steps and agents for creative work:

```yaml
name: feature-workflow
version: 1.0

states:
  draft:
    description: Agent analyzes requirements and writes task description
    model: impl-fast
    agent: claude-code
    initial: true

  pending:
    description: Agent implements the feature
    model: impl-fast
    agent: claude-code
    agent_timeout: 30m

  build:
    description: Build the project to verify implementation compiles
    program: "make build"
    program_timeout: 10m

  test:
    description: Run the test suite against the implementation
    program: "make test"
    program_timeout: 15m

  agent-review:
    description: A separate agent reviews the implementation
    model: review-deep
    agent: claude-code
    agent_timeout: 20m

  agent-review-fix:
    description: Implementing agent addresses reviewer findings
    model: impl-fast
    agent: claude-code
    agent_timeout: 30m

  completed:
    description: Feature implemented, built, tested, and reviewed
    final: true

  failed:
    description: Pipeline failed
    final: true

transitions:
  - from: draft
    to: pending
    description: Analysis complete, ready for implementation

  - from: pending
    to: build
    description: Implementation complete, verify it builds

  - from: build
    to: test
    description: Build succeeded, run tests
    exit_code: 0
  - from: build
    to: pending
    description: Build failed, agent must fix the code
    exit_code: nonzero

  - from: test
    to: agent-review
    description: Tests passed, submit for review
    exit_code: 0
  - from: test
    to: pending
    description: Tests failed, agent must fix the code
    exit_code: nonzero

  - from: agent-review
    to: completed
    description: Review passed, feature complete
  - from: agent-review
    to: agent-review-fix
    description: Review found issues

  - from: agent-review-fix
    to: build
    description: Fixes applied, re-verify build
```

In this workflow, the agent implements code, then deterministic build and test steps verify the result. If they fail, control returns to the agent for fixes. This creates a tight agent-program feedback loop where each component does what it does best.

## Related Specifications

- [States Specification](rhei-states.spec.md) — State machine format, per-state fields, artifact contracts
- [Agents Specification](rhei-agents.spec.md) — Agent states (the AI-driven complement to program states)
- [Transitions Specification](rhei-transitions.spec.md) — Transition callbacks, timeout transitions, `exit_code` field
- [How Rhei Is Used](rhei-usage.spec.md) — Roles, coordination patterns, agent workflows
- [Templates Specification](rhei-templates.spec.md) — Reusable plan bundles (program states work in templates)
