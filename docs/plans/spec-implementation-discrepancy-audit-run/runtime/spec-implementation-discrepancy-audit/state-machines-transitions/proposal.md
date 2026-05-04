# Reconciliation Proposal: State Machines and Transitions

Source elaboration: `runtime/spec-implementation-discrepancy-audit/state-machines-transitions/elaboration.md`

This proposal names a primary human decision option for each elaborated
discrepancy and records a credible alternative. Decision options use the audit
vocabulary: `update-spec`, `update-implementation`, `update-both`,
`defer-follow-up`, `no-change`.

## SMT-E001: Default schema and shipped defaults are split between current and legacy formats

- Primary decision: `update-both`.
- Next edits: migrate `crates/rhei-validator/src/default-states.yaml` to the
  current profile-based default from `docs/specs/states.yaml`, including
  version `3.0`, transition descriptions, `profiles`, and `node_policy`. Update
  shipped examples, templates, and plan-writer references that still teach
  state-level `initial: true`. Keep legacy machine loading as an explicit
  compatibility path, but document it in `docs/specs/rhei-states.spec.md` as
  deprecated input that new authored machines must not emit. Remove the silent
  `transitions: []` default for strict/current machines, or gate it behind the
  same legacy compatibility path.
- Expected tests: add a fixture test that compares the compiled built-in
  default with `docs/specs/states.yaml` at the semantic level. Add validator
  tests that current-schema machines require `profiles`, `node_policy`, and
  `transitions`, plus compatibility tests proving legacy fixtures still load
  only through the intended fallback.
- Reason preferable: users should see one default machine across CLI output,
  docs, skills, and examples, while existing legacy plans still need a
  controlled migration path.
- Alternative: `update-implementation` to reject all legacy machines
  immediately. This is cleaner technically, but it risks breaking existing
  plans and templates without a transition window.

## SMT-E002: Profile and node-policy semantics are only partially implemented

- Primary decision: `update-implementation`.
- Next edits: replace the string `overrides[].match` model with the specified
  structured matcher containing optional `type` and `level`, preserving a
  legacy parser path only if needed for old fixtures. Add a shared helper that
  resolves a node's profile through `node_policy` and use it in validation,
  `rhei reset`, `rhei next`, authored-state checks, and initial-state
  auto-advance. Add profile graph validation for at least one final state and
  a path from every non-final allowed state to a final allowed state. Move
  `by_type` and override `type` / `level` checks into plan-aware validation
  where the node structure is available.
- Expected tests: add validator tests for structured overrides, invalid match
  keys, unknown node kinds, invalid levels, profiles without finals, and
  dead-end profile graphs. Add CLI tests where root/default/kind/level
  policies resolve to different initial states for `reset` and `next`.
- Reason preferable: profiles are the current spec's policy layer. Implementing
  the full resolver prevents accepted machines from being unrepresentable,
  dead-ended, or reset through the wrong legacy initial state.
- Alternative: `update-both` to scale back node policy to id-pattern overrides
  and a machine-wide initial state. This would match current code more closely
  but would remove the main reason profiles exist.

## SMT-E003: Required state and transition descriptions are not enforced or preserved

- Primary decision: `update-implementation`.
- Next edits: make state `description` required and non-empty for current
  schema machines. Add `description: Option<String>` or a required string to
  `TransitionRule`, preserve it through parsing/serialization, and require
  non-empty transition descriptions in strict/current validation. Keep any
  legacy relaxation explicitly scoped to legacy machines.
- Expected tests: add load-failure tests for missing or empty state
  descriptions and transition descriptions. Add a round-trip or CLI `rhei
  states --json` test proving transition descriptions are preserved.
- Reason preferable: descriptions are part of the authoring, monitoring, and
  writer contract. Dropping them silently makes generated workflows harder to
  inspect.
- Alternative: `update-spec` to make descriptions recommended rather than
  required. This would reduce validation churn but would weaken monitoring and
  generated-machine quality.

## SMT-E004: Target, model, and gating validation does not match resolution rules

- Primary decision: `update-implementation`.
- Next edits: introduce a settings-aware validation pass that resolves
  `target`, `all_targets`, `agent`, `agent_mode`, `model`, and `all_models`
  against the merged registries used at run time. Add a validator warning
  channel, or a structured diagnostics type, so `agent` on a gating state can
  be reported as a warning while program/tooling on gates remain errors. Route
  template context construction through the same model resolver so
  `{model.provider}` and `{model.name}` come from settings-backed model
  profiles, not just inline selector pieces.
- Expected tests: add settings-backed validation tests for unknown target
  agents, unknown modes, invalid model ids, and gating-state agent warnings.
  Add template rendering tests where the model profile id differs from the
  provider model name.
- Reason preferable: execution identity errors should be reported before a
  workflow starts, and template variables should describe the actual resolved
  execution model.
- Alternative: `update-both` to define registry resolution as run-time only
  and remove the gating-agent warning requirement. This is simpler, but it
  delays common authoring mistakes until execution.

## SMT-E005: Final and gating states are not uniformly absorbing

- Primary decision: `update-implementation`.
- Next edits: in `execute_transition`, reject any source state whose base state
  is `final: true` before exact or wildcard transition matching. In
  `complete_command`, reject a current state with `gating: true` before calling
  `find_completion_state`, with a diagnostic that only explicit human
  `rhei transition` may leave a gate.
- Expected tests: add manual transition tests proving a wildcard cannot move a
  task out of a final state. Add `rhei complete` tests where `human-review`
  has a direct transition to `completed` but completion exits non-zero and
  leaves the task unchanged.
- Reason preferable: final states are the workflow closure boundary and gates
  are the human authority boundary. Both should be protected in the mutation
  commands, not only in ready-task discovery.
- Alternative: `update-spec` to allow explicit manual transitions from finals
  and completion from gates. This would make recovery easier, but it weakens
  the two strongest safety invariants in the state model.

## SMT-E006: Counted visit accounting is not scoped per target or model fanout

- Primary decision: `update-both`.
- Next edits: define the persisted fanout counter shape in the spec, then
  implement it consistently in `ensure_current_state_visit_count`,
  `update_metadata_for_transition`, state rendering, condition evaluation, and
  artifact/template resolution. A practical shape is to keep the existing
  scalar `stateVisits.<state>` for non-fanout states and add a scoped map such
  as `stateVisitsByTarget.<state>.<target_slug>` or
  `stateVisitsByModel.<state>.<model_id>` for fanout states.
- Expected tests: add multi-target and multi-model counted-loop tests where
  one fanout member loops or exhausts without consuming another member's
  budget. Include artifact paths using `{visit_count}` and condition operands
  using `visitCount`.
- Reason preferable: implementation needs a concrete metadata contract before
  it can make fanout counters observable and stable for callbacks and future
  tooling.
- Alternative: `update-spec` to define visits as task-wide even for fanout
  states. This would preserve the current metadata shape but makes independent
  fanout review budgets impossible.

## SMT-E007: Polling states are validated but not implemented as scheduling semantics

- Primary decision: `update-implementation`.
- Next edits: wire `poll:` into `rhei run`: persist `pollNextAttemptAt`,
  record attempts in `stateVisits`, exclude tasks with future poll deadlines
  from the ready set, release `--parallel` slots between attempts, expose
  `pollAttempts` and `pollMaxAttempts` condition operands, prevent self-loop
  retries after exhaustion, select the first matching non-self-loop exhaustion
  route, and clear poll metadata on non-self-loop exit. Update `examples/ci-heal`
  once runtime behavior is real.
- Expected tests: add short-interval runtime tests for first attempt,
  self-loop scheduling delay, no busy loop before `pollNextAttemptAt`,
  exhaustion routing, missing exhaustion route diagnostics, cleanup on exit,
  and concurrent poll tasks.
- Reason preferable: the schema and examples already present polling as an
  available feature. Implementing the runtime semantics closes the hazardous
  gap between "valid YAML" and executable behavior.
- Alternative: `defer-follow-up` by marking `poll:` experimental and
  non-executable in the specs and examples until runtime work is scheduled.
  This is credible for a near-term release, but it should include visible
  warnings because validation currently accepts poll states.

## SMT-E008: Artifact enforcement order and template namespaces differ from spec and examples

- Primary decision: `update-both`.
- Next edits: update the artifact-order prose in
  `docs/specs/rhei-transitions.spec.md` to match the safer runtime sequence:
  run `on_leave`, resolve redirects, check source outputs and target inputs,
  commit the state write, then run `on_enter` with rollback on failure. Then
  update shipped examples and templates to use the implemented namespace:
  `{task_id}`, `{visit_count}`, and `{meta.<key>}` instead of `{task.id}`,
  `{visit}`, and `{task.metadata.<key>}`. Add a docs note that unknown
  variables fail open and should be caught by example smoke tests.
- Expected tests: add transition tests that lock in callback/artifact ordering
  and rollback behavior. Add example/template rendering tests that fail when
  shipped examples contain unresolved stale variables from known old forms.
- Reason preferable: the current callback order avoids running target-entry
  side effects before entry artifacts are valid, and the stale examples are
  clearly the wrong side of the namespace mismatch.
- Alternative: `update-implementation` to delay source-output checks until
  after `on_enter`. This follows the current prose literally, but makes
  `on_enter` run after an invalid source handoff and increases rollback
  complexity.

## SMT-E009: System-triggered transitions lack trigger classification and payloads

- Primary decision: `update-implementation`.
- Next edits: extend `execute_transition` to accept a trigger classification
  and initial `transitionData`. Pass `triggeredBy: "system"` for program
  exit-code routes, agent timeout routes, and tooling-unavailable routes.
  Populate timeout data with timeout duration and agent id. Add run-time
  selection for `mcp_unavailable` and `skill_unavailable` transitions before
  spawning an agent when required tooling is unavailable.
- Expected tests: add callback-context tests for program exit-code,
  agent-timeout, and tooling-unavailable transitions. Assert
  `transition.triggeredBy == "system"` and the expected payload shape in
  `transitionData`.
- Reason preferable: callbacks and monitoring need to distinguish manual
  operator action from engine/system recovery paths.
- Alternative: `update-spec` to classify all CLI-mediated transitions as
  `user`. This would match current callback context, but it loses useful audit
  information and contradicts the transition trigger model.

## SMT-E010: Callback platform, mapping, preflight, and error handling support are narrower than spec

- Primary decision: `update-both`.
- Next edits: split the transition callback spec into a supported v1 surface
  and future platform goals. For v1, document `cli:` callbacks as the only
  executable platform unless JS/Python/Java work is actually scheduled. Update
  `StateMachine` validation to reject or clearly warn on unsupported
  top-level `callbacks:` mappings and `error_handling:` instead of silently
  ignoring them. Add preflight validation for `cli:` callback commands that
  can be checked without executing workflow logic.
- Expected tests: add validation tests for unsupported callback platforms,
  ignored top-level callback mappings, and malformed `error_handling:` blocks.
  Add CLI callback preflight tests for missing command files or unavailable
  executables where the callback reference is statically checkable.
- Reason preferable: the current spec is much broader than the available
  runtime and language bindings. Making the supported v1 contract explicit
  prevents users from authoring valid-looking machines that cannot execute.
- Alternative: `update-implementation` to implement top-level mappings,
  JavaScript, Python, Java, and full error policies now. This is the eventual
  parity path, but it is a large cross-runtime project rather than a small
  reconciliation.

## SMT-E011: Callback TransitionContext omits active state and counted-loop metadata

- Primary decision: `update-implementation`.
- Next edits: extend `build_transition_context_json` to include a top-level
  `state` object containing the active state's name, description,
  instructions, personality, `final`, `gating`, `visits`, and resolved
  `inputs` / `outputs` with `exists` flags. Synthesize
  `task.metadata.visitCount` for counted states from the same visit counter
  used by transition conditions.
- Expected tests: add callback tests that inspect `ctx.state.inputs`,
  `ctx.state.outputs`, artifact existence values, `ctx.state.visits`, and
  `ctx.task.metadata.visitCount` during a counted-loop transition.
- Reason preferable: callback authors should not need to reconstruct the
  active state definition or visit count from unrelated metadata.
- Alternative: `update-spec` to remove `ctx.state` and `visitCount` from the
  callback contract. This would match the current payload but significantly
  reduces callback usefulness.

## SMT-E012: Concurrent scheduling semantics are implemented for agent batches only

- Primary decision: `update-both`.
- Next edits: align this with the broader scheduler work by applying
  `StateDef.concurrent` to every autonomous scheduled work unit that can run in
  parallel, including agents, programs, and poll attempts. Clarify in
  `docs/specs/rhei-states.spec.md` that callback-only progression remains
  sequential unless it is represented as scheduled autonomous work, and that
  fanout invocations from one task are not split by `concurrent`.
- Expected tests: add `rhei run --parallel` tests for two ready program tasks
  in a non-concurrent state, two program tasks in a concurrent state, and poll
  tasks whose deadlines have elapsed. Keep existing agent batch tests.
- Reason preferable: users should not need to know which implementation batch
  a state falls into to predict the effect of `concurrent`.
- Alternative: `update-spec` to say `concurrent` applies only to agent tasks.
  This matches the current implementation, but it makes the field less useful
  and conflicts with polling and program-state prose.

## SMT-E013: State-machine-writer output guidance can produce runtime-incompatible shapes

- Primary decision: `update-both`.
- Next edits: update the state-machine-writer spec and skill examples after
  the runtime changes above land, especially structured node-policy overrides,
  transition descriptions, profile reachability, and callback support. Until
  unsupported callback mappings are implemented, have the writer emit only the
  supported callback form or mark broader callback examples as future. Remove
  stale skeleton wording that tells autonomous actors "when to transition out"
  under orchestrator authority.
- Expected tests: add a writer-output fixture or golden example and validate it
  through the current CLI. Include a fixture with `overrides[].match`,
  transition descriptions, profiles, gates, and supported callbacks.
- Reason preferable: the writer is a user-facing generator. Its output should
  be valid against the same runtime users will execute.
- Alternative: `update-spec` to make the writer emit only today's legacy
  runtime-compatible subset. This would avoid runtime work, but it would force
  the writer to lag behind the current state-machine design.

## SMT-E014: Raw template state machines with Mustache placeholders are not valid before instantiation

- Primary decision: `update-spec`.
- Next edits: clarify in the template and state-machine docs that raw
  `.agents/rhei/templates/**/states.yaml` files are templates, not direct
  `rhei states --state-machine` inputs, unless the template explicitly opts in
  to raw validation. Require rendered template instances to pass normal
  state-machine validation. Add authoring guidance to quote placeholders when
  possible to keep raw YAML parseable for editors, while still treating semantic
  validation as post-instantiation.
- Expected tests: add template-instantiation smoke tests that render bundled
  templates with representative values and run state-machine validation on the
  rendered files. If raw YAML parseability is desired, add a separate lint that
  parses templates with placeholder-safe substitution rather than ordinary
  state-machine loading.
- Reason preferable: Mustache templates are not concrete state machines until
  their variables are bound. Validating the rendered output gives users the
  meaningful guarantee without constraining template syntax unnecessarily.
- Alternative: `update-implementation` to add a `rhei states --template` mode
  that substitutes dummy values before validation. This could improve template
  authoring, but it should be a template-tooling feature rather than changing
  ordinary state-machine validation.
