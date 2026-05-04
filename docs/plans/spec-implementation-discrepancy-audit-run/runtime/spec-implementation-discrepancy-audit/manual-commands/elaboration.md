# Discrepancy Elaboration: Manual Worker and Inspection Commands

Source findings: `runtime/spec-implementation-discrepancy-audit/manual-commands/discrepancies.md`

This elaboration groups duplicate findings, marks weak evidence as tentative,
and records no-discrepancy areas. It does not choose a reconciliation strategy.

## Duplicate Merges

- MC-001-B and MC-031-B are the same behavioral gap: `rhei complete` does not
  reject gating states.
- MC-001-C is the missing-test companion to that gating completion gap.
- MC-002-A is the shared root for the profile-vs-legacy-initial issues in
  `rhei next` and `rhei reset`; MC-041-B is the reset-specific manifestation.
- MC-023-A and MC-023-C both concern transition result/audit trail behavior.
  MC-033-A is a coupled no-discrepancy: completion currently writes only one
  result entry because transition writes none.
- MC-080-C is an aggregate diagnostic finding whose concrete mismatches are
  already elaborated under `next`, `transition`, and `complete` output.
- MC-070-A, MC-070-B, MC-071-A, and MC-072-A are grouped as missing specified
  `rhei viz` CLI integration, with the `xtask` prototype called out separately.

## Elaborated Discrepancies

### D-001: `rhei next` help describes old transition semantics

Source findings: MC-000-C. Classification: `spec-stale`.

- Exact mismatch: the current command help says `next` transitions the next
  ready task forward one step. The `rhei-next` spec says default `next` claims a
  task by assigning it and does not advance state, except for the special
  non-runnable initial-state auto-advance path.
- Why it matters: help text is often the first contract users and scripts see.
  A worker may believe `next` is responsible for workflow advancement and skip
  an explicit `transition`, or a reviewer may misread the command's safety
  properties.
- Affected: humans using `rhei --help`, manual agents that receive or quote CLI
  help, and docs generated from clap metadata.
- Risk: user-facing.
- Current verification: the implementation exposes the stale help strings in
  the clap command definition. Existing `next` behavior tests exercise command
  execution, but the discrepancy file did not identify help-text assertions.

### D-002: `rhei complete` can exit gating states

Source findings: MC-001-B, MC-001-C, MC-031-B. Classification:
`implementation-diverges`, with a companion `missing-test`.

- Exact mismatch: the complete spec requires rejecting a current state whose
  state definition has `gating: true`. The implementation rejects terminal
  states and open descendants, then directly selects the first non-cancelled
  terminal transition. It does not inspect the current state's gating flag, so a
  default `human-review -> completed` edge is eligible for `rhei complete`.
- Why it matters: gating states are intended to require explicit human action.
  If an autonomous or manual worker can call `complete` from `human-review`, the
  gate no longer protects approval, legal, security, or PM review steps.
- Affected: human-review workflows, custom machines with gating states, and any
  agent wrapper that treats `complete` as safe from every non-terminal state.
- Risk: user-facing, because it can advance real plan state past a required
  human checkpoint.
- Current verification: `rhei run` has e2e coverage showing it stops at
  `human-review` gates, and `complete` has coverage for terminal rejection,
  child blocking, cancelled-target exclusion, and successful completion. No
  visible test asserts that `rhei complete` rejects a gating state.

### D-003: Commands still depend on legacy per-state `initial`

Source findings: MC-002-A, MC-041-B. Classification: `implementation-diverges`.

- Exact mismatch: the states spec says `profiles` and `node_policy` replace
  per-state `initial: true`; each node's initial state is resolved from its
  profile. `rhei next` and `rhei reset` still derive initiality by scanning
  `StateDef.initial`. `next` uses that flag to identify auto-claimable tasks;
  `reset` computes one machine-wide initial state and applies it to all nodes.
- Why it matters: profile-only machines can validate but fail or behave
  incorrectly at runtime. Machines with different node profiles cannot be reset
  correctly, and tasks in a profile initial state may not be claimable if the
  state itself has no legacy initial flag.
- Affected: profile-authored state machines such as the checked-in spec machine,
  newer templates, examples, and users writing machines according to the current
  states spec.
- Risk: user-facing for command behavior; internal for schema compatibility
  debt.
- Current verification: validator coverage exists for profile fields and task
  state validation. Existing `next` and `reset` tests use legacy initial-style
  machines, so they verify legacy behavior rather than profile-only behavior.

### D-004: Built-in defaults and templates still author legacy initial schema

Source findings: MC-002-B. Classification: `spec-stale`.

- Exact mismatch: the current states spec and state-machine-writer skill say not
  to author `initial: true` under states. The built-in default machine and
  several templates still use that legacy schema, and the validator permits a
  compatibility mode when profile fields are absent.
- Why it matters: users have two conflicting examples of the state-machine
  format: the normative spec points to profiles, while shipped defaults and
  templates teach the old form. Tooling also has an undocumented compatibility
  mode.
- Affected: template authors, state-machine authors, validator maintainers, and
  anyone comparing `docs/specs/states.yaml` with runtime defaults.
- Risk: mostly internal/documentation, but user-facing when copied templates
  become the de facto API.
- Current verification: validator tests cover compatibility and profile
  rejection rules, but the spec does not document the compatibility mode.

### D-005: Runtime template variables for `model.provider` and `model.name` use the wrong namespace

Source findings: MC-003-B. Classification: `implementation-diverges`.

- Exact mismatch: the states spec defines `{model.provider}` as the resolved
  provider id and `{model.name}` as the provider model name for the selected
  model profile. The implementation derives `model.provider` from an inline
  target selector's provider, and `model.name` from the selected model
  identifier. It does not resolve both fields through settings-backed model
  profile data.
- Why it matters: state instructions and artifact paths can render misleading
  or incomplete model information. This is especially visible in multi-provider
  workflows where a model profile id is not the same as the provider model name.
- Affected: manual agents reading rendered instructions, workflows using
  `{model.provider}` or `{model.name}`, and artifacts keyed by model variables.
- Risk: user-facing for prompts and artifact names; internal for settings/model
  resolution consistency.
- Current verification: template variable substitution and simple conditionals
  have coverage. The audit did not identify tests specifically asserting the
  provider/name namespace semantics.

### D-006: `rhei next` text and JSON output do not match specified shapes

Source findings: MC-010-A, MC-010-B. Classification: `implementation-diverges`.

- Exact mismatch: the spec's claim text is `Task <ID>: <title>`, `State:
  <state>`, a blank line, then instructions; peek text starts with `Next:`.
  The implementation prints `Task <id> claimed: '<from>' -> '<to>'` or `Task
  <id> (already in '<state>')`, includes task content and children, and labels
  instructions with `--- Instructions (<state>) ---`. JSON omits
  `model_provider` and `model_name`, while adding fields such as `kind`,
  `from_state`, `personality`, `content`, and `children`.
- Why it matters: scripts and agents consuming `next` output cannot rely on the
  documented contract. Extra human text may be useful, but unversioned shape
  differences make automation brittle.
- Affected: manual agents, wrappers using JSON, users following docs, and tests
  written from the spec.
- Risk: user-facing.
- Current verification: e2e tests cover current JSON output, children, and
  personality rendering. They verify implementation behavior, not the specified
  output contract.

### D-007: `rhei next` usage/options table omits implemented flags

Source findings: MC-010-C. Classification: `spec-stale`.

- Exact mismatch: the spec usage/options table lists only `--peek`, while the
  CLI also exposes `--task`, `--json`, and `--no-callbacks`. JSON is discussed
  later in the spec, but the command surface table is incomplete.
- Why it matters: users scanning the options table can miss important
  automation and targeting modes. Incomplete option documentation also makes
  command compatibility harder to audit.
- Affected: users, docs consumers, command wrappers, and shell-completion
  expectations.
- Risk: user-facing documentation risk.
- Current verification: clap exposes the additional flags and e2e tests cover
  at least `--task` and `--json`. The spec table remains stale.

### D-008: Auto-pick `rhei next` only claims legacy initial-state tasks

Source findings: MC-011-A. Classification: `implementation-diverges`.

- Exact mismatch: the spec defines claimability as satisfied priors, no
  assignee, non-terminal/non-gating state, and existing required inputs. The
  implementation first finds ready tasks and then filters to states whose
  `StateDef.initial` flag is true.
- Why it matters: an unassigned ready task already in `pending`,
  `agent-review`, or another non-initial work state is skipped by automatic
  `next`, even though the spec says it is claimable. This can leave resumable
  manual work invisible to the default command.
- Affected: workers resuming mid-workflow tasks, PMs expecting `next` to pick
  the first unassigned ready task, and custom machines with multiple work
  states.
- Risk: user-facing.
- Current verification: current tests cover dependency order, repeated
  next-to-completion behavior, targeted `--task`, and runnable initial-state
  handling. They do not prove auto-pick claimability for non-initial ready work.

### D-009: Cancelled-prerequisite semantics conflict across specs

Source findings: MC-011-B. Classification: `ambiguous-spec`.

- Exact mismatch: the next spec says priors are satisfied when prior tasks are
  terminal, naming `completed` or `cancelled`. The list spec says readiness uses
  terminal, non-cancelled prerequisites, and usage prose says priors must be
  completed. Implementation and tests use terminal-but-not-cancelled.
- Why it matters: downstream work may either start after cancellation or remain
  blocked, depending on which spec text a user follows. Cancellation semantics
  directly affect dependency safety.
- Affected: plan authors, manual workers, `next`, `list --ready`, and
  orchestrators relying on prerequisite readiness.
- Risk: user-facing.
- Current verification: there is an e2e test that cancelled prerequisites do not
  unblock dependents, matching implementation and list spec behavior. The next
  spec remains inconsistent.

### D-010: Claim mode does not re-read and revalidate claimability under one claim lock

Source findings: MC-012-A. Classification: `implementation-diverges`.

- Exact mismatch: the spec requires acquiring a lock, re-reading, and
  revalidating claimability under that lock before writing the assignee
  atomically. The implementation selects a task before locking, may perform an
  auto-transition, reloads once, then writes `**Assignee:**` with a separate
  lock. If another writer inserted an assignee, the assignee rewrite no-ops, but
  `next_command` still prints the task as claimed.
- Why it matters: concurrent workers can receive misleading claim output. The
  losing worker may start work it did not actually claim, which undermines
  predictable manual execution.
- Affected: parallel manual agents, humans running `next` concurrently, and any
  orchestrator that depends on claim output as a lock guarantee.
- Risk: user-facing concurrency risk.
- Current verification: assignee write code holds an exclusive file lock and
  avoids overwriting an existing assignee. The audit did not identify a
  concurrent claim test that asserts loser behavior or re-selection.

### D-011: Tentative: dedicated peek-mode behavior coverage is not visible

Source findings: MC-013-B. Classification: `missing-test`.

- Exact mismatch: this is a verification gap, not a proven behavior mismatch.
  The spec says `next --peek` is read-only, takes no lock, writes no assignee,
  performs no state change, and reports missing inputs like claim mode. The
  implementation appears to guard auto-transition and assignee writes with
  `!peek`, but the scoped test list did not show dedicated peek assertions.
- Why it matters: peek is intended for safe inspection. A regression that made
  it mutate state would be high impact.
- Affected: PM-style navigation, scripts, humans inspecting ready work, and
  agents that use peek before claim.
- Risk: user-facing if broken; current finding is internal test risk.
- Current verification: implementation guards are visible. Existing `next`
  tests are broad but not explicitly named for peek read-only/no-assignee/no
  missing-artifact parity behavior.

### D-012: No-claimable diagnostics do not implement the three specified summaries

Source findings: MC-014-B. Classification: `implementation-diverges`.

- Exact mismatch: the spec requires distinct summaries for all terminal,
  gating/human action, and all in-flight/claimed. The implementation emits
  different wording, includes a mid-workflow/non-initial category, treats gating
  states through that ready-non-initial path rather than a human-action summary,
  and lacks an explicit all-claimed/in-flight summary.
- Why it matters: the diagnostic is how a human or orchestrator distinguishes a
  finished plan from blocked human review or already-claimed work. Different
  categories change operational decisions.
- Affected: manual workers, PMs, scripts parsing stderr/stdout, and agents
  deciding whether to wait, transition, or stop.
- Risk: user-facing.
- Current verification: there is a `next_fails_when_all_completed` test and
  other `next` failure tests. The audit did not identify coverage for the full
  three-summary matrix.

### D-013: Backtick-wrapped CLI state values are not accepted by `rhei transition`

Source findings: MC-020-B. Classification: `implementation-diverges`.

- Exact mismatch: the transition spec says CLI state values follow main-spec
  state-value rendering, including backtick-wrapped non-identifiers. The
  implementation validates the raw `--from` and `--to` strings with
  `machine.is_valid_state()` before normalizing, so a literal CLI value such as
  `` `in review` `` is rejected even when that is the markdown rendering.
- Why it matters: states with spaces or other non-identifier names have a valid
  markdown representation but an incompatible command-line contract.
- Affected: users of custom machines with non-identifier state names, examples
  using escaped state values, and shell/completion flows.
- Risk: user-facing.
- Current verification: transition tests cover normal identifiers, workspace
  updates, CAS, wildcard transitions, disallowed paths, and missing target
  inputs. The audit did not identify a CLI test for backtick-wrapped state
  arguments.

### D-014: `rhei transition` does not validate the whole plan before mutating

Source findings: MC-021-B. Classification: `missing-validation`.

- Exact mismatch: the transition spec says to load the state machine and plan
  and validate before mutation. `transition_command` loads both but does not
  call `validate_with_machine`; `execute_transition` parses enough to locate the
  task, check current state, and validate the edge, but does not perform full
  semantic plan validation.
- Why it matters: transition may mutate a plan that other commands would reject
  as invalid. That can compound existing plan problems or make invalid plans
  harder to repair.
- Affected: humans using transition on damaged or partially edited plans,
  custom tooling, and workflows relying on validation as a command invariant.
- Risk: user-facing data integrity risk, with internal consistency impact.
- Current verification: transition has tests for successful state change, CAS,
  invalid transition, nonexistent task, callbacks, counted loops, and workspace
  behavior. Validation-before-mutation is not visibly covered.

### D-015: CAS conflict text differs from the specified text

Source findings: MC-021-C. Classification: `implementation-diverges`.

- Exact mismatch: the spec message is `Task <ID> is in state '<actual>', not
  '<from>'. Another transition may have preceded this call.` The implementation
  emits `conflict: Task <ID> is in state '<actual>', expected '<from>'`.
- Why it matters: the implementation message is actionable, but exact text
  differences matter for documented UX and scripts matching known diagnostics.
- Affected: humans, automated wrappers, and test suites written from the spec.
- Risk: user-facing but low severity unless consumers parse exact text.
- Current verification: CAS conflict tests exist and assert current behavior.

### D-016: Tentative: transition output-check ordering conflicts between specs

Source findings: MC-022-B. Classification: `ambiguous-spec`.

- Exact mismatch: the transition-command spec lists required output checks
  before `on_leave`; the states spec says outputs are checked after callbacks
  complete and before committing. The implementation follows the states spec by
  executing `on_leave`, resolving redirects, then checking outputs and target
  inputs before writing.
- Why it matters: callback authors need to know whether `on_leave` is allowed to
  create required outputs. The ordering changes callback responsibilities and
  whether a missing output blocks callback execution.
- Affected: state-machine authors, callback authors, and manual transition
  users.
- Risk: user-facing for callback workflows; internal for spec consistency.
- Current verification: callback tests and artifact tests exist, but the audit
  did not identify a test that explicitly locks down whether `on_leave` may
  produce required outputs.

### D-017: `rhei transition` does not append transition entries to result files

Source findings: MC-023-A, MC-023-C. Classification:
`implementation-diverges`, with companion `missing-test`.

- Exact mismatch: the transition and complete specs require `rhei transition` to
  append a heading entry to `runtime/results/<task-id>.md`. The shared
  `execute_transition` function only rewrites state and metadata; only
  `complete_command` calls `append_result_entry`.
- Why it matters: result files are specified as an ordered audit trail of every
  transition. Without transition entries, they only record completion, losing
  intermediate state history such as review/fix loops.
- Affected: humans auditing task history, agents summarizing work, PMs reading
  runtime results, and any tool that expects transition journals.
- Risk: user-facing auditability risk.
- Current verification: transition tests verify state changes and CAS behavior.
  No visible test asserts transition result-file append behavior. No visible
  test asserts that `transition` preserves an existing `**Assignee:**` line,
  though the implementation does not modify assignee lines.

### D-018: `rhei transition` success output differs from the spec

Source findings: MC-023-B. Classification: `implementation-diverges`.

- Exact mismatch: the spec shows ASCII `->` and requires a `(callbacks skipped)`
  suffix when `--no-callbacks` is used. The implementation prints a Unicode
  arrow and does not vary the success message for `--no-callbacks`.
- Why it matters: this is primarily a command contract and scripting issue. The
  missing suffix also hides whether callbacks were intentionally skipped.
- Affected: manual users, logs, scripts, and tests matching documented output.
- Risk: user-facing, low-to-moderate severity.
- Current verification: tests assert transition success output using current
  Unicode-arrow behavior, and callback skip behavior is covered functionally.

### D-019: `rhei complete` success output uses a Unicode arrow

Source findings: MC-030-B. Classification: `implementation-diverges`.

- Exact mismatch: the complete spec's success output uses ASCII `->`; the
  implementation prints a Unicode arrow.
- Why it matters: exact output contracts matter to scripts and documentation,
  even though the human meaning is clear.
- Affected: command wrappers, logs, users comparing output to docs, and tests
  based on the spec.
- Risk: user-facing, low severity.
- Current verification: completion behavior tests exist, but the audit only
  identified the implementation's current output line, not a spec-conformance
  output test.

### D-020: Result-link de-duplication checks file existence rather than link existence

Source findings: MC-032-B. Classification: `implementation-diverges`.

- Exact mismatch: the spec says `complete` appends the task-body result link if
  that link is not already present. The implementation decides whether to insert
  the link based on whether `runtime/results/<task-id>.md` existed before
  appending. If a result file already exists but the task body lacks the link,
  the link is skipped.
- Why it matters: transition audit files, precreated result files, or partial
  failures can leave results discoverable only in `runtime/`, not from the task
  body.
- Affected: humans reading plans, agents summarizing completed tasks, and
  recovery from partial command failures.
- Risk: user-facing discoverability risk.
- Current verification: unit/integration coverage exists for normal result-link
  insertion before child nodes and assignee removal. The audit did not identify
  a test for preexisting result file without an existing task-body link.

### D-021: `rhei complete` is not atomic across state transition, result append, and task rewrite

Source findings: MC-033-B. Classification: `implementation-diverges`.

- Exact mismatch: the complete spec describes one atomic operation that changes
  state, appends the result entry, removes assignee, links the result file, and
  writes the task file. The implementation first calls `execute_transition`,
  which writes state and releases locks, then appends the result entry and
  rewrites the task body separately.
- Why it matters: a failure after the state transition can leave a task terminal
  while still assigned and without a result link or result message. That state
  is hard for agents and humans to reason about.
- Affected: manual workers, recovery tooling, result readers, and concurrent
  command users.
- Risk: user-facing data consistency risk.
- Current verification: current tests cover successful complete rewrites and
  that completion currently appends only one result entry. The audit did not
  identify failure-injection tests covering partial completion.

### D-022: `rhei reset` does not validate the plan before mutating

Source findings: MC-040-B. Classification: `missing-validation`.

- Exact mismatch: the reset spec says reset refuses to operate on an invalid
  plan. `reset_command` loads the plan and state machine, computes an initial
  state, rewrites task files, clears metadata, and removes runtime output
  without calling `validate_with_machine`.
- Why it matters: reset is destructive. Running it on an invalid plan can erase
  runtime evidence and rewrite state before surfacing semantic problems that
  validation would have caught.
- Affected: humans repairing plans, CI cleanup scripts, and directory workspace
  users.
- Risk: user-facing data-loss/data-integrity risk.
- Current verification: reset tests cover restoring state and removing runtime
  for single-file and workspace plans. Validation refusal is not visibly tested.

### D-023: `rhei reset` does not remove `**Assignee:**`

Source findings: MC-041-A. Classification: `implementation-diverges`.

- Exact mismatch: the reset spec requires removing `**Assignee:**` from every
  task node. The implementation rewrites state lines, strips result links,
  clears visit metadata, and deletes runtime, but there is no assignee-stripping
  step in the reset path.
- Why it matters: after reset, tasks can look freshly initial but still claimed.
  That blocks `next` from picking them and makes a supposedly clean rerun
  depend on stale assignee metadata.
- Affected: anyone rerunning a plan after partial execution, manual agents,
  templates with assignee-bearing tasks, and CI reset workflows.
- Risk: user-facing.
- Current verification: reset tests verify state restoration and runtime
  deletion. The audit did not identify reset tests asserting assignee removal.

### D-024: Tentative: dedicated `rhei list` behavior tests are missing

Source findings: MC-050-B. Classification: `missing-test`.

- Exact mismatch: this is a test-coverage gap, not a proven behavior mismatch.
  The scoped visible e2e coverage for `list` is shell completion of list
  filters, not command behavior for filtering, output shape, source order,
  hierarchy, read-only behavior, or JSON fields.
- Why it matters: `list` is a read-only inspection command with many filters.
  Small regressions can mislead humans about readiness, blocked work, or current
  assignees without changing plan files.
- Affected: humans inspecting plans, scripts using `list --json`, and PMs
  comparing ready/blocked work.
- Risk: internal test risk with user-facing consequences if behavior regresses.
- Current verification: implementation appears to match the specified filter
  surface and output fields, and dynamic completion tests cover list flag
  completion. Behavioral command tests were not visible in the scoped audit.

### D-025: `rhei states` omits schema fields needed by manual agents

Source findings: MC-060-B. Classification: `implementation-diverges`.

- Exact mismatch: the state inspection command renders states, instructions,
  artifacts, visits, model/all_models, and transitions, but omits `gating` in
  both text and JSON. JSON also omits `target`, `all_targets`, `agent`,
  `agent_mode`, `program`, `poll`, `mcp_servers`, `skills`, `profiles`, and
  `node_policy`. Text omits targets, agent/program/poll/tooling/profile data.
- Why it matters: `rhei states` is a manual inspection tool. Omitting gating and
  execution/tooling fields prevents humans and agents from understanding which
  states are human-only, which states execute autonomously, which tools are
  needed, and which profiles apply.
- Affected: manual workers, state-machine authors, PMs inspecting workflows, and
  scripts consuming `states --json`.
- Risk: user-facing.
- Current verification: unit coverage exists for rendering states and
  transitions and JSON including personality. The omitted fields are not
  visibly covered by tests.

### D-026: Specified `rhei viz` CLI is absent; visualization exists only as an `xtask` prototype

Source findings: MC-070-A, MC-070-B, MC-071-A, MC-072-A. Classification:
`implementation-diverges`.

- Exact mismatch: the viz spec requires a `rhei viz [PATH]` subcommand, options,
  a `Commands::Viz` variant, a `viz_command()`, and a `crates/rhei-viz` crate.
  The workspace has no CLI subcommand and no `crates/rhei-viz`. Existing code
  lives under `xtask`, exposes `cargo xtask examples viz`, writes under
  `target/rhei-viz`, and only renders static example HTML.
- Why it matters: users cannot run the specified command or rely on the
  documented options. The prototype is useful but not installed on the normal
  Rhei CLI path.
- Affected: users expecting `rhei viz`, documentation readers, release
  packaging, and anyone wanting static or live plan inspection from the CLI.
- Risk: user-facing feature absence.
- Current verification: prototype code serializes plan data and renders the
  three tabs. There is no shipped CLI integration to test against the spec.

### D-027: Viz prototype derives plan active state more broadly than the spec

Source findings: MC-071-B. Classification: `implementation-diverges`.

- Exact mismatch: the spec derives `active` only from a fixed active-state list
  plus terminal declarations. The prototype marks any non-terminal state other
  than `draft` and the machine initial as active.
- Why it matters: custom machines with waiting, blocked, or human-only
  non-terminal states can render as active even when the spec would classify the
  overall plan as pending or another state.
- Affected: users of the prototype, demos based on custom state machines, and a
  future migration from `xtask` into `rhei viz`.
- Risk: user-facing for the prototype; internal if carried into the real CLI.
- Current verification: the prototype comments say it is dogfooding the future
  command and should stay aligned. The audit did not identify tests for plan
  state derivation against the spec table.

### D-028: CLI diagnostics aggregate has no additional unique mismatch

Source findings: MC-080-C. Classification: `implementation-diverges`.

- Exact mismatch: missing-artifact diagnostics align, but exact no-task, CAS,
  transition success, and complete success diagnostics differ from their command
  specs. These concrete mismatches are elaborated in D-012, D-015, D-018, and
  D-019.
- Why it matters: grouped diagnostic drift makes the CLI feel inconsistent with
  the documented contract and can break scripts that key off expected messages.
- Affected: humans, scripts, tests, and generated docs.
- Risk: user-facing.
- Current verification: JSON-mode command errors and parse/validation
  diagnostics have existing implementation and tests. The exact command text
  mismatches are covered only by tests that assert current implementation
  behavior where such tests exist.

## Areas Where No Discrepancy Was Found

- Role boundaries: the `rhei run` worker prompt preserves orchestrator-owned
  transitions and tells spawned workers not to call `rhei transition` or
  `rhei complete`.
- Worker skill loop: `skills/rhei-plan-worker/SKILL.md` documents the manual
  loop and human gate behavior at a high level.
- Default flow shape: the checked-in default/spec state graph has the expected
  states, terminal markers, gating marker, and transitions, aside from the
  schema-version issue called out above.
- Profile validation: when profiles and node policy are present, the validator
  checks profile initial/allowed states, rejects authored `initial: true`, and
  validates authored task states against resolved profile policy.
- Artifact definition validation: duplicate artifact names, empty paths,
  non-optional outputs, absolute paths, and static `..` escapes are covered.
- Runtime template basics: variable substitution, unknown-variable preservation,
  and simple `{if ...}{else}{endif}` preprocessing are implemented.
- Initial auto-advance: for legacy per-state initial machines, `next` advances
  non-runnable initial states and leaves runnable initial states in place.
- Assignee placeholder behavior: `next` does not write a placeholder assignee
  when no agent resolves.
- Peek implementation path: `next --peek` skips the implemented auto-transition
  and assignee write paths, even though dedicated tests were not visible.
- Missing input diagnostics: missing input artifact errors include task, state,
  artifact name, and path.
- Transition command surface: `rhei transition <RHEI_PLAN> --task --from --to`
  and `--no-callbacks` are exposed.
- Transition core mechanics: CAS re-read under lock, declared-transition
  validation, counted visits, callback execution, target input/source output
  checks, and atomic state writes are implemented.
- Completion command surface: `rhei complete <RHEI_PLAN> --task --result` and
  `--no-callbacks` are exposed.
- Completion target basics: terminal-state rejection, descendant blocking, and
  non-cancelled terminal target selection are implemented.
- Completion rewrite basics: the normal path writes a result entry, removes the
  assignee, and inserts the result link before child nodes.
- Completion entry count today: `complete` currently appends only one result
  entry because `transition` currently appends none.
- Reset basics: `rhei reset` exists, prints the specified two-line shape, strips
  result links, clears visit metadata, and deletes `runtime/`.
- List command surface and behavior: filter flags, conflict constraints,
  comma/repeated state parsing, `--limit`, `--json`, read-only source-order
  traversal, non-cancelled readiness semantics, and text/JSON field shapes align
  with the list spec.
- States command existence: `rhei states` exists and prints a useful subset of
  states, instructions, artifacts, visits, models, and transitions.
- JSON-mode errors: JSON output modes render dispatch errors as JSON objects.
- Parse and validation diagnostics: parse/validation errors include useful file
  and problem context, with visible coverage.

