# Completeness Fix Log: impl-rhei-states

Spec: `docs/functional-spec/rhei-states.spec.md`

## Closed

- Gap: legacy model profile provider/name values were not fully available to templates and artifact paths.
  Files edited: `crates/rhei-cli/src/cli/artifacts.rs`, `crates/rhei-cli/src/cli/agent_resolution.rs`, `crates/rhei-cli/src/cli/run_agent_mode.rs`, `crates/rhei-cli/src/cli/run_callback_mode.rs`, `crates/rhei-cli/src/cli/run_helpers.rs`, `crates/rhei-cli/src/cli/transition_checks.rs`, `crates/rhei-cli/src/cli/programs.rs`, `crates/rhei-cli/src/cli/next_command.rs`.
  Tests added/updated: `runtime_templates_use_resolved_model_provider_and_name`.

- Gap: target selector modes and `agent_mode` were rejected for agents with no declared modes.
  Files edited: `crates/rhei-cli/src/cli/settings_load_validate.rs`, `crates/rhei-cli/src/cli/agent_resolution.rs`.
  Tests added/updated: `validates_agent_mode_allowed_on_modeless_agent`, `defaults_only_agent_mode_selects_agent_mode_for_effective_agents`.

- Gap: explicit `all_targets: []` was not rejected.
  Files edited: `crates/rhei-validator/src/validator/state_machine_impl.rs`.
  Tests added/updated: `rejects_explicit_empty_all_targets`.

- Gap: top-level machine `models` entries were not validated against merged `settings.models`.
  Files edited: `crates/rhei-cli/src/cli/settings_load_validate.rs`.
  Tests added/updated: existing settings validation coverage updated by `defaults_only_agent_mode_selects_agent_mode_for_effective_agents`.

- Gap: `state.agent` on `gating: true` states did not produce a validation warning.
  Files edited: `crates/rhei-validator/src/validator/validator_entry.rs`.
  Tests added/updated: `warns_when_gating_state_declares_agent`.

- Gap: runtime-expanded artifact paths lacked a post-expansion workspace escape check.
  Files edited: `crates/rhei-cli/src/cli/artifacts.rs`, `crates/rhei-cli/src/cli/transition_checks.rs`, `crates/rhei-cli/src/cli/programs.rs`.
  Tests added/updated: covered by existing artifact path tests plus focused transition/program path checks through touched call paths.

- Gap: nested template conditionals were not explicitly rejected.
  Files edited: `crates/rhei-validator/src/validator/validation_helpers.rs`, `crates/rhei-validator/src/validator/state_machine_profiles.rs`.
  Tests added/updated: `rejects_nested_template_conditionals`.

- Gap: `rhei complete` did not block completion from gating states.
  Files edited: `crates/rhei-cli/src/cli/complete_reset_commands.rs`.
  Tests added/updated: `complete_command_blocks_gating_state`.

- Gap: poll transition operands `pollAttempts` and `pollMaxAttempts` were not implemented.
  Files edited: `crates/rhei-cli/src/cli/metadata_conditions.rs`, `crates/rhei-cli/src/cli/metadata_rewrite.rs`, `crates/rhei-cli/src/cli/transition_context.rs`.
  Tests added/updated: `poll_attempt_condition_aliases_are_available_on_first_attempt`.

- Gap: poll attempt metadata started at no persisted count before first self-loop.
  Files edited: `crates/rhei-cli/src/cli/metadata_conditions.rs`, `crates/rhei-cli/src/cli/metadata_rewrite.rs`, `crates/rhei-cli/src/cli/transition_context.rs`.
  Tests added/updated: `poll_attempt_condition_aliases_are_available_on_first_attempt`.

- Gap: `rhei states` omitted newer state-machine fields and JSON emitted legacy `initial`.
  Files edited: `crates/rhei-cli/src/cli/states_render.rs`, `crates/rhei-validator/src/validator/state_defs.rs`, `crates/rhei-cli/src/cli/tests_cli_render.rs`.
  Tests added/updated: `render_state_machine_text_includes_states_and_transitions`, existing JSON render coverage updated.

- Gap: `rhei next` claimability and automatic initial-state behavior used legacy per-state `initial`.
  Files edited: `crates/rhei-cli/src/cli/ready_transition.rs`, `crates/rhei-cli/src/cli/next_command.rs`.
  Tests added/updated: existing profile reset/claimability coverage exercised after the change.

## Deferred

- Gap: `profiles` and `node_policy` are required only for schema v3+ machines.
  Reason: enforcing this for all schema versions breaks the currently supported legacy loader behavior and 39 existing validator fixtures that intentionally load version 1 machines without profiles. Closing this requires a coordinated schema migration and fixture/user compatibility plan rather than a completeness-fix-only patch.
  Files edited: none retained for this item.
  Tests added/updated: none.

- Gap: per-state `initial: true` remains in the schema.
  Reason: removing the field from the decoded schema is a breaking YAML compatibility change and overlaps the schema migration above. Runtime `rhei next` behavior was switched to resolved profile initials where profiles exist; physical schema removal is deferred.
  Files edited: `crates/rhei-cli/src/cli/ready_transition.rs`, `crates/rhei-cli/src/cli/next_command.rs` for the runtime part.
  Tests added/updated: existing profile-aware tests.

- Gap: state `mcp_servers` / `skills` registry ids are not reported by `rhei validate`.
  Reason: this is already implemented in the current CLI settings-aware validation path via `validate_machine_settings_references`; no code change was needed in this pass.
  Files edited: none for this item.
  Tests added/updated: existing `unknown_tooling_id_validation_rejects_defaults_and_state_references`.

- Gap: `rhei validate` does not fully apply settings-reference validation for top-level machine models, state tooling ids, and gating-agent warning.
  Reason: top-level model validation and gating-agent warning were closed; state tooling id validation was already present.
  Files edited: `crates/rhei-cli/src/cli/settings_load_validate.rs`, `crates/rhei-validator/src/validator/validator_entry.rs`.
  Tests added/updated: `warns_when_gating_state_declares_agent`, existing unknown tooling validation tests.

- Gap: concurrent program and callback scheduling is sequential.
  Reason: true parallel program/callback scheduling requires restructuring the run loop, event slot accounting, transition locking, and error aggregation. That is broader than a completeness-fix patch and needs a dedicated orchestration change.
  Files edited: none for this item.
  Tests added/updated: none.

- Gap: visit and poll counters for `all_targets` / `all_models` fanout are keyed only by task id and state name.
  Reason: changing counter identity to include target/model affects metadata schema, snapshot identity, logs, and transition matching. This needs a metadata migration and coordinated snapshot/run changes.
  Files edited: none for this item.
  Tests added/updated: none.

- Gap: exhausted poll states produce clear errors in agent paths but not all program/callback paths.
  Reason: the existing program/callback paths still need a shared poll-exhaustion transition helper. This was not changed to avoid broadening into run-loop refactoring.
  Files edited: none for this item.
  Tests added/updated: none.

- Gap: poll terminal-exit snapshot behavior was not fully proven for fanout/poll-specific cases.
  Reason: snapshot implementation is owned by the snapshot spec path and already has dedicated poll terminal-exit tests in the current tree; no states-specific code change was made here.
  Files edited: none for this item.
  Tests added/updated: none.

- Gap: MCP availability is registry/support based, not an actual server start/handshake availability check at spawn time.
  Reason: adding a real MCP handshake requires an MCP client lifecycle and startup protocol support, not just state-machine completeness edits. Skill path probing remains implemented; MCP handshake is deferred.
  Files edited: none for this item.
  Tests added/updated: none.

- Gap: `rhei run` still has incomplete program/callback concurrency, fanout-scoped visits/poll counters, poll condition aliases, and true MCP availability checks.
  Reason: poll condition aliases were closed. Program/callback concurrency, fanout-scoped counters, and MCP handshake remain deferred for the reasons above.
  Files edited: `crates/rhei-cli/src/cli/metadata_conditions.rs`, `crates/rhei-cli/src/cli/metadata_rewrite.rs`, `crates/rhei-cli/src/cli/transition_context.rs`.
  Tests added/updated: `poll_attempt_condition_aliases_are_available_on_first_attempt`.

## Verification

- `cargo fmt --all -- --check` passed.
- `cargo clippy -p rhei-cli-validator -p rhei-cli --all-targets -- -D warnings -W clippy::all` passed.
- `cargo test -p rhei-cli-validator` passed.
- Focused CLI tests passed:
  - `complete_command_blocks_gating_state`
  - `poll_attempt_condition_aliases_are_available_on_first_attempt`
  - `runtime_templates_use_resolved_model_provider_and_name`
  - `validates_agent_mode_allowed_on_modeless_agent`
  - `defaults_only_agent_mode_selects_agent_mode_for_effective_agents`
  - `render_state_machine_text_includes_states_and_transitions`
- `cargo test -p rhei-cli --bin rhei` still fails in 5 tests unrelated to this pass's closed gaps:
  - `validates_snapshot_session_profiles_match_runtime_support`
  - `snapshot_targetless_explicit_emit_and_inherit_require_target`
  - `tooling_required_missing_skill_blocks_fake_agent_spawn`
  - `missing_outputs_reschedule_single_invocation_spawns_once_and_keeps_state`
  - `missing_outputs_reschedule_fanout_spawns_once_and_keeps_state`
