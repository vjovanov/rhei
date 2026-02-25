# Scope Inventory: syntax-authoring-validation

Task partition: core plan language and validation contract.

This inventory is a boundary map, not a discrepancy finding. The comparison
state should use these claims and surfaces as the checklist for behavior.

## Specification Files And Sections

Primary normative files:

- `docs/rhei.spec.md`
  - `Plan Formats`
  - `Single-File Plan (1 Agent, or low concurrency)`
  - `Directory Workspace (Agent Teams, High Concurrency)`
  - `Directory Workspace Metadata`
  - `Grammar (EBNF)`
  - `Semantic Constraints`
  - `Semantic Constraints / 1. Dependency Integrity`
  - `Semantic Constraints / 2. State Validity`
  - `Semantic Constraints / 3. Acyclic Dependencies`
  - `Semantic Constraints / 4. Hierarchical Task Consistency`
  - `Semantic Constraints / 5. Identifier Uniqueness`
  - `Semantic Constraints / 6. Link Integrity`
  - `Semantic Constraints / 7. Node Kind Validity`
  - `Semantic Constraints / 8. Result Block Consistency`
  - `Semantic Constraints / 9. Terminal Tree Coherence`
  - `Semantic Constraints / 10. State Artifact Contracts`
  - `File Extension`
  - `CLI Command Groups`
- `docs/specs/rhei-authoring.spec.md`
  - `A Minimal Plan`
  - `Authoring Workflow`
  - `Tasks and Child Tasks`
  - `Tasks and Child Tasks / Numeric vs named tasks`
  - `Tasks and Child Tasks / Child task nodes`
  - `Tasks and Child Tasks / Depth and node kinds`
  - `Metadata`
  - `Metadata / State values with spaces`
  - `Metadata / Dependencies`
  - `Using a Custom State Machine`
  - `Common Pitfalls`
  - `Worked Examples`
  - `Advancing Task States with rhei transition`
  - `Progress format`

Related specs used only as dependencies for this partition:

- `docs/specs/rhei-states.spec.md`: state machine `name`, state definitions,
  `final`, `visits`, `profiles`, and `node_policy` are referenced by state
  validity, terminal-state, and artifact-contract claims.
- `docs/specs/rhei-transitions.spec.md`: runtime metadata ownership,
  `stateVisits`, callbacks, artifact check ordering, and SDK naming aliases are
  referenced, but transition graph semantics belong mainly to the
  `state-machines-transitions` partition.
- `docs/specs/rhei-usage.spec.md`: authoring guide links to usage patterns;
  only diagnostics/readiness examples that affect syntax and validation should
  be considered here.

## Claim Map

### 1. File Shape And Grammar

Normative claims:

- A single-file plan has exactly the Rhei H1 shape `# Rhei: <title>`, optional
  `**States:** <state-machine-name>`, optional YAML frontmatter, optional H2
  content sections, a required final `## Tasks` section, and at least one root
  task node.
- If present, `**States:**` is the first non-empty line after the H1.
- Content sections precede `## Tasks`; everything after `## Tasks` is task
  structure, and another H2 after tasks is invalid.
- Task headings are H3-H6 and match
  `<heading> <kind> <task_id>: <title>`.
- `task_id` segments are `NUMBER` or `IDENTIFIER`, with dotted child paths.
- Optional blank lines after `## Tasks` and at the start of workspace task
  files are accepted.
- YAML frontmatter is parsed as YAML and may contain `structure` and
  `metadata.tasks.*`; core task fields remain markdown.
- Ordinary task body text, code fences, and non-structural section content must
  not be mis-tokenized as task grammar.
- Rhei plan files use `.rhei.md`; workspace task files under `tasks/` are
  regular `.md` files.

Implementation surfaces:

- `crates/rhei-core/src/ast.rs`
  - `Rhei`
  - `Task`
  - `TaskId`
  - `TaskIdSegment`
  - `ContentSection`
  - `Metadata`
  - `Structure`
  - `DEFAULT_NODE_KIND`
  - `DEFAULT_MAX_LEVELS`
  - `MAX_ALLOWED_LEVELS`
- `crates/rhei-core/src/text.rs`
  - `parse_task_id`
  - `parse_task_id_segment`
- `crates/rhei-core/src/lexer.rs`
  - `Tokenizer`
  - `tokenize`
  - regexes for Rhei header, Tasks section, node headers, state/prior/assignee
    metadata, and fenced-code handling
- `crates/rhei-core/src/tokens.rs`
  - `Token::RheiHeader`
  - `Token::MetadataStates`
  - `Token::TasksSection`
  - `Token::SectionHeader`
  - `Token::NodeHeader`
  - `Token::MetadataPrior`
  - `Token::MetadataState`
  - `Token::MetadataAssignee`
  - `Token::TextContent`
- `crates/rhei-core/src/parser.rs`
  - `ParseError`
  - `parse`
  - `parse_collect`
  - `parse_frontmatter`
  - `parse_structure`
  - `NodeBuilder`
  - `finalize_builder`
  - `unwind_to_level`
  - `unescape_state`
  - `parse_workspace_index`
  - `parse_workspace_tasks`
- `crates/rhei-cli/src/main.rs`
  - `load_plan`
  - `parse_input_file`
  - `read_input_file`
  - `complete_rhei_plan_path`
  - `run_validation_once`
  - `render_command`
  - `list_command`

Tests and fixtures:

- `crates/rhei-core/tests/lexer_smoke.rs`
  - `tokenizes_basic_structure`
  - `tokenizes_assignee_after_state_and_prior`
  - `tokenizes_named_task_ids_and_prior_references`
- `crates/rhei-core/tests/lexer_edge_cases.rs`
  - `malformed_structure_near_misses_fall_back_to_text_tokens`
  - `distinguishes_valid_named_task_ids_from_invalid_boundaries`
  - `malformed_structure_inside_fenced_code_blocks_is_not_tokenized`
  - `state_metadata_backtick_escaping`
- `crates/rhei-core/tests/fixtures.rs`
  - `INVALID_FIXTURE_MALFORMED_STATE_METADATA`
  - `INVALID_FIXTURE_MALFORMED_PRIOR_METADATA`
  - `INVALID_FIXTURE_LATE_METADATA_AFTER_CONTENT`
  - `INVALID_FIXTURE_MISSING_STATE`
  - `INVALID_FIXTURE_PRIOR_BEFORE_STATE`
- `crates/rhei-core/src/parser.rs` unit tests
  - `parses_minimal_plan_with_hierarchical_tasks`
  - `parses_plan_frontmatter_metadata`
  - `parses_structure_frontmatter_with_custom_node_kinds_and_depth`
  - `errors_on_malformed_task_heading_in_tasks_section`
  - assignee ordering/duplicate tests
  - `errors_on_depth_over_structure_max_levels`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - parser/CLI diagnostics tests around malformed H1, task headings, missing
    state, malformed state/prior metadata, late metadata, and parse-vs-validation
    labeling.

User-facing commands:

- `rhei validate <plan>`
- `rhei validate --watch <plan>`
- `rhei render <plan> --format json|github|progress`
- `rhei list <plan>`
- shell completions for `.rhei.md` and workspace-directory candidates

Authoring surfaces:

- `skills/rhei-plan-writer/SKILL.md`
  - `Output Contract`
  - `Single-File Plan`
  - `Task Block`
  - `Validation Checklist`
  - `File Extension`
- `skills/rhei-plan-worker/SKILL.md`
  - `Operating Loop`
  - `Editing Discipline`

### 2. Directory Workspace Semantics

Normative claims:

- A Directory Workspace consists of `index.rhei.md`, a `tasks/` directory, and
  `.md` task files.
- `index.rhei.md` contains the Rhei title, optional `**States:**`, optional
  frontmatter, and content sections; it must not contain `## Tasks`.
- Workspace task files start directly with task definitions and do not contain
  a Rhei H1 or independent frontmatter.
- All task files are parsed and merged into one logical task graph at runtime.
- `**Prior:**` validation resolves globally across all task files.
- Task ids are globally unique across the workspace.
- Workspace metadata lives only in `index.rhei.md`.
- Markdown `**State:**`, `**Prior:**`, `**Assignee:**`, and `> **Result:**`
  remain source of truth in the task file that contains the task.
- Runtime-managed `metadata.tasks.<id>.*`, including `stateVisits`, is stored
  in `index.rhei.md`.
- Workspace artifact and relative-link roots are the directory containing
  `index.rhei.md`.

Implementation surfaces:

- `crates/rhei-core/src/workspace.rs`
  - `Workspace`
  - `is_workspace`
  - `load_workspace`
  - `task_sources`
- `crates/rhei-core/src/parser.rs`
  - `WorkspaceIndex`
  - `parse_workspace_index`
  - `parse_workspace_tasks`
- `crates/rhei-cli/src/main.rs`
  - `LoadedPlan`
  - `LoadedPlan::task_file`
  - `load_plan`
  - `auto_state_machine_path`
  - `execution_workspace_root`
  - `transition_command`
  - `execute_transition`
  - `complete_command`
  - `reset_command`
  - `result_workspace_root`
  - `clear_runtime_metadata_in_file`

Tests and fixtures:

- `crates/rhei-core/src/parser.rs`
  - `parses_workspace_index_frontmatter_metadata`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - `workspace_loads_and_validates_correctly`
  - `validate_auto_discovers_workspace_root_state_machine_from_states_declaration`
  - `workspace_render_json_includes_all_tasks`
  - `workspace_duplicate_task_id_across_files_is_reported`
  - `workspace_missing_index_is_not_detected_as_workspace`
  - `workspace_empty_tasks_directory_is_reported`
  - `workspace_transition_updates_correct_task_file`
  - `workspace_transition_updates_index_metadata_for_counted_loops`
  - `workspace_run_advances_tasks_to_completion`
  - `workspace_reset_restores_initial_states_and_removes_runtime`
  - `workspace_index_with_tasks_section_is_rejected`

Templates and examples:

- `.agents/rhei/templates/spec-implementation-discrepancy-audit/index.rhei.md`
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/tasks/*.md`
- `.agents/rhei/templates/changeset-review/index.rhei.md`
- `.agents/rhei/templates/changeset-review/tasks/01-coordinate.md`
- `.agents/rhei/templates/hourly-human-intervention/index.rhei.md`
- `.agents/rhei/templates/hourly-human-intervention/tasks/*.md`
- `examples/spec-implementation-discrepancy-audit-example/index.rhei.md`
- `examples/spec-implementation-discrepancy-audit-example/tasks/*.md`
- `examples/changeset-review-example/index.rhei.md`
- `examples/changeset-review-example/tasks/01-coordinate.md`
- `examples/review-fix-visits/index.rhei.md`
- `examples/review-fix-visits/tasks/01-review-loop.md`

User-facing commands:

- `rhei validate <workspace-dir>`
- `rhei render <workspace-dir> --format json|github|progress`
- `rhei list <workspace-dir>`
- `rhei transition <workspace-dir> --task <id> --from <state> --to <state>`
- `rhei next <workspace-dir>`
- `rhei complete <workspace-dir> --task <id> --result <text>`
- `rhei reset <workspace-dir>`
- `rhei run <workspace-dir>`
- `rhei instantiate <template> --output <workspace-dir>`

Authoring surfaces:

- `skills/rhei-plan-writer/SKILL.md`
  - `Directory Workspace`
  - `Frontmatter`
  - `Validation Checklist`
- `skills/rhei-template-writer/SKILL.md`
  - template layout and workspace skeleton guidance
- `skills/rhei-plan-worker/SKILL.md`
  - `Unorchestrated Mode Notes`

### 3. Structure, Node Kinds, Task Hierarchy

Normative claims:

- When `structure` is omitted, defaults are `maxLevels: 2` and
  `nodeKinds: [task]`.
- `structure.maxLevels` is valid only from 1 through 4.
- `structure.nodeKinds` values are unique `IDENTIFIER`s, normalized
  case-insensitively, and default to `[task]`.
- `rhei` is reserved: it must not appear in `structure.nodeKinds`, under
  `node_policy.by_type`, or as a non-root node kind.
- The heading keyword is the node kind; matching is case-insensitive.
- Node depth and id path depth must match: H3 one segment, H4 two, H5 three,
  H6 four.
- Child ids extend the parent id by exactly one segment.
- Sibling ids under the same parent are unique.
- Task depth must not exceed `structure.maxLevels`.
- All task ids are unique across the whole logical plan.
- The authoring guide additionally says named task ids are conceptual anchors
  and "must not declare child tasks"; this should be compared against the
  formal spec's mixed numeric/named-path allowance.

Implementation surfaces:

- `crates/rhei-core/src/ast.rs`
  - `Structure::default`
  - `Structure::accepts_kind`
  - `TaskId::depth`
  - `TaskId::extends`
  - `TaskId::parent`
  - `TaskId::as_number`
  - `TaskId::as_named`
- `crates/rhei-core/src/parser.rs`
  - `parse_structure`
  - `parse`
  - heading depth checks
  - parent-extension checks
  - title-empty and malformed-heading errors
- `crates/rhei-validator/src/lib.rs`
  - `validate_task_id_uniqueness`
  - `validate_sibling_uniqueness`
  - `StateMachine::validate_profiles_and_node_policy`
  - `StateMachine::profile_for`
  - `StateMachine::root_profile`

Tests and fixtures:

- `crates/rhei-core/src/parser.rs`
  - `parses_structure_frontmatter_with_custom_node_kinds_and_depth`
  - `errors_on_depth_over_structure_max_levels`
  - malformed heading tests
- `crates/rhei-validator/src/lib.rs`
  - `valid_subtask_numbering_ok`
  - `duplicate_sibling_child_id_is_rejected`
  - profile/node-policy reserved-kind tests:
    `rejects_node_policy_by_type_with_reserved_kind`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - `malformed_task_heading_reports_parse_error_instead_of_child_id_validation_error`
  - malformed task heading and unknown node kind CLI tests

Authoring surfaces:

- `skills/rhei-plan-writer/SKILL.md`
  - `ID Policy`
  - `Child Task Format`
  - `Frontmatter`
  - `Validation Checklist`

### 4. Task Metadata Fields

Normative claims:

- `**State:**` is mandatory and must be the first line after the task header.
- `**Prior:**` is optional and, when present, immediately follows
  `**State:**`.
- `**Assignee:**` is optional after `**Prior:**` and is runtime-owned.
- Metadata fields appearing after body content are invalid.
- Malformed `**State:**`, `**Prior:**`, or `**Assignee:**` near-miss lines
  should produce parse diagnostics rather than being silently accepted.
- `rhei next` writes `**Assignee:**`; `rhei complete` removes it.
- Plan writers should not author `**Assignee:**` or result blocks.
- No metadata fields beyond `**State:**`, `**Prior:**`, and runtime
  `**Assignee:**` are part of the task metadata grammar.

Implementation surfaces:

- `crates/rhei-core/src/parser.rs`
  - `NodeBuilder.state`
  - `NodeBuilder.prior`
  - `NodeBuilder.assignee`
  - `NodeBuilder.metadata_closed`
  - `finalize_builder`
  - state/prior/assignee parsing branches
  - `is_recoverable_error`
  - `strip_for_recovery`
- `crates/rhei-core/src/lexer.rs`
  - state/prior/assignee tokenization
- `crates/rhei-validator/src/lib.rs`
  - `validate_assignee_nonempty`
- `crates/rhei-cli/src/main.rs`
  - `insert_task_assignee`
  - `write_task_assignee`
  - `rewrite_task_completion`
  - `complete_command`
  - `next_command`

Tests and fixtures:

- `crates/rhei-core/tests/lexer_smoke.rs`
  - assignee and prior tokenization
- `crates/rhei-core/tests/lexer_edge_cases.rs`
  - `assignee_metadata_with_various_values`
  - `assignee_without_colon_is_plain_text`
- `crates/rhei-core/tests/fixtures.rs`
  - malformed metadata and ordering fixtures
- `crates/rhei-core/src/parser.rs`
  - missing state, prior-before-state, assignee-before-state, duplicate
    assignee, late metadata tests
- `crates/rhei-validator/src/lib.rs`
  - `prior_without_state_is_parse_error`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - CLI parse diagnostics for malformed state/prior metadata and missing state
  - `workspace_reset_restores_initial_states_and_removes_runtime` checks
    assignee/result cleanup in workspace flow
- `crates/rhei-cli/src/main.rs` tests
  - `rewrite_task_completion_removes_assignee_and_appends_result_link`
  - `rewrite_task_completion_without_assignee_still_appends_result_link`
  - `rewrite_task_completion_inserts_result_link_before_child`

User-facing commands:

- `rhei next`
- `rhei complete`
- `rhei reset`
- `rhei render --format github|json|progress`
- `rhei list --assignee|--no-assignee`

Authoring surfaces:

- `skills/rhei-plan-writer/SKILL.md`
  - `Task Block`
  - `Metadata`
  - `Editing Existing Rhei Plans`
- `skills/rhei-plan-worker/SKILL.md`
  - `Assignee Discipline`
  - `Editing Discipline`

### 5. `**States:**` Lookup And State Validity

Normative claims:

- A plan without `**States:**` uses the built-in `rhei` state machine.
- When `**States:**` is declared, single-file plans auto-resolve a sibling
  `states.yaml`; workspaces auto-resolve `<workspace>/states.yaml`.
- The resolved YAML file's `name` must match the `**States:**` value.
- `--state-machine <path>` overrides automatic lookup.
- All authored `**State:**` values must be defined in the active state
  machine.
- A state defined globally but excluded from the node's resolved profile
  `allowed` set is invalid for that node.
- State-name rendering is normative: bare form only for canonical names that
  match `IDENTIFIER`; names with spaces or punctuation use backticks.
- Backticked state values support escaped backslash and escaped backtick.
- Counted visit suffixes are allowed only for states declaring `visits`; suffix
  values must be greater than 1 and within budget.
- Exact state-name lookup wins before interpreting a trailing `-<digits>` as a
  counted visit suffix.

Implementation surfaces:

- `crates/rhei-core/src/parser.rs`
  - `re_states_decl`
  - `rhei_states_checked`
  - `unescape_state`
- `crates/rhei-core/src/lexer.rs`
  - `Tokenizer::unescape_state`
- `crates/rhei-validator/src/lib.rs`
  - `StateMachine`
  - `StateDef`
  - `Profile`
  - `NodePolicy`
  - `StateMachine::builtin_default`
  - `StateMachine::from_yaml_str`
  - `StateMachine::from_yaml_file`
  - `StateMachine::is_valid_state`
  - `StateMachine::allowed_states`
  - `StateMachine::profile_for`
  - `StateMachine::root_profile`
  - `parse_task_state`
  - `ParsedTaskState`
  - `validate_state_consistency`
  - `validate_task_state_instance`
  - `validate_task_state_against_profile`
  - `validate_profiles_and_node_policy`
- `crates/rhei-cli/src/main.rs`
  - `load_state_machine`
  - `auto_state_machine_path`
  - `resolve_state_machine_for_loaded_plan`
  - `state_machine_label`
  - `states_command`
  - `normalized_state_name`
  - `format_task_state_value`
  - `format_state_metadata_value`
  - `state_visit_limit`
  - `current_state_visit_count`
  - `render_visit_count`

Tests and fixtures:

- `crates/rhei-validator/src/lib.rs`
  - `reports_invalid_state_with_allowed_list`
  - `accepts_valid_states_and_escaped_spaces`
  - `accepts_counted_state_suffix_within_budget`
  - `rejects_counted_state_suffix_of_one`
  - `rejects_counted_state_suffix_when_state_has_no_visits`
  - profile/node-policy tests:
    `loads_profiles_and_node_policy`,
    `rejects_profiles_without_node_policy`,
    `rejects_node_policy_without_profiles`,
    `rejects_profile_with_initial_not_in_allowed`,
    `rejects_profile_with_unknown_state_in_allowed`,
    `rejects_node_policy_default_with_undefined_profile`,
    `enforces_profile_allowed_on_task_state`,
    `rejects_task_state_outside_profile_allowed`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - state-machine auto-discovery/name mismatch tests
  - `validate_accepts_counted_state_suffix_within_budget`
  - `transition_counted_loop_updates_metadata_and_blocks_exhausted_reentry`
  - `transition_from_authored_counted_state_treats_start_as_first_visit`
  - workspace counted-loop metadata test
- `examples/escaped-state-values.rhei.md`
- `examples/states-with-spaces.yaml`
- `examples/review-fix-visits/index.rhei.md`
- `examples/review-fix-visits/states.yaml`
- `skills/rhei-plan-writer/references/default-states.md`
- `crates/rhei-validator/src/default-states.yaml`

User-facing commands:

- `rhei states`
- `rhei states --json`
- `rhei validate [--state-machine <path>] <plan>`
- `rhei transition [--state-machine <path>] ...`
- `rhei next [--state-machine <path>] ...`
- `rhei complete [--state-machine <path>] ...`
- completions:
  `complete_yaml_path`, `complete_state_name`, `complete_transition_from_state`,
  `complete_transition_to_state`

Authoring surfaces:

- `docs/specs/rhei-authoring.spec.md / Using a Custom State Machine`
- `skills/rhei-plan-writer/SKILL.md / Allowed States`
- `skills/rhei-state-machine-writer/SKILL.md / Validation Checklist`

### 6. Prior Dependency Semantics

Normative claims:

- A `**Prior:**` item is a kind-qualified task reference.
- Each prior reference must resolve in the same logical plan:
  same document for single-file, merged graph for workspace.
- A prior list must not contain duplicate references.
- A task must not reference itself.
- A task must not reference any ancestor; child tasks cannot list their parent
  as `**Prior:**`.
- If a follow-up must wait for a completed parent task, author it as a top-level
  sibling with `**Prior:**` pointing at the parent.
- Dependency graph must be a DAG.
- Dependency readiness is defined only by terminal-state semantics:
  all referenced tasks must be in states marked `final: true`; state
  `instructions` must not change readiness.
- Authoring guide exposes dependencies as SDK aliases
  `task.metadata.dependsOn` / `task.metadata.depends_on` / CLI JSON naming.

Implementation surfaces:

- `crates/rhei-core/src/parser.rs`
  - prior parsing branch
  - `re_prior_ref`
- `crates/rhei-core/src/lexer.rs`
  - `Token::MetadataPrior`
- `crates/rhei-validator/src/lib.rs`
  - `build_task_index`
  - `validate_dependency_integrity`
  - `validate_circular_dependencies`
  - `validate_task_id_uniqueness`
  - `validate_sibling_uniqueness`
- `crates/rhei-cli/src/main.rs`
  - `dependency_is_satisfied`
  - `find_ready_tasks`
  - `find_claimable_tasks`
  - `diagnose_no_claimable`
  - `list_command` ready/blocked filters
  - `next_command`
  - `run_command`
  - `find_next_transition`
- `crates/rhei-output/src/lib.rs`
  - JSON/GitHub/progress renderers for prior fields and dependency display
- `crates/rhei-napi/src/lib.rs`
  - public API shape for plan/task JSON, if exposed

Tests and fixtures:

- `crates/rhei-validator/src/lib.rs`
  - `reports_missing_numeric_dependency`
  - `reports_missing_named_dependency`
  - `rejects_child_prior_to_parent`
  - `rejects_descendant_prior_to_ancestor`
  - `ok_when_all_dependencies_exist_named_and_numeric`
  - `detects_two_node_cycle`
  - `detects_three_node_cycle`
  - `detects_self_cycle`
  - `passes_on_dag`
  - `no_false_cycle_with_missing_dependency`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - semantic validation failures for missing prior, cycle, parent-as-prior
  - run/next/list tests involving dependency readiness and blocking
  - workspace validation tests with cross-file prior references
- `crates/rhei-core/tests/fixtures.rs`
  - `INVALID_FIXTURE_MISSING_PRIOR`
  - `INVALID_FIXTURE_PARENT_AS_PRIOR`
  - cycle fixtures

User-facing commands:

- `rhei validate`
- `rhei next`
- `rhei run`
- `rhei list --ready`
- `rhei list --blocked`
- `rhei list --has-prior <task-id>`
- `rhei render --format json|github|progress`

Authoring surfaces:

- `skills/rhei-plan-writer/SKILL.md`
  - `Planning Workflow`
  - `Validation Checklist`
- `skills/rhei-plan-worker/SKILL.md`
  - `Task Selection`

### 7. Link Integrity

Normative claims:

- All relative markdown links in content sections and task content must resolve
  to existing files.
- Single-file links resolve relative to the plan file's directory.
- Workspace links resolve relative to the workspace root, not relative to the
  physical task file.
- External URLs and fragment-only anchors are not checked.
- Links with fragments check only the file portion.
- Workspace link resolution normalizes `.` and `..`, and a normalized path
  escaping the workspace root is invalid.
- `> **Result:**` links are validated by result-block consistency, not by
  general link integrity; result files may be created later.

Implementation surfaces:

- `crates/rhei-validator/src/lib.rs`
  - `extract_markdown_links`
  - `collect_all_links`
  - `is_non_file_link`
  - `validate_markdown_links`
  - `Validator::validate_with_base`
  - `validate_with_machine_and_base`
- `crates/rhei-cli/src/main.rs`
  - `run_validation_once`
  - `execution_workspace_root`
  - `load_plan`

Tests and fixtures:

- `crates/rhei-validator/src/lib.rs`
  - `extract_markdown_links_finds_all_links`
  - `extract_markdown_links_handles_no_links`
  - `is_non_file_link_classifies_correctly`
  - `link_validation_reports_missing_file`
  - `link_validation_passes_when_file_exists`
  - `link_validation_ignores_external_urls`
  - `link_validation_strips_fragment_from_file_link`
  - `link_validation_checks_task_and_subtask_content`
  - `link_validation_skipped_without_base_path`

User-facing commands:

- `rhei validate <plan-or-workspace>`

### 8. Result Block Consistency And Completion Output

Normative claims:

- A result block has exact shape
  `> **Result:** [<task-id>](runtime/results/<task-id>.md)`.
- The link text must equal the enclosing task id.
- The target must be exactly `runtime/results/<task-id>.md`.
- Result blocks are inserted by `rhei complete` after task content and before
  child tasks.
- Result blocks are task-local metadata and not general prose.
- Result files may be created later by runtime commands, so result links are
  not general link-integrity failures.
- `rhei complete` writes/appends `runtime/results/<task-id>.md`, inserts the
  result link, and removes the assignee.
- `rhei reset` removes result links and runtime output for workspace reset.

Implementation surfaces:

- `crates/rhei-cli/src/main.rs`
  - `complete_command`
  - `append_result_entry`
  - `rewrite_task_completion`
  - `strip_result_links`
  - `reset_command`
  - `result_workspace_root`
- Candidate parser/validator surfaces to inspect for dedicated result-block
  validation:
  - `crates/rhei-core/src/parser.rs`
  - `crates/rhei-validator/src/lib.rs`
  - `crates/rhei-output/src/lib.rs`

Tests and fixtures:

- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - `complete_succeeds_when_all_subtasks_are_terminal`
  - `complete_rejects_parent_with_non_terminal_subtasks`
  - workspace reset result cleanup tests
- `crates/rhei-cli/src/main.rs` tests
  - `rewrite_task_completion_removes_assignee_and_appends_result_link`
  - `rewrite_task_completion_without_assignee_still_appends_result_link`
  - `rewrite_task_completion_inserts_result_link_before_child`

User-facing commands:

- `rhei complete <plan> --task <id> --result <message>`
- `rhei reset <plan-or-workspace>`
- `rhei render --format github|progress|json`

Authoring surfaces:

- `skills/rhei-plan-writer/SKILL.md`
  - `Task Block`
  - `Editing Existing Rhei Plans`
- `skills/rhei-plan-worker/SKILL.md`
  - `Assignee Discipline`
  - `Editing Discipline`

### 9. Terminal Tree Coherence

Normative claims:

- A terminal state means any state whose active state machine definition has
  `final: true`.
- In the built-in machine, terminal states are `completed` and `cancelled`.
- A task node in a terminal state must not contain any non-terminal
  descendants.
- `rhei complete` must not complete a parent while any child task is
  non-terminal.

Implementation surfaces:

- `crates/rhei-validator/src/lib.rs`
  - `validate_terminal_tree_coherence`
  - local `is_terminal` inside that validator
  - `parse_task_state`
- `crates/rhei-cli/src/main.rs`
  - `is_terminal_state`
  - `non_terminal_descendants`
  - `complete_command`
  - `diagnose_no_claimable`
  - `list_command --terminal|--non-terminal`

Tests and fixtures:

- `crates/rhei-validator/src/lib.rs`
  - `terminal_parent_with_non_terminal_subtask_errors`
  - `terminal_parent_with_terminal_subtasks_is_valid`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - `complete_rejects_parent_with_non_terminal_subtasks`
  - `complete_succeeds_when_all_subtasks_are_terminal`

User-facing commands:

- `rhei validate`
- `rhei complete`
- `rhei list --terminal`
- `rhei list --non-terminal`

### 10. State Artifact Contracts

Normative claims:

- State machine states may declare file `inputs` and `outputs`.
- Entering a state may require input files to exist.
- Leaving a state may require output files to exist.
- Artifact paths resolve relative to the execution root:
  single-file plan directory or workspace root.
- Artifact path values described elsewhere as workspace-relative mean relative
  to this execution root.
- Artifact existence is enforced by runtime commands (`rhei transition`,
  `rhei complete`, `rhei run`, and `rhei next`), not pure markdown validation.
- Optional callbacks are implicit success when omitted.
- Artifact definition validation includes non-empty names/paths, duplicate
  names, relative paths, no workspace-root escape, and `optional: true` only
  on inputs.
- Runtime template variables such as `{task_id}`, `{state}`, `{visit_count}`,
  `{model}`, `{target.slug}`, `{agent}`, and `{input.<name>.path}` are part of
  the execution contract where state instructions reference artifacts.

Implementation surfaces:

- `crates/rhei-validator/src/lib.rs`
  - `StateArtifactDef`
  - `StateDef.inputs`
  - `StateDef.outputs`
  - `validate_artifact_definitions`
  - `path_escapes_workspace_root`
  - `validate_template_conditions`
  - `parse_duration_secs` only if timeout diagnostics intersect command flow
- `crates/rhei-cli/src/main.rs`
  - `execution_workspace_root`
  - `RuntimeTemplateContext`
  - `artifact_relative_path`
  - `resolve_artifact_path`
  - `resolve_runtime_template_variable`
  - `ensure_state_inputs_exist`
  - `ensure_state_outputs_exist`
  - `ensure_state_inputs_exist_for_transition`
  - `ensure_state_outputs_exist_for_transition`
  - `state_outputs_exist_for_resolved_invocation`
  - `task_has_pending_agent_invocations`
  - `execute_transition`
  - `next_command`
  - `complete_command`
  - `run_command`
  - `compose_agent_prompt`

Tests and fixtures:

- `crates/rhei-validator/src/lib.rs`
  - `loads_state_machine_with_artifact_contracts`
  - `rejects_duplicate_artifact_names_in_same_state_field`
  - `rejects_absolute_artifact_paths`
  - `rejects_artifact_paths_that_escape_workspace_root`
  - template-condition tests for declared input references
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - callback/run tests with outputs
  - all-model callback artifact tests
  - run/transition/complete tests involving callback-created output artifacts
- `examples/review-fix-visits/states.yaml`
- `examples/review-fix-visits/README.md`
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml`
- `.agents/rhei/templates/changeset-review/states.yaml`
- `.agents/rhei/templates/hourly-human-intervention/states.yaml`

User-facing commands:

- `rhei next`
- `rhei transition`
- `rhei complete`
- `rhei run`
- `rhei states`

Authoring surfaces:

- `skills/rhei-state-machine-writer/SKILL.md`
  - `Artifact Contracts`
  - `Validation Checklist`
- `skills/rhei-plan-worker/SKILL.md`
  - `Task Selection`
  - `State Transitions`

### 11. Diagnostics And Command Contracts

Normative claims:

- `rhei validate` checks syntax, state validity, dependency integrity, DAG
  shape, link integrity, and terminal tree coherence.
- Parse failures should be surfaced as parse diagnostics, not mislabeled as
  semantic validation failures.
- Semantic validation failures should aggregate actionable messages.
- `rhei states` exposes allowed states, instructions, and transitions for
  authoring.
- `rhei render --format progress` shows title, content sections, task states,
  prior dependencies, and children without truncating content.
- `rhei transition` gives an atomic compare-and-swap path for manual state
  changes and reports conflicts with the actual current state.
- `rhei next` prints task details, resolved instructions, and, in JSON mode,
  machine-readable task details.
- `rhei list` exposes ready/blocked, state, assignee, kind, prior, parent/root,
  terminal/non-terminal, and search filters.
- `rhei instantiate` validates generated plans/workspaces after materializing
  templates.

Implementation surfaces:

- `crates/rhei-cli/src/main.rs`
  - `Commands`
  - `dispatch`
  - `run_validation_once`
  - `parse_report`
  - `parse_errors_report`
  - `render_parse_diagnostic`
  - `render_multi_parse_diagnostic`
  - `validation_report`
  - `render_validation_diagnostic`
  - `format_validation_errors`
  - `states_command`
  - `render_state_machine_text`
  - `render_state_machine_json`
  - `render_command`
  - `list_command`
  - `transition_command`
  - `next_command`
  - `print_next_output`
  - `complete_command`
  - `templates::instantiate_command`
- `crates/rhei-output/src/lib.rs`
  - `to_json_value`
  - `to_github_markdown`
  - `ProgressReportOutput`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - CLI help output tests
  - parse/validation diagnostic tests
  - `valid_plan_parses_validates_and_renders_across_crates`
  - `cli_validate_and_render_use_real_fixture_files`
  - render progress/GitHub/JSON tests
  - transition CAS/conflict tests
  - next/list/complete/run tests
- `crates/rhei-cli/src/main.rs` unit tests
  - `parse_diagnostic_includes_line_info_when_available`
  - `validation_failure_formatting_aggregates_multiple_errors`

User-facing commands:

- `rhei validate`
- `rhei render`
- `rhei states`
- `rhei list`
- `rhei transition`
- `rhei next`
- `rhei complete`
- `rhei run`
- `rhei reset`
- `rhei templates`
- `rhei instantiate`
- `rhei completions`

## Implementation Roots To Inspect

Required by task:

- `crates`
- `skills`
- `.agents/rhei/templates`
- `examples`

Primary code files:

- `crates/rhei-core/src/ast.rs`
- `crates/rhei-core/src/text.rs`
- `crates/rhei-core/src/tokens.rs`
- `crates/rhei-core/src/lexer.rs`
- `crates/rhei-core/src/parser.rs`
- `crates/rhei-core/src/workspace.rs`
- `crates/rhei-core/src/lib.rs`
- `crates/rhei-validator/src/lib.rs`
- `crates/rhei-validator/src/default-states.yaml`
- `crates/rhei-cli/src/main.rs`
- `crates/rhei-output/src/lib.rs`
- `crates/rhei-napi/src/lib.rs`

Primary tests:

- `crates/rhei-core/tests/fixtures.rs`
- `crates/rhei-core/tests/lexer_smoke.rs`
- `crates/rhei-core/tests/lexer_edge_cases.rs`
- `crates/rhei-cli/tests/integration.rs`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
- unit tests embedded in:
  - `crates/rhei-core/src/parser.rs`
  - `crates/rhei-validator/src/lib.rs`
  - `crates/rhei-cli/src/main.rs`

Primary skills:

- `skills/rhei-plan-writer/SKILL.md`
- `skills/rhei-plan-writer/references/default-states.md`
- `skills/rhei-plan-worker/SKILL.md`
- `skills/rhei-state-machine-writer/SKILL.md`
- `skills/rhei-template-writer/SKILL.md`

Primary template directories:

- `.agents/rhei/templates/spec-implementation-discrepancy-audit/`
- `.agents/rhei/templates/spec-review/`
- `.agents/rhei/templates/changeset-review/`
- `.agents/rhei/templates/hourly-human-intervention/`
- `.agents/rhei/templates/multi-model-analysis/`

Primary examples:

- `examples/escaped-state-values.rhei.md`
- `examples/states-with-spaces.yaml`
- `examples/release-automation.rhei.md`
- `examples/human-review-loop.rhei.md`
- `examples/pm-onboarding-experiment.rhei.md`
- `examples/spec-implementation-discrepancy-audit-example/`
- `examples/changeset-review-example/`
- `examples/review-fix-visits/`
- `examples/living-review-loop/`

## Out Of Scope For This Partition

- Full state-machine transition graph schema, callback semantics, condition
  language, agent execution model, polling, model fanout, MCP/skill registry
  behavior, and TUI monitoring details except where they directly affect
  state-name validity, terminal-state interpretation, artifact enforcement, or
  diagnostics listed above.
- Template manifest input schema beyond its role in producing syntactically
  valid plans/workspaces and invoking `rhei validate` after instantiation.
- External SDK runtime behavior beyond the authoring guide's naming aliases
  for dependency metadata.
