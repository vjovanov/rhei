# impl-rhei-run — Implementation Notes

Spec: `docs/functional-spec/rhei-run.spec.md`

The bulk of `rhei-run.spec.md` was already implemented before this task
(command surface, execution loop, ready-set scan, gating handling, parallel
execution, dry-run output, callback-only mode, polling state behavior,
concurrent-state rule, RunOptions plumbing, dashboard/TUI integration).
The PR#2 diff against `main` (`origin/main..origin/fix/snapshot-spec-review`)
adds four normative changes to this spec — the Snapshots flag group, the
spawn-time inherit preload step, the select → emit → apply transition
ordering, and the polling-state snapshot rules. This task closes those gaps
within the rhei-run surface and leaves the snapshot module proper (storage,
manifest writes, preload resolver, `--from-snapshot` semantics, cache
maintenance) to `impl-rhei-snapshots` and `impl-rhei-snapshot-operations`,
which are the spec owners for that work.

## Coverage matrix

| Spec section | Implementation | Status |
|---|---|---|
| §Usage | `Commands::Run` in `crates/rhei-cli/src/main.rs` | preexisting |
| §Options → Standalone | `StandaloneExecutionFlags` (`main.rs:5461`) | preexisting |
| §Options → Agent Execution | `AgentExecutionFlags` (`main.rs:5491`) | preexisting |
| §Options → **Snapshots** | **new**: `SnapshotExecutionFlags` (`main.rs:~5518`) with `--from-snapshot`, `--override-inherit`, `--task`, `--target`; `clap` `requires` enforces `--override-inherit` ⇒ `--from-snapshot` | this task |
| §Options → Program Execution | `ProgramExecutionFlags` (`main.rs:5509`) | preexisting |
| §Execution Loop step 1 (load + validate) | `run_command` (`main.rs:~7680`) | preexisting |
| §Execution Loop step 2 (ready set, poll exclusion) | `find_ready_tasks` (`main.rs:~9123`) — poll exclusion is honored via `pollNextAttemptAt` metadata in `transition_rule_is_applicable` / `evaluate_transition_condition` | preexisting |
| §Execution Loop step 3 (mode selection, target resolution, **snapshot preload**, spawn) | `run_agent_mode` (`main.rs:~7730`); **new**: `preload_snapshot_inherit_before_spawn` hook invoked before `spawn_and_wait_agent` in both the sequential and parallel-spawn branches | this task (call site); preload body is deferred to `impl-rhei-snapshots` |
| §Execution Loop step 3 — timeout / SIGTERM / 10 s grace / SIGKILL | `spawn_and_wait_agent` (`main.rs:~7280`) | preexisting |
| §Execution Loop step 4 (Completion Condition) | `state_outputs_exist_for_resolved_invocation`, exit-status branches in `run_agent_mode` | preexisting |
| §Execution Loop step 5 (**select** transition without applying) | **new**: `try_auto_advance_task` (`main.rs:~9396`) refactored to call `find_next_transition` first, then the emit hook, then `execute_transition` | this task |
| §Execution Loop step 6 (**emit** snapshots after selection, before application) | **new**: `emit_snapshots_after_transition_selection` hook called from `try_auto_advance_task`. Body is a no-op stub; the snapshot writes are owned by `impl-rhei-snapshots` (`rhei-snapshots.spec.md` §10.2). The call site pins the spec-mandated ordering. | this task (call site); emit body deferred to `impl-rhei-snapshots` |
| §Execution Loop step 7 (apply transition; subprocess must not call `rhei transition`) | `execute_transition` (`main.rs:~5033`) | preexisting |
| §Execution Loop step 8 (loop until no progress; exit code semantics) | `run_agent_mode` outer loop + summary block | preexisting |
| §Gating handling (non-immediate-abort) | gating short-circuit in `run_agent_mode` ready-task loop | preexisting |
| §Dry Run | dry-run branches in `run_agent_mode` and `run_callback_mode` | preexisting |
| §Parallel Execution (slot index, `SlotAssigned`/`SlotReleased`, lock serialization) | parallel branch in `run_agent_mode` + TUI `RunEvent` emissions | preexisting |
| §Polling States — `pollNextAttemptAt`, sleep until earliest deadline, exhaustion, clearing on non-self-loop exit | poll branches in `transition_rule_is_applicable`, `evaluate_transition_condition`, `update_metadata_for_transition`, and the run loop | preexisting |
| §Polling States — **`snapshot.inherit` rejected in v1** | validator concern (owned by `impl-rhei-states` / `impl-rhei-snapshots`); `rhei run` has no separate runtime guard because the validator runs at `run_command` entry. | preexisting validator hook; full rule wired by `impl-rhei-snapshots` |
| §Polling States — **snapshot emit suppression on self-loop attempts** | hook contract in `emit_snapshots_after_transition_selection`: callers pass `(current_state, selected_to_state)`; the snapshot module inspects them and suppresses emit when the selected transition is a self-loop. | this task (contract); suppression body deferred to `impl-rhei-snapshots` |
| §Concurrent vs. Serial States | non-concurrent-state filter inside `run_agent_mode` (`main.rs:~8222`) | preexisting |
| §Relationship to Other Commands | exclusivity is enforced by the file lock plus the manual-worker `claim` step; no code change | preexisting |

## Changes made in this task

1. **`SnapshotExecutionFlags` flag group** (`main.rs`)
   - Added a new `#[derive(Args)]` struct with the four flags specified in
     `rhei-run.spec.md` §Options → Snapshots: `--from-snapshot <REF>`,
     `--override-inherit`, `--task <TASK_ID>`, `--target <SLUG>`.
   - `--override-inherit` carries clap `requires = "from_snapshot"` so the
     pair-only contract from `rhei-snapshot-operations.spec.md` §2 Run
     Override is enforced at parse time. The pair contract also covers the
     "must still declare `snapshot.inherit:`" rule — that runtime check lives
     in the snapshot module owned by `impl-rhei-snapshots`.
   - The flag struct lives in a dedicated `Snapshots` help heading so the
     four flags appear under their own section in `rhei run --help`.

2. **`Run` command variant, `RunOptions`, and `dispatch()` plumbing**
   - `Commands::Run` now carries `snapshot: SnapshotExecutionFlags` alongside
     the existing standalone / agent / program flag groups.
   - `RunOptions` gains a `snapshot: SnapshotExecutionFlags` field, accessor
     methods (`from_snapshot`, `override_inherit`, `snapshot_task_selector`,
     `snapshot_target_selector`), and an updated `From<…tuple…>` impl.
   - `dispatch()` and `parse_execute_run_options` (the `rhei instantiate
     --execute` argument forwarder) pass the new flag group through.
   - `default_run_options()` includes the default (all-`None`) snapshot flag
     set.

3. **Orchestration hook for snapshot inherit preload** (spec §Execution Loop
   step 3)
   - New `preload_snapshot_inherit_before_spawn` function invoked from both
     the sequential and parallel-spawn branches of `run_agent_mode`, after
     the prompt is composed and the agent log path is computed, but
     immediately before `spawn_and_wait_agent`. The hook takes the resolved
     state, task, `ResolvedAgent`, and `RunOptions`, so the snapshot module
     can read the four CLI overrides plus the agent transport profile.
   - Body is a deliberate no-op stub. The contract is documented in-place
     and references `rhei-snapshots.spec.md` §10.1 Spawn-Time Preload and
     `rhei-snapshot-operations.spec.md` §2 Run Override. The actual
     resolution, `compat:` evaluation, `ResumeStrategy`/`ForkStrategy`
     staging, and `g<N>.tmp-*` directory creation live in
     `impl-rhei-snapshots`.

4. **Orchestration hook for snapshot emit between select and apply** (spec
   §Execution Loop steps 5–7)
   - `try_auto_advance_task` is refactored so the spec's three-phase shape is
     visible in the code rather than implied: select (`find_next_transition`)
     → emit hook (`emit_snapshots_after_transition_selection`) → apply
     (`execute_transition`). The intermediate hook fires exactly once per
     post-spawn transition decision.
   - The emit hook receives `(machine, task, current_state,
     selected_to_state)`. That signature is enough for the snapshot module to
     (a) determine whether the state is agent-bearing and snapshot-capable
     and (b) detect a poll self-loop (`selected_to_state == current_state`
     under a `poll:` block) so it can suppress emit per spec §Polling States.
   - The hook is a no-op stub today; the body is owned by
     `impl-rhei-snapshots`. The call site placement matches the spec's
     mandate that emit fires "after transition selection and before the
     transition is applied."

5. **Tests** (in `crates/rhei-cli/src/main.rs` `tests` module)
   - `parses_run_command_with_separated_flag_groups` is updated to also
     assert the new snapshot-flag-group defaults (all `None` / `false`).
   - `parses_run_command_with_snapshot_flags` — new test that asserts the
     four flags parse with realistic values, including the spec-style ref
     example from `rhei-snapshot-operations.spec.md` §1.3
     (`1.2.3:implementation:pending@2:claude-code-anthropic-claude-opus-4-7/g3`).
   - `run_rejects_override_inherit_without_from_snapshot` — new test that
     pins the clap-level `--override-inherit` ⇒ `--from-snapshot` requirement.
   - `run_help_separates_standalone_and_agent_flags` is extended to assert
     the new `Snapshots:` help heading and the four flag names appear in
     `rhei run --help`.

## Deferrals

- **Snapshot preload and emit bodies.** Both `preload_snapshot_inherit_before_spawn`
  and `emit_snapshots_after_transition_selection` are no-op stubs whose call
  sites are now wired into the spec-correct positions. The actual
  implementations — resolving the source snapshot, evaluating `compat:`,
  applying `ResumeStrategy`/`ForkStrategy`, writing the auto- and named-
  snapshot manifests + transcripts atomically — belong to
  `impl-rhei-snapshots` (which owns `rhei-snapshots.spec.md` §10.1 and §10.2).
  The flag accessors (`opts.from_snapshot()`, `opts.override_inherit()`,
  `opts.snapshot_task_selector()`, `opts.snapshot_target_selector()`) are
  passed into the preload hook so the consuming task can read them without
  touching `RunOptions` again.
- **Polling-state validator rule (`snapshot.inherit` rejected on poll states
  in v1).** The rejection rule is a validator concern per
  `rhei-snapshots.spec.md` §11 Validation Rules; it is added by
  `impl-rhei-snapshots` (which owns the snapshot YAML schema on `StateDef`)
  and/or `impl-rhei-states`. `rhei run` runs the validator at command entry
  (`validate_with_machine` in `run_command`) so the rule, once wired,
  applies without a separate runtime guard here.
- **Auto-emit `_state` suppression for `final:`/`gating:`/`program:` states.**
  These are runtime decisions made inside the emit hook body, which is owned
  by `impl-rhei-snapshots`. The hook contract gives the implementer the
  state def via `machine.states.get(current_state)` so the suppression rules
  can be honored without re-plumbing.
- **`--from-snapshot` plus "target state must still declare
  `snapshot.inherit:`" check.** Lives in the snapshot module
  (`impl-rhei-snapshots` / `impl-rhei-snapshot-operations`); CLI parsing is
  flag-level only here.

## Spec-line cross-references for non-obvious calls

- Spec §Options → Snapshots, lines 38–44 → `SnapshotExecutionFlags`
  (`main.rs` near `ProgramExecutionFlags`).
- Spec §Execution Loop step 3, line 63 ("If the state declares
  `snapshot.inherit:`, resolve and preload the source snapshot before
  spawning the agent") → `preload_snapshot_inherit_before_spawn` call sites
  in `run_agent_mode` sequential branch (`main.rs:~8385`) and parallel
  branch (`main.rs:~8621`).
- Spec §Execution Loop steps 5–7, lines 67–81 → `try_auto_advance_task`
  three-phase refactor with explicit `// Step 5/6/7` markers (`main.rs`
  near `find_next_transition`).
- Spec §Polling States, lines 126–128 ("Snapshot emit … is suppressed for
  self-loop attempts and runs only on a terminal non-self-loop exit") →
  contract documented on `emit_snapshots_after_transition_selection`; the
  selected-transition signature lets the snapshot module compare against
  `current_state` to detect a self-loop.

## Build / test result

- `cargo build -p rhei-cli` → clean.
- `cargo build --workspace` → clean.
- `cargo test -p rhei-cli --bin rhei` → 59 / 59 pass.
- `cargo test --workspace --no-fail-fast` → all pass except
  `e2e::run_tests::changeset_review_human_review_state_is_gating_in_shipped_workflows`,
  which fails on `main` as well (pre-existing fixture issue verified via
  `git stash`-checkpoint isolation; unrelated to this task).
