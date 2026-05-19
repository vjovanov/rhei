# FS-rhei-agents: Rhei Agents Specification

This document specifies how Rhei integrates with coding agents — the CLI tools that execute work on tasks. It covers agent configuration, resolution order, invocation profiles, prompt composition, parallel execution, timeout handling, and log capture.

For the state machine format see [States Specification](rhei-states.spec.md). For transition callbacks see [Transitions Specification](rhei-transitions.spec.md).

## Overview

Rhei can spawn coding agents directly from `rhei run`. Instead of requiring hand-written `workflow.sh` callback scripts, the run command resolves an agent for each task, composes a prompt from the state machine instructions, and spawns the agent as a subprocess. The spawned agent does the work for the current state, writes any required artifacts, and exits. `rhei run` remains the transition authority: after the subprocess exits, the engine evaluates the declared forward transitions and performs the state change itself. Callbacks still fire on transitions — agents and callbacks are complementary.

## 1. Agent Configuration

Rhei settings separate **model identity** from **agent transport** and
**tooling**:

- A **model profile** is the semantic identity used by state machines, callbacks,
  templates, logs, and multi-model execution.
- An **agent profile** is the CLI transport used when `rhei run` spawns an
  autonomous coding agent.
- A model profile names the concrete provider/model pair and may define
  agent-specific launch overrides for autonomous execution.
- An **MCP server profile** describes a Model Context Protocol server the agent
  can connect to for tool access during a state.
- A **skill profile** describes a reusable agent-side prompt/resource bundle
  enabled for a state.

This separation keeps callback-only workflows model-centric while still letting
`rhei run` resolve the exact subprocess invocation, tool surface, and skill
bundle when agent mode is enabled.

### 1.1. Global and Project Settings

Files:

- Global: `~/.config/rhei/settings.json`
- Project: `.rhei/settings.json` in the workspace or plan directory

Both files use the same schema. Project settings compose with global settings by
key rather than replacing the whole file.

```json
{
  "defaults": {
    "model": "impl-fast",
    "agent": null,
    "agent_mode": "yolo",
    "agent_timeout": "30m",
    "program_timeout": "10m",
    "mcp_servers": [],
    "skills": []
  },
  "agents": {
    "claude-code": {
      "command": ["claude"],
      "prompt_flag": "-p",
      "model_flag": "--model",
      "mcp_config_flag": "--mcp-config",
      "skill_flag": "--skill",
      "stdin_prompt": false,
      "modes": {
        "yolo":   ["--permission-mode", "bypassPermissions"],
        "safe":   ["--permission-mode", "default"],
        "review": ["--permission-mode", "plan"]
      }
    },
    "codex": {
      "command": ["codex", "exec"],
      "model_flag": "--model",
      "mcp_flag": "--mcp",
      "stdin_prompt": true,
      "modes": {
        "yolo": ["--sandbox", "danger-full-access", "--skip-git-repo-check", "-a", "never"],
        "safe": ["--sandbox", "workspace-write"]
      }
    },
    "pi": {
      "command": ["pi"],
      "prompt_flag": "-p",
      "model_flag": "--model",
      "skill_flag": "--skill",
      "stdin_prompt": false
    }
  },
  "models": {
    "impl-fast": {
      "provider": "anthropic",
      "model": "claude-sonnet-4-6",
      "default_agent": "claude-code",
      "agents": {
        "claude-code": {
          "args": ["--permission-mode", "default"],
          "autonomous_args": ["--permission-mode", "bypassPermissions"]
        }
      }
    },
    "review-deep": {
      "provider": "openai",
      "model": "o3",
      "default_agent": "codex",
      "agents": {
        "codex": {
          "args": ["--sandbox", "workspace-write"],
          "autonomous_args": [
            "--sandbox",
            "danger-full-access",
            "--skip-git-repo-check"
          ]
        }
      }
    }
  },
  "mcp_servers": {
    "linear": {
      "command": ["npx", "-y", "@modelcontextprotocol/server-linear"],
      "env": { "LINEAR_WORKSPACE": "${LINEAR_WORKSPACE}" }
    },
    "postgres": {
      "command": ["mcp-postgres", "--readonly"],
      "env": { "DATABASE_URL": "${DATABASE_URL}" }
    },
    "grafana": {
      "url": "https://grafana.internal/mcp",
      "transport": "sse"
    }
  },
  "skills": {
    "security-review": { "path": "~/.claude/skills/security-review" },
    "test-authoring":  { "path": ".rhei/skills/test-authoring" }
  },
  "snapshots": {
    "cache_dir": ".rhei/cache/snapshots",
    "redactor": null
  }
}
```

#### 1.1.1. `defaults`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | string or null | No | Default model profile id |
| `agent` | string or null | No | Default agent id resolved against the `agents` registry. Inline agent objects are not accepted — define custom agents in `agents.<id>` and reference them here by id. |
| `agent_mode` | string or null | No | Default agent mode (named flag set) applied when a state does not set `agent_mode`. |
| `agent_timeout` | string or null | No | Default autonomous agent timeout |
| `program_timeout` | string or null | No | Default program timeout |
| `mcp_servers` | array | No | Default MCP server entries applied to every agent state. Entries are ids or inline definitions. See [MCP Servers](#114-mcp_servers). |
| `skills` | array | No | Default skill entries applied to every agent state. Entries are ids or inline definitions. See [Skills](#115-skills). |

#### 1.1.2. `agents`

`agents` is a registry of agent transport profiles keyed by agent id. States
and defaults reference agents by id — inline agent definitions are not
permitted on a state or on `defaults.agent`. The registry is the only place an
agent's `command`, flags, and modes are declared.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `command` | string array | Yes | Base command and fixed arguments |
| `prompt_flag` | string | No | Flag to pass the prompt (e.g., `--prompt`, `-p`). Omit if using stdin. |
| `model_flag` | string | No | Flag to pass the concrete provider model name. Omit if the agent doesn't support model selection. |
| `stdin_prompt` | boolean | No | When `true`, the prompt is piped to stdin instead of passed via flag. Default: `false`. |
| `timeout` | string | No | Default timeout for this agent (e.g., `30m`). Overridden by state-level `agent_timeout`. |
| `mcp_flag` | string | No | Flag used to attach one MCP server per occurrence. `rhei run` emits the flag once per resolved server with a launch spec as its value. Mutually exclusive with `mcp_config_flag`. |
| `mcp_config_flag` | string | No | Flag used to attach a generated MCP config file. `rhei run` writes the resolved set to a temporary JSON file and passes it with this flag once. Mutually exclusive with `mcp_flag`. |
| `skill_flag` | string | No | Flag used to enable one skill per occurrence. `rhei run` emits the flag once per resolved skill id. Omit to declare the agent does not support skills. |
| `modes` | object | No | Named flag sets, keyed by mode name. Values are ordered string arrays appended to the command at spawn time. See [Modes](#22-modes). |
| `session` | object | No | Optional `CustomAgentProfile.session` block describing snapshot resume, fork, interactive continuation, and transcript layout capabilities. The authoritative schema is [Snapshots Specification — CustomAgentProfile.session](rhei-snapshots.spec.md#91-customagentprofilesession). |

Built-in agent ids (see [Known Agent Profiles](#2-known-agent-profiles)) are
preloaded as the default agents registry. A user entry with the same id in
global or project settings replaces the built-in entry wholesale — `command`,
flags, and `modes` are taken from the user entry without field-level merging.

#### 1.1.3. `models`

`models` is a registry of named model profiles keyed by model id.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `provider` | string | Yes | Provider identifier such as `openai` or `anthropic` |
| `model` | string | Yes | Concrete provider model name such as `o3` or `claude-sonnet-4-6` |
| `default_agent` | string | No | Preferred agent id when `rhei run` needs to spawn this model autonomously |
| `agents` | object | No | Per-agent launch overrides for this model, keyed by agent id |

Each `models.<id>.agents.<agent-id>` binding has this shape:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `args` | string array | No | Additional agent arguments for normal/inherited invocation |
| `autonomous_args` | string array | No | Arguments preferred by `rhei run` when launching autonomously |
| `timeout` | string | No | Timeout default specific to this model-agent binding |

#### 1.1.4. `mcp_servers`

`mcp_servers` is a registry of named MCP server profiles keyed by server id.
Entries describe how to launch or connect to a Model Context Protocol server
so it can be attached to an agent subprocess.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `command` | string array | One of `command` / `url` | Command and arguments to launch a local MCP server |
| `url` | string | One of `command` / `url` | URL of a remote MCP server. Requires `transport`. |
| `transport` | string | No | Transport for remote servers. Supported values: `sse`, `websocket`. Ignored for `command`-based servers. |
| `env` | object | No | Environment variables for the server process. Values may reference host environment with `${VAR}` syntax. Only meaningful for `command`-based servers. |
| `working_directory` | string | No | Working directory for the server process. Only meaningful for `command`-based servers. |
| `startup_timeout` | string | No | Maximum time to wait for the server to complete its MCP handshake after launch. Duration format (`30s`, `10s`, …). Default: `10s`. |

`command` and `url` are mutually exclusive. An entry must declare exactly one.

#### 1.1.5. `skills`

`skills` is a registry of named skill profiles keyed by skill id. Each entry
identifies an agent-side skill bundle that should be enabled for states that
list it.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Filesystem path to the skill bundle. Leading `~` expands to the user's home directory. |
| `description` | string | No | Human-readable description of the skill's purpose. |

Skills are agent-specific capabilities. A resolved skill is wired to the agent
only when the resolved agent profile declares a `skill_flag`; otherwise the
skill is skipped with a warning (see [Missing Tooling](#6-missing-tooling)). This
keeps state machines portable across agents that do not implement skills.

#### 1.1.6. `snapshots`

`snapshots` is an optional top-level settings block for session snapshot
storage, redaction, cache TTLs, and experimental adapter gates. This spec only
declares where the block lives in settings; field definitions and defaults are
authoritative in [Snapshot Operations Specification — Configuration](rhei-snapshot-operations.spec.md#4-configuration).

### 1.2. Per-State Settings

The `target` / `all_targets` fields are the preferred execution selectors on
state definitions in `states.yaml`. The legacy `model` / `all_models`,
optional `agent`, `mcp_servers`, and `skills` fields remain supported for
compatibility. See
[States Specification — Agent Field](rhei-states.spec.md#5-agent-field) and
[States Specification — MCP Servers and Skills](rhei-states.spec.md#7-mcp-servers-and-skills).

### 1.3. Merge Semantics

Built-in agents load first, global settings compose over them, then project
settings compose over the result:

- `defaults` shallow-override by field.
- `agents` merge by agent id. A user entry with the same id as a built-in or a
  global entry **replaces that entry wholesale** — there is no field-level
  merge within a single agent entry. To tweak just one mode of a built-in
  agent, redeclare the whole entry.
- `models` merge by model id.
- `models.<id>.agents` merge by agent id.
- `mcp_servers` merge by server id.
- `skills` merge by skill id.
- `null` explicitly clears an inherited optional field.

`defaults.mcp_servers` and `defaults.skills` are replaced wholesale by the
project list when the project defines them, not concatenated. Use an empty
array (`"mcp_servers": []`) to clear inherited defaults, or repeat entries
explicitly to extend them. This keeps the precedence predictable — every
level's effective tooling default is the one written there.

This lets a project override only a model's concrete provider/model pair or
only a model-agent binding's autonomous arguments without redefining unrelated
global entries.

### 1.4. Resolution Order

When `rhei run` or a callback needs model context for a task in a given state,
it resolves the model id in this order:

1. **CLI override** — `--model <MODEL>`
2. **State-level** — `model` on the state definition in `states.yaml`
3. **Project defaults** — `.rhei/settings.json` `defaults.model`
4. **Global defaults** — `~/.config/rhei/settings.json` `defaults.model`

The resolved model id must exist in the merged `models` registry. If no model
is configured at any level, model-specific callback and template fields are
omitted.

When `rhei run` is launching an autonomous agent, it resolves the agent id in
this order:

1. **CLI override** — `--agent <AGENT>`
2. **State-level** — `agent` on the state definition in `states.yaml`
3. **Project defaults** — `.rhei/settings.json` `defaults.agent`
4. **Global defaults** — `~/.config/rhei/settings.json` `defaults.agent`
5. **Model default** — `models.<id>.default_agent`

The resolved id must match an entry in the merged `agents` registry
(built-ins → global → project). An id with no matching entry is a
configuration error:

```
error: agent 'my-agent' is not defined. Add an entry to agents.<id> in
.rhei/settings.json or ~/.config/rhei/settings.json, or reference one of the
built-in ids (claude-code, codex, cursor, gemini, kilocode).
```

If no agent is configured at any level and the resolved model does not declare
a `default_agent`, `rhei run` fails:

```
error: no agent configured for model 'impl-fast'.
Set defaults.agent, the state's agent, models.impl-fast.default_agent, or pass --agent <AGENT> to rhei run.
```

For a state that declares `all_targets`, this resolution is bypassed for the
fields encoded directly in each selector: the agent id, optional mode, optional
provider, and model name come from the selector itself. Validation must still
verify that the referenced agent exists and that any referenced mode exists on
that agent. For the legacy `all_models` form, agent resolution still runs
independently for each model-specific execution of the state through the normal
order above. When snapshots are enabled, each legacy `model` or `all_models`
execution must resolve an effective target tuple `(agent, mode?, provider,
model)` before snapshot emit or inherit can run; otherwise explicit snapshot
fields are rejected and auto-emit is skipped.

#### 1.4.1. Mode Resolution Order

When the resolved agent declares `modes`, `rhei run` picks one at spawn time:

1. **CLI override** — `--agent-mode <MODE>`
2. **State-level** — `agent_mode` on the state definition in `states.yaml`
3. **Project defaults** — `.rhei/settings.json` `defaults.agent_mode`
4. **Global defaults** — `~/.config/rhei/settings.json` `defaults.agent_mode`
5. **Registry default** — the first declared mode in the agent entry's
   `modes` map
6. **None** — if the agent entry has no `modes`, no mode flags are appended

The resolved mode name must be a key in the agent entry's `modes` map when
the map is non-empty. A missing mode is a spawn-time error.

When `rhei run` composes the tool surface for a state, it resolves the
effective MCP server and skill sets:

1. Start from `defaults.mcp_servers` (and `defaults.skills`) as resolved by the
   merged settings.
2. Union with the state's `mcp_servers` (and `skills`) list, if any. An empty
   list on the state clears the defaults for that state.
3. Deduplicate by id. Within a single effective set, later entries for the same
   id override earlier ones — a state-level override wins over a defaults
   entry.
4. Resolve each id against the merged `mcp_servers` / `skills` registry. Inline
   object entries on the state (or in `defaults`) do not require a registry
   entry; they are used as-is.
5. An id with no registry match and no inline definition is a validation
   error.

The resolved sets are distinct per state and per invocation. Changing the
current state or restarting `rhei run` recomputes them.

### 1.5. Partial Overrides

Each level can override model and agent independently:

```yaml
# states.yaml — state overrides only the model profile
states:
  agent-review:
    model: review-deep
```

```json
// .rhei/settings.json — project sets the default agent transport
{
  "defaults": {
    "agent": "codex"
  }
}
```

Result for `agent-review`: model=`review-deep` (from state), agent=`codex`
(from project defaults unless `review-deep.default_agent` or a CLI override
supersedes it).

## 2. Known Agent Profiles

Rhei ships with built-in invocation profiles for known coding agents. Each
profile defines how to spawn the agent, deliver the prompt, pass the concrete
provider model name, and set transport-level defaults. Each built-in also
declares a `yolo` mode carrying the autonomous/dangerous flag set that was
historically the agent's default.

| Agent ID | Binary | Prompt Delivery | Model Flag | MCP Wiring | Skill Wiring | `yolo` Mode Flags |
|----------|--------|-----------------|------------|------------|--------------|-------------------|
| `claude-code` | `claude` | `-p <prompt>` | `--model <m>` | `--mcp-config <path>` | `--skill <id>` | `--permission-mode bypassPermissions` |
| `codex` | `codex exec` | `--` (stdin) | `--model <m>` | `--mcp <spec>` (per server) | unsupported | `--sandbox danger-full-access --skip-git-repo-check` |
| `gemini` | `gemini` | `--prompt <prompt>` | `--model <m>` | unsupported | unsupported | `--approval-mode yolo` |
| `cursor` | `cursor-agent` | `--print <prompt>` | `--model <m>` | unsupported | unsupported | `--force` |
| `kilocode` | `kilo` | positional via `--auto <prompt>` | `--model <m>` | unsupported | unsupported | `--yolo` |
| `pi` | `pi` | `-p <prompt>` | `--model <m>` | unsupported | `--skill <path>` | (no modes — pi has no permission layer; isolate at the sandbox/container level) |

The agent IDs match those used by `rhei install-skills --agent`.

Snapshot session support is tracked separately from prompt/model invocation.
In v1, built-in `cursor` and `kilocode` profiles do not expose a supported
`CustomAgentProfile.session`; auto-emit is skipped and explicit snapshot
operations fail with `unsupported-snapshot-session`. Gemini snapshot support is
provisional and remains unsupported until the snapshot adapter spike resolves
its resume and path layout.

Agents marked `unsupported` for MCP or skills receive a warning at spawn time
when the resolved state's effective set includes entries they cannot consume.
Required MCP entries (see [Missing Tooling](#6-missing-tooling)) escalate the
warning to an error.

A user-written entry for one of these ids in `settings.json` replaces the
built-in entry wholesale (see [Merge Semantics](#13-merge-semantics)).

### 2.1. Custom Agents

When the built-in profiles don't fit, declare a new agent in the `agents`
registry — **never inline** on a state or on `defaults.agent`:

```json
{
  "agents": {
    "my-agent": {
      "command": ["my-agent", "--autonomous"],
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "stdin_prompt": false,
      "modes": {
        "yolo": ["--permissions", "full"],
        "safe": ["--permissions", "read-only"]
      }
    }
  }
}
```

States and defaults then reference the agent by id:

```yaml
states:
  pending:
    agent: my-agent
    agent_mode: yolo
```

```json
{
  "defaults": {
    "agent": "my-agent",
    "agent_mode": "safe"
  }
}
```

Inline agent objects are rejected by the validator. This keeps state
machines portable — the transport surface lives in `settings.json` where a
project or a user can replace it without touching `states.yaml`.

### 2.2. Modes

A mode is a named ordered list of extra CLI flags. When `rhei run` spawns the
agent, the resolved mode's flags are appended right after the base
`command`, before the prompt and model flags. The full flag order is:

```
<command...> <mode flags...> <prompt_flag> <prompt>? <model_flag> <model>? <mcp/skill flags...>
```

(When `stdin_prompt` is `true`, the prompt is written to stdin instead of
being passed via `prompt_flag`, and `--` is appended after the model flag so
agents like `codex exec --` get a clean positional-arg separator.)

Mode names are free-form. `yolo` is a widely-used convention for
"autonomous, dangerous posture"; `safe`, `review`, `plan`, and `audit` are
other common choices. Rhei does not interpret mode names — it only injects
the named flag list.

An agent entry may declare zero, one, or many modes. When no modes are
declared, no mode flags are appended and `agent_mode` must not be set for
states that use this agent. See
[Mode Resolution Order](#141-mode-resolution-order) for how a mode is selected.

## 3. Prompt Composition

When `rhei run` spawns an agent for a task, it composes a prompt from the state machine definition and the task content. The prompt has this structure:

```
# Task {task_id}: {task_title}

## State: {state}

{resolved personality, if present}

## Instructions

{resolved instructions from state definition}

## Task Content

{task body from the plan, including any child task nodes}

## Rhei Commands

You are working in a rhei-managed plan at `{plan_path}`.
The `rhei run` process that spawned you is responsible for advancing the task after you exit successfully.
Do not run `rhei transition` or `rhei complete` from this spawned agent process unless the workflow explicitly instructs you to launch a nested or delegated execution that manages its own state independently.

Available transitions from `{state}`:
{list of declared transitions from current state, with descriptions}
```

The prompt carries domain instructions only. It does not contain completion
prose such as "create every required output artifact and then exit":
completion is enforced by the state's [Completion Condition](#32-completion-condition),
not by prompt wording. Required artifact paths are already visible to the agent
via resolved `{output.<name>.path}` variables in the state's `instructions`,
and every supported agent exits deterministically after one turn in its native
headless mode.

Template variables (`{task_id}`, `{model}`, `{model.provider}`,
`{model.name}`, `{visit_count}`, etc.) are resolved before the prompt is sent,
using the same resolution rules as `rhei next`. See [Template Variables](rhei-states.spec.md#4-template-variables-in-instructions-and-personality).

The prompt is delivered to the agent via its configured prompt delivery mechanism (flag or stdin).

### 3.1. Completion Authority

Every state has a **completion authority** — the role that decides when the
state's work is done and drives the resulting transition. Rhei defines two
authorities; exactly one applies to any given execution of a state.

| Authority | Applies when | Transition driver |
|-----------|--------------|-------------------|
| `worker` | The invoking role is a manual worker (human, `rhei-plan-worker` skill session, or direct `rhei next` / `rhei transition` / `rhei complete` caller). | The worker calls `rhei transition` or `rhei complete`. |
| `orchestrator` | `rhei run` has spawned the agent or program for this state. | `rhei run` evaluates the state's [Completion Condition](#32-completion-condition), then selects and executes the matching forward transition. |

Completion authority is determined by the execution mode, not declared on the
state. The same state definition is legal under both authorities. This is what
lets a plan be driven either agent-by-agent (manual workers) or end-to-end
(`rhei run`) without rewriting `states.yaml`.

Normative rules:

- Under `worker` authority, the worker owns both the work and the transition.
  `rhei run` is not involved.
- Under `orchestrator` authority, the spawned subprocess owns the work;
  `rhei run` owns the transition. The subprocess must not call
  `rhei transition` or `rhei complete`, and must not edit `**State:**` lines
  directly. The one exception is a nested execution started from within the
  agent, which manages its own state independently of the outer `rhei run`.
- `instructions` and `personality` describe domain work only. They must not
  describe how or when to stop, whether to call transition commands, or how
  completion is detected. Those are properties of the execution model, not of
  the state.
- Gating states (`gating: true`) bypass completion authority: no subprocess is
  spawned and no automatic transition fires.

This separation keeps state transitions serialized through one orchestrator
even when many agents run in parallel.

### 3.2. Completion Condition

When completion authority is `orchestrator`, `rhei run` decides deterministically
when the state's work is complete. The completion condition is a property of
the state machine and the execution mode, not of the prompt — so the same
determinism applies to every execution of that state regardless of which agent
is resolved.

The condition is normative and universal for agent states:

1. The subprocess exits with code `0`, **and**
2. Every required artifact declared in the state's `outputs:` list exists on disk.

Both are evaluated after the process exits. If the state declares no `outputs:`,
condition (2) is vacuously true and exit alone suffices.

This contract maps 1:1 onto the native headless mode of every supported agent.
All six built-ins — `claude-code -p`, `codex exec`, `gemini --prompt --yolo`,
`cursor-agent --print --force`, `kilo --auto --yolo`, and `pi -p` — run one
turn-loop and exit. Rhei detects completion from process exit plus declared
output artifacts; no cross-agent stop signal, sentinel file, or "done" RPC is
defined or needed.

Program states have their own exit-code-driven completion semantics documented
in [Program States Specification](rhei-programs.spec.md#3-exit-code-transitions);
the `outputs:` clause of the condition above does not apply to them unless the
state also declares `outputs:`.

#### 3.2.1. Runtime Semantics

Under `orchestrator` authority, `rhei run`:

1. Spawns the subprocess and waits on `(subprocess exit) OR (timeout fires)`.
2. On timeout, sends `SIGTERM` to the subprocess, 10 s grace, then `SIGKILL`.
   Timeout transitions fire per [Timeout Handling](#7-timeout-handling). Agents
   that fork long-running descendants should install their own cleanup; the
   engine kills only the direct subprocess.
3. On non-zero exit, routes through the exit-code / error transition path
   documented in the [Execution Loop](#52-execution-loop). The artifact check is
   skipped.
4. On exit code `0`:
   - Verify every required output artifact exists.
   - If any is missing, the task stays in its current state and the engine
     logs `warning: agent exited 0 but required outputs are missing for task
     {id} in state '{state}': <name1>, <name2>`. No transition fires.
   - Otherwise, evaluate forward transitions in normal selection order and
     execute the first match.

#### 3.2.2. Timeout Requirement

Under `orchestrator` authority, a timeout must resolve to a finite value
through the chain documented in [Timeout Handling](#7-timeout-handling). States
that resolve to no timeout at any level are a validation error under
`orchestrator` authority. Under `worker` authority there is no timeout
enforcement.

This closes the one remaining non-determinism in the completion contract: an
agent that hangs without producing outputs is bounded by the timeout and
routed to the state's timeout transition (or fails the task with a warning
when no timeout transition is declared).

## 4. Environment Variables

The agent subprocess inherits these environment variables, consistent with the
callback environment:

| Variable | Value |
|----------|-------|
| `RHEI_PLAN_PATH` | Absolute path to the plan file or workspace directory |
| `RHEI_TASK_ID` | Current task identifier |
| `RHEI_STATE` | Current state name |
| `RHEI_MODEL` | Model profile id, if configured |
| `RHEI_MODEL_PROVIDER` | Resolved provider id, if configured |
| `RHEI_MODEL_NAME` | Resolved provider model name, if configured |
| `RHEI_AGENT` | Agent identifier, if configured |
| `RHEI_MCP_SERVERS` | Comma-separated list of resolved MCP server ids that started successfully. Empty when none are attached. |
| `RHEI_MCP_<NAME>_AVAILABLE` | `true` or `false` for each declared MCP server in the state's effective set. `<NAME>` is the server id uppercased with hyphens and spaces replaced by underscores. |
| `RHEI_SKILLS` | Comma-separated list of resolved skill ids enabled for this state. Empty when none are attached. |
| `RHEI_SKILL_<ID>_AVAILABLE` | `true` or `false` for each declared skill in the state's effective set. `<ID>` follows the same transformation as MCP names. |

The agent's working directory is set to the workspace root (for directory workspaces) or the plan file's parent directory (for single-file plans).

## 5. `rhei run` — Agent Mode

### 5.1. CLI

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
| `--agent <AGENT>` | | Override the agent transport for this run |
| `--model <MODEL>` | | Override the model profile id for this run |
| `--continue-on-error` | false | Continue to the next task when an agent or program exits non-zero |
| `--parallel <N>` | 1 | Maximum number of agents/programs to run concurrently. `0` means unlimited. |
| `--program-timeout <DURATION>` | | Override program timeout for this run (e.g., `10m`, `1h`). See [Program States Specification](rhei-programs.spec.md#4-timeout-handling). |

### 5.2. Execution Loop

#### 5.2.1. Sequential Mode (default, `--parallel 1`)

1. Load plan and state machine. Validate.
2. Find the next claimable task (same eligibility as `rhei next`).
3. Resolve the model and, if agent mode is enabled, the agent for the task's current state (resolution order above).
4. If agent mode is enabled and no agent is configured, fail with an error.
5. Compose the prompt (see [Prompt Composition](#3-prompt-composition)).
6. Log the spawn to `runtime/logs/task-{task_id}-{state}[-{visit_count}].log`.
7. Spawn the agent CLI as a subprocess with the composed prompt.
8. Wait for the agent process to exit (subject to timeout — see [Timeout Handling](#7-timeout-handling)).
9. Re-read the plan. If some external actor changed the task's state while the agent was running, respect that authoritative plan state and continue the loop from there.
10. Otherwise, if the agent exited `0`, evaluate the current state's declared forward transitions in normal transition-selection order. If one transition matches, `rhei run` executes it and logs the resulting state change.
11. If the agent exited `0` and no forward transition matches, log a warning: `warning: agent exited 0 but task {id} did not advance from '{state}'`. Continue to the next task.
12. If the agent exited non-zero:
    - Without `--continue-on-error`: log the error and stop.
    - With `--continue-on-error`: log the error, skip this task, continue.
13. Repeat until no claimable tasks remain or all tasks are terminal.

#### 5.2.2. Parallel Mode (`--parallel N` where N > 1 or N = 0)

1. Load plan and state machine. Validate.
2. Find all claimable tasks (same eligibility as `rhei next`, but collect all candidates).
3. Select up to N tasks that are mutually independent (no dependency edges between them). When N = 0, select all independent claimable tasks.
4. For each selected task, resolve the model and agent, compose the prompt, and spawn the agent subprocess concurrently. Each agent writes to its own log file and is treated as a worker for the task's current state, not as a transition authority.
5. Wait for any agent to exit (timeout or completion).
6. When an agent exits:
   a. Re-read the plan.
   b. Process the result using the same rules as sequential mode: if an external actor already changed the task state, respect it; otherwise, on exit `0`, let `rhei run` evaluate and execute the next matching forward transition; on non-zero exit, apply the error path.
   c. Scan for newly claimable tasks (dependencies may have been unblocked).
   d. If new tasks are claimable and the pool is below N, spawn agents for them.
7. Repeat until no claimable tasks remain or all tasks are terminal.

**Independence rule:** Two tasks are independent when neither appears in the other's transitive `**Prior:**` chain. The engine must not spawn two agents that could produce conflicting edits to the same task file. For directory workspaces, each task lives in a separate file, so file conflicts are avoided by construction.

**Single-file plans:** Parallel mode is limited to `--parallel 1` for single-file plans because agents could produce conflicting edits to the same file. `rhei run` prints a warning if `--parallel` > 1 is requested with a single-file plan and falls back to sequential execution.

### 5.3. Interaction Between Agents and Callbacks

Agents and callbacks are complementary, not exclusive:

- **Agent** does the work of the current state (coding, reviewing, fixing, writing artifacts).
- **`rhei run`** evaluates success or failure and performs the state transition.
- **Callbacks** handle side effects of that transition (creating artifacts, spawning tasks, notifying systems).

When `rhei run` is in agent mode:
1. The agent is spawned for the current state.
2. The agent performs work and exits.
3. `rhei run` selects and executes the transition.
4. `on_leave` / `on_enter` callbacks fire as part of that engine-driven transition.

`--no-callbacks` suppresses callbacks but not agent or program spawning. `--no-agent` suppresses agent spawning but not program spawning or callbacks. `--no-program` suppresses program spawning but not agent spawning or callbacks. All three can be combined independently.

### 5.4. Gating States

When a task reaches a gating state (`gating: true`), `rhei run` does not spawn an agent. Instead it logs:

```
Task {id} is in gating state '{state}'. Waiting for human action.
```

The task is skipped and the engine continues with other claimable tasks. When the human transitions the task out of the gating state (via `rhei transition`), the next run pass picks it up.

## 6. Missing Tooling

Tooling — MCP servers and skills — can fail to be available for reasons that
only surface at spawn time: a binary is not on `PATH`, a remote MCP URL is
unreachable, an env var is unset, or a skill path does not exist. Rhei
distinguishes three failure classes:

| Failure | When detected | Default behavior |
|---------|---------------|------------------|
| Id referenced in a state but not in the merged registry and not an inline entry | `rhei validate` / settings load | Hard error. Same class as a dangling `model:` or `agent:` reference. |
| Registry entry exists but the MCP server cannot start, URL is unreachable, the handshake times out, or a skill path does not exist | Agent spawn | Depends on the entry's `optional` flag (see below). |
| MCP server starts but crashes mid-session | During agent execution | The agent surfaces the protocol error; Rhei does not intervene. The failure appears in the agent log. |

An MCP server or skill is considered **available** when:

- For `command`-based MCP servers: the subprocess launched, the MCP handshake
  completed, and the server remained alive, all within its `startup_timeout`
  (default `10s`).
- For `url`-based MCP servers: the transport-level connection completed and
  the MCP handshake succeeded within `startup_timeout`.
- For skills: the configured `path` exists and is readable.

### 6.1. Required vs optional entries

Per-state `mcp_servers` and `skills` entries may be declared required (the
default) or `optional: true`. The `defaults` lists in `settings.json` follow
the same rules.

- **Required (default):** if the entry fails its availability check,
  `rhei run` does not spawn the agent. The engine looks for an
  `mcp_unavailable` or `skill_unavailable` transition from the current state
  in `states.yaml`:
  - If one is declared, it fires with `triggeredBy: 'system'`. The callback
    receives the unavailable ids in `transitionData.unavailable`.
  - Otherwise the task stays in its current state and the engine logs
    `error: required tooling unavailable for task {id} in state '{state}':
    <id1>, <id2>`. `--continue-on-error` behaves the same way it does for
    agent exits: the engine logs and skips the task.
- **Optional (`optional: true`):** if the entry fails, `rhei run` logs a
  warning and spawns the agent with the remaining resolved tooling. The
  prompt's template variables and env vars reflect availability so
  instructions and callbacks can branch. Missing optional skills are always
  non-fatal and emit the same warning regardless of agent support.

An unsupported agent (one whose profile declares no `mcp_flag` /
`mcp_config_flag` for a required MCP entry, or no `skill_flag` for a required
skill entry) is treated identically to an availability failure: required
entries escalate to the `*_unavailable` path; optional entries produce a
warning and are dropped.

See [Transitions Specification](rhei-transitions.spec.md) for declaring
`mcp_unavailable` / `skill_unavailable` transitions and
[States Specification — Template Variables](rhei-states.spec.md#4-template-variables-in-instructions-and-personality)
for `{mcp.<name>.available}` and `{skill.<id>.available}` in prompts.

## 7. Timeout Handling

### 7.1. Configuration

Timeout can be set at four levels:

1. **Per-state** — `agent_timeout` field on a state definition:
   ```yaml
   states:
     pending:
       agent_timeout: 30m
   ```

2. **Per-model/agent binding** — `timeout` field in a model profile's `agents` binding:
   ```json
   {
     "models": {
       "impl-fast": {
         "provider": "anthropic",
         "model": "claude-sonnet-4-6",
         "agents": {
           "claude-code": { "timeout": "45m" }
         }
       }
     }
   }
   ```

3. **Per-agent profile** — `timeout` field on an entry in the `agents`
   registry:
   ```json
   {
     "agents": {
       "my-agent": { "command": ["my-agent"], "timeout": "1h" }
     }
   }
   ```

4. **Defaults** — `defaults.agent_timeout` in settings:
   ```json
   { "defaults": { "agent_timeout": "30m" } }
   ```

Resolution: state-level > model-agent binding > agent-profile > settings defaults.

Under `orchestrator` [Completion Authority](#31-completion-authority), a timeout
must resolve to a finite value at some level of the chain; missing timeouts on
orchestrator-driven states are a validation error. Under `worker` authority
the resolution is optional and the engine does not impose a timeout on manual
work.

### 7.2. Duration Format

Durations use a human-readable format: `30s`, `5m`, `1h`, `2h30m`. Supported units:

| Unit | Suffix | Example |
|------|--------|---------|
| Seconds | `s` | `30s` |
| Minutes | `m` | `5m` |
| Hours | `h` | `1h` |

Units can be combined: `1h30m`, `2h15m30s`.

### 7.3. Timeout Behavior

When an agent process exceeds its timeout:

1. `rhei run` sends `SIGTERM` to the agent process.
2. After a 10-second grace period, if the process has not exited, send `SIGKILL`.
3. Log to the task log: `agent timed out after {duration}`.
4. Look for a timeout transition from the current state in the state machine.
5. If a timeout transition exists, fire it (with its `on_leave`/`on_enter` callbacks).
6. If no timeout transition exists, the task remains in its current state and the engine logs a warning.

When snapshot capture is enabled, a timeout may still produce an auto-emitted
transcript for operator inspection, but that snapshot is classified
`completion: timeout` and is not preloadable by authored `snapshot.inherit:`.
See [Snapshots Specification — Compatibility Predicates](rhei-snapshots.spec.md#5-compatibility-predicates).

### 7.4. Timeout Transitions

Timeout transitions are declared in the `transitions` array with the `timeout` field. The existing `timeout` field in the transition schema (see [Transitions Specification](rhei-transitions.spec.md#44-transition-definition)) is used by `rhei run` to determine what to do when an agent times out.

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
    model: impl-fast
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

### 7.5. Timeout Callbacks

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

## 8. Log Capture

All agent stdout and stderr are captured to log files in the `runtime/logs/` directory relative to the workspace or plan root.

### 8.1. Log File Naming

| Scenario | Log file path |
|----------|---------------|
| Simple state | `runtime/logs/task-{task_id}-{state}.log` |
| Counted-loop state | `runtime/logs/task-{task_id}-{state}-{visit_count}.log` |
| Model-specific state | `runtime/logs/task-{task_id}-{state}-{model}.log` |
| Both visits and model | `runtime/logs/task-{task_id}-{state}-{model}-{visit_count}.log` |

### 8.2. Log Format

Each log file contains:

```
=== rhei agent log v1 ===
agent: claude-code
model: impl-fast
provider: anthropic
model_name: claude-sonnet-4-6
task: 3
state: pending
started: 2026-04-20T10:30:00Z
timeout: 30m
plan: /home/user/project/plan.rhei.md
mcp_servers: postgres,grafana?
skills: test-authoring
===

<raw agent stdout and stderr, interleaved>

=== exit ===
code: 0
duration: 4m23s
ended: 2026-04-20T10:34:23Z
===
```

The header and footer are added by `rhei run`. The `v1` suffix is the log format version — increment it when the header/footer structure changes. The body is the raw, unmodified output of the agent process.

Each entry in `mcp_servers:` and `skills:` is the resolved id; an entry
suffixed with `?` was declared `optional: true` and failed its availability
check — it was dropped before spawn and is recorded for diagnostics. A
missing line means the state declared no entries of that kind.

### 8.3. Log Directory

`runtime/logs/` is created automatically by `rhei run` if it does not exist. `rhei reset` removes the entire `runtime/` directory, including logs.

## 9. Dry-Run Output

`rhei run --dry-run` in agent mode shows what would be spawned without executing:

```
Pass 1: 2 ready, 0 terminal, 5 total.

Would spawn: claude -p "<prompt...>" --model claude-sonnet-4-6
  Task 1: Set up database schema [draft -> pending]
  Agent: claude-code, Model: impl-fast (anthropic/claude-sonnet-4-6), Timeout: 30m
  Log: runtime/logs/task-1-pending.log

Would spawn: claude -p "<prompt...>" --model claude-sonnet-4-6
  Task 3: Write frontend components [draft -> pending]
  Agent: claude-code, Model: impl-fast (anthropic/claude-sonnet-4-6), Timeout: 30m
  Log: runtime/logs/task-3-pending.log

Dry run complete - no agents were spawned.
```

## 10. `rhei run --no-agent` — Callback-Only Mode

When `--no-agent` is passed, `rhei run` reverts to pre-agent behavior: it advances tasks through the state machine using transition callbacks only, without spawning any agent processes. This is the existing behavior for backward compatibility.

## Related Specifications

- [States Specification](rhei-states.spec.md) — state machine format, `model`/`agent` fields, template variables
- [Program States Specification](rhei-programs.spec.md) — deterministic program execution (the algorithmic complement to agent states)
- [Transitions Specification](rhei-transitions.spec.md) — transition callbacks, timeout transitions, exit-code transitions
- [How Rhei Is Used](rhei-usage.spec.md) — roles, coordination patterns, agent workflows
- [Next Command](rhei-next.spec.md) — `rhei next` behavioral contract
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
- [Install Skills](rhei-install-skills.spec.md) — `rhei install-skills` for agent integration
