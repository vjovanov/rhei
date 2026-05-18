# Quality Fix Plan pass 1

## Accepted

- Q-settings-parse-swallowed: Invalid settings files are silently replaced with defaults.
  - Files: crates/rhei-cli/src/main.rs
  - Approach: Change `load_settings` and `load_merged_settings` to return `MietteResult<RheiSettings>`. Treat `ErrorKind::NotFound` as defaults, but surface read and `serde_json` parse/schema failures with the offending settings path. Plumb `?` through all call sites that already return `MietteResult`; for completion helpers that cannot fail, degrade explicitly to defaults with a narrow comment instead of silently hiding run-time configuration errors.
  - Tests/checks: Add focused unit coverage in `crates/rhei-cli/src/main.rs` for malformed project settings failing and missing settings still defaulting. Run `cargo test -p rhei-cli --bin rhei settings`.

- Q-required-tooling-still-spawns: Required missing or unsupported tooling does not block agent spawn.
  - Files: crates/rhei-cli/src/main.rs
  - Approach: Add a pre-spawn tooling availability gate after `resolve_tooling` and before prompt composition/spawn. Use the currently available signals only: unresolved registry/inline entries, missing skill paths already collapsed to `definition = None`, and agents lacking `mcp_flag`/`mcp_config_flag` or `skill_flag`. Optional failures should warn and be dropped; required failures should prevent spawn and either fire the matching `mcp_unavailable`/`skill_unavailable` system transition with `transitionData.unavailable` and `transitionData.kind`, or log the required-tooling error and honor `--continue-on-error`. Do not add real MCP handshake/network probing in this pass.
  - Tests/checks: Add unit tests for required vs optional failure classification and transition matching, plus a small `rhei run` regression with a fake agent proving a required missing skill does not spawn. Run `cargo test -p rhei-cli --bin rhei tooling`.

- Q-run-ignores-claim-ownership: `rhei run` schedules already-assigned tasks.
  - Files: crates/rhei-cli/src/main.rs
  - Approach: Use a run-specific readiness helper for agent/program/callback scheduling that preserves the existing non-terminal, non-gating, prior-satisfied behavior but excludes tasks with `assignee.is_some()`. Keep `rhei next` claimability behavior separate so this does not accidentally change manual claim semantics.
  - Tests/checks: Add a regression around the readiness helper or `rhei run --dry-run` with one ready assigned task and one ready unassigned task. Run `cargo test -p rhei-cli --bin rhei assigned`.

- Q-null-does-not-clear: `null` cannot clear inherited settings defaults.
  - Files: crates/rhei-cli/src/main.rs
  - Approach: Preserve field presence during settings composition, preferably by loading settings as `serde_json::Value` alongside typed validation and applying project-over-global merges from the raw object. Omitted fields inherit; explicit `null` clears optional fields. Cover at least `defaults.model`, `defaults.agent`, `defaults.agent_mode`, top-level legacy optional defaults, `models.<id>.default_agent`, and optional model-agent binding fields such as `timeout`.
  - Tests/checks: Add focused merge tests for project `null` clearing inherited defaults and a model-agent timeout. Run `cargo test -p rhei-cli --bin rhei merge`.

- Q-model-registry-required: Resolved model ids are allowed without a `models` registry entry.
  - Files: crates/rhei-cli/src/main.rs
  - Approach: In legacy model resolution (`model`, `all_models`, defaults, and CLI `--model`), once a model id resolves to `Some(id)`, require `settings.models.contains_key(id)` before deriving agent defaults or model flag values. Keep `target`/`all_targets` behavior separate because target selectors carry provider/model directly and bypass normal model-id resolution.
  - Tests/checks: Add a unit test that `resolve_legacy_agent_with_model` errors for an unknown configured model id and still returns no model context when no model is configured. Run `cargo test -p rhei-cli --bin rhei model_registry`.

- Q-parallel-timeout-data-lost: Parallel timeout transitions lose the resolved timeout duration.
  - Files: crates/rhei-cli/src/main.rs
  - Approach: Carry `resolved.timeout_secs` out of each parallel worker, either in the thread result tuple or in `AgentSpawnOutcome`, and pass it to `fire_timeout_transition` on the parallel timeout path exactly as sequential mode already does.
  - Tests/checks: Add a parallel timeout regression that uses a timeout transition callback/log assertion to verify the resolved state/agent timeout, not only the transition rule literal, reaches `transitionData.timeout`. Run `cargo test -p rhei-cli --bin rhei timeout`.

## Rejected / Deferred

- Q-mcp-flag-value-not-launch-spec: defer
  - Reason: The spec requires a per-server launch spec for `mcp_flag`, but the exact string format is not defined here and is agent-specific. A fix now would risk baking in the wrong transport contract; defer until the spec or agent profile schema defines the launch-spec encoding.

- Q-parallel-waits-for-batch: defer
  - Reason: Processing parallel completions as they arrive requires replacing the current fixed-batch/vector-join loop with a completion-channel scheduler that refills capacity after each result and re-evaluates dependencies. That is a real scheduler rewrite, not a bounded quality-fix item for this pass.
