# Quality Fix Log pass 2

## Q-agent-no-outputs-skips-spawn

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/cli/run_agent_mode.rs`
  - `crates/rhei-cli/src/cli/run_helpers.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans/run_agent_regressions.rs`
- Notes:
  - Agent states with no declared `outputs:` now keep resolved invocations pending for spawn instead of treating the empty output list as already complete.
  - Pending-invocation checks return false after a no-output agent run so the task can auto-advance after the subprocess exits.

## Q-model-only-missing-agent-falls-back

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/cli/agent_resolution.rs`
  - `crates/rhei-cli/src/cli/run_command.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans/run_agent_regressions.rs`
- Notes:
  - Run mode selection now examines runnable tasks and enters agent mode when a ready state declares autonomous agent/model/target work, even if no invocation resolves.
  - The existing missing-agent error is then surfaced by agent mode instead of silently falling back to callback-only transitions.

## Q-program-zero-missing-outputs-aborts

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/cli/run_agent_mode.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans/run_agent_regressions.rs`
- Notes:
  - Successful program exits now check required source outputs before looking for or applying the exit-code transition.
  - Missing required outputs emit a warning and leave the task in its current state without aborting the run.

## Q-fanout-split-by-parallel-limit

- Status: applied
- Files edited:
  - `crates/rhei-cli/src/cli/run_agent_mode.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - `crates/rhei-cli/tests/integration_markdown_plans/run_agent_regressions.rs`
- Notes:
  - Agent scheduling now selects up to `--parallel` task ids and includes every resolved invocation for each selected task.
  - Non-concurrent state deferral remains task-based.

## Checks

- `cargo fmt --all` - passed
- `cargo test -p rhei-cli --test integration_markdown_plans` - passed (79 tests)

## Left Unfixed

- None.
