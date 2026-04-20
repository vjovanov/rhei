# Rhei Agents Specification

This document specifies how Rhei integrates with coding agents — the CLI tools that execute work on tasks. It covers agent configuration, resolution order, invocation profiles, prompt composition, parallel execution, timeout handling, and log capture.

For the state machine format see [States Specification](rhei-states.spec.md). For transition callbacks see [Transitions Specification](rhei-transitions.spec.md).

## Overview

Rhei can spawn coding agents directly from `rhei run`. Instead of requiring hand-written `workflow.sh` callback scripts, the run command resolves an agent for each task, composes a prompt from the state machine instructions, and spawns the agent as a subprocess. The agent does the work and calls `rhei transition` or `rhei complete` to advance the task. Callbacks still fire on transitions — agents and callbacks are complementary.

## Agent Configuration

### Global Settings

File: `~/.config/rhei/settings.json`

```json
{
  "agent": "claude-code",
  "model": null
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `agent` | string or object | Yes | Agent identifier (known ID) or custom agent profile |
| `model` | string or null | No | Default model override passed to the agent CLI |

### Project Settings

File: `.rhei/settings.json` in the workspace or plan directory.

Same schema as global settings. When present, overrides global settings.

```json
{
  "agent": "claude-code",
  "model": "claude-sonnet-4-6"
}
```

### Per-State Settings

The `agent` and `model` fields on state definitions in `states.yaml`. See [States Specification — Agent Field](rhei-states.spec.md#agent-field).

### Resolution Order

When `rhei run` needs an agent for a task in a given state, it resolves the agent/model pair using this precedence (first match wins):

1. **State-level** — `agent` and/or `model` on the state definition in `states.yaml`.
2. **Project-level** — `.rhei/settings.json` in the workspace or plan directory.
3. **Global-level** — `~/.config/rhei/settings.json`.
4. **CLI override** — `--agent` and `--model` flags on `rhei run` override all levels.

If no agent is configured at any level and no `--agent` flag is passed, `rhei run` fails:

```
error: no agent configured.
Set one in ~/.config/rhei/settings.json, .rhei/settings.json, or the state machine.
Alternatively, pass --agent <AGENT> to rhei run.
```

The `model` field is optional at every level. When omitted, the agent is spawned without a model flag and uses its own default.

### Partial Overrides

Each level can override `agent`, `model`, or both independently. The resolution merges across levels:

```yaml
# states.yaml — state overrides only the model
states:
  agent-review:
    model: o3        # uses project-level agent, but forces model to o3
```

```json
// .rhei/settings.json — project sets the agent
{ "agent": "claude-code" }
```

Result for `agent-review`: agent=`claude-code` (from project), model=`o3` (from state).

## Known Agent Profiles

Rhei ships with built-in invocation profiles for known coding agents. Each profile defines how to spawn the agent, deliver the prompt, pass the model, and set default flags.

| Agent ID | Binary | Prompt Delivery | Model Flag | Default Args |
|----------|--------|-----------------|------------|--------------|
| `claude-code` | `claude` | `-p <prompt>` | `--model <m>` | `--permission-mode bypassPermissions` |
| `codex` | `codex` | `exec -- -` (stdin) | `--model <m>` | `--sandbox danger-full-access` |
| `aider` | `aider` | `--message <prompt>` | `--model <m>` | |
| `kilocode` | `kilo` | `-p <prompt>` | `--model <m>` | |
| `cursor` | `cursor` | `--prompt <prompt>` | `--model <m>` | |

The agent IDs match those used by `rhei install-skills --agent`.

### Custom Agent Profiles

When the built-in profiles don't fit, specify a custom agent as an object:

```json
{
  "agent": {
    "id": "my-agent",
    "command": ["my-agent", "--autonomous"],
    "prompt_flag": "--prompt",
    "model_flag": "--model",
    "stdin_prompt": false
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Identifier for logs and diagnostics |
| `command` | string array | Yes | Base command and fixed arguments |
| `prompt_flag` | string | No | Flag to pass the prompt (e.g., `--prompt`, `-p`). Omit if using stdin. |
| `model_flag` | string | No | Flag to pass the model. Omit if the agent doesn't support model selection. |
| `stdin_prompt` | boolean | No | When `true`, the prompt is piped to stdin instead of passed via flag. Default: `false`. |
| `timeout` | string | No | Default timeout for this agent (e.g., `30m`). Overridden by state-level `agent_timeout`. |

Custom agent profiles can appear in `settings.json` (global or project) or inline in `states.yaml`:

```yaml
states:
  pending:
    agent:
      id: my-agent
      command: ["my-agent", "--autonomous"]
      prompt_flag: "--prompt"
      stdin_prompt: false
```

## Prompt Composition

When `rhei run` spawns an agent for a task, it composes a prompt from the state machine definition and the task content. The prompt has this structure:

```
# Task {task_id}: {task_title}

## State: {state}

{resolved personality, if present}

## Instructions

{resolved instructions from state definition}

## Task Content

{task body from the plan, including subtasks}

## Rhei Commands

You are working in a rhei-managed plan at `{plan_path}`.
Use these commands to advance the task:

- `rhei transition {plan_path} --task {task_id} --from {state} --to <target>` — advance to the next state
- `rhei complete {plan_path} --task {task_id} --result "<message>"` — complete the task

Available transitions from `{state}`:
{list of declared transitions from current state, with descriptions}

Do not modify **State:** lines in the plan directly. Use the rhei CLI.
```

Template variables (`{task_id}`, `{model}`, `{visit_count}`, etc.) are resolved before the prompt is sent, using the same resolution rules as `rhei next`. See [Template Variables](rhei-states.spec.md#template-variables-in-instructions-and-personality).

The prompt is delivered to the agent via its configured prompt delivery mechanism (flag or stdin).

## Environment Variables

The agent subprocess inherits these environment variables, consistent with the callback environment:

| Variable | Value |
|----------|-------|
| `RHEI_PLAN_PATH` | Absolute path to the plan file or workspace directory |
| `RHEI_TASK_ID` | Current task identifier |
| `RHEI_STATE` | Current state name |
| `RHEI_MODEL` | Model identifier, if configured |
| `RHEI_AGENT` | Agent identifier |

The agent's working directory is set to the workspace root (for directory workspaces) or the plan file's parent directory (for single-file plans).

## `rhei run` — Agent Mode

### CLI

```
rhei run <RHEI_PLAN> [--dry-run] [--no-callbacks] [--no-agent] [--no-program]
                     [--agent <AGENT>] [--model <MODEL>] [--continue-on-error]
                     [--parallel <N>] [--program-timeout <DURATION>]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--dry-run` | false | Show what would be spawned without executing |
| `--no-callbacks` | false | Skip `on_leave`/`on_enter` callbacks |
| `--no-agent` | false | Disable agent spawning; fall back to callback-only advancement (pre-agent behavior) |
| `--no-program` | false | Disable program spawning; fall back to callback-only advancement for program states |
| `--agent <AGENT>` | | Override agent for this run (ignores settings files and state-level config) |
| `--model <MODEL>` | | Override model for this run |
| `--continue-on-error` | false | Continue to the next task when an agent or program exits non-zero |
| `--parallel <N>` | 1 | Maximum number of agents/programs to run concurrently. `0` means unlimited. |
| `--program-timeout <DURATION>` | | Override program timeout for this run (e.g., `10m`, `1h`). See [Program States Specification](rhei-programs.spec.md#timeout-handling). |

### Execution Loop

#### Sequential Mode (default, `--parallel 1`)

1. Load plan and state machine. Validate.
2. Find the next claimable task (same eligibility as `rhei next`).
3. Resolve the agent for the task's current state (resolution order above).
4. If no agent is configured, fail with an error.
5. Compose the prompt (see [Prompt Composition](#prompt-composition)).
6. Log the spawn to `runtime/logs/task-{task_id}-{state}[-{visit_count}].log`.
7. Spawn the agent CLI as a subprocess with the composed prompt.
8. Wait for the agent process to exit (subject to timeout — see [Timeout Handling](#timeout-handling)).
9. Re-read the plan. Check whether the task's state changed.
10. If the task advanced or completed, log the transition and continue the loop.
11. If the task state did not change and the agent exited 0, log a warning: `warning: agent exited 0 but task {id} did not advance from '{state}'`. Continue to the next task.
12. If the agent exited non-zero:
    - Without `--continue-on-error`: log the error and stop.
    - With `--continue-on-error`: log the error, skip this task, continue.
13. Repeat until no claimable tasks remain or all tasks are terminal.

#### Parallel Mode (`--parallel N` where N > 1 or N = 0)

1. Load plan and state machine. Validate.
2. Find all claimable tasks (same eligibility as `rhei next`, but collect all candidates).
3. Select up to N tasks that are mutually independent (no dependency edges between them). When N = 0, select all independent claimable tasks.
4. For each selected task, resolve the agent, compose the prompt, and spawn the agent subprocess concurrently. Each agent writes to its own log file.
5. Wait for any agent to exit (timeout or completion).
6. When an agent exits:
   a. Re-read the plan.
   b. Process the result (same rules as sequential mode: check state change, handle errors).
   c. Scan for newly claimable tasks (dependencies may have been unblocked).
   d. If new tasks are claimable and the pool is below N, spawn agents for them.
7. Repeat until no claimable tasks remain or all tasks are terminal.

**Independence rule:** Two tasks are independent when neither appears in the other's transitive `**Prior:**` chain. The engine must not spawn two agents that could produce conflicting edits to the same task file. For directory workspaces, each task lives in a separate file, so file conflicts are avoided by construction.

**Single-file plans:** Parallel mode is limited to `--parallel 1` for single-file plans because agents could produce conflicting edits to the same file. `rhei run` prints a warning if `--parallel` > 1 is requested with a single-file plan and falls back to sequential execution.

### Interaction Between Agents and Callbacks

Agents and callbacks are complementary, not exclusive:

- **Agent** does the work (coding, reviewing, fixing).
- **Callbacks** handle side effects (creating artifacts, spawning tasks, notifying systems).

When `rhei run` is in agent mode:
1. The agent is spawned for the current state.
2. When the agent calls `rhei transition` or `rhei complete`, callbacks fire as usual.
3. After the agent exits, `rhei run` checks the plan state and continues.

`--no-callbacks` suppresses callbacks but not agent or program spawning. `--no-agent` suppresses agent spawning but not program spawning or callbacks. `--no-program` suppresses program spawning but not agent spawning or callbacks. All three can be combined independently.

### Gating States

When a task reaches a gating state (`gating: true`), `rhei run` does not spawn an agent. Instead it logs:

```
Task {id} is in gating state '{state}'. Waiting for human action.
```

The task is skipped and the engine continues with other claimable tasks. When the human transitions the task out of the gating state (via `rhei transition`), the next run pass picks it up.

## Timeout Handling

### Configuration

Timeout can be set at three levels:

1. **Per-state** — `agent_timeout` field on a state definition:
   ```yaml
   states:
     pending:
       agent_timeout: 30m
   ```

2. **Per-agent profile** — `timeout` field in custom agent definitions:
   ```json
   { "agent": { "id": "my-agent", "command": ["my-agent"], "timeout": "1h" } }
   ```

3. **Global default** — `agent_timeout` in settings:
   ```json
   { "agent": "claude-code", "agent_timeout": "30m" }
   ```

Resolution: state-level > agent-profile > settings-level. If no timeout is configured at any level, there is no timeout (the engine waits indefinitely).

### Duration Format

Durations use a human-readable format: `30s`, `5m`, `1h`, `2h30m`. Supported units:

| Unit | Suffix | Example |
|------|--------|---------|
| Seconds | `s` | `30s` |
| Minutes | `m` | `5m` |
| Hours | `h` | `1h` |

Units can be combined: `1h30m`, `2h15m30s`.

### Timeout Behavior

When an agent process exceeds its timeout:

1. `rhei run` sends `SIGTERM` to the agent process.
2. After a 10-second grace period, if the process has not exited, send `SIGKILL`.
3. Log to the task log: `agent timed out after {duration}`.
4. Look for a timeout transition from the current state in the state machine.
5. If a timeout transition exists, fire it (with its `on_leave`/`on_enter` callbacks).
6. If no timeout transition exists, the task remains in its current state and the engine logs a warning.

### Timeout Transitions

Timeout transitions are declared in the `transitions` array with the `timeout` field. The existing `timeout` field in the transition schema (see [Transitions Specification](rhei-transitions.spec.md#transition-definition)) is used by `rhei run` to determine what to do when an agent times out.

When a task is being worked by an agent and the agent exceeds the state's timeout:

1. The engine kills the agent process.
2. The engine evaluates timeout transitions from the current state.
3. The first matching timeout transition fires.

The `triggeredBy` field on these transitions is set to `'system'`.

Example:

```yaml
states:
  pending:
    description: Task is ready for implementation.
    agent: claude-code
    agent_timeout: 30m

  timed-out:
    description: Agent failed to complete within the time budget.
    gating: true
    instructions: |
      The agent timed out while working on this task. A human must decide:
      - Return to `pending` for another attempt
      - Cancel the task
      - Increase the timeout and retry

transitions:
  - from: pending
    to: timed-out
    description: Agent exceeded the time budget
    timeout: 30m
    on_enter: "cli:bash ./workflow.sh notify-timeout"

  - from: timed-out
    to: pending
    description: Human decided to retry

  - from: timed-out
    to: cancelled
    description: Human decided to abandon after timeout
```

The `timeout` field on the transition and `agent_timeout` on the state serve different roles:

| Field | Where | Purpose |
|-------|-------|---------|
| `agent_timeout` | State definition | How long the engine waits before killing the agent process |
| `timeout` | Transition definition | Which transition fires when a timeout occurs (existing field, now also used by agent mode) |

When `agent_timeout` is set on a state but no transition with `timeout` exists from that state, the agent is killed but the task remains in its current state with a warning logged.

### Timeout Callbacks

Timeout transitions support the same `on_leave` and `on_enter` callbacks as any other transition. This enables notification, logging, or cleanup on timeout:

```yaml
transitions:
  - from: pending
    to: timed-out
    description: Agent exceeded time budget
    timeout: 30m
    on_leave: "cli:bash ./workflow.sh save-partial-work"
    on_enter: "cli:bash ./workflow.sh notify-timeout"
```

The callback receives a `TransitionContext` with `triggeredBy: 'system'` and the timeout duration in `transitionData.timeout`.

## Log Capture

All agent stdout and stderr are captured to log files in the `runtime/logs/` directory relative to the workspace or plan root.

### Log File Naming

| Scenario | Log file path |
|----------|---------------|
| Simple state | `runtime/logs/task-{task_id}-{state}.log` |
| Counted-loop state | `runtime/logs/task-{task_id}-{state}-{visit_count}.log` |
| Model-specific state | `runtime/logs/task-{task_id}-{state}-{model}.log` |
| Both visits and model | `runtime/logs/task-{task_id}-{state}-{model}-{visit_count}.log` |

### Log Format

Each log file contains:

```
=== rhei agent log v1 ===
agent: claude-code
model: claude-sonnet-4-6
task: 3
state: pending
started: 2026-04-20T10:30:00Z
timeout: 30m
plan: /home/user/project/plan.rhei.md
===

<raw agent stdout and stderr, interleaved>

=== exit ===
code: 0
duration: 4m23s
ended: 2026-04-20T10:34:23Z
===
```

The header and footer are added by `rhei run`. The `v1` suffix is the log format version — increment it when the header/footer structure changes. The body is the raw, unmodified output of the agent process.

### Log Directory

`runtime/logs/` is created automatically by `rhei run` if it does not exist. `rhei reset` removes the entire `runtime/` directory, including logs.

## Dry-Run Output

`rhei run --dry-run` in agent mode shows what would be spawned without executing:

```
Pass 1: 2 ready, 0 terminal, 5 total.

Would spawn: claude -p "<prompt...>" --model claude-sonnet-4-6
  Task 1: Set up database schema [draft -> pending]
  Agent: claude-code, Model: claude-sonnet-4-6, Timeout: 30m
  Log: runtime/logs/task-1-pending.log

Would spawn: claude -p "<prompt...>" --model claude-sonnet-4-6
  Task 3: Write frontend components [draft -> pending]
  Agent: claude-code, Model: claude-sonnet-4-6, Timeout: 30m
  Log: runtime/logs/task-3-pending.log

Dry run complete - no agents were spawned.
```

## `rhei run --no-agent` — Callback-Only Mode

When `--no-agent` is passed, `rhei run` reverts to pre-agent behavior: it advances tasks through the state machine using transition callbacks only, without spawning any agent processes. This is the existing behavior for backward compatibility.

## Related Specifications

- [States Specification](rhei-states.spec.md) — state machine format, `agent` field, template variables
- [Program States Specification](rhei-programs.spec.md) — deterministic program execution (the algorithmic complement to agent states)
- [Transitions Specification](rhei-transitions.spec.md) — transition callbacks, timeout transitions, exit-code transitions
- [How Rhei Is Used](rhei-usage.spec.md) — roles, coordination patterns, agent workflows
- [Next Command](rhei-next.spec.md) — `rhei next` behavioral contract
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
- [Install Skills](rhei-install-skills.spec.md) — `rhei install-skills` for agent integration
