# Discrepancy Audit: Run Orchestration, Agents, Programs, and Callbacks

Partition: `run-orchestration-agents-programs`

Audit target: autonomous execution semantics for `rhei run`, scoped by
`runtime/spec-implementation-discrepancy-audit/run-orchestration-agents-programs/scope.md`.

## RO-001: Command Surface and Flag Groups

Classification: no-discrepancy

The documented `rhei run <RHEI_PLAN_OR_WORKSPACE> [flags]` surface is represented in the CLI. The standalone, agent, and program flag groups in `docs/specs/rhei-run.spec.md:19`, `docs/specs/rhei-run.spec.md:30`, and `docs/specs/rhei-run.spec.md:39` match `StandaloneExecutionFlags`, `AgentExecutionFlags`, `ProgramExecutionFlags`, and `RunOptions` in `crates/rhei-cli/src/main.rs:5400`, `crates/rhei-cli/src/main.rs:5425`, `crates/rhei-cli/src/main.rs:5443`, and `crates/rhei-cli/src/main.rs:5453`. `--parallel 0` is preserved through `RunOptions::parallel` and later interpreted as unlimited for agent batches in `crates/rhei-cli/src/main.rs:5472` and `crates/rhei-cli/src/main.rs:7880`.

## RO-002: Orchestrated Mode Selection vs Callback-Only Mode

Classification: implementation-diverges

The spec says orchestrated subprocess execution is selected when any *reachable* non-terminal, non-gating state declares autonomous work (`docs/specs/rhei-run.spec.md:48`). The implementation scans every non-terminal, non-gating state in the machine and does not test reachability from any task or profile before selecting agent/program mode (`crates/rhei-cli/src/main.rs:7323`). An unreachable autonomous state can therefore force `run_agent_mode`.

Classification: no-discrepancy

Program priority over agent spawning is implemented for ready tasks: `state_def.program.is_some()` is tested before agent invocation resolution (`crates/rhei-cli/src/main.rs:7489`). Missing model/target-driven agent transport does not silently fall back when agent spawning is enabled; it returns a missing-agent error (`crates/rhei-cli/src/main.rs:7499`, `crates/rhei-cli/src/main.rs:7506`). A regression test covers the missing-agent behavior for a model-declared workflow (`crates/rhei-cli/tests/e2e/run_tests.rs:434`).

## RO-003: Ready Set, Dependencies, Gating Exclusion, Inputs, and Poll Exclusion

Classification: implementation-diverges

The ready-set spec requires current-state required `inputs:` to exist and pending poll deadlines to exclude tasks (`docs/specs/rhei-run.spec.md:51`). `find_ready_tasks` only checks terminal/gating state and prior dependency satisfaction; it does not inspect current-state inputs, state `poll`, or `metadata.tasks.<id>.pollNextAttemptAt` (`crates/rhei-cli/src/main.rs:8674`). Required input checks exist as helpers (`crates/rhei-cli/src/main.rs:4842`) and are used for transition entry (`crates/rhei-cli/src/main.rs:5272`), but not for ready-set selection or before program/agent spawn.

Classification: implementation-diverges

Polling runtime behavior is specified but not implemented. The specs require self-loops to persist `pollNextAttemptAt`, delay future ready-set membership, cap attempts with `poll.max_attempts`, and clear poll metadata on non-self-loop exit (`docs/specs/rhei-run.spec.md:93`, `docs/specs/rhei-run.spec.md:101`; `docs/specs/rhei-states.spec.md:75`). The validator accepts `poll` and validates its shape (`crates/rhei-validator/src/lib.rs:923`), but the CLI has no `pollNextAttemptAt` handling, and `state_visit_limit` only reads `visits`, not `poll.max_attempts` (`crates/rhei-cli/src/main.rs:3811`). Conditions such as `pollAttempts >= pollMaxAttempts` used by the shipped CI example are not recognized by `resolve_condition_operand` (`crates/rhei-cli/src/main.rs:3838`; `examples/ci-heal/states.yaml:105`).

## RO-004: Parallel Scheduling, Concurrent States, Fanout, and File Safety

Classification: implementation-diverges

Programs do not follow the same parallel scheduling path as agents. The spec says programs respect parallel mode and the same independence/concurrency rules as agents (`docs/specs/rhei-programs.spec.md:275`; `docs/specs/rhei-run.spec.md:52`). Implementation processes `program_tasks` synchronously in a loop before the agent batch is computed (`crates/rhei-cli/src/main.rs:7623`, `crates/rhei-cli/src/main.rs:7645`) and applies the non-concurrent state filter only to `agent_tasks` (`crates/rhei-cli/src/main.rs:7842`). Multiple ready program tasks in a non-concurrent state can therefore run in the same pass, and independent program states do not run concurrently up to `--parallel N`.

Classification: implementation-diverges

Within-task fanout is not kept together when `--parallel` is smaller than the number of resolved invocations. The spec says all invocations for one scheduled task via `all_targets` / `all_models` stay together (`docs/specs/rhei-run.spec.md:110`). The implementation expands each resolved invocation into one `agent_tasks` entry (`crates/rhei-cli/src/main.rs:7547`) and then slices the flat list by `batch_size` (`crates/rhei-cli/src/main.rs:7880`). This can split one task's fanout across passes. `task_has_pending_agent_invocations` prevents transition until remaining outputs exist (`crates/rhei-cli/src/main.rs:6415`), but it does not satisfy the "spawned together" scheduling claim.

Classification: no-discrepancy

Single-file `--parallel > 1` falls back to sequential execution with a warning (`crates/rhei-cli/src/main.rs:7304`). State writes are protected by exclusive file locks and compare-and-swap state verification in `execute_transition` (`crates/rhei-cli/src/main.rs:4997`, `crates/rhei-cli/src/main.rs:5043`). Directory workspace parallel behavior has e2e coverage (`crates/rhei-cli/tests/e2e/run_tests.rs:115`).

## RO-005: Subprocess Completion Authority and State Mutation Ownership

Classification: ambiguous-spec

The run and usage specs state that under orchestrator authority spawned subprocesses must not call `rhei transition` or `rhei complete` (`docs/specs/rhei-run.spec.md:57`; `docs/specs/rhei-usage.spec.md:32`). The programs spec separately states that programs may call those commands and that explicit mutation takes precedence over exit-code evaluation (`docs/specs/rhei-programs.spec.md:162`). The implementation follows the latter behavior for programs and also tolerates state changes after agent execution by re-reading the plan and skipping normal auto-advance when the state changed (`crates/rhei-cli/src/main.rs:7723`, `crates/rhei-cli/src/main.rs:8016`, `crates/rhei-cli/src/main.rs:8275`).

Classification: no-discrepancy

The spawned-agent prompt explicitly states that `rhei run` owns advancement and forbids `rhei transition`, `rhei complete`, and direct `**State:**` edits except for nested executions (`crates/rhei-cli/src/main.rs:6602`). A unit test checks that completion-detection prose is not smuggled into the prompt (`crates/rhei-cli/src/main.rs:11399`).

## RO-006: Output Artifact Enforcement and Completion Condition

Classification: implementation-diverges

Agent states whose required outputs already exist, including states with no outputs declared, can be treated as already complete without spawning the agent. The spec says successful agent completion requires subprocess exit `0` plus required outputs, and if no outputs are declared then exit `0` suffices (`docs/specs/rhei-run.spec.md:56`). The implementation filters invocations by whether outputs already exist before spawning (`crates/rhei-cli/src/main.rs:7513`); if the pending set is empty, it queues a callback transition instead of spawning (`crates/rhei-cli/src/main.rs:7529`). That makes pre-existing outputs, or an empty output list, sufficient to bypass the subprocess exit condition.

Classification: implementation-diverges

Program output enforcement does not match the documented zero-exit behavior. The spec says required program outputs are checked after exit and before transition commit; a zero exit with a missing output leaves the task in its current state (`docs/specs/rhei-programs.spec.md:319`). The implementation checks source-state outputs inside `execute_transition` (`crates/rhei-cli/src/main.rs:5263`) and propagates the error with `?` after selecting an exit-code transition (`crates/rhei-cli/src/main.rs:7778`). This aborts the run instead of logging a warning and leaving the task in place. Existing e2e coverage only verifies a successful program state that produces its artifact (`crates/rhei-cli/tests/e2e/run_tests.rs:246`), not the missing-output branch.

Classification: implementation-diverges

Required current-state inputs are not checked before program spawn even though the program spec requires that (`docs/specs/rhei-programs.spec.md:319`). The program branch builds the runtime context and calls `spawn_and_wait_program` without calling `ensure_state_inputs_exist` (`crates/rhei-cli/src/main.rs:7645`, `crates/rhei-cli/src/main.rs:7684`). `build_program_command` exposes input paths and existence flags in environment variables (`crates/rhei-cli/src/main.rs:7082`), but that is not enforcement.

Classification: no-discrepancy

Runtime artifact path variables for `{input.<name>.path}`, `{input.<name>.exists}`, and `{output.<name>.path}` are implemented in the template resolver (`crates/rhei-cli/src/main.rs:4579`, `crates/rhei-cli/src/main.rs:4598`, `crates/rhei-cli/src/main.rs:4619`).

Classification: spec-stale

The shipped `examples/ci-heal` workflow uses template variables that are not in the current states spec and are not resolved by the implementation: `{task.id}`, `{task.metadata.branch}`, and `{visit}` (`examples/ci-heal/states.yaml:35`, `examples/ci-heal/states.yaml:43`, `examples/ci-heal/states.yaml:65`). The current spec documents `{task_id}`, `{visit_count}`, and metadata access through the supported runtime variable set (`docs/specs/rhei-states.spec.md:229`, `docs/specs/rhei-states.spec.md:350`), and the resolver implements `task_id`, `visit_count`, and `meta.<key>` rather than `task.*` (`crates/rhei-cli/src/main.rs:4530`, `crates/rhei-cli/src/main.rs:4564`).

## RO-007: Settings Schema, Merge Order, Agent Registry, and Tooling Resolution

Classification: implementation-diverges

The settings schema and resolution order in the spec use nested `defaults.agent`, `defaults.model`, and a `models` registry with `models.<id>.default_agent` / `models.<id>.agents` (`docs/specs/rhei-agents.spec.md:235`, `docs/specs/rhei-agents.spec.md:256`, `docs/specs/rhei-agents.spec.md:270`). The implementation `RheiSettings` has top-level `agent`, `agent_mode`, `model`, and `agent_timeout`, but no `models` registry, and `SettingsDefaults` lacks `agent` and `model` (`crates/rhei-cli/src/main.rs:5523`, `crates/rhei-cli/src/main.rs:5552`). Agent resolution therefore uses CLI > state > top-level settings agent only (`crates/rhei-cli/src/main.rs:6114`) and model resolution uses CLI/model override > state > top-level settings model (`crates/rhei-cli/src/main.rs:6137`), with no model default-agent binding.

Classification: missing-validation

The spec says unresolved MCP/skill ids are validation errors and required unavailable tooling routes through `mcp_unavailable` / `skill_unavailable` (`docs/specs/rhei-agents.spec.md:711`, `docs/specs/rhei-agents.spec.md:732`). Implementation resolves unknown ids to entries with `definition: None` (`crates/rhei-cli/src/main.rs:5981`, `crates/rhei-cli/src/main.rs:5997`) and then only exposes availability in env vars (`crates/rhei-cli/src/main.rs:6724`). There is no spawn-time handshake/path availability check, no required-vs-optional failure routing, and `validate_machine_settings_references` only checks agent/target references (`crates/rhei-cli/src/main.rs:5749`). Unit coverage currently locks in the "unknown id resolves unavailable" behavior (`crates/rhei-cli/src/main.rs:11877`).

Classification: no-discrepancy

Built-in agent ids listed by the spec are present, including `claude-code`, `codex`, `gemini`, `cursor`, `kilocode`, and `pi` (`crates/rhei-cli/src/main.rs:5592`, `crates/rhei-cli/src/main.rs:5610`, `crates/rhei-cli/src/main.rs:5628`, `crates/rhei-cli/src/main.rs:5658`, `crates/rhei-cli/src/main.rs:5644`, `crates/rhei-cli/src/main.rs:5676`). Agent registry replacement by id and MCP/skill registry merge by id are implemented in `load_merged_settings` (`crates/rhei-cli/src/main.rs:5707`, `crates/rhei-cli/src/main.rs:5717`), and default tooling replacement/clear semantics are represented by `Option<Vec<_>>` and tested (`crates/rhei-cli/src/main.rs:5727`, `crates/rhei-cli/src/main.rs:11821`).

## RO-008: Model, Agent, Mode, `target`, `all_targets`, and `all_models` Resolution

Classification: implementation-diverges

Model profile resolution is only partially implemented. The spec requires resolved model ids to exist in the merged settings `models` registry and supports model-provider/model-name data (`docs/specs/rhei-agents.spec.md:266`; `docs/specs/rhei-agents.spec.md:84`). The validator only checks `state.model` / `state.all_models` against the state-machine top-level `models` list (`crates/rhei-validator/src/lib.rs:716`, `crates/rhei-validator/src/lib.rs:834`, `crates/rhei-validator/src/lib.rs:860`), and the CLI settings type has no model registry (`crates/rhei-cli/src/main.rs:5523`). CLI `--model` values and top-level settings model values are not checked against a merged settings model registry.

Classification: implementation-diverges

Agent and mode resolution do not follow the documented nested-default order. The spec order includes project/global `defaults.agent`, global/project `defaults.model`, and `models.<id>.default_agent` (`docs/specs/rhei-agents.spec.md:261`, `docs/specs/rhei-agents.spec.md:273`). Implementation does not store those fields and resolves agent from CLI > state > top-level settings only (`crates/rhei-cli/src/main.rs:6114`). Mode resolution uses merged `settings.defaults.agent_mode` before top-level `settings.agent_mode` (`crates/rhei-cli/src/main.rs:6147`), but there is no separate global/project distinction after merge.

Classification: no-discrepancy

`target` / `all_targets` selector parsing and validation are present (`crates/rhei-validator/src/lib.rs:749`, `crates/rhei-validator/src/lib.rs:756`), runtime resolution bypasses normal legacy model/agent resolution (`crates/rhei-cli/src/main.rs:6195`), and target template variables are implemented (`crates/rhei-cli/src/main.rs:4549`, `crates/rhei-cli/src/main.rs:4485`). Shipped multi-target examples rely on `{target.slug}` in artifact paths as specified (`docs/specs/rhei-usage.spec.md:279`).

## RO-009: Agent Prompt Composition, Environment, Command Assembly, and Logs

Classification: implementation-diverges

Agent log format does not match the spec. The spec requires `=== rhei agent log v1 ===`, provider/model-name metadata where applicable, started/ended timestamps, and the `=== exit ===` footer shape (`docs/specs/rhei-agents.spec.md:917`). The implementation writes `=== rhei agent log ===` without `v1`, omits started/ended, and only writes `model:` rather than provider/model_name (`crates/rhei-cli/src/main.rs:6878`, `crates/rhei-cli/src/main.rs:6888`, `crates/rhei-cli/src/main.rs:7000`). Program logs use the `v1` marker but similarly omit started/ended (`crates/rhei-cli/src/main.rs:7142`, `crates/rhei-cli/src/main.rs:7197`; spec at `docs/specs/rhei-programs.spec.md:217`).

Classification: implementation-diverges

The agent environment does not expose all documented model fields. The spec includes `RHEI_MODEL_PROVIDER` and `RHEI_MODEL_NAME` as applicable. Implementation sets `RHEI_MODEL` when a model string exists, and sets `RHEI_MODEL_PROVIDER` only for target selectors with a provider; it never sets `RHEI_MODEL_NAME` (`crates/rhei-cli/src/main.rs:6672`, `crates/rhei-cli/src/main.rs:6678`).

Classification: implementation-diverges

Autonomous dry-run can create runtime artifacts. The spec says dry-run creates no runtime artifacts (`docs/specs/rhei-run.spec.md:73`). `run_agent_mode` always selects a frontend at startup (`crates/rhei-cli/src/main.rs:7375`), and `select_frontend` always opens `runtime/transitions.log` via `JournalSink::open`, creating the runtime directory (`crates/rhei-tui/src/frontend.rs:51`; `crates/rhei-tui/src/journal.rs:24`). Callback-only dry-run does not use this path; the discrepancy is specific to autonomous run mode.

Classification: no-discrepancy

Prompt composition includes task heading, current state, personality, instructions, task content, child tasks, and a `Rhei Commands` section with orchestrator-authority instructions (`crates/rhei-cli/src/main.rs:6579`, `crates/rhei-cli/src/main.rs:6602`). Prompt delivery via prompt flag or stdin and mode/model flag assembly are implemented in `build_agent_command` (`crates/rhei-cli/src/main.rs:6643`, `crates/rhei-cli/src/main.rs:6651`, `crates/rhei-cli/src/main.rs:6657`), with unit coverage for supported transports (`crates/rhei-cli/src/main.rs:12029`).

## RO-010: Agent Timeout Requirement, Timeout Behavior, and Timeout Transitions

Classification: implementation-diverges

Timeout transition firing is based on whether a timeout was configured, not whether a timeout actually occurred. In the agent nonzero-exit branch, any non-success status with `resolved.timeout_secs.is_some()` calls `fire_timeout_transition` (`crates/rhei-cli/src/main.rs:8096`, `crates/rhei-cli/src/main.rs:8103`). The program branch has the same pattern (`crates/rhei-cli/src/main.rs:7734`). A normal nonzero exit from a timed state can therefore be routed as a timeout.

Classification: implementation-diverges

Timeout callbacks do not receive `triggeredBy: 'system'` or `transitionData.timeout` as specified (`docs/specs/rhei-agents.spec.md:843`, `docs/specs/rhei-agents.spec.md:902`; `docs/specs/rhei-programs.spec.md:204`). `fire_timeout_transition` delegates to `execute_transition` without a trigger or timeout data parameter (`crates/rhei-cli/src/main.rs:8580`, `crates/rhei-cli/src/main.rs:8605`). `execute_transition` hard-codes `triggeredBy` to `"user"` for on_leave and `"user"` / `"callback"` for on_enter (`crates/rhei-cli/src/main.rs:5160`, `crates/rhei-cli/src/main.rs:5310`).

Classification: implementation-diverges

The agent timeout resolution chain omits model-agent binding timeouts because there is no settings model registry. The spec requires state-level > model-agent binding > agent-profile > defaults (`docs/specs/rhei-agents.spec.md:772`, `docs/specs/rhei-agents.spec.md:802`). Implementation resolves state `agent_timeout`, agent profile timeout, top-level settings `agent_timeout`, then `defaults.agent_timeout` (`crates/rhei-cli/src/main.rs:6170`).

Classification: no-discrepancy

The finite timeout requirement is enforced at runtime before spawning non-dry-run agent invocations (`crates/rhei-cli/src/main.rs:7541`, `crates/rhei-cli/src/main.rs:6288`). The direct subprocess is terminated with SIGTERM, grace period, then kill in `spawn_and_wait_agent` and `spawn_and_wait_program` (`crates/rhei-cli/src/main.rs:6968`, `crates/rhei-cli/src/main.rs:7168`).

## RO-011: Program Declaration, Environment, Spawn, Exit-Code Routing, Artifacts, and Logs

Classification: implementation-diverges

Program working directories are not constrained to remain within the workspace. The spec requires `program.working_directory` to be relative to the workspace root and resolve within it. Implementation templates the value and joins it to `workspace_root`, but does not normalize or reject paths that escape via `..` or absolute path behavior (`crates/rhei-cli/src/main.rs:7022`). Validator only checks non-empty string shape (`crates/rhei-validator/src/lib.rs:1481`).

Classification: missing-validation

The program spec says `model` and `all_models` are ignored for programs and declaring them with `program` is a validation warning, while `program` with `agent` or `gating` is invalid. The validator rejects `program` on final/gating states and `agent` plus `program` (`crates/rhei-validator/src/lib.rs:883`, `crates/rhei-validator/src/lib.rs:828`), but there is no validation warning or structural handling for `model` / `all_models` on program states.

Classification: no-discrepancy

Program declaration parsing supports string commands and object commands, including string array exec form, env, working_directory, and shell boolean (`crates/rhei-cli/src/main.rs:6441`; validator shape checks at `crates/rhei-validator/src/lib.rs:1425`). Exit-code routing evaluates specific integer/array matches before `"nonzero"` and errors on multiple matches at the same specificity (`crates/rhei-cli/src/main.rs:7217`). Program env merges base `RHEI_*` vars first and then applies `program.env`, allowing program env collisions to win (`crates/rhei-cli/src/main.rs:7060`, `crates/rhei-cli/src/main.rs:7110`).

## RO-012: Callback Invocation, Context, Rejection, Data Passing, Redirects, and Rollback

Classification: implementation-diverges

Only `cli:` callbacks are executable in the current core callback executor. The usage and callbacks specs describe TypeScript/JavaScript, Python, Java, and CLI callback examples as supported surfaces (`docs/specs/rhei-usage.spec.md:275`; `docs/specs/rhei-callbacks.spec.md:3`). `ShellCallbackExecutor` strips only the `cli:` prefix and returns `UnknownPlatform` otherwise (`crates/rhei-core/src/callback.rs:157`). This makes non-CLI callback examples unsupported by the current run/transition execution path.

Classification: implementation-diverges

System-triggered callback context is missing for timeout and tooling-unavailable routes. The callback context builder supports `triggeredBy` (`crates/rhei-cli/src/main.rs:4231`), but `execute_transition` currently passes `"user"` to on_leave and `"user"` / `"callback"` to on_enter (`crates/rhei-cli/src/main.rs:5160`, `crates/rhei-cli/src/main.rs:5310`). Timeout transitions therefore do not satisfy the specified `triggeredBy: 'system'` / timeout data contract.

Classification: no-discrepancy

Basic CLI callback mechanics match the spec: stdin JSON is delivered (`crates/rhei-core/src/callback.rs:181`), `success: false` rejects (`crates/rhei-cli/src/main.rs:5180`), `data` accumulates into `transitionData` (`crates/rhei-cli/src/main.rs:5195`), `nextState` redirects are validated before state write (`crates/rhei-cli/src/main.rs:5207`), and on_enter failure rolls back state writes (`crates/rhei-cli/src/main.rs:5331`). Integration tests cover these paths (`crates/rhei-cli/tests/integration_markdown_plans.rs:1374`, `crates/rhei-cli/tests/integration_markdown_plans.rs:1406`, `crates/rhei-cli/tests/integration_markdown_plans.rs:1734`).

## RO-013: Failure Routing and Continue-On-Error

Classification: implementation-diverges

The spec says `rhei run` exits nonzero when progress halts with non-terminal tasks remaining and no further advancement is possible (`docs/specs/rhei-run.spec.md:59`). `run_agent_mode` and `run_callback_mode` return `Ok(())` after "No tasks could be advanced" / no-progress paths unless an earlier explicit error was returned (`crates/rhei-cli/src/main.rs:8373`, `crates/rhei-cli/src/main.rs:8434`, `crates/rhei-cli/src/main.rs:8552`, `crates/rhei-cli/src/main.rs:8576`). Stalled non-terminal plans can therefore exit successfully.

Classification: implementation-diverges

Nonzero agent exits do not route through a general exit-code/error transition path. The scoped claim says nonzero agent exits route through defined exit-code/error transitions; implementation only logs the nonzero exit and optionally fires a timeout transition when any timeout is configured (`crates/rhei-cli/src/main.rs:8096`, `crates/rhei-cli/src/main.rs:8103`, `crates/rhei-cli/src/main.rs:8114`). Program nonzero exit-code routing is implemented separately (`crates/rhei-cli/src/main.rs:7764`, `crates/rhei-cli/src/main.rs:7803`).

Classification: no-discrepancy

`--continue-on-error` is applied to agent/program spawn errors and nonzero exits in the main result-handling branches (`crates/rhei-cli/src/main.rs:7809`, `crates/rhei-cli/src/main.rs:8114`, `crates/rhei-cli/src/main.rs:8348`). Callback failure halting has integration coverage (`crates/rhei-cli/tests/integration_markdown_plans.rs:2047`).

## RO-014: Gating Barriers and Human Review Boundaries

Classification: no-discrepancy

`find_ready_tasks` excludes gating states (`crates/rhei-cli/src/main.rs:8692`), dependencies only satisfy when terminal and not cancelled (`crates/rhei-cli/src/main.rs:8666`), and both agent and callback run loops therefore stop autonomous transition out of gates. Gating is treated as a barrier rather than an immediate global abort because independent ready tasks continue to be processed until no ready tasks remain (`crates/rhei-cli/src/main.rs:7436`, `crates/rhei-cli/src/main.rs:8447`). E2E coverage exists for stopping at human review and allowing other branches to continue before halting (`crates/rhei-cli/tests/e2e/run_tests.rs:296`, `crates/rhei-cli/tests/e2e/run_tests.rs:340`).

Classification: no-discrepancy

Program states are rejected on gating states (`crates/rhei-validator/src/lib.rs:892`), and tooling fields are rejected on gating states (`crates/rhei-validator/src/lib.rs:1267`, `crates/rhei-validator/src/lib.rs:1331`). Shipped changeset review workflows include a gating human-review state and have e2e coverage (`crates/rhei-cli/tests/e2e/run_tests.rs:401`).

## RO-015: Runtime Journaling and User-Facing Monitoring Events

Classification: no-discrepancy

Slot assignment and release events are emitted for spawned agents and programs (`crates/rhei-cli/src/main.rs:7671`, `crates/rhei-cli/src/main.rs:7957`, `crates/rhei-cli/src/main.rs:8182`), and `JournalSink` writes one line for `SlotAssigned` plus one line for `SlotReleased` with exit, duration, and outcome metadata (`crates/rhei-tui/src/journal.rs:75`). Journal tests cover append behavior and release-line metadata (`crates/rhei-tui/src/journal.rs:176`, `crates/rhei-tui/src/journal.rs:216`).

Classification: implementation-diverges

The journaling implementation conflicts with the dry-run no-runtime-artifacts claim. Because `select_frontend` always opens the journal even before dry-run branching, autonomous dry-runs can create `runtime/transitions.log` (`crates/rhei-cli/src/main.rs:7375`; `crates/rhei-tui/src/frontend.rs:51`). This is the same dry-run discrepancy noted under RO-009, but it directly affects the runtime journal claim.

Classification: no-discrepancy

Runtime logs are created under the workspace or plan root (`crates/rhei-cli/src/main.rs:7367`, `crates/rhei-cli/src/main.rs:6742`, `crates/rhei-cli/src/main.rs:7014`), and `rhei reset` removes `runtime/` for both directory workspaces and single-file plans (`crates/rhei-cli/src/main.rs:9414`). E2E coverage checks reset cleanup in the bash-agent fixture (`crates/rhei-cli/tests/e2e/run_tests.rs:491`).

## Targeted Verification Run

- `cargo test -p rhei-cli --test integration run_executes_program_states_and_routes_on_exit_code` passed.
- `cargo test -p rhei-cli --test integration_markdown_plans run_dry_run_shows_transitions_without_changes` passed.
