# Completeness Fix Log: impl-rhei-run

Spec: `docs/functional-spec/rhei-run.spec.md`

## Fixed

- Dry-run output/artifacts gaps:
  - Implemented the exact `would transition: Task <ID>  <from> -> <to>` dry-run line for callback, agent, and program run paths.
  - Dry-run no longer acquires `.rhei/run.lock` and uses a stdout-only frontend, so it does not create `runtime/transitions.log` or other runtime frontend artifacts.
  - Files edited: `crates/rhei-cli/src/main.rs`, `crates/rhei-cli/tests/integration_markdown_plans.rs`.
  - Tests updated: `run_dry_run_shows_transitions_without_changes`.

- Callback-only mode selection gap:
  - `rhei run` now chooses callback-only mode globally when all autonomous work has been disabled by `--no-agent` / `--no-program`, instead of entering agent/program orchestration and downgrading per state.
  - Files edited: `crates/rhei-cli/src/main.rs`.
  - Tests covered by existing/updated run integration tests, including `run_executes_all_models_callbacks_without_agent_configuration`.

- Ready-set input eligibility gap:
  - The run ready set now requires the current state's required `inputs:` artifacts to exist before scheduling a task.
  - Files edited: `crates/rhei-cli/src/main.rs`, `crates/rhei-cli/tests/integration_markdown_plans.rs`.
  - Tests added: `run_ready_set_requires_state_inputs`.

- Poll ready-set and retry lifecycle gaps:
  - The ready set now excludes poll states with `metadata.tasks.<id>.pollNextAttemptAt.<state>` in the future.
  - Poll self-loops now persist `pollNextAttemptAt` and increment `stateVisits`, release the run slot, and the run loop sleeps until the next poll deadline when only poll-blocked work remains.
  - Poll self-loops are refused once `poll.max_attempts` is exhausted through the existing transition applicability path.
  - Non-self-loop poll exits clear both `pollNextAttemptAt.<state>` and `stateVisits.<state>`.
  - Files edited: `crates/rhei-cli/src/main.rs`, `crates/rhei-cli/tests/integration_markdown_plans.rs`.
  - Tests added: `run_poll_self_loop_schedules_next_attempt_and_clears_on_exit`.

- Polling snapshot-inherit rejection gap:
  - Validator now rejects states that declare both `poll:` and `snapshot.inherit:`.
  - Files edited: `crates/rhei-validator/src/lib.rs`.
  - Tests added: `rejects_poll_with_snapshot_inherit`.

- Program exit-code transition selection gap:
  - Program exit-code routing now selects the first declared matching rule after condition filtering instead of failing on multiple matching rules.
  - Files edited: `crates/rhei-cli/src/main.rs`.
  - Tests covered by existing program run tests.

- Stuck non-terminal exit gap:
  - Run modes now return non-zero when progress halts with non-terminal tasks remaining, except when remaining work is only terminal, human-gating/gating-blocked, or poll-blocked.
  - Files edited: `crates/rhei-cli/src/main.rs`, `crates/rhei-cli/tests/integration_markdown_plans.rs`.
  - Tests updated: `run_callback_failure_halts_execution`.

- Manual-worker overlap / in-flight ready-set gap:
  - `rhei run` scheduling now uses the runnable-task path that excludes tasks carrying an assignee, so a manually claimed task is not scheduled by the run orchestrator.
  - Files edited: `crates/rhei-cli/src/main.rs`.
  - Tests covered by existing runnable/claimable behavior; no new test added in this patch.

## Deferred

- Snapshot preload and emission implementation:
  - Deferred: the current run code has only orchestration hook points and snapshot cache commands; implementing true session preload, compatibility checks, auto `_state` emission, named `snapshot.emit:`, parent refs, transcript capture, and `--from-snapshot` override semantics requires the snapshot/session module ownership and supported agent session layouts. This patch preserved and tightened the authored `snapshot.inherit` target-state guard and added the poll/inherit validator rejection, but did not implement full snapshot IO.
  - Remaining affected gaps: `snapshot.inherit` preload behavior, `--from-snapshot` / `--override-inherit` override semantics beyond the target-state guard, auto/named snapshot emission, poll self-loop snapshot suppression by actual emit IO, terminal poll-exit snapshot emission.

- Program subprocess parallelism:
  - Deferred: agent fanout already uses concurrent subprocess slots, but program states still use the existing sequential result-application loop. Closing this requires a larger scheduler refactor so program subprocesses can spawn concurrently while transition application remains serialized through the existing lock/CAS path.
  - Remaining affected gaps: `--parallel <N>` for program subprocesses.

- Subprocess prohibition runtime guard:
  - Deferred: prompts tell spawned agents not to call `rhei transition` / `rhei complete`, and the orchestrator owns normal transitions, but there is no reliable runtime guard for arbitrary external program subprocesses invoking the CLI without introducing execution sandboxing or command interception outside the current architecture.

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy -p rhei-cli -p rhei-cli-validator --all-targets -- -D warnings -W clippy::all`
- `cargo test -p rhei-cli --test integration_markdown_plans run_ -- --nocapture`
- `cargo test -p rhei-cli-validator poll -- --nocapture`

Additional check:

- `cargo test -p rhei-cli --test integration run_ -- --nocapture` was run and all run-related behavior covered by this patch passed, but the command failed on an unrelated existing fixture assertion: `examples/changeset-review-example/states.yaml` is missing `human-review`.
