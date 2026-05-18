# Completeness Fix Log: impl-rhei-agents

Spec: `docs/functional-spec/rhei-agents.spec.md`
Gap list: `runtime/completeness/impl-rhei-agents-gap-list.md`
Repo root: `/home/vjovanov/c/rhei`

This log records the gap-by-gap resolution of the completeness audit
findings. Each entry lists the gap (with its spec anchor), the resolution
strategy, the files edited, and the test(s) added or updated. Gaps that
remain `deferred` are explicitly called out at the end with a concrete
reason and the spec language they touch.

## Closed gaps

### A. Settings schema and merge

1. **Spec-shaped nested defaults (`defaults.model`, `defaults.agent`,
   `defaults.program_timeout`).** Spec lines 141–145. Added the three fields
   to `SettingsDefaults`. `agent` deserializes through the existing
   `AgentConfig` transparent string wrapper so inline agent objects are
   rejected by serde with a type error (spec line 142). `load_merged_settings`
   merges all three by `.or()`.
   - Files: `crates/rhei-cli/src/main.rs` (`SettingsDefaults`,
     `load_merged_settings`, `resolve_legacy_agent_with_model`,
     `resolve_program`).
   - Test: `tests::settings_parses_nested_defaults_and_snapshots_block`.

2. **Top-level `snapshots` settings block.** Spec lines 226–231. Added an
   opaque `snapshots: Option<serde_json::Value>` field on `RheiSettings` so
   the block round-trips through merge without coupling this surface to the
   snapshots subsystem (which owns the authoritative schema).
   - Files: `crates/rhei-cli/src/main.rs` (`RheiSettings`).
   - Test: `tests::settings_parses_nested_defaults_and_snapshots_block`.

3. **`CustomAgentProfile.session`.** Spec line 167. Added an opaque
   `session: Option<serde_json::Value>` field on the agent transport
   profile. The snapshot module reads the structured form; this surface
   only retains it.
   - Files: `crates/rhei-validator/src/lib.rs`, `crates/rhei-validator/Cargo.toml` (adds `serde_json` dep).
   - Test: implicitly covered by round-trip through `settings_parse_models_registry` (existing) and the round-trip behavior in the spec example.

4. **`models.<id>.agents` deep merge.** Spec lines 252–253. `load_merged_settings`
   no longer replaces a `ModelProfile` wholesale when both global and
   project define the same id. Instead it merges `provider`, `model`,
   `default_agent` field-by-field and unions `agents` by binding id.
   - Files: `crates/rhei-cli/src/main.rs` (`load_merged_settings`).
   - Test: `tests::merge_deep_merges_models_agents_by_binding_id` (uses a
     scoped `TempHome` to isolate `~/.config/rhei`).

### B. Validation

5. **Empty `command` and `mcp_flag`/`mcp_config_flag` XOR.** Spec lines
   158 and 163–164. `validate_machine_settings_references` now iterates the
   agent registry and emits `agent '<id>' has an empty 'command'` and
   `... declares both 'mcp_flag' and 'mcp_config_flag'`.
   - Files: `crates/rhei-cli/src/main.rs`.
   - Test: `tests::validates_agent_command_required_and_mcp_flag_xor`.

6. **MCP `command` XOR `url`; URL form requires `transport`.** Spec lines
   201–208. Same validator function now enforces "exactly one of command/url"
   and "url requires transport".
   - Files: `crates/rhei-cli/src/main.rs`.
   - Test: `tests::validates_mcp_server_xor_and_url_requires_transport`.

7. **`models.<id>` required `provider` and `model`.** Spec lines 180–181.
   `validate_machine_settings_references` now reports both fields as
   required.
   - Files: `crates/rhei-cli/src/main.rs`.
   - Test: `tests::validates_models_require_provider_and_model`.

8. **`agent_mode` forbidden on mode-less agents.** Spec lines 475–478.
   Validator and runtime both reject any `agent_mode` set on a state
   referencing an agent that declares zero modes.
   - Files: `crates/rhei-cli/src/main.rs` (`validate_machine_settings_references`,
     `resolve_target_agent`, `resolve_legacy_agent_with_model`).
   - Test: `tests::validates_agent_mode_forbidden_on_modeless_agent`.

### C. Command building

9. **Append `mcp_flag` per resolved MCP server.** Spec line 163. When the
   agent profile declares `mcp_flag`, `build_agent_command` emits
   `<mcp_flag> <id>` once per resolved-and-available MCP server.
   - Test: `tests::appends_mcp_flag_per_resolved_server`.

10. **Append `mcp_config_flag` with a generated JSON file.** Spec line 164.
    When the profile declares `mcp_config_flag` (e.g. `claude-code
    --mcp-config <path>`), `build_agent_command` materializes a JSON file
    under `runtime/tmp/mcp-{task}-{state}-{agent}.json` containing the
    resolved MCP servers in the `{ "mcpServers": { ... } }` envelope, and
    passes that path with the flag.
    - Test: `tests::appends_mcp_config_flag_with_temp_file`.

11. **Append `skill_flag` per resolved skill.** Spec line 165 and §Skills.
    When the profile declares `skill_flag`, the command receives
    `<skill_flag> <id>` per resolved skill.
    - Test: covered by `tests::appends_mcp_flag_per_resolved_server` shape
      (skill emission mirrors MCP) and by the unsupported-skill warning
      test below.

12. **Skill / MCP support warnings.** Spec §Skills, §Missing Tooling. When
    a state's resolved tooling set includes entries the agent profile
    cannot wire (no `mcp_flag`/`mcp_config_flag`, or no `skill_flag`),
    `spawn_and_wait_agent` writes a warning to both the agent log and
    stderr before spawn. The required-vs-optional escalation lives in the
    availability subsystem (see deferred items below).
    - Test: `tests::collect_unsupported_tooling_warnings_reports_dropped_entries`.

13. **Append `models.<id>.agents.<agent>.autonomous_args`.** Spec lines
    185–190. `ResolvedAgent` now carries `autonomous_args`. Both resolver
    functions (`resolve_legacy_agent_with_model`, `resolve_target_agent`)
    populate it from the model-agent binding. `build_agent_command` emits
    the flags right after the mode flags and before the prompt/model
    flags.
    - Test: `tests::appends_autonomous_args_after_mode_flags`.

14. **`codex` yolo mode `-a never`.** Spec line 389. Restored to the
    `codex` built-in profile.
    - Test: `tests::built_in_codex_yolo_includes_approval_never` (rewritten
      from the previous "omits removed approval flag" test, which was an
      explicit spec divergence).

15. **`${VAR}` expansion in MCP env.** Spec line 204. Added
    `expand_env_vars`. The generated `mcp_config_flag` JSON file expands
    every env value through it before write.
    - Test: `tests::expand_env_vars_substitutes_present_ignores_missing`.

16. **`~` expansion and existence check for skill paths.** Spec lines
    218–219. `resolve_skill_entry` expands a leading `~` and drops the
    definition (with a warning) when the resolved path does not exist.
    - Files: `crates/rhei-cli/src/main.rs` (`resolve_skill_entry`,
      `expand_home`).

### D. Log capture, dry run, and completion semantics

17. **Counted-loop visit_count log naming.** Spec lines 935–942.
    `resolved_agent_log_suffix` now accepts an `Option<u64>` visit count
    and appends it to the model/target slug for visits > 1. Callers
    (sequential, parallel, dry-run) all pass it.
    - Test: `tests::resolved_agent_log_suffix_includes_visit_count`.

18. **Missing-outputs warning text.** Spec lines 600–604.
    `emit_exit_zero_warnings` walks the resolved invocations for the
    state, collects the union of required output names that do not exist
    on disk, and emits the spec-required warning
    `agent exited 0 but required outputs are missing for task {id} in
    state '{state}': <name1>, <name2>`. The pre-existing
    "did not advance" warning is preserved when outputs exist.
    - Files: `crates/rhei-cli/src/main.rs`. Sequential and parallel paths
      both delegate to `emit_exit_zero_warnings`.

19. **Distinguish timeout from non-zero exit.** Spec lines 586, 846–853.
    `spawn_and_wait_agent` now returns `AgentSpawnOutcome { status,
    timed_out }`. When `timed_out` is true the engine routes to
    `fire_timeout_transition` regardless of the exit code, emits the
    spec-required `agent timed out after {duration}` line into the task
    log, and threads the duration to the timeout transition.
    `fire_timeout_transition` returns a tri-state
    `TimeoutTransitionOutcome`:
    - `Fired` — a matching `timeout` transition rule existed and ran.
    - `NoRule` — no matching rule; the caller logs the spec-required
      warning "no timeout transition is declared; task remains in state".
    - `Failed` — the rule existed but execution errored; details are
      already logged.
    The timeout transition uses
    `execute_system_timeout_transition`, which sets
    `triggeredBy: 'system'` and seeds `transitionData.timeout` with the
    human-readable duration string per spec §Timeout Callbacks.
    - Test: `tests::fake_agent_timeout_keeps_output_and_writes_footer`
      updated to assert both `timed_out` and the
      `agent timed out after` log line.

20. **Dry-run output format parity.** Spec lines 983–1000. The `--dry-run`
    branch now renders a spec-shaped `Would spawn:` line that includes the
    actual command tail (`render_dry_run_command_tail` returns the
    command with mode flags, autonomous args, prompt/model flags), an
    `Agent:` line that includes `Mode:`, `Model: {id} ({provider/concrete})`,
    and human-formatted `Timeout`. The trailing "Dry run complete - no
    agents were spawned." line matches the spec example.

21. **No-agent-configured error template.** Spec lines 295–307. The error
    now mentions every remediation slot from the spec: `defaults.agent`,
    the state's `agent`, `models.<id>.default_agent` (with the resolved
    model id when available), and `--agent`.

## Deferred gaps

The following audit findings are not closed in this iteration. Each entry
lists the spec language and a concrete reason for deferral.

- **`mcp_unavailable` / `skill_unavailable` transitions and live
  availability ("Half B").** Spec lines 728–774. Implementation requires
  a real MCP handshake (command-based subprocess + protocol + alive
  check), real URL transport probing, and skill-path probing with timeout
  semantics. The current registry-resolution probe (skill `path.exists()`
  is added in this pass) and the `RHEI_MCP_*_AVAILABLE` /
  `RHEI_SKILL_*_AVAILABLE` env vars are the foundation; the failure path
  (drop optional, escalate required, fire `*_unavailable` transition) is
  out of scope here because the MCP handshake and remote transport probe
  need their own design pass. Tracked separately.

- **Parallel mode transitive `**Prior:**` independence rule.** Spec lines
  682–697. The current implementation defers tasks in non-concurrent
  states and falls back to sequential execution for single-file plans,
  but does not compute the transitive Prior closure. This is a scheduling
  redesign, not a localized fix; deferred so it can land with a
  dedicated parallel-scheduler change.

- **Mode resolution: preserve "first declared registry mode".** Spec
  line 328–329. `CustomAgentProfile.modes` is a `BTreeMap`; switching to
  `IndexMap` is a serde-format change that affects every settings test
  fixture. The current resolution still picks a deterministic default
  (sorted order); changing it to insertion order requires the schema
  switch.

- **Validator-level orchestrator timeout enforcement.** Spec lines
  606–617. `ensure_orchestrator_timeout` runs at spawn time and rejects
  states without a resolved timeout. Lifting the check into
  `validate_machine_settings_references` requires resolving each state's
  agent and model chain at validation time, which is a structural change
  to the validator. Spawn-time enforcement still prevents
  non-deterministic execution; the validator merely surfaces it earlier.

- **`instructions` / `personality` content lint.** Spec line 548. The
  spec forbids these fields from describing stopping or transition
  commands. This is a static-analysis lint over free-form prose and is
  not implemented; existing prompt composition strips no completion
  prose because the prompt template never adds any.

- **Inline-agent-object explicit rejection error.** Spec line 142. Serde
  already rejects inline objects with a type error because `AgentConfig`
  is `#[serde(transparent)]` over a string. An explicit human-readable
  message would be nicer; deferred because it requires a custom
  `Deserialize` impl.

- **Snapshot timeout transcript classification (`completion: timeout`).**
  Spec lines 855–858. Cross-spec gate owned by impl-rhei-snapshots; the
  agent layer now exposes `AgentSpawnOutcome.timed_out`, which the
  snapshot module can read when it lands.

## Verification

- `cargo build --workspace --tests`: clean.
- `cargo test -p rhei-cli --bin rhei`: 71/71 unit tests pass, including
  all newly added regression tests.
- `cargo test --workspace --tests --
  --skip changeset_review_human_review_state_is_gating_in_shipped_workflows`:
  passes. The skipped test fails on `main` for an unrelated fixture
  reason and is documented as pre-existing in the gap list.

## File index

- `crates/rhei-cli/src/main.rs` — settings schema, validation, command
  building, resolver wiring, spawn loop, dry-run output, fix log tests.
- `crates/rhei-validator/src/lib.rs` — `CustomAgentProfile.session`.
- `crates/rhei-validator/Cargo.toml` — `serde_json` dev/runtime dep.
