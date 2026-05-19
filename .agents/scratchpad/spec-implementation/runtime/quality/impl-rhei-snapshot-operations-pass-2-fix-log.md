# Quality Fix Log pass 2

## Q-staging-manifests-visible

- Status: applied.
- Files edited:
  - `crates/rhei-cli/src/cli/snapshot_list_show.rs`
  - `crates/rhei-cli/src/cli/tests_snapshot_runtime.rs`
- Summary: restricted recursive manifest discovery to committed `g<N>` generation directories and skipped directory components containing `.tmp-`; added a regression covering `read_snapshot_records`, reference resolution, and inherit preload visibility with a stale staging manifest present.
- Checks:
  - `cargo test -p rhei-cli cli::tests_snapshot_runtime::snapshot_reader_ignores_stale_staging_manifests` was attempted, but the test target did not compile because pre-existing dirty code in `crates/rhei-cli/src/cli/tests_settings_tooling.rs` still calls the older 3-argument `should_use_agent_mode` signature while `crates/rhei-cli/src/cli/run_command.rs` now defines a 5-argument signature.

## Q-run-override-selectors-source-scoped

- Status: applied.
- Files edited:
  - `crates/rhei-cli/src/cli/run_agent_mode.rs`
  - `crates/rhei-cli/src/cli/snapshot_runtime_preload.rs`
  - `crates/rhei-cli/src/cli/tests_snapshot_runtime.rs`
- Summary: added pre-spawn selection of exactly one active `snapshot.inherit` run invocation for `--from-snapshot`, scoped `--task` and `--target` to the current task/target invocation axis, and applied the override only to that selected invocation while other invocations use authored inheritance.
- Checks:
  - `cargo test -p rhei-cli cli::tests_snapshot_runtime::snapshot_from_snapshot_requires_unique_run_invocation` was not rerun after the compile blocker above; it would hit the same test-target compile error before executing this test.

## Q-gc-reachability-source-axis

- Status: applied.
- Files edited:
  - `crates/rhei-cli/src/cli/snapshot_refs_gc.rs`
  - `crates/rhei-cli/src/cli/tests_snapshots_gc.rs`
- Summary: filtered active GC protection by `snapshot.inherit.from` before applying name, state, visit, target, and generation checks; `self` now protects only the active task's own records and `ancestor` protects only ancestor task records.
- Checks:
  - `cargo test -p rhei-cli cli::tests_snapshots_gc::snapshot_active_inherit_protection_respects_source_axis` was not rerun after the compile blocker above; it would hit the same test-target compile error before executing this test.

## Q-redactor-default-env-empty

- Status: applied.
- Files edited:
  - `docs/functional-spec/rhei-snapshot-operations.spec.md`
  - `crates/rhei-cli/src/cli/snapshot_records.rs`
  - `crates/rhei-cli/src/cli/snapshot_runtime_emit.rs`
  - `crates/rhei-cli/src/cli/tests_snapshot_runtime.rs`
- Summary: documented and set the default redactor environment (`RHEI_EXECUTABLE_PATH`, `RHEI_WORKSPACE_ROOT`, `RHEI_PROJECT_SETTINGS_PATH`, `RHEI_GLOBAL_SETTINGS_PATH`) before allowlisted overrides, and appended bounded redactor diagnostics to the run log without writing stderr to manifests.
- Checks:
  - `cargo test -p rhei-cli cli::tests_snapshot_runtime::snapshot_redactor_receives_minimal_default_env_and_logs_diagnostics` was not rerun after the compile blocker above; it would hit the same test-target compile error before executing this test.

## Additional Checks

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `cargo check -p rhei-cli --bin rhei`
- `git diff --check -- <edited files>`

## Intentionally Left Unfixed

- The unrelated `should_use_agent_mode` test signature mismatch in `crates/rhei-cli/src/cli/tests_settings_tooling.rs` was left unchanged because it is outside the accepted pass-2 fix plan and was present in the dirty worktree before these quality fixes.
