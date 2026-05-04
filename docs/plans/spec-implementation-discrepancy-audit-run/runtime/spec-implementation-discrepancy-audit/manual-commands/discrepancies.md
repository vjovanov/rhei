# Discrepancy Audit: Manual Worker and Inspection Commands

Partition: `manual-commands`

Scope source: `runtime/spec-implementation-discrepancy-audit/manual-commands/scope.md`

This file records comparison findings only. It does not propose fixes.

## Summary

- `implementation-diverges`: 25 findings
- `spec-stale`: 3 findings
- `ambiguous-spec`: 2 findings
- `missing-validation`: 2 findings
- `missing-test`: 4 findings
- `no-discrepancy`: 25 findings

## MC-000: Role Boundaries and Manual Loop

### MC-000-A: `rhei run` prompt preserves orchestrator-owned transitions

Classification: `no-discrepancy`

The run-agent prompt explicitly tells spawned workers that `rhei run` owns task advancement and that they must not call `rhei transition` / `rhei complete` or modify `**State:**` lines directly. This matches the usage spec's boundary between manual workers and `rhei run` execution. Evidence:

- `docs/specs/rhei-usage.spec.md:31`
- `docs/specs/rhei-usage.spec.md:103`
- `crates/rhei-cli/src/main.rs:6602`
- `crates/rhei-cli/src/main.rs:6607`

### MC-000-B: Worker skills describe the manual loop and human gates

Classification: `no-discrepancy`

The plan-worker skill documents the intended manual loop: `rhei next`, work in the current state, `rhei transition` as needed, `rhei complete`, and stop at terminal or gating states. Evidence:

- `skills/rhei-plan-worker/SKILL.md:30`
- `skills/rhei-plan-worker/SKILL.md:33`
- `skills/rhei-plan-worker/SKILL.md:34`
- `skills/rhei-plan-worker/SKILL.md:35`
- `skills/rhei-plan-worker/SKILL.md:36`

### MC-000-C: CLI help still describes `next` as a transition command

Classification: `spec-stale`

The `rhei next` command help says it "Transition[s] the next ready task to the next state" and explains that it "transitions it forward one step", while `docs/specs/rhei-next.spec.md` says the default claim step assigns the task and does not advance state except for the special non-runnable-initial auto-advance behavior. The help text appears to preserve older command semantics. Evidence:

- `docs/specs/rhei-next.spec.md:19`
- `docs/specs/rhei-next.spec.md:21`
- `crates/rhei-cli/src/main.rs:290`
- `crates/rhei-cli/src/main.rs:292`

## MC-001: Default State Flow, Gating, Instructions, and Completion Paths

### MC-001-A: Default transition graph and terminal flags match the default flow

Classification: `no-discrepancy`

The checked-in spec machine has the expected states, terminal states, human-review gate, and default transition graph. The built-in validator YAML has the same state names and transitions, though it uses the older `initial: true` form covered separately under MC-002. Evidence:

- `docs/specs/states.yaml:8`
- `docs/specs/states.yaml:40`
- `docs/specs/states.yaml:48`
- `docs/specs/states.yaml:55`
- `docs/specs/states.yaml:63`
- `crates/rhei-validator/src/default-states.yaml:8`
- `crates/rhei-validator/src/default-states.yaml:40`
- `crates/rhei-validator/src/default-states.yaml:48`
- `crates/rhei-validator/src/default-states.yaml:55`
- `crates/rhei-validator/src/default-states.yaml:63`

### MC-001-B: `rhei complete` does not reject gating states

Classification: `implementation-diverges`

The complete spec requires rejection when the current state has `gating: true`, and the state-machine-writer skill repeats that `rhei complete` must refuse to exit gating states. The implementation rejects terminal states and non-terminal descendants, but it does not check `StateDef.gating` before selecting a terminal completion target. A default `human-review -> completed` transition therefore remains eligible for `rhei complete`. Evidence:

- `docs/specs/rhei-complete.spec.md:63`
- `docs/specs/rhei-complete.spec.md:64`
- `skills/rhei-state-machine-writer/SKILL.md:252`
- `crates/rhei-cli/src/main.rs:9322`
- `crates/rhei-cli/src/main.rs:9331`
- `crates/rhei-cli/src/main.rs:9342`
- `crates/rhei-cli/src/main.rs:9619`

### MC-001-C: No explicit test covers completion rejection from a gating state

Classification: `missing-test`

The scoped tests cover terminal-state selection, cancellation exclusion, and child-task blocking, but there is no visible test named for `human-review` / gating rejection by `rhei complete`. Evidence:

- `crates/rhei-cli/tests/e2e/next_tests.rs:456`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:1235`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:1281`
- `crates/rhei-cli/src/main.rs:11597`
- `crates/rhei-cli/src/main.rs:11617`
- `crates/rhei-cli/src/main.rs:11634`

## MC-002: State Machine Schema Used By Commands

### MC-002-A: Commands still rely on legacy per-state `initial`

Classification: `implementation-diverges`

The states spec says profiles and node policy replace per-state `initial: true`; the checked-in spec machine uses `profiles.default-rhei.initial: draft`. The validator can load profiles, but manual commands still use `StateDef.initial` to find claimable tasks and reset targets. A profile-only machine such as `docs/specs/states.yaml`, `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml`, or `examples/ci-heal/states.yaml` has no per-state initial flag, so `rhei next` auto-selection and `rhei reset` do not resolve profile initial states as specified. Evidence:

- `docs/specs/rhei-states.spec.md:60`
- `docs/specs/rhei-states.spec.md:63`
- `docs/specs/rhei-states.spec.md:720`
- `docs/specs/states.yaml:107`
- `docs/specs/states.yaml:125`
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml:242`
- `examples/ci-heal/states.yaml:126`
- `crates/rhei-cli/src/main.rs:8723`
- `crates/rhei-cli/src/main.rs:8728`
- `crates/rhei-cli/src/main.rs:9446`
- `crates/rhei-cli/src/main.rs:9450`

### MC-002-B: Built-in default machine and several templates use legacy initial-state schema

Classification: `spec-stale`

The state-machine-writer skill and the current states spec say not to author `initial: true` on states, but the built-in default YAML and several templates still use that legacy schema without `profiles` / `node_policy`. The validator intentionally permits legacy machines when both profile fields are absent, which means the current spec does not describe the compatibility mode that the implementation and templates still use. Evidence:

- `docs/specs/rhei-states.spec.md:91`
- `docs/specs/rhei-states.spec.md:101`
- `skills/rhei-state-machine-writer/SKILL.md:92`
- `skills/rhei-state-machine-writer/SKILL.md:250`
- `crates/rhei-validator/src/default-states.yaml:16`
- `.agents/rhei/templates/changeset-review/states.yaml:65`
- `.agents/rhei/templates/spec-review/states.yaml:24`
- `crates/rhei-validator/src/lib.rs:631`
- `crates/rhei-validator/src/lib.rs:1008`
- `crates/rhei-validator/src/lib.rs:1022`

### MC-002-C: Profile validation exists for authored task states

Classification: `no-discrepancy`

When a machine declares profiles and node policy, validator code checks that profile initial and allowed states exist, rejects `initial: true` in states, and checks authored task states against the resolved profile's allowed set. Evidence:

- `crates/rhei-validator/src/lib.rs:1021`
- `crates/rhei-validator/src/lib.rs:1059`
- `crates/rhei-validator/src/lib.rs:1092`
- `crates/rhei-validator/src/lib.rs:1156`
- `crates/rhei-validator/src/lib.rs:1860`
- `crates/rhei-validator/src/lib.rs:1878`

### MC-002-D: Artifact definition validation covers static path safety and output optionality

Classification: `no-discrepancy`

The validator rejects duplicate artifact names, empty paths, `optional: true` on outputs, absolute paths, and static `..` escapes. Evidence:

- `docs/specs/rhei-states.spec.md:134`
- `docs/specs/rhei-states.spec.md:135`
- `docs/specs/rhei-states.spec.md:136`
- `crates/rhei-validator/src/lib.rs:1556`
- `crates/rhei-validator/src/lib.rs:1582`
- `crates/rhei-validator/src/lib.rs:1587`
- `crates/rhei-validator/src/lib.rs:1592`

## MC-003: Template Variables in Instructions and Personality

### MC-003-A: Runtime template substitution and simple conditionals are implemented

Classification: `no-discrepancy`

`rhei next` builds a runtime template context, resolves variables at output time, leaves unknown variables verbatim, and preprocesses `{if ...}{else}{endif}` blocks for input/MCP/skill availability. Evidence:

- `docs/specs/rhei-states.spec.md:344`
- `docs/specs/rhei-states.spec.md:375`
- `docs/specs/rhei-states.spec.md:376`
- `docs/specs/rhei-states.spec.md:381`
- `crates/rhei-cli/src/main.rs:4530`
- `crates/rhei-cli/src/main.rs:4556`
- `crates/rhei-cli/src/main.rs:4655`
- `crates/rhei-cli/src/main.rs:4763`
- `crates/rhei-cli/src/main.rs:4805`

### MC-003-B: `model.provider` / `model.name` do not match the spec namespace

Classification: `implementation-diverges`

The states spec defines `{model.provider}` as the resolved provider id and `{model.name}` as the resolved provider model name. The implementation resolves `model.provider` only from an inline target selector's provider and resolves `model.name` to the selected model identifier, not a provider model name from settings. Evidence:

- `docs/specs/rhei-states.spec.md:359`
- `docs/specs/rhei-states.spec.md:360`
- `docs/specs/rhei-states.spec.md:361`
- `crates/rhei-cli/src/main.rs:4557`
- `crates/rhei-cli/src/main.rs:4559`
- `crates/rhei-cli/src/main.rs:4560`

## MC-010: `rhei next` Usage, Options, and Output Modes

### MC-010-A: `rhei next` text output does not match the specified claim/peek formats

Classification: `implementation-diverges`

The next spec defines claim-mode text as `Task <ID>: <title>`, `State: <current-state>`, blank line, then instructions, and peek-mode text as `Next: Task <ID>: <title>` plus `State: <current-state>`. The implementation prints either `Task <id> claimed: '<from>' -> '<to>'` or `Task <id> (already in '<state>')`, includes task content and children, and labels instructions as `--- Instructions (<state>) ---`. There is no distinct `Next:` peek text path. Evidence:

- `docs/specs/rhei-next.spec.md:52`
- `docs/specs/rhei-next.spec.md:56`
- `docs/specs/rhei-next.spec.md:91`
- `docs/specs/rhei-next.spec.md:93`
- `crates/rhei-cli/src/main.rs:9854`
- `crates/rhei-cli/src/main.rs:9890`
- `crates/rhei-cli/src/main.rs:9908`
- `crates/rhei-cli/src/main.rs:9931`

### MC-010-B: `rhei next` JSON output omits specified model provider/name fields and adds unstated fields

Classification: `implementation-diverges`

The next spec says JSON includes `task_id`, `title`, `state`, optional `agent`, optional `model`, optional `model_provider`, optional `model_name`, and `instructions`. The implementation emits extra fields (`kind`, `from_state`, `personality`, `content`, `children`), emits `model`, and does not emit `model_provider` or `model_name`. Evidence:

- `docs/specs/rhei-next.spec.md:120`
- `docs/specs/rhei-next.spec.md:122`
- `docs/specs/rhei-next.spec.md:128`
- `docs/specs/rhei-next.spec.md:129`
- `docs/specs/rhei-next.spec.md:130`
- `crates/rhei-cli/src/main.rs:9871`
- `crates/rhei-cli/src/main.rs:9875`
- `crates/rhei-cli/src/main.rs:9877`
- `crates/rhei-cli/src/main.rs:9879`
- `crates/rhei-cli/src/main.rs:9885`

### MC-010-C: The implemented `next` option surface is broader than the usage table

Classification: `spec-stale`

The next spec usage/options table only lists `--peek`, while the CLI exposes `--task`, `--json`, and `--no-callbacks` as well. JSON output is discussed later in the spec, but the usage/options section is incomplete relative to the actual command surface. Evidence:

- `docs/specs/rhei-next.spec.md:7`
- `docs/specs/rhei-next.spec.md:13`
- `crates/rhei-cli/src/main.rs:295`
- `crates/rhei-cli/src/main.rs:299`
- `crates/rhei-cli/src/main.rs:302`
- `crates/rhei-cli/src/main.rs:305`
- `crates/rhei-cli/src/main.rs:308`

## MC-011: Claimability and Plan Order

### MC-011-A: Auto-pick `rhei next` only claims per-state initial tasks, not all non-terminal/non-gating tasks

Classification: `implementation-diverges`

The next spec defines a claimable task as one whose priors are satisfied, no assignee is present, current state is not final and not gating, and required inputs exist. The implementation first finds ready tasks, then filters them to states with `StateDef.initial == true`. This means an unassigned ready task already in `pending`, `agent-review`, or another non-initial work state is not auto-claimable, even when the spec's claimability definition says it should be. Evidence:

- `docs/specs/rhei-next.spec.md:23`
- `docs/specs/rhei-next.spec.md:25`
- `docs/specs/rhei-next.spec.md:26`
- `docs/specs/rhei-next.spec.md:27`
- `docs/specs/rhei-next.spec.md:28`
- `crates/rhei-cli/src/main.rs:8674`
- `crates/rhei-cli/src/main.rs:8719`
- `crates/rhei-cli/src/main.rs:8723`
- `crates/rhei-cli/src/main.rs:8728`

### MC-011-B: `cancelled` prerequisite semantics conflict across specs and implementation

Classification: `ambiguous-spec`

The next spec says priors are satisfied when prior tasks are terminal states, naming `completed` or `cancelled`. The list spec says readiness uses terminal, non-cancelled prerequisites, and usage prose says "all priors completed". The implementation and tests use terminal-but-not-cancelled. Evidence:

- `docs/specs/rhei-next.spec.md:25`
- `docs/specs/rhei-list.spec.md:44`
- `docs/specs/rhei-list.spec.md:45`
- `docs/specs/rhei-list.spec.md:46`
- `docs/specs/rhei-usage.spec.md:29`
- `crates/rhei-cli/src/main.rs:8662`
- `crates/rhei-cli/src/main.rs:8667`
- `crates/rhei-cli/tests/e2e/next_tests.rs:421`

### MC-011-C: Initial-state auto-advance is implemented, including callbacks

Classification: `no-discrepancy`

For legacy machines with a per-state initial flag, `rhei next` detects non-runnable initial states, finds the first forward transition, and executes it before printing instructions. Runnable initial states are not auto-transitioned. Evidence:

- `docs/specs/rhei-next.spec.md:21`
- `crates/rhei-cli/src/main.rs:8834`
- `crates/rhei-cli/src/main.rs:9175`
- `crates/rhei-cli/src/main.rs:9182`
- `crates/rhei-cli/src/main.rs:9192`
- `crates/rhei-cli/tests/e2e/next_tests.rs:243`

## MC-012: Atomic Claim, Assignee, and Agent Resolution

### MC-012-A: Claim mode does not re-read and revalidate the claim under the assignee-write lock

Classification: `implementation-diverges`

The next spec requires claim mode to acquire a lock, re-read and revalidate claimability under the lock, write the assignee atomically, and release the lock. The implementation selects a task before locking, optionally performs an auto-transition through `execute_transition`, reloads once, resolves the agent, and then writes `**Assignee:**` with a separate lock. `write_task_assignee` no-ops if another writer already inserted an assignee, but `next_command` still prints the task as claimed rather than failing/reselecting. Evidence:

- `docs/specs/rhei-next.spec.md:40`
- `docs/specs/rhei-next.spec.md:41`
- `docs/specs/rhei-next.spec.md:42`
- `docs/specs/rhei-next.spec.md:44`
- `docs/specs/rhei-next.spec.md:45`
- `crates/rhei-cli/src/main.rs:9145`
- `crates/rhei-cli/src/main.rs:9218`
- `crates/rhei-cli/src/main.rs:9235`
- `crates/rhei-cli/src/main.rs:9238`
- `crates/rhei-cli/src/main.rs:9737`
- `crates/rhei-cli/src/main.rs:9740`
- `crates/rhei-cli/src/main.rs:9746`

### MC-012-B: No placeholder assignee is written when no agent resolves

Classification: `no-discrepancy`

The implementation only writes `**Assignee:**` if `resolve_agent` produced an agent id. This matches the spec's "no placeholder" clause. Evidence:

- `docs/specs/rhei-next.spec.md:44`
- `crates/rhei-cli/src/main.rs:9231`
- `crates/rhei-cli/src/main.rs:9238`
- `crates/rhei-cli/src/main.rs:9239`

## MC-013: Peek Mode Read-Only Behavior

### MC-013-A: Peek mode skips writes and auto-transition

Classification: `no-discrepancy`

`next_command` guards the initial auto-transition and assignee write with `!peek`, so peek mode does not change state and does not set an assignee. Evidence:

- `docs/specs/rhei-next.spec.md:78`
- `docs/specs/rhei-next.spec.md:80`
- `crates/rhei-cli/src/main.rs:9192`
- `crates/rhei-cli/src/main.rs:9235`
- `crates/rhei-cli/src/main.rs:9238`

### MC-013-B: Dedicated peek-mode behavior coverage is not visible

Classification: `missing-test`

The scoped test list has broad `next` tests, but no visible test named for `next --peek` read-only behavior, no-lock/no-assignee behavior, or missing-artifact parity in peek mode. Evidence:

- `crates/rhei-cli/tests/e2e/next_tests.rs:5`
- `crates/rhei-cli/tests/e2e/next_tests.rs:81`
- `crates/rhei-cli/tests/e2e/next_tests.rs:102`
- `crates/rhei-cli/tests/e2e/next_tests.rs:243`
- `crates/rhei-cli/tests/e2e/next_tests.rs:503`

## MC-014: No-Task Status and Missing Artifact Diagnostics

### MC-014-A: Missing input artifact diagnostic includes task, state, artifact name, and path

Classification: `no-discrepancy`

The implementation formats missing input errors with a context line naming task and state plus a missing-artifact line naming artifact and path. Evidence:

- `docs/specs/rhei-next.spec.md:63`
- `docs/specs/rhei-next.spec.md:71`
- `crates/rhei-cli/src/main.rs:4842`
- `crates/rhei-cli/src/main.rs:4870`
- `crates/rhei-cli/src/main.rs:9141`
- `crates/rhei-cli/src/main.rs:9170`
- `crates/rhei-cli/tests/e2e/next_tests.rs:503`

### MC-014-B: No-claimable diagnostics do not implement the three specified summaries

Classification: `implementation-diverges`

The spec requires three distinguishable summaries: all terminal, gating/human action, and all in-flight/claimed. The implementation emits different messages, includes a "mid-workflow" category, treats gating states as ready non-initial work rather than the specified human-action summary, and has no explicit in-flight/claimed summary. Evidence:

- `docs/specs/rhei-next.spec.md:100`
- `docs/specs/rhei-next.spec.md:104`
- `docs/specs/rhei-next.spec.md:106`
- `docs/specs/rhei-next.spec.md:107`
- `docs/specs/rhei-next.spec.md:108`
- `crates/rhei-cli/src/main.rs:8733`
- `crates/rhei-cli/src/main.rs:8768`
- `crates/rhei-cli/src/main.rs:8779`
- `crates/rhei-cli/src/main.rs:8789`
- `crates/rhei-cli/src/main.rs:8807`
- `crates/rhei-cli/src/main.rs:8823`

## MC-020: `rhei transition` Usage, Options, and State Values

### MC-020-A: Transition usage and required flags are implemented

Classification: `no-discrepancy`

The CLI exposes `rhei transition <RHEI_PLAN> --task --from --to` and `--no-callbacks`. Evidence:

- `docs/specs/rhei-transition-cmd.spec.md:5`
- `docs/specs/rhei-transition-cmd.spec.md:13`
- `crates/rhei-cli/src/main.rs:198`
- `crates/rhei-cli/src/main.rs:203`
- `crates/rhei-cli/src/main.rs:206`
- `crates/rhei-cli/src/main.rs:209`
- `crates/rhei-cli/src/main.rs:212`

### MC-020-B: Backtick-wrapped CLI state values are not accepted as normalized state names

Classification: `implementation-diverges`

The transition spec says `--from` and `--to` state values follow main-spec rendering rules, including backtick-wrapped non-identifiers. `execute_transition` validates the raw CLI argument with `machine.is_valid_state(from)` / `machine.is_valid_state(to)` before any normalization, so a literal backtick-wrapped CLI value such as `` `in review` `` is not accepted even though markdown state metadata uses that rendering. Evidence:

- `docs/specs/rhei-transition-cmd.spec.md:20`
- `examples/escaped-state-values.rhei.md:10`
- `examples/states-with-spaces.yaml:10`
- `crates/rhei-cli/src/main.rs:4987`
- `crates/rhei-cli/src/main.rs:4988`
- `crates/rhei-cli/src/main.rs:4992`
- `crates/rhei-cli/src/main.rs:3771`

## MC-021: CAS, Locking, Transition Authorization, and Atomic Writes

### MC-021-A: CAS, locking, transition validation, counted visits, and atomic state writes are implemented

Classification: `no-discrepancy`

`execute_transition` opens and locks the metadata/task files, re-reads current state under lock, checks compare-and-swap state, validates declared transitions, updates counted-visit metadata, rewrites state, and writes through temp files. Evidence:

- `docs/specs/rhei-transition-cmd.spec.md:26`
- `docs/specs/rhei-transition-cmd.spec.md:27`
- `docs/specs/rhei-transition-cmd.spec.md:28`
- `docs/specs/rhei-transition-cmd.spec.md:31`
- `crates/rhei-cli/src/main.rs:4997`
- `crates/rhei-cli/src/main.rs:5024`
- `crates/rhei-cli/src/main.rs:5043`
- `crates/rhei-cli/src/main.rs:5060`
- `crates/rhei-cli/src/main.rs:5248`
- `crates/rhei-cli/src/main.rs:5283`
- `crates/rhei-cli/src/main.rs:5303`

### MC-021-B: `rhei transition` does not validate the whole plan before mutating

Classification: `missing-validation`

The transition spec requires loading and validating the state machine and plan. `transition_command` loads the plan and machine, but unlike `next_command` and `complete_command`, it does not call `validate_with_machine` before executing the transition. `execute_transition` parses enough to find the task and checks state/edge validity, but it does not perform whole-plan semantic validation. Evidence:

- `docs/specs/rhei-transition-cmd.spec.md:24`
- `crates/rhei-cli/src/main.rs:4924`
- `crates/rhei-cli/src/main.rs:4932`
- `crates/rhei-cli/src/main.rs:4948`
- `crates/rhei-cli/src/main.rs:5024`
- `crates/rhei-cli/src/main.rs:5060`
- contrast: `crates/rhei-cli/src/main.rs:9091`
- contrast: `crates/rhei-cli/src/main.rs:9305`

### MC-021-C: CAS conflict text is actionable but not the specified text

Classification: `implementation-diverges`

The transition spec's CAS conflict text is `Task <ID> is in state '<actual>', not '<from>'. Another transition may have preceded this call.` The implementation emits `conflict: Task <ID> is in state '<actual>', expected '<from>'`. Evidence:

- `docs/specs/rhei-transition-cmd.spec.md:44`
- `docs/specs/rhei-transition-cmd.spec.md:45`
- `docs/specs/rhei-transition-cmd.spec.md:46`
- `crates/rhei-cli/src/main.rs:5052`
- `crates/rhei-cli/src/main.rs:5053`

## MC-022: Artifact Enforcement and Callbacks

### MC-022-A: Required outputs and target inputs are enforced on transitions

Classification: `no-discrepancy`

The implementation checks required source outputs and target inputs during transition execution, and the input/output helper functions report task, state, artifact name, and path. Evidence:

- `docs/specs/rhei-transition-cmd.spec.md:29`
- `crates/rhei-cli/src/main.rs:5263`
- `crates/rhei-cli/src/main.rs:5272`
- `crates/rhei-cli/src/main.rs:4882`
- `crates/rhei-cli/src/main.rs:4906`
- `crates/rhei-cli/src/main.rs:4842`
- `crates/rhei-cli/src/main.rs:4870`

### MC-022-B: Output-check ordering conflicts between specs

Classification: `ambiguous-spec`

The transition-command spec lists output verification before `on_leave`; the states spec says outputs are checked after callbacks complete and before committing the transition. The implementation follows the latter ordering: it executes `on_leave`, resolves redirects, then checks outputs and inputs before writing. Evidence:

- `docs/specs/rhei-transition-cmd.spec.md:29`
- `docs/specs/rhei-transition-cmd.spec.md:30`
- `docs/specs/rhei-states.spec.md:248`
- `docs/specs/rhei-states.spec.md:249`
- `crates/rhei-cli/src/main.rs:5155`
- `crates/rhei-cli/src/main.rs:5263`
- `crates/rhei-cli/src/main.rs:5283`

## MC-023: Transition Audit Trail, Assignee Preservation, and Output

### MC-023-A: `rhei transition` does not append a result/audit entry

Classification: `implementation-diverges`

The transition spec requires each `rhei transition` to append a `## <from> -> <to>` / `## <from> → <to>` result entry to `runtime/results/<task-id>.md`. `execute_transition` only rewrites state and unlocks; it never calls `append_result_entry`. `append_result_entry` is called by `complete_command` only. Evidence:

- `docs/specs/rhei-transition-cmd.spec.md:33`
- `docs/specs/rhei-transition-cmd.spec.md:71`
- `docs/specs/rhei-complete.spec.md:47`
- `crates/rhei-cli/src/main.rs:5303`
- `crates/rhei-cli/src/main.rs:5363`
- `crates/rhei-cli/src/main.rs:9367`
- `crates/rhei-cli/src/main.rs:9371`
- `crates/rhei-cli/src/main.rs:9702`

### MC-023-B: Transition success output uses Unicode arrow and omits callbacks-skipped suffix

Classification: `implementation-diverges`

The transition spec specifies ASCII `->` in stdout and requires `(callbacks skipped)` when `--no-callbacks` is used. The implementation always prints a Unicode arrow and ignores `no_callbacks` in the success message. Evidence:

- `docs/specs/rhei-transition-cmd.spec.md:55`
- `docs/specs/rhei-transition-cmd.spec.md:56`
- `docs/specs/rhei-transition-cmd.spec.md:59`
- `docs/specs/rhei-transition-cmd.spec.md:62`
- `crates/rhei-cli/src/main.rs:4924`
- `crates/rhei-cli/src/main.rs:4930`
- `crates/rhei-cli/src/main.rs:4958`

### MC-023-C: Transition result trail and assignee-preservation behavior need explicit tests

Classification: `missing-test`

The scoped tests verify state changes and CAS behavior, but no visible test asserts that `rhei transition` appends a result entry or that transition preserves an existing `**Assignee:**` line. Evidence:

- `crates/rhei-cli/tests/e2e/transition_tests.rs:5`
- `crates/rhei-cli/tests/e2e/transition_tests.rs:22`
- `crates/rhei-cli/tests/e2e/transition_tests.rs:42`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:940`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:1106`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:1131`

## MC-030: `rhei complete` Usage, Required Result, and Output

### MC-030-A: Complete usage and required result flag are implemented

Classification: `no-discrepancy`

The CLI exposes `rhei complete <RHEI_PLAN> --task <TASK> --result <RESULT>` with optional `--no-callbacks`. Evidence:

- `docs/specs/rhei-complete.spec.md:5`
- `docs/specs/rhei-complete.spec.md:13`
- `crates/rhei-cli/src/main.rs:313`
- `crates/rhei-cli/src/main.rs:319`
- `crates/rhei-cli/src/main.rs:322`
- `crates/rhei-cli/src/main.rs:325`

### MC-030-B: Complete success output uses Unicode arrow instead of specified ASCII `->`

Classification: `implementation-diverges`

The complete spec's success output uses ASCII `->`; the implementation prints a Unicode arrow. Evidence:

- `docs/specs/rhei-complete.spec.md:94`
- `docs/specs/rhei-complete.spec.md:97`
- `crates/rhei-cli/src/main.rs:9382`
- `crates/rhei-cli/src/main.rs:9383`

## MC-031: Completion Eligibility and Target Selection

### MC-031-A: Terminal rejection, descendant blocking, and non-cancelled terminal target selection are implemented

Classification: `no-discrepancy`

`complete_command` rejects already terminal tasks and parent tasks with non-terminal descendants. `find_completion_state` selects the first reachable terminal state that is not `cancelled`. Evidence:

- `docs/specs/rhei-complete.spec.md:63`
- `docs/specs/rhei-complete.spec.md:65`
- `docs/specs/rhei-complete.spec.md:68`
- `crates/rhei-cli/src/main.rs:9322`
- `crates/rhei-cli/src/main.rs:9331`
- `crates/rhei-cli/src/main.rs:9340`
- `crates/rhei-cli/src/main.rs:9619`
- `crates/rhei-cli/src/main.rs:9624`
- `crates/rhei-cli/src/main.rs:9628`

### MC-031-B: Gating-state completion rejection is missing

Classification: `implementation-diverges`

Same behavioral gap as MC-001-B, recorded here for the complete command contract: `complete_command` does not inspect the current state's `gating` flag before completing. Evidence:

- `docs/specs/rhei-complete.spec.md:64`
- `crates/rhei-cli/src/main.rs:9319`
- `crates/rhei-cli/src/main.rs:9322`
- `crates/rhei-cli/src/main.rs:9331`
- `crates/rhei-cli/src/main.rs:9342`

## MC-032: Completion Result File, Link Placement, and Assignee Removal

### MC-032-A: Completion writes a result entry, removes assignee, and inserts the result link before child nodes

Classification: `no-discrepancy`

The completion path appends the mandatory result message, removes `**Assignee:**`, and inserts `> **Result:** ...` before the first child/descendant heading. Unit and integration coverage exists for these rewrite behaviors. Evidence:

- `docs/specs/rhei-complete.spec.md:27`
- `docs/specs/rhei-complete.spec.md:31`
- `docs/specs/rhei-complete.spec.md:70`
- `docs/specs/rhei-complete.spec.md:71`
- `crates/rhei-cli/src/main.rs:9367`
- `crates/rhei-cli/src/main.rs:9371`
- `crates/rhei-cli/src/main.rs:9765`
- `crates/rhei-cli/src/main.rs:9781`
- `crates/rhei-cli/src/main.rs:9792`
- `crates/rhei-cli/src/main.rs:9802`
- `crates/rhei-cli/src/main.rs:11727`

### MC-032-B: Result-link de-duplication is based on result-file existence, not link existence

Classification: `implementation-diverges`

The spec says the task body receives a result link if the result link is not already present. The implementation decides whether to insert the link based on whether `runtime/results/<task-id>.md` existed before appending the completion entry. If a result file already exists without a task-body link, the link is skipped. Evidence:

- `docs/specs/rhei-complete.spec.md:72`
- `docs/specs/rhei-complete.spec.md:75`
- `crates/rhei-cli/src/main.rs:9369`
- `crates/rhei-cli/src/main.rs:9370`
- `crates/rhei-cli/src/main.rs:9374`
- `crates/rhei-cli/src/main.rs:9379`

## MC-033: Completion Transition Execution

### MC-033-A: Completion appends only one result entry today

Classification: `no-discrepancy`

Because `execute_transition` currently does not append a result entry and `complete_command` appends exactly one entry, `rhei complete` does not duplicate entries in the current implementation. This no-discrepancy is coupled to MC-023-A's transition-audit discrepancy. Evidence:

- `docs/specs/rhei-complete.spec.md:69`
- `docs/specs/rhei-complete.spec.md:70`
- `crates/rhei-cli/src/main.rs:9357`
- `crates/rhei-cli/src/main.rs:9367`
- `crates/rhei-cli/src/main.rs:9371`

### MC-033-B: Complete is not atomic across transition, result append, and task-body rewrite

Classification: `implementation-diverges`

The complete spec describes completion as one atomic operation that transitions, appends the result entry, links the result file, removes assignee, and writes the task file atomically. The implementation runs `execute_transition`, which writes state and releases locks, then separately appends the result file and rewrites the task body. Failure after the state transition can leave the task terminal without the result/link/unassignment updates. Evidence:

- `docs/specs/rhei-complete.spec.md:3`
- `docs/specs/rhei-complete.spec.md:69`
- `docs/specs/rhei-complete.spec.md:70`
- `docs/specs/rhei-complete.spec.md:71`
- `docs/specs/rhei-complete.spec.md:72`
- `docs/specs/rhei-complete.spec.md:73`
- `crates/rhei-cli/src/main.rs:9357`
- `crates/rhei-cli/src/main.rs:9365`
- `crates/rhei-cli/src/main.rs:9367`
- `crates/rhei-cli/src/main.rs:9374`
- `crates/rhei-cli/src/main.rs:5363`
- `crates/rhei-cli/src/main.rs:5366`

## MC-040: `rhei reset` Usage, Scope, Safety, and Output

### MC-040-A: Reset command exists and prints the specified two-line shape

Classification: `no-discrepancy`

The CLI exposes `rhei reset <RHEI_PLAN>` and prints a reset count line plus either `Removed runtime output.` or `No runtime output was present.` Evidence:

- `docs/specs/rhei-reset.spec.md:5`
- `docs/specs/rhei-reset.spec.md:40`
- `docs/specs/rhei-reset.spec.md:43`
- `docs/specs/rhei-reset.spec.md:44`
- `docs/specs/rhei-reset.spec.md:54`
- `crates/rhei-cli/src/main.rs:329`
- `crates/rhei-cli/src/main.rs:9395`
- `crates/rhei-cli/src/main.rs:9429`
- `crates/rhei-cli/src/main.rs:9437`

### MC-040-B: Reset does not validate the plan before mutating

Classification: `missing-validation`

The reset spec says reset refuses to operate on an invalid plan. `reset_command` loads the plan and state machine but does not call `validate_with_machine` before rewriting task files and deleting runtime output. Evidence:

- `docs/specs/rhei-reset.spec.md:15`
- `crates/rhei-cli/src/main.rs:9395`
- `crates/rhei-cli/src/main.rs:9396`
- `crates/rhei-cli/src/main.rs:9397`
- `crates/rhei-cli/src/main.rs:9407`
- contrast: `crates/rhei-cli/src/main.rs:9091`
- contrast: `crates/rhei-cli/src/main.rs:9305`

## MC-041: Reset Semantics

### MC-041-A: Reset does not remove `**Assignee:**`

Classification: `implementation-diverges`

The reset spec requires removing `**Assignee:**` from every task node. The reset implementation rewrites all state lines to a single initial state, strips result links, and clears visit metadata; there is no assignee-stripping step in `reset_plan_file_states`, `strip_result_links`, or `rewrite_all_states_to_initial`. Evidence:

- `docs/specs/rhei-reset.spec.md:20`
- `crates/rhei-cli/src/main.rs:9476`
- `crates/rhei-cli/src/main.rs:9484`
- `crates/rhei-cli/src/main.rs:9485`
- `crates/rhei-cli/src/main.rs:9546`
- `crates/rhei-cli/src/main.rs:9570`

### MC-041-B: Reset resets every node to one legacy initial state, not each node's resolved profile initial

Classification: `implementation-diverges`

The reset spec requires resolving each node's profile through `node_policy` and resetting that node to the profile's `initial`. The implementation computes one machine-wide initial state by scanning `StateDef.initial` and applies it to every task/child state line. This fails profile-only machines and cannot support different profile initials for different node kinds. Evidence:

- `docs/specs/rhei-reset.spec.md:17`
- `docs/specs/rhei-reset.spec.md:18`
- `docs/specs/rhei-reset.spec.md:19`
- `docs/specs/rhei-states.spec.md:720`
- `crates/rhei-cli/src/main.rs:9398`
- `crates/rhei-cli/src/main.rs:9446`
- `crates/rhei-cli/src/main.rs:9450`
- `crates/rhei-cli/src/main.rs:9407`
- `crates/rhei-cli/src/main.rs:9570`

### MC-041-C: Reset removes result links, clears visit metadata, and deletes runtime

Classification: `no-discrepancy`

Aside from the assignee/profile issues above, reset does strip result links, clear `metadata.tasks.<id>.stateVisits`, and remove the runtime directory. Evidence:

- `docs/specs/rhei-reset.spec.md:21`
- `docs/specs/rhei-reset.spec.md:22`
- `docs/specs/rhei-reset.spec.md:23`
- `crates/rhei-cli/src/main.rs:9485`
- `crates/rhei-cli/src/main.rs:9488`
- `crates/rhei-cli/src/main.rs:9414`
- `crates/rhei-cli/src/main.rs:9422`

## MC-050: `rhei list` Usage and Filters

### MC-050-A: List command filter surface matches the spec

Classification: `no-discrepancy`

The CLI exposes the scoped filter flags, conflict constraints, comma/repeat state parsing, `--limit`, and `--json`. Evidence:

- `docs/specs/rhei-list.spec.md:16`
- `docs/specs/rhei-list.spec.md:20`
- `docs/specs/rhei-list.spec.md:21`
- `docs/specs/rhei-list.spec.md:25`
- `docs/specs/rhei-list.spec.md:28`
- `docs/specs/rhei-list.spec.md:30`
- `crates/rhei-cli/src/main.rs:135`
- `crates/rhei-cli/src/main.rs:140`
- `crates/rhei-cli/src/main.rs:148`
- `crates/rhei-cli/src/main.rs:165`
- `crates/rhei-cli/src/main.rs:179`
- `crates/rhei-cli/src/main.rs:185`
- `crates/rhei-cli/src/main.rs:191`
- `crates/rhei-cli/src/main.rs:194`

### MC-050-B: Dedicated list behavior tests are missing

Classification: `missing-test`

The visible e2e coverage for `rhei list` is dynamic shell completion of list filters, not behavioral tests for filtering, output shape, source order, hierarchy, read-only behavior, or JSON fields. Evidence:

- `crates/rhei-cli/tests/e2e/completions_tests.rs:403`
- `crates/rhei-cli/tests/e2e/completions_tests.rs:432`
- `crates/rhei-cli/tests/e2e/completions_tests.rs:446`
- `crates/rhei-cli/tests/e2e/completions_tests.rs:461`
- `crates/rhei-cli/tests/e2e/completions_tests.rs:475`
- `crates/rhei-cli/tests/e2e/completions_tests.rs:490`

## MC-051: List Read-Only Behavior and Readiness Semantics

### MC-051-A: List readiness uses the same non-cancelled terminal dependency rule as implementation `next`

Classification: `no-discrepancy`

`list_command` flattens tasks in source order, normalizes states, and uses `dependency_is_satisfied`, which requires a terminal state other than `cancelled`. It does not write files or take locks. Evidence:

- `docs/specs/rhei-list.spec.md:39`
- `docs/specs/rhei-list.spec.md:41`
- `docs/specs/rhei-list.spec.md:44`
- `docs/specs/rhei-list.spec.md:48`
- `crates/rhei-cli/src/main.rs:1332`
- `crates/rhei-cli/src/main.rs:1336`
- `crates/rhei-cli/src/main.rs:1355`
- `crates/rhei-cli/src/main.rs:1360`
- `crates/rhei-cli/src/main.rs:1431`
- `crates/rhei-cli/src/main.rs:8666`

## MC-052: List Text and JSON Output

### MC-052-A: List text and JSON shapes match the specified fields

Classification: `no-discrepancy`

The implementation prints one task per line with depth indentation, kind, id, title, state, optional priors, optional assignee, and prints the specified flat JSON fields. Empty text output exits successfully after printing the specified empty message. Evidence:

- `docs/specs/rhei-list.spec.md:54`
- `docs/specs/rhei-list.spec.md:63`
- `docs/specs/rhei-list.spec.md:66`
- `docs/specs/rhei-list.spec.md:71`
- `docs/specs/rhei-list.spec.md:89`
- `crates/rhei-cli/src/main.rs:1451`
- `crates/rhei-cli/src/main.rs:1455`
- `crates/rhei-cli/src/main.rs:1463`
- `crates/rhei-cli/src/main.rs:1473`
- `crates/rhei-cli/src/main.rs:1478`
- `crates/rhei-cli/src/main.rs:1488`
- `crates/rhei-cli/src/main.rs:1492`

## MC-060: `rhei states`

### MC-060-A: `rhei states` exists and prints states, instructions, artifacts, and transitions

Classification: `no-discrepancy`

The command loads the selected state machine and renders text or JSON. Text includes state descriptions, initial/final flags, visits, model/all_models, inputs, outputs, personality, instructions, and transitions. JSON includes a subset of those fields. Evidence:

- `docs/specs/rhei-usage.spec.md:91`
- `skills/rhei-plan-worker/SKILL.md:28`
- `crates/rhei-cli/src/main.rs:129`
- `crates/rhei-cli/src/main.rs:1292`
- `crates/rhei-cli/src/main.rs:3343`
- `crates/rhei-cli/src/main.rs:3377`
- `crates/rhei-cli/src/main.rs:3385`
- `crates/rhei-cli/src/main.rs:3402`
- `crates/rhei-cli/src/main.rs:3410`
- `crates/rhei-cli/src/main.rs:3439`

### MC-060-B: State inspection omits important schema fields needed by manual agents

Classification: `implementation-diverges`

The scope requires state inspection to show enough data for manual workers to understand gating, artifacts, visits, models/targets, and legal transitions. The renderer omits `gating` in text and JSON, and JSON omits `target`, `all_targets`, `agent`, `agent_mode`, `program`, `poll`, `mcp_servers`, `skills`, `profiles`, and `node_policy`. Text also omits targets, agent/program/poll/tooling/profile data. Evidence:

- `docs/specs/rhei-states.spec.md:68`
- `docs/specs/rhei-states.spec.md:72`
- `docs/specs/rhei-states.spec.md:73`
- `docs/specs/rhei-states.spec.md:75`
- `docs/specs/rhei-states.spec.md:77`
- `docs/specs/rhei-states.spec.md:81`
- `docs/specs/rhei-states.spec.md:84`
- `docs/specs/rhei-states.spec.md:86`
- `docs/specs/rhei-states.spec.md:88`
- `docs/specs/rhei-states.spec.md:707`
- `crates/rhei-cli/src/main.rs:3358`
- `crates/rhei-cli/src/main.rs:3362`
- `crates/rhei-cli/src/main.rs:3377`
- `crates/rhei-cli/src/main.rs:3380`
- `crates/rhei-cli/src/main.rs:3385`
- `crates/rhei-cli/src/main.rs:3439`
- `crates/rhei-cli/src/main.rs:3444`
- `crates/rhei-cli/src/main.rs:3456`

## MC-070: `rhei viz` Usage, CLI Integration, and Non-Mutation

### MC-070-A: The specified `rhei viz` CLI is not implemented

Classification: `implementation-diverges`

The viz spec requires a `rhei viz [PATH]` subcommand, CLI options, a `Commands::Viz` variant, `viz_command()`, and a `crates/rhei-viz` crate. The workspace has no `crates/rhei-viz` member, `Commands` has no `Viz` variant, and root CLI help has no `viz` command. Evidence:

- `docs/specs/rhei-viz.spec.md:20`
- `docs/specs/rhei-viz.spec.md:23`
- `docs/specs/rhei-viz.spec.md:33`
- `docs/specs/rhei-viz.spec.md:185`
- `docs/specs/rhei-viz.spec.md:187`
- `Cargo.toml:2`
- `Cargo.toml:9`
- `crates/rhei-cli/src/main.rs:96`
- `crates/rhei-cli/src/main.rs:98`
- `crates/rhei-cli/src/main.rs:335`

### MC-070-B: Existing visualization is an `xtask` prototype with a different command surface

Classification: `implementation-diverges`

The available visualization code lives under `xtask`, whose own comment says it dogfoods the future `rhei viz` command before the real subcommand ships. It exposes `cargo xtask examples viz`, writes under `target/rhei-viz`, and does not implement the `rhei viz` options from the spec. Evidence:

- `xtask/src/viz.rs:1`
- `xtask/src/viz.rs:3`
- `xtask/src/viz.rs:5`
- `xtask/src/main.rs:75`
- `xtask/src/main.rs:83`
- `xtask/src/main.rs:101`
- `xtask/src/main.rs:252`
- `xtask/src/main.rs:278`

## MC-071: Viz Views, Data Shape, and Plan State Derivation

### MC-071-A: The prototype implements the basic data shape and three tabs, but not through the specified crate/CLI

Classification: `implementation-diverges`

The `xtask` prototype uses `rhei-core`, serializes `{title, source, state, tasks[]}` with task/subtask data, escapes JSON for script embedding, and the HTML has Gantt/Cube/Sankey tabs. Because it is not available through `rhei viz` or `crates/rhei-viz`, the specified implementation target remains absent. Evidence:

- `docs/specs/rhei-viz.spec.md:45`
- `docs/specs/rhei-viz.spec.md:157`
- `docs/specs/rhei-viz.spec.md:183`
- `xtask/src/viz.rs:13`
- `xtask/src/viz.rs:21`
- `xtask/src/viz.rs:31`
- `xtask/src/viz.rs:40`
- `xtask/src/viz.rs:48`
- `xtask/src/viz.rs:243`
- `xtask/assets/viz-template.html:69`
- `xtask/assets/viz-template.html:76`
- `xtask/assets/viz-template.html:80`
- `xtask/assets/viz-template.html:84`

### MC-071-B: Prototype plan-state derivation is broader than the spec's active-state list

Classification: `implementation-diverges`

The viz spec's active derivation is based on a fixed active-state list (`in_progress`, `needs-review`, `review`, `prove`, `consolidate`, `agent-review`) plus terminal declarations. The prototype marks any non-terminal state other than `draft` and the machine initial as `active`. This is observably different for custom machines with non-terminal waiting or blocked states. Evidence:

- `docs/specs/rhei-viz.spec.md:101`
- `docs/specs/rhei-viz.spec.md:106`
- `docs/specs/rhei-viz.spec.md:109`
- `xtask/src/viz.rs:147`
- `xtask/src/viz.rs:176`
- `xtask/src/viz.rs:178`
- `xtask/src/viz.rs:182`

## MC-072: Viz Serving Mode

### MC-072-A: `--serve` live mode is absent

Classification: `implementation-diverges`

The viz spec requires `rhei viz --serve` with loopback HTTP, file watching, plan updates, and disconnect shutdown. There is no `rhei viz` command, and the `xtask` prototype only renders static HTML files for examples. Evidence:

- `docs/specs/rhei-viz.spec.md:123`
- `docs/specs/rhei-viz.spec.md:125`
- `docs/specs/rhei-viz.spec.md:127`
- `docs/specs/rhei-viz.spec.md:128`
- `docs/specs/rhei-viz.spec.md:129`
- `xtask/src/main.rs:83`
- `xtask/src/main.rs:101`
- `xtask/src/main.rs:246`
- `xtask/src/main.rs:247`

## MC-080: CLI Diagnostics

### MC-080-A: JSON-mode command errors are rendered as JSON

Classification: `no-discrepancy`

The CLI detects JSON output modes for `next`, `states`, `list`, `templates`, and JSON render, and emits a JSON error object when dispatch fails. Evidence:

- `crates/rhei-cli/src/main.rs:456`
- `crates/rhei-cli/src/main.rs:458`
- `crates/rhei-cli/src/main.rs:472`
- `crates/rhei-cli/src/main.rs:483`

### MC-080-B: Parse and validation diagnostics include file/problem context

Classification: `no-discrepancy`

The CLI has dedicated parse and validation diagnostic rendering functions, and tests cover parse/validation failure surfaces. Evidence:

- `crates/rhei-cli/src/main.rs:10945`
- `crates/rhei-cli/src/main.rs:10993`
- `crates/rhei-cli/src/main.rs:11026`
- `crates/rhei-cli/src/main.rs:11471`
- `crates/rhei-cli/src/main.rs:11487`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:441`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:463`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:503`
- `crates/rhei-cli/tests/integration_markdown_plans.rs:670`

### MC-080-C: Several command diagnostics do not match exact specified text

Classification: `implementation-diverges`

Missing-artifact diagnostics are aligned, but exact no-task, CAS, and success-output diagnostics differ from their command specs as recorded in MC-014-B, MC-021-C, MC-023-B, and MC-030-B. Evidence:

- `docs/specs/rhei-next.spec.md:100`
- `docs/specs/rhei-transition-cmd.spec.md:44`
- `docs/specs/rhei-transition-cmd.spec.md:55`
- `docs/specs/rhei-complete.spec.md:94`
- `crates/rhei-cli/src/main.rs:8733`
- `crates/rhei-cli/src/main.rs:5052`
- `crates/rhei-cli/src/main.rs:4958`
- `crates/rhei-cli/src/main.rs:9382`
