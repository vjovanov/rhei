# Discrepancy Elaboration: Templates, Skills, Completions, and Generated Workflows

Source findings: `runtime/spec-implementation-discrepancy-audit/templates-skills-completions/discrepancies.md`

This elaboration groups duplicate findings, marks weak evidence as tentative,
and records no-discrepancy areas. It does not choose a reconciliation strategy.

## Duplicate Merges

- TSC-002's `type: path` coercion mismatch and TSC-004's
  `hourly-human-intervention` absolute-path output are the same root behavior.
  The shipped-template case is recorded as concrete impact under D-003, not as
  a separate reconciliation target.
- TSC-007's missing completion tests are test-coverage companions to the
  comma-aware `--skills` and `--values` completion gaps, plus broader acceptance
  matrix coverage. They are elaborated separately as a tentative test finding
  because they do not by themselves prove runtime behavior.
- TSC-005's state-machine-writer finding contains two related assistant-facing
  schema mismatches: unsupported object-shaped `node_policy.overrides[].match`
  and transition `description` not being represented by runtime transition
  rules. They are grouped under D-010 because both affect generated custom state
  machines.

## Elaborated Discrepancies

### D-001: `rhei templates` hides invalid template directories without warning

Source findings: TSC-001. Classification: `implementation-diverges`.

- Exact mismatch: the template spec says `rhei templates` skips invalid or
  unreadable template directories and prints a warning naming the skipped path.
  The implementation attempts to load each manifest, but on load failure it
  simply `continue`s without emitting a diagnostic.
- Why it matters: a broken project-local or user-global template can disappear
  from discovery with no signal. Users may conclude no template exists, while
  authors lose the path and error needed to fix the manifest.
- Affected: template authors, users running `rhei templates`, and agents trying
  to discover reusable workflows from project/user template roots.
- Risk: user-facing.
- Current verification: the mismatch is visible in
  `crates/rhei-cli/src/main.rs` around `discover_templates`, where manifest-load
  errors are ignored. The source audit also records a targeted CLI check where
  an invalid project template returned status 0, printed "No templates found",
  and produced no stderr warning. No visible test asserts the required warning.

### D-002: Plain template listing omits required path and required-input count

Source findings: TSC-001. Classification: `implementation-diverges`.

- Exact mismatch: the spec says `rhei templates` prints name, version,
  description, source path, and required input count. JSON output includes
  `path` and `required_inputs`, but plain output prints name, version, source
  label, description, and an input-name summary; it does not print the template
  path or a numeric required-input count.
- Why it matters: humans using the default output cannot see where a template
  was resolved from or how many inputs are mandatory. This also makes plain and
  JSON output disagree on the advertised information surface.
- Affected: CLI users, documentation examples, and simple text-based wrappers
  around `rhei templates`.
- Risk: user-facing.
- Current verification: implementation code confirms the JSON/plain split.
  Existing e2e coverage currently asserts that the plain output does not contain
  the template path, so the test suite protects the implementation behavior
  rather than the spec behavior.

### D-003: Template `type: path` inputs are rewritten and validated instead of preserved

Source findings: TSC-002 and TSC-004. Classification:
`implementation-diverges`.

- Exact mismatch: the template spec says `type: path` values render exactly as
  supplied or defaulted, with relative paths interpreted against `cwd` only when
  the CLI itself needs to perform file operations; omitted optional paths with
  no default resolve to `""`. The implementation rejects empty path strings,
  joins relative paths against `cwd`, checks existence for non-default values,
  and stores the resolved absolute path string for rendering.
- Why it matters: generated plans become host-specific, template defaults that
  were intentionally workspace-relative become absolute, and users cannot pass
  a future path or a literal path value that does not exist yet. Optional path
  inputs without defaults also cannot resolve to the spec's empty string.
- Affected: all templates with `type: path` inputs, especially shipped
  workflows that expect workspace-relative paths such as
  `hourly-human-intervention`, and users instantiating templates from different
  machines or directories.
- Risk: user-facing.
- Current verification: the implementation behavior is visible in path coercion
  in `crates/rhei-cli/src/main.rs`. The source audit records targeted checks:
  `spec-review docs/rhei.spec.md` rendered an absolute repository path,
  `spec-review does/not/exist` failed before rendering, and
  `hourly-human-intervention` rendered defaults like `master` and `graalvm/ce`
  as absolute `/home/...` paths.

### D-004: Bare `KEY=VALUE` handling rejects some positional literals

Source findings: TSC-002. Classification: `implementation-diverges`,
tentative.

- Exact mismatch: under the source audit's reading of the template spec,
  `KEY=VALUE` should be treated as an assignment only when `KEY` names a
  declared input; otherwise the value should remain eligible as a positional
  literal. The implementation treats any syntactically identifier-like prefix
  as an assignment attempt and errors if the key is undeclared.
- Why it matters: single-required-input templates cannot accept values such as
  `foo=bar` positionally when `foo` is not an input name. That is surprising for
  inputs that are paths, filters, labels, or other domain strings where `=`
  commonly appears.
- Affected: template users passing positional values containing `=`, and
  template authors relying on the single-required-input fallback.
- Risk: user-facing.
- Current verification: the implementation parses input args this way in
  `parse_template_input_args`. The source audit records a targeted check where
  `rhei instantiate spec-review foo=bar` failed with an undeclared-input error
  instead of applying the single-required-input fallback.
- Tentative note: the spec wording around "valid input identifier" is somewhat
  ambiguous when read without the audit interpretation, so this finding is
  weaker than the path coercion finding and should be reconciled with a wording
  pass.

### D-005: Template validation does not check state-machine MCP or skill references

Source findings: TSC-003. Classification: `missing-validation`.

- Exact mismatch: the template settings spec says MCP server and skill ids
  referenced by a template `states.yaml` must resolve through bundled or global
  settings, and `rhei validate` should surface remaining dangling references as
  errors. The validation hook calls `validate_machine_settings_references`, but
  that helper checks only agent/profile selectors and targets. It does not check
  state-level `mcp_servers` or `skills` against the merged registries.
- Why it matters: a template can validate successfully while referencing
  missing tools that will only fail later during execution or availability
  checks. That moves a template-authoring error from instantiation/validation
  time into runtime.
- Affected: template authors bundling `settings.json`, users instantiating
  workflows with MCP/skill requirements, and agents relying on validation as a
  preflight check.
- Risk: user-facing at execution time; internal for validation completeness.
- Current verification: code inspection confirms the validation function only
  checks `agent`, `agent_mode`, and `target` selectors. Existing template tests
  cover settings relocation and malformed rendered settings, but the source
  audit did not identify tests for dangling MCP or skill ids in `states.yaml`.

### D-006: Some shipped templates still use legacy state-machine shape

Source findings: TSC-004. Classification: `implementation-diverges`.

- Exact mismatch: the scoped claims say bundled custom `states.yaml` files
  should be complete current machines with `profiles` and `node_policy`.
  `changeset-review`, `hourly-human-intervention`, and `spec-review` declare
  state-level `initial: true` and omit `profiles` and `node_policy`. They
  validate because the loader still accepts legacy machines without those
  blocks.
- Why it matters: shipped templates teach the legacy authoring style while
  assistant-facing and spec-facing guidance points authors toward current
  profile/node-policy machines. Users copying these templates may keep producing
  legacy machines, and generated workflows may not exercise newer validator and
  runtime paths.
- Affected: template users, template authors, state-machine-writer users, and
  maintainers auditing compatibility behavior.
- Risk: mixed. It is user-facing as an example/authoring-contract issue and
  internal as schema migration debt.
- Current verification: filesystem/code inspection confirms the legacy fields in
  the three templates and confirms the validator accepts missing `profiles` and
  `node_policy` as legacy compatibility. Validation was also re-run for the
  checked-in examples named by the source audit, and they currently pass.

### D-007: Shipped template accompaniment is incomplete for `spec-review` and `multi-model-analysis`

Source findings: TSC-004. Classification: `implementation-diverges`.

- Exact mismatch: the template-writer skill says every template should ship a
  root `README.md` and a pre-rendered example under
  `examples/<template-name>-example/` that passes `rhei validate`.
  `spec-review` has no template-root `README.md` and no
  `examples/spec-review-example/`. `multi-model-analysis` has a template
  `README.md` but no `examples/multi-model-analysis-example/`.
- Why it matters: users and reviewers cannot inspect a known-good rendered
  workspace for those templates, and assistant-facing authoring guidance is not
  reflected by all bundled templates. This weakens the smoke-test story for
  generated workflow shape.
- Affected: template users, reviewers, maintainers of shipped workflow
  templates, and agents following `rhei-template-writer`.
- Risk: user-facing for discoverability and confidence; internal for release
  verification.
- Current verification: filesystem inspection confirms the missing README and
  example directories. Existing examples that are checked in for other
  templates validate successfully, but no checked-in smoke example exists for
  these two cases.

### D-008: Plan-writer default-state reference does not match the compiled default machine

Source findings: TSC-005. Classification: `implementation-diverges`.

- Exact mismatch: `skills/rhei-plan-writer/references/default-states.md`
  describes a built-in default machine version `3.0` with `profiles` and
  `node_policy`. The compiled default states YAML used by the validator is
  version `2.0`, uses state-level `initial: true`, and has no profile or
  node-policy blocks.
- Why it matters: assistant-generated plans or explanations can rely on a
  default state shape that is not the one the CLI actually uses. That creates
  conflicting guidance for initial-state resolution and node policy.
- Affected: plan-writing agents, users reading skill references, and
  maintainers comparing assistant guidance to runtime defaults.
- Risk: mostly internal/documentation, with user-facing impact when agents
  author plans from the stale reference.
- Current verification: direct file inspection confirms the version/schema
  mismatch between the skill reference and
  `crates/rhei-validator/src/default-states.yaml`. The source audit did not
  identify a test that compares assistant references to the compiled default.

### D-009: Plan-worker skill overstates cancelled-prior readiness

Source findings: TSC-005. Classification: `implementation-diverges`.

- Exact mismatch: the plan-worker skill says a task is claimable when every
  prior is terminal, and explicitly names `completed` or `cancelled` in the
  default machine. The CLI dependency helper treats `cancelled` as not
  satisfying dependencies, even though it is terminal.
- Why it matters: agents following the skill may expect work downstream of a
  cancelled task to become claimable, but `rhei next` and readiness checks will
  keep it blocked. That can lead to incorrect status reports or attempts to work
  around the CLI.
- Affected: manual workers, orchestrated agents reading the installed skill,
  users cancelling prerequisite tasks, and downstream task owners.
- Risk: user-facing workflow behavior and assistant guidance mismatch.
- Current verification: direct code inspection confirms `dependency_is_satisfied`
  excludes normalized state `cancelled`, while the skill text says terminal
  priors satisfy selection. The broader manual-command audit also found this
  behavior covered by CLI tests in that partition.

### D-010: State-machine-writer guidance can generate schema the runtime does not support

Source findings: TSC-005. Classification: `implementation-diverges`, with one
tentative subfinding.

- Exact mismatch: the state-machine-writer spec and skill show
  `node_policy.overrides[].match` as an object like `{ type: <kind>, level: <n>
  }`. The validator deserializes `match` into a string pattern and currently
  matches it exactly against task id. Separately, the writer spec marks
  transition `description` as required, but the runtime `TransitionRule` type
  does not represent or preserve a `description` field.
- Why it matters: an assistant following the writer guidance can produce a
  machine that fails to load because `match` is object-shaped. For transition
  descriptions, the runtime accepts or ignores the authoring metadata rather
  than enforcing the stated requirement, so generated documentation and runtime
  schema drift.
- Affected: state-machine authors, `rhei-state-machine-writer` users, template
  authors bundling custom machines, and validator/runtime maintainers.
- Risk: user-facing when generated machines fail validation; internal for
  schema/documentation consistency.
- Current verification: code inspection confirms `NodePolicyOverride.pattern` is
  a string and profile resolution compares it directly to task id. Code
  inspection also confirms `TransitionRule` lacks a `description` field.
- Tentative note: the object-shaped `match` mismatch is strong. The transition
  `description` part is weaker because unknown YAML fields may be ignored rather
  than causing load failure; the concrete risk is missing enforcement and lost
  metadata, not necessarily an immediate validation error.

### D-011: `install-skills --link` is ignored for Cursor

Source findings: TSC-006. Classification: `implementation-diverges`.

- Exact mismatch: the install-skills spec says default behavior copies files
  and `--link` symlinks them. Cursor installation accepts a `link` parameter but
  names it `_link` and always writes generated `.mdc` files with embedded skill
  content.
- Why it matters: users choosing link mode for development expect installed
  Cursor rules to stay connected to the source skill files. Instead they get a
  copied/transformed snapshot and later source edits will not propagate.
- Affected: Cursor users, local development installs, and maintainers testing
  installed skill updates across agents.
- Risk: user-facing for Cursor install behavior.
- Current verification: code inspection confirms Cursor ignores the link flag.
  Existing link-mode test coverage exercises Kilocode, while Cursor tests only
  verify `.mdc` creation and frontmatter format.

### D-012: `install-skills --skills` completion is not comma-aware

Source findings: TSC-007. Classification: `implementation-diverges`.

- Exact mismatch: the completion spec says `--skills` completion should replace
  only the segment after the last comma and avoid suggesting skills already
  present in the comma-separated list. The implementation uses a plain static
  prefix completer over the entire current token, so it cannot reason about
  comma-separated segments or duplicates.
- Why it matters: shell completion can insert malformed or duplicate skill
  lists, which undermines the discoverability goal for agent-specific skill
  installation.
- Affected: users installing subsets of skills, especially in shells using the
  dynamic completion callback.
- Risk: user-facing command-line UX.
- Current verification: code inspection confirms `complete_skill_name` delegates
  to static prefix completion. The source audit records a dynamic completion
  check where completing after `rhei-plan-worker,rhei-` produced a duplicate
  `rhei-plan-worker` candidate.

### D-013: Template input completion ignores `--values` files

Source findings: TSC-007. Classification: `implementation-diverges`.

- Exact mismatch: the completion spec says template input completion should
  consider values supplied by `--values` files using instantiate precedence, and
  may parse readable YAML/JSON files to suppress already supplied keys. The
  completion parser records `--set` and `--set-file` values as supplied input
  args but does not add `--values` contents to the supplied-key set.
- Why it matters: completion can keep suggesting keys the user already supplied
  in a values file. That produces noisy guidance and can encourage accidental
  overrides.
- Affected: template users who provide non-scalar or multi-input data through
  values files, and authors relying on values-file workflows for complex
  templates.
- Risk: user-facing command-line UX.
- Current verification: code inspection confirms `completion_template_and_inputs`
  tracks `--values` as an option expecting a value but does not parse that file
  into input args. The source audit records a targeted completion check where a
  values file containing `alpha: x` did not suppress `alpha=`.

### D-014: Completion acceptance-test coverage is incomplete

Source findings: TSC-007. Classification: `missing-test`, tentative.

- Exact mismatch: the completion spec calls for coverage across path domains,
  `--set-file`, single-required-input fallback, already supplied keys, quiet
  degradation for invalid manifests/unreadable values, transition `--from` and
  `--to`, and comma-separated skill segments. The visible e2e tests cover shell
  generation/install, template names, basic assignment keys and boolean values,
  task ids, transition targets, list filters, and install paths, but not the
  comma-aware or `--values` cases above and not the full stated matrix.
- Why it matters: gaps in completion coverage let command-line UX regress
  without failing CI. This is especially relevant because completion behavior is
  dynamic and context-dependent.
- Affected: completion maintainers and users relying on shell discovery for
  templates, tasks, states, and skill installation.
- Risk: internal test risk with user-facing consequences.
- Current verification: the source audit compared the spec's acceptance list to
  `crates/rhei-cli/tests/e2e/completions_tests.rs`. The concrete behavioral
  gaps in D-012 and D-013 also have targeted CLI checks recorded in the source
  discrepancies.
- Tentative note: this is marked tentative because it is a visible-test
  inventory, not a proof that no hidden or downstream tests cover the omitted
  cases.

## No-Discrepancy Areas Recorded

- Template discovery order, duplicate hiding, direct path references, manifest
  loading, and one-entrypoint layout detection match the scoped claims. Project
  roots are searched before user roots, duplicate template directory names are
  hidden, path-like references bypass named discovery, and layout validation
  enforces exactly one plan entrypoint.
- Top-level manifest validation is broadly implemented: manifest name matching,
  non-empty names/descriptions, unique input names, `required`/`default`
  conflict handling, contiguous positive positional indexes, scalar-only
  `validate`, full-match Rust regex compilation, array `items`, and illegal
  `items`/`properties` combinations. The path-specific behavior in D-003 is the
  exception recorded for this area.
- Core template materialization matches the main contract: output conflicts are
  rejected outside dry-run, dry-run uses scratch space, `template.yaml` and
  hidden files are excluded, root `settings.json` relocates to
  `.rhei/settings.json`, rendered settings are JSON-validated, text/binary
  detection uses null bytes in the first 8 KiB, UTF-8 text renders with strict
  MiniJinja, `|slug` is registered, and legacy `\{{` escapes are consumed.
- Post-instantiation flow mostly matches: instantiated output is validated,
  root `states.yaml` is passed into validation and `--execute`, validation
  failures remove output unless `--keep-on-error`, summary sections are printed,
  and a shell-quoted reproducible command is emitted. D-005 is the missing
  settings-reference validation within this otherwise matching flow.
- Checked-in examples that exist for the audited templates currently validate:
  `examples/changeset-review-example`,
  `examples/hourly-human-intervention-example`,
  `examples/spec-implementation-discrepancy-audit-example`,
  `examples/review-fix-visits`, and `examples/ci-heal`.
- Generated task metadata ordering was not found discrepant in the inspected
  template task files: `**State:**` appears first and `**Prior:**` second where
  present. Parent/ancestor prior safety is also reflected in the
  template-writer skill and enforced by validator code/tests that reject a child
  listing its parent or ancestor as `**Prior:**`.
- The template-writer skill accurately captures several current boundaries:
  exact template layout, root `settings.json` relocation, MiniJinja versus
  runtime single-brace variables, no authored `**Assignee:**` or result blocks,
  parent/ancestor prior safety, instantiate-before-validate, and validation of
  rendered rather than raw templated state machines. D-007 records the bundled
  template accompaniment drift from that skill.
- The core `install-skills` command surface and most agent-specific paths match
  the spec: flags/options are present, `all` expands to the concrete agents,
  local roots use project-root discovery, source resolution checks installed and
  repo-dev skill directories, and e2e tests cover Claude, Cursor, Kilocode
  symlink, Codex, uninstall, dry-run, and reinstall refresh behavior. D-011 is
  the Cursor-specific link-mode exception.
- Completion generation and installation generally match the output contract:
  supported shells are present, stdout generation is script-only without install
  options, generated completions call back through `COMPLETE`, documented
  install paths are computed, writes use a temp file/persist path, and e2e tests
  cover supported shells and install paths. D-012 through D-014 are scoped to
  dynamic completion behavior and coverage gaps.
- The generated-workflow and assistant-facing surfaces mostly preserve the
  manual-worker versus `rhei run` orchestration boundary. The usage spec, the
  plan-worker skill, and the spawned-agent prompt all distinguish manual
  `next`/`transition`/`complete` ownership from orchestrator-owned state
  mutation under `rhei run`.
