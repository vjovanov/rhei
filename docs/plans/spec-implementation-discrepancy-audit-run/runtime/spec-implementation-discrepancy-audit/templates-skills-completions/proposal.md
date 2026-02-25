# Reconciliation Proposal: Templates, Skills, Completions, and Generated Workflows

Source elaboration: `runtime/spec-implementation-discrepancy-audit/templates-skills-completions/elaboration.md`

This proposal names a primary human decision option for each elaborated
discrepancy and records a credible alternative. Decision options use the audit
vocabulary: `update-spec`, `update-implementation`, `update-both`,
`defer-follow-up`, `no-change`.

## D-001: `rhei templates` hides invalid template directories without warning

- Primary decision: `update-implementation`.
- Next edits: change template discovery so `rhei templates` reports skipped
  invalid or unreadable template directories to stderr, including the skipped
  path and manifest/load error. Keep completion and other best-effort discovery
  call sites quiet by either returning skipped diagnostics to the caller or
  adding an explicit quiet mode.
- Expected tests: add an e2e test with an invalid project-local template
  directory. Assert status 0, valid templates still list, and stderr contains a
  warning naming the invalid path. Add a completion-path regression proving
  invalid templates do not leak diagnostics into shell completion output.
- Reason preferable: the spec already describes the right user experience:
  broad discovery should continue, but broken templates should not disappear
  silently.
- Alternative: `update-spec` to document silent skipping. This would preserve
  current behavior but makes template authoring failures hard to diagnose.

## D-002: Plain template listing omits required path and required-input count

- Primary decision: `update-implementation`.
- Next edits: update plain `rhei templates` output in
  `crates/rhei-cli/src/main.rs` to include the resolved template path and a
  numeric required-input count for each template. Keep the current input-name
  summary if useful, but do not let it replace the required count.
- Expected tests: update `crates/rhei-cli/tests/e2e/templates_tests.rs` so the
  plain listing asserts path visibility and required-input count, alongside the
  existing JSON assertions for `path` and `required_inputs`.
- Reason preferable: JSON already exposes the required data, and the plain
  human-facing output should not be a lower-fidelity discovery surface.
- Alternative: `update-spec` to define the current compact plain output and
  reserve path/count for JSON. This is credible for terminal readability, but
  it weakens the documented default command.

## D-003: Template `type: path` inputs are rewritten and validated instead of preserved

- Primary decision: `update-implementation`.
- Next edits: change `coerce_template_input_value` and the manifest default
  validation path so `type: path` resolves to the exact supplied/default string,
  accepts omitted optional paths as `""`, and does not require non-default
  values to exist. Limit cwd-relative path resolution to CLI file operations
  that actually need to read or write files, not template rendering.
- Expected tests: add template instantiation tests for a relative path rendered
  unchanged, a nonexistent future path accepted, an optional path with no
  default rendering as an empty string, and shipped-template defaults such as
  `hourly-human-intervention` remaining workspace-relative rather than absolute.
- Reason preferable: templates should render portable workflow text. Rewriting
  values to host-specific absolute paths makes generated plans less reusable
  and rejects legitimate future-path inputs.
- Alternative: `update-spec` to require path existence checks and absolute
  canonicalization. This would match current code but would turn `type: path`
  into an input-file contract rather than a templated path value.

## D-004: Bare `KEY=VALUE` handling rejects some positional literals

- Primary decision: `update-spec`.
- Next edits: clarify `docs/specs/rhei-templates.spec.md` and
  `docs/specs/rhei-completions.spec.md` that a bare argument with an
  identifier-like prefix before `=` is parsed as an assignment attempt and must
  name a declared input; only non-identifier prefixes remain positional
  literals. Add an example showing that `foo=bar` errors when `foo` is not a
  declared input, and that `./foo=bar` or another non-identifier/path-like
  prefix can be positional.
- Expected tests: add or retain a template CLI test asserting undeclared
  identifier-like `KEY=VALUE` exits with the undeclared-input diagnostic, plus
  a positional fallback test for a value containing `=` whose prefix is not a
  valid identifier.
- Reason preferable: the current implementation is safer for typo detection.
  Treating undeclared identifier-like assignments as positional values can hide
  misspelled input names, especially under the single-required-input fallback.
- Alternative: `update-implementation` to treat undeclared identifier-like
  `KEY=VALUE` as positional when positional mapping is otherwise unambiguous.
  This is friendlier for literal query strings, but it weakens script-safe
  assignment diagnostics.

## D-005: Template validation does not check state-machine MCP or skill references

- Primary decision: `update-implementation`.
- Next edits: extend `validate_machine_settings_references` so it validates
  state-level `mcp_servers` and `skills` against the merged settings registries
  after template `settings.json` relocation and merge. Surface dangling ids as
  `rhei validate` / post-instantiation validation errors.
- Expected tests: add template dry-run and direct validation tests where a
  bundled `states.yaml` references unknown MCP server and skill ids, then
  passes once the ids are declared in bundled or project settings. Keep tests
  for unknown agents/modes/targets.
- Reason preferable: missing tools are authoring errors in generated
  workflows. Reporting them during validation is cheaper and clearer than
  failing later during execution.
- Alternative: `update-spec` to make MCP and skill reference checks run-time
  only. This would simplify validation, but it would make `rhei instantiate
  --dry-run` a weaker preflight for template authors.

## D-006: Some shipped templates still use legacy state-machine shape

- Primary decision: `update-both`.
- Next edits: migrate `.agents/rhei/templates/changeset-review/states.yaml`,
  `.agents/rhei/templates/hourly-human-intervention/states.yaml`, and
  `.agents/rhei/templates/spec-review/states.yaml` to current
  `profiles`/`node_policy` machines without state-level `initial: true`.
  Update the state-machine specs and assistant-facing references to document
  legacy machines as compatibility input only, not the style new templates
  should emit.
- Expected tests: run `rhei validate` against every shipped template example
  and add a fixture or lint-style test that bundled template `states.yaml`
  files include `profiles` and `node_policy` and do not use state-level
  `initial: true`.
- Reason preferable: shipped templates are executable examples. They should
  teach the current schema while the loader keeps legacy acceptance as an
  explicit migration path.
- Alternative: `update-spec` to bless legacy state-level `initial` as equally
  current. This would reduce migration work but preserve two competing
  authoring styles.

## D-007: Shipped template accompaniment is incomplete for `spec-review` and `multi-model-analysis`

- Primary decision: `update-implementation`.
- Next edits: add a template-root `README.md` for
  `.agents/rhei/templates/spec-review/`, generate and check in
  `examples/spec-review-example/`, and generate and check in
  `examples/multi-model-analysis-example/`. Ensure the examples are produced
  from representative values files or commands documented in the template
  READMEs.
- Expected tests: add example validation coverage for the new directories and
  a template inventory test that every bundled template has a root `README.md`
  and an `examples/<template-name>-example/` directory that validates.
- Reason preferable: examples are the lowest-friction smoke tests for generated
  workflow validity, and the template-writer skill already tells agents to
  create them.
- Alternative: `update-spec` to downgrade README/example accompaniment to a
  recommendation. This is plausible for experimental templates, but it weakens
  reviewability for shipped workflows.

## D-008: Plan-writer default-state reference does not match the compiled default machine

- Primary decision: `update-both`.
- Next edits: reconcile `skills/rhei-plan-writer/references/default-states.md`
  with `crates/rhei-validator/src/default-states.yaml` and
  `docs/specs/states.yaml`. Prefer migrating the compiled default to the
  current profile-based machine and then updating the skill reference to
  describe that exact default. If that migration is deferred, edit the skill
  reference to label the current compiled v2 machine honestly.
- Expected tests: add a semantic comparison test between the compiled default
  state machine and the checked-in default spec fixture, and add a lightweight
  skill/docs consistency check or release checklist item for the default-state
  reference.
- Reason preferable: agents should not generate plans from a default workflow
  that differs from the one the CLI actually loads.
- Alternative: `update-implementation` limited to editing the skill reference
  down to the current compiled v2 behavior. This is a useful short-term patch,
  but it leaves the broader default-machine migration unresolved.

## D-009: Plan-worker skill overstates cancelled-prior readiness

- Primary decision: `update-both`.
- Next edits: update `skills/rhei-plan-worker/SKILL.md` and the relevant
  readiness prose in `docs/specs/rhei-usage.spec.md` / command specs so
  prerequisite satisfaction means terminal and non-cancelled. Remove examples
  that say default-machine `cancelled` priors unblock downstream work.
- Expected tests: retain the existing CLI coverage that cancelled
  prerequisites do not unblock dependents, and add a spec-derived readiness
  test name if needed so the non-cancelled rule is visible in CI.
- Reason preferable: a cancelled prerequisite usually means required work did
  not happen. The CLI's safer behavior should be the documented behavior agents
  follow.
- Alternative: `update-implementation` to treat `cancelled` as satisfying
  dependencies. This would match the skill's current terminal-prior wording,
  but it risks starting downstream tasks after skipped work.

## D-010: State-machine-writer guidance can generate schema the runtime does not support

- Primary decision: `update-implementation`.
- Next edits: implement the current writer contract in the runtime: replace
  string-only `NodePolicyOverride.match` with a structured matcher that can
  express `type` and `level`, keep an explicit legacy string-pattern parser if
  old fixtures need it, and add `description` to `TransitionRule` so transition
  descriptions are parsed, preserved, and required for current-schema machines.
- Expected tests: add validator tests for object-shaped override matches,
  invalid match keys, type/level resolution against real plan nodes, legacy
  string overrides if retained, missing transition descriptions, and
  transition-description round trips through `rhei states --json` or equivalent
  serialization.
- Reason preferable: the state-machine-writer skill is meant to generate
  current machines. Runtime support should accept the schema the writer is
  instructed to emit instead of forcing agents to know hidden validator limits.
- Alternative: `update-spec` to scale the writer contract back to string
  id/glob overrides and optional transition descriptions. This is smaller, but
  it removes useful profile-routing semantics and weakens generated-machine
  documentation.

## D-011: `install-skills --link` is ignored for Cursor

- Primary decision: `update-both`.
- Next edits: decide and document Cursor-specific link semantics in
  `docs/specs/rhei-install-skills.spec.md`, then make
  `install_cursor` enforce them. The pragmatic v1 edit is to reject
  `--agent cursor --link` with a clear diagnostic explaining that Cursor rules
  are generated `.mdc` snapshots, not live symlinks, unless maintainers choose
  to support symlinking raw `SKILL.md` content as valid Cursor input.
- Expected tests: add Cursor install tests for `--link`: either an error test
  with no `.mdc` written, or, if true link support is chosen, an assertion that
  the installed `.mdc` path is a symlink and source edits propagate. Keep the
  existing `.mdc` frontmatter tests for copy mode.
- Reason preferable: the current behavior silently ignores a user-selected
  development mode. If Cursor cannot faithfully link transformed `.mdc` files,
  the spec and implementation should say so explicitly instead of pretending.
- Alternative: `update-implementation` to make Cursor link mode symlink the
  source `SKILL.md` directly to a `.mdc` path. This preserves "live" updates,
  but may drop Cursor frontmatter/globs and change the installed rule format.

## D-012: `install-skills --skills` completion is not comma-aware

- Primary decision: `update-implementation`.
- Next edits: replace `complete_skill_name`'s whole-token static prefix logic
  with comma-segment parsing. Complete only the text after the last comma,
  prepend the already typed comma-separated prefix to each candidate, and omit
  skills already present in earlier segments.
- Expected tests: add dynamic completion tests in
  `crates/rhei-cli/tests/e2e/completions_tests.rs` for first-segment
  completion, second-segment replacement after a comma, duplicate suppression,
  and empty segment completion after a trailing comma.
- Reason preferable: comma-separated `--skills` is the documented command
  syntax, so completion should operate on list segments rather than the whole
  token.
- Alternative: `update-spec` to document only whole-token completion. This
  would match current code but makes the documented list option clumsy and
  duplicate-prone.

## D-013: Template input completion ignores `--values` files

- Primary decision: `update-implementation`.
- Next edits: teach the instantiate completion parser to retain readable
  `--values` file paths and parse them with the same top-level-key rules used
  by instantiation. Include parsed keys in the supplied-input set used by
  `complete_template_assignment_keys`, while degrading quietly when a values
  file is unreadable or invalid.
- Expected tests: add dynamic completion tests where `--values values.yaml`
  containing `alpha: x` suppresses `alpha=`, multiple values files merge in
  command order, `--set` / `--set-file` still override suppression, and invalid
  values files do not make completion fail noisily.
- Reason preferable: completion should reflect the same precedence model users
  get at instantiation time. Otherwise it keeps suggesting keys the user has
  already supplied.
- Alternative: `update-spec` to make `--values` parsing optional and explicitly
  allow completion to ignore values-file contents. This is simpler, but noisy
  for complex templates where values files are the normal input path.

## D-014: Completion acceptance-test coverage is incomplete

- Primary decision: `update-implementation`.
- Next edits: expand `crates/rhei-cli/tests/e2e/completions_tests.rs` into the
  acceptance matrix named by `docs/specs/rhei-completions.spec.md`: path domains,
  `--set-file`, single-required-input fallback, already supplied keys,
  invalid-manifest and unreadable-values quiet degradation, transition
  `--from`/`--to`, comma-separated `--skills`, and `--values` supplied-key
  behavior.
- Expected tests: the reconciliation is the tests themselves. The suite should
  include both positive candidates and negative assertions for suppressed
  duplicates/keys where completion is supposed to filter them out.
- Reason preferable: the concrete behavior gaps in D-012 and D-013 show that
  dynamic completion needs focused regression coverage.
- Alternative: `no-change` if maintainers have equivalent downstream or hidden
  completion tests. This leaves the public CI suite less useful for future
  contributors.
