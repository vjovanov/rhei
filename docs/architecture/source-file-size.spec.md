# AR-source-file-size: Source File Size Architecture

Rhei source files are part of the working memory shared by humans and agents.
They must stay small enough for focused review, reliable agent context loading,
and predictable refactoring. This architecture rule supports readable,
reviewable plans and predictable execution work. §GOAL-rhei-outcomes

## File Size Policy

Hand-authored source, template, example, and test files must be kept in the
500-line range.

- A file at or below 500 lines needs no special justification.
- A file above 500 lines and at or below 2000 lines is a large-file exception.
  It must be listed in a large-file register with its path, reason the size is
  necessary, owner or owning area, and the condition that should trigger
  splitting it. The register must not record exact line counts because they
  become stale quickly.
- A file above 2000 lines is not allowed. It must be split before the work that
  creates or expands it is considered architecturally complete.

Generated files, vendored third-party files, lockfiles, and external fixtures
may be excluded only when they are clearly marked as generated or third-party
and are not edited by hand. If a generated file becomes a regular hand-edited
maintenance surface, this policy applies to it.

Textual specification files with the `.spec.md` suffix are exempt from the
500-line exception register because they are addressed through grund
declarations and citations instead of being loaded as one undifferentiated file.
They may grow past 500 lines when the declaration remains coherent and
navigable through `grund <ID>`, `grund <ID> --toc`, and subsection reads.

## Large-File Register

Large-file exceptions belong in this section or in a more specific `AR-*`
document that cites this rule. Each entry must explain why the file cannot yet
be split without making the design harder to understand.

| Path | Reason | Split Trigger |
|---|---|---|
| `crates/rhei-cli/src/main_parts/run_agent_mode.rs` | Mechanical extraction from the former CLI monolith; still one orchestration loop. | Split into scheduler, sequential execution, parallel execution, and result handling modules. |
| `examples/hourly-human-intervention-example/states.yaml` | Example state machine intentionally shows a complete workflow in one file. | Split by template/example support if Rhei gains multi-file state machines. |
| `.agents/rhei/templates/hourly-human-intervention/states.yaml` | Template state machine mirrors the example as an instantiable workflow. | Split by template support if Rhei gains multi-file state machines. |
| `.agents/rhei/templates/spec-implementation/states.yaml` | Template state machine must be copied as one instantiable workflow artifact. | Split by template support if Rhei gains multi-file state machines. |
| `examples/spec-implementation-example/states.yaml` | Example mirrors the spec-implementation template state machine. | Split by template support if Rhei gains multi-file state machines. |
| `crates/rhei-cli/tests/e2e/completions_tests.rs` | E2E completion scenarios share setup and assertions. | Split by shell or command group when new cases are added. |
| `crates/rhei-cli/tests/e2e/templates_tests.rs` | Template E2E scenarios share fixtures and setup. | Split by template command area when new cases are added. |
| `crates/rhei-cli/tests/e2e/next_tests.rs` | `next` command E2E scenarios share command fixtures. | Split by readiness, assignee, and transition behavior when new cases are added. |
| `.agents/rhei/templates/changeset-review/states.yaml` | Template state machine must be copied as one instantiable workflow artifact. | Split by template support if Rhei gains multi-file state machines. |
| `crates/rhei-cli/tests/e2e/run_tests.rs` | Run-command E2E scenarios share setup and process assertions. | Split by callback, agent, program, and snapshot behavior when new cases are added. |
| `examples/changeset-review-example/states.yaml` | Example mirrors the changeset-review template state machine. | Split by template support if Rhei gains multi-file state machines. |
| `crates/rhei-core/src/ast.rs` | AST types are reviewed together as the core language model. | Split workspace/task/state structs if more public fields are added. |

## Split Shape

When a file crosses the hard limit, split it along existing behavioral
boundaries first. The split should preserve public behavior and make the next
split obvious. Do not create arbitrary numeric chunks unless the file is being
split mechanically as a temporary containment step; those chunks must still be
named after the behavior they contain.

`crates/rhei-cli/src/main.rs` is only the CLI shell. It includes focused parts
under `crates/rhei-cli/src/main_parts/`:

- `cli_*` contains clap command declarations and top-level dispatch.
- `completions_*` contains shell completion and completion-context helpers.
- `templates_*` contains template discovery, validation, rendering, and
  materialization.
- `states_render`, `metadata_*`, `transition_*`, `artifacts`, and
  `system_transitions_*` contain state-machine inspection, plan metadata,
  artifact contracts, and transition application.
- `run_options`, `settings_*`, `tooling_*`, `agent_resolution`,
  `agent_command`, `agent_spawn`, and `programs` contain run configuration,
  settings merge/validation, tooling resolution, agent command construction,
  agent spawning, and program-state execution.
- `snapshots_*` and `snapshot_runtime_*` contain snapshot CLI/cache handling
  and run-loop snapshot emit/preload hooks.
- `run_command`, `run_agent_mode`, `run_callback_mode`,
  `run_failure_transitions`, and `ready_transition` contain orchestration,
  scheduling, failure routing, and automatic transition selection.
- `next_command`, `complete_reset_*`, `render_install_*`,
  `install_skills_*`, and `diagnostics` contain the remaining command
  families and shared diagnostics.
- `tests_*` contains CLI unit tests split by nearby behavior. Add new unit
  tests next to the part that owns the behavior.

`crates/rhei-validator/src/lib.rs` is only the validator shell. It includes
focused parts under `crates/rhei-validator/src/lib_parts/`:

- `preamble` contains public imports, report types, errors, agent/profile
  schema primitives, and target parsing.
- `state_defs` contains state, snapshot, profile, node-policy, and state
  machine declarations.
- `state_machine_impl` contains `StateMachine` loading, core accessors, and
  model/target validation.
- `state_machine_snapshots` contains snapshot emit/inherit validation.
- `state_machine_runtime_validation` contains program, poll, and tooling
  validation.
- `state_machine_profiles` contains profile/node-policy validation, schema
  version interpretation, and template-condition validation.
- `validation_helpers_*` contains shared semantic validators and parsing
  helpers.
- `validator_entry` contains public validation entrypoints, plan traversal,
  state/profile checks, dependency integrity, and terminal-tree coherence.
- `validator_links` contains Markdown link extraction and file-reference
  validation.
- `tests_*`, `tests_profiles`, `tests_poll`, and `tests_snapshots` contain
  validator unit tests split by validation topic.

`crates/rhei-core/src/parser.rs` is only the parser API shell and shared
frontmatter helpers. Parser implementation parts live under
`crates/rhei-core/src/parser/`:

- `builder` contains node-stack assembly and node finalization.
- `plan` contains the main Markdown plan parser.
- `recovery` contains best-effort multi-error parsing.
- `workspace` contains directory workspace index and task-file parsing.
- `plan_tests` and `workspace_tests` keep parser tests beside the behavior
  they exercise.

`crates/rhei-tui/src/dashboard.rs` is only the dashboard sink and HTTP
request shell. Dashboard parts live under `crates/rhei-tui/src/dashboard/`:

- `state` contains event reduction and serializable dashboard payload types.
- `html` contains the embedded browser UI.
- `tests` contains dashboard state and URL-encoding tests.

`crates/rhei-tui/src/tui.rs` is only the terminal lifecycle, channel, and
input loop shell. Terminal UI parts live under `crates/rhei-tui/src/tui/`:

- `state` contains event reduction and snapshot construction.
- `render` contains ratatui layout and widget rendering.
- `text` contains stream labels, truncation, and terminal-text sanitization.
- `tests` contains terminal input, state, rendering-line, and text tests.

`crates/rhei-output/src/lib.rs` is only the output crate API shell. Renderer
parts live beside it:

- `json` contains JSON conversion.
- `github` contains GitHub-oriented Markdown rendering.
- `progress` contains terminal progress report rendering.
- `common` contains shared task-label formatting helpers.
- `tests` contains renderer tests split out of the public API shell.

`crates/rhei-cli/tests/integration_markdown_plans.rs` is only the integration
test shell. Shared fixture helpers live in
`crates/rhei-cli/tests/integration_markdown_plans/common.rs`; behavior groups
live in sibling files named for their command or behavior area:
`validation_*`, `transitions_*`, `callbacks_*`, `run_*`, `reset`, and
`workspace_*`.

Future work must keep new code inside the owning part file or create a new
part with a behavior name. If adding code would push a part past the 500-line
range, split that part before adding more behavior. If a temporary mechanical
part such as `tests_4` remains above the target range, only shrink or split it;
do not add unrelated behavior to it.

## Current Violations

No hand-authored repository file is currently known to be above the hard
2000-line limit. New work must not introduce one.

| Path | Required Direction |
|---|---|
| _None._ | |
