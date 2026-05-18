# Quality Review pass 1 - target codex[xhigh]:openai:gpt-5.5

- Q-settings-parse-swallowed: Invalid settings files are silently replaced with defaults
  - Severity: high
  - File: crates/rhei-cli/src/main.rs
  - Detail: `load_settings` returns `RheiSettings::default()` for any JSON deserialization failure, not just for a missing file. That means malformed project/global settings, inline `defaults.agent` objects, wrong field types, or other schema violations are ignored and the run proceeds with built-ins or partial defaults. This breaks the settings contract and can silently spawn the wrong agent/model instead of failing configuration validation.
  - Evidence: The spec says global and project settings use the declared schema and project settings compose by key (`docs/functional-spec/rhei-agents.spec.md:35-39`), and inline agent definitions are not permitted on states or `defaults.agent` (`docs/functional-spec/rhei-agents.spec.md:151-154`). The loader discards `serde_json::from_str` errors with `unwrap_or_default()` (`crates/rhei-cli/src/main.rs:6071-6074`).
  - Suggested fix: Make settings loading return `MietteResult<RheiSettings>`; treat `NotFound` as defaults but surface parse/schema errors with the offending path, and plumb that result through `load_merged_settings` callers.

- Q-null-does-not-clear: `null` cannot clear inherited settings defaults
  - Severity: medium
  - File: crates/rhei-cli/src/main.rs
  - Detail: The merge layer uses plain `Option<T>` and `project.defaults.<field>.or(global.defaults.<field>)`, so an explicit project `null` is indistinguishable from an omitted field and inherits the global value. This violates the documented clearing semantics and can force inherited model, agent, mode, or timeout settings onto a project that explicitly disabled them.
  - Evidence: The spec requires "`null` explicitly clears an inherited optional field" (`docs/functional-spec/rhei-agents.spec.md:244-256`). The merge code inherits global values whenever the project field deserializes to `None` (`crates/rhei-cli/src/main.rs:6133-6140`), and model profile merging has the same presence-loss pattern for optional profile fields (`crates/rhei-cli/src/main.rs:6111-6119`).
  - Suggested fix: Track field presence during deserialization for settings and model profiles, or merge from raw `serde_json::Value`, so omitted means inherit and explicit `null` means clear.

- Q-model-registry-required: Resolved model ids are allowed without a `models` registry entry
  - Severity: medium
  - File: crates/rhei-cli/src/main.rs
  - Detail: Agent resolution treats `settings.models.get(id)` as optional and falls back to passing the unresolved model id as the concrete model name. The spec requires any resolved model id to exist in the merged `models` registry; allowing a pass-through id loses provider/model validation and can spawn agents with a semantic profile id like `impl-fast` instead of the concrete provider model.
  - Evidence: The spec states "The resolved model id must exist in the merged `models` registry" (`docs/functional-spec/rhei-agents.spec.md:270-280`). `resolve_legacy_agent_with_model` computes `model_profile` with `and_then`, never errors when it is `None`, and falls back to `model.clone()` for `model_name` (`crates/rhei-cli/src/main.rs:6687-6699`, `crates/rhei-cli/src/main.rs:6777-6778`).
  - Suggested fix: After resolving a model id, require `settings.models.contains_key(id)` unless no model is configured at any level; report a configuration error before resolving/spawning the agent.

- Q-required-tooling-still-spawns: Required missing or unsupported tooling does not block agent spawn
  - Severity: high
  - File: crates/rhei-cli/src/main.rs
  - Detail: Required skills with missing paths are dropped to `definition = None`, and unsupported MCP/skill wiring only emits warnings. There is no run-loop gate that prevents spawn, fires `mcp_unavailable`/`skill_unavailable`, or logs the required hard error for non-optional entries. Agents can therefore run without tooling that the state declared as required, which is a correctness and safety issue for review/deploy states that rely on that context.
  - Evidence: The spec says required unavailable tooling must prevent spawn and either fire an unavailable transition with `triggeredBy: 'system'` or leave the task in place with an error (`docs/functional-spec/rhei-agents.spec.md:754-763`), and unsupported required tooling is treated the same way (`docs/functional-spec/rhei-agents.spec.md:770-774`). Current skill resolution only warns and drops missing paths (`crates/rhei-cli/src/main.rs:6496-6516`); unsupported tooling is explicitly "not driven from here" and only produces warnings (`crates/rhei-cli/src/main.rs:7548-7553`, `crates/rhei-cli/src/main.rs:7907-7912`).
  - Suggested fix: Add a pre-spawn availability phase that partitions required vs optional MCP/skill failures, drops only optional failures, and routes required failures through the `*_unavailable` transition/error path before `spawn_and_wait_agent`.

- Q-mcp-flag-value-not-launch-spec: Per-server `mcp_flag` receives only an id, not a launch spec
  - Severity: medium
  - File: crates/rhei-cli/src/main.rs
  - Detail: For agents using `mcp_flag`, the command builder appends `--mcp <id>` for each resolved MCP server. The spec requires the flag value to be a launch spec for that server. Passing only the id means an agent such as `codex` cannot know the command/url/env/working-directory needed to attach the MCP server.
  - Evidence: The spec defines `mcp_flag` as "one MCP server per occurrence" with "a launch spec as its value" (`docs/functional-spec/rhei-agents.spec.md:163`). The builder currently does `cmd.arg(flag).arg(&entry.id)` whenever an entry has a definition (`crates/rhei-cli/src/main.rs:7388-7392`).
  - Suggested fix: Serialize each resolved `McpServerProfile` into the agent's expected launch-spec string for `mcp_flag`, including expanded env and working directory, instead of passing the registry id.

- Q-run-ignores-claim-ownership: `rhei run` schedules already-assigned tasks
  - Severity: high
  - File: crates/rhei-cli/src/main.rs
  - Detail: The agent run loop uses `find_ready_tasks`, which filters only terminal/gating states and prior dependencies. It does not check `**Assignee:**`, even though claimability excludes already-assigned tasks. A background `rhei run` can therefore spawn an autonomous agent on work that a human or another agent has already claimed, causing conflicting edits and invalidating the "same eligibility as `rhei next`" contract.
  - Evidence: The spec requires sequential and parallel run modes to use claimable-task eligibility matching `rhei next` (`docs/functional-spec/rhei-agents.spec.md:664-680`, `docs/functional-spec/rhei-agents.spec.md:682-686`). `run_agent_mode` pulls candidates from `find_ready_tasks` (`crates/rhei-cli/src/main.rs:8442-8445`), while the claimable helper explicitly filters `task.assignee.is_none()` (`crates/rhei-cli/src/main.rs:10082-10099`); `find_ready_tasks` itself never checks assignee (`crates/rhei-cli/src/main.rs:10043-10079`).
  - Suggested fix: Use a run-specific claimability predicate that includes the assignee exclusion while preserving the intended autonomous-state progression semantics, and add a regression test with an assigned ready task.

- Q-parallel-timeout-data-lost: Parallel timeout transitions lose the resolved timeout duration
  - Severity: medium
  - File: crates/rhei-cli/src/main.rs
  - Detail: In the parallel result path, the resolved agent is moved into the worker thread and the timeout handler calls `fire_timeout_transition` with `None`. `fire_timeout_transition` then falls back to the transition rule's `timeout` literal for `transitionData.timeout`. If the resolved agent timeout came from the state, model-agent binding, agent profile, or settings default and differs from the transition's marker, callbacks receive the wrong timeout duration.
  - Evidence: The spec requires timeout callbacks to receive the timeout duration in `transitionData.timeout` (`docs/functional-spec/rhei-agents.spec.md:920-929`). The parallel path passes `None` (`crates/rhei-cli/src/main.rs:9450-9470`), and the handler substitutes `rule.timeout` when `timeout_secs` is absent (`crates/rhei-cli/src/main.rs:9968-9971`).
  - Suggested fix: Include `resolved.timeout_secs` in the thread result tuple or in `AgentSpawnOutcome`, and pass it to `fire_timeout_transition` in both sequential and parallel paths.

- Q-parallel-waits-for-batch: Parallel mode waits for all spawned agents before processing any completion
  - Severity: medium
  - File: crates/rhei-cli/src/main.rs
  - Detail: The spec says parallel mode waits for any agent to exit, processes that result, then scans for newly claimable tasks and refills the pool. The implementation spawns a fixed batch, then joins handles in vector order. If the first handle is long-running and another finishes quickly, the finished task's transition and callbacks are delayed until the earlier handle completes, and newly unblocked tasks are not scheduled until the whole batch is done.
  - Evidence: The parallel execution loop requires "Wait for any agent to exit" and refill below capacity after each completion (`docs/functional-spec/rhei-agents.spec.md:682-694`). Current code stores all thread handles and then processes them via blocking `handle.join()` in insertion order (`crates/rhei-cli/src/main.rs:9310-9366`).
  - Suggested fix: Use a completion channel from worker threads to the orchestrator, process results as they arrive, and schedule newly claimable independent tasks whenever the active count drops below the configured parallel limit.
