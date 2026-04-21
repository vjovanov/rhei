# 0003 - Agents must be defined in a registry with named modes

## Status

proposed

## Context

Today, a coding-agent profile (command, prompt flag, model flag, stdin
behavior, MCP/skill wiring) can appear in three places:

1. The `agents` registry in `settings.json` — specified in
   [rhei-agents.spec.md](../specs/rhei-agents.spec.md) but **not implemented**
   in `RheiSettings` today.
2. `defaults.agent` in `settings.json` — string id **or** inline object.
3. The `agent:` field on a state in `states.yaml` — string id **or** inline
   object (via `AgentConfig::Custom(CustomAgentProfile)`).

The inline form makes agent definitions redundant across states, leaks
transport details into workflow definitions, and forces templates to expose
a transport-shaped surface area at instantiation time. The current
`multi-model-analysis` template is the worst case: it parametrizes
`gemini_command`, `gemini_command_arg_1`, `gemini_command_arg_2`,
`gemini_prompt_flag`, and `gemini_model_flag` purely to build an inline agent
object inside `states.yaml`.

A second gap: the built-in profiles hard-code one set of "autonomous" flags
each (`claude-code` → `--permission-mode bypassPermissions`; `codex` →
`--sandbox danger-full-access --skip-git-repo-check`). There is no way for a
state to ask for a different permission or sandbox posture without redefining
the whole agent. Users need to switch between e.g. a yolo posture for
implementation states and a review-only posture for review states using the
same underlying CLI.

## Decision

Agents are defined **only** in the `agents` registry in `settings.json`
(global or project). `states.yaml` and `defaults.agent` reference agents by
string id. Each agent entry declares a set of named **modes**, each of which
is an ordered list of extra flags appended at spawn time.

### 1. Agent registry schema

```json
{
  "agents": {
    "<agent-id>": {
      "command": ["<binary>", "<fixed args>"],
      "prompt_flag": "-p",
      "model_flag": "--model",
      "stdin_prompt": false,
      "timeout": "30m",
      "mcp_flag": null,
      "mcp_config_flag": "--mcp-config",
      "skill_flag": "--skill",
      "modes": {
        "yolo":   ["--permission-mode", "bypassPermissions"],
        "safe":   ["--permission-mode", "default"],
        "review": ["--permission-mode", "plan"]
      }
    }
  }
}
```

- The registry key is the agent id. There is no `id:` field inside the value.
- `command`, `prompt_flag`, `model_flag`, `stdin_prompt`, `mcp_flag`,
  `mcp_config_flag`, `skill_flag`, and `timeout` keep their current meaning
  (see [rhei-agents.spec.md — Custom Agent Profiles]).
- `modes` is an optional object mapping mode name → flag list. A well-known
  mode name `yolo` is a convention, not a reserved word: Rhei does not
  interpret the name, it only injects the flag list. Authors are free to add
  `safe`, `review`, `plan`, `audit`, or any domain-specific mode.
- Omitting `modes` is equivalent to declaring a single implicit mode `default`
  with no extra flags.

### 2. Built-in agents ship as the default global layer

The built-in profiles (`claude-code`, `codex`, `aider`, `kilocode`, `cursor`)
are materialized as a default global `agents` registry at load time, each
with a `yolo` mode carrying the flags that are currently their hard-coded
defaults. A user-written entry with the same id in global **or** project
settings replaces the built-in entry **wholesale** — there is no per-field
merge within a single agent entry, to keep the transport surface predictable.

### 3. Config layers

Two files, same schema:

- Global: `~/.config/rhei/settings.json`
- Project: `<plan-root>/.rhei/settings.json`

Project wins per agent id. Merging:

- The `agents` map merges by id: project ids add to or replace global ids.
- An agent entry is replaced wholesale when its id appears in a higher layer
  (built-ins → global → project). This is deliberately coarser than the
  existing `mcp_servers` / `skills` registries, because an agent is one
  cohesive invocation profile — partial overrides of just `prompt_flag` or
  just one mode would surprise more than they would help.

### 4. States and defaults

`defaults.agent` and a state's `agent:` are **string-valued only**:

```yaml
# states.yaml
states:
  impl:
    agent: claude-code
    agent_mode: yolo
  review:
    agent: claude-code
    agent_mode: review
```

```json
// .rhei/settings.json
{
  "defaults": {
    "agent": "claude-code",
    "agent_mode": "yolo"
  }
}
```

Any object-shaped value for `agent:` is a validation error with a message
pointing to the `agents` registry.

### 5. Mode resolution

When `rhei run` spawns an agent for a state, the mode is resolved in this
order (first match wins):

1. CLI override — `--agent-mode <MODE>`
2. State-level — `agent_mode` on the state definition
3. Project defaults — `.rhei/settings.json` `defaults.agent_mode`
4. Global defaults — `~/.config/rhei/settings.json` `defaults.agent_mode`
5. The agent entry's first declared mode, if `modes` is non-empty
6. Otherwise — no mode (bare command)

The resolved mode name must be a key in the resolved agent's `modes` map
(unless resolution falls through to step 6). A missing mode is a validation
error at spawn time.

### 6. Agent resolution

When resolving an agent id from `states.yaml` or defaults:

1. Look up the id in the merged `agents` registry (built-ins → global →
   project). If found, use that entry.
2. Otherwise, fail validation:
   `error: agent '<id>' is not defined. Add an entry to agents.<id> in
   .rhei/settings.json or ~/.config/rhei/settings.json.`

The current "treat unknown id as a raw binary name" fallback in
`main.rs` is removed — it hides typos and couples state machines to host
binaries without explicit configuration.

### 7. Spawn-time flag order

```
<command...> <mode flags...> <prompt_flag> <prompt>? <model_flag> <model>? <mcp/skill flags...>
```

Prompt delivery via stdin (`stdin_prompt: true`) suppresses `<prompt_flag>
<prompt>`, matching today's behavior.

### 8. Code changes (`crates/rhei-validator`, `crates/rhei-cli`)

- Remove `AgentConfig::Custom` and the `#[serde(untagged)]` attribute on
  `AgentConfig`. `agent:` deserializes as a plain string. Internally, keep a
  newtype (or just `String`) for clarity.
- Keep `CustomAgentProfile` as the value type of the `agents` registry; add
  a `modes: BTreeMap<String, Vec<String>>` field (default empty).
- Add `agents: BTreeMap<String, CustomAgentProfile>` to `RheiSettings`.
- Extend `load_merged_settings` to merge `agents` by id (wholesale per
  entry), seeded by the built-in registry.
- Add `agent_mode: Option<String>` on `StateDef`, `RheiSettings`,
  `SettingsDefaults`, and as `--agent-mode` on `rhei run`.
- Rewrite `build_agent_command` around the unified `CustomAgentProfile`
  (there is only one code path after this change; the three
  `AgentConfig::Custom` arms collapse).
- Delete the "unknown id → treat as binary" fallback branch.

### 9. Spec changes

- `docs/specs/rhei-agents.spec.md`:
  - `agents` registry section documents `modes`.
  - `defaults.agent` row: type becomes **string** only.
  - "Custom Agent Profiles" section: inline-in-`states.yaml` / inline-in-
    `defaults.agent` examples removed. Examples show registry + state
    reference.
  - Built-in profile table gains a `Default Mode` column; the flags listed
    today become the built-in `yolo` mode.
  - Resolution-order section grows a parallel "Agent mode resolution"
    subsection.
- `docs/specs/rhei-states.spec.md`:
  - `agent:` field type becomes string; add `agent_mode:` field.
- `docs/specs/rhei-usage.spec.md`:
  - `rhei run --agent-mode <MODE>` added.
- `docs/rhei.spec.md`:
  - Cross-reference updates only.

### 10. Template changes

- `.agents/rhei/templates/multi-model-analysis/states.yaml` uses
  `agent: gemini` in the `gemini-analyze` state.
- The template emits a `.rhei/settings.json` under the instantiated plan
  containing the `agents.gemini` entry.
- The `gemini_command`, `gemini_command_arg_1`, `gemini_command_arg_2`,
  `gemini_prompt_flag`, `gemini_model_flag` inputs are dropped.
  `gemini_model` stays, as it's semantic, not transport.

## Consequences

### Easier

- State machines stop leaking transport details. A state says what it needs
  the agent to do, not how to launch it.
- Templates stop exposing CLI flag surface as instantiation inputs.
- One-line switch between permission postures — `agent_mode: yolo` vs
  `agent_mode: review` — using the same agent id.
- Agent profiles become shareable across plans and teams: drop a
  `~/.config/rhei/settings.json` fragment and every plan picks it up.
- Validation surfaces typos in agent ids at load time rather than at spawn.

### Harder

- One-off agent definitions now require touching `settings.json` — there's
  no shortcut for "just drop an inline command into this state."
- The wholesale-per-entry merge semantics mean overriding just one mode of a
  built-in agent requires re-declaring the full entry. The alternative
  (field-level merge) is richer but less predictable, and agent entries are
  small enough that duplication is acceptable.
- Template authors who need custom agents must ship a `settings.json`
  fragment alongside `states.yaml`. This is new surface for the template
  system to handle.

### Migration

- Any existing `states.yaml` or `settings.json` with inline `agent:` objects
  fails validation with a pointer to move the profile into
  `agents.<id>`.
- The `multi-model-analysis` template is updated in the same PR as the
  validator/CLI changes; there are no other in-tree users of inline agents.
- No public release has shipped with an `agents` registry yet, so the
  wholesale-merge rule does not break any deployed configuration.
