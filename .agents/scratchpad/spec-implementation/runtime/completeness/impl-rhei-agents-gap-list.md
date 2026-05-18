# Aggregated Completeness Gaps: impl-rhei-agents

Spec: `docs/functional-spec/rhei-agents.spec.md`

Inputs:

- `runtime/completeness/impl-rhei-agents-gaps-claude-code-yolo-anthropic-claude-opus-4-7.md`
- `runtime/completeness/impl-rhei-agents-gaps-codex-xhigh-openai-gpt-5-5.md`

Aggregation rule: retained items marked `missing` or `partial` by any reviewer unless all reviewers marked the same requirement covered with concrete evidence. Where reviewers disagree, the disagreement is preserved for `completeness-fix`.

## Overview

- `partial` / disputed: `rhei run` spawns agents and remains transition authority, but one reviewer found it uses `find_ready_tasks` rather than `rhei next` claimability and does not check assignees. Another reviewer marked the overview behavior covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:9`, `crates/rhei-cli/src/main.rs:7808`, `crates/rhei-cli/src/main.rs:9123`, `crates/rhei-cli/src/main.rs:9168`, `crates/rhei-cli/src/main.rs:8311`, `crates/rhei-cli/src/main.rs:8343`, `crates/rhei-cli/src/main.rs:8439`; disputed covered evidence includes `crates/rhei-cli/src/main.rs:7730`, `crates/rhei-cli/src/main.rs:7227`, `crates/rhei-cli/src/main.rs:8439`.

## Agent Configuration

- `partial` / disputed: settings locations are partially implemented. One reviewer found project settings load only from `<plan-root>/.rhei/settings.json`, while the spec allows workspace or plan directory settings. Another reviewer marked settings load covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:31`, `crates/rhei-cli/src/main.rs:5922`, `crates/rhei-cli/src/main.rs:5928`, `crates/rhei-cli/src/main.rs:7688`.

- `partial` / disputed: settings schema composition does not fully match the spec-shaped `defaults` block. One reviewer found nested `defaults.model`, `defaults.agent`, and `defaults.program_timeout` absent or ignored, while legacy top-level `model`, `agent`, and `program_timeout` are supported. Another reviewer marked some of these default fields covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:38`, `docs/functional-spec/rhei-agents.spec.md:141`, `docs/functional-spec/rhei-agents.spec.md:142`, `docs/functional-spec/rhei-agents.spec.md:145`, `crates/rhei-cli/src/main.rs:5711`, `crates/rhei-cli/src/main.rs:5715`, `crates/rhei-cli/src/main.rs:5719`, `crates/rhei-cli/src/main.rs:5779`, `crates/rhei-cli/src/main.rs:5964`, `crates/rhei-cli/src/main.rs:5966`, `crates/rhei-cli/src/main.rs:5968`, `crates/rhei-cli/src/main.rs:6378`, `crates/rhei-cli/src/main.rs:6390`, `crates/rhei-cli/src/main.rs:6842`.

- `partial` / disputed: settings parse failures and inline-agent defaults may not produce explicit validation errors. One reviewer notes settings parse failures silently become defaults and inline `defaults.agent` rejection is not explicit. Evidence: `docs/functional-spec/rhei-agents.spec.md:38`, `docs/functional-spec/rhei-agents.spec.md:142`, `crates/rhei-cli/src/main.rs:5913`, `crates/rhei-cli/src/main.rs:5916`, `crates/rhei-cli/src/main.rs:5723`, `crates/rhei-cli/src/main.rs:5779`.

- `partial`: agent profile `command` is required by the spec, but an empty command can deserialize and then panic via `expect` during command construction instead of producing a validation error. Evidence: `docs/functional-spec/rhei-agents.spec.md:158`, `crates/rhei-validator/src/lib.rs:165`, `crates/rhei-validator/src/lib.rs:167`, `crates/rhei-cli/src/main.rs:6941`.

- `missing`: `mcp_flag` and `mcp_config_flag` are mutually exclusive by spec, but no profile/settings validation enforces mutual exclusion. Evidence: `docs/functional-spec/rhei-agents.spec.md:163`, `docs/functional-spec/rhei-agents.spec.md:164`, `crates/rhei-validator/src/lib.rs:180`, `crates/rhei-validator/src/lib.rs:184`, `crates/rhei-cli/src/main.rs:6928`.

- `missing`: agent profile `session` is absent, so snapshot session support and unsupported-session behavior cannot be represented. Evidence: `docs/functional-spec/rhei-agents.spec.md:167`, `docs/functional-spec/rhei-agents.spec.md:397`, `crates/rhei-validator/src/lib.rs:165`, `crates/rhei-validator/src/lib.rs:196`.

- `partial` / disputed: `models.<id>.provider` and `models.<id>.model` are required by the spec, but one reviewer found they are optional and not validated as required. Another reviewer marked these fields covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:176`, `docs/functional-spec/rhei-agents.spec.md:180`, `docs/functional-spec/rhei-agents.spec.md:181`, `crates/rhei-cli/src/main.rs:5731`, `crates/rhei-cli/src/main.rs:5742`, `crates/rhei-cli/src/main.rs:5745`, `crates/rhei-cli/src/main.rs:5749`.

- `partial`: `models.<id>.agents.<agent-id>.args` and `autonomous_args` are parsed but not appended to spawned commands. Evidence: `docs/functional-spec/rhei-agents.spec.md:185`, `docs/functional-spec/rhei-agents.spec.md:189`, `docs/functional-spec/rhei-agents.spec.md:190`, `crates/rhei-cli/src/main.rs:5760`, `crates/rhei-cli/src/main.rs:5765`, `crates/rhei-cli/src/main.rs:6441`, `crates/rhei-cli/src/main.rs:6950`.

- `partial` / `missing`: MCP server profiles lack required validation and availability behavior. Gaps include exact-one-of `command` or `url`, remote `url` requiring/validating `transport`, `${VAR}` environment expansion, default/enforced `startup_timeout`, and live startup handshake. Evidence: `docs/functional-spec/rhei-agents.spec.md:195`, `docs/functional-spec/rhei-agents.spec.md:201`, `docs/functional-spec/rhei-agents.spec.md:202`, `docs/functional-spec/rhei-agents.spec.md:204`, `docs/functional-spec/rhei-agents.spec.md:206`, `docs/functional-spec/rhei-agents.spec.md:208`, `crates/rhei-validator/src/lib.rs:204`, `crates/rhei-validator/src/lib.rs:220`, `crates/rhei-cli/src/main.rs:6041`, `crates/rhei-cli/src/main.rs:6066`.

- `partial` / `missing`: skill profile `path` exists, but `~` expansion, existence checks, readability checks, and spawn-time skill wiring are incomplete. Resolved skills must be wired only when the agent declares `skill_flag`; otherwise they should be skipped with a warning. Evidence: `docs/functional-spec/rhei-agents.spec.md:212`, `docs/functional-spec/rhei-agents.spec.md:218`, `docs/functional-spec/rhei-agents.spec.md:219`, `docs/functional-spec/rhei-agents.spec.md:221`, `crates/rhei-validator/src/lib.rs:227`, `crates/rhei-cli/src/main.rs:6225`, `crates/rhei-cli/src/main.rs:6068`, `crates/rhei-cli/src/main.rs:6928`, `crates/rhei-cli/src/main.rs:6976`, `crates/rhei-cli/src/main.rs:6999`.

- `missing`: top-level settings `snapshots` block is absent from `RheiSettings`. Evidence: `docs/functional-spec/rhei-agents.spec.md:226`, `crates/rhei-cli/src/main.rs:5709`, `crates/rhei-cli/src/main.rs:5737`.

## Merge Semantics

- `partial`: `defaults` shallow override is incomplete for spec-shaped nested fields because `defaults.model`, `defaults.agent`, and `defaults.program_timeout` are not fully supported. Evidence: `docs/functional-spec/rhei-agents.spec.md:247`, `crates/rhei-cli/src/main.rs:5954`, `crates/rhei-cli/src/main.rs:5957`, `crates/rhei-cli/src/main.rs:5963`.

- `partial`: `models` merge by model id is implemented as whole-profile replacement; `models.<id>.agents` does not deep-merge by agent id across global/project settings. Evidence: `docs/functional-spec/rhei-agents.spec.md:252`, `docs/functional-spec/rhei-agents.spec.md:253`, `crates/rhei-cli/src/main.rs:5949`, `crates/rhei-cli/src/main.rs:5950`.

- `partial`: explicit `null` does not reliably clear inherited optional fields because many settings fields use `Option<T>` without presence tracking, making omitted and null indistinguishable. Evidence: `docs/functional-spec/rhei-agents.spec.md:256`, `crates/rhei-cli/src/main.rs:5957`, `crates/rhei-cli/src/main.rs:5963`.

- `partial`: partial model override examples are not fully supported because model profiles are replaced wholesale and `autonomous_args` is parsed but unused. Evidence: `docs/functional-spec/rhei-agents.spec.md:264`, `crates/rhei-cli/src/main.rs:5950`, `crates/rhei-cli/src/main.rs:5765`.

## Resolution Order

- `partial`: model id resolution does not fully include spec-shaped project/global `defaults.model`; only legacy top-level merged `settings.model` is used. Evidence: `docs/functional-spec/rhei-agents.spec.md:270`, `crates/rhei-cli/src/main.rs:6378`, `crates/rhei-cli/src/main.rs:6380`, `crates/rhei-cli/src/main.rs:6382`, `crates/rhei-cli/src/main.rs:6385`.

- `missing` / disputed: resolved model id must exist in the merged `models` registry. One reviewer found unknown model ids are treated as pass-through literals and fall back to the id for `model_name`; another reviewer thought validation likely catches this but did not verify exact behavior. Evidence: `docs/functional-spec/rhei-agents.spec.md:278`, `crates/rhei-cli/src/main.rs:6388`, `crates/rhei-cli/src/main.rs:6457`, `crates/rhei-cli/src/main.rs:6458`, `crates/rhei-cli/src/main.rs:6964`; disputed possible validation evidence: `crates/rhei-cli/src/main.rs:5977`.

- `partial`: autonomous agent id resolution misses spec-shaped nested `defaults.agent`, though CLI/state/legacy settings and model default are used. Evidence: `docs/functional-spec/rhei-agents.spec.md:282`, `crates/rhei-cli/src/main.rs:6390`, `crates/rhei-cli/src/main.rs:6392`, `crates/rhei-cli/src/main.rs:6394`, `crates/rhei-cli/src/main.rs:6396`, `crates/rhei-cli/src/main.rs:6399`.

- `partial` / disputed: the no-agent-configured error exists but does not match the spec's model-specific remediation template. Evidence: `docs/functional-spec/rhei-agents.spec.md:301`, `crates/rhei-cli/src/main.rs:6404`, `crates/rhei-cli/src/main.rs:7872`, `crates/rhei-cli/src/main.rs:7877`.

- `partial`: `all_targets` bypasses normal resolution, but a target with a mode on a mode-less agent can be accepted. Evidence: `docs/functional-spec/rhei-agents.spec.md:309`, `crates/rhei-cli/src/main.rs:6483`, `crates/rhei-cli/src/main.rs:6485`, `crates/rhei-cli/src/main.rs:6300`, `crates/rhei-cli/src/main.rs:6318`.

- `partial`: validation of `target`/`all_targets` mode references only checks modes when the profile has a non-empty mode map. Evidence: `docs/functional-spec/rhei-agents.spec.md:311`, `crates/rhei-cli/src/main.rs:6005`, `crates/rhei-cli/src/main.rs:6014`, `crates/rhei-cli/src/main.rs:6021`.

- `missing`: snapshot-enabled legacy `model`/`all_models` executions do not enforce effective target tuple resolution before snapshot emit/inherit; snapshot field rejection and auto-emit skip behavior are absent. Evidence: `docs/functional-spec/rhei-agents.spec.md:315`, `crates/rhei-cli/src/main.rs:6253`, `crates/rhei-cli/src/main.rs:6472`.

- `partial`: mode resolution does not preserve "first declared registry mode" because mode maps are represented as `BTreeMap`; nested project/global default precedence is also collapsed by merge. Evidence: `docs/functional-spec/rhei-agents.spec.md:320`, `crates/rhei-cli/src/main.rs:6418`, `crates/rhei-cli/src/main.rs:6422`, `crates/rhei-cli/src/main.rs:6427`, `crates/rhei-validator/src/lib.rs:196`.

- `partial`: explicit/resolved `agent_mode` is accepted for agents with no modes, though the spec says it must not be set. Evidence: `docs/functional-spec/rhei-agents.spec.md:332`, `docs/functional-spec/rhei-agents.spec.md:475`, `crates/rhei-cli/src/main.rs:6430`, `crates/rhei-cli/src/main.rs:6431`, `crates/rhei-cli/src/main.rs:6950`.

- `missing` / disputed: tooling id with no registry match and no inline definition should be a validation error. One reviewer found the resolver records `definition: None` without emitting a pre-spawn validation error; another reviewer marked `resolve_mcp_entry` / `resolve_skill_entry` as returning an error. Evidence: `docs/functional-spec/rhei-agents.spec.md:348`, `crates/rhei-cli/src/main.rs:6221`, `crates/rhei-cli/src/main.rs:6238`, `crates/rhei-cli/src/main.rs:7704`; disputed covered evidence: `crates/rhei-cli/src/main.rs:6209`, `crates/rhei-cli/src/main.rs:6240`.

## Known And Custom Agent Profiles

- `partial`: built-in MCP and skill support flags match the table structurally, but `mcp_flag`, `mcp_config_flag`, and `skill_flag` are not appended to spawned command lines. Evidence: `docs/functional-spec/rhei-agents.spec.md:388`, `docs/functional-spec/rhei-agents.spec.md:389`, `docs/functional-spec/rhei-agents.spec.md:393`, `crates/rhei-cli/src/main.rs:5822`, `crates/rhei-cli/src/main.rs:5823`, `crates/rhei-cli/src/main.rs:5839`, `crates/rhei-cli/src/main.rs:5905`, `crates/rhei-cli/src/main.rs:6928`, `crates/rhei-cli/src/main.rs:6999`.

- `partial` / disputed: `codex` built-in `yolo` mode may be missing `-a never` from the spec table/example; another reviewer marked built-in mode flags covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:386`, `docs/functional-spec/rhei-agents.spec.md:389`, `crates/rhei-cli/src/main.rs:5832`, `crates/rhei-cli/src/main.rs:5847`.

- `missing`: built-in snapshot session support and unsupported-session behavior for built-ins are not implemented because `CustomAgentProfile.session` is absent. Evidence: `docs/functional-spec/rhei-agents.spec.md:397`, `crates/rhei-validator/src/lib.rs:165`, `crates/rhei-validator/src/lib.rs:196`.

- `missing`: agents unsupported for MCP/skills should warn at spawn time, and required entries should escalate to errors. No command-builder or run-loop path emits those warnings/errors. Evidence: `docs/functional-spec/rhei-agents.spec.md:404`, `crates/rhei-cli/src/main.rs:6928`, `crates/rhei-cli/src/main.rs:6999`.

- `missing`: when an agent declares no modes, `agent_mode` must not be set for states using that agent; implementation accepts it. Evidence: `docs/functional-spec/rhei-agents.spec.md:475`, `crates/rhei-cli/src/main.rs:5993`, `crates/rhei-cli/src/main.rs:6002`, `crates/rhei-cli/src/main.rs:6430`, `crates/rhei-cli/src/main.rs:6950`.

## Prompt Composition

- `partial` / disputed: prompt composition mostly exists, but one reviewer found `## Task Content` is omitted when content is empty and child task nodes are rendered under a separate `## Child Tasks` section rather than as part of task body. Another reviewer marked the structure covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:480`, `crates/rhei-cli/src/main.rs:6886`, `crates/rhei-cli/src/main.rs:6890`, `crates/rhei-cli/src/main.rs:6893`, `crates/rhei-cli/src/main.rs:6894`, `crates/rhei-cli/src/main.rs:6897`, `crates/rhei-cli/src/main.rs:6909`.

- `partial` / disputed: template variables are resolved before prompt delivery, but `{model.provider}` and `{model.name}` may not be fully resolved from the `models` registry; provider comes from target selectors and name may resolve to the model id. Another reviewer marked template resolution covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:517`, `crates/rhei-cli/src/main.rs:6858`, `crates/rhei-cli/src/main.rs:6865`, `crates/rhei-cli/src/main.rs:4615`, `crates/rhei-cli/src/main.rs:4617`, `crates/rhei-cli/src/main.rs:4618`.

## Completion Authority And Condition

- `missing` / disputed: `instructions` and `personality` must not describe stopping, transition commands, or completion detection. One reviewer found no validator enforces this; another treated it as non-normative stylistic guidance. Evidence: `docs/functional-spec/rhei-agents.spec.md:548`, `crates/rhei-validator/src/lib.rs:481`, `crates/rhei-validator/src/lib.rs:484`.

- `partial` / disputed: gating states bypass subprocess spawning and auto-transition, but one reviewer found the required gating log unreachable because gating tasks are filtered out of `find_ready_tasks`. Another reviewer marked the log path covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:552`, `docs/functional-spec/rhei-agents.spec.md:716`, `crates/rhei-cli/src/main.rs:9141`, `crates/rhei-cli/src/main.rs:7845`.

- `partial`: when an agent exits 0 but required outputs are missing, the task stays in state, but the spec-required warning with missing output names is not emitted. Evidence: `docs/functional-spec/rhei-agents.spec.md:566`, `docs/functional-spec/rhei-agents.spec.md:586`, `crates/rhei-cli/src/main.rs:7884`, `crates/rhei-cli/src/main.rs:8418`, `crates/rhei-cli/src/main.rs:8419`, `crates/rhei-cli/src/main.rs:8436`, `crates/rhei-cli/src/main.rs:8457`.

- `partial` / disputed: runtime semantics for non-zero exits versus timeouts are unclear. One reviewer found non-timeout non-zero exits with any configured timeout are treated as timeout candidates; another marked non-zero routing covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:586`, `crates/rhei-cli/src/main.rs:7345`, `crates/rhei-cli/src/main.rs:7354`, `crates/rhei-cli/src/main.rs:7361`, `crates/rhei-cli/src/main.rs:8476`.

- `partial` / disputed: orchestrator authority requires a finite timeout and missing timeout is specified as a validation error. One reviewer found enforcement only at runtime and skipped in dry-run; another marked validation covered through `ensure_orchestrator_timeout`. Evidence: `docs/functional-spec/rhei-agents.spec.md:606`, `crates/rhei-cli/src/main.rs:6578`, `crates/rhei-cli/src/main.rs:7912`.

## Environment Variables

- `partial`: MCP and skill environment variables are set, but availability reflects registry/inline resolution rather than real spawn-time availability. Evidence: `docs/functional-spec/rhei-agents.spec.md:633`, `crates/rhei-cli/src/main.rs:6068`, `crates/rhei-cli/src/main.rs:7097`, `crates/rhei-cli/src/main.rs:7100`, `crates/rhei-cli/src/main.rs:7106`.

## `rhei run` Agent Mode

- `partial` / disputed: sequential mode should find the next claimable task using the same eligibility as `rhei next`; one reviewer found it uses `find_ready_tasks` and misses assignee/claimability semantics. Another reviewer marked this covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:662`, `crates/rhei-cli/src/main.rs:7704`, `crates/rhei-cli/src/main.rs:7808`, `crates/rhei-cli/src/main.rs:7870`, `crates/rhei-cli/src/main.rs:8311`, `crates/rhei-cli/src/main.rs:8343`, `crates/rhei-cli/src/main.rs:8389`, `crates/rhei-cli/src/main.rs:8439`, `crates/rhei-cli/src/main.rs:8457`, `crates/rhei-cli/src/main.rs:8470`.

- `partial`: sequential spawn logging does not include counted-loop visit suffix as specified; this also appears under log capture. Evidence: `docs/functional-spec/rhei-agents.spec.md:662`, `crates/rhei-cli/src/main.rs:7115`, `crates/rhei-cli/src/main.rs:7126`, `crates/rhei-cli/src/main.rs:6595`, `crates/rhei-cli/src/main.rs:6601`.

- `partial`: parallel mode selects tasks by readiness and non-concurrent-state filtering rather than the spec's transitive `Prior` independence rule; it may also wait for all spawned threads in a batch before scheduling newly claimable work. Evidence: `docs/functional-spec/rhei-agents.spec.md:682`, `docs/functional-spec/rhei-agents.spec.md:696`, `crates/rhei-cli/src/main.rs:8213`, `crates/rhei-cli/src/main.rs:8252`, `crates/rhei-cli/src/main.rs:8506`, `crates/rhei-cli/src/main.rs:8627`.

- `missing` / disputed: the independence/file-conflict rule is not directly implemented beyond single-file fallback and non-concurrent-state filtering. Another reviewer considered directory workspace conflicts OK by construction. Evidence: `docs/functional-spec/rhei-agents.spec.md:696`, `crates/rhei-cli/src/main.rs:7692`, `crates/rhei-cli/src/main.rs:8213`.

- `missing` / disputed: when a task reaches a gating state, `rhei run` must log `Task {id} is in gating state '{state}'. Waiting for human action.` and continue; one reviewer found the log path unreachable, while another marked it covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:716`, `crates/rhei-cli/src/main.rs:9141`, `crates/rhei-cli/src/main.rs:7845`.

## Missing Tooling

- `missing`: spawn-time availability checks for command MCP servers, URL MCP servers, and skills are not implemented; availability is effectively registry resolution. Evidence: `docs/functional-spec/rhei-agents.spec.md:728`, `docs/functional-spec/rhei-agents.spec.md:739`, `crates/rhei-cli/src/main.rs:6068`, `crates/rhei-cli/src/main.rs:6081`, `crates/rhei-cli/src/main.rs:6087`.

- `missing`: required tooling failure should prevent spawn and fire `mcp_unavailable` or `skill_unavailable` transitions with `triggeredBy: system` and `transitionData.unavailable`; no such run-loop path exists. Evidence: `docs/functional-spec/rhei-agents.spec.md:754`, `crates/rhei-cli/src/main.rs:8294`, `crates/rhei-cli/src/main.rs:8343`.

- `missing`: required tooling failure without a transition should leave the task in place and log the spec error template, with `--continue-on-error` handling. Evidence: `docs/functional-spec/rhei-agents.spec.md:760`, `crates/rhei-cli/src/main.rs:8294`, `crates/rhei-cli/src/main.rs:8487`.

- `missing` / `partial`: optional unavailable tooling should warn, drop the entry before spawn, and reflect availability in prompt/env. Entries are not probed or dropped; availability variables only reflect registry/inline resolution. Evidence: `docs/functional-spec/rhei-agents.spec.md:764`, `crates/rhei-cli/src/main.rs:6068`, `crates/rhei-cli/src/main.rs:7062`, `crates/rhei-cli/src/main.rs:7097`.

- `missing`: unsupported agent tooling support should be treated like availability failure: required escalates and optional warns/drops. No unsupported-tooling gate is present, and MCP/skill flags are not emitted. Evidence: `docs/functional-spec/rhei-agents.spec.md:770`, `crates/rhei-cli/src/main.rs:6928`, `crates/rhei-cli/src/main.rs:6999`.

## Timeout Handling

- `partial`: timeout resolution precedence is mostly implemented, but nested defaults and invalid settings durations are disputed; one reviewer found invalid durations silently fall through rather than erroring. Evidence: `docs/functional-spec/rhei-agents.spec.md:824`, `crates/rhei-cli/src/main.rs:6443`, `crates/rhei-cli/src/main.rs:6452`.

- `partial` / disputed: missing orchestrator timeout should be a validation error. One reviewer found runtime-only enforcement and dry-run skip; another marked `ensure_orchestrator_timeout` covered. Evidence: `docs/functional-spec/rhei-agents.spec.md:826`, `crates/rhei-cli/src/main.rs:6578`, `crates/rhei-cli/src/main.rs:7912`.

- `partial`: timeout behavior sends SIGTERM, waits 10 seconds, then SIGKILL for the direct subprocess, but timeout is not represented distinctly from ordinary non-zero exit status after a timeout is configured. Evidence: `docs/functional-spec/rhei-agents.spec.md:846`, `crates/rhei-cli/src/main.rs:7352`, `crates/rhei-cli/src/main.rs:7354`, `crates/rhei-cli/src/main.rs:7357`, `crates/rhei-cli/src/main.rs:7361`, `crates/rhei-cli/src/main.rs:8363`.

- `missing` / disputed: task log must include `agent timed out after {duration}`. One reviewer found no timeout-specific log line, while another noted timeout is recorded in footer semantics but did not find the exact text. Evidence: `docs/functional-spec/rhei-agents.spec.md:850`, `crates/rhei-cli/src/main.rs:7384`.

- `partial` / disputed: timeout transitions exist, but one reviewer found they are invoked for any non-success status with a configured timeout, ignore transition conditions, and run callbacks as `triggeredBy: user` without timeout data. Another reviewer marked timeout transition behavior mostly covered except for unverified `transitionData.timeout`. Evidence: `docs/functional-spec/rhei-agents.spec.md:851`, `docs/functional-spec/rhei-agents.spec.md:929`, `crates/rhei-cli/src/main.rs:8476`, `crates/rhei-cli/src/main.rs:9028`, `crates/rhei-cli/src/main.rs:9038`, `crates/rhei-cli/src/main.rs:5215`, `crates/rhei-cli/src/main.rs:5226`, `crates/rhei-cli/src/main.rs:5370`.

- `missing` / disputed: if no timeout transition exists, the task should remain in state and the engine should log a warning. One reviewer found `fire_timeout_transition` silently does nothing when no rule exists; another marked behavior covered via generic non-zero logging. Evidence: `docs/functional-spec/rhei-agents.spec.md:853`, `crates/rhei-cli/src/main.rs:9038`, `crates/rhei-cli/src/main.rs:9076`.

- `missing`: snapshot timeout transcripts classified as `completion: timeout` and not preloadable are not implemented in this surface. Evidence: `docs/functional-spec/rhei-agents.spec.md:855`, `crates/rhei-cli/src/main.rs:7223`, `crates/rhei-cli/src/main.rs:7384`.

## Log Capture

- `partial`: log naming covers simple and model-specific states, but counted-loop visit suffix and combined model-plus-visit suffix are not implemented. Evidence: `docs/functional-spec/rhei-agents.spec.md:935`, `crates/rhei-cli/src/main.rs:7114`, `crates/rhei-cli/src/main.rs:6595`, `crates/rhei-cli/src/main.rs:8312`.

- `partial`: `mcp_servers:` and `skills:` log entries can format `?`, but real availability failure is not implemented, so `?` only reflects unresolved definitions rather than spawn-time failed availability. Evidence: `docs/functional-spec/rhei-agents.spec.md:974`, `crates/rhei-cli/src/main.rs:7062`, `crates/rhei-cli/src/main.rs:7077`, `crates/rhei-cli/src/main.rs:7281`.

## Dry Run And Callback-Only Mode

- `partial`: `rhei run --dry-run` does not render the actual command line, concrete provider model name, provider/model tuple, or human-formatted timeout as shown by the spec. Evidence: `docs/functional-spec/rhei-agents.spec.md:983`, `crates/rhei-cli/src/main.rs:8257`, `crates/rhei-cli/src/main.rs:8274`, `crates/rhei-cli/src/main.rs:8278`, `crates/rhei-cli/src/main.rs:8735`.

- `partial` / disputed: dry-run output wording diverges from the spec example, including the per-pass header and final message. Evidence: `docs/functional-spec/rhei-agents.spec.md:983`, `crates/rhei-cli/src/main.rs:8257`, `crates/rhei-cli/src/main.rs:8283`.
