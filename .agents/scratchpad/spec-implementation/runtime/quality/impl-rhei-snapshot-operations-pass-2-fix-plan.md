# Quality Fix Plan pass 2

## Accepted

- Q-staging-manifests-visible: Snapshot readers must not parse in-progress or stale `g<N>.tmp-*` staging manifests as committed cache records.
  - Files: crates/rhei-cli/src/cli/snapshot_list_show.rs, crates/rhei-cli/src/cli/tests_snapshot_runtime.rs
  - Approach: Restrict manifest discovery to committed generation directories whose parent directory name is exactly `g<N>` and skip any directory component containing `.tmp-`; keep schema validation as the final guard. Add a regression that creates a valid committed generation plus a stale `g2.tmp-*` directory containing a manifest and verifies `read_snapshot_records`, list/show, and preload-visible readers ignore the staging record without failing.
  - Tests/checks: cargo test -p rhei-cli cli::tests_snapshot_runtime::snapshot_reader_ignores_stale_staging_manifests

- Q-run-override-selectors-source-scoped: `rhei run --from-snapshot` must identify exactly one active target task/fanout invocation before applying the override.
  - Files: crates/rhei-cli/src/cli/run_agent_mode.rs, crates/rhei-cli/src/cli/snapshot_runtime_preload.rs, crates/rhei-cli/src/cli/tests_snapshot_runtime.rs
  - Approach: Add a pre-spawn override-context check for `--from-snapshot` that enumerates active non-terminal tasks whose current state declares `snapshot.inherit`, expands the resolved fanout target slug(s) already used by the run loop, applies `--task` and `--target` to the current-run invocation axis, and errors with candidate task/target pairs unless exactly one invocation remains. In `preload_snapshot_inherit_before_spawn`, apply the override only to that selected current task/target; other invocations continue with authored inheritance. Keep source snapshot disambiguation in `resolve_snapshot_ref` and contract validation unchanged.
  - Tests/checks: cargo test -p rhei-cli cli::tests_snapshot_runtime::snapshot_from_snapshot_requires_unique_run_invocation

- Q-gc-reachability-source-axis: GC active-inherit protection must only protect snapshots reachable through the active `snapshot.inherit.from` source axis.
  - Files: crates/rhei-cli/src/cli/snapshot_refs_gc.rs, crates/rhei-cli/src/cli/tests_snapshots_gc.rs
  - Approach: Before evaluating name/select/generation protection, check `inherit.from`: for `self`, only protect records whose `task_id` equals the active task id; for `ancestor`, only protect records whose `task_id` is an ancestor of the active task id; preserve existing visit, target, state, and generation selector checks after that source-axis filter.
  - Tests/checks: cargo test -p rhei-cli cli::tests_snapshots_gc::snapshot_active_inherit_protection_respects_source_axis

- Q-redactor-default-env-empty: Redactor subprocesses need the spec-required minimal default environment and redactor diagnostics in the run log.
  - Files: docs/functional-spec/rhei-snapshot-operations.spec.md, crates/rhei-cli/src/cli/snapshot_records.rs, crates/rhei-cli/src/cli/snapshot_runtime_emit.rs, crates/rhei-cli/src/cli/tests_snapshot_runtime.rs
  - Approach: Document the concrete default redactor environment names in the spec, then after `env_clear()` set only those defaults before applying `snapshots.redactor_env` allowlist overrides. Include variables for the rhei executable path, workspace root, project settings path, and global settings path. Thread the existing snapshot emission log path into redactor execution and append one bounded diagnostic line per redactor run with path, status, timeout flag, truncation flag, and stderr summary; keep stderr out of `manifest.json`.
  - Tests/checks: cargo test -p rhei-cli cli::tests_snapshot_runtime::snapshot_redactor_receives_minimal_default_env_and_logs_diagnostics

## Rejected / Deferred

- None.
