# Scope Inventory: Run Orchestration, Agents, Programs, and Callbacks

Partition task: `run-orchestration-agents-programs`

This inventory covers autonomous execution semantics for `rhei run`: mode selection, ready-set selection, scheduling, agent and program subprocess execution, completion authority, output artifact enforcement, settings and registry resolution, target selectors, timeouts, callbacks, and failure routing.

## In Scope

Specification files:

- `docs/specs/rhei-run.spec.md`
- `docs/specs/rhei-agents.spec.md`
- `docs/specs/rhei-programs.spec.md`
- `docs/specs/rhei-callbacks.spec.md`
- `docs/specs/rhei-usage.spec.md` sections: `Roles`, `Coordination Through the State Machine`, `Command Surface`, `Pattern 0`, `Pattern 3`, `Pattern 4`, `Pattern 5`, `Pattern 6`, `Pattern 7`, `Pattern 8`, `Pattern 9`, `Pattern 10`

Implementation roots:

- `crates`
- `skills`
- `.agents/rhei/templates`
- `examples`

Adjacent specs that are not primary partition inputs but define referenced contracts:

- `docs/specs/rhei-states.spec.md` for state fields, artifact contracts, template variables, `concurrent`, `poll`, `target`, `all_targets`, `model`, `all_models`, `agent`, `agent_mode`, `agent_timeout`, `program`, and `program_timeout`.
- `docs/specs/rhei-transitions.spec.md` for transition conditions, `on_leave`, `on_enter`, timeout transitions, tooling-unavailable triggers, `TransitionContext`, and `TransitionResult`.
- `docs/specs/rhei-next.spec.md` for the claimability rule referenced by `rhei run`.
- `docs/specs/rhei-run-tui.spec.md` only for `runtime/transitions.log` / slot journal requirements referenced by `rhei-run.spec.md`.

## Shared Implementation Surfaces

Primary CLI and runtime:

- `crates/rhei-cli/src/main.rs`
  - Command definitions: `Commands::Run`, `StandaloneExecutionFlags`, `AgentExecutionFlags`, `ProgramExecutionFlags`, `RunOptions`
  - Entrypoints: `run_command`, `run_agent_mode`, `run_callback_mode`
  - Ready and progress selection: `find_ready_tasks`, `dependency_is_satisfied`, `find_claimable_tasks`, `diagnose_no_claimable`, `state_declares_autonomous_execution`, `find_next_transition`, `try_auto_advance_task`
  - Transition execution: `execute_transition`, `transition_rule_is_applicable`, `evaluate_transition_condition`, `fire_timeout_transition`, `resolve_callback_paths`, `build_transition_context_json`, `merge_transition_data`
  - Artifact checks: `ensure_state_inputs_exist`, `ensure_state_outputs_exist`, `ensure_state_inputs_exist_for_transition`, `ensure_state_outputs_exist_for_transition`, `state_outputs_exist_for_resolved_invocation`, `task_has_pending_agent_invocations`
  - Settings and resolution: `RheiSettings`, `SettingsDefaults`, `built_in_agents`, `load_settings`, `load_merged_settings`, `validate_machine_settings_references`, `resolve_tooling`, `effective_mcp_entries`, `effective_skill_entries`, `resolve_mcp_entry`, `resolve_skill_entry`
  - Target and agent resolution: `slugify_target_value`, `resolve_target_agent`, `resolve_legacy_agent_with_model`, `resolve_agent_invocations`, `resolve_agent`, `transition_contexts_for_state`, `callback_contexts_for_state`, `ensure_orchestrator_timeout`, `resolved_agent_log_suffix`
  - Agent prompt and spawn: `compose_agent_prompt`, `build_agent_command`, `inject_tooling_env`, `agent_log_path`, `spawn_agent_output_reader`, `drain_agent_output_reader`, `spawn_and_wait_agent`
  - Program spawn and routing: `ProgramSpec`, `ProgramCommand`, `ResolvedProgram`, `parse_program_spec`, `resolve_program`, `program_log_path`, `build_program_command`, `spawn_and_wait_program`, `transition_matches_exit_code`, `find_program_exit_transition`
  - Runtime templating: `RuntimeTemplateContext`, `resolve_runtime_template_text`, `resolve_runtime_template_variable`, `process_conditional_blocks`, `evaluate_if_condition`, `resolve_artifact_path`, `artifact_relative_path`

State-machine and validation surfaces:

- `crates/rhei-validator/src/lib.rs`
  - Data structures: `StateMachine`, `StateDef`, `StateArtifactDef`, `PollConfig`, `AgentConfig`, `CustomAgentProfile`, `McpServerProfile`, `SkillProfile`, `StateMcpEntry`, `StateSkillEntry`, `ExecutionTarget`
  - Target parsing: `parse_execution_target`, `ExecutionTarget::{selector,slug}`
  - Validation methods: `StateMachine::from_yaml_str`, `StateMachine::from_yaml_file`, `StateMachine::validate_model_configuration`, `StateMachine::validate_program_configuration`, `StateMachine::validate_poll_configuration`
  - Validation helpers: `validate_program_value`, `validate_program_command`, `validate_artifact_definitions`, `validate_state_mcp_entries`, `validate_state_skill_entries`, `validate_transition_tooling_trigger`
  - Plan validation: `validate_with_machine`, `validate_with_machine_and_base`
- `crates/rhei-core/src/ast.rs`
  - `TransitionRule::{from,to,on_leave,on_enter,condition,timeout,exit_code,mcp_unavailable,skill_unavailable}`
  - `CallbackRef`, `Task`, `Rhei`, `StateName`
- `crates/rhei-core/src/callback.rs`
  - `CallbackContext`, `CallbackResult`, `CallbackExecutor`, `ShellCallbackExecutor`, `NoopCallbackExecutor`, `parse_callback_stdout`
- `crates/rhei-core/src/parser.rs`
  - parsing of plan tasks, frontmatter runtime metadata, `stateVisits`, escaped state names
- `crates/rhei-core/src/workspace.rs`
  - `is_workspace`, `load_workspace`
- `crates/rhei-tui/src/event.rs`
  - `RunEvent::{RunStarted,PassStarted,SlotAssigned,AgentOutput,SlotReleased,PassEnded,RunFinished}`, `TaskOutcome`
- `crates/rhei-tui/src/journal.rs`
  - `JournalSink`, `runtime/transitions.log` append behavior
- `crates/rhei-napi/src/lib.rs`
  - NAPI surface currently present for callback-language claims that mention TypeScript bindings

Core test surfaces:

- `crates/rhei-cli/src/main.rs` unit tests:
  - `parses_run_command_with_separated_flag_groups`
  - `run_help_separates_standalone_and_agent_flags`
  - `compose_agent_prompt_carries_domain_instructions_only`
  - `resolve_tooling_unions_defaults_with_state_overrides_by_id`
  - `resolve_tooling_empty_state_list_clears_defaults`
  - `resolve_tooling_omitted_state_inherits_defaults`
  - `resolve_tooling_inline_definition_does_not_require_registry`
  - `resolve_tooling_unknown_id_resolves_to_unavailable`
  - `env_id_segment_normalizes_id`
  - `format_tooling_log_line_marks_unavailable_optional_with_question_mark`
  - `resolve_legacy_agent_uses_defaults_agent_timeout`
  - `built_in_codex_command_omits_removed_approval_flag`
  - `output_reader_logs_and_emits_complete_and_partial_lines`
  - `supported_agents_keep_expected_prompt_transports`
  - `fake_claude_profile_streams_prompt_flag_output`
  - `fake_codex_profile_streams_stdin_prompt_output`
  - `fake_pi_profile_streams_prompt_flag_output`
  - `fake_agent_timeout_keeps_output_and_writes_footer`
  - `inherited_output_pipe_does_not_block_agent_completion`
- `crates/rhei-cli/tests/e2e/run_tests.rs`
  - `run_single_file_linear_to_completion`
  - `run_single_file_parallel_to_completion`
  - `run_single_file_independent_to_completion`
  - `run_workspace_linear_to_completion`
  - `run_workspace_parallel_to_completion`
  - `run_bash_agent_team_fixture_to_completion`
  - `run_living_review_loop_fixture_to_completion`
  - `run_executes_program_states_and_routes_on_exit_code`
  - `run_callback_mode_stops_at_human_review`
  - `run_callback_mode_waits_for_other_branches_before_halting_at_human_review`
  - `changeset_review_human_review_state_is_gating_in_shipped_workflows`
  - `run_prefers_agent_mode_for_model_declared_workflows_without_falling_back_to_callbacks`
  - `run_partially_advanced_completes_remaining`
  - `run_already_completed_is_noop`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - Callback tests: `callback_on_leave_and_on_enter_invoked_on_transition`, `callback_on_leave_failure_blocks_transition`, `no_callbacks_flag_skips_callback_execution`, `callback_unknown_platform_produces_clear_error`, `callback_rejection_surfaces_spec_error_message`, `callback_redirect_via_next_state_retargets_declared_transition`, `callback_redirect_to_undeclared_transition_is_rejected`, `callback_receives_transition_context_on_stdin`, `callback_on_enter_failure_rolls_back_state_write`
  - Run tests: `run_advances_linear_chain_to_completion`, `run_advances_parallel_ready_tasks`, `run_uses_counted_loop_exit_when_visit_budget_is_exhausted`, `run_dry_run_shows_transitions_without_changes`, `run_callback_failure_halts_execution`, `run_executes_relative_callback_from_state_machine_directory`, `run_executes_all_models_callbacks_without_agent_configuration`, `run_skips_already_completed_tasks`, `run_no_callbacks_flag_skips_callbacks`, `workspace_run_advances_tasks_to_completion`
- `crates/rhei-cli/tests/e2e/next_tests.rs`
  - `next_does_not_auto_transition_runnable_initial_states`
  - `complete_fails_when_required_output_artifact_is_missing`
- `crates/rhei-cli/tests/e2e/transition_tests.rs`
  - `transition_fails_when_target_state_input_artifact_is_missing`
- `crates/rhei-validator/src/lib.rs` unit tests:
  - Model/target tests: `loads_state_machine_with_models_and_state_selectors`, `rejects_state_machine_with_unknown_state_model`, `rejects_state_machine_with_conflicting_state_model_selectors`, `rejects_state_machine_with_unknown_all_models_entry`, `parses_execution_target_with_mode_and_provider`, `loads_state_machine_with_target_selectors`, `rejects_state_machine_with_conflicting_target_and_model_selectors`
  - Program tests: `rejects_program_on_gating_state`, `rejects_exit_code_transition_from_non_program_state`
  - Tooling tests: `state_mcp_servers_accepts_string_and_object_forms`, `state_mcp_servers_empty_list_preserved_as_clear_marker`, `state_mcp_servers_rejects_duplicate_ids`, `state_mcp_servers_rejects_both_command_and_url`, `state_mcp_servers_rejected_on_gating_state`, `state_mcp_servers_rejected_on_program_state`, `state_skills_rejected_on_terminal_state`, `template_condition_accepts_mcp_and_skill_when_declared`, `template_condition_rejects_mcp_not_declared`, `transition_mcp_unavailable_accepts_true_and_list`, `transition_mcp_unavailable_rejects_false`, `transition_mcp_unavailable_rejects_on_program_state`
  - Polling tests: `accepts_well_formed_poll_state`, `rejects_poll_with_invalid_interval`, `rejects_poll_with_zero_max_attempts`, `rejects_poll_with_visits`, `rejects_poll_on_gating_state`, `rejects_poll_without_self_loop`
- `crates/rhei-core/src/callback.rs` unit tests around shell callbacks, JSON parsing, rejection, stdout/stderr capture, stdin delivery, and no-op callbacks.
- `crates/rhei-tui/src/journal.rs` tests `writes_assigned_and_released_lines` and `appends_on_second_open`.

Fixture, template, skill, and example surfaces:

- `crates/rhei-cli/tests/e2e/fixtures/bash-agent-team/*`
- `crates/rhei-cli/tests/e2e/fixtures/living-review-loop/*`
- `.agents/rhei/templates/multi-model-analysis/*`
- `.agents/rhei/templates/changeset-review/*`
- `.agents/rhei/templates/hourly-human-intervention/*`
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/*`
- `.agents/rhei/templates/spec-review/*`
- `examples/changeset-review-example/*`
- `examples/hourly-human-intervention-example/*`
- `examples/living-review-loop/*`
- `examples/review-fix-visits/*`
- `examples/ci-heal/*`
- `examples/spec-implementation-discrepancy-audit-example/*`
- `examples/claude-code/*`
- `examples/human-review-loop.rhei.md`
- `examples/release-automation.rhei.md`
- `skills/rhei-plan-worker/SKILL.md`
- `skills/rhei-plan-writer/SKILL.md`
- `skills/rhei-plan-writer/references/default-states.md`
- `skills/rhei-state-machine-writer/SKILL.md`
- `skills/rhei-template-writer/SKILL.md`

## Normative Claim Inventory

### RO-001: `rhei run` Command Surface and Flag Groups

Spec sections:

- `docs/specs/rhei-run.spec.md` -> `Usage`, `Options`
- `docs/specs/rhei-agents.spec.md` -> ``rhei run` - Agent Mode` / `CLI`
- `docs/specs/rhei-programs.spec.md` -> ``rhei run` Integration` / `Flags`

Normative claims:

- User command is `rhei run <RHEI_PLAN_OR_WORKSPACE> [flags]`.
- Standalone flags are `--dry-run`, `--no-callbacks`, `--continue-on-error`, `--parallel <N>`, `--tui`, and `--no-tui`.
- Agent flags are `--no-agent`, `--agent <AGENT>`, `--agent-mode <MODE>`, and `--model <MODEL>`.
- Program flags are `--no-program` and `--program-timeout <DURATION>`.
- `--parallel 0` means unlimited.
- `--no-callbacks`, `--no-agent`, and `--no-program` combine independently.
- `--no-agent` does not suppress programs; `--no-program` does not suppress agents.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::Commands::Run`
- `crates/rhei-cli/src/main.rs::StandaloneExecutionFlags`
- `crates/rhei-cli/src/main.rs::AgentExecutionFlags`
- `crates/rhei-cli/src/main.rs::ProgramExecutionFlags`
- `crates/rhei-cli/src/main.rs::RunOptions`
- `crates/rhei-cli/src/main.rs::run_command`
- Unit tests `parses_run_command_with_separated_flag_groups`, `run_help_separates_standalone_and_agent_flags`
- E2E tests using `run_cli("run", ...)` in `crates/rhei-cli/tests/e2e/run_tests.rs`

### RO-002: Orchestrated Mode Selection vs Callback-Only Mode

Spec sections:

- `docs/specs/rhei-run.spec.md` -> `Execution Loop`
- `docs/specs/rhei-agents.spec.md` -> `Overview`, ``rhei run --no-agent` - Callback-Only Mode`
- `docs/specs/rhei-programs.spec.md` -> ``rhei run` Integration` / `Execution Loop`

Normative claims:

- `rhei run` uses orchestrated subprocess execution whenever any reachable non-terminal, non-gating state declares autonomous work through `program`, `agent`, `target`, `all_targets`, `model`, or `all_models`.
- Callback-only advancement is used only when no such state exists, or when the caller disables spawning with `--no-agent` and/or `--no-program`.
- A state that declares model/target-driven work but lacks a resolvable agent transport must fail with a missing-agent configuration error, not silently fall back to callback-only transition.
- Program states take priority over agent/default-agent spawning when a state declares `program`.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::state_declares_autonomous_execution`
- `crates/rhei-cli/src/main.rs::run_command`
- `crates/rhei-cli/src/main.rs::run_agent_mode`
- `crates/rhei-cli/src/main.rs::run_callback_mode`
- `crates/rhei-cli/src/main.rs::resolve_program`
- `crates/rhei-cli/src/main.rs::resolve_agent_invocations`
- E2E `run_prefers_agent_mode_for_model_declared_workflows_without_falling_back_to_callbacks`
- E2E `run_executes_program_states_and_routes_on_exit_code`
- Integration `run_executes_all_models_callbacks_without_agent_configuration`
- Integration `run_no_callbacks_flag_skips_callbacks`

### RO-003: Ready Set, Dependencies, Gating Exclusion, Inputs, and Poll Exclusion

Spec sections:

- `docs/specs/rhei-run.spec.md` -> `Execution Loop`, `Polling States`
- `docs/specs/rhei-agents.spec.md` -> `Execution Loop`
- `docs/specs/rhei-usage.spec.md` -> `Roles`, `Pattern 3`, `Pattern 4`, `Pattern 5`

Normative claims:

- Each pass computes a ready set from tasks whose `**Prior:**` dependencies are all terminal, whose current state is non-terminal and non-gating, and whose required current-state `inputs:` artifacts exist.
- Tasks in gating states are skipped by autonomous execution.
- Tasks blocked behind gating dependencies do not become ready.
- Polling states with future `metadata.tasks.<id>.pollNextAttemptAt.<state-name>` are excluded until the wall-clock deadline.
- A self-loop on a polling state persists `pollNextAttemptAt` and attempt counters, releases the `--parallel` slot, and is retried later.
- Once `stateVisits.<state-name>` reaches `poll.max_attempts`, self-loop selection is refused and the first matching non-self-loop is selected; if none matches, the task failure follows `--continue-on-error`.
- A non-self-loop exit clears poll deadline and visit metadata.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::find_ready_tasks`
- `crates/rhei-cli/src/main.rs::dependency_is_satisfied`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist_for_transition`
- `crates/rhei-cli/src/main.rs::find_next_transition`
- `crates/rhei-cli/src/main.rs::transition_rule_is_applicable`
- `crates/rhei-cli/src/main.rs::loop_reentry_allowed`
- `crates/rhei-cli/src/main.rs::update_metadata_for_transition`
- `crates/rhei-cli/src/main.rs::clear_runtime_state_visits`
- `crates/rhei-validator/src/lib.rs::PollConfig`
- `crates/rhei-validator/src/lib.rs::StateMachine::validate_poll_configuration`
- Validator polling unit tests listed above
- E2E `run_callback_mode_stops_at_human_review`
- E2E `run_callback_mode_waits_for_other_branches_before_halting_at_human_review`
- Example `examples/ci-heal/states.yaml`
- Example `examples/ci-heal/index.rhei.md`

### RO-004: Parallel Scheduling, Concurrent States, Fanout, and File Safety

Spec sections:

- `docs/specs/rhei-run.spec.md` -> `Execution Loop`, `Parallel Execution`, `Concurrent vs. Serial States`
- `docs/specs/rhei-agents.spec.md` -> `Execution Loop` / `Parallel Mode`
- `docs/specs/rhei-usage.spec.md` -> `Pattern 3`, `Pattern 3b`, `Pattern 7`

Normative claims:

- Up to `--parallel N` subprocesses run concurrently; `N = 0` means all eligible independent work.
- For states with `concurrent: false` or omitted, at most one ready task in that state is scheduled per pass.
- For states with `concurrent: true`, any number of ready tasks may be scheduled together, bounded by `--parallel`.
- The `concurrent` flag does not change transition semantics.
- Within-task fanout through `all_targets` or `all_models` stays together; every resolved invocation for one scheduled task is spawned together.
- Tasks already in flight must not be scheduled again.
- Single-file plans requested with `--parallel > 1` fall back to sequential execution with a warning.
- Directory workspaces support parallel work by isolating tasks in separate files.
- State writes are serialized through file locks so concurrent completions do not corrupt the plan.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::run_command` effective parallel calculation
- `crates/rhei-cli/src/main.rs::run_agent_mode` scheduling loop and non-concurrent filtering
- `crates/rhei-cli/src/main.rs::execute_transition` file locks and compare-and-swap
- `crates/rhei-core/src/workspace.rs::is_workspace`
- `crates/rhei-core/src/workspace.rs::load_workspace`
- `crates/rhei-tui/src/event.rs::RunEvent::{SlotAssigned,SlotReleased}`
- `crates/rhei-tui/src/journal.rs::JournalSink`
- E2E `run_workspace_parallel_to_completion`
- E2E `run_living_review_loop_fixture_to_completion`
- Integration `workspace_run_advances_tasks_to_completion`
- Integration `run_advances_parallel_ready_tasks`
- Template `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml` with `concurrent: true`
- Template `.agents/rhei/templates/multi-model-analysis/states.yaml`
- Example `examples/changeset-review-example/states.yaml`
- Example `examples/living-review-loop/team-states.yaml`

### RO-005: Subprocess Completion Authority and State Mutation Ownership

Spec sections:

- `docs/specs/rhei-run.spec.md` -> opening paragraph, `Execution Loop`, `Relationship to Other Commands`
- `docs/specs/rhei-agents.spec.md` -> `Completion Authority`, `Completion Condition`, `Runtime Semantics`
- `docs/specs/rhei-usage.spec.md` -> `Roles`, `Command Surface`, `Pattern 3`, `Pattern 5`

Normative claims:

- Under `orchestrator` authority, the spawned agent or program owns the work; `rhei run` owns the transition.
- Spawned agents must not call `rhei transition` or `rhei complete`, and must not edit `**State:**` lines directly, except for nested independent executions.
- `rhei run` evaluates completion after subprocess exit and executes the matching transition itself.
- Manual worker flow and `rhei run` are mutually exclusive per task execution.
- If an external actor changed the task state while the subprocess was running, `rhei run` respects the authoritative plan state and skips normal auto-advance for that run result.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::compose_agent_prompt`
- `crates/rhei-cli/src/main.rs::spawn_and_wait_agent`
- `crates/rhei-cli/src/main.rs::spawn_and_wait_program`
- `crates/rhei-cli/src/main.rs::run_agent_mode` post-spawn re-read and `state_after` checks
- `crates/rhei-cli/src/main.rs::try_auto_advance_task`
- `crates/rhei-cli/src/main.rs::execute_transition`
- Unit `compose_agent_prompt_carries_domain_instructions_only`
- E2E and integration run tests listed above
- `skills/rhei-plan-worker/SKILL.md` section noting this skill is distinct from `rhei run`
- `skills/rhei-state-machine-writer/SKILL.md` guidance that machine-critical decisions should be structural, not prose-only

### RO-006: Output Artifact Enforcement and Completion Condition

Spec sections:

- `docs/specs/rhei-run.spec.md` -> `Execution Loop`
- `docs/specs/rhei-agents.spec.md` -> `Completion Condition`, `Runtime Semantics`, `Prompt Composition`
- `docs/specs/rhei-programs.spec.md` -> `Artifact Contracts`

Normative claims:

- For agent states under orchestrator authority, successful completion requires subprocess exit code `0` and every required `outputs:` artifact to exist on disk.
- If no outputs are declared, exit code `0` suffices for agent completion.
- If an agent exits `0` but required outputs are missing, the task stays in its current state, a warning is logged, and no transition fires.
- For program states, required `inputs` are checked before spawn, and required `outputs` are checked after program exit and before exit-code transition commit.
- A zero-exit program with missing required outputs leaves the task in its current state.
- Artifact paths and prompt instructions can use `{input.<name>.path}` / `{output.<name>.path}` runtime variables.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist`
- `crates/rhei-cli/src/main.rs::ensure_state_outputs_exist`
- `crates/rhei-cli/src/main.rs::state_outputs_exist_for_resolved_invocation`
- `crates/rhei-cli/src/main.rs::task_has_pending_agent_invocations`
- `crates/rhei-cli/src/main.rs::ensure_state_outputs_exist_for_transition`
- `crates/rhei-cli/src/main.rs::resolve_runtime_template_variable`
- `crates/rhei-cli/src/main.rs::resolve_artifact_path`
- `crates/rhei-validator/src/lib.rs::StateArtifactDef`
- `crates/rhei-validator/src/lib.rs::validate_artifact_definitions`
- E2E `complete_fails_when_required_output_artifact_is_missing`
- E2E `transition_fails_when_target_state_input_artifact_is_missing`
- E2E `run_bash_agent_team_fixture_to_completion`
- E2E `run_living_review_loop_fixture_to_completion`
- Example `examples/changeset-review-example/states.yaml`
- Example `examples/hourly-human-intervention-example/states.yaml`
- Template `.agents/rhei/templates/multi-model-analysis/states.yaml`
- Template `.agents/rhei/templates/changeset-review/states.yaml`

### RO-007: Settings Schema, Merge Order, Agent Registry, and Tooling Resolution

Spec sections:

- `docs/specs/rhei-agents.spec.md` -> `Agent Configuration`, `Global and Project Settings`, `defaults`, `agents`, `models`, `mcp_servers`, `skills`, `Merge Semantics`, `Resolution Order`, `Partial Overrides`, `Known Agent Profiles`, `Custom Agents`, `Modes`, `Missing Tooling`
- `docs/specs/rhei-usage.spec.md` -> `Pattern 0`

Normative claims:

- Settings live at `~/.config/rhei/settings.json` and `.rhei/settings.json`; project settings compose with global settings by key.
- Built-in agents load first, global settings compose over them, then project settings compose over the result.
- `defaults` shallow-override by field.
- `agents` merge by id, and a user entry replacing a built-in or global id replaces that whole agent entry without field-level merging.
- `models`, `models.<id>.agents`, `mcp_servers`, and `skills` merge by id.
- `defaults.mcp_servers` and `defaults.skills` are replaced wholesale by project lists when present; an empty list clears inherited defaults.
- Agents and defaults reference agent ids in the merged `agents` registry; inline agent definitions are rejected.
- Built-in agent ids include `claude-code`, `codex`, `gemini`, `cursor`, `kilocode`, and `pi`.
- Built-in agent profiles define command, prompt transport, model flag, MCP/skill support, and `yolo` mode flags.
- Custom agents must be declared in the `agents` registry, not inline on states or `defaults.agent`.
- Tooling effective set starts from defaults, unions state entries, supports empty state lists as clear markers, deduplicates by id with state entries winning, and resolves ids against registries unless inline definitions are present.
- Missing required tooling triggers a tooling-unavailable route or error; optional missing tooling is warned and dropped.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::RheiSettings`
- `crates/rhei-cli/src/main.rs::SettingsDefaults`
- `crates/rhei-cli/src/main.rs::built_in_agents`
- `crates/rhei-cli/src/main.rs::load_settings`
- `crates/rhei-cli/src/main.rs::load_merged_settings`
- `crates/rhei-cli/src/main.rs::validate_machine_settings_references`
- `crates/rhei-cli/src/main.rs::resolve_tooling`
- `crates/rhei-cli/src/main.rs::effective_mcp_entries`
- `crates/rhei-cli/src/main.rs::effective_skill_entries`
- `crates/rhei-cli/src/main.rs::resolve_mcp_entry`
- `crates/rhei-cli/src/main.rs::resolve_skill_entry`
- `crates/rhei-cli/src/main.rs::inject_tooling_env`
- `crates/rhei-cli/src/main.rs::format_tooling_log_line`
- `crates/rhei-validator/src/lib.rs::AgentConfig`
- `crates/rhei-validator/src/lib.rs::CustomAgentProfile`
- `crates/rhei-validator/src/lib.rs::McpServerProfile`
- `crates/rhei-validator/src/lib.rs::SkillProfile`
- `crates/rhei-validator/src/lib.rs::validate_state_mcp_entries`
- `crates/rhei-validator/src/lib.rs::validate_state_skill_entries`
- Unit tests for tooling resolution and built-in transports listed above
- Validator tooling tests listed above
- `.agents/rhei/templates/changeset-review/settings.json`
- `.agents/rhei/templates/hourly-human-intervention/settings.json`
- `.agents/rhei/templates/multi-model-analysis/settings.json`
- `examples/changeset-review-example/.rhei/settings.json`
- `examples/hourly-human-intervention-example/.rhei/settings.json`
- `skills/rhei-template-writer/SKILL.md` settings bundling guidance

### RO-008: Model, Agent, Mode, `target`, `all_targets`, and `all_models` Resolution

Spec sections:

- `docs/specs/rhei-agents.spec.md` -> `Resolution Order`, `Mode Resolution Order`, `Per-State Settings`
- `docs/specs/rhei-usage.spec.md` -> `Pattern 0`, `Pattern 7`, `Pattern 8`

Normative claims:

- Model id resolution order is CLI `--model`, state `model`, project `defaults.model`, global `defaults.model`.
- Resolved model id must exist in merged `models`; when none is configured, model-specific callback and template fields are omitted.
- Agent id resolution order is CLI `--agent`, state `agent`, project `defaults.agent`, global `defaults.agent`, then `models.<id>.default_agent`.
- Resolved agent id must exist in merged `agents`; missing agent ids are configuration errors.
- `all_targets` bypasses normal model/agent resolution for fields encoded in the selector: agent id, optional mode, optional provider, and model name.
- `all_targets` validation still verifies referenced agent and mode exist.
- Legacy `all_models` runs normal agent resolution independently for each model execution.
- Mode resolution order is CLI `--agent-mode`, state `agent_mode`, project `defaults.agent_mode`, global `defaults.agent_mode`, registry default first mode, then none.
- If an agent has modes, resolved mode must name one of them; if an agent has no modes, no mode flags are appended.
- Target selectors provide `{target}`, `{target.slug}`, `{agent}`, `{agent.mode}`, and `{model}` template values.
- One task with `all_targets` should execute once per target and downstream synthesis can consume per-target artifacts.

Implementation and tests to compare:

- `crates/rhei-validator/src/lib.rs::ExecutionTarget`
- `crates/rhei-validator/src/lib.rs::parse_execution_target`
- `crates/rhei-validator/src/lib.rs::StateMachine::validate_model_configuration`
- `crates/rhei-cli/src/main.rs::resolve_target_agent`
- `crates/rhei-cli/src/main.rs::resolve_legacy_agent_with_model`
- `crates/rhei-cli/src/main.rs::resolve_agent_invocations`
- `crates/rhei-cli/src/main.rs::transition_contexts_for_state`
- `crates/rhei-cli/src/main.rs::callback_contexts_for_state`
- `crates/rhei-cli/src/main.rs::resolve_runtime_template_variable`
- `crates/rhei-cli/src/main.rs::resolved_agent_log_suffix`
- Validator tests `parses_execution_target_with_mode_and_provider`, `loads_state_machine_with_target_selectors`, `rejects_state_machine_with_conflicting_target_and_model_selectors`
- Integration `run_executes_all_models_callbacks_without_agent_configuration`
- E2E `run_living_review_loop_fixture_to_completion`
- Template `.agents/rhei/templates/multi-model-analysis/states.yaml`
- Template `.agents/rhei/templates/changeset-review/states.yaml`
- Example `examples/changeset-review-example/states.yaml`
- Example `examples/living-review-loop/team-states.yaml`
- `skills/rhei-state-machine-writer/SKILL.md` target/model guidance

### RO-009: Agent Prompt Composition, Environment, Command Assembly, and Logs

Spec sections:

- `docs/specs/rhei-agents.spec.md` -> `Prompt Composition`, `Environment Variables`, `Known Agent Profiles`, `Modes`, `Log Capture`, `Dry-Run Output`
- `docs/specs/rhei-run.spec.md` -> `Execution Loop`, `Dry Run`, `Parallel Execution`

Normative claims:

- Agent prompt includes task heading, current state, resolved personality if present, resolved instructions, task body and child task nodes, and a `Rhei Commands` section.
- The prompt tells spawned agents that `rhei run` advances the task and that they must not call transition/complete or edit state lines except for nested independent executions.
- Prompt content should not encode completion-detection prose; completion is enforced structurally by exit plus artifacts.
- Template variables are resolved before prompt delivery.
- Prompt is delivered by configured `prompt_flag` or stdin.
- Mode flags are appended immediately after the base command and before prompt/model flags; stdin agents get `--` after the model flag.
- Agent subprocess env includes `RHEI_PLAN_PATH`, `RHEI_TASK_ID`, `RHEI_STATE`, `RHEI_MODEL`, `RHEI_MODEL_PROVIDER`, `RHEI_MODEL_NAME`, `RHEI_AGENT`, `RHEI_MCP_SERVERS`, `RHEI_MCP_<NAME>_AVAILABLE`, `RHEI_SKILLS`, and `RHEI_SKILL_<ID>_AVAILABLE` as applicable.
- Agent working directory is workspace root or single-file plan parent.
- Agent stdout/stderr are captured in `runtime/logs/`.
- Agent log names cover simple, counted-loop, model-specific, and combined visit/model cases.
- Agent log format starts with `=== rhei agent log v1 ===`, includes header metadata, raw interleaved body, and `=== exit ===` footer.
- `rhei run --dry-run` uses the same selection logic but does not spawn subprocesses, execute callbacks, acquire file locks, rewrite markdown, or create runtime artifacts.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::compose_agent_prompt`
- `crates/rhei-cli/src/main.rs::build_agent_command`
- `crates/rhei-cli/src/main.rs::inject_tooling_env`
- `crates/rhei-cli/src/main.rs::agent_log_path`
- `crates/rhei-cli/src/main.rs::spawn_agent_output_reader`
- `crates/rhei-cli/src/main.rs::spawn_and_wait_agent`
- `crates/rhei-cli/src/main.rs::run_agent_mode` dry-run branch
- Unit `compose_agent_prompt_carries_domain_instructions_only`
- Unit `supported_agents_keep_expected_prompt_transports`
- Unit `fake_claude_profile_streams_prompt_flag_output`
- Unit `fake_codex_profile_streams_stdin_prompt_output`
- Unit `fake_pi_profile_streams_prompt_flag_output`
- Unit `output_reader_logs_and_emits_complete_and_partial_lines`
- Integration `run_dry_run_shows_transitions_without_changes`
- E2E fixture `crates/rhei-cli/tests/e2e/fixtures/bash-agent-team/*`

### RO-010: Agent Timeout Requirement, Timeout Behavior, and Timeout Transitions

Spec sections:

- `docs/specs/rhei-run.spec.md` -> `Execution Loop`
- `docs/specs/rhei-agents.spec.md` -> `Timeout Requirement`, `Timeout Handling`, `Timeout Transitions`, `Timeout Callbacks`

Normative claims:

- Under orchestrator authority, every spawned agent invocation must resolve to a finite timeout.
- Agent timeout resolution order is state `agent_timeout`, model-agent binding timeout, agent profile timeout, settings defaults.
- Missing timeout under orchestrator authority is a validation error; manual worker authority has no timeout enforcement.
- On timeout, `rhei run` sends `SIGTERM`, waits 10 seconds, then sends `SIGKILL`.
- Timeout transitions are transitions with `timeout`; first matching timeout transition from the current state fires.
- Timeout transitions fire callbacks like normal transitions.
- Timeout-triggered callbacks receive `triggeredBy: 'system'` and timeout duration in `transitionData.timeout`.
- If no timeout transition exists, the task remains in its current state and a warning is logged.
- The engine kills only the direct subprocess.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::ensure_orchestrator_timeout`
- `crates/rhei-cli/src/main.rs::resolve_legacy_agent_with_model`
- `crates/rhei-cli/src/main.rs::resolve_target_agent`
- `crates/rhei-cli/src/main.rs::spawn_and_wait_agent`
- `crates/rhei-cli/src/main.rs::fire_timeout_transition`
- `crates/rhei-cli/src/main.rs::execute_transition`
- `crates/rhei-validator/src/lib.rs::parse_duration_secs`
- Unit `resolve_legacy_agent_uses_defaults_agent_timeout`
- Unit `fake_agent_timeout_keeps_output_and_writes_footer`
- Unit `inherited_output_pipe_does_not_block_agent_completion`
- Any run tests with `agent_timeout` in `examples/changeset-review-example/.rhei/settings.json`, `.agents/rhei/templates/*/settings.json`, and plan states.

### RO-011: Program Declaration, Environment, Spawn, Exit-Code Routing, Artifacts, and Logs

Spec sections:

- `docs/specs/rhei-programs.spec.md` -> `Program Declaration`, `Template Variables in Commands`, `Environment Variables`, `Exit-Code Transitions`, `Timeout Handling`, `Log Capture`, ``rhei run` Integration`, `Artifact Contracts`, `Instructions and Personality`, `Models`, `Validation Rules`, `Per-State Fields`, `Transition Field`
- `docs/specs/rhei-run.spec.md` -> `Execution Loop`
- `docs/specs/rhei-usage.spec.md` -> `Pattern 9`, `Pattern 10`

Normative claims:

- `program` may be a non-empty string or an object with `command`.
- String form runs via system shell; object `command` array bypasses shell unless `shell: true`.
- `program.env` values support template variables and merge on top of base `RHEI_*` vars, with `program.env` winning collisions.
- `program.working_directory` is relative to workspace root, supports template variables, defaults to workspace root or single-file plan parent, and must resolve within the workspace.
- Program commands resolve template variables before spawn.
- Program env includes `RHEI_PLAN_PATH`, `RHEI_TASK_ID`, `RHEI_STATE`, `RHEI_VISIT_COUNT`, `RHEI_INPUT_<NAME>_EXISTS`, and `RHEI_INPUT_<NAME>_PATH`.
- Exit-code transitions support integer, integer array, and `"nonzero"`.
- Exit-code evaluation checks specific values first, then `"nonzero"`, fires exactly one transition, uses conditions to disambiguate, warns on zero with no match, and errors/uses `--continue-on-error` on nonzero with no match.
- If a program directly changes task state before exit, exit-code evaluation is skipped.
- Program timeout resolution order is CLI `--program-timeout`, state `program_timeout`, settings `program_timeout`.
- Program timeout uses SIGTERM -> 10 second grace -> SIGKILL and timeout-transition evaluation, with `triggeredBy: 'system'`.
- Program logs use `runtime/logs/`, `=== rhei program log v1 ===`, `program:` header, and `=== exit ===` footer.
- `--no-agent` has no effect on programs; `--no-program` suppresses program spawning and uses callback-only advancement.
- A state must not declare both `program` and `gating: true`; must not declare both `agent` and `program`; program on final state is invalid.
- `exit_code` transitions must originate from program states and should not overlap at the same specificity without mutually exclusive conditions.
- Program `instructions` are documentation only; `personality` is ignored.
- `model` and `all_models` are ignored for programs; declaring them with `program` is a validation warning.

Implementation and tests to compare:

- `crates/rhei-validator/src/lib.rs::StateDef::{program,program_timeout}`
- `crates/rhei-validator/src/lib.rs::StateMachine::validate_program_configuration`
- `crates/rhei-validator/src/lib.rs::validate_program_value`
- `crates/rhei-validator/src/lib.rs::validate_program_command`
- `crates/rhei-cli/src/main.rs::parse_program_spec`
- `crates/rhei-cli/src/main.rs::resolve_program`
- `crates/rhei-cli/src/main.rs::build_program_command`
- `crates/rhei-cli/src/main.rs::spawn_and_wait_program`
- `crates/rhei-cli/src/main.rs::program_log_path`
- `crates/rhei-cli/src/main.rs::transition_matches_exit_code`
- `crates/rhei-cli/src/main.rs::find_program_exit_transition`
- `crates/rhei-cli/src/main.rs::run_agent_mode` program branch
- Validator tests `rejects_program_on_gating_state`, `rejects_exit_code_transition_from_non_program_state`
- E2E `run_executes_program_states_and_routes_on_exit_code`
- Example `examples/ci-heal/states.yaml`
- Example `examples/ci-heal/index.rhei.md`
- `skills/rhei-state-machine-writer/SKILL.md` guidance for `program`, `exit_code`, and deterministic workflow steps

### RO-012: Callback Invocation, Context, Rejection, Data Passing, Redirects, and Rollback

Spec sections:

- `docs/specs/rhei-callbacks.spec.md` -> `Basic Transition Approval`
- `docs/specs/rhei-callbacks.spec.md` -> `Dependency Validation`
- `docs/specs/rhei-callbacks.spec.md` -> `Data Passing Between Callbacks`
- `docs/specs/rhei-callbacks.spec.md` -> `State Redirection`
- `docs/specs/rhei-callbacks.spec.md` -> `Accessing Custom Metadata`
- `docs/specs/rhei-callbacks.spec.md` -> `Environment-Aware Logic`
- `docs/specs/rhei-agents.spec.md` -> `Interaction Between Agents and Callbacks`, `Timeout Callbacks`
- `docs/specs/rhei-programs.spec.md` -> `Callbacks`
- `docs/specs/rhei-usage.spec.md` -> `Pattern 6`

Normative claims:

- Transition callbacks run on declared `on_leave` and `on_enter` hooks.
- Callbacks can approve with `{"success": true}` or reject with `{"success": false, "error": "..."}`.
- Callback rejection blocks the transition and surfaces the callback error.
- CLI/bash callbacks receive `TransitionContext` JSON on stdin and can also use environment variables.
- Callback context exposes plan, task, task metadata including dependencies, transition `from`/`to`/`triggeredBy`/timestamp, accumulated `transitionData`, and execution environment platform/version/working directory.
- Data returned from `on_leave` flows to `on_enter` through `transitionData`.
- `on_leave` can return `nextState` to redirect to another declared transition from the same source.
- Redirects to unknown states or undeclared transitions are rejected before state write.
- `on_enter` failures roll back the state write.
- `--no-callbacks` skips `on_leave` / `on_enter` execution.
- `rhei run` fires callbacks as part of engine-driven transitions after agents or programs exit successfully and a target transition is selected.
- Timeout transitions and tooling-unavailable transitions should fire callbacks with `triggeredBy: 'system'` where specified.

Implementation and tests to compare:

- `crates/rhei-core/src/callback.rs::CallbackContext`
- `crates/rhei-core/src/callback.rs::CallbackResult`
- `crates/rhei-core/src/callback.rs::ShellCallbackExecutor`
- `crates/rhei-core/src/callback.rs::NoopCallbackExecutor`
- `crates/rhei-core/src/callback.rs::parse_callback_stdout`
- `crates/rhei-cli/src/main.rs::build_transition_context_json`
- `crates/rhei-cli/src/main.rs::execute_transition`
- `crates/rhei-cli/src/main.rs::merge_transition_data`
- `crates/rhei-cli/src/main.rs::resolve_callback_paths`
- `crates/rhei-cli/src/main.rs::callback_contexts_for_state`
- Integration callback tests listed above
- Integration `run_executes_relative_callback_from_state_machine_directory`
- Integration `run_callback_failure_halts_execution`
- Integration `run_no_callbacks_flag_skips_callbacks`
- Examples `examples/review-fix-visits/*`
- Examples `examples/living-review-loop/*`
- Example `examples/ci-heal/.rhei/*.sh`

### RO-013: Failure Routing and Continue-On-Error

Spec sections:

- `docs/specs/rhei-run.spec.md` -> `Execution Loop`
- `docs/specs/rhei-agents.spec.md` -> `Runtime Semantics`, `Execution Loop`, `Missing Tooling`, `Timeout Behavior`
- `docs/specs/rhei-programs.spec.md` -> `Exit-Code Transitions`, `Timeout Handling`

Normative claims:

- Nonzero agent exits route through the exit-code/error transition path where defined; without a route and without `--continue-on-error`, `rhei run` aborts nonzero.
- With `--continue-on-error`, failed agent/program tasks are logged and skipped while other ready work continues.
- Nonzero program exits route via `exit_code` matching or apply `--continue-on-error` if no transition matches.
- Missing required outputs after zero exit do not transition and should leave the task in current state.
- Missing required tooling follows required/optional behavior and may route through `mcp_unavailable` or `skill_unavailable`.
- Timeout failures route through timeout transitions or leave the task in place with a warning.
- `rhei run` exits `0` when every task reaches a terminal state; it exits nonzero when progress halts with non-terminal tasks remaining and no advancement possible.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::RunOptions::continue_on_error`
- `crates/rhei-cli/src/main.rs::run_agent_mode` agent and program result handling
- `crates/rhei-cli/src/main.rs::find_program_exit_transition`
- `crates/rhei-cli/src/main.rs::fire_timeout_transition`
- `crates/rhei-cli/src/main.rs::ensure_state_outputs_exist`
- `crates/rhei-validator/src/lib.rs::validate_transition_tooling_trigger`
- E2E `run_executes_program_states_and_routes_on_exit_code`
- E2E `run_callback_mode_waits_for_other_branches_before_halting_at_human_review`
- Integration `run_callback_failure_halts_execution`
- Unit `fake_agent_timeout_keeps_output_and_writes_footer`
- Validator tooling trigger tests listed above

### RO-014: Gating Barriers and Human Review Boundaries

Spec sections:

- `docs/specs/rhei-run.spec.md` -> `Execution Loop`
- `docs/specs/rhei-agents.spec.md` -> `Completion Authority`, `Gating States`
- `docs/specs/rhei-programs.spec.md` -> `Gating States`
- `docs/specs/rhei-usage.spec.md` -> `Reviewer`, `Human Operator`, `State Flow`, `Pattern 4`

Normative claims:

- `rhei run` must not transition out of gating states.
- No agent or program is spawned for a gating state.
- A gating state is a barrier, not an immediate global abort: independent non-gating work continues until remaining non-terminal tasks are gated or blocked behind gates.
- Exiting a gating state requires explicit human-initiated `rhei transition`.
- `human-review` gates must not be bypassed by autonomous workflows.
- A state must not combine `program` with `gating: true`.
- Tooling fields that imply autonomous agent execution should not be valid on gating states where specified.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::find_ready_tasks`
- `crates/rhei-cli/src/main.rs::run_agent_mode`
- `crates/rhei-cli/src/main.rs::run_callback_mode`
- `crates/rhei-validator/src/lib.rs::StateDef::gating`
- `crates/rhei-validator/src/lib.rs::StateMachine::validate_program_configuration`
- `crates/rhei-validator/src/lib.rs::validate_state_mcp_entries`
- `crates/rhei-validator/src/lib.rs::validate_state_skill_entries`
- E2E `run_callback_mode_stops_at_human_review`
- E2E `run_callback_mode_waits_for_other_branches_before_halting_at_human_review`
- E2E `changeset_review_human_review_state_is_gating_in_shipped_workflows`
- Validator `rejects_program_on_gating_state`
- Validator `state_mcp_servers_rejected_on_gating_state`
- Template `.agents/rhei/templates/changeset-review/states.yaml`
- Example `examples/changeset-review-example/states.yaml`
- `skills/rhei-plan-worker/SKILL.md`
- `skills/rhei-plan-writer/references/default-states.md`

### RO-015: Runtime Journaling and User-Facing Monitoring Events

Spec sections:

- `docs/specs/rhei-run.spec.md` -> `Parallel Execution`
- `docs/specs/rhei-agents.spec.md` -> `Log Capture`
- `docs/specs/rhei-programs.spec.md` -> `Log Capture`

Normative claims:

- Each spawned subprocess gets a slot index.
- `runtime/transitions.log` receives one line per `SlotAssigned` and one per `SlotReleased`.
- Slot release lines include outcome, duration, and exit code where available.
- Runtime logs are created under the workspace or plan root.
- `rhei reset` removes `runtime/`, including logs.

Implementation and tests to compare:

- `crates/rhei-cli/src/main.rs::run_agent_mode`
- `crates/rhei-cli/src/main.rs::agent_log_path`
- `crates/rhei-cli/src/main.rs::program_log_path`
- `crates/rhei-cli/src/main.rs::spawn_agent_output_reader`
- `crates/rhei-cli/src/main.rs::spawn_and_wait_agent`
- `crates/rhei-cli/src/main.rs::spawn_and_wait_program`
- `crates/rhei-tui/src/event.rs`
- `crates/rhei-tui/src/frontend.rs`
- `crates/rhei-tui/src/journal.rs::JournalSink`
- `crates/rhei-tui/src/stdout.rs`
- `crates/rhei-tui/src/tui.rs`
- Journal tests in `crates/rhei-tui/src/journal.rs`
- Unit agent log/output tests in `crates/rhei-cli/src/main.rs`
- E2E `reset_bash_agent_team_fixture_restores_initial_state`

## User-Facing Commands to Exercise

- `rhei run <plan-or-workspace>`
- `rhei run <plan-or-workspace> --dry-run`
- `rhei run <plan-or-workspace> --no-callbacks`
- `rhei run <plan-or-workspace> --continue-on-error`
- `rhei run <plan-or-workspace> --parallel <N>`
- `rhei run <plan-or-workspace> --parallel 0`
- `rhei run <plan-or-workspace> --no-agent`
- `rhei run <plan-or-workspace> --agent <AGENT>`
- `rhei run <plan-or-workspace> --agent-mode <MODE>`
- `rhei run <plan-or-workspace> --model <MODEL>`
- `rhei run <plan-or-workspace> --no-program`
- `rhei run <plan-or-workspace> --program-timeout <DURATION>`
- `rhei validate <plan-or-workspace>` for settings, state-machine, program, target, artifact, polling, and tooling validation
- `rhei transition <plan-or-workspace> --task <ID> --from <STATE> --to <STATE>` as a manual/human gating counterpart and callback execution surface
- `rhei next <plan-or-workspace>` only as the referenced claimability baseline
- `rhei reset <plan-or-workspace>` for `runtime/` cleanup behavior

## High-Value Compare Checklist

- Compare `rhei-run.spec.md` ready-set requirements against `find_ready_tasks`, especially current-state required input checks and poll deadline exclusion.
- Compare run parallel scheduling claims against `run_agent_mode`, especially whether program states respect `--parallel`, whether `all_targets` fanout stays together, and whether non-concurrent filtering applies to programs as well as agents.
- Compare output artifact enforcement claims against agent and program branches separately.
- Compare settings schema and merge order claims against `RheiSettings` and `load_merged_settings`, especially nested `defaults.model`, `defaults.agent`, model registry support, and model-agent binding timeouts.
- Compare `all_targets` and `all_models` claims against `resolve_agent_invocations`, runtime template variables, artifact path rendering, and callback context fanout.
- Compare timeout claims against `spawn_and_wait_agent`, `spawn_and_wait_program`, `fire_timeout_transition`, and callback context `triggeredBy` / `transitionData.timeout`.
- Compare callback example semantics against `ShellCallbackExecutor` and `execute_transition`, especially context shape, data propagation, redirect behavior, on-enter rollback, and run-triggered callback authority.
- Compare log format claims against `spawn_and_wait_agent`, `spawn_and_wait_program`, and `JournalSink`.
- Compare shipped templates/examples to ensure they rely only on behavior that implementation supports: `.agents/rhei/templates/multi-model-analysis`, `.agents/rhei/templates/changeset-review`, `.agents/rhei/templates/hourly-human-intervention`, `examples/ci-heal`, `examples/living-review-loop`, and `examples/changeset-review-example`.
