# Completeness Audit: `docs/functional-spec/rhei-agents.spec.md`

**Auditor:** claude-code[yolo]:anthropic:claude-opus-4-7
**Spec:** `docs/functional-spec/rhei-agents.spec.md`
**Implementation surface:** `crates/rhei-cli/src/main.rs`, `crates/rhei-validator/src/lib.rs`
**Methodology:** Every normative claim in the spec is enumerated and classified as `covered` / `partial` / `missing` / `not-normative`. Evidence is cited with `file:line`. Code quality is out of scope.

Legend: ✅ covered · 🟡 partial · ❌ missing · ⚪ not-normative (descriptive prose only)

---

## 1. Overview & framing (spec §Overview, lines 7–9)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 1.1 | `rhei run` resolves an agent for each task and spawns it as a subprocess | ✅ | `crates/rhei-cli/src/main.rs:7730` (`run_agent_mode`), `crates/rhei-cli/src/main.rs:7227` (`spawn_and_wait_agent`) |
| 1.2 | After subprocess exits, the engine evaluates declared forward transitions and performs the state change | ✅ | `crates/rhei-cli/src/main.rs:8439` (`try_auto_advance_task`) |
| 1.3 | Callbacks still fire on transitions — agents and callbacks are complementary | ✅ | `crates/rhei-cli/src/main.rs:9367` calls `execute_transition`, which runs `on_leave`/`on_enter` |

---

## 2. Agent Configuration — `defaults` block (spec §Agent Configuration → `defaults`, lines 31–148)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 2.1 | Settings load from `~/.config/rhei/settings.json` and `.rhei/settings.json` | ✅ | `crates/rhei-cli/src/main.rs:5922` (`load_merged_settings`) reads global then project |
| 2.2 | Both files use the same schema; project composes with global by key | ✅ | `crates/rhei-cli/src/main.rs:5922–5965` |
| 2.3 | `defaults.model` (string or null) | ✅ | `RheiSettings` parses defaults — `crates/rhei-cli/src/main.rs:5780` (`SettingsDefaults`) |
| 2.4 | `defaults.agent` accepts string-id only — inline agent objects rejected | 🟡 | `SettingsDefaults` parses `agent` as string only; explicit rejection / error path for inline objects not verified — JSON deserialisation will silently fail. The spec wording at line 142 ("Inline agent objects are not accepted") deserves an explicit validation error. No explicit "inline agent object" rejection found. |
| 2.5 | `defaults.agent_mode` (string or null) | ✅ | `crates/rhei-cli/src/main.rs:5780` `SettingsDefaults.agent_mode` |
| 2.6 | `defaults.agent_timeout` (duration) | ✅ | `crates/rhei-cli/src/main.rs:5780` `SettingsDefaults.agent_timeout` |
| 2.7 | `defaults.program_timeout` (duration) | ✅ | Resolved via program-spec path; consumed by `--program-timeout` CLI override (`crates/rhei-cli/src/main.rs:5514`). Parsing of `defaults.program_timeout` in `SettingsDefaults` not explicitly enumerated in audit. |
| 2.8 | `defaults.mcp_servers` (array) | ✅ | `SettingsDefaults.mcp_servers` at `crates/rhei-cli/src/main.rs:5780` |
| 2.9 | `defaults.skills` (array) | ✅ | `SettingsDefaults.skills` at `crates/rhei-cli/src/main.rs:5780` |

---

## 3. Agent Configuration — `agents` registry (spec lines 149–172)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 3.1 | `agents` is a registry keyed by agent id; inline agent definitions on a state or `defaults.agent` are forbidden | 🟡 | Registry exists (`built_in_agents` at `crates/rhei-cli/src/main.rs:5802`). No code explicitly rejects inline definitions with a typed validation error; reliance is on serde's struct shape. |
| 3.2 | `command: string array` (required) | ✅ | `CustomAgentProfile.command` — `crates/rhei-validator/src/lib.rs:165` |
| 3.3 | `prompt_flag` (optional) | ✅ | `CustomAgentProfile` per `crates/rhei-validator/src/lib.rs:165` |
| 3.4 | `model_flag` (optional) | ✅ | Same struct |
| 3.5 | `stdin_prompt` boolean (default false) | ✅ | Same struct |
| 3.6 | `timeout` (per-agent default) | ✅ | `CustomAgentProfile.timeout`; consulted by `resolve_legacy_agent_with_model` at `crates/rhei-cli/src/main.rs:6451` |
| 3.7 | `mcp_flag` (mutually exclusive with `mcp_config_flag`) | 🟡 | Field parsed (`crates/rhei-cli/src/main.rs:5839` for `codex`) but **never appended to the spawn command line** — see §11 below |
| 3.8 | `mcp_config_flag` (mutually exclusive with `mcp_flag`) | 🟡 | Field parsed (`crates/rhei-cli/src/main.rs:5822` for `claude-code`) but **never appended to the spawn command line** — see §11 below |
| 3.9 | Mutual exclusion validation of `mcp_flag` vs `mcp_config_flag` | ❌ | No enforcement found. Both can be declared without error. |
| 3.10 | `skill_flag` (omit ⇒ agent does not support skills) | 🟡 | Field parsed (`crates/rhei-cli/src/main.rs:5823, 5905`) but **never appended to the spawn command line** — see §11 below |
| 3.11 | `modes` (named flag sets, ordered string array values) | ✅ | `CustomAgentProfile.modes` — `crates/rhei-validator/src/lib.rs:165`; built-in `yolo` modes populated at `crates/rhei-cli/src/main.rs:5815–5908` |
| 3.12 | `session` block (per `rhei-snapshots.spec.md`) | ❌ | No `session` field found on `CustomAgentProfile`. `Grep session\|snapshot` in `crates/rhei-validator/src/lib.rs` returns no matches. The cross-spec snapshot integration is unimplemented in v1 of this surface (the implementation notes treat snapshots as out of scope; spec language is normative). |
| 3.13 | Built-in agent ids preloaded; user entry with same id replaces wholesale | ✅ | `crates/rhei-cli/src/main.rs:5932–5938` (project entries override global; global overrides built-ins) |

---

## 4. Agent Configuration — `models` registry (spec lines 174–192)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 4.1 | `models` registry keyed by model id | ✅ | `RheiSettings.models` at `crates/rhei-cli/src/main.rs:5731` |
| 4.2 | `provider` required | ✅ | `ModelProfile.provider` at `crates/rhei-cli/src/main.rs:5743` |
| 4.3 | `model` required (concrete provider model name) | ✅ | `ModelProfile.model` at `crates/rhei-cli/src/main.rs:5743` |
| 4.4 | `default_agent` (optional) | ✅ | `ModelProfile.default_agent`; consumed by `resolve_legacy_agent_with_model` at `crates/rhei-cli/src/main.rs:6400` |
| 4.5 | `agents` per-agent overrides keyed by agent id | ✅ | `ModelProfile.agents` at `crates/rhei-cli/src/main.rs:5743`; `ModelAgentBinding` at `crates/rhei-cli/src/main.rs:5764` |
| 4.6 | `models.<id>.agents.<agent>.args` (extra args) | 🟡 | Field parsed (`ModelAgentBinding.args`) but `build_agent_command` (`crates/rhei-cli/src/main.rs:6928`) never appends them — see implementation-notes Deferral #2 |
| 4.7 | `models.<id>.agents.<agent>.autonomous_args` (autonomous overrides) | 🟡 | Parsed but unused — same Deferral #2 |
| 4.8 | `models.<id>.agents.<agent>.timeout` | ✅ | Consulted at `crates/rhei-cli/src/main.rs:6447` |

---

## 5. Agent Configuration — `mcp_servers` registry (spec lines 194–208)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 5.1 | Registry keyed by server id | ✅ | `McpServerProfile` at `crates/rhei-validator/src/lib.rs:204` |
| 5.2 | `command` (string array) OR `url` (string) | ✅ | Fields present on `McpServerProfile` |
| 5.3 | `command` and `url` are **mutually exclusive**; entry must declare exactly one | ❌ | No validator enforces the XOR rule. A JSON object with both fields will silently deserialize. |
| 5.4 | `transport` (`sse`, `websocket`) for remote servers | 🟡 | Field present on `McpServerProfile`; allowed-values validation not located. |
| 5.5 | `env` object with `${VAR}` expansion | 🟡 | Field present. `${VAR}` expansion at spawn time is part of Half B (deferred per implementation notes). |
| 5.6 | `working_directory` (command servers only) | ✅ | Field present on `McpServerProfile` |
| 5.7 | `startup_timeout` (duration, default `10s`) | ❌ | Field exists in struct (`crates/rhei-validator/src/lib.rs:220`) but the default and enforcement live in Half B, which is unimplemented. No code applies a 10 s wait. |

---

## 6. Agent Configuration — `skills` registry (spec lines 210–224)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 6.1 | Registry keyed by skill id | ✅ | `SkillProfile` at `crates/rhei-validator/src/lib.rs:227` |
| 6.2 | `path` required, leading `~` expands | 🟡 | Field present; `~` expansion logic not located in audit. If the field is consumed only via env reflection, expansion may not be applied. |
| 6.3 | `description` optional | ✅ | Field present |
| 6.4 | Skills wired to agent only if profile declares `skill_flag`; otherwise skipped with a warning | ❌ | No skip-with-warning path exists because the `skill_flag` path itself is not used (see §11). |

---

## 7. `snapshots` settings block (spec lines 226–231)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 7.1 | Top-level `snapshots` block exists in settings | ❌ | No `snapshots` field on `RheiSettings`. `Grep` for `snapshot` against `crates/rhei-validator/src/lib.rs` returned no matches. The spec defers field definitions to `rhei-snapshots.spec.md` but explicitly declares the block "lives" in settings (spec line 230) — this is not loaded by the implementation. |

---

## 8. Per-State Settings (spec §Per-State Settings, lines 234–240)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 8.1 | `target` / `all_targets` are the preferred selectors on states | ✅ | `ExecutionTarget` at `crates/rhei-validator/src/lib.rs:345`; `parse_execution_target` at `crates/rhei-validator/src/lib.rs:394` |
| 8.2 | Legacy `model` / `all_models`, optional `agent`, `mcp_servers`, `skills` fields remain supported | ✅ | `resolve_legacy_agent_with_model` at `crates/rhei-cli/src/main.rs:6369`; `resolve_tooling` at `crates/rhei-cli/src/main.rs:6131` |

---

## 9. Merge Semantics (spec §Merge Semantics, lines 243–266)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 9.1 | Built-ins load first, then global, then project | ✅ | `crates/rhei-cli/src/main.rs:5932–5961` |
| 9.2 | `defaults` shallow-override by field | ✅ | `crates/rhei-cli/src/main.rs:5956–5961` (`.or()` per field) |
| 9.3 | `agents` merge by id — same id **replaces wholesale**, no field-level merge | ✅ | `crates/rhei-cli/src/main.rs:5932–5938` (entries replace whole `CustomAgentProfile`) |
| 9.4 | `models` merge by model id | ✅ | `crates/rhei-cli/src/main.rs:5949–5952` |
| 9.5 | `models.<id>.agents` merge by agent id | 🟡 | Top-level `models` merge replaces a `ModelProfile` wholesale; the spec language at line 254 expects per-binding merge. The agent map within a model is **not** deep-merged across global/project. |
| 9.6 | `mcp_servers` merge by server id | ✅ | `crates/rhei-cli/src/main.rs:5941–5944` |
| 9.7 | `skills` merge by skill id | ✅ | `crates/rhei-cli/src/main.rs:5945–5948` |
| 9.8 | `null` explicitly clears an inherited optional field | 🟡 | `.or()` chains do not distinguish "absent" from "explicit null" because serde deserializes both to `None` unless typed as `Option<Option<T>>`. Behavior likely matches spec for most fields by coincidence, but no explicit "null clears" handling exists. |
| 9.9 | `defaults.mcp_servers` / `defaults.skills` are replaced wholesale by project (not concatenated); `[]` clears inherited | ✅ | `crates/rhei-cli/src/main.rs:5959–5960` — `.or()` performs wholesale replacement when project has any value (including `[]`). |

---

## 10. Resolution Order (spec §Resolution Order, lines 268–319)

### 10A. Model id resolution (lines 271–280)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 10A.1 | Order: CLI `--model` → state `model` → project `defaults.model` → global `defaults.model` | ✅ | `resolve_legacy_agent_with_model` at `crates/rhei-cli/src/main.rs:6369–6400` (model precedence) |
| 10A.2 | Resolved model id must exist in merged `models` registry | 🟡 | Resolved model id is **used** to look up `ModelProfile`, but missing-id error message specifically tied to the registry was not enumerated. Code in `validate_machine_settings_references` (`crates/rhei-cli/src/main.rs:5977`) likely catches this; the precise "model X not found" error text was not located. |
| 10A.3 | If no model is configured at any level, model-specific callback/template fields are omitted | ✅ | `RHEI_MODEL` / `RHEI_MODEL_PROVIDER` / `RHEI_MODEL_NAME` env injection at `crates/rhei-cli/src/main.rs:6983–6998` is gated on `Option::is_some()` |

### 10B. Agent id resolution (lines 282–307)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 10B.1 | Order: CLI `--agent` → state `agent` → project `defaults.agent` → global `defaults.agent` → `models.<id>.default_agent` | 🟡 | The implementation collapses project + global into one composed `RheiSettings.agent` via `load_merged_settings`. Step 5 (`models.<id>.default_agent`) is consulted at `crates/rhei-cli/src/main.rs:6400`. Distinct project-vs-global precedence is therefore handled correctly by merge order, even though `resolve_legacy_agent_with_model` only sees the composed view. |
| 10B.2 | Resolved id must match an entry in merged `agents` registry; unknown id is a configuration error with the spec's exact error template | 🟡 | `resolve_legacy_agent_with_model` errors when id is unknown (`crates/rhei-cli/src/main.rs:~6420`), but the **exact error wording** from spec lines 295–299 ("Add an entry to agents.<id>… reference one of the built-in ids (claude-code, codex, cursor, gemini, kilocode)") was not verified verbatim. |
| 10B.3 | "No agent configured" error with spec's exact wording at lines 304–307 ("Set defaults.agent, the state's agent, models.impl-fast.default_agent, or pass --agent <AGENT>…") | 🟡 | Error exists at `crates/rhei-cli/src/main.rs:7877` ("no agent configured.\nFix by either:\n  • Re-run with --agent…  • Add to …/.rhei/settings.json…") — wording diverges from spec template but the spec template is presented as illustrative (`error: …`). Treating as 🟡 because the spec lists each remediation slot it must mention; the implementation mentions only `--agent` and `.rhei/settings.json` and omits `models.<id>.default_agent`. |
| 10B.4 | For states with `all_targets`, resolution is bypassed for fields encoded in each selector | ✅ | `resolve_target_agent` at `crates/rhei-cli/src/main.rs:6300` parses `agent[mode]:provider:model` selectors directly |
| 10B.5 | Validation must still verify the referenced agent and mode exist | ✅ | `resolve_target_agent` validates at `crates/rhei-cli/src/main.rs:6308–6327` |
| 10B.6 | For legacy `all_models`, agent resolution runs per model | ✅ | `resolve_agent_invocations` (called from `crates/rhei-cli/src/main.rs:7870`) iterates per model |
| 10B.7 | When snapshots are enabled, each legacy `model`/`all_models` execution must resolve an effective target tuple before emit/inherit; otherwise explicit snapshot fields are rejected and auto-emit is skipped | ❌ | Snapshots are not implemented in this surface; this gate is missing. |

### 10C. Mode Resolution Order (lines 321–334)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 10C.1 | Order: CLI `--agent-mode` → state `agent_mode` → project `defaults.agent_mode` → global `defaults.agent_mode` → registry-default (first declared mode) → none | 🟡 | `resolve_legacy_agent_with_model` at `crates/rhei-cli/src/main.rs:6418–6427` covers CLI, state, defaults, registry-default. Distinct project/global precedence is handled via merge order (single composed `defaults` object) — equivalent to spec ordering. |
| 10C.2 | Resolved mode must be a key in the agent's `modes` map when non-empty; missing is a spawn-time error | ✅ | Validation at `crates/rhei-cli/src/main.rs:6430–6439` |
| 10C.3 | If `modes` is empty, no mode flags appended | ✅ | `build_agent_command` mode block iterates only present mode flags (`crates/rhei-cli/src/main.rs:6950–6956`) |

### 10D. Per-State Tooling Resolution (lines 335–352)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 10D.1 | Start from `defaults.mcp_servers` / `defaults.skills` | ✅ | `resolve_tooling` at `crates/rhei-cli/src/main.rs:6131` |
| 10D.2 | Union with state's lists; empty list on state clears defaults | 🟡 | Resolution code present; precise "empty list clears" semantics not separately verified. |
| 10D.3 | Deduplicate by id; later entries override earlier | 🟡 | Not separately verified; likely satisfied by `resolve_tooling` walking entries in order. |
| 10D.4 | Resolve each id against merged registry; inline object entries on state/defaults do not require a registry entry | ✅ | `resolve_mcp_entry` / `resolve_skill_entry` accept inline objects (`crates/rhei-cli/src/main.rs:6209–6240`) |
| 10D.5 | Id with no registry match and no inline definition is a validation error | ✅ | Same functions return an error |
| 10D.6 | Resolved sets are distinct per state and per invocation; restart recomputes | ✅ | `resolve_tooling` called per spawn |

---

## 11. Known Agent Profiles (spec lines 378–410)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 11.1 | `claude-code`: `claude -p <prompt> --model <m> --mcp-config <path> --skill <id>` + `--permission-mode bypassPermissions` for `yolo` | 🟡 | Profile declared at `crates/rhei-cli/src/main.rs:5815–5827` matches spec. But **`--mcp-config` and `--skill` flags are never actually appended to the spawn command** — `build_agent_command` at `crates/rhei-cli/src/main.rs:6928–7001` does not reference `mcp_flag`, `mcp_config_flag`, or `skill_flag`. |
| 11.2 | `codex`: `codex exec --model <m> --mcp <spec>` + stdin prompt + `--sandbox danger-full-access --skip-git-repo-check` for `yolo` | 🟡 | Profile at `crates/rhei-cli/src/main.rs:5832–5847` matches spec; spec table line 389 also lists `-a never` flag for yolo but `built_in_agents` codex `yolo` does **not** include `-a never`. The illustrative settings example at lines 71–73 *does* include `-a never`. Either the spec table or the implementation is incorrect — flag-set divergence is a gap. `--mcp` per-server emission is missing (see 11.1). |
| 11.3 | `gemini`: `gemini --prompt <prompt> --model <m>` + `--approval-mode yolo` | ✅ | `crates/rhei-cli/src/main.rs:5851–5861` |
| 11.4 | `cursor`: `cursor-agent --print <prompt> --model <m>` + `--force` | ✅ | `crates/rhei-cli/src/main.rs:5881–5891` |
| 11.5 | `kilocode`: `kilo --auto <prompt> --model <m>` + `--yolo` | ✅ | `crates/rhei-cli/src/main.rs:5866–5876` |
| 11.6 | `pi`: `pi -p <prompt> --model <m> --skill <path>` with no modes | 🟡 | Profile at `crates/rhei-cli/src/main.rs:5898–5908` matches structurally. `--skill <path>` (note: spec says `<path>` for pi vs `<id>` for claude-code) is parsed but **not emitted** — same gap as 11.1. |
| 11.7 | In v1, built-in `cursor`/`kilocode` do not expose supported `CustomAgentProfile.session`; auto-emit skipped, explicit ops fail with `unsupported-snapshot-session` | ❌ | No `session` field on `CustomAgentProfile`; no `unsupported-snapshot-session` error path. |
| 11.8 | Gemini snapshot support is provisional/unsupported | ❌ | Snapshot surface absent. |
| 11.9 | Agents marked `unsupported` for MCP/skills receive a warning at spawn time when the effective set includes entries they cannot consume | ❌ | No such warning path located. (`Grep` for `unsupported` in `main.rs` returns only condition-evaluator messages.) |
| 11.10 | Required MCP entries escalate the unsupported-agent warning to an error | ❌ | Missing — see 11.9 and §15. |
| 11.11 | A user-written entry for a built-in id replaces the built-in entry wholesale | ✅ | Confirmed by §9.3. |

---

## 12. Custom Agents & Modes (spec §Custom Agents, §Modes, lines 412–478)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 12.1 | Custom agents declared in `agents` registry, not inline | 🟡 | Achievable via registry, but no positive validation error forbids inline. See 2.4 / 3.1. |
| 12.2 | Resolved mode's flags appended right after base `command`, before prompt/model/mcp/skill flags | ✅ | Flag order in `build_agent_command` at `crates/rhei-cli/src/main.rs:6944–6974` matches spec (`<command> <mode> <prompt_flag> <prompt> <model_flag> <model> [--]`) |
| 12.3 | When `stdin_prompt` is true, prompt is written to stdin and `--` is appended after the model flag | ✅ | `crates/rhei-cli/src/main.rs:6972–6974` |
| 12.4 | When the agent declares **no modes**, `agent_mode` must not be set for states that use this agent | ❌ | `validate_machine_settings_references` at `crates/rhei-cli/src/main.rs:5993–6002` only errors when `profile.modes.is_empty() == false`; if the agent has no modes the validator silently accepts `agent_mode`. Direct violation of spec line 476–478. |
| 12.5 | Mode-resolution selection — see §10C | ✅/🟡 | Covered above |

---

## 13. Prompt Composition (spec §Prompt Composition, lines 480–522)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 13.1 | Prompt structure begins with `# Task {task_id}: {task_title}` then `## State: {state}` | ✅ | `compose_agent_prompt` at `crates/rhei-cli/src/main.rs:6886–6889` |
| 13.2 | `{resolved personality, if present}` rendered before `## Instructions` | ✅ | `crates/rhei-cli/src/main.rs:6890–6892` |
| 13.3 | `## Instructions` section with resolved instructions | ✅ | `crates/rhei-cli/src/main.rs:6893` |
| 13.4 | `## Task Content` section (task body + child task nodes) | ✅ | `crates/rhei-cli/src/main.rs:6894–6908` |
| 13.5 | `## Rhei Commands` section with `plan_path`, "rhei run is responsible for advancing", warning against calling `rhei transition`/`rhei complete` except for nested executions | ✅ | `crates/rhei-cli/src/main.rs:6909–6917`. Implementation includes the extra line `The active state machine is …` (not in spec but harmless extension); inclusion of the "do not modify `**State:**` lines directly" wording is an extension. |
| 13.6 | `Available transitions from {state}:` followed by list of declared transitions with descriptions | ✅ | `crates/rhei-cli/src/main.rs:6914–6917` |
| 13.7 | Prompt carries domain instructions only — no completion prose | ✅ | Test `compose_agent_prompt_carries_domain_instructions_only` at `crates/rhei-cli/src/main.rs:11864` |
| 13.8 | Template variables (`{task_id}`, `{model}`, `{model.provider}`, `{model.name}`, `{visit_count}`, …) resolved before send | ✅ | `resolve_runtime_template_text` called for personality / instructions in `compose_agent_prompt` |
| 13.9 | Prompt delivered via configured mechanism (flag or stdin) | ✅ | `build_agent_command` at `crates/rhei-cli/src/main.rs:6958–6962` |

---

## 14. Completion Authority & Condition (spec lines 523–617)

### 14A. Authority (lines 523–557)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 14A.1 | Two authorities: `worker` and `orchestrator`; determined by execution mode, not declared on the state | ✅ | Spec is descriptive of architecture; `rhei run` is the orchestrator path (`crates/rhei-cli/src/main.rs:7730`). |
| 14A.2 | Under `orchestrator`, spawned subprocess must not call `rhei transition`/`rhei complete`, must not edit `**State:**` directly; nested execution exempted | ⚪ | Normative against the agent's behavior, not enforceable in the engine. Prompt at `crates/rhei-cli/src/main.rs:6909–6917` communicates the rule. |
| 14A.3 | `instructions`/`personality` describe domain work only — not completion mechanics | ⚪ | Stylistic guidance for state machine authors; not enforced. |
| 14A.4 | Gating states bypass completion authority — no subprocess spawned, no automatic transition fires | ✅ | `crates/rhei-cli/src/main.rs:7845–7853` |

### 14B. Condition (lines 559–584)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 14B.1 | Condition: subprocess exits 0 AND every required artifact in state's `outputs:` exists | ✅ | `state_outputs_exist_for_resolved_invocation` at `crates/rhei-cli/src/main.rs:6604` plus `try_auto_advance_task` post-exit at `crates/rhei-cli/src/main.rs:8439` |
| 14B.2 | If state declares no `outputs:`, condition (2) is vacuously true | ✅ | `ensure_state_outputs_exist` returns Ok when no outputs are declared |
| 14B.3 | Contract maps 1:1 onto native headless mode of every supported agent | ⚪ | Descriptive — verified by built-in profiles. |
| 14B.4 | Program states have their own exit-code-driven completion semantics (out of scope here) | ⚪ | Reference to `rhei-programs.spec.md`. |

### 14C. Runtime Semantics (lines 586–605)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 14C.1 | Spawn subprocess, wait on exit OR timeout | ✅ | `spawn_and_wait_agent` at `crates/rhei-cli/src/main.rs:7227` |
| 14C.2 | On timeout, send SIGTERM, 10 s grace, then SIGKILL | ✅ | `crates/rhei-cli/src/main.rs:7352–7367` with `AGENT_TERMINATE_GRACE` constant at `crates/rhei-cli/src/main.rs:7128–7131` |
| 14C.3 | "Agents that fork long-running descendants should install their own cleanup; engine kills only direct subprocess" | ⚪ | Architectural caveat. Behavior matches by virtue of using `child.kill()` on the direct PID. |
| 14C.4 | On non-zero exit, route through exit-code / error transition path | ✅ | `crates/rhei-cli/src/main.rs:8469–8496` (sequential), parallel path mirrors. `fire_timeout_transition` at `crates/rhei-cli/src/main.rs:9029` |
| 14C.5 | On exit 0, verify outputs; if missing, task stays in state and engine logs `warning: agent exited 0 but required outputs are missing for task {id} in state '{state}': <name1>, <name2>` | 🟡 | Task stays in state (verified — `try_auto_advance_task` returns `None` because forward-transition input check fails), and a warning is logged at `crates/rhei-cli/src/main.rs:8457–8466`, but **the exact spec wording with the list of missing output names is not emitted.** The implementation logs only `"warning: agent exited 0 but task X did not advance from 'Y'"` and `"warning: agent exited 0 but task X could not auto-advance from 'Y': <err>"`. |
| 14C.6 | Otherwise evaluate forward transitions in selection order and execute first match | ✅ | `try_auto_advance_task` → `find_next_transition` |

### 14D. Timeout Requirement (lines 607–617)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 14D.1 | Under orchestrator authority, a timeout must resolve to a finite value at some level; missing-timeout states are a validation error | ✅ | `ensure_orchestrator_timeout` at `crates/rhei-cli/src/main.rs:6578–6590` with explicit error message including the four resolution levels |
| 14D.2 | Under worker authority there is no timeout enforcement | ✅ | `rhei next`/`rhei transition` paths do not enforce timeouts |

---

## 15. Missing Tooling (spec §Missing Tooling, lines 726–779)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 15.1 | Id referenced but not in registry/inline ⇒ hard error at `rhei validate` / settings load | ✅ | `resolve_mcp_entry` / `resolve_skill_entry` at `crates/rhei-cli/src/main.rs:6209–6240` |
| 15.2 | Registry entry exists but server/skill fails availability check at spawn ⇒ optional/required behavior | ❌ | Half B (live handshake/probe) is not implemented. Implementation notes explicitly defer this. |
| 15.3 | MCP server crashes mid-session ⇒ agent surfaces protocol error; Rhei does not intervene | ⚪ | Descriptive — emergent behavior. |
| 15.4 | Availability rules: command MCP → process + handshake + alive within `startup_timeout`; URL MCP → connection + handshake; skill → path exists and readable | ❌ | Not implemented; availability is currently `definition.is_some()` (`crates/rhei-cli/src/main.rs:6081–6087`) — i.e. "is it in the registry?". |
| 15.5 | `optional: true` field on per-state and default entries | ✅ | `ResolvedMcpEntry.optional` / `ResolvedSkillEntry.optional` at `crates/rhei-cli/src/main.rs:6047–6064` |
| 15.6 | Required entry fails availability ⇒ do not spawn; look for `mcp_unavailable` / `skill_unavailable` transition; fire with `triggeredBy: 'system'`; populate `transitionData.unavailable` | ❌ | No `mcp_unavailable` / `skill_unavailable` handling. `Grep` for those terms returns no matches. |
| 15.7 | If no such transition, task stays in current state and engine logs the spec's error template | ❌ | No code path emits this error. |
| 15.8 | Optional entry fails ⇒ warning + spawn with remaining tooling; template variables / env reflect availability | 🟡 | Env reflection wired via `RHEI_MCP_<NAME>_AVAILABLE` / `RHEI_SKILL_<ID>_AVAILABLE` at `crates/rhei-cli/src/main.rs:7100–7110`, but the "warning + drop" path only fires once Half B is in place — current implementation never drops anything because nothing ever fails. |
| 15.9 | Unsupported agent (no `mcp_flag`/`mcp_config_flag`/`skill_flag` for entries) treated identically to availability failure | ❌ | Not implemented; flags themselves are unused (§11.1, §11.6). |

---

## 16. Environment Variables (spec §Environment Variables, lines 619–638)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 16.1 | `RHEI_PLAN_PATH` absolute path to plan file or workspace directory | ✅ | `crates/rhei-cli/src/main.rs:6976` |
| 16.2 | `RHEI_TASK_ID` | ✅ | `crates/rhei-cli/src/main.rs:6977` |
| 16.3 | `RHEI_STATE` | ✅ | `crates/rhei-cli/src/main.rs:6978` |
| 16.4 | `RHEI_MODEL` (model profile id) | ✅ | `crates/rhei-cli/src/main.rs:6984` |
| 16.5 | `RHEI_MODEL_PROVIDER` | ✅ | `crates/rhei-cli/src/main.rs:6994` |
| 16.6 | `RHEI_MODEL_NAME` | ✅ | `crates/rhei-cli/src/main.rs:6997` |
| 16.7 | `RHEI_AGENT` | ✅ | `crates/rhei-cli/src/main.rs:6979` |
| 16.8 | `RHEI_MCP_SERVERS` comma-separated list of resolved MCP server ids that started successfully | 🟡 | Wired at `crates/rhei-cli/src/main.rs:7098`, but "started successfully" semantics depend on Half B; currently lists all resolved (registry-present) ids. |
| 16.9 | `RHEI_MCP_<NAME>_AVAILABLE` `true`/`false` for each declared server | 🟡 | Wired at `crates/rhei-cli/src/main.rs:7100–7104`; always `true` once Half A resolves the id (see §15.4). Name transformation (uppercase, hyphens/spaces → underscore) implemented in `env_id_segment` at `crates/rhei-cli/src/main.rs:6110–6114`. |
| 16.10 | `RHEI_SKILLS` comma-separated resolved skill ids | 🟡 | Same caveat as 16.8 |
| 16.11 | `RHEI_SKILL_<ID>_AVAILABLE` | 🟡 | `crates/rhei-cli/src/main.rs:7106–7110` — same caveat |
| 16.12 | Working directory set to workspace root (for workspaces) or plan file's parent (for single-file plans) | 🟡 | Not separately verified in audit (spec line 638). |

---

## 17. `rhei run` Agent Mode CLI & Execution Loop (spec lines 640–699)

### 17A. CLI flags (lines 643–660)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 17A.1 | `--dry-run` | ✅ | `crates/rhei-cli/src/main.rs:5464` |
| 17A.2 | `--no-callbacks` | ✅ | `crates/rhei-cli/src/main.rs:5467` |
| 17A.3 | `--no-agent` | ✅ | `crates/rhei-cli/src/main.rs:5494` |
| 17A.4 | `--no-program` | ✅ | `crates/rhei-cli/src/main.rs:5512` |
| 17A.5 | `--agent <AGENT>` | ✅ | `crates/rhei-cli/src/main.rs:5496` |
| 17A.6 | `--model <MODEL>` | ✅ | `crates/rhei-cli/src/main.rs:5502` |
| 17A.7 | `--continue-on-error` | ✅ | `crates/rhei-cli/src/main.rs:5470` |
| 17A.8 | `--parallel <N>` (default 1; `0` = unlimited) | ✅ | `crates/rhei-cli/src/main.rs:5472`; `batch_size` honors `0 ⇒ all` at `crates/rhei-cli/src/main.rs:8252–8255` |
| 17A.9 | `--program-timeout <DURATION>` | ✅ | `crates/rhei-cli/src/main.rs:5514` |
| 17A.10 | `--agent-mode <MODE>` (implied by §Mode Resolution Order item 1) | ✅ | `crates/rhei-cli/src/main.rs:5498–5500` (not in the spec table at lines 650–660 — spec table omits this flag entirely, though the resolution-order section at line 324 normatively assumes it exists) |

### 17B. Sequential Mode (lines 663–680)

| # | Step | Status | Evidence / Gap |
|---|---|---|---|
| 17B.1 | Load plan and state machine; validate | ✅ | `load_plan` at `crates/rhei-cli/src/main.rs:7808` |
| 17B.2 | Find next claimable task (same eligibility as `rhei next`) | ✅ | `find_ready_tasks` at `crates/rhei-cli/src/main.rs:7809` |
| 17B.3 | Resolve model/agent for state | ✅ | `resolve_agent_invocations` at `crates/rhei-cli/src/main.rs:7870` |
| 17B.4 | If agent mode enabled and no agent configured, fail with error | ✅ | `crates/rhei-cli/src/main.rs:7877–7882` |
| 17B.5 | Compose prompt | ✅ | `compose_agent_prompt` |
| 17B.6 | Log spawn to `runtime/logs/task-{task_id}-{state}[-{visit_count}].log` | 🟡 | Log path uses target/model slug rather than `visit_count`. See §19. |
| 17B.7 | Spawn agent CLI as subprocess | ✅ | `spawn_and_wait_agent` |
| 17B.8 | Wait for agent (subject to timeout) | ✅ | Same |
| 17B.9 | Re-read plan; if external actor changed state, respect it | ✅ | `try_auto_advance_task` reloads plan at `crates/rhei-cli/src/main.rs:9351` |
| 17B.10 | On exit 0, evaluate forward transitions; execute first match | ✅ | `try_auto_advance_task` → `find_next_transition` |
| 17B.11 | On exit 0 with no match, log `warning: agent exited 0 but task {id} did not advance from '{state}'` | ✅ | `crates/rhei-cli/src/main.rs:8457–8460` |
| 17B.12 | On non-zero exit without `--continue-on-error`: log + stop | ✅ | `crates/rhei-cli/src/main.rs:8487–8495` |
| 17B.13 | On non-zero exit with `--continue-on-error`: log + skip + continue | ✅ | `crates/rhei-cli/src/main.rs:8487` (returns inverse condition) |
| 17B.14 | Repeat until no claimable tasks remain | ✅ | Outer loop at `crates/rhei-cli/src/main.rs:7807–8732` |

### 17C. Parallel Mode (lines 682–699)

| # | Step | Status | Evidence / Gap |
|---|---|---|---|
| 17C.1 | Find **all** claimable tasks | ✅ | `find_ready_tasks` |
| 17C.2 | Select up to N **mutually independent** tasks (no dependency edges between them); N=0 means unlimited | 🟡 | Implementation uses a *concurrent-state* rule (`crates/rhei-cli/src/main.rs:8213–8250`) — at most one task per non-`concurrent` state per pass — **not** the transitive `**Prior:**` chain rule the spec describes (lines 695–697). This may allow tasks with implicit dependencies to run concurrently if they happen to be in different states, or block independent tasks in the same state. Behavioral divergence from spec. |
| 17C.3 | For each selected task, resolve model+agent, compose prompt, spawn concurrently | ✅ | Parallel branch at `crates/rhei-cli/src/main.rs:8505–8725` |
| 17C.4 | Each agent writes to its own log file | ✅ | Per-agent `agent_log_path` |
| 17C.5 | Wait for any agent to exit | ✅ | Parallel branch joins threads |
| 17C.6 | On exit, re-read plan; apply same rules as sequential | ✅ | Parallel branch reloads via `try_auto_advance_task` |
| 17C.7 | Scan for newly claimable tasks; spawn if pool below N | ✅ | Outer pass loop re-evaluates after each exit |
| 17C.8 | Independence rule: two tasks independent ⇔ neither in other's transitive `**Prior:**` chain | ❌ | Not implemented. See 17C.2. |
| 17C.9 | Engine must not spawn two agents that could produce conflicting edits to the same task file | ❌ | Beyond same-state filter, no file-conflict detection. For directory workspaces this is OK by construction. |
| 17C.10 | Single-file plans: `--parallel > 1` warns and falls back to sequential | ✅ | `crates/rhei-cli/src/main.rs:7692–7702` |

---

## 18. Interaction Between Agents and Callbacks; Gating States (spec lines 701–724)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 18.1 | Agent does work; `rhei run` evaluates and transitions; callbacks fire on transition | ✅ | `try_auto_advance_task` → `execute_transition` |
| 18.2 | `--no-callbacks` suppresses callbacks but not agent or program spawning | ✅ | Threaded through `try_auto_advance_task(..., no_callbacks)` |
| 18.3 | `--no-agent` suppresses agent spawning but not program/callbacks | ✅ | `crates/rhei-cli/src/main.rs:7873` |
| 18.4 | `--no-program` suppresses program spawning but not agent/callbacks | ✅ | `crates/rhei-cli/src/main.rs:5512` |
| 18.5 | All three flags combinable independently | ✅ | Independent boolean fields |
| 18.6 | Gating states: `rhei run` logs "Task {id} is in gating state '{state}'. Waiting for human action." and skips | ✅ | `crates/rhei-cli/src/main.rs:7845–7853` |
| 18.7 | When human transitions task out via `rhei transition`, next run pass picks it up | ✅ | Pass loop re-evaluates ready tasks each pass |

---

## 19. Log Capture (spec §Log Capture, lines 931–981)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 19.1 | All agent stdout/stderr captured to `runtime/logs/` relative to workspace/plan root | ✅ | `spawn_and_wait_agent` writes to `agent_log_path` |
| 19.2 | Simple state: `runtime/logs/task-{task_id}-{state}.log` | ✅ | `agent_log_path` at `crates/rhei-cli/src/main.rs:7115–7126` |
| 19.3 | Counted-loop state: `runtime/logs/task-{task_id}-{state}-{visit_count}.log` | ❌ | Implementation uses `resolved_agent_log_suffix` (`crates/rhei-cli/src/main.rs:6595–6601`), which suffixes by target slug or model name — **not** visit count. The spec's counted-loop log naming pattern is not produced. |
| 19.4 | Model-specific state: `runtime/logs/task-{task_id}-{state}-{model}.log` | ✅ | Model fallback in `resolved_agent_log_suffix` |
| 19.5 | Both visits and model: `runtime/logs/task-{task_id}-{state}-{model}-{visit_count}.log` | ❌ | Same gap as 19.3 — no visit-count component |
| 19.6 | Header begins with `=== rhei agent log v1 ===` | ✅ | `crates/rhei-cli/src/main.rs:7257` |
| 19.7 | Header includes `agent:`, `model:`, `provider:`, `model_name:`, `task:`, `state:`, `started:`, `timeout:`, `plan:`, `mcp_servers:`, `skills:` | ✅ | `crates/rhei-cli/src/main.rs:7257–7293` |
| 19.8 | Header ends with `===` separator before raw body | ✅ | `crates/rhei-cli/src/main.rs:7293` |
| 19.9 | Footer: `=== exit ===` / `code:` / `duration:` / `ended:` / `===` | ✅ | `crates/rhei-cli/src/main.rs:7387–7392` |
| 19.10 | `v1` is the format version — incremented when header/footer shape changes | ✅ | Constant in header writer |
| 19.11 | Body is raw, unmodified output | ✅ | Direct pipe of stdout/stderr |
| 19.12 | Optional entries that failed availability ⇒ suffixed with `?` in `mcp_servers:` / `skills:` | ✅ | `format_tooling_log_line` at `crates/rhei-cli/src/main.rs:7067–7086` |
| 19.13 | Missing line ⇒ state declared no entries of that kind | ✅ | `format_tooling_log_line` returns `None` ⇒ no line written |
| 19.14 | `runtime/logs/` created automatically; `rhei reset` removes entire `runtime/` | ✅ | Directory created on demand; `rhei reset` documented elsewhere |

---

## 20. Dry-Run Output (spec §Dry-Run Output, lines 983–1001)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 20.1 | `rhei run --dry-run` shows what would be spawned without executing | ✅ | `crates/rhei-cli/src/main.rs:8257–8283` |
| 20.2 | Per-pass header: `Pass N: X ready, Y terminal, Z total.` | 🟡 | Pass headers exist via `RunEvent::PassEnded` and the run sink, but exact format-string parity with spec line 988 was not verified. |
| 20.3 | "Would spawn:" lines that include the actual command + key flags | 🟡 | Implementation emits `"Would spawn: {agent_id} (model: {model_str})"` and follow-on `"Agent: …, Model: …, Timeout: …"` and `"Log: …"`. Spec example renders the actual shell command (`claude -p "<prompt...>" --model claude-sonnet-4-6`). The implementation omits the literal command + prompt placeholder. |
| 20.4 | Final "Dry run complete - no agents were spawned." | 🟡 | Implementation emits `"Dry run complete — no programs or agents were spawned."` (en-dash + extends to programs). Wording divergence; equivalent in spirit. |

---

## 21. `rhei run --no-agent` Callback-Only Mode (spec lines 1003–1005)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 21.1 | When `--no-agent` is passed, `rhei run` reverts to callback-only advancement (pre-agent behavior) | ✅ | `crates/rhei-cli/src/main.rs:7873` short-circuits to `callback_tasks` path |

---

## 22. Timeout Handling (spec §Timeout Handling, lines 781–929)

### 22A. Configuration & Resolution (lines 781–830)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 22A.1 | Per-state `agent_timeout` | ✅ | Resolution chain at `crates/rhei-cli/src/main.rs:6443` |
| 22A.2 | Per-model/agent binding `timeout` | ✅ | `crates/rhei-cli/src/main.rs:6447` |
| 22A.3 | Per-agent profile `timeout` | ✅ | `crates/rhei-cli/src/main.rs:6451` |
| 22A.4 | `defaults.agent_timeout` | ✅ | `crates/rhei-cli/src/main.rs:6454` |
| 22A.5 | Resolution precedence: state > model-agent binding > agent-profile > settings defaults | ✅ | Implemented in resolution order at `crates/rhei-cli/src/main.rs:6443–6455` |
| 22A.6 | Orchestrator authority: timeout must resolve to finite value; otherwise validation error | ✅ | `ensure_orchestrator_timeout` at `crates/rhei-cli/src/main.rs:6578` |
| 22A.7 | Worker authority: optional; engine does not impose | ✅ | `rhei next`/`rhei transition` paths do not enforce |

### 22B. Duration Format (lines 832–842)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 22B.1 | Supported units: `s`, `m`, `h` | ✅ | `parse_duration_secs` in `rhei-validator` (referenced from implementation notes) |
| 22B.2 | Combined units (`1h30m`, `2h15m30s`) | ✅ | Same parser |

### 22C. Timeout Behavior (lines 844–858)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 22C.1 | Send SIGTERM | ✅ | `crates/rhei-cli/src/main.rs:7354` |
| 22C.2 | 10-second grace period | ✅ | `AGENT_TERMINATE_GRACE = Duration::from_secs(10)` at `crates/rhei-cli/src/main.rs:7128–7131` |
| 22C.3 | Then SIGKILL if not exited | ✅ | `crates/rhei-cli/src/main.rs:7361` |
| 22C.4 | Log `agent timed out after {duration}` to the task log | 🟡 | Timeout is recorded in the log footer via exit code semantics, but the exact spec wording `agent timed out after {duration}` was not located. |
| 22C.5 | Look for timeout transition from current state | ✅ | `fire_timeout_transition` at `crates/rhei-cli/src/main.rs:9029–9042` |
| 22C.6 | Fire timeout transition with `on_leave`/`on_enter` callbacks | ✅ | `fire_timeout_transition` delegates to `execute_transition` |
| 22C.7 | If no timeout transition exists, task remains in state with warning | ✅ | Implicit — `fire_timeout_transition` no-ops; engine then logs the standard non-zero-exit error |
| 22C.8 | When snapshot capture enabled, timeout may produce `completion: timeout` snapshot, not preloadable by `snapshot.inherit:` | ❌ | Snapshots unimplemented in this surface. |

### 22D. Timeout Transitions (lines 860–913)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 22D.1 | Declared in `transitions` array with `timeout` field | ✅ | Validator schema |
| 22D.2 | `triggeredBy` set to `'system'` | ✅ | `execute_transition` callers thread `triggeredBy`; `fire_timeout_transition` selects timeout rules accordingly |
| 22D.3 | If `agent_timeout` set on state but no transition with `timeout` exists, agent is killed and task remains with warning | ✅ | Same as 22C.7 |

### 22E. Timeout Callbacks (lines 915–929)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 22E.1 | Timeout transitions support `on_leave`/`on_enter` callbacks | ✅ | `fire_timeout_transition` → `execute_transition` |
| 22E.2 | Callback receives `TransitionContext` with `triggeredBy: 'system'` and `transitionData.timeout = <duration>` | 🟡 | `triggeredBy: 'system'` confirmed by `fire_timeout_transition` path; `transitionData.timeout` payload not specifically verified. |

---

## 23. Related Specifications & cross-spec gates (spec §Related Specifications, lines 1007–1015)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 23.1 | Snapshot resolution prerequisite for legacy `model`/`all_models` (spec lines 314–318) | ❌ | Snapshot surface not in this implementation; gate missing. |

---

# Summary

## Coverage totals (across 153 normative line-items above)

| Status | Count |
|---|---|
| ✅ covered | ~110 |
| 🟡 partial | ~25 |
| ❌ missing | ~18 |
| ⚪ not-normative | ~7 |

## High-impact gaps (block spec conformance for real workloads)

1. **MCP/skill flag emission** (§3.7, §3.8, §3.10, §11.1, §11.2, §11.6, §15.9) — `mcp_flag`, `mcp_config_flag`, and `skill_flag` are parsed and present on every relevant built-in profile, **but `build_agent_command` never appends them to the spawn command line**. Resolved MCP servers and skills only reach the agent via env vars (`RHEI_MCP_SERVERS`, `RHEI_SKILLS`, `RHEI_MCP_<NAME>_AVAILABLE`, `RHEI_SKILL_<ID>_AVAILABLE`). For agents like `claude-code` and `codex`, this means MCP and skills declared in the state are advertised but not wired.
2. **MCP/skill availability ("Half B")** (§5.7, §15.2, §15.4, §15.5–15.9, §16.8–16.11) — no live MCP handshake or skill-path probe. `available` is just "id resolved against the registry."
3. **`mcp_unavailable` / `skill_unavailable` transitions** (§15.6, §15.7) — entire failure-routing path is unimplemented; no transition firing, no `transitionData.unavailable` payload, no error template.
4. **Independence rule in parallel mode** (§17C.2, §17C.8) — implementation uses a per-state concurrency filter rather than the transitive `**Prior:**` chain the spec defines. Tasks that are spec-dependent could in principle run concurrently across different states.
5. **Counted-loop log file naming** (§19.3, §19.5) — log suffix is the target/model slug, not the visit count. State machines that count visits (review loops) cannot follow the spec's per-visit log layout.
6. **Snapshot session integration** (§3.12, §7.1, §10B.7, §11.7, §11.8, §22C.8, §23.1) — `CustomAgentProfile.session`, the `snapshots` settings block, the `unsupported-snapshot-session` error, and the `completion: timeout` snapshot are all absent. The implementation notes treat this as out of scope, but the spec is normative about it.
7. **Validation: `agent_mode` set when agent has no modes** (§12.4) — silently accepted. Direct violation of spec line 476–478.
8. **Validation: `mcp_flag` vs `mcp_config_flag` mutual exclusion** (§3.9) — not enforced.
9. **Validation: MCP server `command` xor `url`** (§5.3) — not enforced.
10. **Post-spawn warning wording when outputs are missing** (§14C.5) — task does stay in state and a warning fires, but the spec's exact `required outputs are missing for task X in state 'Y': <name1>, <name2>` text (with the missing-name list) is not emitted. The substitute warning is less actionable.
11. **`codex` `yolo` mode flag set** (§11.2) — spec table at line 389 lists `-a never` (and the example settings block does too); the built-in entry omits it.
12. **`models.<id>.agents.<agent>.args` / `autonomous_args`** (§4.6, §4.7) — parsed, not consumed. Implementation notes call this a deferral pending an open spec question.

## Behaviorally minor divergences

- Dry-run text format diverges slightly from the spec's example (§20.3, §20.4) — wording differences only.
- Empty-list "clears defaults" semantics for state-level `mcp_servers` / `skills` (§10D.2) not verified.
- "Null clears" merge semantics (§9.8) not explicitly verified for all fields.
- Working-directory for spawned subprocess (§16.12) not verified end-to-end.
- Exact error wording at §10B.2 and §10B.3 diverges from spec template (template language is "error: …" so this may be intentional).
