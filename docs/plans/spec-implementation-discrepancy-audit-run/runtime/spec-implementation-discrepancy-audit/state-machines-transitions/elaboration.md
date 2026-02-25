# State Machines and Transitions Elaboration

Task: `state-machines-transitions`

Source discrepancy file: `runtime/spec-implementation-discrepancy-audit/state-machines-transitions/discrepancies.md`

This elaboration consolidates duplicate findings, marks weaker findings as tentative, and records areas where the audit found no discrepancy. It does not choose a reconciliation strategy.

## Consolidated Discrepancies

### SMT-E001: Default state-machine schema and shipped defaults are split between current and legacy formats

Source discrepancies: SMT-001, SMT-015.

Exact mismatch: The current state spec makes `profiles` and `node_policy` required, moves initial-state selection into profiles, and points to `docs/specs/states.yaml` as the authoritative machine-readable default. The compiled default loaded by `StateMachine::builtin_default()` is `crates/rhei-validator/src/default-states.yaml`, version `2.0`, still using `draft.initial: true` and no profiles/node policy. The validator also still accepts machines where both `profiles` and `node_policy` are omitted, and it defaults missing `transitions` to an empty list even though the transition spec lists transitions as required. Several shipped examples/templates still use state-level `initial: true`.

Why it matters: Users can see or generate a spec-conformant profile-based machine in docs/skills while the CLI's built-in behavior and some examples still demonstrate or execute the legacy shape. A workflow can validate under the implementation while not matching the current spec, and the default machine can differ depending on whether a user follows docs, plan-writer references, or compiled runtime behavior.

Affected: Plan authors, state-machine authors, template users, `rhei states`, `rhei validate`, `rhei next`, `rhei reset`, and any code assuming the built-in default is the spec `3.0` default.

Risk: User-facing. The mismatch affects visible YAML, generated templates, command behavior, and validation outcomes.

Verification currently exists: There is implementation evidence in `StateMachine`, `StateDef::initial`, and `default-states.yaml`; there are tests around profile/node-policy validation when one side is present or references are invalid. There is no evidence in the discrepancy file of a test that compares `docs/specs/states.yaml`, the plan-writer reference, and the compiled default, or that rejects machines omitting both `profiles` and `node_policy`.

### SMT-E002: Profile and node-policy semantics are only partially implemented

Source discrepancies: SMT-002, SMT-014.

Exact mismatch: The spec defines `node_policy.overrides[].match` as an object with optional `type` and `level`. The implementation models it as a string `pattern` and compares it exactly against task id. The spec requires each profile's `allowed` set to include at least one final state and requires every non-final allowed state to have a path to a final state. The validator checks names, membership, duplicates, and references, but not final-state presence or graph reachability. The spec also requires `by_type` keys and override `type`/`level` values to match declared plan structure; the implementation validates only empty/reserved `by_type` keys and profile references, and does not represent structured override type/level. Runtime reset and next/claim logic still use `StateDef::initial`, not the resolved profile initial.

Why it matters: A profile can be accepted even if tasks assigned to it can never complete. Node-specific policies described by the spec and emitted by the state-machine-writer shape cannot be represented by the current validator. Profile-specific initial states may not drive reset or claimability, so different node kinds can be reset or claimed according to a legacy global state flag rather than their profile.

Affected: Custom state-machine authors, state-machine-writer users, workflows with multiple node kinds, workflows relying on profile-specific initial states, `rhei validate`, `rhei reset`, and `rhei next`.

Risk: User-facing. It can cause machines that appear valid to be unrepresentable, dead-ended, or scheduled incorrectly.

Verification currently exists: Validator tests cover some profile/node-policy structure, such as missing paired blocks, undefined profiles, reserved `rhei` in `by_type`, duplicate allowed entries, and state membership. The discrepancy file did not identify tests for profile final-state reachability, structured override matching by type/level, by-type validation against declared node kinds, or reset/next behavior through resolved profile initial states.

### SMT-E003: Required state and transition descriptions are not enforced or preserved consistently

Source discrepancies: SMT-003, SMT-009, SMT-014.

Exact mismatch: The transition and writer specs require every state and every transition to carry a human-readable `description`. `StateDef.description` is optional and documented as permissive. `TransitionRule` has no `description` field, so transition descriptions are not required by loading and cannot be preserved through the core transition AST.

Why it matters: The spec treats descriptions as part of the authoring and monitoring contract. If runtime loading drops or permits missing descriptions, generated state machines can pass implementation validation while producing weaker CLI output, weaker monitoring surfaces, and less inspectable workflow definitions than the spec requires.

Affected: State-machine authors, generated machines from the state-machine-writer role, docs/monitoring tools, and any downstream tooling expecting descriptions in loaded transition data.

Risk: Mostly user-facing, because descriptions are author-facing and operator-facing metadata. There is also an internal tooling risk for code that expects the AST to carry descriptions.

Verification currently exists: The discrepancy file identifies implementation structure showing `StateDef.description: Option<String>` and no `TransitionRule.description`. It does not identify validation tests that reject missing descriptions or serialization tests that preserve transition descriptions.

### SMT-E004: Target, model, and gating validation does not fully match spec-level resolution rules

Source discrepancies: SMT-003, SMT-008.

Exact mismatch: The states spec requires target selectors to resolve agent ids and modes against the merged agent registry at validation time. The implementation parses selector grammar and validates mutual exclusions but does not resolve target agent ids or modes during state-machine loading. The states spec says `agent` on a gating state is a validation warning, while program/tooling on gating states are validation errors. The implementation has validation errors for several autonomous fields on final/gating states, but the audit did not find a state-machine warning channel or a check that emits the gating-agent warning. Separately, `{model.provider}` and `{model.name}` do not resolve from settings-backed legacy `model` / `all_models`; provider comes only from inline targets, and name resolves to the model id.

Why it matters: Machines can pass validation even when target selectors refer to agents or modes that will not resolve later. Prompt/artifact templates can also receive different model values than the spec advertises, especially in legacy model-profile workflows. The missing gating-agent warning weakens feedback for a configuration that will never spawn autonomous work.

Affected: State-machine authors using `target`, `all_targets`, `model`, `all_models`, or `agent` on gates; `rhei validate`; `rhei next`; `rhei run`; callback/template consumers.

Risk: User-facing. Invalid execution identities surface late, and template output can be wrong or misleading.

Verification currently exists: Tests and validator code cover selector grammar, target/all-target mutual exclusion, legacy-field exclusion, `model`/`all_models` checks, agent/program mutual exclusion, program shape, and program/tooling final/gating exclusions. The discrepancy file did not identify tests that resolve target agents/modes against settings at validation time or assert the gating-agent warning behavior. The warning-channel part is tentative because the spec calls for a warning but the current validator API appears error-oriented.

### SMT-E005: Final and gating states are not uniformly absorbing for manual and completion commands

Source discrepancies: SMT-004, SMT-009.

Exact mismatch: The transition spec says final states are absorbing and wildcard transitions match any non-final source only. `execute_transition` validates `from`/`to`, compare-and-swap, and then applies exact or wildcard transition rules without first rejecting a final source state. This means a wildcard edge can permit a manual transition from a final state. The states and transitions specs also say autonomous commands, including `rhei complete`, must not transition out of a gating state. `complete_command` rejects terminal states and open children, then asks `find_completion_state` for a one-hop terminal transition; `find_completion_state` has no gating check.

Why it matters: Final states represent workflow closure and should be stable. Gating states represent human authority. Allowing a wildcard out of a final state or allowing `rhei complete` out of a gate would bypass those contracts if the transition table contains a matching edge.

Affected: Users running `rhei transition` manually, users running `rhei complete`, workflows with wildcard cancellation/completion edges, and any automation relying on terminal/gating states as authority boundaries.

Risk: User-facing and workflow-integrity risk.

Verification currently exists: Tests cover invalid transitions, CAS conflicts, wildcard cancellation, and completion selection that prefers non-cancelled terminals and refuses to fall back to `cancelled`. Ready-task discovery is tested/implemented to skip terminal and gating states for `rhei run` and automatic selection. The discrepancy file did not identify tests proving `rhei transition` rejects wildcard transitions from final states or `rhei complete` refuses gating states.

### SMT-E006: Counted visit accounting is not scoped per target/model fanout

Source discrepancies: SMT-005.

Exact mismatch: The spec says counted visit accounting is scoped per model or target when a state uses `all_models` or `all_targets`. The implementation stores counters at `metadata.tasks.<id>.stateVisits.<state>` and functions such as `ensure_current_state_visit_count` and `update_metadata_for_transition` key only by task id and state name. There is no target or model dimension in the persisted counter shape.

Why it matters: In fanout states, one target/model can consume or increment the visit budget for another. That makes retry/review budgets shared across executions that the spec defines as isolated.

Affected: Multi-target and multi-model workflows using `visits`, including independent review loops and fanout agent states.

Risk: User-facing for workflows using fanout counted loops; internal for metadata-shape consumers.

Verification currently exists: Existing tests cover counted-loop basics: invalid suffixes, `visits: 0` rejection, metadata updates, rendering suffixes for visits greater than one, condition operands, and loop exhaustion. The discrepancy file explicitly notes no found test proving per-target or per-model counter isolation.

### SMT-E007: Polling states are validated but not implemented as runtime scheduling semantics

Source discrepancies: SMT-006, SMT-013.

Exact mismatch: The states and transitions specs define `poll.interval`, `poll.max_attempts`, `pollNextAttemptAt`, `pollAttempts`, `pollMaxAttempts`, self-loop retry behavior, exhaustion behavior, slot release between attempts, and concurrent poll scheduling. The validator checks poll shape and self-loop presence, but the CLI runtime does not reference `pollNextAttemptAt`, `pollAttempts`, or `pollMaxAttempts`. Condition operands support counted visits and numeric task metadata, not poll aliases. `state_visit_limit` reads only `StateDef::visits`, while poll states are required to omit `visits`, so poll attempt caps are not connected to the loop-budget helpers. The shipped `ci-heal` example explicitly says poll behavior applies once the block is wired through `rhei run`.

Why it matters: Users can author and validate a poll state that the engine treats like an ordinary ready state. Time-based waits, attempt caps, slot release, and exhaustion transitions will not behave as specified.

Affected: `rhei run`, workflows waiting on CI/deployments/external systems, examples that teach poll workflows, and any transition conditions using poll operands.

Risk: User-facing. The feature appears available in schema validation and examples but is not executable as specified.

Verification currently exists: Validator tests cover well-formed poll states, invalid intervals, zero attempts, mutual exclusion with visits, gating/final exclusions, and required self-loops. The discrepancy file did not identify runtime tests for poll deadlines, poll operands, max-attempt exhaustion, or concurrent poll scheduling.

### SMT-E008: Artifact enforcement order and artifact/template variable namespaces differ from the spec and examples

Source discrepancies: SMT-007, SMT-008.

Exact mismatch: The transition spec says source outputs are checked after callbacks complete and before the state write is committed. The implementation runs `on_leave`, resolves redirects, checks source outputs and target inputs, writes the state, and only then runs `on_enter`, with rollback on `on_enter` failure. This means artifact checks happen before `on_enter`, not after all callbacks. Separately, current artifact path resolution supports variables such as `{task_id}`, `{visit_count}`, `{target}`, `{target.slug}`, `{agent}`, `{model}`, and selected model/agent subfields; shipped examples use `{task.id}`, `{visit}`, and `{task.metadata.branch}`, which are left literal or unresolved because the runtime namespace uses `{task_id}`, `{visit_count}`, and `{meta.<key>}`.

Why it matters: Callback authors and artifact-contract authors can disagree with the engine about when a file is allowed to be produced. Example users can copy paths or env variables that never resolve. Literal braces in paths can create unexpected artifact locations.

Affected: `rhei transition`, `rhei complete`, callback authors, program states, examples such as `examples/ci-heal`, and workflows that use artifact paths as completion signals.

Risk: User-facing. It affects command success/failure and files written or checked in the workspace.

Verification currently exists: Tests cover artifact validation, optional input behavior, output existence enforcement, and transition failure when target inputs or completion outputs are missing. The discrepancy file did not identify tests for callback/artifact ordering or tests that validate shipped example variable names against the runtime namespace.

### SMT-E009: System-triggered transitions do not carry the specified trigger classification or payloads

Source discrepancies: SMT-010.

Exact mismatch: The transition spec classifies program exit-code transitions, agent timeout transitions, and tooling-unavailable transitions as `triggeredBy: "system"` and describes timeout/tooling-specific transition data. Program exit-code and timeout paths delegate to `execute_transition`, whose `on_leave` context uses `triggeredBy: "user"` and whose `on_enter` context switches only to `"callback"` when there was a callback redirect. `fire_timeout_transition` has no parameter for timeout/agent payload data and starts with empty `transitionData`. Tooling-unavailable transition fields are parsed and structurally validated (`mcp_unavailable`, `skill_unavailable`), but the audit found no runtime path in `rhei-cli` that selects these transitions when required MCP servers or skills are unavailable.

Why it matters: Callbacks cannot distinguish manual transitions from engine/system transitions according to the spec. Timeout callbacks do not receive the timeout metadata the spec describes. Tooling-unavailable recovery edges can validate but never fire at runtime.

Affected: Callback authors, `rhei run`, program states, agent timeout handling, MCP/skill-dependent workflows, and monitoring/audit consumers of transition context.

Risk: User-facing for automated workflows and callback behavior; internal for event/metadata consumers.

Verification currently exists: Tests cover program exit-code matching order and condition filtering. Validator tests cover `mcp_unavailable` shape. Callback tests cover receiving context and callback redirect/rejection behavior. The discrepancy file did not identify tests asserting `triggeredBy: "system"`, timeout transition payloads, or runtime firing of tooling-unavailable transitions.

### SMT-E010: Callback platform, mapping, preflight, and error-handling support are narrower than the transition spec

Source discrepancies: SMT-011.

Exact mismatch: The transition spec lists CLI, JavaScript/nodejs, Python, and Java callback platforms, supports platform-prefixed identifiers and top-level logical `callbacks:` mappings, defines `error_handling:`, and requires registered callbacks to be verified as callable before execution begins. The runtime executor accepts only `cli:` callbacks through `ShellCallbackExecutor`; the NAPI crate exposes only `version()` and `help()`. `StateMachine` does not represent top-level `callbacks:` or `error_handling:`, so serde ignores those sections and the CLI cannot resolve logical callback names through mappings. Callback commands are spawned when a transition executes, not preflighted before execution begins.

Why it matters: A state machine can be valid per spec but non-executable in the current runtime. Logical callback names and non-CLI examples in the spec do not have corresponding runtime resolution. Callback failures happen at transition time rather than at workflow startup or validation time.

Affected: State-machine authors, callback authors, SDK/binding users, JavaScript/Python/Java examples, and workflows relying on declarative callback mappings or error policies.

Risk: User-facing for authored workflows; internal for platform API parity.

Verification currently exists: CLI callback tests cover optional callbacks, rejection, redirect, data accumulation, `--no-callbacks`, unknown callback platform errors, and `on_enter` rollback. Unit tests cover `ShellCallbackExecutor`. The discrepancy file did not identify tests for JS/Python/Java callback execution, top-level callback mappings, `error_handling`, or callback preflight callability.

### SMT-E011: Callback TransitionContext omits the spec's active state object and counted-loop visit metadata

Source discrepancies: SMT-012.

Exact mismatch: The transition spec's `TransitionContext` includes a top-level `state` object containing the active state definition and resolved artifact contracts, including `exists` values. `build_transition_context_json` currently emits `rhei`, `task`, `transition`, `transitionData`, and `environment`, but no top-level `state`. The spec also describes counted-loop callback metadata such as `task.metadata.visitCount`; the context builder merges persisted metadata plus implicit `state` and `dependsOn`, but does not synthesize a visit-count field.

Why it matters: Callbacks cannot inspect active state contracts or artifact existence through the documented context shape. Counted-loop callbacks must infer visits from other metadata or cannot rely on the documented field.

Affected: Callback authors, monitoring integrations, counted-loop workflows, and any callback that needs state instructions/artifacts.

Risk: User-facing for callback API consumers; internal for transition-context schema compatibility.

Verification currently exists: The audit found no discrepancy for implicit `task.metadata.state`, implicit `dependsOn`, custom task metadata merging, or environment fields (`platform`, version, working directory). Callback tests verify that callbacks receive context, but the discrepancy file did not identify tests asserting the required `state` object or synthesized visit-count metadata.

### SMT-E012: Concurrent scheduling semantics are implemented for agent batches only

Source discrepancies: SMT-013.

Status: Tentative.

Exact mismatch: The states spec describes `concurrent` as applying to `rhei run` scheduling generally. The implementation enforces non-concurrent grouping/defer behavior in the `agent_tasks` batch. Program and callback-only progression are sequential for other control-flow reasons, and the audit did not find an equivalent scheduling branch applying the `concurrent` flag to those modes. Poll-state concurrency also depends on poll deadlines, which are not implemented.

Why it matters: If users read `concurrent` as a general scheduling contract, program or callback-only workflows may not honor the same visible rule. If the current sequential behavior is intentional, the spec is broader than the implementation surface.

Affected: `rhei run --parallel`, agent states, program states, callback-only runs, and poll workflows.

Risk: Mixed. The agent-state behavior is user-facing and implemented. The non-agent mismatch is tentative because other sequencing rules may make the flag irrelevant rather than observably wrong.

Verification currently exists: Implementation evidence shows `StateDef.concurrent` and run-time grouping for agent tasks. The discrepancy file did not identify parallel scheduling tests for program/callback-only states or poll deadline concurrency.

### SMT-E013: State-machine-writer output guidance can produce runtime-incompatible shapes

Source discrepancies: SMT-014.

Exact mismatch: The state-machine-writer spec and skill mirror current spec guidance: profiles/node policy are required, state-level `initial: true` is forbidden, transition descriptions are required, human gates use `gating: true`, profile final-state reachability is required, object-shaped overrides are shown, and top-level callbacks are allowed. Several of those shapes are not represented or enforced by the runtime: object-shaped `overrides[].match` is not accepted, top-level callback mappings are ignored, transition descriptions are not preserved, and profile reachability is not checked. The writer spec also contains a stale output skeleton that says state instructions may describe "when to transition out", while later guidance says autonomous instructions under orchestrator authority must not tell actors when to call transition commands.

Why it matters: The writer role can generate YAML that follows the writer spec but fails to load or loses data in the runtime. It can also teach conflicting instruction style.

Affected: Users invoking the state-machine-writer skill, templates derived from its examples, state-machine authors, and validators.

Risk: User-facing. This affects generated artifacts and authoring guidance.

Verification currently exists: The skill itself contains checks aligned with the current spec. The discrepancy file did not identify an automated fixture that runs generated writer output through the current validator/executor. The stale-instruction point is documentation-only and therefore weaker than runtime mismatches.

### SMT-E014: Raw template state machines with Mustache placeholders are not valid state machines before instantiation

Source discrepancies: SMT-015.

Status: Tentative / ambiguous-spec.

Exact mismatch: Some raw template `states.yaml` files contain Mustache placeholders in YAML scalar positions, for example `target: {{audit_target}}`. Directly loading those files as state machines can fail because YAML parses the placeholder syntax as a map rather than a string. Instantiated examples use concrete selectors and can be valid.

Why it matters: It is unclear whether raw templates are meant to be valid input to `rhei states --state-machine` before instantiation. If they are, the template files violate the same state-machine validation path users are told to use. If they are only templates, the spec should not imply direct validation of raw template YAML.

Affected: Template authors, template tests, CLI users inspecting bundled templates, and documentation/examples around `rhei states --state-machine`.

Risk: Mostly internal/tooling unless users run CLI validation directly on raw templates.

Verification currently exists: The discrepancy file records a command result showing direct load failure for at least one raw template. It also records that instantiated audit and changeset examples include current gating states. No broader template-validation test is identified.

## Areas With No Discrepancy Found

- `rhei complete` selects a one-hop non-cancelled terminal transition and refuses to fall back to `cancelled`.
- Ready-task discovery skips terminal and gating states, so `rhei run`, callback scheduling, and automatic ready selection avoid autonomous work from gates and terminals.
- Basic counted-loop mechanics exist: `visits: 0` is rejected, invalid counted suffixes are rejected, metadata stores `metadata.tasks.<id>.stateVisits.<state>`, rendered markdown writes suffixes only for visits greater than one, and visit operands are available for transition conditions.
- Polling state schema validation exists for shape, duration, max attempts, final/gating exclusions, mutual exclusion with `visits`, and self-loop presence.
- Artifact definition validation exists for non-empty unique names, relative non-escaping paths, optional input behavior, and required output checks. Optional input path/existence values are exposed to instructions and programs.
- Runtime instruction/personality templating is implemented for `rhei next`, unknown variables fail open, and conditional blocks for inputs, MCP availability, and skill availability are implemented.
- Several state structural validations are implemented: `target`/`all_targets` mutual exclusion, target/legacy-field exclusion, `model`/`all_models` checks, agent/program mutual exclusion, program shape and final/gating exclusions, timeout parsing, artifact validation, poll validation, and MCP/skill duplicate/exclusion checks.
- Manual transitions enforce valid state names, compare-and-swap `--from`, declared exact/wildcard edges, conditions, counted-loop applicability, required outputs/inputs, and callback redirect declarations, subject to the final-state wildcard gap recorded above.
- `find_next_transition` prefers exact transitions before wildcard transitions and skips wildcard transitions to terminal states for forward progress.
- Program exit-code routing prefers specific integer/array transitions before `"nonzero"` transitions and applies transition conditions while selecting.
- CLI callbacks support optional `on_leave`/`on_enter`, rejection, redirect, data accumulation, `success: false` with `nextState` downgrade, `--no-callbacks`, and `on_enter` rollback.
- Callback task metadata includes implicit `state` and `dependsOn`, and custom frontmatter task metadata is merged without clobbering those canonical fields.
- Callback environment context includes platform, package version, and working directory.
- `StateDef.concurrent` exists, and `rhei run` applies non-concurrent scheduling for agent task batches by keeping fanout invocations for the same task together and deferring other tasks in the same non-concurrent state to later passes.
- The state-machine-writer skill mirrors several current spec rules: profiles/node policy are required, state-level `initial: true` is not a state field, transitions require descriptions, human gates use `gating: true`, and profiles need final-state reachability.
- Current profile-based shipped audit and changeset examples include gating human-review states.
