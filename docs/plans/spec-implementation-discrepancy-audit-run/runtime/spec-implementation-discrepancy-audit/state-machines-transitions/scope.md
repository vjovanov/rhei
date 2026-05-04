# Scope Inventory: State Machines and Transitions

Partition task: `state-machines-transitions`

This inventory covers Rhei state-machine YAML semantics and explicit transition behavior. It names the normative claims from the state, transition, and state-machine-writer specifications, then maps each claim to the implementation files, tests, fixtures, templates, skills, and user-facing commands that should satisfy it.

## In Scope

Specification files:

- `docs/specs/rhei-states.spec.md`
  - `Schema Additions` / `Top-level fields` / `Per-state fields`
  - `Validation Rules`
  - `Polling States`
  - `Artifact Contracts`
  - `Template Variables in Instructions and Personality`
  - `Agent Field`
  - `Program States`
  - `MCP Servers and Skills`
  - `Profiles`
  - `Node Policy`
  - default `States`
  - default `Transitions`
  - `Completion paths`
- `docs/specs/rhei-transitions.spec.md`
  - `Goals` and `Requirements`
  - `TransitionContext Data Structure`
  - `Rhei File Metadata Format`
  - `Counted Loop Metadata`
  - `Metadata Access in Callbacks`
  - `Transition Triggers`
  - `YAML State Machine Format Specification`
  - `State Definition`
  - `Counted Loops`
  - `Transition Definition`
  - `Artifact Enforcement`
  - `Wildcard Semantics`
  - `Callback Declaration`
  - `Callback Mappings`
  - `Error Handling Configuration`
  - `Example 8: Transition Validation Flow`
- `docs/specs/rhei-state-machine-writer.spec.md`
  - `Purpose`
  - `Inputs`
  - `Output` / `Output Structure`
  - `Design Rules`
  - `State Design`
  - `Transition Design`
  - `Profile and Node Policy Design`
  - `Instructions Design`
  - `Workflow`
  - `Examples`
  - `Relationship to Other Roles`
  - `When to Use a Custom State Machine`
  - `File Placement`

Implementation roots:

- `crates`
- `skills`
- `.agents/rhei/templates`
- `examples`

Primary command surfaces to compare:

- `rhei states [--json] [--state-machine <path>]`
- `rhei validate <plan-or-workspace>`
- `rhei next <plan-or-workspace> [--task <id>] [--peek] [--json] [--no-callbacks]`
- `rhei transition <plan-or-workspace> --task <id> --from <state> --to <state> [--no-callbacks]`
- `rhei complete <plan-or-workspace> --task <id> --result <message> [--no-callbacks]`
- `rhei run <plan-or-workspace> [--parallel <n>] [--no-callbacks] [--no-agent] [--no-program]`
- `rhei reset <plan-or-workspace>`

Adjacent specs are not primary inputs for this partition, but some scoped claims explicitly reference their execution details. The next comparison state should consult them only as supporting context where the three in-scope specs link to them: `docs/specs/rhei-run.spec.md`, `docs/specs/rhei-agents.spec.md`, `docs/specs/rhei-programs.spec.md`, `docs/specs/rhei-callbacks.spec.md`, and `docs/rhei.spec.md`.

## Shared Implementation Surfaces

Core AST and callback model:

- `crates/rhei-core/src/ast.rs`
  - `StateName`
  - `CallbackRef`
  - `TransitionRule::{from,to,on_leave,on_enter,condition,timeout,exit_code,mcp_unavailable,skill_unavailable}`
  - `Task`, `Rhei`, and metadata-bearing task fields exposed to transition contexts
- `crates/rhei-core/src/callback.rs`
  - `CallbackContext`
  - `CallbackResult`
  - `CallbackExecutor`
  - `ShellCallbackExecutor`
  - `NoopCallbackExecutor`
  - `parse_callback_stdout`
- `crates/rhei-core/src/parser.rs`
  - plan frontmatter metadata parsing
  - markdown task fields for `**State:**`, `**Prior:**`, `**Assignee:**`, and result links
- `crates/rhei-core/src/workspace.rs`
  - single-file vs Directory Workspace loading boundaries

State-machine loading and validation:

- `crates/rhei-validator/src/lib.rs`
  - `StateArtifactDef`
  - `StateDef`, including the legacy `StateDef::initial` compatibility surface that must be compared against the current profile-based spec
  - `PollConfig`
  - `Profile`
  - `NodePolicy`
  - `NodePolicyOverride`
  - `StateMachine`
  - `StateMachine::{builtin_default,from_yaml_str,from_yaml_file,is_valid_state,allowed_states,transitions,profile_for,root_profile}`
  - `parse_task_state`
  - `parse_execution_target`
  - `validate_with_machine`
  - `validate_with_machine_and_base`
  - `validate_model_configuration`
  - `validate_program_configuration`
  - `validate_poll_configuration`
  - `validate_tooling_configuration`
  - `validate_profiles_and_node_policy`
  - `validate_template_conditions`
  - `validate_artifact_definitions`
  - `validate_state_consistency`
  - `validate_task_state_against_profile`
  - `validate_task_state_instance`
  - `validate_terminal_tree_coherence`
- `crates/rhei-validator/src/default-states.yaml`
- `docs/specs/states.yaml`

CLI execution and command behavior:

- `crates/rhei-cli/src/main.rs`
  - command definitions: `Commands::{States,Validate,Next,Transition,Complete,Run,Reset}`
  - state-machine display: `states_command`, `render_state_machine_text`, `render_state_machine_json`
  - transition execution: `transition_command`, `execute_transition`, `transition_rule_is_applicable`, `evaluate_transition_condition`, `find_next_transition`, `find_completion_state`
  - system transition routing: `fire_timeout_transition`, `find_program_exit_transition`, `transition_matches_exit_code`
  - counted visits: `task_visit_count`, `state_visit_limit`, `current_state_visit_count`, `loop_reentry_allowed`, `ensure_current_state_visit_count`, `update_metadata_for_transition`, `format_task_state_value`, `render_visit_count`
  - callbacks and context: `resolve_callback_paths`, `build_transition_context_json`, `merge_transition_data`, `callback_contexts_for_state`
  - runtime templating: `RuntimeTemplateContext`, `resolve_runtime_template_text`, `resolve_runtime_template_variable`, `process_conditional_blocks`, `evaluate_if_condition`, `state_instructions`, `compose_agent_prompt`
  - artifacts: `artifact_relative_path`, `resolve_artifact_path`, `ensure_state_inputs_exist`, `ensure_state_outputs_exist`, `ensure_state_inputs_exist_for_transition`, `ensure_state_outputs_exist_for_transition`, `state_outputs_exist_for_resolved_invocation`, `task_has_pending_agent_invocations`
  - ready set and scheduling: `find_ready_tasks`, `is_terminal_state`, `run_agent_mode`, `state_declares_autonomous_execution`
  - command-specific completion/reset behavior: `next_command`, `complete_command`, `reset_command`, `initial_state_name`, `reset_target_files`, `reset_plan_file_states`, `find_completion_state`
- `crates/rhei-napi/src/lib.rs`
  - NAPI surface currently present for language-binding claims in the transition specification

Core tests and fixtures:

- `crates/rhei-validator/src/lib.rs` unit tests
  - state schema and model/target validation tests
  - artifact validation tests such as duplicate names, absolute paths, escaping root, optional output rejection
  - counted-state suffix tests: `accepts_counted_state_suffix_within_budget`, `rejects_counted_state_suffix_of_one`, `rejects_counted_state_suffix_when_state_has_no_visits`, `rejects_state_machine_with_zero_visits`
  - profile/node-policy tests including `profile_for_returns_none_when_not_declared`, `rejects_node_policy_without_profiles`, `rejects_node_policy_default_with_undefined_profile`, `rejects_node_policy_by_type_with_reserved_kind`
  - poll tests: `accepts_well_formed_poll_state`, `rejects_poll_with_invalid_interval`, `rejects_poll_with_zero_max_attempts`, `rejects_poll_with_visits`, `rejects_poll_on_gating_state`, `rejects_poll_without_self_loop`
  - gating/tooling/program tests such as `rejects_program_on_gating_state`, `state_mcp_servers_rejected_on_gating_state`
- `crates/rhei-cli/src/main.rs` unit tests
  - `render_state_machine_json_includes_state_personality`
  - `find_completion_state_prefers_non_cancelled_terminal`
  - `find_completion_state_does_not_fall_back_to_cancelled`
  - `find_completion_state_returns_none_when_no_terminal_reachable`
  - prompt, template, target, tooling, agent, and program routing tests near the referenced helpers
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - `transition_counted_loop_updates_metadata_and_blocks_exhausted_reentry`
  - `transition_from_authored_counted_state_treats_start_as_first_visit`
  - `workspace_transition_updates_index_metadata_for_counted_loops`
  - `transition_wildcard_from_allows_any_source`
  - `transition_fails_on_cas_conflict`
  - `transition_fails_on_invalid_transition`
  - callback tests: `callback_on_leave_and_on_enter_invoked_on_transition`, `callback_on_leave_failure_blocks_transition`, `no_callbacks_flag_skips_callback_execution`, `callback_unknown_platform_produces_clear_error`, `callback_rejection_surfaces_spec_error_message`, `callback_redirect_via_next_state_retargets_declared_transition`, `callback_redirect_to_undeclared_transition_is_rejected`, `callback_receives_transition_context_on_stdin`, `callback_on_enter_failure_rolls_back_state_write`
  - `run_uses_counted_loop_exit_when_visit_budget_is_exhausted`
  - run callback/tooling/agent-program tests that exercise transition selection from autonomous execution
- `crates/rhei-cli/tests/e2e/next_tests.rs`
  - `next_fails_with_explicit_error_when_current_state_input_artifact_is_missing`
  - `complete_fails_when_required_output_artifact_is_missing`
  - `complete_fails_when_only_cancelled_terminal_is_available`
- `crates/rhei-cli/tests/e2e/transition_tests.rs`
  - `transition_wildcard_to_cancelled`
  - `transition_fails_when_target_state_input_artifact_is_missing`
- `crates/rhei-cli/tests/e2e/run_tests.rs`
  - `run_callback_mode_stops_at_human_review`
  - `run_callback_mode_waits_for_other_branches_before_halting_at_human_review`
  - `changeset_review_human_review_state_is_gating_in_shipped_workflows`
  - run tests for program exit-code routing, output artifacts, parallel scheduling, and shipped workflow fixtures
- `crates/rhei-cli/tests/e2e/fixtures/bash-agent-team/*`
- `crates/rhei-cli/tests/e2e/fixtures/living-review-loop/*`

Skill, template, and example surfaces:

- `skills/rhei-state-machine-writer/SKILL.md`
- `skills/rhei-template-writer/SKILL.md`
- `skills/rhei-plan-writer/references/default-states.md`
- `skills/rhei-plan-worker/SKILL.md`
- `.agents/rhei/templates/changeset-review/states.yaml`
- `.agents/rhei/templates/hourly-human-intervention/states.yaml`
- `.agents/rhei/templates/multi-model-analysis/states.yaml`
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml`
- `.agents/rhei/templates/spec-review/states.yaml`
- `.agents/rhei/templates/*/template.yaml` where templates bundle or point at state machines
- `examples/changeset-review-example/states.yaml`
- `examples/ci-heal/states.yaml`
- `examples/claude-code/states.yaml`
- `examples/hourly-human-intervention-example/states.yaml`
- `examples/living-review-loop/team-states.yaml`
- `examples/review-fix-visits/states.yaml`
- `examples/spec-implementation-discrepancy-audit-example/states.yaml`
- `examples/states-with-spaces.yaml`
- `examples/human-review-loop.rhei.md`
- `examples/release-automation.rhei.md`

## Normative Claim Inventory

### SMT-001: YAML State Machine Root Schema and Default Machine

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Schema Additions`, `States`, `Transitions`, `Completion paths`
- `docs/specs/rhei-transitions.spec.md` -> `YAML State Machine Format Specification`, `Root-Level Fields`, `State Definition`, `Transition Definition`
- `docs/specs/rhei-state-machine-writer.spec.md` -> `Output`, `Output Structure`

Normative claims:

- A YAML state machine declares root fields including `name`, `version`, `states`, `transitions`, optional `models`, optional `callbacks`, optional `error_handling`, required `profiles`, and required `node_policy`.
- The `profiles` and `node_policy` blocks replace state-level `initial: true`; state definitions must not carry their own initial flag.
- The default Rhei machine has states `draft`, `pending`, `agent-review`, `agent-review-fix`, `human-review`, `completed`, and `cancelled`.
- `completed` and `cancelled` are final states.
- `human-review` is a gating state.
- The default legal transition table is the one in `docs/specs/states.yaml`; any unlisted transition is forbidden.
- `rhei complete` can complete only from a non-gating state that has a one-hop non-cancelled terminal transition.

Implementation and artifacts to compare:

- `docs/specs/states.yaml`
- `crates/rhei-validator/src/default-states.yaml`
- `skills/rhei-plan-writer/references/default-states.md`
- `crates/rhei-validator/src/lib.rs::StateDef`
- `crates/rhei-validator/src/lib.rs::StateDef::initial`
- `crates/rhei-validator/src/lib.rs::StateMachine`
- `crates/rhei-validator/src/lib.rs::StateMachine::builtin_default`
- `crates/rhei-validator/src/lib.rs::StateMachine::from_yaml_str`
- `crates/rhei-cli/src/main.rs::states_command`
- `crates/rhei-cli/src/main.rs::render_state_machine_text`
- `crates/rhei-cli/src/main.rs::render_state_machine_json`
- `crates/rhei-cli/src/main.rs::find_completion_state`
- `crates/rhei-cli/src/main.rs::complete_command`
- commands: `rhei states`, `rhei validate`, `rhei complete`

Tests, templates, examples:

- `crates/rhei-cli/src/main.rs` tests for state-machine rendering and completion selection
- `crates/rhei-cli/tests/e2e/next_tests.rs::complete_fails_when_only_cancelled_terminal_is_available`
- all shipped `.agents/rhei/templates/*/states.yaml`
- all shipped `examples/*/states.yaml`

### SMT-002: Profiles and Node Policy

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Schema Additions`, `Profiles`, `Node Policy`
- `docs/specs/rhei-state-machine-writer.spec.md` -> `Profile and Node Policy Design`, `Workflow`

Normative claims:

- A profile is a named `{initial, allowed}` state policy.
- `profiles` must be present and non-empty.
- Each profile's `initial` must be a defined state, must appear in `allowed`, and `allowed` must contain at least one final state.
- Every non-final state in a profile's `allowed` set must have a path to a final state using transitions that stay within `allowed`.
- `node_policy.root` and `node_policy.default` are required and must reference defined profiles.
- Profile resolution order is root, then ordered `overrides`, then `by_type`, then `default`.
- `by_type` keys must be declared non-root node kinds; `rhei` is reserved for the root and must not appear in `by_type`.
- `overrides[].match` may use only `type` and `level`; level must be within the plan structure's allowed range.
- Authored `**State:**` values must belong to the node's resolved profile `allowed` set.
- `rhei reset` returns each node to its resolved profile's `initial`, not to a single machine-wide initial state.

Implementation and artifacts to compare:

- `crates/rhei-validator/src/lib.rs::Profile`
- `crates/rhei-validator/src/lib.rs::NodePolicy`
- `crates/rhei-validator/src/lib.rs::NodePolicyOverride`
- `crates/rhei-validator/src/lib.rs::StateMachine::profile_for`
- `crates/rhei-validator/src/lib.rs::StateMachine::root_profile`
- `crates/rhei-validator/src/lib.rs::validate_profiles_and_node_policy`
- `crates/rhei-validator/src/lib.rs::validate_task_state_against_profile`
- `crates/rhei-validator/src/lib.rs::validate_terminal_tree_coherence`
- `crates/rhei-cli/src/main.rs::reset_command`
- `crates/rhei-cli/src/main.rs::initial_state_name`
- `crates/rhei-cli/src/main.rs::reset_target_files`
- `crates/rhei-cli/src/main.rs::reset_plan_file_states`
- commands: `rhei validate`, `rhei reset`, `rhei states --json`

Tests, templates, examples:

- `crates/rhei-validator/src/lib.rs` profile and node-policy unit tests
- `crates/rhei-cli/tests/integration_markdown_plans.rs` workspace counted-loop and reset-related tests
- `crates/rhei-cli/tests/e2e/run_tests.rs::reset_bash_agent_team_fixture_restores_initial_state`
- `skills/rhei-state-machine-writer/SKILL.md`
- `.agents/rhei/templates/*/states.yaml`
- `examples/living-review-loop/team-states.yaml`

### SMT-003: Per-State Fields and Validation

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Per-state fields`, `Validation Rules`, `Agent Field`, `Program States`, `MCP Servers and Skills`
- `docs/specs/rhei-transitions.spec.md` -> `State Definition`
- `docs/specs/rhei-state-machine-writer.spec.md` -> `State Design`, `Instructions Design`

Normative claims:

- State definitions may declare `description`, `instructions`, `personality`, `final`, `gating`, `concurrent`, `poll`, `visits`, `target`, `all_targets`, `all_models`, `model`, `agent`, `agent_mode`, `agent_timeout`, `program`, `program_timeout`, `inputs`, `outputs`, `mcp_servers`, and `skills`.
- `description` is required by the YAML state definition.
- `final: true` states are terminal and have no autonomous work.
- `gating: true` states are human-only for autonomous execution.
- `concurrent` is a scheduling hint only; it must not alter state entry, exit, or transition validity.
- `target` and `all_targets` use the specified selector grammar and are mutually exclusive.
- `target` / `all_targets` must not be combined with legacy `model`, `all_models`, `agent`, or `agent_mode`.
- `model` and `all_models` must reference declared machine-level `models`; `model` and `all_models` are mutually exclusive.
- `agent` must be a non-empty registry id, not an inline object; it is invalid on final states and warned or excluded on gating states according to the states spec.
- `agent_mode` requires `agent` and must name a mode on the resolved agent when modes exist.
- `program` must be a non-empty string or valid object with `command`; it is mutually exclusive with `agent` and invalid on final or gating states.
- Timeout fields must parse as valid duration strings.
- `mcp_servers` and `skills` entries may be registry ids or inline definitions with unique ids; they are invalid on gating states and program states.
- Empty `mcp_servers: []` and `skills: []` explicitly clear inherited defaults.

Implementation and artifacts to compare:

- `crates/rhei-validator/src/lib.rs::StateDef`
- `crates/rhei-validator/src/lib.rs::parse_execution_target`
- `crates/rhei-validator/src/lib.rs::validate_model_configuration`
- `crates/rhei-validator/src/lib.rs::validate_program_configuration`
- `crates/rhei-validator/src/lib.rs::validate_tooling_configuration`
- `crates/rhei-validator/src/lib.rs::validate_state_mcp_entries`
- `crates/rhei-validator/src/lib.rs::validate_state_skill_entries`
- `crates/rhei-cli/src/main.rs::state_declares_autonomous_execution`
- `crates/rhei-cli/src/main.rs::resolve_target_agent`
- `crates/rhei-cli/src/main.rs::resolve_agent_invocations`
- `crates/rhei-cli/src/main.rs::resolve_program`
- commands: `rhei validate`, `rhei run`, `rhei states --json`

Tests, templates, examples:

- validator model/target, program, and tooling tests in `crates/rhei-validator/src/lib.rs`
- `crates/rhei-cli/src/main.rs` unit tests for target parsing, tooling resolution, and agent/program behavior
- `.agents/rhei/templates/multi-model-analysis/states.yaml`
- `examples/claude-code/states.yaml`
- `examples/ci-heal/states.yaml`

### SMT-004: Gating and Terminal Behavior

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Per-state fields`, `States`, `Completion paths`
- `docs/specs/rhei-transitions.spec.md` -> `State Definition`, `Wildcard Semantics`
- `docs/specs/rhei-state-machine-writer.spec.md` -> `State Design`, `Instructions Design`

Normative claims:

- Terminal states are absorbing; tasks in final states cannot transition further, including via wildcard transitions.
- Gating states block autonomous commands and engine-triggered transitions out of the state.
- Only explicit human-initiated `rhei transition` may exit a gating state.
- `rhei next` and `rhei run` must skip tasks in gating states.
- `rhei complete` must not complete from a gating state.
- State-machine writer output must model human approval gates explicitly with `gating: true` and human-facing instructions.

Implementation and artifacts to compare:

- `crates/rhei-validator/src/lib.rs::validate_terminal_tree_coherence`
- `crates/rhei-cli/src/main.rs::is_terminal_state`
- `crates/rhei-cli/src/main.rs::find_ready_tasks`
- `crates/rhei-cli/src/main.rs::run_agent_mode`
- `crates/rhei-cli/src/main.rs::next_command`
- `crates/rhei-cli/src/main.rs::complete_command`
- `crates/rhei-cli/src/main.rs::find_completion_state`
- `crates/rhei-cli/src/main.rs::transition_rule_is_applicable`
- commands: `rhei next`, `rhei run`, `rhei complete`, `rhei transition`

Tests, templates, examples:

- `crates/rhei-cli/tests/e2e/run_tests.rs::run_callback_mode_stops_at_human_review`
- `crates/rhei-cli/tests/e2e/run_tests.rs::run_callback_mode_waits_for_other_branches_before_halting_at_human_review`
- `crates/rhei-cli/tests/e2e/run_tests.rs::changeset_review_human_review_state_is_gating_in_shipped_workflows`
- `crates/rhei-cli/tests/e2e/next_tests.rs::complete_fails_when_only_cancelled_terminal_is_available`
- `.agents/rhei/templates/changeset-review/states.yaml`
- `examples/changeset-review-example/states.yaml`
- `examples/human-review-loop.rhei.md`

### SMT-005: Counted Visits and Counted Loops

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Per-state fields`, `Validation Rules`
- `docs/specs/rhei-transitions.spec.md` -> `Rhei File Metadata Format`, `Counted Loop Metadata`, `Metadata Access in Callbacks`, `Counted Loops`
- `docs/specs/rhei-state-machine-writer.spec.md` -> `State Design`, `Transition Design`, `Instructions Design`

Normative claims:

- `visits` is a state-level loop budget and must be an integer greater than or equal to `1`.
- Counted-loop counters are task-instance metadata stored under `metadata.tasks.<id>.stateVisits.<state-name>`.
- First entry into a counted state records visit `1`; first visit is rendered as the bare state name.
- Later re-entry increments the counter and writes `**State:** <state>-<n>` for visits greater than `1`.
- The `-1` suffix is invalid; a suffix is valid only for states that declare `visits` and must not exceed the budget.
- Transitions from counted-loop states expose `visitCount` and `visits` for condition evaluation.
- Once `visitCount >= visits`, loop-back transitions into the same counted state are exhausted and the machine must take another allowed transition.
- When combined with `all_models` or `all_targets`, visit accounting is scoped to each model-specific or target-specific execution.

Implementation and artifacts to compare:

- `crates/rhei-validator/src/lib.rs::StateDef::visits`
- `crates/rhei-validator/src/lib.rs::parse_task_state`
- `crates/rhei-validator/src/lib.rs::validate_task_state_instance`
- `crates/rhei-cli/src/main.rs::task_visit_count`
- `crates/rhei-cli/src/main.rs::state_visit_limit`
- `crates/rhei-cli/src/main.rs::current_state_visit_count`
- `crates/rhei-cli/src/main.rs::loop_reentry_allowed`
- `crates/rhei-cli/src/main.rs::ensure_current_state_visit_count`
- `crates/rhei-cli/src/main.rs::update_metadata_for_transition`
- `crates/rhei-cli/src/main.rs::format_task_state_value`
- `crates/rhei-cli/src/main.rs::render_visit_count`
- `crates/rhei-cli/src/main.rs::evaluate_transition_condition`
- commands: `rhei validate`, `rhei transition`, `rhei run`, `rhei next`

Tests, templates, examples:

- `crates/rhei-validator/src/lib.rs::rejects_state_machine_with_zero_visits`
- validator counted-state suffix tests
- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_counted_loop_updates_metadata_and_blocks_exhausted_reentry`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_from_authored_counted_state_treats_start_as_first_visit`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::workspace_transition_updates_index_metadata_for_counted_loops`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::run_uses_counted_loop_exit_when_visit_budget_is_exhausted`
- `examples/review-fix-visits/states.yaml`

### SMT-006: Polling States

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Polling States`
- `docs/specs/rhei-states.spec.md` -> `Validation Rules`
- `docs/specs/rhei-transitions.spec.md` -> `Counted Loops`

Normative claims:

- `poll` marks a state as time-triggered and contains required `interval` and `max_attempts`.
- `poll.interval` is a valid duration string; `poll.max_attempts` is an integer at least `1`.
- `poll` is invalid on final states and gating states.
- `poll` is mutually exclusive with `visits`.
- A polling state must declare at least one self-loop transition.
- Poll attempts use the same `metadata.tasks.<id>.stateVisits.<state-name>` counter as counted visits.
- A self-loop from a poll state means "retry after interval"; the engine must set `metadata.tasks.<id>.pollNextAttemptAt.<state-name> = now() + interval`, release the `--parallel` slot, and exclude the task until elapsed.
- A non-self-loop exit clears the poll deadline and state visit counter for that state.
- Once attempts reach `poll.max_attempts`, self-loops are refused and the first matching non-self-loop should be selected.
- Poll transitions expose `pollAttempts` and `pollMaxAttempts` only for transitions whose `from` state declares `poll`.

Implementation and artifacts to compare:

- `crates/rhei-validator/src/lib.rs::PollConfig`
- `crates/rhei-validator/src/lib.rs::StateDef::poll`
- `crates/rhei-validator/src/lib.rs::validate_poll_configuration`
- `crates/rhei-cli/src/main.rs::find_ready_tasks`
- `crates/rhei-cli/src/main.rs::find_next_transition`
- `crates/rhei-cli/src/main.rs::transition_rule_is_applicable`
- `crates/rhei-cli/src/main.rs::evaluate_transition_condition`
- `crates/rhei-cli/src/main.rs::update_metadata_for_transition`
- `crates/rhei-cli/src/main.rs::run_agent_mode`
- direct metadata key search target: `pollNextAttemptAt`
- commands: `rhei validate`, `rhei run`

Tests, templates, examples:

- `crates/rhei-validator/src/lib.rs::accepts_well_formed_poll_state`
- `crates/rhei-validator/src/lib.rs::rejects_poll_with_invalid_interval`
- `crates/rhei-validator/src/lib.rs::rejects_poll_with_zero_max_attempts`
- `crates/rhei-validator/src/lib.rs::rejects_poll_with_visits`
- `crates/rhei-validator/src/lib.rs::rejects_poll_on_gating_state`
- `crates/rhei-validator/src/lib.rs::rejects_poll_without_self_loop`
- `examples/ci-heal/states.yaml`
- `examples/ci-heal/.rhei/gh-ci-status.sh`
- `examples/ci-heal/.rhei/commit-and-push.sh`

### SMT-007: Artifact Contracts

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Artifact Contracts`, `Optional Inputs`
- `docs/specs/rhei-transitions.spec.md` -> `State Definition`, `Artifact Enforcement`
- `docs/specs/rhei-state-machine-writer.spec.md` -> `Instructions Design`, `Workflow`

Normative claims:

- `inputs` and `outputs` are arrays of artifact definitions keyed by unique `name`.
- Artifact definitions require `name` and `path`, and may include `description`.
- `optional: true` is valid only for input artifacts.
- Artifact paths are execution-root-relative templates and must not be absolute or escape the execution root after expansion.
- Required inputs are checked before entering the target state and before work begins in the current state.
- Optional inputs do not block entry, but their resolved path and existence are exposed to templates and programs.
- Required outputs are checked after callbacks complete and before transition commit; `rhei complete` is subject to the same output checks.
- Artifact enforcement is file-existence only in v1.
- Artifact path templates support task, state, visit, target, agent, model, and artifact-related variables named by the spec.

Implementation and artifacts to compare:

- `crates/rhei-validator/src/lib.rs::StateArtifactDef`
- `crates/rhei-validator/src/lib.rs::validate_artifact_definitions`
- `crates/rhei-cli/src/main.rs::artifact_relative_path`
- `crates/rhei-cli/src/main.rs::resolve_artifact_path`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist`
- `crates/rhei-cli/src/main.rs::ensure_state_outputs_exist`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist_for_transition`
- `crates/rhei-cli/src/main.rs::ensure_state_outputs_exist_for_transition`
- `crates/rhei-cli/src/main.rs::state_outputs_exist_for_resolved_invocation`
- `crates/rhei-cli/src/main.rs::task_has_pending_agent_invocations`
- `crates/rhei-cli/src/main.rs::resolve_runtime_template_variable`
- commands: `rhei validate`, `rhei next`, `rhei transition`, `rhei complete`, `rhei run`

Tests, templates, examples:

- validator artifact tests in `crates/rhei-validator/src/lib.rs`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_fails_with_explicit_error_when_current_state_input_artifact_is_missing`
- `crates/rhei-cli/tests/e2e/next_tests.rs::complete_fails_when_required_output_artifact_is_missing`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_fails_when_target_state_input_artifact_is_missing`
- `crates/rhei-cli/tests/e2e/fixtures/bash-agent-team/*`
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml`
- `examples/spec-implementation-discrepancy-audit-example/states.yaml`

### SMT-008: Template Variables, Instructions, and Personality

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Template Variables in Instructions and Personality`
- `docs/specs/rhei-transitions.spec.md` -> `TransitionContext Data Structure`, `State Definition`
- `docs/specs/rhei-state-machine-writer.spec.md` -> `Instructions Design`

Normative claims:

- `instructions` and `personality` support `{variable}` substitution and are resolved by `rhei next` at output time.
- Unknown variables remain verbatim; template resolution is fail-open.
- Templates are pure text substitution; transition conditions carry decision logic.
- Conditional blocks use artifact/tooling availability variables where supported.
- `{visit_count}` resolves to the active visit, and `{visits}` is meaningful only for counted-loop states.
- `{input.<name>.path}`, `{input.<name>.exists}`, and `{output.<name>.path}` derive from artifact contracts.
- Target, model, agent, and tooling variables are available only when the relevant state execution context exists.
- State-machine writer instructions should describe domain work and reference concrete artifacts and template variables instead of hand-authored placeholders.

Implementation and artifacts to compare:

- `crates/rhei-cli/src/main.rs::RuntimeTemplateContext`
- `crates/rhei-cli/src/main.rs::resolve_runtime_template_text`
- `crates/rhei-cli/src/main.rs::resolve_runtime_template_variable`
- `crates/rhei-cli/src/main.rs::process_conditional_blocks`
- `crates/rhei-cli/src/main.rs::evaluate_if_condition`
- `crates/rhei-cli/src/main.rs::state_instructions`
- `crates/rhei-cli/src/main.rs::next_command`
- `crates/rhei-cli/src/main.rs::compose_agent_prompt`
- `crates/rhei-cli/src/main.rs::render_state_machine_json`
- commands: `rhei next`, `rhei run`, `rhei states --json`

Tests, templates, examples:

- `crates/rhei-cli/src/main.rs::render_state_machine_json_includes_state_personality`
- `crates/rhei-cli/src/main.rs::compose_agent_prompt_carries_domain_instructions_only`
- `crates/rhei-cli/tests/e2e/templates_tests.rs`
- `.agents/rhei/templates/*/states.yaml`
- `skills/rhei-state-machine-writer/SKILL.md`

### SMT-009: Explicit Transition Selection and Validation

Spec sections:

- `docs/specs/rhei-transitions.spec.md` -> `Requirements`, `Transition Definition`, `Wildcard Semantics`, `Example 8: Transition Validation Flow`
- `docs/specs/rhei-states.spec.md` -> `Transitions`, `Completion paths`
- `docs/specs/rhei-state-machine-writer.spec.md` -> `Transition Design`, `Workflow`

Normative claims:

- Every valid transition must be explicitly declared; unlisted transitions are forbidden.
- A transition declares required `from`, `to`, and `description`.
- `from: "*"` matches any non-final state.
- Specific transitions take precedence over wildcard transitions.
- Engines must not synthesize wildcard transitions; cancellation paths must be explicit.
- A transition from a final state is always forbidden.
- Conditions are evaluated for system-triggered and engine-selected transitions.
- Manual `rhei transition` validates the `--from` current-state compare-and-swap guard and the declared edge.
- Callback redirects must correspond to a declared transition from the original current state.
- `rhei complete` selects a reachable non-cancelled terminal in one hop and must reject when none exists.

Implementation and artifacts to compare:

- `crates/rhei-core/src/ast.rs::TransitionRule`
- `crates/rhei-cli/src/main.rs::transition_command`
- `crates/rhei-cli/src/main.rs::execute_transition`
- `crates/rhei-cli/src/main.rs::transition_rule_is_applicable`
- `crates/rhei-cli/src/main.rs::evaluate_transition_condition`
- `crates/rhei-cli/src/main.rs::find_next_transition`
- `crates/rhei-cli/src/main.rs::find_completion_state`
- `crates/rhei-cli/src/main.rs::complete_command`
- commands: `rhei transition`, `rhei complete`, `rhei run`

Tests, templates, examples:

- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_wildcard_from_allows_any_source`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_wildcard_to_cancelled`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_fails_on_cas_conflict`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_fails_on_invalid_transition`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_redirect_via_next_state_retargets_declared_transition`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_redirect_to_undeclared_transition_is_rejected`
- `.agents/rhei/templates/*/states.yaml`
- `examples/*/states.yaml`

### SMT-010: Transition Triggers and System Routing

Spec sections:

- `docs/specs/rhei-transitions.spec.md` -> `Transition Triggers`
- `docs/specs/rhei-transitions.spec.md` -> `Transition Definition`
- `docs/specs/rhei-states.spec.md` -> `Program States`, `MCP Servers and Skills`, `Polling States`

Normative claims:

- Transitions may be initiated by user, callback, system, or engine triggers.
- User triggers come from explicit API or CLI calls such as `rhei transition`.
- Callback triggers occur when callbacks return a successful `nextState` redirect.
- System triggers cover condition evaluation, timers, program exit codes, agent timeouts, and tooling-unavailable events.
- Engine triggers represent normal autonomous workflow progression during `rhei run`.
- Program exit-code routing evaluates specific integer and array matches before `"nonzero"` transitions, then conditions for disambiguation.
- `exit_code` transitions are only meaningful from program states.
- Agent timeout transitions fire after the configured timeout, include timeout transition data, and execute callbacks normally.
- Tooling-unavailable transitions fire only for unavailable required MCP servers or skills and include unavailable ids in transition data.
- Optional tooling never triggers tooling-unavailable transitions.

Implementation and artifacts to compare:

- `crates/rhei-cli/src/main.rs::transition_command`
- `crates/rhei-cli/src/main.rs::run_agent_mode`
- `crates/rhei-cli/src/main.rs::run_callback_mode`
- `crates/rhei-cli/src/main.rs::try_auto_advance_task`
- `crates/rhei-cli/src/main.rs::find_next_transition`
- `crates/rhei-cli/src/main.rs::find_program_exit_transition`
- `crates/rhei-cli/src/main.rs::transition_matches_exit_code`
- `crates/rhei-cli/src/main.rs::fire_timeout_transition`
- `crates/rhei-cli/src/main.rs::resolve_tooling`
- `crates/rhei-cli/src/main.rs::transition_contexts_for_state`
- `crates/rhei-validator/src/lib.rs::validate_transition_tooling_trigger`
- commands: `rhei transition`, `rhei run`

Tests, templates, examples:

- `crates/rhei-cli/tests/e2e/run_tests.rs::run_executes_program_states_and_routes_on_exit_code`
- `crates/rhei-cli/tests/integration_markdown_plans.rs` run tests for callback/system routing
- `crates/rhei-validator/src/lib.rs` tests for `exit_code`, `mcp_unavailable`, and `skill_unavailable` validation
- `examples/ci-heal/states.yaml`
- `examples/release-automation.rhei.md`

### SMT-011: Callback Declaration, Context, Result, and Error Semantics

Spec sections:

- `docs/specs/rhei-transitions.spec.md` -> `Requirements`, `TransitionContext Data Structure`, `Callback Declaration`, `Callback Mappings`, `Error Handling Configuration`
- `docs/specs/rhei-state-machine-writer.spec.md` -> `Transition Design`, `Workflow`

Normative claims:

- `on_leave` and `on_enter` callbacks are optional; omitted callbacks are implicit success.
- Callbacks receive a `TransitionContext` containing the plan, task, active state definition, transition info, accumulated transition data, and execution environment.
- Callback results support success, redirect, and rejection.
- `success: false` blocks the transition and leaves the task in its current state.
- `success: true, nextState: <state>` redirects only when the target is a declared transition from the current state.
- `success: false` with `nextState` is invalid and treated as rejection.
- Callback `data` from `on_leave` is accumulated into `transitionData` for later callbacks.
- Platform-prefixed callback identifiers and logical callback names with top-level `callbacks:` mappings are both valid.
- Platform identifiers are `cli`, `nodejs`, `python`, and `java`.
- Prefixes take precedence over callback mappings for the same name.
- `on_enter` failure must roll back the state write before surfacing failure or applying configured error-handling policy.
- Callback implementations should be verified as callable before execution begins.

Implementation and artifacts to compare:

- `crates/rhei-core/src/callback.rs::CallbackContext`
- `crates/rhei-core/src/callback.rs::CallbackResult`
- `crates/rhei-core/src/callback.rs::ShellCallbackExecutor`
- `crates/rhei-core/src/callback.rs::NoopCallbackExecutor`
- `crates/rhei-core/src/callback.rs::parse_callback_stdout`
- `crates/rhei-cli/src/main.rs::resolve_callback_paths`
- `crates/rhei-cli/src/main.rs::build_transition_context_json`
- `crates/rhei-cli/src/main.rs::merge_transition_data`
- `crates/rhei-cli/src/main.rs::callback_contexts_for_state`
- `crates/rhei-cli/src/main.rs::execute_transition`
- `crates/rhei-napi/src/lib.rs`
- commands: `rhei transition --no-callbacks`, `rhei complete --no-callbacks`, `rhei run --no-callbacks`

Tests, templates, examples:

- callback tests in `crates/rhei-core/src/callback.rs`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_on_leave_and_on_enter_invoked_on_transition`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_on_leave_failure_blocks_transition`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::no_callbacks_flag_skips_callback_execution`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_unknown_platform_produces_clear_error`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_rejection_surfaces_spec_error_message`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_redirect_via_next_state_retargets_declared_transition`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_redirect_to_undeclared_transition_is_rejected`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_receives_transition_context_on_stdin`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_on_enter_failure_rolls_back_state_write`
- `crates/rhei-cli/tests/e2e/fixtures/bash-agent-team/workflow.sh`
- `crates/rhei-cli/tests/e2e/fixtures/living-review-loop/workflow.sh`

### SMT-012: Transition Context and Task Metadata Shape

Spec sections:

- `docs/specs/rhei-transitions.spec.md` -> `TransitionContext Data Structure`
- `docs/specs/rhei-transitions.spec.md` -> `Rhei File Metadata Format`
- `docs/specs/rhei-transitions.spec.md` -> `Metadata Access in Callbacks`

Normative claims:

- Runtime metadata is stored in YAML frontmatter at the plan root for single-file plans and `index.rhei.md` for Directory Workspaces.
- Markdown-owned task fields remain in markdown and are not duplicated in frontmatter.
- `metadata.tasks.<id>` stores custom metadata and runtime-maintained state visit counters.
- Callback task metadata must expose implicit `state` and `dependsOn` fields and custom metadata.
- Runtimes may project markdown-owned fields such as `assignee` into callback metadata as computed values, not persisted frontmatter.
- Counted-loop callbacks should expose `task.metadata.stateVisits`, `task.metadata.visitCount`, and `state.visits`.
- Transition context state definitions include resolved artifact contracts with `exists` values.
- Environment context includes platform, version, and working directory.

Implementation and artifacts to compare:

- `crates/rhei-core/src/parser.rs`
- `crates/rhei-core/src/ast.rs::Task`
- `crates/rhei-core/src/ast.rs::Rhei`
- `crates/rhei-core/src/callback.rs::CallbackContext`
- `crates/rhei-cli/src/main.rs::build_transition_context_json`
- `crates/rhei-cli/src/main.rs::callback_contexts_for_state`
- `crates/rhei-cli/src/main.rs::resolve_artifact_path`
- `crates/rhei-cli/src/main.rs::task_visit_count`
- commands: `rhei transition`, `rhei complete`, `rhei run`

Tests, templates, examples:

- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_receives_transition_context_on_stdin`
- counted-loop integration tests in `crates/rhei-cli/tests/integration_markdown_plans.rs`
- workspace metadata tests in `crates/rhei-cli/tests/integration_markdown_plans.rs`

### SMT-013: Concurrent State Scheduling Metadata

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Per-state fields`
- `docs/specs/rhei-states.spec.md` -> `Polling States` / `Interaction with other state features`
- `docs/specs/rhei-transitions.spec.md` -> `State Definition`, `Counted Loops`

Normative claims:

- `concurrent: true` allows `rhei run` to work multiple ready tasks in the same state simultaneously up to `--parallel`.
- `concurrent: false` or omitted means at most one ready task per pass is scheduled for that state; remaining ready tasks are deferred to a later pass.
- `concurrent` is independent from fanout execution through `all_targets` or `all_models`.
- `concurrent` is independent from state entry, exit, and transition validity.
- A concurrent poll state may have multiple tasks in flight, each with its own poll deadline.

Implementation and artifacts to compare:

- `crates/rhei-validator/src/lib.rs::StateDef::concurrent`
- `crates/rhei-cli/src/main.rs::run_agent_mode`
- `crates/rhei-cli/src/main.rs::StandaloneExecutionFlags`
- `crates/rhei-cli/src/main.rs::find_ready_tasks`
- commands: `rhei run --parallel <n>`

Tests, templates, examples:

- `crates/rhei-cli/tests/e2e/run_tests.rs` parallel run tests
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml`
- `examples/spec-implementation-discrepancy-audit-example/states.yaml`

### SMT-014: State-Machine Writer Role and Output Contract

Spec sections:

- `docs/specs/rhei-state-machine-writer.spec.md` -> `Purpose`, `Inputs`, `Output`, `Design Rules`, `Workflow`, `Examples`, `File Placement`

Normative claims:

- The state-machine writer produces one YAML file conforming to the YAML State Machine Format, ready to be referenced by a plan's `**States:**` declaration.
- It derives states from distinct workflow phases and transitions from real handoffs, decisions, failure modes, and recovery paths.
- It maps human approval responsibilities to explicit gating states.
- It must mark at least one state final and provide practical cancellation paths.
- It must not use state-level `initial: true`; starting states belong in profiles.
- It should keep state count proportional to workflow complexity.
- It should encode exit conditions structurally using required `outputs:`, `condition`, and `exit_code` fields rather than telling autonomous agents to call transition commands.
- It should define profiles and node policy after state and transition design, using one profile unless node kinds genuinely require different flows.
- It should validate the design manually against profile reachability, final-state reachability, orphan states, identifier naming, and timeout requirements, then validate with `rhei states --state-machine <path>` when the CLI is available.
- Custom state machine files are normally placed in the project root, `.rhei/`, `.agents/rhei/state-machines/`, or alongside a plan/template that references them.

Implementation and artifacts to compare:

- `skills/rhei-state-machine-writer/SKILL.md`
- `skills/rhei-template-writer/SKILL.md`
- `skills/rhei-plan-writer/SKILL.md`
- `skills/rhei-plan-writer/references/default-states.md`
- `.agents/rhei/templates/*/states.yaml`
- `.agents/rhei/templates/*/template.yaml`
- commands: `rhei states --state-machine <path>`, `rhei validate`

Tests, templates, examples:

- template fixtures under `.agents/rhei/templates`
- example state machines under `examples`
- e2e tests that instantiate or validate templates and shipped examples

### SMT-015: Shipped Templates, Examples, and Skills Must Reflect the Spec

Spec sections:

- All in-scope sections above, especially `Output Structure`, `State Design`, `Transition Design`, `Profiles`, `Node Policy`, `Artifact Contracts`, and default `States` / `Transitions`.

Normative claims:

- Shipped templates and examples that include `states.yaml` should conform to the current state-machine schema.
- Human-review states in shipped workflows should be marked `gating: true` when they represent human gates.
- Example workflows for counted visits, polling, artifacts, callbacks, model/target fanout, and program states should exercise the corresponding spec claims without relying on legacy schema forms.
- Skills that describe state-machine authoring or plan execution should not instruct agents to mutate root `**State:**` lines directly under orchestrated execution.
- Default-state documentation in skills should match the enforced default machine.

Implementation and artifacts to compare:

- `skills/rhei-plan-worker/SKILL.md`
- `skills/rhei-plan-writer/references/default-states.md`
- `skills/rhei-state-machine-writer/SKILL.md`
- `skills/rhei-template-writer/SKILL.md`
- `.agents/rhei/templates/changeset-review/states.yaml`
- `.agents/rhei/templates/hourly-human-intervention/states.yaml`
- `.agents/rhei/templates/multi-model-analysis/states.yaml`
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml`
- `.agents/rhei/templates/spec-review/states.yaml`
- `examples/changeset-review-example/states.yaml`
- `examples/ci-heal/states.yaml`
- `examples/claude-code/states.yaml`
- `examples/hourly-human-intervention-example/states.yaml`
- `examples/living-review-loop/team-states.yaml`
- `examples/review-fix-visits/states.yaml`
- `examples/spec-implementation-discrepancy-audit-example/states.yaml`
- `examples/states-with-spaces.yaml`
- `examples/human-review-loop.rhei.md`

Tests and commands:

- `crates/rhei-cli/tests/e2e/run_tests.rs::changeset_review_human_review_state_is_gating_in_shipped_workflows`
- template e2e tests under `crates/rhei-cli/tests/e2e/templates_tests.rs`
- fixture run tests under `crates/rhei-cli/tests/e2e/run_tests.rs`
- commands: `rhei states --state-machine <path>`, `rhei validate <plan-or-workspace>`, `rhei run <plan-or-workspace>`
