# Reconciliation Proposal: Syntax, Authoring, Parsing, and Validation

Source elaboration: `runtime/spec-implementation-discrepancy-audit/syntax-authoring-validation/elaboration.md`

This proposal names a primary human decision option for each elaborated
discrepancy and records a credible alternative. Decision options use the audit
vocabulary: `update-spec`, `update-implementation`, `update-both`,
`defer-follow-up`, `no-change`.

## D-001: Non-structural preambles can be accepted and discarded

- Primary decision: `update-implementation`.
- Next edits: make the parser reject non-empty text that has no active content
  section or task receiver. For single-file plans, ordinary prose before the
  first H2 content section or `## Tasks` should produce a parse diagnostic
  instead of being dropped. For workspace task files, reject any Rhei H1,
  frontmatter-looking block, H2 section, or prose before the first task node,
  while continuing to allow leading blank lines.
- Expected tests: add parser and CLI validation tests for dropped single-file
  intro prose, workspace task-file prose preambles, workspace task-file Rhei
  headers, and workspace task-file frontmatter preambles. Keep fenced-code
  tests proving structural-looking text inside task content is not rejected.
- Reason preferable: silent content loss is the unsafe behavior. The current
  spec is strict and predictable, and implementation should preserve or reject
  authored text rather than accept and discard it.
- Alternative: `update-both` to allow a single unsectioned intro block in
  single-file plans and parse it as a content section. This is credible for
  author convenience, but it still leaves workspace task-file preambles
  invalid and would broaden the formal grammar for little gain.

## D-002: Workspace task discovery is not recursive

- Primary decision: `update-implementation`.
- Next edits: change workspace loading to walk `tasks/` recursively for `.md`
  files, skip non-files and hidden/runtime directories as appropriate, and sort
  discovered paths deterministically before parsing and merging. Preserve the
  existing global duplicate-id validation across all discovered files.
- Expected tests: add workspace integration tests with nested task files,
  duplicate ids split across nested and direct files, a nested dependency edge,
  and task-file rewrite behavior for a nested task source.
- Reason preferable: the spec and examples already teach nested task files.
  Recursive discovery prevents valid-looking workspaces from silently skipping
  tasks.
- Alternative: `update-spec` to restrict workspace task files to direct
  children of `tasks/`. This would simplify loading, but it would make the
  nested-link example stale and reduce the usefulness of workspaces for larger
  plans.

## D-003: `structure.nodeKinds` does not enforce the `IDENTIFIER` grammar

- Primary decision: `update-implementation`.
- Next edits: validate every `structure.nodeKinds` value with the same
  `IDENTIFIER` rule used by task-id segments: first character is a letter, and
  remaining characters are letters, digits, `-`, or `_`. Keep existing
  case-insensitive duplicate and reserved `rhei` checks.
- Expected tests: add parser/frontmatter tests rejecting node kinds that start
  with digits, contain whitespace, contain punctuation outside `-` and `_`, or
  are empty after trimming. Add positive coverage for mixed-case, hyphenated,
  and underscored identifiers.
- Reason preferable: invalid node-kind declarations cannot be matched by the
  documented heading grammar, so accepting them creates confusing configuration
  that can never work consistently.
- Alternative: `update-spec` to allow arbitrary non-empty strings as node
  kinds. This is not preferable because headings and prior references still
  need a compact unambiguous token.

## D-004: The authoring guide still forbids named task children

- Primary decision: `update-spec`.
- Next edits: update `docs/specs/rhei-authoring.spec.md` to remove the named
  task child prohibition and show that named and numeric path segments may be
  mixed when each child id extends its parent. Check the plan-writer skill text
  and examples for consistency, but no implementation change is required.
- Expected tests: no runtime test is required beyond existing named-child
  parser/integration coverage. A docs smoke check can be added if the project
  has one for examples embedded in specs.
- Reason preferable: the implementation, formal spec, and plan-writer guidance
  already agree that named children are valid. The stale guide text is the only
  conflicting surface.
- Alternative: `update-implementation` to reintroduce the prohibition. This
  would be a breaking reduction of the current grammar and would conflict with
  existing tests that document the prohibition's removal.

## D-005: Runtime command paths are root-`Task`-centric

- Primary decision: `update-implementation`.
- Next edits: introduce shared task-node lookup and rewrite helpers that work
  across all parsed nodes, not only top-level `Task` nodes. Use them in
  `next`, `transition`, `complete`, assignee insertion/removal, result-link
  insertion, dependency-state maps, workspace task-file selection, and direct
  `--task` lookup. Preserve the authored node kind when rewriting headings and
  metadata.
- Expected tests: add CLI integration tests for claiming, transitioning,
  completing, assigning, and dependency-unblocking child tasks. Add custom root
  kind tests such as `### Bug 1:` for direct lookup and result insertion. Add
  workspace variants where the target node lives in a task file and has a
  non-root id.
- Reason preferable: the language model already treats H3-H6 nodes of declared
  kinds as task nodes. Runtime commands should operate on the same graph that
  parsing and validation accept.
- Alternative: `update-both` to narrow the spec so only root `Task` nodes are
  mutable runtime targets and children/custom kinds are structural annotations.
  This would be simpler, but it would invalidate the generic node-kind design
  and make hierarchical execution much less useful.

## D-006: Duplicate `**State:**` metadata silently overwrites

- Primary decision: `update-implementation`.
- Next edits: add a duplicate-state guard in task metadata parsing matching
  the existing duplicate-assignee behavior. The parser should emit a diagnostic
  when a second valid-looking `**State:**` line appears before task content.
- Expected tests: add parser and CLI diagnostics tests for duplicate state
  lines, including a case where the two values differ. Verify the first value
  is not silently replaced in recovered parse output.
- Reason preferable: the grammar allows one state field, and visible markdown
  with two state lines is contradictory. Rejecting it avoids hidden state
  changes during validation and rendering.
- Alternative: `update-spec` to define "last `**State:**` wins". This would
  match the current parser but would be a surprising metadata rule and weaken
  authoring diagnostics.

## D-007: `**Prior:**` parsing and validation are incomplete

- Primary decision: `update-implementation`.
- Next edits: parse `**Prior:**` as a full-line grammar instead of extracting
  regex matches from arbitrary text. Store prior references as kind-qualified
  values in the AST, reject empty lists and malformed separators, reject
  duplicates, and validate that the authored kind matches the referenced node's
  kind before readiness or cycle checks run.
- Expected tests: add parser/validator tests for missing list items, comma
  without following space, semicolon or free-text separators, empty
  `**Prior:**`, duplicate references, kind mismatch, custom-kind dependencies,
  and successful mixed named/numeric dependencies.
- Reason preferable: dependencies are central to scheduling. Full-line parsing
  plus kind-aware validation makes authored prerequisites auditable and keeps
  custom node kinds from collapsing into id-only references.
- Alternative: `update-both` to simplify the spec so `**Prior:**` is an
  id-only free-form extraction field. This would preserve current AST shape,
  but it would make malformed dependency lines valid and remove useful
  diagnostics.

## D-008: `--state-machine` bypasses declared-name validation

- Primary decision: `update-implementation`.
- Next edits: after loading an explicit `--state-machine <path>`, compare the
  loaded machine `name` with the plan's `**States:**` declaration using the
  same rule as automatic discovery. Emit a clear mismatch diagnostic that names
  the plan declaration, loaded machine name, and override path.
- Expected tests: add CLI integration tests where an explicit override path
  has a matching name, a mismatched name, and a missing plan `**States:**`
  declaration that falls back to the default behavior.
- Reason preferable: `**States:**` is the plan's auditable workflow contract.
  An explicit path should choose the file location, not silently change the
  declared workflow identity.
- Alternative: `update-spec` to say `--state-machine` intentionally overrides
  both lookup and name matching. This is useful for emergency debugging, but it
  makes scripted validation less trustworthy.

## D-009: Markdown state rendering rules are not enforced

- Primary decision: `update-implementation`.
- Next edits: implement state-value parsing according to the documented
  rendering rules. Bare state metadata must be a valid `IDENTIFIER`; states
  with spaces or punctuation must be backticked; quoted values must unescape
  backslash and backtick consistently; malformed quoted values should produce
  parse diagnostics before semantic state lookup.
- Expected tests: add parser and validator tests for bare identifier states,
  rejected bare multi-word states, valid backticked states with spaces,
  escaped backticks/backslashes, unterminated backticks, and round-trip
  rendering through `rhei render`.
- Reason preferable: the spec provides a portable markdown encoding for custom
  state names. Enforcing it keeps plans interoperable and makes escaped names
  round-trip instead of depending on raw parser behavior.
- Alternative: `update-spec` to allow any non-empty raw state string and treat
  backticks as optional decoration. This is easier for the parser, but it
  removes the canonical encoding needed for reliable markdown authoring.

## D-010: Cancelled dependencies do not satisfy readiness

- Primary decision: `update-spec`.
- Next edits: update `docs/rhei.spec.md` and related authoring/usage prose so
  dependency readiness means every prior is terminal and non-cancelled, unless
  a future state-machine policy explicitly opts into cancellation satisfying a
  dependency. Define the built-in `cancelled` state as terminal for closure but
  not successful prerequisite satisfaction.
- Expected tests: retain existing implementation tests asserting cancelled
  prerequisites do not unblock dependents. Add or rename a spec-derived test so
  the non-cancelled rule is visible in the readiness suite.
- Reason preferable: a cancelled prerequisite normally means required work did
  not happen. The current implementation and related command proposals already
  follow the safer scheduling rule; the core spec should state that rule
  directly.
- Alternative: `update-implementation` to treat all terminal priors, including
  `cancelled`, as ready. This matches the current wording literally, but it can
  start downstream work after an unmet prerequisite.

## D-011: Workspace link validation uses the wrong root and does not block escapes

- Primary decision: `update-implementation`.
- Next edits: pass the workspace root, meaning the directory containing
  `index.rhei.md`, into link validation for workspace inputs whether the CLI
  argument is the workspace directory or the index file. Normalize relative
  link targets, reject paths that escape the workspace root with `..`, and only
  then check existence. Keep external URLs and same-document anchors excluded
  from filesystem validation.
- Expected tests: add workspace validation tests for a valid link relative to
  the workspace root, validation invoked via workspace directory and via
  `index.rhei.md`, missing workspace-relative files, `../` escape links that
  point to existing outside files, fragments, and external URLs.
- Reason preferable: workspace-relative links are part of the integrity
  contract. The validator should neither reject valid in-workspace links due to
  the wrong base nor allow outside files to satisfy workspace metadata.
- Alternative: `update-spec` to define workspace link roots as the parent of
  the workspace directory when validating a directory path. This would bless an
  unintuitive CLI artifact and weaken workspace containment.

## D-012: Authored result blocks are not parsed or validated

- Primary decision: `update-implementation`.
- Next edits: add result-block metadata to the AST, parse
  `> **Result:** [<task-id>](runtime/results/<task-id>.md)` before ordinary
  task content, and validate that both link text and target match the enclosing
  task id. Update render/reset/complete helpers to treat parsed result blocks
  as runtime-owned metadata rather than body prose.
- Expected tests: add parser and validator tests for valid result blocks,
  mismatched link text, mismatched target path, duplicate result blocks,
  result blocks after task body content if metadata ordering forbids them, and
  result insertion before child headings. Keep existing completion/reset tests.
- Reason preferable: completed plans already contain result blocks authored by
  the runtime. Parsing and validating them closes the gap between source
  markdown and the AST and prevents malformed completion metadata from looking
  like ordinary prose.
- Alternative: `update-spec` to remove authored result blocks from the grammar
  and describe them as best-effort runtime prose. This would avoid AST work,
  but it would abandon the result consistency constraint the spec already
  promises.

## D-013: Workspace validation still reports only the first parse error

- Primary decision: `update-implementation`.
- Next edits: add a workspace parse-collection path that aggregates
  recoverable parse diagnostics from `index.rhei.md` and every discovered task
  file before semantic validation. Include file-relative paths and line
  numbers in each diagnostic, and keep fatal workspace-shape errors such as a
  missing `tasks/` directory as immediate failures.
- Expected tests: add workspace CLI diagnostics tests with multiple malformed
  task files, malformed index plus malformed task file, and a mix of parse and
  semantic errors. Assert all recoverable parse diagnostics are reported with
  their source file paths.
- Reason preferable: single-file validation already gives authors an
  aggregated repair loop. Workspaces need the same diagnostic quality because
  generated plans commonly contain errors in more than one task file.
- Alternative: `defer-follow-up` to keep first-error workspace loading and
  document it as a known limitation. This is credible if parser recovery across
  task files is too large for the current milestone, but it leaves a clear
  authoring-experience gap.

## D-014: Dependency alias guidance is ambiguous for JSON surfaces

- Primary decision: `update-spec`.
- Next edits: update `docs/specs/rhei-authoring.spec.md` to distinguish the
  stable JSON surfaces explicitly. State that callback context exposes
  `task.metadata.dependsOn` and SDKs may expose idiomatic aliases such as
  `depends_on`, while `rhei render --format json` currently exposes parsed
  markdown dependencies as `prior` unless a deliberate renderer migration is
  scheduled. Remove or narrow the phrase that includes "CLI JSON" in the SDK
  alias statement.
- Expected tests: no behavior test is required for a documentation-only
  clarification. If render JSON snapshots exist, keep them unchanged and add a
  docs/example check only if the repository validates embedded JSON examples.
- Reason preferable: the implementation exposes two different JSON surfaces
  for different consumers. Clarifying that boundary avoids a breaking render
  JSON change and prevents users from assuming callback names apply everywhere.
- Alternative: `update-both` to add `metadata.dependsOn` or `depends_on`
  aliases to `rhei render --format json` while documenting a compatibility
  period for `prior`. This may be worthwhile later, but it should be treated as
  an API migration rather than a small authoring-spec cleanup.
