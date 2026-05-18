# impl-rhei-agents — Implementation Notes

Spec: `docs/functional-spec/rhei-agents.spec.md`

The bulk of `rhei-agents.spec.md` was already implemented before this task
(settings loader, six built-in agents, agent/mode/timeout resolution, prompt
composition, env injection, parallel spawn loop, MCP/skill resolution, dry-run
output, timeout transitions, gating handling, log capture infrastructure). The
work in this task closes five concrete spec gaps and leaves the rest
unchanged.

## Coverage matrix

| Spec section | Implementation | Status |
|---|---|---|
| §Overview, §Completion Authority | `crates/rhei-cli/src/main.rs` `run_agent_mode`, `spawn_and_wait_agent`, `ensure_orchestrator_timeout` | preexisting |
| §Agent Configuration → `defaults` | `RheiSettings`, `SettingsDefaults` (`main.rs:5709`-`5765`) | preexisting |
| §Agent Configuration → `agents` registry, §Custom Agents, §Modes | `built_in_agents` (`main.rs:5763`), `CustomAgentProfile` in `rhei-validator` | preexisting; built-ins match spec table for `claude-code`, `codex`, `gemini`, `cursor`, `kilocode`, `pi` |
| §Agent Configuration → `models` registry | **new**: `ModelProfile`, `ModelAgentBinding` structs and `RheiSettings.models` (`main.rs:5736`-`5770`); merge in `load_merged_settings` | this task |
| §Agent Configuration → `mcp_servers`, `skills` | `McpServerProfile`, `SkillProfile`, `StateMcpEntry`, `StateSkillEntry`, `resolve_tooling`, `inject_tooling_env` | preexisting |
| §Per-State Settings, §Merge Semantics | `load_merged_settings`, `validate_machine_settings_references` | preexisting; extended for `models` merge |
| §Resolution Order (model + agent) | `resolve_legacy_agent_with_model`, `resolve_target_agent` (`main.rs:6244`-`6520`) — now also honors `models.<id>.default_agent` and `models.<id>.agents.<id>.timeout` | partially new (this task) |
| §Mode Resolution Order | `resolve_legacy_agent_with_model` mode block | preexisting |
| §Partial Overrides | composed `load_merged_settings` and resolution chains | preexisting |
| §Known Agent Profiles table | `built_in_agents` (6 entries with `yolo` modes, MCP / skill flags per spec) | preexisting |
| §Prompt Composition | `compose_agent_prompt` (`main.rs:~6750`) | preexisting |
| §Completion Condition + Runtime Semantics | `state_outputs_exist_for_resolved_invocation`, `try_auto_advance_task`, `fire_timeout_transition` | preexisting |
| §Environment Variables | `build_agent_command` env block — now sets `RHEI_MODEL_PROVIDER` from registry (not only target) and `RHEI_MODEL_NAME` from resolved concrete name | partially new (this task) |
| §`rhei run` Agent Mode CLI + Execution Loop | `RunOptions`, `run_agent_mode` | preexisting |
| §Interaction Between Agents and Callbacks | `run_agent_mode` + `execute_transition` | preexisting |
| §Gating States | `gating: true` short-circuit in `run_agent_mode` loop | preexisting |
| §Missing Tooling | `resolve_tooling`, `ResolvedMcpEntry.optional`, `inject_tooling_env`, `format_tooling_log_line` (Half A: registry resolution). Half B (live MCP handshake + skill-path probes) tracked separately. | preexisting (Half A) |
| §Timeout Handling — configuration / duration format | `parse_duration_secs` in `rhei-validator`, resolved at four levels in `resolve_legacy_agent_with_model` / `resolve_target_agent` | preexisting |
| §Timeout Behavior + Transitions + Callbacks | `spawn_and_wait_agent` (SIGTERM, 10 s grace, SIGKILL), `fire_timeout_transition` | preexisting |
| §Log Capture (file naming, header, footer) | `agent_log_path`, log header + footer in `spawn_and_wait_agent` — header now `=== rhei agent log v1 ===` and carries `provider:`, `model_name:`, `started:`/`ended:`, human-readable `duration:`/`timeout:` | partially new (this task) |
| §Dry-Run Output | dry-run branch in `run_agent_mode` | preexisting |
| §`rhei run --no-agent` | `run_callback_mode` | preexisting |

## Changes made in this task

1. **Model registry parsing** (`main.rs`)
   - Added `ModelProfile { provider, model, default_agent, agents }` and
     `ModelAgentBinding { args, autonomous_args, timeout }` (spec §`models`).
   - Added `RheiSettings.models: BTreeMap<String, ModelProfile>` so
     `~/.config/rhei/settings.json` and `.rhei/settings.json` can declare
     model profiles per spec.
   - `load_merged_settings` now merges `models` by id (global → project),
     matching the documented Merge Semantics.

2. **Concrete model name on `--model`**
   - `ResolvedAgent` carries new `model_provider: Option<String>` and
     `model_name: Option<String>` fields, populated from the resolved
     `ModelProfile` (or, for explicit targets, the target selector / model
     profile fallback).
   - `build_agent_command` now passes the concrete provider model name to the
     agent's `model_flag` (falls back to the rhei model id when no registry
     entry exists), per spec §`models` (`model` = concrete provider name).

3. **`RHEI_MODEL_PROVIDER` and `RHEI_MODEL_NAME` env vars** (spec §Environment
   Variables)
   - `RHEI_MODEL` continues to expose the rhei model profile id.
   - `RHEI_MODEL_PROVIDER` is now set from the resolved model profile (was
     only set from `ExecutionTarget`).
   - `RHEI_MODEL_NAME` is newly set from the resolved concrete model name.

4. **Resolution Order step 5: `models.<id>.default_agent`** (spec §Resolution
   Order)
   - `resolve_legacy_agent_with_model` now consults
     `models.<resolved-model>.default_agent` after global defaults when no
     agent was configured anywhere else.

5. **Per-model-agent binding timeout** (spec §Timeout Handling)
   - `resolve_legacy_agent_with_model` and `resolve_target_agent` now insert
     `models.<id>.agents.<agent>.timeout` between state-level and
     agent-profile timeouts in the four-level resolution chain.

6. **Log header v1** (spec §Log Capture / §Log Format)
   - Header line is now `=== rhei agent log v1 ===` (spec line 949).
   - Added `provider:`, `model_name:`, `started:` (ISO 8601 UTC) lines.
   - `timeout:` now renders as `30m`, `1h`, `2h30m`, … (was `1800s`).
   - Footer adds `ended:` and renders `duration:` in the same human-readable
     form (was `4m23s`-style required by spec).
   - Helpers `format_iso8601_utc`, `civil_from_days`, `format_duration_human`
     added (no `chrono`/`jiff` dependency — see Deferrals).

## Tests added

In `crates/rhei-cli/src/main.rs` (`tests` module):

- `resolve_legacy_agent_pulls_default_agent_from_model_registry` — verifies
  Resolution Order step 5 wiring.
- `resolve_legacy_agent_prefers_model_agent_binding_timeout` — covers the
  new per-model-agent timeout slot in the chain.
- `build_agent_command_uses_concrete_model_name_for_flag` — asserts
  `--model claude-sonnet-4-6` is passed when the rhei profile is `impl-fast`,
  and that `RHEI_MODEL` / `RHEI_MODEL_PROVIDER` / `RHEI_MODEL_NAME` are set.
- `build_agent_command_falls_back_to_model_id_when_registry_missing` —
  backward-compat fallback for unregistered model ids.
- `settings_parse_models_registry` — JSON deserialization of the spec's
  example `models` block.
- `format_iso8601_utc_renders_epoch_origin` and
  `format_iso8601_utc_renders_known_instant` — verify the date formatter.
- `format_duration_human_matches_spec_examples` — `0s`, `30s`, `5m`, `1h`,
  `2h30m`, `4m23s`.
- `agent_log_header_uses_v1_format_and_spec_fields` — spawns a quiet fake
  agent and asserts the v1 header + the new `provider:` / `model_name:` /
  `timeout:` (`30m`) / `started:` / `ended:` / `duration:` lines.

End-to-end coverage targeting the mock agent is handled by the shared
`e2e-aggregate` task per the workspace's [README](../../README.md) and is
intentionally out of scope here. The full per-spec pipeline (completeness
audit + 2× quality review/fix loop) runs after this state.

## Deferrals

- **MCP availability "Half B"** — live MCP handshake and skill-path
  existence probes were already deferred before this task. The struct shape
  (`ResolvedMcpEntry.optional`, `definition: Option<…>`,
  `RHEI_MCP_<NAME>_AVAILABLE`) is in place; the spawn-time probe loop is the
  remaining work. Tracked by inline comments in `main.rs` near
  `ResolvedTooling` and out of scope per the existing comment that "Half B
  will hook actual MCP handshake checks and skill-path probes into the same
  struct, leaving call sites unchanged".
- **`ModelAgentBinding.args` / `autonomous_args`** — the spec lists these
  fields (`models.<id>.agents.<id>.args`, `autonomous_args`) but does not
  describe a precise consumption point distinct from the agent profile's
  modes. The parser accepts them so settings.json files using them load
  cleanly; `rhei run` does not yet append them to the spawn command. Wiring
  them depends on resolving the open question of when "autonomous" applies
  (intended to map to `agent_mode = yolo`) which the spec leaves implicit.
  Marked `#[allow(dead_code)]` so the forward-compatible parse does not
  yellow the build.
- **`v1` log format formal versioning** — increment is implemented; no
  `v2` migration is needed yet. If a future change touches header/footer
  shape, bump the suffix per spec line 970.

## Build / test result

- `cargo build -p rhei-cli` → clean.
- `cargo test -p rhei-cli --bin rhei` → 57 / 57 pass.
- `cargo test` (workspace) → 73 / 74 pass. The single failing test —
  `e2e::run_tests::changeset_review_human_review_state_is_gating_in_shipped_workflows`
  — fails on `main` as well; it is unrelated to this task (verified by a
  `git stash` checkpoint while running the test in isolation).

## Spec-line cross-references for non-obvious calls

- Spec §Log Capture / `=== rhei agent log v1 ===` line 949 → `main.rs`
  log-header writer in `spawn_and_wait_agent`.
- Spec §Resolution Order step 5 / line 289 → `model_profile.and_then(|p|
  p.default_agent.clone())` block in `resolve_legacy_agent_with_model`.
- Spec §Environment Variables / lines 624-636 → `build_agent_command` env
  block (`RHEI_MODEL`, `RHEI_MODEL_PROVIDER`, `RHEI_MODEL_NAME`, etc.).
- Spec §`models` / line 181 ("concrete provider model name such as `o3` or
  `claude-sonnet-4-6`") → `model_flag_value` in `build_agent_command`.
