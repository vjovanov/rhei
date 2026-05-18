# Quality Fix Plan pass 2

## Accepted

- Q-defaults-only-agent-mode-skipped: `rhei run` must enter autonomous agent mode when an effective agent/model resolves from CLI flags, settings defaults, or model defaults, not only when a state declares agent fields.
  - Files: `crates/rhei-cli/src/main.rs`
  - Approach: Replace the mode-selection predicate with one based on the effective per-state invocation resolution used by agent spawning. When `--no-agent` is false and a runnable non-terminal/non-gating state can resolve an agent invocation from CLI/settings/model defaults, route to `run_agent_mode`; keep explicit callback mode only when no autonomous invocation can resolve.
  - Tests/checks: Add focused CLI/run-loop unit coverage in `crates/rhei-cli/src/main.rs` for `--agent codex`, `defaults.agent`, `defaults.model`, and `models.<id>.default_agent` causing agent mode selection; run `cargo test -p rhei-cli defaults_only_agent_mode` and `cargo test -p rhei-cli --lib`.

- Q-missing-outputs-respawn-loop: An exit-0 agent that does not produce required outputs must warn and stop the attempt instead of being immediately rescheduled.
  - Files: `crates/rhei-cli/src/main.rs`
  - Approach: In both sequential and parallel result handling, check the completed invocation's required outputs before calling the fanout/pending-invocation continuation path. If the just-finished invocation is missing required outputs, emit the specified warning, leave the task in its current state, mark no auto-advance for that attempt, and avoid respawning the same invocation in the same run loop.
  - Tests/checks: Add run-loop tests covering a successful fake agent with missing required outputs in single-invocation and fanout paths; assert one spawn, warning text, and unchanged state. Run `cargo test -p rhei-cli missing_outputs_reschedule`.

- Q-optional-tooling-disappears: Optional unavailable tooling must remain visible to prompt templates, environment variables, and logs while still being omitted from spawn attachment flags.
  - Files: `crates/rhei-cli/src/main.rs`
  - Approach: Preserve optional unavailable MCP/skill entries in the resolved tooling structure with explicit availability/status metadata. Filter only when constructing concrete command-line attachment flags or generated MCP config; render `{mcp.<id>.available}`, `{skill.<id>.available}`, `RHEI_MCP_<ID>_AVAILABLE=false`, `RHEI_SKILL_<ID>_AVAILABLE=false`, and log `?` entries from the preserved metadata.
  - Tests/checks: Add unit tests for prompt context rendering, agent environment, and log header output for optional unavailable MCP and skill entries. Run `cargo test -p rhei-cli optional_tooling_availability`.

- Q-target-selector-overridden-by-registry: `target` and `all_targets` selectors must use their literal provider/model values even when a model registry key has the same name.
  - Files: `crates/rhei-cli/src/main.rs`
  - Approach: In `resolve_target_agent`, keep the selector's provider and model as the effective provider/model name for prompt, env, slug, and command construction. Continue consulting a same-named model profile only for compatible per-agent binding metadata such as timeout and autonomous args.
  - Tests/checks: Add resolver tests where `target.model` collides with a `models` registry entry whose concrete `model` differs; assert the selector model is passed through. Run `cargo test -p rhei-cli target_selector_literal_model`.

- Q-mode-default-order-lost: Custom agent mode defaulting must use declaration order, not lexicographic key order.
  - Files: `crates/rhei-validator/src/lib.rs`, `crates/rhei-cli/src/main.rs`
  - Approach: Change `CustomAgentProfile.modes` to an insertion-order map type already available in the validator crate, and update built-in/test construction accordingly. Keep explicit `agent_mode` precedence unchanged; only the fallback `first declared mode` selection should change.
  - Tests/checks: Add a settings deserialization/resolution test with modes declared as `yolo` then `safe`, asserting fallback mode is `yolo`. Run `cargo test -p rhei-cli mode_default_order` and `cargo test -p rhei-validator custom_agent_profile`.

- Q-tooling-unknown-id-not-validation: Unknown id-only MCP/skill references must fail validation/settings load instead of becoming spawn-time availability failures.
  - Files: `crates/rhei-cli/src/main.rs`
  - Approach: Extend merged-settings validation to check every id-only MCP and skill entry from defaults and state definitions against the merged registries. Treat missing ids as hard configuration errors; reserve spawn-time unavailability for known registry or inline entries whose concrete launch/path checks fail.
  - Tests/checks: Replace the existing unknown-id-as-unavailable test with validation failure coverage for defaults and state-level MCP/skill references, while retaining optional known-but-unavailable behavior. Run `cargo test -p rhei-cli unknown_tooling_id_validation`.

## Rejected / Deferred

- Q-tooling-availability-not-checked: defer
  - Reason: The requested fix requires implementing real MCP command/URL startup, handshake probing, timeout handling, and process lifecycle cleanup before agent spawn. That is a broad transport subsystem change and should not be bundled into this bounded quality-fix pass.
