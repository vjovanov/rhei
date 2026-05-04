# Discrepancy Elaboration: Syntax, Authoring, Parsing, and Validation

Source findings: `runtime/spec-implementation-discrepancy-audit/syntax-authoring-validation/discrepancies.md`

This elaboration groups duplicate or tightly coupled findings, marks weak
evidence as tentative, and records no-discrepancy areas. It does not choose a
reconciliation strategy.

## Duplicate And Related Merges

- SY-001-B and SY-001-D share the same parser behavior: structural or prose
  lines with no active content/task receiver are accepted and dropped. They are
  elaborated together, with separate single-file and workspace effects.
- SY-002-E and SY-005-E are both root-task-only runtime limitations despite
  the language treating child nodes and custom node kinds as full task nodes.
- SY-003-C, SY-005-B, and SY-005-C are the same dependency-contract surface:
  `**Prior:**` is parsed too loosely and stored without enough information for
  all required validation.
- SY-007-B, SY-007-C, and SY-007-D are one workspace-link validation gap plus
  its missing-test companion.
- SY-008-B and SY-008-C are one result-block validation gap plus its
  missing-test companion.

## Elaborated Discrepancies

### D-001: Non-structural preambles can be accepted and discarded

Source findings: SY-001-B, SY-001-D. Classification:
`implementation-diverges` and `missing-validation`.

- Exact mismatch: the formal grammar allows pre-task prose only inside H2
  content sections, and workspace task files must start directly with task
  definitions. The parser accepts non-empty prose before the first single-file
  H2 and accepts workspace task-file preambles because task files are parsed by
  prepending a synthetic Rhei header and `## Tasks`. If no content section or
  open task exists, those lines are ignored rather than rejected.
- Why it matters: authors can put meaningful context, frontmatter-looking
  blocks, or even a misleading `# Rhei:` line where the spec says they are
  invalid, and validation can still pass while silently losing that text from
  the AST and render outputs.
- Affected: plan authors, plan-writer skills, workspace templates, render/list
  consumers, and any tool that assumes parsed content is a faithful projection
  of the markdown file.
- Risk: user-facing, because authored content can disappear from tool output
  without a diagnostic.
- Current verification: normal grammar parsing and code-fence handling have
  parser and lexer tests. The single-file discard behavior is directly covered
  by a parser unit test that expects no content sections after an intro line.
  No visible workspace test rejects task-file preambles.

### D-002: Workspace task discovery is not recursive

Source finding: SY-001-E. Classification: `implementation-diverges`.

- Exact mismatch: the spec describes arbitrary `.md` task files within
  `tasks/` and gives a nested task-file link example. The loader uses a single
  `read_dir` over `tasks/` and filters only direct `.md` children, so nested
  task files are not discovered or merged.
- Why it matters: workspace authors can organize task files in subdirectories
  that appear valid from the spec but are invisible to validation, rendering,
  dependency checks, and runtime execution.
- Affected: larger workspaces, generated templates that might group task files,
  dependency validation across task files, and commands such as `validate`,
  `render`, `next`, `run`, and `complete`.
- Risk: user-facing, because tasks may be skipped entirely.
- Current verification: workspace tests cover direct task files, duplicate ids,
  empty task directories, task-file rewrites, and workspace state-machine
  lookup. The discrepancy file found no visible nested-task-file discovery
  coverage.

### D-003: `structure.nodeKinds` does not enforce the `IDENTIFIER` grammar

Source finding: SY-002-B. Classification: `missing-validation`.

- Exact mismatch: the spec requires each `structure.nodeKinds` entry to be a
  unique `IDENTIFIER`, meaning it starts with a letter and then contains only
  letters, digits, `-`, or `_`. The parser validates only type, trim-non-empty,
  uniqueness, and the reserved `rhei` name.
- Why it matters: invalid node-kind declarations can be accepted even though
  no matching task heading could be parsed for some values, creating confusing
  frontmatter that looks configured but cannot work consistently.
- Affected: plan authors, template authors, state-machine node policy authors,
  and diagnostics around unknown node kinds.
- Risk: user-facing authoring/validation risk.
- Current verification: parser tests cover custom node kinds, max depth, and
  reserved/duplicate-style behavior. The discrepancy file found no validation
  that rejects node-kind strings outside the `IDENTIFIER` grammar.

### D-004: The authoring guide still forbids named task children

Source finding: SY-002-D. Classification: `spec-stale`.

- Exact mismatch: the authoring guide says named task ids are conceptual
  anchors and must not declare children. The formal spec allows mixed numeric
  and named path segments, the plan-writer skill allows named child paths, and
  integration-test comments record that the prohibition was removed.
- Why it matters: users following the authoring guide can avoid valid task
  structures or report valid plans as incorrect. It also gives agents
  conflicting instructions when decomposing named work.
- Affected: human authors, plan-writing skills, examples/docs readers, and
  reviewers checking conformance.
- Risk: user-facing documentation risk.
- Current verification: parser and integration coverage exercise named ids and
  child path behavior under the current formal grammar. The stale guide text
  remains as contradictory documentation.

### D-005: Runtime command paths are root-`Task`-centric

Source findings: SY-002-E, SY-005-E. Classification:
`implementation-diverges`.

- Exact mismatch: the language defines every H3-H6 node of any declared kind
  as a task node. Several mutating and selection paths still search only root
  `### Task <id>:` headings or top-level `rhei.tasks`. This affects state and
  assignee rewrites, completion result insertion, direct task lookup, ready
  task scanning, and dependency-state maps.
- Why it matters: child tasks and custom root kinds can parse and validate but
  then be unclaimable, unrewritable, or not found by commands. Dependency
  readiness for child nodes is also incomplete because the runtime readiness
  map is built only from root tasks.
- Affected: hierarchical plans, custom node kinds such as `Bug`, `rhei next`,
  `rhei run`, `rhei complete`, transition/assignee rewrites, and workspace
  task-file updates.
- Risk: user-facing, because runtime behavior diverges from what validated
  plans are allowed to express.
- Current verification: parsing and validator coverage treats children as task
  nodes, and completion tests cover some child-terminal behavior. Existing
  runtime command tests primarily exercise root `Task` ids, so they verify the
  current root-centric paths rather than the full node model.

### D-006: Duplicate `**State:**` metadata silently overwrites

Source finding: SY-003-B. Classification: `missing-validation`.

- Exact mismatch: the grammar permits exactly one `state_field`. The parser
  guards duplicate `**Assignee:**` lines, but each valid-looking `**State:**`
  line before content simply assigns `top.state = Some(...)`, so the last one
  wins.
- Why it matters: accidental duplicate state lines can hide the state that a
  reader or tool expected to be authoritative. It also weakens metadata
  ordering diagnostics because a second state line is not treated as malformed
  metadata.
- Affected: plan authors, reviewers, runtime commands that rely on the parsed
  state, and render/list consumers.
- Risk: user-facing, because the visible markdown can contain contradictory
  state metadata while tools act on only one value.
- Current verification: parser tests cover missing state, metadata order,
  malformed metadata, duplicate assignee, and late metadata. The discrepancy
  file found no duplicate-state rejection test.

### D-007: `**Prior:**` parsing and validation are incomplete

Source findings: SY-003-C, SY-005-B, SY-005-C. Classification:
`missing-validation`.

- Exact mismatch: the grammar requires a comma-space separated list of
  kind-qualified references and forbids duplicate references. The parser
  extracts every regex match from any line starting with `**Prior:**`, accepts
  empty or malformed surrounding text, and stores only `TaskId` values. The
  validator then checks existence, ancestor references, and cycles, but cannot
  check duplicate references or whether the authored reference kind matches the
  referenced node kind.
- Why it matters: malformed dependency lines can be partially accepted,
  duplicate prerequisites can pass, and `Task 1` can resolve to a `Bug 1`
  without a kind-mismatch diagnostic.
- Affected: plan authors, dependency validation, readiness computation, JSON
  renderers, and custom node-kind workflows.
- Risk: user-facing validation risk, with internal AST-shape debt because the
  kind information is discarded before semantic validation.
- Current verification: validator tests cover missing dependencies, named and
  numeric dependencies, ancestor rejection, self/two/three-node cycles, and DAG
  success. Visible coverage does not reject malformed prior separators,
  empty prior lists, duplicate prior refs, or prior kind mismatches.

### D-008: `--state-machine` bypasses declared-name validation

Source finding: SY-004-B. Classification: `implementation-diverges`.

- Exact mismatch: the spec says the resolved YAML file's `name` must match the
  plan's `**States:**` value, while `--state-machine <path>` overrides
  automatic lookup. The explicit override branch loads the machine and returns
  it without comparing `machine.name` to the plan declaration; name matching is
  performed only for auto-discovered files.
- Why it matters: a plan can declare one workflow name while validation and
  runtime commands operate under a different explicit machine. That weakens
  the `**States:**` field as an auditable contract.
- Affected: CLI users passing `--state-machine`, automation wrappers, plans
  checked into repos with expected workflow names, and docs/examples for custom
  machines.
- Risk: user-facing, especially in scripted validation where an override may
  mask the wrong plan declaration.
- Current verification: integration tests cover auto-discovery and
  auto-discovered name mismatch. The mismatch check is not exercised for the
  explicit override path.

### D-009: Markdown state rendering rules are not enforced

Source finding: SY-004-D. Classification: `missing-validation`.

- Exact mismatch: the spec requires bare state values only for canonical names
  matching `IDENTIFIER`; names with whitespace or punctuation must be
  backticked, with escaped backslash and escaped backtick inside the quoted
  form. The parser accepts any non-empty state string, strips only a pair of
  outer backticks, and does not validate or unescape quoted characters. The
  validator then accepts a bare multi-word state if the loaded machine defines
  that exact state.
- Why it matters: markdown plans can use non-canonical state renderings that
  conflict with the documented encoding, and escaped state names cannot round
  trip according to the grammar.
- Affected: plan authors using custom state names, state-machine authors,
  validators, renderers, and examples involving spaces or punctuation.
- Risk: user-facing authoring/interop risk.
- Current verification: validator tests cover state existence, profile
  membership, exact-match-before-counted-suffix behavior, `-1` rejection,
  missing `visits`, and visit-budget limits. The discrepancy file found no
  visible tests for rejecting bare non-identifier state names or validating
  quoted escape sequences.

### D-010: Cancelled dependencies do not satisfy readiness

Source finding: SY-005-D. Classification: `implementation-diverges`.

- Exact mismatch: the core plan spec defines dependency readiness by terminal
  state alone, and the built-in terminal states include both `completed` and
  `cancelled`. The CLI readiness helper explicitly excludes normalized
  `cancelled` from satisfying prior dependencies.
- Why it matters: downstream work after a cancelled prerequisite may be ready
  according to the spec but blocked in `next`, `run`, and ready-list behavior.
  Cancellation semantics determine whether fallback or follow-up work can
  proceed.
- Affected: plan authors, `rhei next`, `rhei run`, ready/blocked listings, and
  custom workflows with cancellation branches.
- Risk: user-facing.
- Current verification: current implementation comments and tests assert that
  cancelled prerequisites do not unblock downstream work. That verifies the
  implementation behavior, not the core spec's terminal-only readiness rule.

### D-011: Workspace link validation uses the wrong root and does not block escapes

Source findings: SY-007-B, SY-007-C, SY-007-D. Classification:
`implementation-diverges`, `missing-validation`, and tentative `missing-test`.

- Exact mismatch: workspace links must resolve relative to the directory
  containing `index.rhei.md`, and normalized `..` paths that escape the
  workspace root must be rejected. `rhei validate <workspace-dir>` passes
  `input.parent()` as the base path, so links resolve relative to the parent of
  the workspace. The validator also checks only `base_path.join(file).exists()`
  and does not normalize and compare against the workspace root.
- Why it matters: valid workspace-relative links can be reported missing, and
  existing files outside the workspace can satisfy links that the spec says
  must be invalid.
- Affected: workspace authors, validation diagnostics, generated workspaces,
  and any workflow relying on link validation as a repository-bound integrity
  check.
- Risk: user-facing validation risk.
- Current verification: visible link tests cover extraction, missing files,
  existing files, external URLs, fragments, task/subtask content, and no-base
  behavior. The missing-test part is tentative because it is based on the
  visible audit search; no scoped tests were found for workspace-root
  resolution or `..` escape rejection.

### D-012: Authored result blocks are not parsed or validated

Source findings: SY-008-B, SY-008-C. Classification: `missing-validation` and
tentative `missing-test`.

- Exact mismatch: the grammar gives `> **Result:** [<task-id>](runtime/results/<task-id>.md)`
  a dedicated task-local metadata production and requires both link text and
  target to match the enclosing task id. The AST has no result field, the
  parser has no result-block branch, and validation has no result-block
  consistency pass; authored result-looking lines are treated as task content
  unless runtime string rewrites later strip or insert them.
- Why it matters: malformed result metadata can be committed and rendered as
  ordinary prose, and result links are not checked against the enclosing task.
  This also blurs the boundary between authored content and runtime-owned
  completion output.
- Affected: `rhei complete`, `rhei reset`, renderers, validators, humans
  reviewing completed tasks, and plan-writing skills that avoid authoring
  result blocks.
- Risk: user-facing validation/documentation risk.
- Current verification: runtime tests cover completion result-file creation,
  result-link insertion, insertion before child headings, assignee removal, and
  reset cleanup. The missing-test companion is tentative because it is based on
  the visible audit search; no tests were found for rejecting mismatched
  authored result link text or targets.

### D-013: Workspace validation still reports only the first parse error

Source finding: SY-011-B. Classification: `implementation-diverges`.

- Exact mismatch: the diagnostics contract favors actionable parse diagnostics,
  and single-file validation uses `parse_collect` to report multiple
  recoverable parse problems. Workspace validation still goes through
  `load_plan`, and the implementation comment explicitly identifies workspace
  multi-error parsing as a follow-up.
- Why it matters: authors fixing workspace task files can get a slower
  one-error-at-a-time loop even though the single-file path already aggregates
  recoverable parser diagnostics.
- Affected: workspace authors, agents repairing generated workspace task
  files, and `rhei validate <workspace-dir>` users.
- Risk: user-facing diagnostics/authoring friction.
- Current verification: single-file parse/validation separation and aggregated
  semantic validation have tests. Workspace loading tests cover valid and
  invalid workspace shapes, but not multi-error parse aggregation across task
  files.

### D-014: Dependency alias guidance is ambiguous for JSON surfaces

Source finding: SY-011-C. Classification: tentative `ambiguous-spec`.

- Exact mismatch: the authoring guide says SDKs expose dependencies as
  `task.metadata.dependsOn` / `depends_on` and includes "CLI JSON" in that
  naming statement. Callback context exposes `task.metadata.dependsOn`, while
  `rhei render --format json` exposes dependencies as `prior`; the visible
  NAPI surface does not expose a plan/task JSON API beyond version/help.
- Why it matters: consumers cannot tell whether `prior` or
  `task.metadata.dependsOn` is the stable CLI JSON contract. That makes
  automation and documentation around dependency fields brittle.
- Affected: CLI JSON consumers, callback authors, SDK/API documentation, and
  agents reading the authoring guide literally.
- Risk: user-facing for automation docs; internal for API naming consistency.
- Current verification: renderer code and callback context code expose the two
  different shapes. This is tentative because the guide may intend only one
  CLI JSON surface, but it does not identify that boundary.

## No-Discrepancy Areas

- SY-001-A: core single-file grammar for normal task trees is implemented,
  including H1, optional states/frontmatter, content sections, final `## Tasks`,
  H3-H6 task headings, metadata parsing, and fenced-code handling. Verification
  exists in parser and lexer tests.
- SY-001-C: a second H2 after `## Tasks` is rejected, matching the finality
  rule. Verification exists in parser diagnostics.
- SY-002-A and SY-002-C: default `structure` semantics, max depth, reserved
  `rhei`, case-insensitive kind matching, heading/id depth checks, parent-id
  extension, non-empty titles, and declared depth limits match the spec.
  Verification exists in parser and validator tests.
- SY-003-A and SY-003-D: required state metadata, metadata ordering, late
  metadata rejection, malformed near-miss diagnostics, and runtime assignee
  ownership are implemented. Verification exists in parser fixtures, CLI
  diagnostics tests, and completion/assignee rewrite tests.
- SY-004-A and SY-004-C: automatic state-machine lookup, auto-discovered name
  checks, state validity, profile allowed-state checks, exact counted-state
  lookup, and visit-budget validation match the spec. Verification exists in
  validator and CLI integration tests.
- SY-005-A: missing dependencies, ancestor dependencies, self-cycles, and
  multi-node cycles are validated. Verification exists in validator and CLI
  integration tests.
- SY-006-A and SY-006-B: workspace index parsing, direct task-file merging,
  global root-id checks, task-file rewrites, and index-owned `stateVisits`
  metadata are implemented. Verification exists in workspace integration tests.
- SY-007-A: single-file relative link validation and external/anchor skipping
  are implemented. Verification exists in validator link tests.
- SY-008-A: `rhei complete` writes result files, inserts result links, and
  removes assignees; reset strips result links and workspace runtime output.
  Verification exists in completion and reset tests.
- SY-009-A: terminal parent/non-terminal descendant validation is implemented,
  and `rhei complete` blocks completing a parent with open descendants.
  Verification exists in validator and integration tests.
- SY-010-A and SY-010-B: static artifact definition validation covers names,
  paths, duplicates, relativity, root escapes, and output optionality; runtime
  artifact roots use the execution root for single-file plans and workspaces.
  Verification exists in state-machine validator tests and runtime artifact
  command tests.
- SY-011-A: single-file `rhei validate` separates parse and semantic failures
  and aggregates semantic validation errors. Verification exists in CLI
  diagnostics and validator tests.
