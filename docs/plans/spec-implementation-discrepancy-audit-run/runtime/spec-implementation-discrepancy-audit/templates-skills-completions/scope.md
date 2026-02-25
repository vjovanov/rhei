# Scope Inventory: templates-skills-completions

Task partition: generated and assistant-facing workflow surfaces.

This inventory is a boundary map, not a discrepancy finding. The comparison
state should use these claims and surfaces as the checklist for behavior.

## Specification Files And Sections

Primary normative files:

- `docs/specs/rhei-templates.spec.md`
  - `Concepts`
  - `Template Discovery`
  - `Directory Layout`
  - `Manifest Schema (template.yaml)`
  - `Manifest Schema (template.yaml) / Validation Rules`
  - `Template-Shipped Settings`
  - `Instantiation Template Syntax`
  - `Instantiation Template Syntax / Resolution Rules`
  - `Instantiation Template Syntax / Escaping`
  - `CLI Commands / rhei instantiate`
  - `CLI Commands / rhei instantiate / Input UX`
  - `CLI Commands / rhei instantiate / Behavior`
  - `CLI Commands / rhei instantiate / Instantiation Summary Output`
  - `CLI Commands / rhei instantiate / Exit Codes`
  - `CLI Commands / rhei templates`
  - `Example`
  - `Interaction with Existing Features`
  - `Grammar Extension`
  - `Manifest Fields`
  - `File Extension`
- `docs/specs/rhei-install-skills.spec.md`
  - `Usage`
  - `Options`
  - `Agent Targets`
  - `Agent Targets / Claude Code`
  - `Agent Targets / Cursor`
  - `Agent Targets / Windsurf`
  - `Agent Targets / GitHub Copilot`
  - `Agent Targets / Kilocode`
  - `Agent Targets / Pi`
  - `Agent Targets / OpenAI Codex`
  - `Agent Targets / Google Antigravity`
  - `Behavior`
  - `Behavior / Local installation`
  - `Behavior / Detect installed skills`
  - `Behavior / Resolve skill source`
  - `Behavior / Symlink vs copy`
  - `Behavior / Registration`
  - `Behavior / Dry run`
  - `Behavior / Uninstall`
  - `Example Output`
  - `Implementation Notes`
- `docs/specs/rhei-completions.spec.md`
  - `UX Goal`
  - `Usage`
  - `Arguments`
  - `Options`
  - `Supported Shells`
  - `Output Contract`
  - `Completion Contract`
  - `Completion Contract / Global Rules`
  - `Completion Contract / Candidate Display`
  - `Completion Contract / Filesystem Values`
  - `Completion Contract / Dynamic Context`
  - `Command Coverage`
  - `Template Input Completion`
  - `Template Input Completion / Template Name Slot`
  - `Template Input Completion / Positional Slots`
  - `Template Input Completion / Assignment Keys`
  - `Template Input Completion / Assignment Values`
  - `Template Input Completion / Values Files`
  - `Template Input Completion / List Inputs Mode`
  - `Template Input Completion / Unknown Or Invalid Templates`
  - `Task And State Completion`
  - `Agent And Skill Completion`
  - `Performance And Reliability`
  - `Acceptance Tests`
  - `System Installation`
  - `Examples`
  - `Non-Goals`
- `docs/specs/rhei-state-machine-writer.spec.md`
  - `Purpose`
  - `Inputs`
  - `Output`
  - `Output / Output Structure`
  - `Design Rules`
  - `Design Rules / State Design`
  - `Design Rules / Transition Design`
  - `Design Rules / Profile and Node Policy Design`
  - `Design Rules / Instructions Design`
  - `Workflow`
  - `Examples`
  - `Relationship to Other Roles`
  - `When to Use a Custom State Machine`
  - `File Placement`
- `docs/specs/rhei-usage.spec.md`
  - `Roles / Plan Writer`
  - `Roles / Plan Worker`
  - `Roles / Reviewer`
  - `Roles / Human Operator`
  - `Coordination Through the State Machine`
  - `State Flow (Default Machine)`
  - `Command Surface`
  - `Usage Patterns / Pattern 0: Zero-Config Agent Execution`
  - `Usage Patterns / Pattern 1: Single Agent, Start to Finish`
  - `Usage Patterns / Pattern 2: Writer and Worker as Separate Sessions`
  - `Usage Patterns / Pattern 3: Parallel Workers on Independent Branches`
  - `Usage Patterns / Pattern 3b: Highly Distributed Swarms (Directory Workspaces)`
  - `Usage Patterns / Pattern 4: Human-in-the-Loop Checkpoints`
  - `Usage Patterns / Pattern 8: Living Workspace Expansion`
  - `Usage Patterns / Pattern 9: Program States for Deterministic Steps`
  - `Usage Patterns / Pattern 10: CI/CD Pipeline as a Plan`
  - `The Plan as Shared Memory`

Related specs used only as dependencies for this partition:

- `docs/rhei.spec.md`: output plan grammar, directory workspace grammar,
  task metadata order, `**Prior:**`, `**States:**`, result ownership, and file
  extension rules are consumed by templates and skills, but the core grammar
  partition owns parser discrepancies.
- `docs/specs/rhei-authoring.spec.md`: plan-writer expectations are reflected
  in the assistant-facing skills and template skeleton rules.
- `docs/specs/rhei-states.spec.md`: state fields, artifacts, variables,
  `profiles`, `node_policy`, `gating`, `final`, and default machine lookup are
  required by state-machine-writer and template validation claims.
- `docs/specs/rhei-transitions.spec.md`: explicit transitions, callbacks,
  condition expressions, `exit_code`, counted visits, and transition graph
  semantics are referenced by generated workflows.
- `docs/specs/rhei-agents.spec.md`: settings merge, MCP/skills registries,
  completion authority, agent/mode/model resolution, and timeout requirements
  are referenced by templates, completions, and state-machine-writer guidance.
- `docs/specs/rhei-programs.spec.md`: program-state shape is referenced by
  state-machine-writer and template authoring guidance.

## Claim Map

### 1. Template Discovery, Listing, Layout, And Manifest Validation

Normative claims:

- A template is a directory with required `template.yaml`, optional
  `states.yaml`, exactly one plan entry point (`plan.rhei.md` or
  `index.rhei.md` + `tasks/`), optional `settings.json`, and additional files.
- Template names are directory names and identifiers for CLI commands.
- Named template discovery is first-match-wins: project
  `<project>/.agents/rhei/templates/<name>/`, then user
  `~/.agents/rhei/templates/<name>/`.
- `rhei templates` lists discovered templates, supports `--json` and
  `--source <project|user|all>`, prints name/version/description/source/path
  and required input count, and skips invalid/unreadable template directories
  with a warning.
- `rhei instantiate <name>` fails if the matched template is invalid; direct
  path references bypass named discovery and fail if unreadable or invalid.
- `template.yaml` schema fields are `name`, `version`, `description`, and
  optional `inputs`; key order is not normative.
- Manifest `name` must match the enclosing directory; `description` must be
  non-empty; input names are unique identifiers; `version` is a displayed YAML
  scalar with no semantic-version validation.
- Input types are `string`, `number`, `boolean`, `path`, `array`, `object`,
  defaulting to `string`.
- `required: true` is mutually exclusive with `default`; a `default` makes the
  input optional; optional no-default inputs resolve to type-shaped empties.
- `positional` is a positive integer; positional indexes are unique and
  contiguous from `1`.
- `type: array` requires `items`; `type: object` may declare `properties`,
  whose members are required unless `required: false` or `default` is present.
- `validate` is only valid on scalar input types and is a Rust `regex` pattern
  anchored to the whole rendered scalar value.

Implementation surfaces:

- `crates/rhei-cli/src/main.rs`
  - `Commands::Templates`
  - `Commands::Instantiate`
  - `templates::TemplateSource`
  - `templates::TemplateSourceFilter`
  - `templates::TemplateLayout`
  - `templates::TemplateManifest`
  - `templates::TemplateInputDef`
  - `templates::TemplateValueSchema`
  - `templates::TemplateInputType`
  - `templates::DiscoveredTemplate`
  - `templates::templates_command`
  - `templates::parse_template_source_filter`
  - `templates::discover_templates`
  - `templates::template_search_roots`
  - `templates::resolve_template_reference`
  - `templates::template_reference_is_path`
  - `templates::load_template_manifest`
  - `templates::validate_template_manifest`
  - `templates::validate_template_value_schema`
  - `templates::detect_template_layout`
  - `find_project_root`
  - `home_dir`

Tests and fixtures:

- `crates/rhei-cli/tests/e2e/templates_tests.rs`
  - `templates_lists_project_local_templates`
  - `instantiate_accepts_manifest_declared_positional_input`
  - `instantiate_maps_single_required_input_to_one_bare_value`
  - `instantiate_renders_structured_inputs_with_minijinja_loops`
- `crates/rhei-cli/tests/integration.rs` and `crates/rhei-cli/src/main.rs`
  unit parser tests for `Commands::Templates` and `Commands::Instantiate`
  should be considered supporting coverage where present.

User-facing commands:

- `rhei templates [--json] [--source project|user|all]`
- `rhei instantiate <template> --list-inputs`
- `rhei instantiate <template> [input ...] --set KEY=VALUE --values FILE`

### 2. Template Instantiation, Rendering, Settings, Validation, Summary, And Execution

Normative claims:

- `rhei instantiate` accepts a template name or filesystem path, positional
  inputs, bare `KEY=VALUE`, `--set`, `--set-file`, repeatable `--values`,
  `--output`, `--execute`, `--dry-run`, `--keep-on-error`, and `--list-inputs`.
- Input precedence is: manifest defaults, then `--values` files left-to-right,
  then positional inputs, then bare `KEY=VALUE` and `--set` left-to-right, then
  `--set-file` left-to-right.
- A template can declare explicit positional inputs. If it declares exactly one
  required input and no positional fields, one bare value maps to that input.
- `KEY=VALUE` is an assignment only when `KEY` is a valid declared input name;
  values containing `=` with a non-input prefix remain positional.
- Duplicate input sources are allowed and later values in a precedence group
  override earlier values.
- Array/object values from positional input, bare assignments, `--set`, and
  `--set-file` are parsed as YAML/JSON snippets before validation.
- `type: path` values are rendered exactly as supplied or defaulted. The CLI
  should not rewrite them to absolute paths except when resolving a path for
  CLI file operations; omitted optional paths resolve to `""`.
- Rendering uses strict MiniJinja syntax: `{{ expr }}`, `{% for %}`,
  `{% if %}`, `{% raw %}`, and `|slug`; missing variables or properties fail.
- Runtime single-brace variables such as `{task_id}` and
  `{output.name.path}` pass through instantiation untouched.
- `template.yaml` is parsed before rendering, is never templated, and is
  excluded from output.
- Text files are rendered if no null byte appears in the first 8 KiB; binary
  files are copied verbatim; text files must decode as UTF-8.
- `\{{` remains a legacy literal `{{` escape.
- Hidden files/directories and `template.yaml` are excluded from output.
- A root-level template `settings.json` is rendered and moved to
  `.rhei/settings.json` in the output; it must parse as UTF-8 JSON after
  rendering; `${VAR}` secret expansion belongs to runtime settings, not
  instantiation.
- `--output` defaults to `./<template-name>/` and must not already exist in
  normal mode; normal instantiation fails instead of merging or overwriting.
- `--dry-run` materializes into scratch space, skips the requested output path
  existence check, validates the scratch output, reports the requested output
  tree, and writes nothing at the requested output path.
- File permissions and directory structure are preserved for materialized
  files except the root `settings.json` relocation.
- Instantiation runs `rhei validate` after rendering. If the output root has
  `states.yaml`, that file is the state machine; otherwise validation uses the
  built-in default. Settings are composed from global settings and output-root
  `.rhei/settings.json`.
- Validation warnings are printed; validation errors abort and remove output
  unless `--keep-on-error` is passed.
- Successful instantiation prints the specified summary headings and content:
  output path, task/state counts, files, task tree, recent task definitions,
  stop reason, and a reproducible shell-safe `rhei instantiate ... --output`
  invocation.
- Stop reasons distinguish dry-run, already-complete plans, human gates, next
  ready task, blocked tasks, and no claimable task.
- `--execute` runs `rhei run <output>` after successful validation and uses
  the instantiated root `states.yaml` by default when present.
- Exit code `0` means success; exit code `1` covers lookup, input, render,
  output conflict, validation, or execution errors.
- The parser never sees `{{...}}`; templates are a preprocessing layer that
  must produce valid Rhei documents.

Implementation surfaces:

- `crates/rhei-cli/src/main.rs`
  - `templates::instantiate_command`
  - `templates::collect_template_inputs`
  - `templates::parse_template_input_args`
  - `templates::map_template_positional_inputs`
  - `templates::load_template_values_file`
  - `templates::parse_assignment`
  - `templates::compile_full_match_regex`
  - `templates::coerce_template_input_value`
  - `templates::coerce_template_value`
  - `templates::parse_template_sequence`
  - `templates::parse_template_mapping`
  - `templates::parse_structured_template_value`
  - `templates::empty_template_value`
  - `templates::scalar_template_value_as_string`
  - `templates::print_template_inputs`
  - `templates::materialize_template`
  - `templates::materialize_template_dir`
  - `templates::is_text_template_file`
  - `templates::render_template_text`
  - `templates::print_instantiated_workspace_summary`
  - `templates::print_output_tree`
  - `templates::print_task_tree`
  - `templates::render_task_definition`
  - `templates::describe_instantiation_stop`
  - `templates::ready_tasks_from_flat`
  - `templates::blocked_tasks_from_flat`
  - `templates::print_template_instantiation_command`
  - `templates::format_template_instantiation_command`
  - `templates::shell_quote`
  - `run_validation_once`
  - `load_plan`
  - `resolve_state_machine_for_loaded_plan`
  - `auto_state_machine_path`
  - `load_merged_settings`
  - `validate_machine_settings_references`
  - `run_command`
- `crates/rhei-core/src/workspace.rs`
  - `is_workspace`
  - `load_workspace`
- `crates/rhei-validator/src/lib.rs`
  - `StateMachine::builtin_default`
  - `StateMachine::from_yaml_str`
  - `validate_with_machine_and_base`
  - state artifact, profile, node policy, MCP, skill, model, and program
    validation that rendered templates must satisfy.

Tests and fixtures:

- `crates/rhei-cli/tests/e2e/templates_tests.rs`
  - `instantiate_renders_template_variables_and_validates_output`
  - `instantiate_prints_output_tree_task_tail_and_stop_reason`
  - `instantiate_project_hourly_human_intervention_template_prints_summary`
  - `instantiate_accepts_manifest_declared_positional_input`
  - `instantiate_maps_single_required_input_to_one_bare_value`
  - `instantiate_relocates_root_settings_json_into_rhei_dir`
  - `instantiate_renders_structured_inputs_with_minijinja_loops`
  - `instantiate_rejects_template_settings_json_with_malformed_render`
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - workspace auto-discovery, validation, and `states.yaml` lookup tests are
    relevant when they assert instantiated workspace behavior.
- `crates/rhei-validator/src/lib.rs` unit tests around profiles,
  `node_policy`, model selectors, artifact contracts, program states, MCP,
  skills, terminal child coherence, and parent-as-prior rejection.

User-facing commands:

- `rhei instantiate <template> [input ...]`
- `rhei instantiate <template> --dry-run`
- `rhei instantiate <template> --execute`
- `rhei instantiate <template> --keep-on-error`
- `rhei instantiate <template> --set-file KEY=PATH`
- `rhei validate <instantiated-output>`
- `rhei run <instantiated-output>`
- `rhei next <instantiated-output>`

### 3. Bundled Templates, Rendered Examples, And Generated Task Structure

Normative claims:

- Shipped templates are themselves spec-facing workflow surfaces and should
  satisfy the same manifest, skeleton, state-machine, settings, and validation
  rules as user-authored templates.
- A rendered plan declaring `**States:** <name>` must have a bundled or
  auto-discovered `states.yaml` whose YAML `name` matches that declaration.
- Bundled custom `states.yaml` files must be complete state machines, not
  prose sketches; they should include states, transitions, profiles, and
  `node_policy`.
- Generated plans and workspace task files must follow the Rhei plan grammar:
  H1 in the entry point, optional `**States:**`, directory-workspace task files
  under `tasks/`, task headings with `**State:**` first, `**Prior:**` second,
  no authored `**Assignee:**`, and no authored result blocks.
- Generated child tasks must not list their parent or another ancestor as
  `**Prior:**`; follow-up work that waits for the parent should be generated as
  a top-level sibling task.
- Templates that generate living workspaces must keep speculative tasks out of
  the graph until artifacts justify them.
- Examples under `examples/<template-name>-example/` are the pre-rendered smoke
  tests for shipped templates and should pass `rhei validate` as shipped.
- Template README files should document the workflow, inputs, task paths,
  canonical instantiation command, and example location where the
  assistant-facing `rhei-template-writer` skill requires that accompaniment.

Implementation, templates, and fixtures:

- `.agents/rhei/templates/spec-review/`
  - `template.yaml`
  - `index.rhei.md`
  - `states.yaml`
  - `tasks/01-review.md`
- `.agents/rhei/templates/multi-model-analysis/`
  - `template.yaml`
  - `index.rhei.md`
  - `states.yaml`
  - `settings.json`
  - `tasks/01-analysis.md`
- `.agents/rhei/templates/hourly-human-intervention/`
  - `template.yaml`
  - `index.rhei.md`
  - `states.yaml`
  - `settings.json`
  - `tasks/01-fetch-issues.md`
  - `tasks/02-fetch-prs.md`
- `.agents/rhei/templates/changeset-review/`
  - `template.yaml`
  - `index.rhei.md`
  - `states.yaml`
  - `settings.json`
  - `tasks/01-coordinate.md`
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/`
  - `template.yaml`
  - `index.rhei.md`
  - `states.yaml`
  - `tasks/manual-commands.md`
  - `tasks/run-orchestration-agents-programs.md`
  - `tasks/state-machines-transitions.md`
  - `tasks/syntax-authoring-validation.md`
  - `tasks/templates-skills-completions.md`
  - `tasks/tui-monitoring.md`
- `examples/spec-implementation-discrepancy-audit-example/`
  - `instantiation-values.yaml`
  - rendered `index.rhei.md`, `states.yaml`, `tasks/*.md`, `README.md`
- `examples/changeset-review-example/`
  - rendered `index.rhei.md`, `states.yaml`, `tasks/01-coordinate.md`,
    `README.md`
- `examples/hourly-human-intervention-example/`
  - rendered `index.rhei.md`, `states.yaml`, `tasks/*.md`, `README.md`
- `examples/multi-model-analysis/` if present as a rendered example; otherwise
  its absence is in scope for comparison because the template is shipped.
- `examples/living-review-loop/`
  - `index.rhei.md`
  - `team-states.yaml`
  - `tasks/01-review-seed.md`
  - `workflow.sh`
- `examples/changeset-review-example/`, `examples/ci-heal/`,
  `examples/claude-code/`, `examples/review-fix-visits/`, and
  `examples/release-automation.rhei.md` are relevant only where they exercise
  generated or custom-state workflow patterns cited by usage or skills.

Validator and parser surfaces that should catch invalid generated workflows:

- `crates/rhei-core/src/parser.rs`
  - task metadata ordering and workspace parsing.
- `crates/rhei-core/src/workspace.rs`
  - task-source merging for directory workspaces.
- `crates/rhei-validator/src/lib.rs`
  - `validate_dependency_integrity`
  - `validate_sibling_uniqueness`
  - `validate_state_consistency`
  - `validate_terminal_tree_coherence`
  - `validate_circular_dependencies`
  - `validate_profiles_and_node_policy`
  - `validate_model_configuration`
  - `validate_program_configuration`
  - `validate_artifact_definitions`
  - `validate_state_skill_entries`
  - `validate_state_mcp_entries`
  - tests `rejects_child_prior_to_parent` and
    `rejects_descendant_prior_to_ancestor`

### 4. Assistant-Facing Skills And Writer Guidance

Normative claims:

- `rhei install-skills` installs the Rhei skills used by coding agents:
  `rhei-plan-writer`, `rhei-plan-worker`, and `rhei-state-machine-writer` by
  default; `--skills` selects a comma-separated subset.
- Skills are assistant-facing workflow contracts and should accurately encode
  current CLI behavior, validator constraints, state-machine schema, and
  runtime ownership boundaries.
- The plan writer skill should create valid single-file plans and directory
  workspaces, use `**State:**` first and `**Prior:**` second, avoid authoring
  runtime-owned assignee/result fields, resolve initial states through
  `profiles`/`node_policy`, and avoid child tasks whose `**Prior:**` lists a
  parent or ancestor.
- The plan worker skill should validate before work, load state-machine
  instructions, use `rhei next` for claiming, respect gating and terminal
  states, use `rhei transition`/`rhei complete` for root states in manual mode,
  not reimplement task selection, and distinguish manual worker authority from
  `rhei run` orchestrator authority.
- The state-machine-writer skill/spec should produce one YAML file conforming
  to the current state-machine format, with `states`, `transitions`,
  `profiles`, and `node_policy`; at least one final state; explicit
  transitions; cancellation paths; connected/reachable profile graphs;
  profile-level initials rather than state-level `initial: true`; and
  instructions that avoid prose exit conditions under orchestrator authority.
- The state-machine writer should gather project specification and team
  structure; map phases to states, approvals to gating states, handoffs to
  transitions, autonomous actors to work states, callbacks to transition
  hooks; validate with `rhei states --state-machine <path>`.
- The template-writer skill should package a reusable workflow as a complete
  template directory with manifest, plan skeleton, optional complete
  `states.yaml`, optional settings, README, and pre-rendered example; it should
  instantiate before validating and check rendered examples with `rhei
  validate` and `rhei run --dry-run`.
- The template-writer skill must preserve the boundary between MiniJinja
  instantiation variables and runtime single-brace variables, enforce the
  manifest rules, move only root `settings.json`, and validate rendered
  `states.yaml` rather than raw templated YAML.

Skill files:

- `skills/rhei-plan-writer/SKILL.md`
  - `Output Contract`
  - `Single-File Plan`
  - `Directory Workspace`
  - `Task Block`
  - `Allowed States`
  - `Child Task Format`
  - `Dependencies`
  - `Validation Checklist`
  - `File Extension`
- `skills/rhei-plan-worker/SKILL.md`
  - `Operating Loop`
  - `Task Selection`
  - `State Transitions`
  - `Assignee Discipline`
  - `Progress Logging`
  - `Agent Review`
  - `Editing Discipline`
  - `Stopping Conditions`
  - `Unorchestrated Mode Notes`
- `skills/rhei-state-machine-writer/SKILL.md`
  - `Output Contract`
  - `State Design`
  - `Transition Design`
  - `Profiles And Node Policy`
  - `Instruction Design`
  - `Workflow`
  - `Validation Checklist`
  - `File Placement`
- `skills/rhei-template-writer/SKILL.md`
  - `Output Contract`
  - `Template Layout`
  - `Required Accompaniments`
  - `Manifest Contract`
  - `Instantiation Template Syntax`
  - `Design Rules`
  - `Workflow`
  - `Validation Checklist`
  - `File Placement`

Implementation and tests:

- `crates/rhei-cli/src/main.rs`
  - `install_skills_command`
  - `resolve_skill_source`
  - `complete_skill_name`
  - `build_marker_content`
  - `install_claude_code`
  - `install_cursor`
  - `install_codex`
- `crates/rhei-cli/tests/e2e/install_skills_tests.rs`
  - coverage that the copied/linked skill content and generated registration
    files preserve the current skill files.
- `crates/rhei-validator/src/lib.rs`
  - validator constraints that skill guidance must not contradict, especially
    `profiles`/`node_policy`, state-level `initial`, parent/ancestor priors,
    terminal descendants, artifacts, MCP/skills, models, and programs.

### 5. Skill Installation Command And Agent-Specific Paths

Normative claims:

- `rhei install-skills` usage accepts `--agent <NAME>`, `--local`, `--link`,
  `--uninstall`, `--dry-run`, and `--skills <LIST>`.
- Supported agents are `claude-code`, `cursor`, `windsurf`, `copilot`,
  `kilocode`, `pi`, `codex`, `antigravity`, and `all`; `all` expands to every
  concrete agent.
- Default skill list is
  `rhei-plan-writer,rhei-plan-worker,rhei-state-machine-writer`.
- Global and local paths are agent-specific:
  - Claude Code: `~/.claude/skills/rhei-<skill>/` and `~/.claude/CLAUDE.md`;
    local `.claude/skills/rhei-<skill>/` and `.claude/CLAUDE.md`.
  - Cursor: `~/.cursor/rules/rhei-<skill>.mdc`; local
    `.cursor/rules/rhei-<skill>.mdc`.
  - Windsurf: `~/.windsurfrules` or
    `~/.codeium/windsurf/memories/global_rules.md`; local `.windsurfrules`.
  - Copilot: `~/.github/copilot-instructions.md`; local
    `.github/copilot-instructions.md`.
  - Kilocode: `~/.kilocode/rules/rhei-<skill>.md`; local equivalent.
  - Pi: `~/.pi/rules/rhei-<skill>.md`; local equivalent.
  - Codex: `~/.agents/skills/rhei-<skill>/SKILL.md`; local
    `.agents/skills/rhei-<skill>/SKILL.md`; no registration file.
  - Antigravity: `~/.antigravity/rules/rhei-<skill>.md`; local equivalent.
- Claude registration uses a `# rhei` block with skill paths and triggers.
- Cursor files use `.mdc` frontmatter and embed `SKILL.md` content.
- Windsurf and Copilot use marker-delimited markdown sections.
- Kilocode, Pi, and Antigravity use plain markdown rule files.
- Codex uses standard skill directories and discovers local/global skills
  without marker registration.
- Local installation finds a project root by walking up for common markers and
  falls back to `cwd`; local symlinks are relative.
- Re-running installation removes/replaces existing Rhei skill files and
  refreshes them.
- Skill sources resolve relative to the installed binary
  `../share/rhei/skills/` or repo dev fallback `skills/`.
- `--link` symlinks; default copies.
- `--dry-run` prints planned actions and does not write.
- `--uninstall` removes copied/symlinked skill files and registration blocks.

Implementation surfaces:

- `crates/rhei-cli/src/main.rs`
  - `Agent`
  - `Commands::InstallSkills`
  - `install_skills_command`
  - `expand_agent_list`
  - `agent_label`
  - `install_agent`
  - `install_claude_code`
  - `inject_claude_md_section`
  - `skill_description`
  - `install_cursor`
  - `install_rules_dir_agent`
  - `install_windsurf`
  - `install_copilot`
  - `install_codex`
  - `build_marker_content`
  - `uninstall_agent`
  - `remove_path`
  - `copy_skill`
  - `copy_dir_recursive`
  - `link_skill`
  - `relative_path`
  - `resolve_skill_source`
  - `find_project_root`
  - `inject_marked_section`
  - `remove_marked_section`
  - `complete_skill_name`

Tests:

- `crates/rhei-cli/tests/e2e/install_skills_tests.rs`
  - `global_install_copy_claude_code`
  - `local_install_cursor`
  - symlink installation test for Kilocode
  - `global_install_copy_codex`
  - `uninstall_removes_files`
  - `dry_run_does_not_create_files`
  - `reinstall_overwrites_existing_skill_files`

User-facing command:

- `rhei install-skills --agent <agent|all> [--local] [--link] [--uninstall]`
  `[--dry-run] [--skills rhei-plan-writer,rhei-plan-worker,...]`

### 6. Shell Completion Generation, Installation, Dynamic Context, And Candidate Coverage

Normative claims:

- `rhei completions <SHELL>` writes only the completion script to stdout
  unless installation/output options are supplied.
- Supported shells are `bash`, `zsh`, `fish`, `powershell`, and `elvish`.
- `--install` writes to the shell's default user or system path; `--system`
  chooses system paths; `--output <PATH>` writes to an explicit path;
  `--dry-run` prints the destination path without writing.
- Install creates parent directories and overwrites the completion file
  atomically enough for normal CLI use.
- Generated scripts call back into the current `rhei` binary through the
  `COMPLETE` environment variable and should reflect command tree, global
  flags, value enums, dynamic values, and template input arguments.
- Completions are advisory and must not mutate plans or metadata, acquire task
  locks, run callbacks, spawn agents/programs, use the network, or print
  human diagnostics to stdout during dynamic completion.
- Completion must degrade quietly when plans, workspaces, state machines,
  settings files, values files, or template manifests cannot be read.
- Completion should filter by current prefix, preserve shell quoting/escaping,
  and provide help text where supported.
- Filesystem completion domains:
  - `RHEI_PLAN`: `.rhei.md` files and workspace directories containing
    `index.rhei.md`, with directories traversable.
  - `--state-machine`: `.yaml` and `.yml`.
  - `--values`: `.yaml`, `.yml`, `.json`.
  - `--output`: directories and new leaf paths.
  - `completions --output`: files and new leaf paths.
  - `--set-file KEY=PATH`: file paths after `=`.
  - template `type: path`: files and directories.
- Command coverage includes global `--state-machine`, plan positionals,
  render format values, list filters, template sources, instantiate template
  names and inputs, run agent/mode/model/timeout/parallel values, next/complete
  task ids, transition task/from/to state values, reset plan paths,
  install-skills agent and comma-aware skill values, and completions shell
  names.
- `rhei instantiate <TEMPLATE>` completes project templates before user
  templates, hides user duplicates when project duplicates exist, shows
  template description/source, and switches to directory completion for
  path-like template references.
- Template input completion parses the command line using instantiation
  precedence, offers the next positional slot or single-required-input
  fallback, suggests remaining `KEY=` assignments, hides supplied
  non-repeatable input keys unless editing that key, and completes right-hand
  values by type.
- `--set-file` completes keys like `--set` but always uses file completion for
  the value.
- `--values` completion may parse readable YAML/JSON values files for ranking
  but must ignore parse failures.
- `--list-inputs` should not require required inputs.
- Task/state completion reads plan/workspace data and resolved state machine
  without claiming tasks or triggering side effects.
- `rhei list --state` is comma-aware; `--assignee`, `--kind`, `--has-prior`,
  and `--parent` complete values present in the selected plan/workspace.
- `rhei transition --from` uses the selected task's current state when known;
  `--to` uses allowed target states from the known/selected from state.
- `rhei run --agent`, `--agent-mode`, and `--model` complete from the same
  merged settings and state-machine resolution used by execution.
- `rhei install-skills --skills` is comma-aware and avoids duplicating already
  supplied skills.
- Acceptance tests should cover every command and dynamic area listed in the
  completions spec.

Implementation surfaces:

- `crates/rhei-cli/src/main.rs`
  - `Commands::Completions`
  - `CompletionShell`
  - `CompletionShell::as_str`
  - `main` call to `CompleteEnv::with_factory(cli_command).bin("rhei").complete()`
  - `completions_command`
  - `write_completion_file`
  - `write_completion_registration`
  - `completion_env_completer`
  - `completion_install_path`
  - `complete_any_path`
  - `complete_yaml_path`
  - `complete_values_path`
  - `complete_rhei_plan_path`
  - `complete_path_with_extensions`
  - `path_completion_candidate`
  - `complete_template_source`
  - `complete_parallel`
  - `complete_duration`
  - `complete_limit`
  - `complete_skill_name`
  - `static_completion`
  - `complete_agent_name`
  - `complete_agent_mode`
  - `complete_model_name`
  - `complete_assignee`
  - `complete_node_kind`
  - `complete_task_id`
  - `complete_transition_from_state`
  - `complete_transition_to_state`
  - `complete_comma_state_name`
  - `complete_state_name`
  - `complete_state_name_with_prefix`
  - `completion_state_machine`
  - `completion_workspace_root`
  - `completion_plan_path`
  - `completion_command_name`
  - `first_command_positional`
  - `completion_option_value`
  - `completion_words`
  - `templates::complete_template_reference`
  - `templates::complete_template_input_arg`
  - `templates::complete_template_set_value`
  - `templates::complete_template_set_file`
  - `templates::complete_template_input_value`
  - `templates::complete_template_assignment`
  - `templates::complete_template_assignment_keys`
  - `templates::complete_template_value_for_input`
  - `templates::template_input_help`
  - `templates::next_positional_input`
  - `templates::supplied_template_input_keys`
  - `templates::completion_template_context`
  - `templates::completion_template_and_inputs`
- `crates/rhei-cli/Cargo.toml`
  - `clap_complete` with `unstable-dynamic`.
- `crates/rhei-cli/src/main.rs` command definitions where `ArgValueCompleter`
  is attached to specific command arguments and options.

Tests:

- `crates/rhei-cli/tests/e2e/completions_tests.rs`
  - `generates_all_supported_shell_completions`
  - `generates_bash_completions`
  - `generates_fish_completions`
  - `dynamic_completion_lists_commands`
  - `dynamic_completion_lists_instantiate_templates`
  - `dynamic_completion_filters_instantiate_templates_by_prefix`
  - `dynamic_completion_lists_template_input_assignments`
  - `dynamic_completion_completes_set_keys_and_boolean_values`
  - `dynamic_completion_completes_task_ids_and_transition_targets`
  - `dynamic_completion_completes_list_filters`
  - `rejects_unknown_completion_shell`
  - `generating_to_stdout_does_not_touch_home_completion_paths`
  - `installs_fish_completions_to_user_default_path`
  - `installs_fish_completions_to_xdg_config_home`
  - `installs_bash_completions_to_xdg_data_home`
  - `installs_completions_to_explicit_output_path`
  - completion overwrite test
  - relative-output install test
  - `dry_run_reports_system_install_path_without_writing`
- `crates/rhei-cli/src/main.rs` unit tests
  - `parses_completions_command`
  - `parses_completions_install_options`
  - `root_help_lists_completions_command`

User-facing commands:

- `rhei completions bash|zsh|fish|powershell|elvish`
- `rhei completions <shell> --install [--user|--system]`
- `rhei completions <shell> --install --output <path>`
- `rhei completions <shell> --dry-run`

### 7. Usage Workflows Connecting Templates, Skills, Completions, And Generated Workspaces

Normative claims:

- Rhei plans are shared artifacts with distinct role mandates: plan writer
  structures work, plan worker executes, reviewer inspects, human operator
  unblocks human gates and may override.
- The state machine is the coordination protocol: it defines states,
  transitions, actors, and handoffs. Agents communicate through plan state and
  artifacts.
- Manual-worker and `rhei run` execution are mutually exclusive per task:
  workers use `rhei next`, `rhei transition`, and `rhei complete`; spawned
  agents under `rhei run` leave state mutation to the orchestrator.
- `human-review` and other gating states are human-only and must not be
  bypassed by automated workflows.
- Directory workspaces are the recommended generated shape for highly
  concurrent or living workflows because task files can be appended/merged
  independently.
- Living workspace expansion should append verification/fix tasks only after
  concrete artifacts justify them; speculative follow-up tasks should not be
  present up front.
- Program states and CI/CD plans are valid generated workflow examples when
  deterministic commands, exit codes, and human gates are encoded in the state
  machine.
- The plan file/workspace is the single source of truth for resumability,
  auditability, human legibility, and composability.

Implementation and workflow surfaces:

- `skills/rhei-plan-writer/SKILL.md`
- `skills/rhei-plan-worker/SKILL.md`
- `skills/rhei-state-machine-writer/SKILL.md`
- `skills/rhei-template-writer/SKILL.md`
- `.agents/rhei/templates/*`
- `examples/living-review-loop/`
- `examples/review-fix-visits/`
- `examples/ci-heal/`
- `examples/release-automation.rhei.md`
- `crates/rhei-cli/tests/e2e/fixtures/living-review-loop/`
- `crates/rhei-cli/tests/e2e/fixtures/bash-agent-team/`
- `crates/rhei-cli/tests/e2e/run_tests.rs` where it verifies orchestrator
  handling of generated/living/task-spawning workflows.

Commands that should satisfy the user-facing workflow:

- `rhei run <plan-or-workspace>`
- `rhei run <plan-or-workspace> --dry-run`
- `rhei run <plan-or-workspace> --parallel <N>`
- `rhei next <plan-or-workspace> [--peek]`
- `rhei transition <plan-or-workspace> --task <id> --from <state> --to <state>`
- `rhei complete <plan-or-workspace> --task <id> --result <text>`
- `rhei validate <plan-or-workspace>`
- `rhei states <plan-or-workspace> [--json]`
- `rhei instantiate <template> --execute`

