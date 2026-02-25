# Discrepancy Audit: Syntax, Authoring, Parsing, and Validation

Partition: `syntax-authoring-validation`

Scope source: `runtime/spec-implementation-discrepancy-audit/syntax-authoring-validation/scope.md`

This file records comparison findings only. It does not propose fixes.

## Summary

- `implementation-diverges`: 8 findings
- `spec-stale`: 1 finding
- `ambiguous-spec`: 1 finding
- `missing-validation`: 9 findings
- `missing-test`: 2 findings
- `no-discrepancy`: 17 findings

## SY-001: File Shape And Grammar

### SY-001-A: Core single-file grammar is implemented for normal task trees

Classification: `no-discrepancy`

The parser recognizes the Rhei H1, optional `**States:**`, optional YAML frontmatter, pre-task H2 sections, final `## Tasks`, H3-H6 task headings, task state metadata, and code fences without tokenizing structural-looking text inside fenced blocks. Evidence:

- `docs/rhei.spec.md:97`
- `docs/rhei.spec.md:182`
- `crates/rhei-core/src/parser.rs:371`
- `crates/rhei-core/src/parser.rs:378`
- `crates/rhei-core/src/parser.rs:422`
- `crates/rhei-core/src/parser.rs:436`
- `crates/rhei-core/src/parser.rs:518`
- `crates/rhei-core/tests/lexer_edge_cases.rs:77`

### SY-001-B: Unsectioned prose before `## Tasks` is accepted and dropped

Classification: `implementation-diverges`

The formal grammar allows content only through H2 `content_section` blocks before `## Tasks`. The parser accepts ordinary non-empty prose after the H1 and before the first H2, but if no content section is open it silently discards that line. A unit test demonstrates this by parsing `Some intro line` before `## Tasks` and expecting no content sections. Evidence:

- `docs/rhei.spec.md:189`
- `docs/rhei.spec.md:201`
- `crates/rhei-core/src/parser.rs:544`
- `crates/rhei-core/src/parser.rs:550`
- `crates/rhei-core/src/parser.rs:1045`
- `crates/rhei-core/src/parser.rs:1068`
- `crates/rhei-core/src/parser.rs:1069`

### SY-001-C: Single-file `## Tasks` finality is enforced

Classification: `no-discrepancy`

The spec says everything after `## Tasks` is task structure and another H2 is invalid. The parser rejects H2 headings inside the tasks section with a dedicated diagnostic. Evidence:

- `docs/specs/rhei-authoring.spec.md:42`
- `docs/rhei.spec.md:236`
- `crates/rhei-core/src/parser.rs:790`
- `crates/rhei-core/src/parser.rs:792`

### SY-001-D: Workspace task files can contain forbidden preambles

Classification: `missing-validation`

The spec and writer skill require workspace task files to start directly with task definitions and contain no Rhei H1 or independent frontmatter. Implementation parses workspace task files by prepending a synthetic single-file header; non-task content before the first task is then ignored because there is no active task node. This means a task file can contain a `# Rhei:` line, frontmatter-looking block, or arbitrary prose before its first task without being rejected. Evidence:

- `docs/rhei.spec.md:151`
- `docs/rhei.spec.md:152`
- `skills/rhei-plan-writer/SKILL.md:30`
- `skills/rhei-plan-writer/SKILL.md:31`
- `crates/rhei-core/src/parser.rs:797`
- `crates/rhei-core/src/parser.rs:804`
- `crates/rhei-core/src/parser.rs:1007`
- `crates/rhei-core/src/parser.rs:1011`

### SY-001-E: Workspace task discovery is not recursive

Classification: `implementation-diverges`

The spec's workspace link example explicitly considers nested task files under `tasks/`, and workspace task files are described as arbitrary `.md` files within that directory. The loader uses a single `read_dir` over `tasks/` and filters only direct entries with extension `md`; nested `.md` task files are not discovered or merged into the logical graph. Evidence:

- `docs/rhei.spec.md:136`
- `docs/rhei.spec.md:137`
- `docs/rhei.spec.md:619`
- `docs/rhei.spec.md:622`
- `crates/rhei-core/src/workspace.rs:50`
- `crates/rhei-core/src/workspace.rs:54`

## SY-002: Structure, Node Kinds, And Task Hierarchy

### SY-002-A: Default depth and kind semantics match the spec

Classification: `no-discrepancy`

The default structure is `maxLevels: 2` with `nodeKinds: [task]`, custom `structure.maxLevels` is limited to 1 through 4, `rhei` is rejected as a node kind, and kind matching is case-insensitive. Evidence:

- `docs/rhei.spec.md:116`
- `docs/rhei.spec.md:389`
- `docs/rhei.spec.md:399`
- `crates/rhei-core/src/ast.rs:18`
- `crates/rhei-core/src/ast.rs:21`
- `crates/rhei-core/src/parser.rs:51`
- `crates/rhei-core/src/parser.rs:59`
- `crates/rhei-core/src/parser.rs:100`
- `crates/rhei-core/src/ast.rs:176`

### SY-002-B: `structure.nodeKinds` values are not validated as `IDENTIFIER`

Classification: `missing-validation`

The spec requires `structure.nodeKinds` entries to be unique `IDENTIFIER`s. The parser validates only string-ness, trim-non-empty, duplicate values, and the reserved `rhei` name. It does not reject entries that start with digits or contain characters outside `[A-Za-z0-9_-]`. Evidence:

- `docs/rhei.spec.md:395`
- `docs/rhei.spec.md:396`
- `docs/rhei.spec.md:352`
- `crates/rhei-core/src/parser.rs:72`
- `crates/rhei-core/src/parser.rs:93`
- `crates/rhei-core/src/parser.rs:94`
- `crates/rhei-core/src/parser.rs:106`

### SY-002-C: Heading depth, id depth, parent extension, and depth limit are enforced

Classification: `no-discrepancy`

The parser checks that H3-H6 depth matches dotted id depth, children extend the immediate parent by exactly one segment, titles are non-empty, and declared `structure.maxLevels` is not exceeded. Evidence:

- `docs/rhei.spec.md:535`
- `docs/rhei.spec.md:545`
- `docs/rhei.spec.md:578`
- `crates/rhei-core/src/parser.rs:583`
- `crates/rhei-core/src/parser.rs:590`
- `crates/rhei-core/src/parser.rs:607`
- `crates/rhei-core/src/parser.rs:632`
- `crates/rhei-core/src/parser.rs:639`

### SY-002-D: The authoring guide's named-task child prohibition is stale

Classification: `spec-stale`

The authoring guide says named task ids are conceptual anchors and "must not declare child tasks." The formal spec allows mixed numeric and named path segments, the plan-writer skill allows named children, and an integration-test comment records that the named-task child prohibition was removed. Evidence:

- `docs/specs/rhei-authoring.spec.md:49`
- `docs/specs/rhei-authoring.spec.md:51`
- `docs/rhei.spec.md:564`
- `docs/rhei.spec.md:574`
- `skills/rhei-plan-writer/SKILL.md:99`
- `skills/rhei-plan-writer/SKILL.md:100`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:755`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:758`

### SY-002-E: Runtime rewrite commands are root-`Task`-centric despite generic node kinds and child task nodes

Classification: `implementation-diverges`

The language treats every H3-H6 node of any declared kind as a task node. Several mutating CLI paths still locate or rewrite only root `### Task <id>:` headers and search only top-level `rhei.tasks` for command targets. This diverges for child task ids and custom root kinds such as `Bug 1`. Evidence:

- `docs/rhei.spec.md:373`
- `docs/rhei.spec.md:374`
- `docs/rhei.spec.md:632`
- `docs/specs/rhei-authoring.spec.md:64`
- `docs/specs/rhei-authoring.spec.md:66`
- `crates/rhei-cli/src/main.rs:8966`
- `crates/rhei-cli/src/main.rs:8973`
- `crates/rhei-cli/src/main.rs:9036`
- `crates/rhei-cli/src/main.rs:9042`
- `crates/rhei-cli/src/main.rs:9313`
- `crates/rhei-cli/src/main.rs:9318`
- `crates/rhei-cli/src/main.rs:9777`
- `crates/rhei-cli/src/main.rs:9798`

## SY-003: Task Metadata Fields

### SY-003-A: Required state and metadata ordering diagnostics exist

Classification: `no-discrepancy`

The parser requires every parsed task to have `**State:**`, rejects `**Prior:**` or `**Assignee:**` before state, rejects metadata after task content, and reports malformed near-miss metadata lines as parse diagnostics. Evidence:

- `docs/rhei.spec.md:267`
- `docs/rhei.spec.md:270`
- `docs/specs/rhei-authoring.spec.md:108`
- `crates/rhei-core/src/parser.rs:160`
- `crates/rhei-core/src/parser.rs:681`
- `crates/rhei-core/src/parser.rs:692`
- `crates/rhei-core/src/parser.rs:713`
- `crates/rhei-core/src/parser.rs:719`
- `crates/rhei-core/src/parser.rs:755`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:574`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:608`

### SY-003-B: Duplicate `**State:**` lines are accepted by overwriting

Classification: `missing-validation`

The grammar has exactly one `state_field`. The parser has a duplicate guard for `**Assignee:**`, but `**State:**` parsing simply assigns `top.state = Some(...)` each time it sees a state line before content, so a second state line silently replaces the first. Evidence:

- `docs/rhei.spec.md:270`
- `docs/rhei.spec.md:282`
- `crates/rhei-core/src/parser.rs:673`
- `crates/rhei-core/src/parser.rs:688`
- `crates/rhei-core/src/parser.rs:767`
- `crates/rhei-core/src/parser.rs:769`

### SY-003-C: `**Prior:**` list shape is only partially parsed

Classification: `missing-validation`

The grammar requires `**Prior:**` to be a comma-space separated list of kind-qualified task references. The parser accepts any line starting with `**Prior:**`, extracts every regex match it can find, and extends the prior list with those ids. It does not require the whole line to match the grammar, does not reject an empty prior list, and does not validate separators. Evidence:

- `docs/rhei.spec.md:284`
- `docs/rhei.spec.md:286`
- `docs/rhei.spec.md:288`
- `crates/rhei-core/src/parser.rs:381`
- `crates/rhei-core/src/parser.rs:705`
- `crates/rhei-core/src/parser.rs:725`
- `crates/rhei-core/src/parser.rs:730`

### SY-003-D: Assignee ownership behavior is implemented

Classification: `no-discrepancy`

The runtime-owned assignee field is represented in the parser/AST, `rhei next` writes it when claiming, and `rhei complete` removes it through completion rewriting. Evidence:

- `docs/rhei.spec.md:268`
- `docs/rhei.spec.md:272`
- `skills/rhei-plan-writer/SKILL.md:52`
- `crates/rhei-core/src/ast.rs:202`
- `crates/rhei-core/src/parser.rs:747`
- `crates/rhei-cli/src/main.rs:9235`
- `crates/rhei-cli/src/main.rs:9240`
- `crates/rhei-cli/src/main.rs:9802`
- `crates/rhei-cli/src/main.rs:9804`

## SY-004: `**States:**` Lookup And State Validity

### SY-004-A: Automatic state-machine lookup and auto-discovered name checks match the spec

Classification: `no-discrepancy`

When no override path is supplied, the CLI resolves sibling `states.yaml` for single-file plans and workspace-root `states.yaml` for workspaces, checks the YAML `name` against `**States:**`, and falls back to the built-in `rhei` machine only when the plan declares `rhei`. Evidence:

- `docs/rhei.spec.md:114`
- `crates/rhei-cli/src/main.rs:1231`
- `crates/rhei-cli/src/main.rs:1235`
- `crates/rhei-cli/src/main.rs:1251`
- `crates/rhei-cli/src/main.rs:1255`
- `crates/rhei-cli/src/main.rs:1261`
- `crates/rhei-cli/src/main.rs:1271`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2498`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2530`

### SY-004-B: `--state-machine` bypasses declared-name validation

Classification: `implementation-diverges`

The spec says the resolved YAML file's `name` must match the plan's `**States:**` value, while `--state-machine <path>` overrides automatic lookup. The explicit override branch loads and returns the machine without comparing its `name` to `loaded.rhei.states`; name matching is only performed in the auto-discovery branch. Evidence:

- `docs/rhei.spec.md:114`
- `docs/specs/rhei-authoring.spec.md:166`
- `docs/specs/rhei-authoring.spec.md:169`
- `crates/rhei-cli/src/main.rs:1244`
- `crates/rhei-cli/src/main.rs:1248`
- `crates/rhei-cli/src/main.rs:1255`
- `crates/rhei-cli/src/main.rs:1267`

### SY-004-C: State profile validity and counted visit suffixes are validated

Classification: `no-discrepancy`

The validator checks authored states against the active state-machine state map, enforces profile `allowed` membership when profiles/node policy exist, implements exact state-name lookup before counted suffix parsing, rejects `-1`, rejects suffixes for states without `visits`, and checks visit budgets. Evidence:

- `docs/rhei.spec.md:462`
- `docs/rhei.spec.md:466`
- `docs/rhei.spec.md:485`
- `docs/rhei.spec.md:498`
- `crates/rhei-validator/src/lib.rs:1675`
- `crates/rhei-validator/src/lib.rs:1676`
- `crates/rhei-validator/src/lib.rs:1680`
- `crates/rhei-validator/src/lib.rs:1878`
- `crates/rhei-validator/src/lib.rs:1893`
- `crates/rhei-validator/src/lib.rs:1906`
- `crates/rhei-validator/src/lib.rs:1914`
- `crates/rhei-validator/src/lib.rs:1923`

### SY-004-D: Markdown state rendering rules are not validated

Classification: `missing-validation`

The spec requires bare state values only for canonical names matching `IDENTIFIER`; names with whitespace or punctuation must be backticked, and backticked values support escaped backslash and escaped backtick. The parser accepts any non-empty `**State:**` value, strips outer backticks only, and does not unescape or validate escaped characters. A bare multi-word state validates if the loaded machine defines that exact state. Evidence:

- `docs/rhei.spec.md:472`
- `docs/rhei.spec.md:479`
- `docs/rhei.spec.md:290`
- `docs/rhei.spec.md:300`
- `crates/rhei-core/src/parser.rs:379`
- `crates/rhei-core/src/parser.rs:687`
- `crates/rhei-core/src/parser.rs:1025`
- `crates/rhei-core/src/parser.rs:1028`
- `crates/rhei-validator/src/lib.rs:1893`
- `crates/rhei-validator/src/lib.rs:1894`

## SY-005: Prior Dependency Semantics

### SY-005-A: Missing, ancestor, self-cycle, and multi-node cycle validation exists

Classification: `no-discrepancy`

The validator builds a global task index, reports missing dependencies, rejects ancestor-as-prior dependencies, and detects dependency cycles. Self-reference is rejected through the cycle detector. Evidence:

- `docs/rhei.spec.md:419`
- `docs/rhei.spec.md:423`
- `docs/rhei.spec.md:523`
- `crates/rhei-validator/src/lib.rs:1764`
- `crates/rhei-validator/src/lib.rs:1791`
- `crates/rhei-validator/src/lib.rs:1802`
- `crates/rhei-validator/src/lib.rs:1806`
- `crates/rhei-validator/src/lib.rs:2080`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:690`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:704`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:719`

### SY-005-B: Duplicate prior references are not checked

Classification: `missing-validation`

The spec forbids duplicate references in one `**Prior:**` list. The validator iterates dependencies for existence and ancestor checks but does not track a per-task seen set for duplicate prior ids. Evidence:

- `docs/rhei.spec.md:419`
- `docs/rhei.spec.md:423`
- `crates/rhei-validator/src/lib.rs:1791`
- `crates/rhei-validator/src/lib.rs:1802`
- `crates/rhei-validator/src/lib.rs:1812`

### SY-005-C: Prior reference kind is discarded and cannot be validated

Classification: `missing-validation`

The spec says a `**Prior:**` item is kind-qualified and that the kind must match the referenced node's declared kind. The lexer/parser capture both kind and id syntactically, but only `TaskId` values are stored in the AST. The validator's dependency index is keyed by `TaskId` only, so `**Prior:** Task 1` can resolve to `Bug 1` without a kind-mismatch diagnostic. Evidence:

- `docs/rhei.spec.md:288`
- `docs/rhei.spec.md:448`
- `docs/rhei.spec.md:449`
- `crates/rhei-core/src/ast.rs:200`
- `crates/rhei-core/src/lexer.rs:42`
- `crates/rhei-core/src/lexer.rs:152`
- `crates/rhei-core/src/parser.rs:725`
- `crates/rhei-core/src/parser.rs:727`
- `crates/rhei-validator/src/lib.rs:1764`
- `crates/rhei-validator/src/lib.rs:1803`

### SY-005-D: Cancelled dependencies do not satisfy readiness

Classification: `implementation-diverges`

The spec defines dependency readiness by terminal-state semantics alone: every referenced dependency is ready when it is in any `final: true` state, and the built-in terminal states include both `completed` and `cancelled`. The CLI explicitly excludes `cancelled` from satisfying dependencies. Evidence:

- `docs/rhei.spec.md:379`
- `docs/rhei.spec.md:381`
- `docs/rhei.spec.md:383`
- `docs/rhei.spec.md:387`
- `crates/rhei-cli/src/main.rs:8662`
- `crates/rhei-cli/src/main.rs:8667`

### SY-005-E: `next` / run readiness is root-task only

Classification: `implementation-diverges`

The authoring guide says child task nodes are full task nodes and may declare `**Prior:**` dependencies just like root tasks. The ready-task scan used by `rhei next` walks only `rhei.tasks` roots and builds dependency state only for root ids; it does not recurse into child task nodes. Evidence:

- `docs/specs/rhei-authoring.spec.md:64`
- `docs/specs/rhei-authoring.spec.md:67`
- `docs/rhei.spec.md:373`
- `docs/rhei.spec.md:377`
- `crates/rhei-cli/src/main.rs:8674`
- `crates/rhei-cli/src/main.rs:8681`
- `crates/rhei-cli/src/main.rs:8689`
- `crates/rhei-cli/src/main.rs:8701`
- `crates/rhei-cli/src/main.rs:8719`
- `crates/rhei-cli/src/main.rs:8723`

## SY-006: Directory Workspace Semantics

### SY-006-A: Workspace index, task merging, global ids, and task-file rewrites are implemented

Classification: `no-discrepancy`

The implementation recognizes a workspace by `index.rhei.md`, rejects `## Tasks` in the index, parses direct task files from `tasks/`, merges them into one `Rhei`, tracks root task source files, validates duplicate root ids across files, and updates the correct task file for root-task transitions. Evidence:

- `docs/rhei.spec.md:133`
- `docs/rhei.spec.md:141`
- `docs/rhei.spec.md:149`
- `crates/rhei-core/src/workspace.rs:28`
- `crates/rhei-core/src/workspace.rs:37`
- `crates/rhei-core/src/workspace.rs:43`
- `crates/rhei-core/src/workspace.rs:70`
- `crates/rhei-core/src/workspace.rs:72`
- `crates/rhei-core/src/parser.rs:967`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2451`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2608`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2652`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2864`

### SY-006-B: Runtime `stateVisits` metadata is stored in the workspace index

Classification: `no-discrepancy`

For workspaces, transition code uses the task file for markdown state rewrites and `index.rhei.md` as the metadata file, matching the spec's single authoritative frontmatter map. Evidence:

- `docs/rhei.spec.md:151`
- `docs/rhei.spec.md:156`
- `docs/rhei.spec.md:162`
- `docs/rhei.spec.md:166`
- `crates/rhei-cli/src/main.rs:4937`
- `crates/rhei-cli/src/main.rs:4943`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2702`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2731`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2738`

## SY-007: Link Integrity

### SY-007-A: Single-file relative links and skipped external/anchor links are validated

Classification: `no-discrepancy`

The validator extracts markdown links from content sections and task content, skips `http://`, `https://`, `mailto:`, and fragment-only links, strips file fragments before checking, and reports missing relative files when a base path is supplied. Evidence:

- `docs/rhei.spec.md:589`
- `docs/rhei.spec.md:596`
- `crates/rhei-validator/src/lib.rs:2018`
- `crates/rhei-validator/src/lib.rs:2026`
- `crates/rhei-validator/src/lib.rs:2046`
- `crates/rhei-validator/src/lib.rs:2055`
- `crates/rhei-validator/src/lib.rs:2063`
- `crates/rhei-validator/src/lib.rs:2069`
- `crates/rhei-validator/src/lib.rs:2070`

### SY-007-B: Workspace validation uses the parent of the workspace, not the workspace root

Classification: `implementation-diverges`

Workspace links must resolve relative to the directory containing `index.rhei.md`. `rhei validate <workspace-dir>` passes `input.parent()` as the link base even when `input` is a workspace directory, so links are resolved relative to the parent directory of the workspace. Evidence:

- `docs/rhei.spec.md:589`
- `docs/rhei.spec.md:594`
- `crates/rhei-cli/src/main.rs:3538`
- `crates/rhei-cli/src/main.rs:3551`
- `crates/rhei-cli/src/main.rs:3554`
- `crates/rhei-cli/src/main.rs:4215`
- `crates/rhei-cli/src/main.rs:4217`

### SY-007-C: Link validation does not reject normalized workspace-root escapes

Classification: `missing-validation`

The spec requires workspace-relative links to normalize `.` and `..` and reject any path that escapes the workspace root. The validator joins the link target to the supplied base path and checks only `exists()`. It does not normalize and compare the result against the root, so an existing `../outside.md` target can pass. Evidence:

- `docs/rhei.spec.md:598`
- `docs/rhei.spec.md:605`
- `crates/rhei-validator/src/lib.rs:2055`
- `crates/rhei-validator/src/lib.rs:2069`
- `crates/rhei-validator/src/lib.rs:2070`

### SY-007-D: Workspace link-root and escape cases are not covered by visible tests

Classification: `missing-test`

The visible link tests cover extraction, missing files, existing files, external URLs, fragments, task/subtask content, and no-base behavior. I did not find scoped tests for workspace-root link resolution, nested workspace task files, or `..` escape rejection. Evidence:

- `crates/rhei-validator/src/lib.rs:3143`
- `crates/rhei-validator/src/lib.rs:3152`
- `crates/rhei-validator/src/lib.rs:2055`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2451`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2572`

## SY-008: Result Block Consistency And Completion Output

### SY-008-A: `rhei complete` writes result files, inserts result links, and removes assignees

Classification: `no-discrepancy`

The completion command transitions to a non-cancelled terminal state, appends a result entry under `runtime/results/<task-id>.md`, inserts `> **Result:** [<id>](runtime/results/<id>.md)`, and strips the target task's assignee line. Reset strips result links and removes workspace runtime output. Evidence:

- `docs/rhei.spec.md:274`
- `docs/rhei.spec.md:278`
- `docs/rhei.spec.md:657`
- `crates/rhei-cli/src/main.rs:9367`
- `crates/rhei-cli/src/main.rs:9371`
- `crates/rhei-cli/src/main.rs:9373`
- `crates/rhei-cli/src/main.rs:9781`
- `crates/rhei-cli/src/main.rs:9802`
- `crates/rhei-cli/src/main.rs:9543`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:1281`

### SY-008-B: Authored result blocks are not parsed or validated as task-local metadata

Classification: `missing-validation`

The spec defines a dedicated `result_block` production and requires link text and target to match the enclosing task id. The AST has no result field, the parser has no result-block branch, and validation does not include a result-block consistency pass; result-looking lines are just task content unless a runtime reset/completion string rewrite handles them. Evidence:

- `docs/rhei.spec.md:274`
- `docs/rhei.spec.md:280`
- `docs/rhei.spec.md:657`
- `docs/rhei.spec.md:680`
- `crates/rhei-core/src/ast.rs:190`
- `crates/rhei-core/src/ast.rs:209`
- `crates/rhei-core/src/parser.rs:797`
- `crates/rhei-core/src/parser.rs:803`
- `crates/rhei-validator/src/lib.rs:1719`
- `crates/rhei-validator/src/lib.rs:1729`
- `crates/rhei-cli/src/main.rs:9546`
- `crates/rhei-cli/src/main.rs:9552`

### SY-008-C: Result-block consistency has no visible validation tests

Classification: `missing-test`

Visible tests cover runtime insertion by `rhei complete`, insertion before child headings, and reset cleanup, but I did not find tests for rejecting authored result blocks with mismatched link text or mismatched `runtime/results/<task-id>.md` target. Evidence:

- `crates/rhei-cli/src/main.rs:11655`
- `crates/rhei-cli/src/main.rs:11685`
- `crates/rhei-cli/src/main.rs:11727`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:1281`

## SY-009: Terminal Tree Coherence

### SY-009-A: Terminal parent / non-terminal descendant validation is implemented

Classification: `no-discrepancy`

The validator interprets terminal states using `final: true` from the active machine, walks descendants, and reports terminal parents that contain non-terminal descendants. `rhei complete` separately blocks completion of a parent with non-terminal descendants. Evidence:

- `docs/rhei.spec.md:379`
- `docs/rhei.spec.md:682`
- `docs/rhei.spec.md:684`
- `crates/rhei-validator/src/lib.rs:1981`
- `crates/rhei-validator/src/lib.rs:1988`
- `crates/rhei-validator/src/lib.rs:1999`
- `crates/rhei-cli/src/main.rs:9331`
- `crates/rhei-cli/src/main.rs:9333`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:1235`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:1281`

## SY-010: State Artifact Contracts

### SY-010-A: Artifact definition validation covers the static contract

Classification: `no-discrepancy`

State-machine loading validates non-empty artifact names and paths, duplicate names within inputs/outputs, relative paths, `..` escapes in static path components, and disallows `optional: true` on outputs. Evidence:

- `docs/rhei.spec.md:694`
- `docs/rhei.spec.md:699`
- `docs/rhei.spec.md:710`
- `crates/rhei-validator/src/lib.rs:1556`
- `crates/rhei-validator/src/lib.rs:1564`
- `crates/rhei-validator/src/lib.rs:1570`
- `crates/rhei-validator/src/lib.rs:1576`
- `crates/rhei-validator/src/lib.rs:1582`
- `crates/rhei-validator/src/lib.rs:1587`
- `crates/rhei-validator/src/lib.rs:1592`

### SY-010-B: Runtime artifact roots use the execution root

Classification: `no-discrepancy`

Runtime artifact resolution uses the single-file plan directory or workspace directory as the execution root, and `next` enforces state inputs before claim/output. Transition and completion paths share the same resolution helpers. Evidence:

- `docs/rhei.spec.md:701`
- `docs/rhei.spec.md:718`
- `crates/rhei-cli/src/main.rs:4215`
- `crates/rhei-cli/src/main.rs:4219`
- `crates/rhei-cli/src/main.rs:4506`
- `crates/rhei-cli/src/main.rs:4527`
- `crates/rhei-cli/src/main.rs:4842`
- `crates/rhei-cli/src/main.rs:4882`
- `crates/rhei-cli/src/main.rs:9127`
- `crates/rhei-cli/src/main.rs:9156`

## SY-011: Diagnostics And Command Contracts

### SY-011-A: `rhei validate` separates parse and semantic failures and aggregates validation errors

Classification: `no-discrepancy`

Single-file validation uses the multi-error parser for recoverable parse problems, reports parse errors before semantic validation, and semantic validation aggregates dependency, state, terminal-tree, assignee, and link errors. Evidence:

- `docs/rhei.spec.md:76`
- `crates/rhei-cli/src/main.rs:3533`
- `crates/rhei-cli/src/main.rs:3542`
- `crates/rhei-cli/src/main.rs:3546`
- `crates/rhei-cli/src/main.rs:3553`
- `crates/rhei-cli/src/main.rs:3559`
- `crates/rhei-validator/src/lib.rs:1719`
- `crates/rhei-validator/src/lib.rs:1729`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:574`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:670`

### SY-011-B: Workspace validation does not use the multi-error parse path

Classification: `implementation-diverges`

The diagnostics contract favors actionable parse diagnostics, and single-file `validate` uses `parse_collect` to enumerate recoverable parse problems. Workspaces still go through `load_plan`, and the implementation comment identifies workspace multi-error parsing as a follow-up. Evidence:

- `docs/rhei.spec.md:76`
- `docs/rhei.spec.md:367`
- `crates/rhei-cli/src/main.rs:3533`
- `crates/rhei-cli/src/main.rs:3536`
- `crates/rhei-cli/src/main.rs:3538`
- `crates/rhei-cli/src/main.rs:3541`
- `crates/rhei-core/src/workspace.rs:66`

### SY-011-C: Dependency alias guidance is ambiguous relative to implemented JSON surfaces

Classification: `ambiguous-spec`

The authoring guide says SDKs expose dependencies as `task.metadata.dependsOn` / `depends_on` and includes "CLI JSON" in that naming statement. The callback context exposes `task.metadata.dependsOn`, but `rhei render --format json` exposes task dependencies as `prior`, and there is no visible NAPI plan API beyond `version()` / `help()`. The guide does not specify which CLI JSON surface is normative. Evidence:

- `docs/specs/rhei-authoring.spec.md:150`
- `docs/specs/rhei-authoring.spec.md:153`
- `crates/rhei-cli/src/main.rs:4318`
- `crates/rhei-cli/src/main.rs:4320`
- `crates/rhei-output/src/lib.rs:82`
- `crates/rhei-output/src/lib.rs:91`
- `crates/rhei-napi/src/lib.rs:1`
