# Discrepancy Audit: Templates, Skills, Completions, and Generated Workflows

Partition: `templates-skills-completions`

Scope source: `runtime/spec-implementation-discrepancy-audit/templates-skills-completions/scope.md`

This audit compares the scoped template, skill-installation, completion, assistant-skill, and generated-workflow claims against implementation code, tests, shipped templates, examples, and targeted CLI behavior. It records discrepancies only; it does not propose fixes.

## TSC-001: Template Discovery, Listing, and Layout

Classification: `implementation-diverges`

`rhei templates` silently skips invalid template directories instead of printing the warning required by the template discovery spec. The spec says invalid/unreadable template directories are skipped with a warning naming the path (`docs/specs/rhei-templates.spec.md:31`, `docs/specs/rhei-templates.spec.md:33`, `docs/specs/rhei-templates.spec.md:396`). The implementation catches manifest-load errors and `continue`s without emitting anything (`crates/rhei-cli/src/main.rs:2489`, `crates/rhei-cli/src/main.rs:2490`). A targeted CLI check with an invalid project template returned status 0, printed "No templates found", and wrote no stderr warning.

Classification: `implementation-diverges`

The plain `rhei templates` listing omits fields the spec requires. The spec says the command prints name, version, description, source path, and required input count (`docs/specs/rhei-templates.spec.md:394`). The JSON output includes `path` and `required_inputs` (`crates/rhei-cli/src/main.rs:1688`, `crates/rhei-cli/src/main.rs:1694`), but the plain output prints only name/version/source, description, and an input-name summary (`crates/rhei-cli/src/main.rs:1727`, `crates/rhei-cli/src/main.rs:1737`). The e2e test currently asserts that the template path is absent from plain output (`crates/rhei-cli/tests/e2e/templates_tests.rs:63`, `crates/rhei-cli/tests/e2e/templates_tests.rs:67`).

Classification: `no-discrepancy`

Named discovery order, duplicate hiding, direct path references, manifest loading, and one-entrypoint layout detection are implemented. Project roots are searched before user roots (`crates/rhei-cli/src/main.rs:2506`, `crates/rhei-cli/src/main.rs:2515`), duplicate template directory names are hidden by `seen` (`crates/rhei-cli/src/main.rs:2467`, `crates/rhei-cli/src/main.rs:2485`), path-like references bypass named discovery (`crates/rhei-cli/src/main.rs:2522`, `crates/rhei-cli/src/main.rs:2543`), and layout validation enforces exactly one of `plan.rhei.md` or `index.rhei.md` with `tasks/` for workspace templates (`crates/rhei-cli/src/main.rs:2733`, `crates/rhei-cli/src/main.rs:2758`).

## TSC-002: Template Manifest Validation and Input Coercion

Classification: `implementation-diverges`

`type: path` inputs are normalized to absolute paths and non-default user paths must exist, contrary to the spec's "render exactly as supplied" rule. The spec says path values are rendered exactly as supplied/defaulted and are only resolved when the CLI itself must perform file operations; omitted optional paths resolve to `""` (`docs/specs/rhei-templates.spec.md:101`, `docs/specs/rhei-templates.spec.md:103`, `docs/specs/rhei-templates.spec.md:233`, `docs/specs/rhei-templates.spec.md:235`). The implementation rejects empty raw path values, joins relative paths against `cwd`, checks existence for non-default values, and stores the resolved path string (`crates/rhei-cli/src/main.rs:3036`, `crates/rhei-cli/src/main.rs:3051`). Targeted CLI checks showed `rhei instantiate spec-review docs/rhei.spec.md` rendering `/home/vjovanov/c/rhei/docs/rhei.spec.md`, and `rhei instantiate spec-review does/not/exist` failing before render.

Classification: `implementation-diverges`

Bare `KEY=VALUE` parsing rejects undeclared identifier keys instead of treating them as positional values when `KEY` is not a declared input. The spec says `KEY=VALUE` is an assignment only when `KEY` names a declared input; values containing `=` with a non-input prefix remain positional (`docs/specs/rhei-templates.spec.md:283`, `docs/specs/rhei-templates.spec.md:289`). The implementation treats any identifier-looking prefix as an assignment attempt and errors if the key is undeclared (`crates/rhei-cli/src/main.rs:2875`, `crates/rhei-cli/src/main.rs:2884`). A targeted CLI check with `rhei instantiate spec-review foo=bar` failed with "does not declare an input named 'foo'" instead of applying the single-required-input positional fallback.

Classification: `no-discrepancy`

The main manifest validation rules are implemented for top-level inputs: manifest name matches directory name, names/descriptions are non-empty identifiers, input names are unique, `required: true` conflicts with `default`, positional indexes are positive/contiguous, `validate` is scalar-only and compiled as a full-match Rust regex, arrays require `items`, and illegal `items`/`properties` combinations are rejected (`crates/rhei-cli/src/main.rs:2557`, `crates/rhei-cli/src/main.rs:2648`, `crates/rhei-cli/src/main.rs:2653`, `crates/rhei-cli/src/main.rs:2731`). Defaults and supplied values are coerced by type, and arrays/objects supplied as strings are parsed as YAML/JSON snippets (`crates/rhei-cli/src/main.rs:2984`, `crates/rhei-cli/src/main.rs:3157`).

## TSC-003: Template Materialization, Settings, Validation, Summary, and Execute

Classification: `no-discrepancy`

Rendering and materialization match the core template contract: output conflicts are rejected outside dry-run, dry-run uses scratch space, `template.yaml` and hidden files are excluded, a root `settings.json` is moved to `.rhei/settings.json`, rendered settings are JSON-validated, text-vs-binary detection checks for null bytes in the first 8 KiB, UTF-8 text files are rendered with strict MiniJinja, `|slug` is registered, and legacy `\{{` escapes are consumed (`crates/rhei-cli/src/main.rs:2036`, `crates/rhei-cli/src/main.rs:2051`, `crates/rhei-cli/src/main.rs:3240`, `crates/rhei-cli/src/main.rs:3295`, `crates/rhei-cli/src/main.rs:3316`, `crates/rhei-cli/src/main.rs:3339`). E2E tests cover rendering, structured-input loops, settings relocation, malformed rendered settings rejection, summary output, and positional/single-required input behavior (`crates/rhei-cli/tests/e2e/templates_tests.rs:72`, `crates/rhei-cli/tests/e2e/templates_tests.rs:588`).

Classification: `missing-validation`

Template post-instantiation validation does not enforce the settings-reference claim for MCP server and skill ids. The template settings spec says state-machine MCP/skill references must resolve through template-shipped or global settings and `rhei validate` surfaces remaining dangling references as errors (`docs/specs/rhei-templates.spec.md:161`, `docs/specs/rhei-templates.spec.md:165`). The validation hook after plan validation calls `validate_machine_settings_references` (`crates/rhei-cli/src/main.rs:3555`, `crates/rhei-cli/src/main.rs:3557`), but that function checks only `agent`, `agent_mode`, and `target` selectors; it does not check state `mcp_servers` or `skills` against the merged registries (`crates/rhei-cli/src/main.rs:5749`, `crates/rhei-cli/src/main.rs:5810`).

Classification: `no-discrepancy`

Instantiation validates the rendered output and uses a root `states.yaml` when present. After materialization, `instantiate_command` computes the rendered entrypoint, passes the rendered root `states.yaml` into `run_validation_once`, removes the output on validation failure unless `--keep-on-error`, prints the specified summary sections, prints a shell-quoted reproducible command, and `--execute` delegates to `run_command` with the rendered entrypoint and state-machine path (`crates/rhei-cli/src/main.rs:2064`, `crates/rhei-cli/src/main.rs:2119`, `crates/rhei-cli/src/main.rs:2125`, `crates/rhei-cli/src/main.rs:2423`).

## TSC-004: Shipped Templates, Examples, and Generated Workflow Shape

Classification: `implementation-diverges`

Several shipped templates/examples still use legacy state-machine shape even though the scoped claims say bundled custom `states.yaml` files should be complete current machines with `profiles` and `node_policy`. `changeset-review`, `hourly-human-intervention`, and `spec-review` declare state-level `initial: true` and omit profile/node-policy blocks (`.agents/rhei/templates/changeset-review/states.yaml:65`, `.agents/rhei/templates/hourly-human-intervention/states.yaml:110`, `.agents/rhei/templates/spec-review/states.yaml:24`). Their rendered examples or related examples validate because the loader still accepts legacy machines without profiles/node policy (`crates/rhei-validator/src/lib.rs:631`, `crates/rhei-validator/src/lib.rs:638`).

Classification: `implementation-diverges`

The `spec-review` template violates the assistant-facing template accompaniment contract, and `multi-model-analysis` has no checked-in smoke example. The template-writer skill requires a root README and a pre-rendered example under `examples/<template-name>-example/` that passes `rhei validate` (`skills/rhei-template-writer/SKILL.md:82`, `skills/rhei-template-writer/SKILL.md:122`). Filesystem audit found no `.agents/rhei/templates/spec-review/README.md`, no `examples/spec-review-example/`, and no `examples/multi-model-analysis-example/`. The `spec-review` template itself exists at `.agents/rhei/templates/spec-review/template.yaml:1`, and `multi-model-analysis` exists at `.agents/rhei/templates/multi-model-analysis/template.yaml:1`.

Classification: `implementation-diverges`

The path-input normalization issue leaks into shipped generated workflows. `hourly-human-intervention` declares workspace-relative `type: path` defaults such as `master`, `graalvm`, `graalvm/ce`, and `graalvm/ee` (`.agents/rhei/templates/hourly-human-intervention/template.yaml:16`, `.agents/rhei/templates/hourly-human-intervention/template.yaml:34`), but instantiation from the repo root rendered those values as absolute `/home/vjovanov/c/rhei/...` paths in the plan and states. This contradicts the template spec's path preservation rule and can make generated workspaces machine-specific (`docs/specs/rhei-templates.spec.md:101`, `crates/rhei-cli/src/main.rs:3036`, `crates/rhei-cli/src/main.rs:3051`).

Classification: `no-discrepancy`

The checked-in examples that exist for the current shipped templates pass validation as shipped. Targeted audit runs succeeded for `examples/changeset-review-example`, `examples/hourly-human-intervention-example`, `examples/spec-implementation-discrepancy-audit-example`, `examples/review-fix-visits`, and `examples/ci-heal`. The rendered template examples also preserve task metadata ordering (`**State:**` first, `**Prior:**` second where present) in the inspected task files, for example `.agents/rhei/templates/spec-implementation-discrepancy-audit/tasks/templates-skills-completions.md:1` and `.agents/rhei/templates/spec-implementation-discrepancy-audit/tasks/templates-skills-completions.md:2`.

## TSC-005: Assistant-Facing Skills and Writer Guidance

Classification: `implementation-diverges`

The plan-writer skill's default-state reference describes a built-in default machine version `3.0` with profiles/node policy, but the compiled default used by the CLI is version `2.0` with state-level `initial: true` and no profiles/node policy. Evidence: `skills/rhei-plan-writer/references/default-states.md:3`, `skills/rhei-plan-writer/references/default-states.md:29`; `crates/rhei-validator/src/default-states.yaml:5`, `crates/rhei-validator/src/default-states.yaml:17`.

Classification: `implementation-diverges`

The plan-worker skill says `cancelled` terminal priors satisfy task selection, but runtime readiness excludes cancelled dependencies. The skill says every prior is ready when it is terminal, "in the default machine that means `completed` or `cancelled`" (`skills/rhei-plan-worker/SKILL.md:43`, `skills/rhei-plan-worker/SKILL.md:45`). The CLI dependency helper rejects `cancelled` even if it is final (`crates/rhei-cli/src/main.rs:8662`, `crates/rhei-cli/src/main.rs:8667`). This is an assistant-facing workflow mismatch because workers following the skill may expect tasks after cancelled priors to become claimable.

Classification: `implementation-diverges`

The state-machine-writer skill and spec encode writer output that the current validator/runtime does not fully support. They show object-shaped `node_policy.overrides[].match` with `{ type, level }` (`docs/specs/rhei-state-machine-writer.spec.md:100`, `docs/specs/rhei-state-machine-writer.spec.md:103`; `skills/rhei-state-machine-writer/SKILL.md:153`, `skills/rhei-state-machine-writer/SKILL.md:157`), but the validator expects `match` to deserialize into a string pattern and matches it exactly against task id (`crates/rhei-validator/src/lib.rs:601`, `crates/rhei-validator/src/lib.rs:679`). They also require transition `description`, while `TransitionRule` does not represent or preserve that field (`docs/specs/rhei-state-machine-writer.spec.md:82`, `crates/rhei-core/src/ast.rs:255`, `crates/rhei-core/src/ast.rs:284`).

Classification: `no-discrepancy`

The template-writer skill accurately captures several current template and validator boundaries: exact template layout, root `settings.json` relocation, MiniJinja versus runtime single-brace variables, no authored `**Assignee:**` or result blocks, parent/ancestor prior safety, instantiate-before-validate, and rendered-state-machine validation rather than raw templated YAML validation (`skills/rhei-template-writer/SKILL.md:51`, `skills/rhei-template-writer/SKILL.md:80`, `skills/rhei-template-writer/SKILL.md:161`, `skills/rhei-template-writer/SKILL.md:218`, `skills/rhei-template-writer/SKILL.md:283`).

## TSC-006: Skill Installation Command and Agent Paths

Classification: `implementation-diverges`

`--link` is ignored for Cursor installs. The install-skills spec says the default copies and `--link` symlinks (`docs/specs/rhei-install-skills.spec.md:150`, `docs/specs/rhei-install-skills.spec.md:152`). Claude, Codex, and rules-directory agents branch on `link`, but `install_cursor` takes `_link` and always writes `.mdc` files with embedded content (`crates/rhei-cli/src/main.rs:10274`, `crates/rhei-cli/src/main.rs:10309`). Existing link-mode test coverage exercises Kilocode only (`crates/rhei-cli/tests/e2e/install_skills_tests.rs:99`, `crates/rhei-cli/tests/e2e/install_skills_tests.rs:112`).

Classification: `no-discrepancy`

The core `install-skills` command surface and most agent-specific paths match the spec. The CLI accepts `--agent`, `--local`, `--link`, `--uninstall`, `--dry-run`, and comma-delimited `--skills` with the documented default list (`crates/rhei-cli/src/main.rs:337`, `crates/rhei-cli/src/main.rs:361`). `all` expands to the eight concrete agents (`crates/rhei-cli/src/main.rs:10065`, `crates/rhei-cli/src/main.rs:10080`), local roots use `find_project_root` (`crates/rhei-cli/src/main.rs:10024`, `crates/rhei-cli/src/main.rs:10761`), source resolution checks installed and repo-dev skill directories (`crates/rhei-cli/src/main.rs:10718`, `crates/rhei-cli/src/main.rs:10758`), and e2e tests cover Claude, Cursor, Kilocode symlink, Codex, uninstall, dry-run, and reinstall refresh behavior (`crates/rhei-cli/tests/e2e/install_skills_tests.rs:40`, `crates/rhei-cli/tests/e2e/install_skills_tests.rs:206`).

## TSC-007: Completion Generation, Installation, and Dynamic Values

Classification: `implementation-diverges`

`rhei install-skills --skills` completion is not comma-aware and can suggest duplicate already-supplied skills. The completion spec requires replacement of only the segment after the last comma and avoidance of already supplied skills (`docs/specs/rhei-completions.spec.md:402`, `docs/specs/rhei-completions.spec.md:410`). The implementation uses a plain static prefix completer over the whole current value (`crates/rhei-cli/src/main.rs:837`, `crates/rhei-cli/src/main.rs:855`). A targeted dynamic completion for `--skills rhei-plan-worker,rhei-` produced candidates including `rhei-plan-worker,rhei-plan-worker`, duplicating the existing segment.

Classification: `implementation-diverges`

Template input completion ignores `--values` files when determining already-supplied inputs. The spec says completion should parse the command line to identify inputs supplied by `--values` and should follow instantiation precedence (`docs/specs/rhei-completions.spec.md:207`, `docs/specs/rhei-completions.spec.md:213`); it also says readable values files may be parsed and parse failures ignored for ranking (`docs/specs/rhei-completions.spec.md:320`, `docs/specs/rhei-completions.spec.md:325`). The completion parser records only `--set` and `--set-file` option values as input args and skips `--values` option values (`crates/rhei-cli/src/main.rs:1964`, `crates/rhei-cli/src/main.rs:1988`). A targeted completion with `--values values.yaml` containing `alpha: x` still suggested `alpha=` as an unsupplied assignment.

Classification: `missing-test`

The visible completion tests do not cover the full acceptance matrix required by the spec. The spec calls out coverage for path domains, `--set-file`, single-required-input fallback, already supplied keys, invalid manifests/unreadable values quiet degradation, transition `--from` and `--to`, and comma-separated skill segments (`docs/specs/rhei-completions.spec.md:430`, `docs/specs/rhei-completions.spec.md:450`). Current e2e coverage exercises shell generation/install, template names, basic assignment keys/boolean values, task ids/transition targets, list filters, and install paths (`crates/rhei-cli/tests/e2e/completions_tests.rs:152`, `crates/rhei-cli/tests/e2e/completions_tests.rs:657`), but not the comma-aware and values-file cases above.

Classification: `no-discrepancy`

Completion generation and installation generally match the output contract. The CLI supports bash, zsh, fish, powershell, and elvish (`crates/rhei-cli/src/main.rs:408`, `crates/rhei-cli/src/main.rs:420`), writes plain generated scripts to stdout when no install/output option is supplied (`crates/rhei-cli/src/main.rs:600`, `crates/rhei-cli/src/main.rs:624`), uses `COMPLETE` callbacks through `clap_complete` dynamic registration (`crates/rhei-cli/src/main.rs:651`, `crates/rhei-cli/src/main.rs:662`), computes documented user/system install paths (`crates/rhei-cli/src/main.rs:675`, `crates/rhei-cli/src/main.rs:700`), writes atomically through a temp file and persist (`crates/rhei-cli/src/main.rs:627`, `crates/rhei-cli/src/main.rs:647`), and has e2e coverage for supported shells and install paths (`crates/rhei-cli/tests/e2e/completions_tests.rs:152`, `crates/rhei-cli/tests/e2e/completions_tests.rs:657`).

## TSC-008: Usage Workflow Boundaries

Classification: `no-discrepancy`

The generated-workflow and assistant-facing surfaces mostly preserve the role boundary between manual workers and `rhei run` orchestration. The usage spec says manual workers use `next` / `transition` / `complete`, while spawned workers under `rhei run` leave state mutation to the orchestrator (`docs/specs/rhei-usage.spec.md:24`, `docs/specs/rhei-usage.spec.md:37`, `docs/specs/rhei-usage.spec.md:103`). The plan-worker skill documents the same distinction (`skills/rhei-plan-worker/SKILL.md:149`, `skills/rhei-plan-worker/SKILL.md:159`), and the spawned-agent prompt enforces the orchestrator boundary (`crates/rhei-cli/src/main.rs:6602`). Template task files inspected in `.agents/rhei/templates/spec-implementation-discrepancy-audit/tasks/` encode this boundary in their generated `Rhei Commands` prose through state-machine instructions rather than authored state/result mutations.

