# Scope Inventory: Manual Worker and Inspection Commands

Partition task: `manual-commands`

This inventory covers command contracts used by humans and manual agents outside the full `rhei run` orchestrator. It includes the manual coordination commands (`next`, `transition`, `complete`, `reset`), read-only inspection commands (`list`, `states`, `viz`), state-machine semantics those commands depend on, and CLI diagnostics that make failures actionable.

## In Scope

Specification files:

- `docs/specs/rhei-next.spec.md`
- `docs/specs/rhei-transition-cmd.spec.md`
- `docs/specs/rhei-complete.spec.md`
- `docs/specs/rhei-reset.spec.md`
- `docs/specs/rhei-list.spec.md`
- `docs/specs/rhei-states.spec.md`
- `docs/specs/rhei-viz.spec.md`
- `docs/specs/rhei-usage.spec.md` sections: `Roles`, `Coordination Through the State Machine`, `State Flow (Default Machine)`, `Command Surface`, `Pattern 1`, `Pattern 2`, `Pattern 3`, `The Plan as Shared Memory`

Implementation roots:

- `crates`
- `skills`
- `.agents/rhei/templates`
- `examples`

Adjacent graph-rendering prototype to check explicitly even though it is outside the requested implementation roots:

- `xtask/src/viz.rs`
- `xtask/src/main.rs`
- `xtask/assets/viz-template.html`

## Shared Implementation Surfaces

Primary CLI surface:

- `crates/rhei-cli/src/main.rs`
  - `Cli`
  - `Commands::{States,List,Transition,Next,Complete,Reset}`
  - `dispatch`
  - `command_wants_json`
  - `emit_json_error`
  - `states_command`
  - `list_command`
  - `transition_command`
  - `execute_transition`
  - `next_command`
  - `complete_command`
  - `reset_command`
  - `render_state_machine_text`
  - `render_state_machine_json`
  - `find_ready_tasks`
  - `find_claimable_tasks`
  - `diagnose_no_claimable`
  - `dependency_is_satisfied`
  - `is_terminal_state`
  - `state_declares_autonomous_execution`
  - `find_next_transition`
  - `find_completion_state`
  - `non_terminal_descendants`
  - `insert_task_assignee`
  - `write_task_assignee`
  - `rewrite_task_state`
  - `rewrite_task_completion`
  - `rewrite_all_states_to_initial`
  - `strip_result_links`
  - `append_result_entry`
  - `ensure_state_inputs_exist`
  - `ensure_state_outputs_exist`
  - `ensure_state_inputs_exist_for_transition`
  - `ensure_state_outputs_exist_for_transition`
  - `resolve_state_machine_for_loaded_plan`
  - `load_plan`
  - `render_parse_diagnostic`
  - `render_multi_parse_diagnostic`
  - `render_validation_diagnostic`
  - shell completion helpers: `complete_task_id`, `complete_transition_from_state`, `complete_transition_to_state`, `complete_state_name`, `complete_assignee`, `complete_node_kind`, `complete_limit`

Shared parsing and state-machine surfaces:

- `crates/rhei-core/src/ast.rs`
  - `Task::{id,kind,title,state,prior,assignee,content,children}`
  - `Rhei::{title,states,structure,metadata,content_sections,tasks}`
  - `TransitionRule::{from,to,on_leave,on_enter,condition,timeout,exit_code,mcp_unavailable,skill_unavailable}`
  - `StateName`, `CallbackRef`
- `crates/rhei-core/src/parser.rs`
  - `parse`
  - `parse_workspace_index`
  - `parse_workspace_tasks`
  - metadata regexes for `**States:**`, `**State:**`, `**Prior:**`, `**Assignee:**`
  - assignee ordering and duplicate diagnostics
- `crates/rhei-core/src/workspace.rs`
  - `is_workspace`
  - `load_workspace`
- `crates/rhei-validator/src/lib.rs`
  - `StateArtifactDef`
  - `StateDef`
  - `PollConfig`
  - `Profile`
  - `NodePolicy`
  - `NodePolicyOverride`
  - `StateMachine`
  - `StateMachine::{builtin_default,from_yaml_str,from_yaml_file,is_valid_state,allowed_states,transitions,profile_for,root_profile}`
  - `parse_task_state`
  - `validate_with_machine`
  - `validate_with_machine_and_base`
  - state machine validation helpers for models, programs, tooling, template conditions, profiles/node policy, polling, artifact paths
- `crates/rhei-validator/src/default-states.yaml`
- `docs/specs/states.yaml`

Test and fixture surfaces:

- `crates/rhei-cli/tests/e2e/next_tests.rs`
- `crates/rhei-cli/tests/e2e/transition_tests.rs`
- `crates/rhei-cli/tests/e2e/completions_tests.rs`
- `crates/rhei-cli/tests/e2e/mod.rs`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
- `crates/rhei-cli/src/main.rs` unit tests around `parses_states_command`, `render_state_machine_text_includes_states_and_transitions`, `render_state_machine_json_includes_state_personality`, `parses_complete_command_with_result`, `parses_complete_command_requires_result`, `parses_reset_command`, `find_completion_state_*`, `rewrite_task_completion_*`, `rewrite_all_states_to_initial_updates_tasks_and_children`
- `crates/rhei-core/tests/*`
- `crates/rhei-cli/tests/e2e/fixtures/living-review-loop/*`
- `crates/rhei-cli/tests/e2e/fixtures/bash-agent-team/*`

Skill/template/example surfaces:

- `skills/rhei-plan-worker/SKILL.md`
- `skills/rhei-plan-writer/references/default-states.md`
- `skills/rhei-state-machine-writer/SKILL.md`
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/*`
- `.agents/rhei/templates/spec-review/*`
- `.agents/rhei/templates/changeset-review/*`
- `.agents/rhei/templates/hourly-human-intervention/*`
- `.agents/rhei/templates/multi-model-analysis/*`
- `examples/spec-implementation-discrepancy-audit-example/*`
- `examples/review-fix-visits/*`
- `examples/living-review-loop/*`
- `examples/changeset-review-example/*`
- `examples/hourly-human-intervention-example/*`
- `examples/ci-heal/*`
- `examples/claude-code/*`
- `examples/escaped-state-values.rhei.md`
- `examples/states-with-spaces.yaml`
- `examples/human-review-loop.rhei.md`

## Command Surface Claims

### MC-000: Role Boundaries and Manual Loop

Spec sections:

- `docs/specs/rhei-usage.spec.md` -> `Roles`
- `docs/specs/rhei-usage.spec.md` -> `Coordination Through the State Machine`
- `docs/specs/rhei-usage.spec.md` -> `Command Surface`
- `docs/specs/rhei-usage.spec.md` -> `Pattern 1: Single Agent, Start to Finish`
- `docs/specs/rhei-usage.spec.md` -> `Pattern 2: Writer and Worker as Separate Sessions`
- `docs/specs/rhei-usage.spec.md` -> `Pattern 3: Parallel Workers on Independent Branches`
- `docs/specs/rhei-usage.spec.md` -> `The Plan as Shared Memory`

Normative claims:

- Plan workers claim eligible tasks with `rhei next`, work according to state `instructions`, advance with `rhei transition`, finish with `rhei complete`, and stop at terminal or gating states.
- Under `rhei run`, spawned workers must not mutate `**State:**`; the orchestrator owns state changes.
- Manual worker commands and `rhei run` are mutually exclusive per task execution.
- The command loop is `next` -> work -> `transition` as needed -> `complete`.
- The plan file is the single source of truth for state, progress, and resumption.
- Human reviewers are the only role that can unblock `human-review` gates.

Implementation and user-facing commands to compare:

- `rhei next`, `rhei next --peek`
- `rhei transition`
- `rhei complete`
- `rhei reset`
- `rhei list`
- `rhei states`
- `rhei run` prompt text only where it tells spawned workers not to transition
- `crates/rhei-cli/src/main.rs::compose_agent_prompt`
- `crates/rhei-cli/src/main.rs` unit test `compose_agent_prompt_carries_domain_instructions_only`
- `skills/rhei-plan-worker/SKILL.md`
- `skills/rhei-plan-writer/references/default-states.md`

### MC-001: Default State Flow, Gating, Instructions, and Completion Paths

Spec sections:

- `docs/specs/rhei-usage.spec.md` -> `State Flow (Default Machine)`
- `docs/specs/rhei-states.spec.md` -> `Per-state fields`
- `docs/specs/rhei-states.spec.md` -> `States`
- `docs/specs/rhei-states.spec.md` -> `Transitions`
- `docs/specs/rhei-states.spec.md` -> `Completion paths`

Normative claims:

- Default states are `draft`, `pending`, `agent-review`, `agent-review-fix`, `human-review`, `completed`, `cancelled`.
- `completed` and `cancelled` are final.
- `human-review` is `gating: true`; autonomous commands must not transition out of it.
- Legal default transitions are exactly those listed in `docs/specs/rhei-states.spec.md#transitions` / `docs/specs/states.yaml`.
- Any unlisted transition is forbidden.
- `pending` and `agent-review` can be completed directly to `completed`.
- `agent-review-fix` cannot complete directly; it must transition to `agent-review` first.
- `human-review` cannot be completed autonomously; only explicit human `rhei transition` may exit it.
- Agents must follow `instructions` in the current state.

Implementation and artifacts:

- `crates/rhei-validator/src/default-states.yaml`
- `docs/specs/states.yaml`
- `crates/rhei-validator/src/lib.rs::StateDef`
- `crates/rhei-validator/src/lib.rs::StateMachine`
- `crates/rhei-cli/src/main.rs::is_terminal_state`
- `crates/rhei-cli/src/main.rs::find_ready_tasks`
- `crates/rhei-cli/src/main.rs::find_completion_state`
- `crates/rhei-cli/src/main.rs::complete_command`
- `crates/rhei-cli/src/main.rs::transition_command`
- `skills/rhei-plan-writer/references/default-states.md`
- `skills/rhei-plan-worker/SKILL.md`
- `examples/human-review-loop.rhei.md`

### MC-002: State Machine Schema Used By Commands

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Schema Additions`
- `docs/specs/rhei-states.spec.md` -> `Top-level fields`
- `docs/specs/rhei-states.spec.md` -> `Per-state fields`
- `docs/specs/rhei-states.spec.md` -> `Validation Rules`
- `docs/specs/rhei-states.spec.md` -> `Artifact Contracts`
- `docs/specs/rhei-states.spec.md` -> `Profiles`
- `docs/specs/rhei-states.spec.md` -> `Node Policy`

Normative claims:

- State-machine YAML supports `models`, `profiles`, `node_policy`, `states`, and `transitions`.
- Per-state fields relevant here include `description`, `instructions`, `personality`, `gating`, `visits`, `target`, `all_targets`, `model`, `all_models`, `agent`, `agent_mode`, `inputs`, `outputs`, `mcp_servers`, `skills`, `program`, and `poll`.
- `gating: true` blocks autonomous `rhei next`, `rhei complete`, and engine-triggered transitions out of the state.
- `inputs` must exist before state entry/claim unless `optional: true`.
- `outputs` must exist before leaving a state; `optional: true` is invalid on outputs.
- Artifact paths are execution-root-relative and must not escape the root after template expansion.
- `profiles` define `{initial, allowed}`; `node_policy` resolves a node's profile.
- `rhei reset` resets each node to its resolved profile's `initial`.
- Authored `**State:**` values must be in the node's resolved profile `allowed` set.

Implementation and artifacts:

- `crates/rhei-validator/src/lib.rs::StateArtifactDef`
- `crates/rhei-validator/src/lib.rs::StateDef`
- `crates/rhei-validator/src/lib.rs::Profile`
- `crates/rhei-validator/src/lib.rs::NodePolicy`
- `crates/rhei-validator/src/lib.rs::StateMachine`
- `crates/rhei-validator/src/lib.rs::validate_profiles_and_node_policy`
- `crates/rhei-validator/src/lib.rs::validate_task_state_against_profile`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist`
- `crates/rhei-cli/src/main.rs::ensure_state_outputs_exist`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist_for_transition`
- `crates/rhei-cli/src/main.rs::ensure_state_outputs_exist_for_transition`
- `crates/rhei-cli/src/main.rs::reset_command`
- `crates/rhei-cli/src/main.rs::initial_state_name`
- `crates/rhei-cli/src/main.rs::rewrite_all_states_to_initial`
- Templates/examples with artifacts and gates:
  - `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml`
  - `.agents/rhei/templates/spec-review/states.yaml`
  - `.agents/rhei/templates/changeset-review/states.yaml`
  - `.agents/rhei/templates/hourly-human-intervention/states.yaml`
  - `examples/review-fix-visits/states.yaml`
  - `examples/ci-heal/states.yaml`

### MC-003: Template Variables in Instructions and Personality

Spec sections:

- `docs/specs/rhei-states.spec.md` -> `Template Variables in Instructions and Personality`
- `docs/specs/rhei-next.spec.md` -> `Output (claim mode)`
- `docs/specs/rhei-next.spec.md` -> `Agent Context`

Normative claims:

- `rhei next` resolves template variables at output time, not load time.
- Variables include `{task_id}`, `{task_title}`, `{state}`, `{visit_count}`, `{visits}`, `{target}`, `{target.slug}`, `{model}`, `{model.provider}`, `{model.name}`, `{agent}`, `{agent.mode}`, `{plan_title}`, `{plan_path}`, `{input.<name>.path}`, `{input.<name>.exists}`, `{output.<name>.path}`, `{mcp.<name>.available}`, `{skill.<id>.available}`, and `{meta.<key>}`.
- Unknown variables fail open and remain verbatim.
- Templates are pure substitution; decision logic belongs in transition conditions.
- Conditional blocks for input/MCP/skill availability suppress whole paragraphs and cannot be nested in v1.
- `rhei next` prints resolved `instructions` and `personality`.

Implementation and artifacts:

- `crates/rhei-cli/src/main.rs::RuntimeTemplateContext`
- `crates/rhei-cli/src/main.rs::resolve_runtime_template_text`
- `crates/rhei-cli/src/main.rs::resolve_runtime_template_variable`
- `crates/rhei-cli/src/main.rs::next_command`
- `crates/rhei-cli/src/main.rs::print_next_output`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_resolves_runtime_template_variables_in_instructions`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_prints_state_personality_in_text_and_json_when_configured`
- `examples/review-fix-visits/states.yaml`
- `.agents/rhei/templates/spec-review/states.yaml`
- `skills/rhei-state-machine-writer/SKILL.md`

## `rhei next` Claims

### MC-010: Usage, Options, and Output Modes

Spec sections:

- `docs/specs/rhei-next.spec.md` -> `Usage`
- `docs/specs/rhei-next.spec.md` -> `Options`
- `docs/specs/rhei-next.spec.md` -> `Output (claim mode)`
- `docs/specs/rhei-next.spec.md` -> `Peek Mode (--peek)`
- `docs/specs/rhei-next.spec.md` -> `Agent Context`

Normative claims:

- Usage is `rhei next <RHEI_PLAN> [--peek]`.
- `--peek` is optional and defaults false.
- Claim-mode text output is `Task <ID>: <title>`, `State: <current-state>`, blank line, resolved instructions.
- Peek output is `Next: Task <ID>: <title>` and `State: <current-state>`.
- JSON output includes `task_id`, `title`, `state`, optional `agent`, optional `model`, optional `model_provider`, optional `model_name`, and `instructions`.
- Text output shows agent after state when agent/model is configured.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::Commands::Next`
- `crates/rhei-cli/src/main.rs::next_command`
- `crates/rhei-cli/src/main.rs::print_next_output`
- `crates/rhei-cli/src/main.rs::command_wants_json`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_single_file_json_output`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_json_includes_children`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_prints_state_personality_in_text_and_json_when_configured`
- `crates/rhei-cli/tests/e2e/completions_tests.rs::dynamic_completion_completes_task_ids_and_transition_targets`

### MC-011: Claimability and Plan Order

Spec sections:

- `docs/specs/rhei-next.spec.md` -> `Default Behavior (Claim Mode)`
- `docs/specs/rhei-next.spec.md` -> `Behavior`
- `docs/specs/rhei-usage.spec.md` -> `Plan Worker`

Normative claims:

- A task is claimable when all `**Prior:**` tasks are terminal, it has no `**Assignee:**`, its state is not final and not gating, and all required current-state `inputs` exist.
- The task's current state is not advanced by claiming; the worker works in that state and later calls `rhei transition` or `rhei complete`.
- Initial states with runnable autonomous work (`program`, `agent`, `target`, `all_targets`, `model`, `all_models`) are claimed and presented in place.
- Non-runnable initial states are auto-advanced to the first forward state before printing instructions.
- Tasks are scanned and selected in plan order.
- The first otherwise-claimable task with missing required inputs causes an immediate error; later tasks must not be skipped.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::find_ready_tasks`
- `crates/rhei-cli/src/main.rs::find_claimable_tasks`
- `crates/rhei-cli/src/main.rs::dependency_is_satisfied`
- `crates/rhei-cli/src/main.rs::state_declares_autonomous_execution`
- `crates/rhei-cli/src/main.rs::find_next_transition`
- `crates/rhei-cli/src/main.rs::next_command`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist_for_transition`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_respects_dependency_order`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_with_task_flag_targets_specific`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_does_not_auto_transition_runnable_initial_states`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_does_not_allow_cancelled_prerequisite_to_unblock_dependents`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_fails_with_explicit_error_when_current_state_input_artifact_is_missing`

### MC-012: Atomic Claim, Assignee, and Agent Resolution

Spec sections:

- `docs/specs/rhei-next.spec.md` -> `Behavior`
- `docs/specs/rhei-next.spec.md` -> `Agent Context`
- `docs/specs/rhei-usage.spec.md` -> `Plan Worker`

Normative claims:

- Claim mode acquires a file lock, re-reads and re-validates claimability under the lock, writes atomically, and releases the lock.
- Claim mode writes `**Assignee:** <current-agent>`.
- `<current-agent>` is resolved through state `agent:` then project settings then global settings.
- If no agent is configured, no placeholder assignee is written.
- `rhei transition` does not alter `**Assignee:**`; `rhei complete` removes it.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::resolve_agent`
- `crates/rhei-cli/src/main.rs::next_command`
- `crates/rhei-cli/src/main.rs::insert_task_assignee`
- `crates/rhei-cli/src/main.rs::write_task_assignee`
- `crates/rhei-core/src/parser.rs` assignee parsing
- `crates/rhei-core/src/ast.rs::Task::assignee`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::assignee_field_round_trips_through_parse_and_json`
- `crates/rhei-cli/src/main.rs` unit tests around `rewrite_task_completion_removes_assignee_and_appends_result_link`
- `skills/rhei-plan-worker/SKILL.md` -> `Assignee Discipline`

### MC-013: Peek Mode Read-Only Behavior

Spec sections:

- `docs/specs/rhei-next.spec.md` -> `Peek Mode (--peek)`
- `docs/specs/rhei-next.spec.md` -> `Output (peek mode)`
- `docs/specs/rhei-usage.spec.md` -> `Pattern 3: Parallel Workers on Independent Branches`

Normative claims:

- `rhei next --peek` performs a read-only scan.
- Peek mode must not acquire a lock, modify state, append result files, or set/clear `**Assignee:**`.
- Peek mode still resolves required inputs for the first otherwise-claimable task and fails with the same missing-artifact error.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::Commands::Next { peek }`
- `crates/rhei-cli/src/main.rs::next_command`
- `crates/rhei-cli/src/main.rs::print_next_output`
- `skills/rhei-plan-worker/SKILL.md` mentions `rhei next <plan> --peek`
- Add/verify dedicated behavior tests in `crates/rhei-cli/tests/e2e/next_tests.rs`

### MC-014: No-Task Status and Missing Artifact Diagnostics

Spec sections:

- `docs/specs/rhei-next.spec.md` -> `Missing Artifact Error`
- `docs/specs/rhei-next.spec.md` -> `No Tasks Ready`

Normative claims:

- Missing input error text includes `Task <ID> cannot be claimed in state <state>.` and `Missing required input artifact: <name> (<path>)`.
- No claimable task prints one of three summaries: all terminal, gating/human action, or all in-flight/claimed.
- These summaries let PMs/orchestrators distinguish complete, blocked, and in-flight plans.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::diagnose_no_claimable`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_fails_when_all_completed`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_fails_with_explicit_error_when_current_state_input_artifact_is_missing`
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_does_not_allow_cancelled_prerequisite_to_unblock_dependents`

## `rhei transition` Claims

### MC-020: Usage, Options, and State Values

Spec sections:

- `docs/specs/rhei-transition-cmd.spec.md` -> `Usage`
- `docs/specs/rhei-transition-cmd.spec.md` -> `Options`

Normative claims:

- Usage is `rhei transition <RHEI_PLAN> --task <TASK_ID> --from <STATE> --to <STATE>`.
- Required flags: `--task`, `--from`, `--to`.
- Optional flag: `--no-callbacks`, default false.
- `--from` and `--to` state values follow main spec state rendering rules: bare identifiers or backtick-wrapped for non-identifier names.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::Commands::Transition`
- `crates/rhei-cli/src/main.rs::transition_command`
- `crates/rhei-cli/src/main.rs::parse_task_id`
- `crates/rhei-cli/src/main.rs::normalized_state_name`
- `crates/rhei-cli/src/main.rs::format_task_state_value`
- `crates/rhei-cli/tests/e2e/transition_tests.rs`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_works_with_named_task_id`
- `examples/escaped-state-values.rhei.md`
- `examples/states-with-spaces.yaml`

### MC-021: CAS, Locking, Transition Authorization, and Atomic Writes

Spec sections:

- `docs/specs/rhei-transition-cmd.spec.md` -> `Behavior`
- `docs/specs/rhei-transition-cmd.spec.md` -> `Compare-and-Swap Conflicts`

Normative claims:

- Command loads and validates state machine and plan, locates task by id, and fails if missing.
- It acquires a file lock on the plan file or containing task file.
- It re-reads current state under lock.
- If actual state differs from `--from`, it fails non-zero and prints actual state.
- It validates that a declared transition exists from `--from` to `--to`; unlisted edges are rejected.
- It rewrites the task `**State:**` line to target state, including counted visit suffix when applicable.
- It writes task files atomically by temp file plus rename.
- Losing workers must re-read the plan and reselect/retry against the new state.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::transition_command`
- `crates/rhei-cli/src/main.rs::execute_transition`
- `crates/rhei-cli/src/main.rs::find_task_current_state`
- `crates/rhei-cli/src/main.rs::rewrite_task_state`
- `crates/rhei-cli/src/main.rs::write_file_atomic`
- `crates/rhei-cli/src/main.rs::update_metadata_for_transition`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_single_file_full_advancement`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_cas_rejects_wrong_from`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_cas_rejects_after_concurrent_change`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_workspace_updates_correct_file`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_disallowed_path_rejected`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_fails_on_cas_conflict`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_fails_on_invalid_transition`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_counted_loop_updates_metadata_and_blocks_exhausted_reentry`

### MC-022: Artifact Enforcement and Callbacks

Spec sections:

- `docs/specs/rhei-transition-cmd.spec.md` -> `Behavior`
- `docs/specs/rhei-states.spec.md` -> `Artifact Contracts`

Normative claims:

- Every required `outputs:` artifact on the source state must exist before leaving the state.
- Every required `inputs:` artifact on the target state must exist before entering the target.
- `on_leave` is executed before the state change unless `--no-callbacks` is set.
- `on_enter` is executed after the state change unless `--no-callbacks` is set.
- `--no-callbacks` skips both callback types.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::ensure_state_outputs_exist_for_transition`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist_for_transition`
- `crates/rhei-cli/src/main.rs::execute_transition`
- `crates/rhei-core/src/callback.rs`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_fails_when_target_state_input_artifact_is_missing`
- `crates/rhei-cli/tests/e2e/next_tests.rs::complete_fails_when_required_output_artifact_is_missing`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_on_leave_and_on_enter_invoked_on_transition`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_on_leave_failure_blocks_transition`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::no_callbacks_flag_skips_callback_execution`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::callback_on_enter_failure_rolls_back_state_write`

### MC-023: Transition Audit Trail, Assignee Preservation, and Output

Spec sections:

- `docs/specs/rhei-transition-cmd.spec.md` -> `Behavior`
- `docs/specs/rhei-transition-cmd.spec.md` -> `Output`
- `docs/specs/rhei-transition-cmd.spec.md` -> `Relationship to Other Commands`
- `docs/specs/rhei-complete.spec.md` -> `Result File Format`

Normative claims:

- `rhei transition` appends a `## <from> -> <to>` audit entry with no message body to `runtime/results/<task-id>.md`.
- It creates `runtime/results/` if needed.
- It does not add, remove, or modify `**Assignee:**`.
- Success output is `Task <ID> transitioned: '<from>' -> '<to>'`.
- With `--no-callbacks`, success output includes `(callbacks skipped)`.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::transition_command`
- `crates/rhei-cli/src/main.rs::execute_transition`
- `crates/rhei-cli/src/main.rs::append_result_entry`
- `crates/rhei-cli/tests/e2e/transition_tests.rs`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::transition_succeeds_and_updates_file`
- Add/verify tests that transition appends result entries and preserves `**Assignee:**`.

## `rhei complete` Claims

### MC-030: Usage, Required Result, and Output

Spec sections:

- `docs/specs/rhei-complete.spec.md` -> `Usage`
- `docs/specs/rhei-complete.spec.md` -> `Options`
- `docs/specs/rhei-complete.spec.md` -> `Output`
- `docs/specs/rhei-complete.spec.md` -> `Examples`

Normative claims:

- Usage is `rhei complete <RHEI_PLAN> --task <TASK_ID> --result <MESSAGE>`.
- Required flags: `--task`, `--result`.
- Optional flag: `--no-callbacks`, default false.
- Success output is `Task <ID> completed: '<from>' -> '<to>' (runtime/results/<ID>.md)`.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::Commands::Complete`
- `crates/rhei-cli/src/main.rs::complete_command`
- `crates/rhei-cli/src/main.rs` unit tests `parses_complete_command_with_result`, `parses_complete_command_requires_result`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::run_complete`

### MC-031: Completion Eligibility and Target Selection

Spec sections:

- `docs/specs/rhei-complete.spec.md` -> `Behavior`
- `docs/specs/rhei-complete.spec.md` -> `Completion Target Selection`
- `docs/specs/rhei-states.spec.md` -> `Completion paths`

Normative claims:

- Load and validate plan and machine; locate task by id.
- Reject if task is already terminal.
- Reject if current state is gating.
- Reject if any descendant task node is non-terminal.
- Completion target is the first non-cancelled terminal state reachable in one declared transition from the current state.
- If no such target exists, fail.
- `cancelled` is never treated as a successful completion target.
- Transition declaration order in YAML is significant when multiple terminal targets exist.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::complete_command`
- `crates/rhei-cli/src/main.rs::find_completion_state`
- `crates/rhei-cli/src/main.rs::non_terminal_descendants`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::complete_rejects_parent_with_non_terminal_subtasks`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::complete_succeeds_when_all_subtasks_are_terminal`
- `crates/rhei-cli/tests/e2e/next_tests.rs::complete_fails_when_only_cancelled_terminal_is_available`
- `crates/rhei-cli/src/main.rs` unit tests `find_completion_state_prefers_non_cancelled_terminal`, `find_completion_state_does_not_fall_back_to_cancelled`, `find_completion_state_returns_none_when_no_terminal_reachable`
- Add/verify explicit gating-state rejection test.

### MC-032: Completion Result File, Link Placement, and Assignee Removal

Spec sections:

- `docs/specs/rhei-complete.spec.md` -> `Result File`
- `docs/specs/rhei-complete.spec.md` -> `Result File Format`
- `docs/specs/rhei-complete.spec.md` -> `Single-File Plans`
- `docs/specs/rhei-complete.spec.md` -> `Directory Workspaces`

Normative claims:

- Result file path is `runtime/results/<task-id>.md` under plan parent for single-file plans or workspace root for directory workspaces.
- `runtime/results/` is created if missing.
- The task body receives `> **Result:** [<task-id>](runtime/results/<task-id>.md)` after task content and before child nodes.
- Result files accumulate one markdown section per state transition.
- Heading format is `## <from> -> <to>`, followed by a blank line and optional message.
- `rhei transition` appends an entry with no message body.
- `rhei complete` appends an entry with the mandatory `--result` message.
- `rhei complete` removes the task's `**Assignee:**` line; no-op if absent.
- If the result link already exists, it is not duplicated.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::result_workspace_root`
- `crates/rhei-cli/src/main.rs::append_result_entry`
- `crates/rhei-cli/src/main.rs::rewrite_task_completion`
- `crates/rhei-cli/src/main.rs::complete_command`
- `crates/rhei-cli/src/main.rs` unit tests `rewrite_task_completion_removes_assignee_and_appends_result_link`, `rewrite_task_completion_without_assignee_still_appends_result_link`, `rewrite_task_completion_inserts_result_link_before_child`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::complete_succeeds_when_all_subtasks_are_terminal`
- `skills/rhei-plan-worker/SKILL.md` -> completion result behavior

### MC-033: Completion Transition Execution

Spec sections:

- `docs/specs/rhei-complete.spec.md` -> `Behavior`

Normative claims:

- `rhei complete` executes the state transition directly with compare-and-swap and callbacks.
- It does not delegate to `rhei transition`.
- Only one result entry is appended per `rhei complete` invocation.
- It writes atomically.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::complete_command`
- `crates/rhei-cli/src/main.rs::execute_transition`
- `crates/rhei-cli/src/main.rs::append_result_entry`
- Callback tests in `crates/rhei-cli/tests/integration_markdown_plans.rs`
- Add/verify test that `complete` does not create duplicate result entries.

## `rhei reset` Claims

### MC-040: Usage, Scope, Safety, and Output

Spec sections:

- `docs/specs/rhei-reset.spec.md` -> `Usage`
- `docs/specs/rhei-reset.spec.md` -> `Behavior`
- `docs/specs/rhei-reset.spec.md` -> `Safety`
- `docs/specs/rhei-reset.spec.md` -> `Output`

Normative claims:

- Usage is `rhei reset <RHEI_PLAN_OR_WORKSPACE>`.
- Input can be a single-file `.rhei.md` plan or directory workspace root.
- Reset refuses to operate on an invalid plan.
- Reset is destructive for runtime state; it does not prompt and has no `--dry-run`.
- It is safe against concurrent `next` / `transition` / `complete` via locking.
- Output line 1 reports reset count and initial state; when child tasks exist it also reports descendant count.
- Output line 2 reports either `Removed runtime output.` or `No runtime output was present.`

Implementation and tests:

- `crates/rhei-cli/src/main.rs::Commands::Reset`
- `crates/rhei-cli/src/main.rs::reset_command`
- `crates/rhei-cli/src/main.rs` unit test `parses_reset_command`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::reset_restores_single_file_plan_to_initial_state`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::workspace_reset_restores_initial_states_and_removes_runtime`
- `crates/rhei-cli/tests/e2e/fixtures/bash-agent-team/README.md`
- `examples/living-review-loop/README.md`

### MC-041: Reset Semantics

Spec sections:

- `docs/specs/rhei-reset.spec.md` -> `Behavior`
- `docs/specs/rhei-reset.spec.md` -> `Relationship to Other Commands`
- `docs/specs/rhei-states.spec.md` -> `Node Policy`

Normative claims:

- For every task node in the merged graph, including descendants, resolve profile through `node_policy`.
- Rewrite `**State:**` to that node's resolved profile `initial`.
- Remove `**Assignee:**` if present.
- Remove `> **Result:**` link blocks.
- Clear counted visit suffixes and delete `metadata.tasks.<id>.stateVisits` entries.
- Delete `runtime/` under workspace root or plan parent.
- Do not modify `# Rhei:` title, content sections, `**Prior:**`, task descriptions, state machine, template source, or user-authored files outside `runtime/`.
- Write each modified task file atomically.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::reset_command`
- `crates/rhei-cli/src/main.rs::initial_state_name`
- `crates/rhei-cli/src/main.rs::reset_target_files`
- `crates/rhei-cli/src/main.rs::reset_plan_file_states`
- `crates/rhei-cli/src/main.rs::clear_runtime_metadata_in_file`
- `crates/rhei-cli/src/main.rs::strip_result_links`
- `crates/rhei-cli/src/main.rs::rewrite_all_states_to_initial`
- `crates/rhei-cli/src/main.rs::clear_runtime_state_visits`
- `crates/rhei-cli/src/main.rs` unit test `rewrite_all_states_to_initial_updates_tasks_and_children`
- `crates/rhei-cli/tests/integration_markdown_plans.rs::workspace_reset_restores_initial_states_and_removes_runtime`
- `examples/changeset-review-example/index.rhei.md`

## `rhei list` Claims

### MC-050: Usage and Filters

Spec sections:

- `docs/specs/rhei-list.spec.md` -> `Usage`
- `docs/specs/rhei-list.spec.md` -> `Options`

Normative claims:

- Usage is `rhei list <RHEI_PLAN> [FILTERS] [--limit N] [--json]`.
- Input can be a single-file plan or directory workspace path.
- Supported filters are `--state`, `--assignee`, `--no-assignee`, `--kind`, `--has-prior`, `--parent`, `--root`, `--contains`, `--terminal`, `--non-terminal`, `--ready`, `--blocked`, `--limit`, and `--json`.
- `--state` is repeatable and comma-separated values are accepted.
- `--assignee` conflicts with `--no-assignee`.
- `--parent` conflicts with `--root`.
- `--terminal` conflicts with `--non-terminal`.
- `--ready` conflicts with `--blocked`.
- Filters combine by logical AND.
- Empty result sets are not errors.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::Commands::List`
- `crates/rhei-cli/src/main.rs::ListFilters`
- `crates/rhei-cli/src/main.rs::list_command`
- `crates/rhei-cli/tests/e2e/completions_tests.rs::dynamic_completion_completes_list_filters`
- Add/verify dedicated list behavior tests; current obvious tests are completion-surface only.

### MC-051: List Read-Only Behavior and Readiness Semantics

Spec sections:

- `docs/specs/rhei-list.spec.md` -> `Behavior`
- `docs/specs/rhei-list.spec.md` -> `Relationship to Other Commands`

Normative claims:

- `rhei list` loads plan and resolves state machine the same way `rhei validate` does.
- It walks the task tree in source order, recording parent id.
- It normalizes filter states and task states through the state machine.
- For `--ready` / `--blocked`, it evaluates prerequisites using the same dependency rule as `rhei next`; the list spec states terminal, non-cancelled prerequisites.
- `--ready` requires non-terminal and non-gating state.
- `--blocked` shows non-terminal tasks with at least one unsatisfied prerequisite.
- `--limit` applies after filtering; `0` means no limit.
- It never mutates plan state and acquires no lock.
- `rhei list --ready` returns all ready tasks; `rhei next --peek` returns the single next claimable task.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::list_command`
- `crates/rhei-cli/src/main.rs::normalized_state_name`
- `crates/rhei-cli/src/main.rs::dependency_is_satisfied`
- `crates/rhei-cli/src/main.rs::is_terminal_state`
- `crates/rhei-cli/src/main.rs::title_case_kind`
- `crates/rhei-core/src/workspace.rs::load_workspace`
- Add/verify dedicated list tests for all filters, source order, hierarchy, and read-only behavior.

### MC-052: List Text and JSON Output

Spec sections:

- `docs/specs/rhei-list.spec.md` -> `Output`

Normative claims:

- Text output is one task per line, indented two spaces per depth level, in source order.
- Text line format includes kind, id, title, state in brackets, optional `(prior: ...)`, and optional `@<assignee>`.
- Empty text output prints `(no tasks match the given filters)` and exits 0.
- JSON output is a flat array, not nested.
- Stable JSON fields are `id`, `kind`, `title`, `state`, `assignee`, `prior`, `parent`, and `depth`.
- `assignee` and `parent` are strings or null.
- `depth` is 1-based segment count.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::list_command`
- `crates/rhei-cli/src/main.rs::title_case_kind`
- `crates/rhei-core/src/ast.rs::TaskId::depth`
- Add/verify dedicated text and JSON output tests.

## `rhei states` Claims

### MC-060: State Inspection Command

Spec sections:

- `docs/specs/rhei-states.spec.md` -> entire state-machine schema and default-state sections
- `docs/specs/rhei-usage.spec.md` -> `Command Surface`

Normative claims:

- Users and manual agents need a command to inspect allowed states, state instructions, and declared transitions.
- `rhei states --json` is a structured inspection mode referenced by `skills/rhei-plan-worker/SKILL.md`.
- State inspection should show enough data for a manual worker to know state meanings, instructions, artifacts, visits, models/targets, and legal transitions.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::Commands::States`
- `crates/rhei-cli/src/main.rs::states_command`
- `crates/rhei-cli/src/main.rs::render_state_machine_text`
- `crates/rhei-cli/src/main.rs::render_state_machine_json`
- `crates/rhei-cli/src/main.rs` unit tests `parses_states_command`, `render_state_machine_text_includes_states_and_transitions`, `render_state_machine_json_includes_state_personality`
- `skills/rhei-plan-worker/SKILL.md` -> run `rhei states` / `rhei states --state-machine <path>` / `--json`
- `skills/rhei-state-machine-writer/SKILL.md` -> `rhei states --state-machine <path>`
- `crates/rhei-validator/src/default-states.yaml`

## `rhei viz` Claims

### MC-070: Viz Usage, CLI Integration, and Non-Mutation

Spec sections:

- `docs/specs/rhei-viz.spec.md` -> `Goals`
- `docs/specs/rhei-viz.spec.md` -> `Non-Goals`
- `docs/specs/rhei-viz.spec.md` -> `Usage`
- `docs/specs/rhei-viz.spec.md` -> `Options`
- `docs/specs/rhei-viz.spec.md` -> `CLI Integration`
- `docs/specs/rhei-viz.spec.md` -> `Security Considerations`

Normative claims:

- User command is `rhei viz [PATH]`.
- PATH may be omitted, a single plan, or a directory of plans.
- Omitted PATH resolves workspace like `rhei run`; if workspace has `.rhei.md` files, all are loaded with a plan selector.
- Options include `--output <PATH>`, `--serve`, `--port <N>`, `--no-open`, `--view <NAME>`, and `--plan-state <NAME>`.
- `--output` writes HTML and exits, implying `--no-open`.
- `--serve` starts a loopback-only HTTP server.
- `rhei viz` is read-only and not a plan editor.
- CLI integration requires `Commands::Viz`, `viz_command()`, and a new `crates/rhei-viz` crate.
- The server must bind to `127.0.0.1` only.
- Plan data in script must be escaped safely.

Implementation and tests:

- Expected in-scope implementation targets:
  - `crates/rhei-cli/src/main.rs::Commands::Viz`
  - `crates/rhei-cli/src/main.rs::viz_command`
  - `crates/rhei-viz`
- Current out-of-root prototype to compare:
  - `xtask/src/viz.rs::collect_plans`
  - `xtask/src/viz.rs::render_html`
  - `xtask/src/viz.rs::escape_json_for_html_script`
  - `xtask/src/main.rs::cmd_viz_all`
  - `xtask/src/main.rs::cmd_viz_one`
  - `xtask/src/main.rs::viz_run_output`
  - `xtask/assets/viz-template.html`
- `xtask/src/viz.rs` tests `render_html_escapes_script_breakouts`, `collect_plans_merges_workspace_and_skips_task_shards_as_standalone`, `collect_plans_preserves_non_task_descendants`, `derive_plan_state_uses_machine_normalization_and_terminals`
- No `crates/rhei-viz` crate or `Commands::Viz` surface was found during scoping; compare state should confirm.

### MC-071: Viz Views, Data Shape, and Plan State Derivation

Spec sections:

- `docs/specs/rhei-viz.spec.md` -> `Views`
- `docs/specs/rhei-viz.spec.md` -> `Level-Grouped Axis Rules (Gantt)`
- `docs/specs/rhei-viz.spec.md` -> `Plan-Level State Derivation`
- `docs/specs/rhei-viz.spec.md` -> `Output Format`
- `docs/specs/rhei-viz.spec.md` -> `Data Shape`

Normative claims:

- HTML contains three tabs sharing one parsed dataset: Swimlane Gantt, Heatmap Cube, Sankey Flow.
- Gantt separates state vocabularies by level unless all levels share the same state vocabulary.
- Canonical state ordering is `draft -> pending -> in_progress -> needs-review -> review -> prove -> consolidate -> completed -> blocked/failed -> cancelled -> archived`, with unknowns sorted after alphabetically.
- Plan-level state is derived from top-level task states unless overridden by `--plan-state`.
- Derivation: all top-level `draft` -> `draft`; all successful terminal `completed` -> `completed`; all terminal with at least one non-completed -> `archived`; any active state among `in_progress`, `needs-review`, `review`, `prove`, `consolidate`, `agent-review` -> `active`; otherwise `pending`.
- Derivation uses active state machine terminal declarations.
- Static output is a single self-contained HTML file with inline CSS, JS, and JSON data, no external references.
- Data shape is `{ title, source, state, tasks[] }`, tasks have `{ id, title, state, prior, subtasks[] }`, subtasks have `{ id, title, state, prior }`.
- Parser producing this data shape should use `crates/rhei-core`.

Implementation and tests:

- Expected in-scope:
  - `crates/rhei-viz` parser/data structs
  - `crates/rhei-core` plan/workspace parsing
- Current out-of-root:
  - `xtask/src/viz.rs::Plan`
  - `xtask/src/viz.rs::Task`
  - `xtask/src/viz.rs::Subtask`
  - `xtask/src/viz.rs::plan_from_rhei`
  - `xtask/src/viz.rs::derive_plan_state`
  - `xtask/src/viz.rs::top_level_task_from_ast`
  - `xtask/src/viz.rs::collect_descendants`
  - `xtask/assets/viz-template.html`

### MC-072: Viz Serving Mode

Spec sections:

- `docs/specs/rhei-viz.spec.md` -> `Serving Modes`

Normative claims:

- Static mode writes to `$TMPDIR/rhei-viz-<hash>.html` and opens a `file://` URL; no background process remains.
- `--output <PATH>` redirects the file and suppresses browser launch.
- Live mode starts a local server at `/`, watches input `.rhei.md` files and enclosing workspace directories using `notify`, reparses on changes, pushes `PlanUpdated`, and exits on Ctrl-C or when last browser client disconnects for more than 60 seconds.
- Live mode has no remote connections and no authentication.

Implementation and tests:

- Expected in-scope:
  - `crates/rhei-viz` with optional `serve` feature
  - `notify` feature-gated watcher
  - `tiny_http` or `axum-minimal` feature-gated HTTP stack
- Current out-of-root prototype does not expose `--serve`.
- Existing dependency to note: `crates/rhei-cli/Cargo.toml` has `notify` for validate/watch and run TUI, but not necessarily `rhei viz`.

## CLI Diagnostics Claims

### MC-080: Parse, Validation, Conflict, Artifact, and JSON Diagnostics

Spec sections:

- `docs/specs/rhei-next.spec.md` -> `Missing Artifact Error`
- `docs/specs/rhei-next.spec.md` -> `No Tasks Ready`
- `docs/specs/rhei-transition-cmd.spec.md` -> `Compare-and-Swap Conflicts`
- `docs/specs/rhei-transition-cmd.spec.md` -> `Output`
- `docs/specs/rhei-complete.spec.md` -> `Output`
- `docs/specs/rhei-reset.spec.md` -> `Output`
- `docs/specs/rhei-list.spec.md` -> `Output`

Normative claims:

- Missing artifact errors identify task, state, artifact name, and path.
- CAS conflict errors identify actual state and expected `--from`.
- Invalid transitions are rejected before writes.
- No-task diagnostics distinguish complete, human-gated, and in-flight/claimed states.
- Text and JSON command modes should provide consistent machine-readable failures when JSON was requested.
- Parse and validation diagnostics should identify the file, problem, and actionable hint.

Implementation and tests:

- `crates/rhei-cli/src/main.rs::render_parse_diagnostic`
- `crates/rhei-cli/src/main.rs::render_multi_parse_diagnostic`
- `crates/rhei-cli/src/main.rs::render_validation_diagnostic`
- `crates/rhei-cli/src/main.rs::file_io_report`
- `crates/rhei-cli/src/main.rs::command_wants_json`
- `crates/rhei-cli/src/main.rs::emit_json_error`
- `crates/rhei-cli/src/main.rs::diagnose_no_claimable`
- `crates/rhei-cli/src/main.rs::ensure_state_inputs_exist`
- `crates/rhei-cli/src/main.rs::ensure_state_outputs_exist`
- `crates/rhei-cli/src/main.rs::execute_transition`
- `crates/rhei-cli/tests/integration_markdown_plans.rs` parse/validation diagnostic tests
- `crates/rhei-cli/tests/e2e/next_tests.rs::next_fails_with_explicit_error_when_current_state_input_artifact_is_missing`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_cas_rejects_wrong_from`
- `crates/rhei-cli/tests/e2e/transition_tests.rs::transition_disallowed_path_rejected`

## Cross-Spec Tensions to Preserve for Compare State

These are not findings yet; they are scoped claims whose exact wording differs across specs and must be compared carefully.

- `rhei next` says priors are satisfied when prior tasks are terminal, explicitly `completed` or `cancelled`; `rhei-usage` says all priors completed; `rhei-list` says readiness uses the same rule as `rhei next` but names terminal, non-cancelled prerequisites.
- `rhei next` says claim mode does not advance the task state, then also says non-runnable initial states are auto-advanced before printing instructions.
- `rhei transition` output examples use ASCII `->`; result file format examples use a Unicode arrow. Implementation may use either in different surfaces, so compare exact stdout and file content separately.
- `rhei-states` profile/node-policy schema says `initial` is not a per-state field when profiles are present, while the built-in default and legacy examples still use per-state `initial: true`.
- `docs/specs/rhei-viz.spec.md` specifies a `rhei viz` CLI/crate; current implementation evidence appears to be an `xtask` dogfood renderer, not an in-scope CLI command.

## Explicitly Out Of Scope

- Full `rhei run` orchestration semantics except where it shares helper functions or worker prompt instructions with manual command contracts.
- Agent spawning, program execution, MCP, skills, and timeout behavior except where state schema and template variables surface through `rhei next`, `rhei states`, or artifact enforcement.
- Template instantiation command behavior except template/example files that exercise manual-command contracts.
- Shell completions as a product area, except completion tests that are currently the only coverage for `rhei list` filter surfaces.
