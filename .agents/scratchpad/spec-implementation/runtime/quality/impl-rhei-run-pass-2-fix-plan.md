# Quality Fix Plan pass 2

## Accepted

- Q-agent-no-outputs-skips-spawn: Agent states with no declared `outputs:` are incorrectly skipped as already complete.
  - Files: `crates/rhei-cli/src/main_parts/run_agent_mode.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_2.rs`
  - Approach: Treat resolved agent invocations for states with no output artifacts as pending work that must be spawned. Only use pre-existing output artifacts to skip a resolved invocation when the state declares one or more required outputs for that invocation/resume case.
  - Tests/checks: Add a run-mode integration test with an agent state that declares no `outputs:` and records that the agent command was invoked, then run `cargo test -p rhei-cli --test integration_markdown_plans`.

- Q-model-only-missing-agent-falls-back: Model-driven ready states without a resolved agent can fall back to callback-only mode instead of failing configuration validation.
  - Files: `crates/rhei-cli/src/main_parts/run_command.rs`, `crates/rhei-cli/src/main_parts/agent_resolution.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_2.rs`
  - Approach: During run mode selection, distinguish states that declare autonomous model/target work from states with no agent work. If a reachable ready state declares `model`, `all_models`, target fanout, or equivalent autonomous agent routing but no agent transport resolves, surface the existing missing-agent configuration error instead of selecting callback mode.
  - Tests/checks: Add a run integration test for a ready state with `model:` and no resolvable state/default/model `default_agent`, asserting that `rhei run` fails with a missing-agent error and does not apply callback-only transitions. Run `cargo test -p rhei-cli --test integration_markdown_plans`.

- Q-program-zero-missing-outputs-aborts: Successful program exits with missing required outputs abort the run instead of leaving the task in place.
  - Files: `crates/rhei-cli/src/main_parts/run_agent_mode.rs`, `crates/rhei-cli/src/main_parts/system_transitions_1.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_2.rs`
  - Approach: Before applying a zero-exit program transition, explicitly verify required source-state outputs. If any are missing, warn and leave the task in its current state without calling transition execution. Preserve existing skip-output behavior for non-zero failure routes.
  - Tests/checks: Add a program-state integration test where exit code `0` matches a transition but required `outputs:` are absent; assert the run succeeds without advancing the task and without aborting. Run `cargo test -p rhei-cli --test integration_markdown_plans`.

- Q-fanout-split-by-parallel-limit: `all_models` / `all_targets` invocations for one scheduled task are split by `--parallel`.
  - Files: `crates/rhei-cli/src/main_parts/run_agent_mode.rs`, `crates/rhei-cli/tests/integration_markdown_plans/run_2.rs`
  - Approach: Change agent scheduling to select up to `--parallel` task ids from the ready set, then include every resolved invocation for each selected task in the spawn batch. Keep non-concurrent state deferral at the task level, and do not let the per-task fanout count consume additional parallel task slots.
  - Tests/checks: Add a run integration test with one ready task that resolves multiple `all_models` or `all_targets` invocations and `--parallel 1`, asserting all invocations for that task are spawned in the same pass. Run `cargo test -p rhei-cli --test integration_markdown_plans`.

## Rejected / Deferred

None.
