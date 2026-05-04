# Discrepancy Elaboration: Run Orchestration, Agents, Programs, and Callbacks

Source findings: `runtime/spec-implementation-discrepancy-audit/run-orchestration-agents-programs/discrepancies.md`

This elaboration groups duplicate findings, marks weak evidence as tentative,
and records no-discrepancy areas. It does not choose a reconciliation strategy.

## Duplicate Merges

- RO-010 timeout callback context and RO-012 system-triggered callback context
  are one mismatch: timeout/tooling-system transitions are executed through the
  normal callback path without system trigger metadata.
- RO-009 autonomous dry-run artifacts and RO-015 runtime journal dry-run
  artifacts are one mismatch: autonomous dry-run initializes the frontend and
  journal before the dry-run branch, creating `runtime/transitions.log`.
- RO-007 settings schema/resolution and RO-008 model/agent/mode resolution are
  coupled symptoms of one settings-model-registry gap.
- RO-003 ready-set poll exclusion and RO-003 polling runtime behavior are
  separated below because one affects task selection and the other affects
  retry/attempt semantics after execution.
- RO-006 program input enforcement overlaps with RO-003 ready-set input
  enforcement, but it is kept separate because the program spec requires a
  pre-spawn check even if ready-set filtering is bypassed or changed.

## Elaborated Discrepancies

### D-001: Unreachable autonomous states select autonomous run mode

Source findings: RO-002. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:48` says autonomous
  subprocess mode is selected only when a reachable non-terminal, non-gating
  state declares autonomous work. `run_command` scans every non-terminal,
  non-gating state in the machine and does not check whether any task can
  reach that state (`crates/rhei-cli/src/main.rs:7323`).
- Why it matters: an unused or stale state definition can force agent/program
  mode for a plan that should run callback-only. That changes error behavior,
  frontend initialization, dry-run side effects, and missing-agent handling.
- Affected: users with broad reusable state machines, templates that include
  optional autonomous states, and callback-only workflows sharing those
  machines.
- Risk: user-facing.
- Current verification: missing-agent behavior for reachable model-declared
  workflows is covered by `run_prefers_agent_mode_for_model_declared_workflows_without_falling_back_to_callbacks`
  (`crates/rhei-cli/tests/e2e/run_tests.rs:434`). The audit did not identify a
  test where only unreachable states declare autonomous work.

### D-002: Ready-set selection ignores current-state inputs

Source findings: RO-003. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:51` requires ready tasks to
  have all required current-state `inputs:` artifacts present. `find_ready_tasks`
  only excludes terminal/gating states and checks prior dependencies
  (`crates/rhei-cli/src/main.rs:8674`). The input helper
  `ensure_state_inputs_exist` exists (`crates/rhei-cli/src/main.rs:4842`) and
  is used for transition entry (`crates/rhei-cli/src/main.rs:5272`), but not for
  ready-set selection or autonomous spawn.
- Why it matters: autonomous execution can start before declared prerequisites
  exist. Programs and agents may receive prompts or env vars for missing inputs
  and perform work in the wrong state instead of remaining blocked.
- Affected: workflows using artifact handoffs between tasks or states,
  especially CI/build programs and agent review steps that consume generated
  files.
- Risk: user-facing.
- Current verification: e2e coverage exists for `rhei complete` and
  `rhei transition` rejecting missing required artifacts on state entry
  (`crates/rhei-cli/tests/e2e/next_tests.rs`,
  `crates/rhei-cli/tests/e2e/transition_tests.rs`). The audit did not identify
  autonomous ready-set/spawn tests for missing current-state inputs.

### D-003: Polling runtime semantics are mostly absent

Source findings: RO-003. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:93` through
  `docs/specs/rhei-run.spec.md:101` specify `pollNextAttemptAt`, delayed
  ready-set membership, attempt counting via `poll.max_attempts`, self-loop
  retry behavior, sleeping until the next poll deadline, and metadata cleanup on
  non-self-loop exit. The validator accepts and validates `poll`
  (`crates/rhei-validator/src/lib.rs:923`), but the CLI ready set does not read
  `pollNextAttemptAt`, `state_visit_limit` only reads legacy `visits`
  (`crates/rhei-cli/src/main.rs:3811`), and condition operands such as
  `pollAttempts` / `pollMaxAttempts` are not resolved
  (`crates/rhei-cli/src/main.rs:3838`).
- Why it matters: workflows authored with `poll:` can busy-loop, ignore retry
  intervals, miss exhaustion routes, or never clear poll metadata. The shipped
  `examples/ci-heal` workflow depends on those semantics.
- Affected: polling workflows, CI watchers, long-running external status
  monitors, and users copying `examples/ci-heal`.
- Risk: user-facing.
- Current verification: validator unit tests cover well-formed and invalid
  `poll` declarations. The audit did not identify CLI/runtime tests for
  `pollNextAttemptAt`, poll sleeping, max-attempt routing, or metadata cleanup.

### D-004: Program scheduling is synchronous and bypasses program parallelism rules

Source findings: RO-004. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:52` and
  `docs/specs/rhei-programs.spec.md:277` say programs follow the same
  `--parallel` and concurrent-state rules as agents. Implementation collects
  `program_tasks`, then runs them one by one before agent batching
  (`crates/rhei-cli/src/main.rs:7623`, `crates/rhei-cli/src/main.rs:7645`).
  The non-concurrent state filter is applied only to `agent_tasks`
  (`crates/rhei-cli/src/main.rs:7842`).
- Why it matters: multiple ready program tasks in a non-concurrent state can
  execute in the same pass, while independent program tasks cannot execute
  concurrently up to `--parallel N`. That is both over-permissive for serialized
  states and under-utilizing for independent deterministic work.
- Affected: deterministic automation states, build/test programs, templates
  relying on program fanout, and users expecting `--parallel` to cover agents
  and programs uniformly.
- Risk: user-facing.
- Current verification: `run_executes_program_states_and_routes_on_exit_code`
  covers a successful program route. Directory workspace parallel tests cover
  agent/callback-style parallel behavior, not concurrent program scheduling.

### D-005: Agent fanout can be split by `--parallel`

Source findings: RO-004. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:110` says all invocations for a
  single scheduled task via `all_targets` or `all_models` are spawned together.
  Implementation flattens each resolved invocation into a separate
  `agent_tasks` entry (`crates/rhei-cli/src/main.rs:7547`) and slices that flat
  list by `batch_size` (`crates/rhei-cli/src/main.rs:7880`).
- Why it matters: one task's fanout can be partially executed in one pass and
  partially deferred to later passes. This weakens the intended comparison
  semantics of multi-model/multi-target states and can make progress depend on
  `--parallel`.
- Affected: multi-target and multi-model workflows, comparison templates, and
  any artifact contract expecting all fanout outputs from the same state visit.
- Risk: user-facing.
- Current verification: `task_has_pending_agent_invocations` prevents
  auto-transition until remaining outputs exist (`crates/rhei-cli/src/main.rs:6415`),
  so partial fanout is guarded from premature completion. The audit did not
  identify a test asserting that all fanout invocations spawn in the same pass.

### D-006: Program subprocesses are allowed to mutate state despite run authority prose

Source findings: RO-005. Classification: `ambiguous-spec`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:57` and
  `docs/specs/rhei-usage.spec.md:32` say spawned subprocesses under
  orchestrator authority must not call `rhei transition` or `rhei complete`.
  `docs/specs/rhei-programs.spec.md:164` says programs may call those commands
  and that explicit mutation takes precedence over exit-code evaluation.
  Implementation follows the program-specific rule by re-reading the plan and
  skipping exit-code evaluation if the state changed
  (`crates/rhei-cli/src/main.rs:7723`).
- Why it matters: the authority model is unclear. Users cannot tell whether
  programs are deterministic workers whose only output is an exit code, or
  trusted workflow actors that may directly advance plan state.
- Affected: program authors, reviewers auditing transition authority, and
  workflow designers deciding whether a program can cross human or policy
  boundaries.
- Risk: user-facing and internal, because both documentation and execution
  semantics are involved.
- Current verification: implementation behavior for program exit-code routing
  is covered by `run_executes_program_states_and_routes_on_exit_code`; spawned
  agent prompts explicitly forbid mutation and have prompt-composition unit
  coverage. The ambiguity itself is not tested because it is a spec conflict.

### D-007: Agent completion can be bypassed by pre-existing or absent outputs

Source findings: RO-006. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:56` requires subprocess exit
  `0` plus required outputs for completion, and says no-output states complete
  based on exit `0`. Implementation filters agent invocations by whether their
  required outputs already exist (`crates/rhei-cli/src/main.rs:7513`). If no
  pending invocation remains, the task is routed to callback/auto-advance
  without spawning (`crates/rhei-cli/src/main.rs:7529`).
- Why it matters: pre-existing files or an empty `outputs:` list can satisfy the
  implementation's completion gate without running the current state's agent.
  That bypasses the intended work and removes the exit-code signal from the
  completion contract.
- Affected: agent states with no declared outputs, reruns with stale artifacts,
  and workflows where output existence is not proof that the current attempt
  succeeded.
- Risk: user-facing.
- Current verification: `task_has_pending_agent_invocations` guards fanout
  completion after spawned agents. The audit did not identify tests asserting
  that no-output agent states still spawn or that stale outputs do not bypass
  execution.

### D-008: Program missing-output handling aborts instead of leaving the task in place

Source findings: RO-006. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-programs.spec.md:319` says required outputs
  are checked after program exit and before transition commit; zero exit with a
  missing output leaves the task in its current state. Implementation selects an
  exit-code transition and then calls `execute_transition` with `?`
  (`crates/rhei-cli/src/main.rs:7778`). Output validation happens inside
  transition execution, so a missing output propagates as a run error.
- Why it matters: a program that exits `0` but fails to produce an artifact
  should be a recoverable no-advance condition. Today it can abort the whole
  run, changing failure routing and `--continue-on-error` expectations.
- Affected: program states with output artifacts, build/report generators, and
  automation that treats missing outputs as "not ready yet".
- Risk: user-facing.
- Current verification: existing e2e coverage verifies successful program
  artifact production, but not the missing-output branch.

### D-009: Program required inputs are not enforced before spawn

Source findings: RO-006. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-programs.spec.md:319` requires missing
  required inputs to abort program spawn. The program branch builds a
  `RuntimeTemplateContext` and calls `spawn_and_wait_program` without invoking
  `ensure_state_inputs_exist` (`crates/rhei-cli/src/main.rs:7645`,
  `crates/rhei-cli/src/main.rs:7684`). `build_program_command` only exposes
  input paths and existence flags in env vars (`crates/rhei-cli/src/main.rs:7082`).
- Why it matters: deterministic programs can run with absent declared inputs,
  which pushes contract enforcement into each script and makes artifact
  dependencies less predictable.
- Affected: program states consuming prior artifacts, shipped examples, and
  users relying on Rhei rather than shell code to enforce workflow contracts.
- Risk: user-facing.
- Current verification: transition-entry input checks have e2e coverage. The
  audit did not identify a program pre-spawn missing-input test.

### D-010: `examples/ci-heal` uses stale runtime template variables

Source findings: RO-006. Classification: `spec-stale`.

- Exact mismatch: `examples/ci-heal/states.yaml` uses `{task.metadata.branch}`,
  `{task.id}`, and `{visit}` (`examples/ci-heal/states.yaml:35`,
  `examples/ci-heal/states.yaml:43`, `examples/ci-heal/states.yaml:65`).
  The current states spec documents `{task_id}`, `{visit_count}`, and `meta.*`
  metadata access (`docs/specs/rhei-states.spec.md:229`,
  `docs/specs/rhei-states.spec.md:350`). The resolver implements `task_id`,
  `visit_count`, and `meta.<key>` (`crates/rhei-cli/src/main.rs:4530`,
  `crates/rhei-cli/src/main.rs:4564`).
- Why it matters: a shipped example appears valid but will render unresolved
  variables in paths, env vars, and instructions. That makes the example an
  unreliable guide for polling and artifact workflows.
- Affected: users copying `examples/ci-heal`, docs/examples maintainers, and
  any tests or demos based on that example.
- Risk: user-facing documentation/example risk.
- Current verification: runtime template variables for supported input/output
  path forms are implemented. The audit did not identify coverage validating
  `examples/ci-heal` against current template variable names.

### D-011: Settings/model registry schema and resolution order are not implemented

Source findings: RO-007, RO-008. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-agents.spec.md:235` through
  `docs/specs/rhei-agents.spec.md:279` specify nested `defaults.agent`,
  `defaults.model`, a merged `models` registry, model `provider` / `model`
  fields, `models.<id>.default_agent`, and model-agent bindings. `RheiSettings`
  has top-level `agent`, `agent_mode`, `model`, `agent_timeout`, and no
  `models` registry (`crates/rhei-cli/src/main.rs:5523`). `SettingsDefaults`
  lacks `agent` and `model` (`crates/rhei-cli/src/main.rs:5552`). Agent and
  model resolution therefore use CLI/state/top-level settings rather than the
  specified nested model registry (`crates/rhei-cli/src/main.rs:6114`,
  `crates/rhei-cli/src/main.rs:6137`).
- Why it matters: users following the current settings spec can write settings
  that are ignored. Model ids are not resolved to provider/model-name data, and
  model-specific default agents/timeouts cannot drive autonomous execution.
- Affected: global/project settings users, multi-model workflows, custom agent
  profiles, callbacks and templates expecting resolved model metadata.
- Risk: user-facing.
- Current verification: built-in agent registry, agent replacement by id, MCP
  and skill merge semantics, and defaults tooling replacement are covered by
  unit tests. Validator tests check `state.model` / `all_models` against the
  state-machine top-level `models` list, not a merged settings model registry.

### D-012: Unknown or unavailable MCP/skill tooling is not routed as specified

Source findings: RO-007. Classification: `missing-validation`.

- Exact mismatch: `docs/specs/rhei-agents.spec.md:711` and
  `docs/specs/rhei-agents.spec.md:732` require unresolved tooling ids to be
  validation/settings-load errors and required unavailable tooling to route
  through `mcp_unavailable` / `skill_unavailable`. Implementation resolves
  unknown ids to entries with `definition: None`
  (`crates/rhei-cli/src/main.rs:5981`, `crates/rhei-cli/src/main.rs:5997`) and
  exposes availability through env/log metadata. `validate_machine_settings_references`
  checks agent/target references but not tooling ids
  (`crates/rhei-cli/src/main.rs:5749`).
- Why it matters: required tools can silently become "unavailable" metadata
  while the agent still spawns. Work that depended on a tool may fail later or
  produce lower-quality results, and declared fallback transitions are not
  authoritative.
- Affected: MCP-backed agent workflows, skill-dependent workflows, and
  operators expecting required tooling to gate execution.
- Risk: user-facing.
- Current verification: unit coverage currently locks in the "unknown id
  resolves unavailable" behavior (`resolve_tooling_unknown_id_resolves_to_unavailable`).
  Validator tests cover shape/duplicate/gating/program restrictions and
  transition trigger declarations, not runtime availability routing.

### D-013: Agent and program log formats do not match the versioned spec

Source findings: RO-009. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-agents.spec.md:917` requires
  `=== rhei agent log v1 ===`, provider/model-name metadata where applicable,
  started/ended timestamps, and the specified `=== exit ===` footer. Agent logs
  write `=== rhei agent log ===`, omit started/ended, and only write `model:`
  (`crates/rhei-cli/src/main.rs:6878`, `crates/rhei-cli/src/main.rs:7000`).
  Program logs use `=== rhei program log v1 ===` but also omit started/ended
  (`crates/rhei-cli/src/main.rs:7142`, `crates/rhei-cli/src/main.rs:7197`;
  spec at `docs/specs/rhei-programs.spec.md:217`).
- Why it matters: monitoring, parsing, and audit tools cannot rely on the
  documented version marker or timestamp fields. Missing provider/model-name
  data also reduces traceability in multi-model runs.
- Affected: TUI/journal consumers, log parsers, operators debugging autonomous
  runs, and multi-agent/multi-model workflows.
- Risk: user-facing.
- Current verification: unit tests cover agent output capture and timeout log
  footer behavior, and journal tests cover journal line append behavior. The
  audit did not identify tests asserting the full versioned log schema.

### D-014: Agent environment omits model profile fields

Source findings: RO-009. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-agents.spec.md:597` documents
  `RHEI_MODEL`, `RHEI_MODEL_PROVIDER`, and `RHEI_MODEL_NAME`. Implementation
  sets `RHEI_MODEL` for any model string and sets `RHEI_MODEL_PROVIDER` only
  from inline target selectors; it never sets `RHEI_MODEL_NAME`
  (`crates/rhei-cli/src/main.rs:6672`, `crates/rhei-cli/src/main.rs:6678`).
- Why it matters: agents and scripts cannot reliably distinguish a Rhei model
  profile id from the provider and provider model name. This blocks
  provider-specific behavior described by the settings/model spec.
- Affected: custom agents, callbacks/scripts reading `RHEI_*`, and workflows
  using settings-backed model profiles.
- Risk: user-facing.
- Current verification: command assembly and supported prompt transports have
  unit coverage. The audit did not identify environment tests for
  `RHEI_MODEL_PROVIDER` / `RHEI_MODEL_NAME`.

### D-015: Autonomous dry-run can create runtime artifacts

Source findings: RO-009, RO-015. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:73` says dry-run creates no
  runtime artifacts. `run_agent_mode` selects a frontend before dry-run handling
  (`crates/rhei-cli/src/main.rs:7375`), and `select_frontend` opens the journal
  (`crates/rhei-tui/src/frontend.rs:51`). `JournalSink::open` creates
  `runtime/` and `runtime/transitions.log` (`crates/rhei-tui/src/journal.rs:24`).
- Why it matters: dry-run is supposed to be side-effect free. Creating runtime
  files can dirty worktrees, confuse monitoring tools, and make test fixtures
  appear changed after inspection.
- Affected: users running `rhei run --dry-run` on autonomous plans, CI checks,
  and repository hygiene around generated runtime artifacts.
- Risk: user-facing.
- Current verification: `run_dry_run_shows_transitions_without_changes` passed
  in the targeted verification listed by the discrepancy audit, but that test
  covers markdown changes rather than absence of runtime artifacts. Journal
  tests intentionally verify file creation/append behavior.

### D-016: Timeout routing fires on any nonzero exit when a timeout is configured

Source findings: RO-010. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:55` and
  `docs/specs/rhei-agents.spec.md:824` define timeout behavior as the path used
  when the subprocess exceeds its timeout. In both agent and program result
  handling, any non-success status with `timeout_secs.is_some()` is treated as
  timed out and can fire a timeout transition (`crates/rhei-cli/src/main.rs:8103`,
  `crates/rhei-cli/src/main.rs:7734`).
- Why it matters: ordinary failures from timed states can be misrouted to
  timeout states. That hides real exit-code failures and can trigger the wrong
  callbacks, notifications, or human review instructions.
- Affected: timed agent and program states, failure routing, monitoring
  outcomes, and workflows with both timeout and error paths.
- Risk: user-facing.
- Current verification: timeout subprocess termination has unit coverage
  (`fake_agent_timeout_keeps_output_and_writes_footer`). The audit did not
  identify tests distinguishing real timeout from normal nonzero exit on a
  timed state.

### D-017: Timeout/system callbacks do not receive system trigger metadata

Source findings: RO-010, RO-012. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-agents.spec.md:843`,
  `docs/specs/rhei-agents.spec.md:902`, and
  `docs/specs/rhei-programs.spec.md:204` require timeout transitions to fire
  with `triggeredBy: 'system'` and timeout data. `fire_timeout_transition`
  delegates to `execute_transition` without trigger/data parameters
  (`crates/rhei-cli/src/main.rs:8580`, `crates/rhei-cli/src/main.rs:8605`).
  `execute_transition` hard-codes callback context trigger values to `"user"`
  for `on_leave` and `"user"` / `"callback"` for `on_enter`
  (`crates/rhei-cli/src/main.rs:5160`, `crates/rhei-cli/src/main.rs:5310`).
- Why it matters: callbacks cannot tell timeout/tooling system routes from
  human-initiated transitions, and cannot read the timeout duration from
  `transitionData.timeout`.
- Affected: notification callbacks, audit callbacks, timeout cleanup scripts,
  and tooling-unavailable fallback routes.
- Risk: user-facing for callback authors; internal for transition context
  consistency.
- Current verification: callback integration tests cover stdin JSON,
  rejection, data accumulation, redirect validation, and rollback. The audit
  did not identify tests for timeout/system trigger context.

### D-018: Agent timeout resolution omits model-agent binding timeouts

Source findings: RO-010. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-agents.spec.md:772` through
  `docs/specs/rhei-agents.spec.md:802` require timeout resolution as
  state-level > model-agent binding > agent-profile > defaults. Implementation
  resolves state `agent_timeout`, agent profile timeout, top-level
  `settings.agent_timeout`, then `defaults.agent_timeout`
  (`crates/rhei-cli/src/main.rs:6170`). There is no settings model registry
  where a model-agent binding timeout could live.
- Why it matters: workflows cannot tune timeout budgets per model/agent
  combination, even though the spec exposes that as the second-highest
  precedence layer.
- Affected: multi-model workflows, slower/faster agent bindings, and operators
  trying to centralize timeout policy in settings.
- Risk: user-facing.
- Current verification: `resolve_legacy_agent_uses_defaults_agent_timeout`
  covers defaults timeout behavior. No visible coverage exists for model-agent
  binding timeouts because the registry is absent.

### D-019: Program working directories can escape the workspace

Source findings: RO-011. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-programs.spec.md:420` requires
  `program.working_directory` to resolve within the workspace root. The
  implementation templates the value and joins it to `workspace_root`, but does
  not normalize and reject `..` traversal or absolute path escape behavior
  (`crates/rhei-cli/src/main.rs:7022`). Validator shape checks only require a
  non-empty string (`crates/rhei-validator/src/lib.rs:1481`).
- Why it matters: a state machine can cause deterministic programs to execute
  outside the plan/workspace boundary. That weakens assumptions about artifact
  paths, local side effects, and operational containment.
- Affected: program states, template authors, and users running third-party
  workflows.
- Risk: user-facing, with local filesystem safety implications.
- Current verification: validator tests cover program declaration shape and
  gating/final exclusions. The audit did not identify tests for workspace-bound
  working-directory validation.

### D-020: Program states do not warn on ignored model selectors

Source findings: RO-011. Classification: `missing-validation`.

- Exact mismatch: `docs/specs/rhei-programs.spec.md:407` says `model` and
  `all_models` are ignored for program states and declaring them with `program`
  should produce a validation warning. The validator rejects `program` on
  final/gating states and rejects `agent` plus `program`
  (`crates/rhei-validator/src/lib.rs:883`, `crates/rhei-validator/src/lib.rs:828`),
  but has no warning for `model` / `all_models` on program states.
- Why it matters: users may believe a program will run once per model or will
  receive model-specific context. The ignored selector can hide an authoring
  mistake.
- Affected: state-machine authors migrating from agent to program states and
  workflows mixing deterministic and model-based states.
- Risk: user-facing validation/documentation risk.
- Current verification: validator program tests cover invalid placement and
  exit-code source restrictions. The audit did not identify warning coverage
  for ignored model selectors on program states.

### D-021: Tentative: Only `cli:` callbacks execute through the core callback path

Source findings: RO-012. Classification: `implementation-diverges`, tentative.

- Exact mismatch: `docs/specs/rhei-callbacks.spec.md:3` presents callbacks
  across TypeScript, Python, Java, and CLI/Bash as supported examples. The core
  `ShellCallbackExecutor` strips only `cli:` and returns `UnknownPlatform` for
  any other callback id (`crates/rhei-core/src/callback.rs:157`).
- Why it matters: users can author callbacks from the callback examples that
  are not executable by the current run/transition path. This is especially
  confusing because callback context and result semantics are otherwise
  specified generically.
- Affected: callback authors, NAPI/language-binding users, and workflows that
  expect non-shell callback support.
- Risk: user-facing.
- Current verification: integration tests cover CLI callback mechanics and an
  unknown-platform error path. They verify the current CLI-only behavior rather
  than the multi-language callback surface.
- Tentative note: this is marked tentative because the callback spec page is
  examples-oriented; the mismatch is strong only if those examples are intended
  as a normative supported-language contract for `rhei run` / `rhei transition`.

### D-022: Stalled non-terminal runs can exit successfully

Source findings: RO-013. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:59` says `rhei run` exits
  nonzero when progress halts with non-terminal tasks remaining and no further
  advancement is possible. `run_agent_mode` and `run_callback_mode` return
  `Ok(())` after no-ready/no-progress paths unless an earlier explicit error
  was returned (`crates/rhei-cli/src/main.rs:8373`,
  `crates/rhei-cli/src/main.rs:8434`, `crates/rhei-cli/src/main.rs:8552`,
  `crates/rhei-cli/src/main.rs:8576`).
- Why it matters: CI and automation can treat a stalled plan as successful.
  That is materially different from "all tasks terminal" and can hide blocked,
  misconfigured, or incomplete workflows.
- Affected: CI integrations, scripts invoking `rhei run`, and users relying on
  process status for orchestration.
- Risk: user-facing.
- Current verification: callback failure halting has integration coverage, and
  gating halt behavior has e2e coverage. The audit did not identify exit-code
  tests for stalled non-terminal/no-progress plans.

### D-023: Nonzero agent exits lack a general error/exit-code route

Source findings: RO-013. Classification: `implementation-diverges`.

- Exact mismatch: `docs/specs/rhei-run.spec.md:58` and
  `docs/specs/rhei-agents.spec.md:573` say nonzero agent exits route through
  exit-code/error transition handling. Implementation logs the nonzero agent
  exit, optionally fires a timeout transition whenever a timeout was configured,
  and otherwise applies `--continue-on-error` / abort behavior
  (`crates/rhei-cli/src/main.rs:8096`, `crates/rhei-cli/src/main.rs:8103`,
  `crates/rhei-cli/src/main.rs:8114`). Program nonzero exit-code routing is
  implemented separately (`crates/rhei-cli/src/main.rs:7764`).
- Why it matters: agent states cannot declare the same kind of structured
  failure routing implied by the run/agent specs. Nonzero failures either abort,
  skip, or are misclassified as timeouts.
- Affected: agent workflows with recoverable error states, human review
  fallback states, and users expecting symmetric program/agent failure routing.
- Risk: user-facing.
- Current verification: `--continue-on-error` branches are present for agent
  spawn errors and nonzero exits. The audit did not identify tests for agent
  nonzero exit-code/error transition routing.

## No-Discrepancy Areas Recorded

- Command surface and flag groups match the documented `rhei run` flags:
  standalone, agent, and program flag structs are represented in the CLI, and
  `--parallel 0` is preserved for unlimited agent batches.
- Program priority over agent/default-agent spawning is implemented for ready
  tasks, and missing model/target-driven agent transport returns a missing-agent
  error rather than silently falling back.
- Single-file `--parallel > 1` falls back to sequential execution with a
  warning, and state writes use exclusive file locks plus compare-and-swap
  state verification.
- The spawned-agent prompt includes task/state instructions and explicitly says
  `rhei run` owns advancement, forbidding `rhei transition`, `rhei complete`,
  and direct `**State:**` edits except for nested executions.
- Runtime artifact path variables for `{input.<name>.path}`,
  `{input.<name>.exists}`, and `{output.<name>.path}` are implemented.
- Built-in agent ids named by the spec are present, and agent registry
  replacement plus MCP/skill registry merge/default tooling semantics have unit
  coverage.
- `target` / `all_targets` parsing and validation are present, runtime target
  resolution bypasses legacy model/agent resolution, and target template
  variables are implemented.
- Prompt composition and command assembly for supported agent transports are
  implemented and covered by unit tests.
- The finite timeout requirement is enforced before non-dry-run agent spawning,
  and direct subprocess termination uses SIGTERM, grace period, then kill for
  agents and programs.
- Program declaration parsing supports string and object command forms, env,
  working directory, and shell mode. Program exit-code routing evaluates
  specific matches before `"nonzero"` and detects same-specificity ambiguity.
- Basic CLI callback mechanics match the spec for stdin JSON, rejection,
  transition data accumulation, redirect validation, and on-enter rollback.
- `--continue-on-error` is applied to main agent/program spawn errors and
  nonzero-exit branches.
- Gating barriers are respected by autonomous ready-set selection and run loops:
  gating states are skipped, blocked dependencies do not become ready, and
  independent non-gating work can continue before the run halts for human input.
- Program/tooling declarations are rejected on gating states where specified.
- Runtime slot assignment/release events and journal append behavior are
  implemented and tested, apart from the dry-run artifact discrepancy above.
- Runtime logs are rooted under the workspace or plan root, and `rhei reset`
  removes `runtime/` for both directory workspaces and single-file plans.

## Verification Notes

- The source discrepancy audit records two targeted commands as passing:
  `cargo test -p rhei-cli --test integration run_executes_program_states_and_routes_on_exit_code`
  and
  `cargo test -p rhei-cli --test integration_markdown_plans run_dry_run_shows_transitions_without_changes`.
- This elaboration did not run additional verification. It records the current
  verification visible in the discrepancy audit and referenced test surfaces.
