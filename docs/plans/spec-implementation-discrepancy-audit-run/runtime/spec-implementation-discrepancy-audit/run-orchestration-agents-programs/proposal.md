# Reconciliation Proposal: Run Orchestration, Agents, Programs, and Callbacks

Source elaboration: `runtime/spec-implementation-discrepancy-audit/run-orchestration-agents-programs/elaboration.md`

This proposal names a primary human decision option for each elaborated
discrepancy and records a credible alternative. Decision options use the audit
vocabulary: `update-spec`, `update-implementation`, `update-both`,
`defer-follow-up`, `no-change`.

## D-001: Unreachable autonomous states select autonomous run mode

- Primary decision: `update-implementation`.
- Next edits: change `run_command` mode selection to inspect only states that
  are reachable from at least one non-terminal, non-gating task in the current
  plan. Add a helper that walks state transitions from each active task state
  and ignores stale state definitions that no task can enter.
- Expected tests: add an e2e test with a callback-only plan whose state machine
  contains an unreachable `model` / `program` state; assert `rhei run` stays in
  callback mode and does not require an agent or create autonomous runtime
  artifacts. Keep the existing missing-agent test for reachable autonomous
  states.
- Reason preferable: the spec's reachable-state rule supports reusable state
  machines with optional autonomous branches. Matching that rule avoids
  surprising users with agent-mode errors caused by dead configuration.
- Alternative: `update-spec` to define mode selection as a whole-machine scan.
  This would match the current implementation, but it makes broad reusable
  machines brittle and turns unreachable definitions into runtime behavior.

## D-002: Ready-set selection ignores current-state inputs

- Primary decision: `update-implementation`.
- Next edits: route `find_ready_tasks` through shared artifact input checking
  for the task's current state. Prefer a non-mutating helper that returns the
  missing required inputs so no-ready diagnostics and dry-run output can explain
  why a task is blocked.
- Expected tests: add autonomous run tests for agent and program states with a
  missing required `inputs:` artifact. Assert no subprocess is spawned, no
  state transition occurs, and the run reports the missing artifact as a
  blocked-ready reason.
- Reason preferable: artifact prerequisites are part of the ready-set contract,
  and enforcing them before spawn keeps dependency handoffs predictable.
- Alternative: `update-spec` to enforce inputs only on transition entry. This
  would preserve current behavior but leaves programs and agents responsible
  for a contract the state machine already declares.

## D-003: Polling runtime semantics are mostly absent

- Primary decision: `update-implementation`.
- Next edits: implement the runtime poll state machine: persist
  `pollNextAttemptAt` and `stateVisits`, exclude future poll attempts from the
  ready set, sleep until the next pending poll deadline when polling is the
  only remaining block, resolve `pollAttempts` / `pollMaxAttempts` condition
  operands, prevent self-loop selection after `poll.max_attempts`, and clear
  poll metadata on non-self-loop exit.
- Expected tests: add CLI/runtime tests with short poll intervals covering
  delayed re-selection, no busy-looping, attempt counting, max-attempt
  exhaustion routing, non-self-loop cleanup, and `--continue-on-error` behavior
  when no exhaustion route matches. Add or update a test for `examples/ci-heal`
  once its stale template variables are fixed.
- Reason preferable: the spec, validator, and shipped example already expose
  `poll:` as a supported workflow feature. Implementing it closes a
  user-facing gap rather than leaving authored polling workflows hazardous.
- Alternative: `defer-follow-up` by marking `poll:` runtime support as
  experimental in the spec and examples. This is credible if the full runtime
  work is too large for the next release, but it should include a visible
  warning because the validator currently accepts the feature.

## D-004: Program scheduling is synchronous and bypasses program parallelism rules

- Primary decision: `update-implementation`.
- Next edits: replace the separate program-before-agent execution path with a
  unified scheduled-work abstraction for program and agent tasks. Apply the
  `--parallel` limit and non-concurrent state filter before dispatch, then run
  programs and agents through the same slot assignment/release and result
  processing loop.
- Expected tests: add directory-workspace e2e tests where two independent
  program tasks run concurrently under `--parallel 2`, and where two ready
  program tasks in a non-concurrent state are serialized across passes. Include
  a mixed agent/program scheduling test if the fake agent harness can support
  it cheaply.
- Reason preferable: programs are autonomous work units under `rhei run`, and
  the spec says they obey the same scheduling rules as agents. One scheduler
  also reduces divergence in future timeout, logging, and failure handling.
- Alternative: `update-spec` to document programs as sequential pre-work. This
  is simpler but contradicts the `--parallel` flag description and
  underutilizes deterministic automation.

## D-005: Agent fanout can be split by `--parallel`

- Primary decision: `update-implementation`.
- Next edits: treat a ready task as the scheduler unit and keep all resolved
  `all_targets` / `all_models` invocations attached to that unit. The
  `--parallel` limit should bound scheduled tasks, not individual fanout
  invocations; once a fanout task is scheduled, spawn all of its invocations in
  that same pass and wait for all required results before routing the task.
- Expected tests: add a multi-target run test with `--parallel 1` and at least
  two targets. Assert all target invocations are spawned in the same pass and
  the task does not transition until all required per-target outputs exist.
- Reason preferable: fanout is a within-task comparison mechanism. Splitting it
  by the global task parallelism flag makes state-visit semantics depend on an
  operational tuning parameter.
- Alternative: `update-spec` to define `--parallel` as an invocation limit
  across fanout. This preserves current batching but weakens the documented
  all-target comparison contract.

## D-006: Program subprocesses are allowed to mutate state despite run authority prose

- Primary decision: `update-spec`.
- Next edits: update `docs/specs/rhei-run.spec.md`,
  `docs/specs/rhei-usage.spec.md`, and the completion-authority section of
  `docs/specs/rhei-agents.spec.md` to distinguish agent subprocesses from
  trusted program subprocesses. State that agents must not mutate plan state,
  while programs may use explicit `rhei transition` / `rhei complete` as a
  documented escape hatch and, when they do, orchestrator exit-code evaluation
  is skipped after the plan re-read.
- Expected tests: retain the existing program exit-code routing coverage and
  add an e2e test where a program explicitly transitions its own task before
  exit; assert `rhei run` observes the state change and does not apply the
  exit-code route.
- Reason preferable: the implementation and program-specific spec already
  support hybrid deterministic programs. Clarifying the authority exception
  avoids a breaking behavior change while making the security and review model
  explicit.
- Alternative: `update-implementation` to forbid or ignore program-initiated
  state changes under `rhei run`. This would strengthen orchestrator authority
  but would break the documented program escape hatch and is difficult to
  enforce against arbitrary local commands beyond detecting the final state
  change.

## D-007: Agent completion can be bypassed by pre-existing or absent outputs

- Primary decision: `update-implementation`.
- Next edits: stop using required-output existence as the predicate for whether
  an agent invocation should spawn. Track current state-visit invocation
  completion separately from artifact presence, or otherwise ensure every
  resolved agent invocation runs at least once for the current task/state visit.
  No-output agent states should spawn and complete based on exit status.
- Expected tests: add e2e tests where an agent state has no `outputs:` and must
  still spawn, and where a stale output file exists before run but the agent is
  still invoked before transition. Add fanout coverage so already-completed
  current-visit invocations are not rerun unnecessarily after a partial pass.
- Reason preferable: the completion condition is exit `0` plus outputs. File
  existence alone is not evidence that the current autonomous work succeeded.
- Alternative: `update-spec` to explicitly allow artifact-cache completion when
  outputs already exist. This would be faster for reruns but would make stale
  artifacts indistinguishable from successful current work.

## D-008: Program missing-output handling aborts instead of leaving the task in place

- Primary decision: `update-implementation`.
- Next edits: after a program exits `0` and before calling
  `execute_transition`, check required outputs for the current program state.
  If any required output is missing, log the documented warning, leave the task
  in its current state, and continue or halt according to the surrounding
  no-progress policy rather than surfacing a transition execution error.
- Expected tests: add a program state with an `exit_code: 0` transition and a
  missing required output. Assert `rhei run` leaves the state unchanged, does
  not append a transition result, reports the missing output, and handles
  `--continue-on-error` consistently with other no-advance failures.
- Reason preferable: missing output after a zero exit is a failed completion
  condition, not a malformed transition. Keeping the task in place preserves
  recoverability and matches the program artifact contract.
- Alternative: `update-spec` to say missing program outputs abort the whole run
  as a transition validation failure. This is easier to implement but makes a
  routine artifact miss more disruptive than the spec intends.

## D-009: Program required inputs are not enforced before spawn

- Primary decision: `update-implementation`.
- Next edits: call the shared required-input checker immediately before
  program spawn, even after ready-set filtering is fixed, so direct scheduler
  changes cannot bypass the program contract. Include missing input names and
  paths in the spawn-abort diagnostic.
- Expected tests: add a program pre-spawn test with a missing required input.
  Assert the command is not executed, no log body from the program appears, the
  state remains unchanged, and the diagnostic names the missing input.
- Reason preferable: deterministic programs should receive a valid declared
  environment from Rhei instead of reimplementing artifact guards in shell.
- Alternative: `update-spec` to make input existence advisory for programs and
  expose only `RHEI_INPUT_*_EXISTS`. This is weaker and conflicts with the
  artifact-contract model used by manual transitions.

## D-010: `examples/ci-heal` uses stale runtime template variables

- Primary decision: `update-implementation`.
- Next edits: update `examples/ci-heal/states.yaml` to use `{task_id}`,
  `{visit_count}`, and `meta.branch` or `meta.<key>` forms supported by the
  runtime template resolver. Check related README text and any template copy
  for the same stale variables.
- Expected tests: add an example validation or template-render smoke test for
  `examples/ci-heal` that fails on unresolved `{task.metadata.*}`,
  `{task.id}`, or `{visit}` placeholders.
- Reason preferable: the current specs and resolver agree on the supported
  names. The shipped example is the stale artifact and should not teach users
  invalid templates.
- Alternative: `update-spec` to restore the old variable aliases as supported
  compatibility names. This may help old workflows, but it expands the template
  surface for an example-specific regression.

## D-011: Settings/model registry schema and resolution order are not implemented

- Primary decision: `update-implementation`.
- Next edits: extend `RheiSettings` and merge logic to support
  `defaults.agent`, `defaults.model`, the merged `models` registry,
  `models.<id>.provider`, `models.<id>.model`, `models.<id>.default_agent`,
  and `models.<id>.agents`. Refactor model, agent, mode, target, callback, and
  template-context resolution through a single settings-backed resolver while
  keeping legacy top-level settings as compatibility fallbacks if required.
- Expected tests: add settings merge tests for global/project model registries,
  model-agent bindings, `default_agent`, nested defaults, null clearing, and
  mode precedence. Add run tests where a state model id resolves to a concrete
  provider/model pair and where an undefined model id is a configuration error.
- Reason preferable: the settings spec is detailed and user-facing; ignoring
  nested defaults and model registries makes documented configuration silently
  ineffective.
- Alternative: `update-both` to deliberately retain only top-level
  `agent` / `model` settings and simplify the spec. This would reduce
  implementation scope but would remove the separation between model identity
  and agent transport that multi-model workflows rely on.

## D-012: Unknown or unavailable MCP/skill tooling is not routed as specified

- Primary decision: `update-implementation`.
- Next edits: make unresolved MCP server and skill ids hard validation or
  settings-load errors unless they are inline definitions. Before agent spawn,
  evaluate required tooling availability and route through matching
  `mcp_unavailable` / `skill_unavailable` transitions with system trigger data;
  if no route matches, leave the task in place and apply the normal
  `--continue-on-error` policy.
- Expected tests: update the unit test that currently locks in
  unknown-as-unavailable behavior. Add validator tests for dangling tooling ids
  and run tests for unavailable required MCP/skill routing, including callback
  context `triggeredBy: "system"` and unavailable id data.
- Reason preferable: tooling declarations are execution prerequisites. Making
  unknown ids explicit errors and required unavailable tools routable prevents
  agents from running with a silently degraded tool surface.
- Alternative: `update-spec` to document unknown tooling as an unavailable
  runtime value exposed only through environment variables. This is more
  permissive but undermines the declared fallback transitions.

## D-013: Agent and program log formats do not match the versioned spec

- Primary decision: `update-implementation`.
- Next edits: update agent log headers to `=== rhei agent log v1 ===`, add
  `started:` to agent and program headers, add `ended:` to exit footers, and
  emit provider/model-name metadata for resolved model profiles. Preserve
  current body capture and timeout footer behavior.
- Expected tests: add log-schema assertions for agent success, agent timeout,
  program success, and program failure. Cover provider/model-name fields for a
  settings-backed model profile and absence of those fields when no model
  resolves.
- Reason preferable: versioned logs are an integration surface for monitoring,
  TUI, and audits. Matching the documented schema is less disruptive than
  teaching parsers the current partial format.
- Alternative: `update-spec` to freeze the current unversioned agent log and
  timestamp omissions. This would avoid a log change but leaves the v1 marker
  misleading.

## D-014: Agent environment omits model profile fields

- Primary decision: `update-implementation`.
- Next edits: populate `RHEI_MODEL`, `RHEI_MODEL_PROVIDER`, and
  `RHEI_MODEL_NAME` from the resolved model profile. For inline target
  selectors, define and implement fallback values for selector identity,
  provider, and provider model name. Use the same resolver as D-011 so logs,
  prompts, callbacks, and env vars agree.
- Expected tests: add command-assembly tests for settings-backed models, legacy
  state `model`, `all_models`, and inline `target` / `all_targets` selectors.
  Assert all three environment variables are correct or intentionally omitted.
- Reason preferable: subprocesses need concrete provider/model metadata to
  reproduce the execution context and implement provider-specific behavior.
- Alternative: `update-spec` to document only `RHEI_MODEL` as stable. This
  would match current behavior but would make the model registry less useful to
  custom agents and scripts.

## D-015: Autonomous dry-run can create runtime artifacts

- Primary decision: `update-implementation`.
- Next edits: move dry-run handling before frontend or journal initialization,
  or add a dry-run frontend that never opens `JournalSink`. Ensure dry-run
  prints planned work from the same scan/selection logic without creating
  `runtime/`, `runtime/transitions.log`, logs, locks, or result files.
- Expected tests: extend dry-run tests to assert no `runtime/` directory or
  `runtime/transitions.log` exists after autonomous dry-run for agent and
  program-capable plans. Keep existing assertions that markdown is unchanged.
- Reason preferable: dry-run is explicitly side-effect free and is commonly
  used in CI or before committing fixtures. Runtime artifact creation violates
  that trust even when task state is unchanged.
- Alternative: `update-spec` to allow dry-run journal creation. This would make
  monitoring easier but contradicts the command's inspection-only purpose.

## D-016: Timeout routing fires on any nonzero exit when a timeout is configured

- Primary decision: `update-implementation`.
- Next edits: make agent and program spawn results carry an explicit
  `timed_out` boolean or enum variant distinct from ordinary exit status.
  Call timeout-transition routing only when the timeout path actually killed
  the subprocess; route ordinary nonzero exits through normal failure or
  exit-code handling.
- Expected tests: add timed agent and program tests where the subprocess exits
  nonzero before the timeout. Assert the timeout transition does not fire, the
  log records the real exit code, and normal error/exit-code handling applies.
  Keep timeout-kill tests for the true timeout path.
- Reason preferable: timeout routes represent elapsed-time failures, not every
  failure from a state that happens to have a timeout budget.
- Alternative: `update-spec` to define timeout transitions as a catch-all
  failure route for timed states. This would be surprising and would hide
  ordinary nonzero exits from failure diagnostics.

## D-017: Timeout/system callbacks do not receive system trigger metadata

- Primary decision: `update-implementation`.
- Next edits: extend `execute_transition` to accept trigger metadata and
  transition data, defaulting to user/manual values for CLI transitions. Pass
  `triggeredBy: "system"` plus timeout duration, agent/program identity, and
  unavailable tooling data from timeout and tooling-unavailable paths. Ensure
  callback redirects preserve or intentionally update the trigger source.
- Expected tests: add callback stdin JSON tests for timeout transitions and
  tooling-unavailable transitions. Assert `triggeredBy`, `transitionData`, and
  rollback behavior match the callback and transitions specs.
- Reason preferable: callback authors need to distinguish human actions from
  engine/system routes, especially for notification and audit callbacks.
- Alternative: `update-spec` to omit trigger metadata for CLI-run callbacks.
  This would simplify `execute_transition`, but it makes the existing
  `TransitionContext` contract less useful.

## D-018: Agent timeout resolution omits model-agent binding timeouts

- Primary decision: `update-implementation`.
- Next edits: after D-011 introduces model-agent bindings, update timeout
  resolution to follow state-level `agent_timeout` > resolved
  `models.<id>.agents.<agent>.timeout` > agent profile `timeout` >
  `defaults.agent_timeout`. Include the selected source in diagnostics or
  debug logs where timeout resolution is reported.
- Expected tests: add timeout precedence tests for every layer, especially a
  model-agent binding overriding an agent-profile timeout and being overridden
  by a state-level timeout. Add a missing-timeout orchestrator error test that
  still fails when none of the layers resolve.
- Reason preferable: per-model/agent timeout policy is important for
  multi-model execution and is already documented in the settings schema.
- Alternative: `defer-follow-up` until the model registry work in D-011 lands.
  This is practical for sequencing, but the final reconciliation should remain
  implementation alignment rather than changing the timeout spec.

## D-019: Program working directories can escape the workspace

- Primary decision: `update-implementation`.
- Next edits: validate templated `program.working_directory` after expansion
  with workspace-root containment checks. Reject absolute paths outside the
  workspace and relative paths that escape through `..`; use lexical
  normalization for paths that may not exist and canonicalization when they do.
- Expected tests: add validator or run-time tests for `../outside`, absolute
  outside paths, symlink-sensitive cases if supported, and valid nested
  workspace paths. Assert rejected states do not spawn programs.
- Reason preferable: program execution is deterministic but still has local
  side effects. Keeping working directories inside the workspace preserves the
  containment model documented for artifacts and runtime logs.
- Alternative: `update-spec` to allow workspace escapes for advanced programs.
  This would be flexible but unsafe for third-party templates and inconsistent
  with the current validation text.

## D-020: Program states do not warn on ignored model selectors

- Primary decision: `update-implementation`.
- Next edits: add a validator warning when a state declares `program` together
  with `model` or `all_models`. Keep it a warning, not an error, so migrations
  from agent states remain possible.
- Expected tests: add validator warning tests for `program` plus `model` and
  `program` plus `all_models`, and a negative test showing a plain program
  state has no warning.
- Reason preferable: the spec already chooses a compatibility warning. Emitting
  it helps authors catch a selector that has no effect at runtime.
- Alternative: `no-change` if validation warnings are not currently surfaced in
  a useful way. This would leave a documented authoring diagnostic missing and
  should be paired with a broader warning-surface follow-up.

## D-021: Tentative: Only `cli:` callbacks execute through the core callback path

- Primary decision: `update-spec`.
- Next edits: clarify `docs/specs/rhei-callbacks.spec.md` with a supported
  runtime matrix. State that the current CLI runtime executes `cli:` callbacks,
  while TypeScript, Python, and Java examples are programmatic SDK or future
  adapter examples unless dedicated callback executors are added.
- Expected tests: retain existing CLI callback integration tests and
  unknown-platform coverage. No behavior test is needed unless maintainers
  choose to add a new callback platform.
- Reason preferable: the discrepancy is tentative because the callback spec is
  examples-oriented. Clarifying the supported runtime avoids promising
  multi-language callback execution before the executor layer exists.
- Alternative: `update-implementation` to add `python:`, `node:` /
  `typescript:`, and `java:` callback executors. This may be desirable later,
  but it is a larger product decision involving dependency discovery,
  packaging, and cross-platform invocation semantics.

## D-022: Stalled non-terminal runs can exit successfully

- Primary decision: `update-implementation`.
- Next edits: after each run pass, distinguish terminal completion, gating-only
  halt, poll-delayed halt, and true stalled non-terminal/no-progress state.
  Return a nonzero error when non-terminal tasks remain and none are runnable,
  polling-delayed, or legitimately waiting at a gating boundary. Include a
  concise diagnostic listing representative blocked tasks and reasons.
- Expected tests: add run exit-code tests for all terminal success,
  missing-input stall, no matching transition/no progress, gating halt, and
  poll-delayed sleep/resume. Assert CI-visible process status differs between
  successful completion and stalled incomplete work.
- Reason preferable: automation must be able to tell "plan completed" from
  "plan is stuck." A successful exit for stalled work hides broken workflows.
- Alternative: `update-spec` to define no-ready/no-progress as a successful
  quiescent exit. This may suit interactive use, but it is poor for CI and
  contradicts the current run spec.

## D-023: Nonzero agent exits lack a general error/exit-code route

- Primary decision: `update-both`.
- Next edits: define an agent failure transition contract in
  `docs/specs/rhei-agents.spec.md` and `docs/specs/rhei-transitions.spec.md`,
  then implement it in `rhei run`. The least disruptive shape is a new
  agent-specific failure selector, such as `agent_exit_code` or
  `agent_error`, rather than reusing program-only `exit_code`; callbacks should
  receive `triggeredBy: "system"` and transition data containing the exit code,
  agent id, model context, and log path. Update validation so the new selector
  is only legal on agent states.
- Expected tests: add agent nonzero routing tests for specific exit code,
  generic nonzero/error fallback, no matching route with and without
  `--continue-on-error`, callback context data, and interaction with true
  timeout routing from D-016.
- Reason preferable: the run and agent specs already promise an error route,
  but the transition schema currently reserves `exit_code` for programs.
  Updating both spec and implementation gives agent workflows structured
  recoverability without weakening program-specific semantics.
- Alternative: `update-spec` to remove the promised agent exit-code/error
  route and document current abort-or-continue behavior for nonzero agent
  exits. This is simpler but leaves agent failure recovery less expressive than
  the spec currently implies.
