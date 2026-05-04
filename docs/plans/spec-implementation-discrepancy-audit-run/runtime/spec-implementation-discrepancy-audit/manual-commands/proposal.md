# Reconciliation Proposal: Manual Worker and Inspection Commands

Source elaboration: `runtime/spec-implementation-discrepancy-audit/manual-commands/elaboration.md`

This proposal names a primary human decision option for each elaborated
discrepancy and records the most credible alternative. Decision options use the
audit vocabulary: `update-spec`, `update-implementation`, `update-both`,
`defer-follow-up`, `no-change`.

## D-001: `rhei next` help describes old transition semantics

- Primary decision: `update-implementation`.
- Next edits: update the clap doc comments for `Commands::Next` in
  `crates/rhei-cli/src/main.rs` so command help says `next` claims a task,
  writes `**Assignee:**` when an agent resolves, and does not advance state
  except for the existing non-runnable initial-state auto-advance path.
- Expected tests: add or update a CLI help assertion covering `rhei next --help`
  and the top-level subcommand summary.
- Reason preferable: the current spec reflects the intended manual-worker
  contract and the implementation behavior is closer to that contract than the
  help text is. Fixing help removes a user-facing lie without changing command
  behavior.
- Alternative: `update-spec` to describe the old "transition next ready task"
  wording. This is not preferable because it would contradict the newer claim
  contract and the worker/orchestrator boundary.

## D-002: `rhei complete` can exit gating states

- Primary decision: `update-implementation`.
- Next edits: in `complete_command`, reject the task when the normalized
  current state has `gating: true`, before selecting the completion target.
  The diagnostic should mention that gating states require explicit human
  `rhei transition`.
- Expected tests: add an e2e or integration test for a task in `human-review`
  where `rhei complete` exits non-zero, leaves the state unchanged, preserves
  the assignee/result state, and does not append a result entry.
- Reason preferable: gating is the safety boundary for human approval. The spec
  and `rhei run` behavior already agree that autonomous completion must stop
  there.
- Alternative: `update-both` to clarify that only an explicit human transition
  may leave a gating state. This is useful wording, but it does not replace the
  required implementation guard.

## D-003: Commands still depend on legacy per-state `initial`

- Primary decision: `update-implementation`.
- Next edits: add a shared state-machine helper that resolves a node's profile
  through `node_policy` and returns that profile's initial state. Use it in
  `rhei next` initial/auto-advance decisions and in `rhei reset` per-node
  reset logic. Keep legacy `StateDef.initial` only as compatibility fallback
  when `profiles`/`node_policy` are absent.
- Expected tests: add profile-only machines for `next` and `reset`, including a
  machine with different root/default or kind profiles. Assert claimability,
  non-runnable initial auto-advance, and reset-to-profile-initial behavior.
- Reason preferable: this makes the runtime honor the current states spec and
  allows machines authored from the spec to work without legacy fields.
- Alternative: `update-spec` to reintroduce per-state `initial` as normative.
  This would simplify the CLI but would undo the profile design and leave
  mixed-profile workspaces underspecified.

## D-004: Built-in defaults and templates still author legacy initial schema

- Primary decision: `update-both`.
- Next edits: migrate `crates/rhei-validator/src/default-states.yaml`,
  `docs/specs/states.yaml`, `.agents/rhei/templates/**/states.yaml`,
  `examples/**/states.yaml`, and skill/template references to
  `profiles`/`node_policy`. Add a short compatibility note to
  `docs/specs/rhei-states.spec.md` explaining that machines without profiles
  may still load for backward compatibility, but new authored machines must use
  profiles.
- Expected tests: update fixture state machines, ensure validator tests still
  cover legacy compatibility, and add a check that shipped defaults/templates
  validate without per-state `initial: true`.
- Reason preferable: users should not learn a deprecated schema from shipped
  examples, and the compatibility behavior should be explicit rather than
  accidental.
- Alternative: `update-spec` to bless both schemas equally. This is less
  preferable because it preserves two competing authoring styles and weakens
  profile-based reset semantics.

## D-005: Runtime template variables for `model.provider` and `model.name` use the wrong namespace

- Primary decision: `update-implementation`.
- Next edits: route runtime template model variables through settings-backed
  model profile resolution. `{model}` should remain the selected model profile
  or selector identity, while `{model.provider}` and `{model.name}` should
  resolve to the provider id and provider model name for that profile. Define
  explicit fallback behavior for inline target selectors that do not reference a
  model profile.
- Expected tests: add template-render tests where the model profile id differs
  from provider model name, and where provider differs between two profiles.
  Cover instructions, personality, and artifact paths.
- Reason preferable: rendered instructions and artifact paths are consumed by
  humans and agents; they need provider/model values that match actual
  execution settings.
- Alternative: `update-spec` to define `{model.provider}` and `{model.name}` as
  the raw inline selector pieces. That would be simpler but would make
  model-profile settings less useful and surprise users of settings-backed
  machines.

## D-006: `rhei next` text and JSON output do not match specified shapes

- Primary decision: `update-both`.
- Next edits: update the spec to define a stable core JSON object plus allowed
  optional detail fields, and update implementation to always include the
  spec-required fields, including `model_provider` and `model_name` when a
  model resolves. For text mode, make the first lines match the documented
  `Task <ID>: <title>` and `State: <state>` shape, then document any additional
  task-content/children detail block if it remains.
- Expected tests: add text-output snapshots for claim and peek mode, JSON shape
  tests for agent/model fields, and compatibility tests that extra JSON fields
  do not replace required fields.
- Reason preferable: the current rich output is useful for manual agents, but
  automation needs a documented stable core. Updating both avoids a breaking
  reduction to bare output while fixing the missing contract fields.
- Alternative: `update-implementation` to exactly match the current spec and
  remove undocumented fields. This is simpler to reason about but could remove
  useful context already consumed by manual agents.

## D-007: `rhei next` usage/options table omits implemented flags

- Primary decision: `update-spec`.
- Next edits: update `docs/specs/rhei-next.spec.md` usage to include
  `[--task <ID>] [--json] [--no-callbacks] [--peek]`, and add those flags to
  the options table with their existing behavior.
- Expected tests: no runtime test is required beyond existing clap/e2e coverage;
  a docs lint or spec audit check can verify option-table completeness if such
  a check is added later.
- Reason preferable: the CLI already exposes and tests these flags. The spec
  table is stale.
- Alternative: `update-implementation` to hide the undocumented flags. This
  would remove useful automation surfaces and conflict with existing tests.

## D-008: Auto-pick `rhei next` only claims legacy initial-state tasks

- Primary decision: `update-implementation`.
- Next edits: change automatic `next` selection to consider every ready,
  unassigned, non-terminal, non-gating task, regardless of whether its state is
  initial. Keep initial-state auto-advance as a post-selection behavior only
  for selected non-runnable initial states.
- Expected tests: add e2e tests where an unassigned `pending` or
  `agent-review-fix` task with satisfied priors is selected by default `next`.
  Add a negative test for claimed tasks and gating states.
- Reason preferable: this matches the claimability rule and makes resumable
  mid-workflow work visible to manual workers.
- Alternative: `update-spec` to limit default `next` to initial states and
  require `--task` for mid-workflow work. This would preserve current behavior
  but makes the command less useful for recovery and manual operation.

## D-009: Cancelled-prerequisite semantics conflict across specs

- Primary decision: `update-spec`.
- Next edits: update `docs/specs/rhei-next.spec.md` and relevant
  `docs/specs/rhei-usage.spec.md` prose so prerequisite satisfaction means
  terminal and non-cancelled, matching `rhei list --ready` and implementation
  behavior.
- Expected tests: retain the existing e2e coverage that cancelled
  prerequisites do not unblock dependents, and add a spec-derived test name if
  needed for visibility.
- Reason preferable: a cancelled prerequisite usually means the required work
  did not happen. Blocking dependents is the safer dependency default and
  already matches implementation/list behavior.
- Alternative: `update-implementation` to treat cancellation as satisfying
  dependencies. That is easier to align with the current `next` text but risks
  starting downstream work after an unmet prerequisite.

## D-010: Claim mode does not re-read and revalidate claimability under one claim lock

- Primary decision: `update-implementation`.
- Next edits: refactor claim mode so selection, lock acquisition, re-read,
  claimability validation, input checks, and assignee insertion happen inside
  one locked critical section for the target task file/index. If the candidate
  becomes unclaimable under lock, either reselect under the fresh snapshot or
  fail with an explicit claim conflict; do not print a claim unless the
  assignee was actually written.
- Expected tests: add a concurrent claim test with two `rhei next` invocations
  against the same task. Assert only one receives a claim and the loser either
  selects another task or reports no claim.
- Reason preferable: `rhei next` is the manual claim primitive. Its output must
  be a reliable ownership signal under concurrency.
- Alternative: `update-spec` to document best-effort claiming and no-op
  duplicate assignment. This would be simpler but would make parallel manual
  worker behavior unpredictable.

## D-011: Dedicated peek-mode behavior coverage is not visible

- Primary decision: `update-implementation`.
- Next edits: add tests only. Cover `rhei next --peek` read-only behavior for
  no assignee write, no state rewrite, no result append, no callbacks, and
  missing-input parity with claim mode.
- Expected tests: new e2e tests in the `next` suite using before/after file
  contents and callback sentinels.
- Reason preferable: the implementation appears guarded today, so the useful
  reconciliation is regression coverage rather than behavior change.
- Alternative: `no-change` if maintainers consider existing broad `next` tests
  sufficient. This leaves a high-value read-only guarantee implicit.

## D-012: No-claimable diagnostics do not implement the three specified summaries

- Primary decision: `update-both`.
- Next edits: update implementation to emit the three documented summaries for
  all-terminal, gating/human-action, and all-claimed/in-flight cases. Update
  the spec to also document the existing blocked-by-prerequisites case, because
  it is a real fourth no-claimable condition not covered by the current table.
- Expected tests: add a no-claimable diagnostic matrix for terminal, gating,
  claimed/in-flight, blocked-prerequisite, and empty-plan cases.
- Reason preferable: humans need the three operationally important categories,
  but the spec should not force the CLI to collapse blocked dependency plans
  into one of those categories.
- Alternative: `update-implementation` to implement only the current three-row
  table. This is more literal but would lose the useful blocked-prerequisite
  diagnostic.

## D-013: Backtick-wrapped CLI state values are not accepted by `rhei transition`

- Primary decision: `update-implementation`.
- Next edits: parse/normalize `--from` and `--to` through the same state-value
  rules used for markdown before calling `machine.is_valid_state()` and before
  CAS/transition matching. Preserve clear diagnostics for malformed backtick
  values.
- Expected tests: add transition CLI tests with states such as ``in review`` in
  markdown form, passing both raw shell-safe values and backtick-wrapped values
  where the spec allows them.
- Reason preferable: the main plan grammar already supports non-identifier
  state names. The CLI should accept the documented rendering of those values.
- Alternative: `update-spec` to require raw canonical state names on the CLI.
  This would be easier to implement but would make the transition spec
  inconsistent with the main state-value rendering rules.

## D-014: `rhei transition` does not validate the whole plan before mutating

- Primary decision: `update-implementation`.
- Next edits: call `validate_with_machine` in `transition_command` before
  mutation, and consider re-running the semantic validation on the locked fresh
  snapshot before writing. Workspace mode should validate the merged graph, not
  only the task file.
- Expected tests: add invalid-plan fixtures where `rhei transition` exits
  non-zero and leaves task state, metadata, callbacks, and result files
  unchanged.
- Reason preferable: transition is a state mutation command; it should not make
  a semantically invalid plan harder to repair.
- Alternative: `update-spec` to document that transition performs only local
  parse/CAS/edge validation. This may help recovery workflows, but it weakens a
  command invariant shared by the other coordination commands.

## D-015: CAS conflict text differs from the specified text

- Primary decision: `update-spec`.
- Next edits: update `docs/specs/rhei-transition-cmd.spec.md` to use the
  current concise diagnostic shape, or explicitly state that the exact prose is
  non-normative while the required data are task id, actual state, and expected
  state.
- Expected tests: keep the existing conflict test asserting current behavior;
  add a more semantic assertion if the test suite supports matching required
  fields instead of exact full text.
- Reason preferable: the implementation message is actionable and already
  tested. This is low-severity textual drift, not a behavior issue.
- Alternative: `update-implementation` to exactly print the current spec text.
  This improves literal conformance but churns tests and scripts for little
  user benefit.

## D-016: Transition output-check ordering conflicts between specs

- Primary decision: `update-spec`.
- Next edits: update `docs/specs/rhei-transition-cmd.spec.md` so `on_leave`
  runs before source output checks, matching `docs/specs/rhei-states.spec.md`
  and current implementation. State explicitly that `on_leave` may create
  required outputs before the transition commits.
- Expected tests: add an artifact/callback transition test where `on_leave`
  creates the required output and the transition succeeds; add a missing-output
  failure test after a callback that does not create the output.
- Reason preferable: allowing callbacks to materialize outputs is useful and is
  already the runtime behavior described by the states spec.
- Alternative: `update-implementation` to check outputs before callbacks. This
  is stricter but would make output-producing callbacks impossible.

## D-017: `rhei transition` does not append transition entries to result files

- Primary decision: `update-implementation`.
- Next edits: append a `runtime/results/<task-id>.md` heading entry for
  successful user-invoked `rhei transition`. Refactor shared transition code so
  `rhei complete` still appends exactly one completion entry and does not
  double-record. Preserve `**Assignee:**` unchanged.
- Expected tests: add transition tests for result-file creation, append order
  across multiple transitions, no message body for transition entries, assignee
  preservation, and no append on failed CAS/validation/artifact checks.
- Reason preferable: result files are the specified audit trail. Missing
  intermediate entries makes review/fix loops hard to reconstruct.
- Alternative: `update-spec` to say only `rhei complete` writes result files.
  This is simpler but loses the documented audit trail and conflicts with the
  complete spec's result-file format.

## D-018: `rhei transition` success output differs from the spec

- Primary decision: `update-implementation`.
- Next edits: change transition success text to ASCII `->` and add
  ` (callbacks skipped)` when `--no-callbacks` is passed.
- Expected tests: update existing transition output assertions and add an
  explicit `--no-callbacks` success-output assertion.
- Reason preferable: the callback-skipped suffix carries operational meaning in
  logs, and ASCII output avoids exact-match drift against the command spec.
- Alternative: `update-spec` to bless the Unicode arrow and omit the suffix.
  This would reduce churn but keep an important callback signal invisible.

## D-019: `rhei complete` success output uses a Unicode arrow

- Primary decision: `update-implementation`.
- Next edits: change the complete success line to use ASCII `->`, matching
  `docs/specs/rhei-complete.spec.md`.
- Expected tests: add or update completion output assertions for successful
  single-file and workspace completion.
- Reason preferable: this is a small, low-risk command contract fix that keeps
  scripts and docs aligned.
- Alternative: `update-spec` to accept the Unicode arrow. This preserves
  current output but creates unnecessary exact-output divergence between
  transition and complete.

## D-020: Result-link de-duplication checks file existence rather than link existence

- Primary decision: `update-implementation`.
- Next edits: make completion decide result-link insertion by scanning the task
  body for an existing `> **Result:**` link for that task, not by checking
  whether the result file already existed.
- Expected tests: add a fixture with a preexisting
  `runtime/results/<task-id>.md` and no task-body result link. Assert
  completion appends the message and inserts the link exactly once.
- Reason preferable: the task body is the discoverability surface. A
  precreated or transition-created result file should not suppress the visible
  link.
- Alternative: `update-spec` to document file-existence-based insertion. This
  would encode a recovery bug into the contract.

## D-021: `rhei complete` is not atomic across state transition, result append, and task rewrite

- Primary decision: `update-both`.
- Next edits: refactor completion so state rewrite, assignee removal, result
  link insertion, and result append are serialized under the same command-level
  lock where practical. Update the spec to avoid promising crash-atomic updates
  across multiple files unless a journal/transaction mechanism is implemented;
  define the intended invariant and recovery behavior instead.
- Expected tests: add failure-injection or unit-level tests for errors between
  transition, result append, and task rewrite. Assert either no visible partial
  mutation or a documented recoverable state. Add concurrency tests that a
  second command cannot observe an unlocked half-completed task.
- Reason preferable: true atomicity across the task file and result file is
  stronger than the current implementation can honestly guarantee. Updating
  both gives users a real consistency improvement and a truthful spec.
- Alternative: `update-implementation` to implement a full journaled
  transaction while leaving the spec unchanged. This is stronger but more
  complex and should be justified by data-loss risk.

## D-022: `rhei reset` does not validate the plan before mutating

- Primary decision: `update-implementation`.
- Next edits: run `validate_with_machine` after loading the plan and before
  computing reset targets or deleting `runtime/`. The command should fail
  without mutation on semantic validation errors.
- Expected tests: add reset tests with invalid state/prior/profile data and
  preexisting runtime output. Assert the command refuses the plan and leaves
  files/runtime unchanged.
- Reason preferable: reset is destructive. Validation-before-mutation prevents
  accidental loss while repairing an invalid plan.
- Alternative: `update-spec` to permit reset as a recovery command for invalid
  plans. This might be useful later, but it should be a separate explicit
  `--force`/repair design rather than default behavior.

## D-023: `rhei reset` does not remove `**Assignee:**`

- Primary decision: `update-implementation`.
- Next edits: update the reset rewrite path to strip `**Assignee:**` metadata
  from every reset task node in both single-file plans and directory workspace
  task files.
- Expected tests: add reset fixtures with assigned root tasks and child tasks.
  Assert all assignee lines are gone and subsequent `rhei next` can claim the
  reset task.
- Reason preferable: reset should return the plan to a clean pre-execution
  state. Stale assignees make the reset unusable for reruns.
- Alternative: `update-spec` to preserve assignees across reset. This would
  support "same owner reruns" workflows but conflicts with the clean-rerun
  purpose and current command prose.

## D-024: Dedicated `rhei list` behavior tests are missing

- Primary decision: `update-implementation`.
- Next edits: add behavior tests for `rhei list` filters, source-order
  hierarchy, `--ready`/`--blocked`, conflict flags, text output, JSON fields,
  empty result output, and read-only behavior.
- Expected tests: new e2e list suite plus at least one workspace fixture. Check
  file contents before/after to confirm no mutation.
- Reason preferable: the audit found no behavior mismatch, so tests are the
  right reconciliation. `list` drives human inspection and should be protected
  from quiet regressions.
- Alternative: `no-change` because the implementation appears aligned today.
  This saves test work but leaves a broad command surface unguarded.

## D-025: `rhei states` omits schema fields needed by manual agents

- Primary decision: `update-implementation`.
- Next edits: extend `states_command` text and JSON renderers to include
  `gating`, `target`, `all_targets`, `agent`, `agent_mode`, `program`, `poll`,
  `mcp_servers`, `skills`, `profiles`, and `node_policy`. Keep text output
  progressively disclosed and readable, but make JSON complete enough for
  agent tooling.
- Expected tests: update `states --json` assertions for all schema fields and
  text assertions for gating/execution/tooling/profile summaries.
- Reason preferable: `rhei states` is the manual inspection command for the
  state machine. Omitting gating and execution/tooling fields prevents workers
  from understanding safe transitions and required runtime context.
- Alternative: `update-spec` to document `rhei states` as a summary-only
  command. This would preserve current output but make the command less useful
  for manual agents.

## D-026: Specified `rhei viz` CLI is absent; visualization exists only as an `xtask` prototype

- Primary decision: `defer-follow-up`.
- Next edits: create a dedicated `rhei viz` implementation plan or feature
  task that covers `Commands::Viz`, a `crates/rhei-viz` crate, static HTML
  rendering, optional `--serve`, CLI options, packaging, and docs. Until that
  lands, mark the viz spec or release docs as planned/unreleased if users might
  read it as shipped.
- Expected tests: follow-up should add CLI parse tests, static output tests
  proving a self-contained HTML file is written, workspace discovery tests, and
  `--serve` tests behind the relevant cargo feature.
- Reason preferable: this is a whole feature, not a small command-contract
  correction. A separate implementation plan keeps the audit actionable without
  under-scoping a new crate and browser UI.
- Alternative: `update-implementation` immediately. That is the final desired
  state if the project wants to ship `rhei viz` now, but it is larger than the
  rest of this manual-command reconciliation batch.

## D-027: Viz prototype derives plan active state more broadly than the spec

- Primary decision: `update-implementation`.
- Next edits: update the `xtask` visualization prototype's plan-state
  derivation to use the table in `docs/specs/rhei-viz.spec.md`: fixed active
  list plus terminal-state declarations, with the documented fallback to
  `pending`.
- Expected tests: add prototype/unit tests for all plan-state derivation rows,
  including custom non-terminal waiting/gating states that should not render as
  `active`.
- Reason preferable: even if the shipped CLI is deferred, the prototype is a
  migration source. Aligning its classifier now reduces future carry-over bugs.
- Alternative: `defer-follow-up` and fix this only when `rhei viz` is promoted
  from `xtask` into the CLI. This avoids prototype churn but lets demos remain
  misleading.

## D-028: CLI diagnostics aggregate has no additional unique mismatch

- Primary decision: `no-change`.
- Next edits: no separate code or spec edit. Track the concrete diagnostic
  reconciliations under D-012, D-015, D-018, and D-019.
- Expected tests: rely on the diagnostic tests proposed for those concrete
  discrepancies; do not add a duplicate aggregate test unless a future
  diagnostics conformance suite is created.
- Reason preferable: the aggregate finding is useful as a summary but does not
  identify an independent contract to change. Duplicating edits here would make
  ownership less clear.
- Alternative: `update-both` to create a single CLI diagnostics contract table
  and conformance test suite covering every command. That could be valuable
  later, but it is broader than this unique mismatch.
