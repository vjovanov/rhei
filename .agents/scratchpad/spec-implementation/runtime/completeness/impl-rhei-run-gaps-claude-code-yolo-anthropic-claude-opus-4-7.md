# Completeness Audit: `docs/functional-spec/rhei-run.spec.md`

**Auditor:** claude-code[yolo]:anthropic:claude-opus-4-7
**Spec:** `docs/functional-spec/rhei-run.spec.md`
**Implementation surface:** `crates/rhei-cli/src/main.rs`, `crates/rhei-tui/src/journal.rs`, `crates/rhei-validator/src/lib.rs`
**Methodology:** Every normative claim in the spec is enumerated and classified as `covered` / `partial` / `missing` / `not-normative`. Evidence is cited with `file:line`. Code quality is out of scope.

Legend: ✅ covered · 🟡 partial · ❌ missing · ⚪ not-normative (descriptive prose only)

---

## 1. Framing & authority (spec lines 3–5)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 1.1 | `rhei run` drives a plan end-to-end by repeatedly claiming the next ready task, spawning the state's agent or program, waiting for completion, and performing the resulting transition | ✅ | `crates/rhei-cli/src/main.rs:7731` (`run_command` → `run_agent_mode`), execution loop `crates/rhei-cli/src/main.rs:7860–8809` |
| 1.2 | `rhei run` operates under `orchestrator` authority — the orchestrator (not the spawned subprocess) owns every state transition | ✅ | `crates/rhei-cli/src/main.rs:9466` (`execute_transition` is invoked by `try_auto_advance_task` after subprocess exit); subprocess does not call `rhei transition` |
| 1.3 | Live TUI behaviour is specified separately in `rhei-run-tui.spec.md` | ⚪ | Cross-reference, not normative |

---

## 2. Usage (spec lines 7–11)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 2.1 | Command form `rhei run <RHEI_PLAN_OR_WORKSPACE> [flags]` | ✅ | `Commands::Run { input, … }` at `crates/rhei-cli/src/main.rs:217–229`; positional arg with `RHEI_PLAN` value name |

---

## 3. Options — Standalone (spec lines 17–26)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 3.1 | `--dry-run` (default `false`): print the sequence of transitions that would be made without executing them | ✅ | `StandaloneExecutionFlags.dry_run` at `crates/rhei-cli/src/main.rs:5468`; consumed by `run_agent_mode` (`main.rs:7965, 8048, 8310, 8323`) and `run_callback_mode` (`main.rs:8989`) |
| 3.2 | `--no-callbacks` (default `false`): skip `on_leave` / `on_enter` callbacks | ✅ | `StandaloneExecutionFlags.no_callbacks` at `crates/rhei-cli/src/main.rs:5471`; passed into `execute_transition` (`main.rs:8020, 9015, 9473`) and into `try_auto_advance_task` |
| 3.3 | `--continue-on-error` (default `false`): continue when an agent or program exits non-zero | ✅ | `crates/rhei-cli/src/main.rs:5474`; checked at non-zero-exit branches (`main.rs:8233, 8244, 8553, 8566, 8786, 8796`) |
| 3.4 | `--parallel <N>` (default `1`, `0 = unlimited`): max number of agents or programs running concurrently | ✅ | `crates/rhei-cli/src/main.rs:5477`; `0`-means-unlimited handled at `crates/rhei-cli/src/main.rs:8319-8320` (`if max_parallel == 0 { agent_tasks.len() }`) |
| 3.5 | `--tui` (auto-detect default): force TUI mode even when stdout is not a TTY | ✅ | `crates/rhei-cli/src/main.rs:5480` + `RunOptions::frontend_kind()` at `crates/rhei-cli/src/main.rs:5570` |
| 3.6 | `--no-tui` (auto-detect default): force plain stdout output even when stdout is a TTY | ✅ | `crates/rhei-cli/src/main.rs:5483`; `RunOptions::frontend_kind()` at `crates/rhei-cli/src/main.rs:5573` |
| 3.7 | `--dashboard` / `--no-dashboard` flags | ⚪ | Implemented but not specified in `rhei-run.spec.md` — out of audit scope (likely belongs to TUI spec). Implementation exists at `crates/rhei-cli/src/main.rs:5485–5489`. |

---

## 4. Options — Agent Execution (spec lines 30–35)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 4.1 | `--no-agent`: disable agent spawning; use callback-only advancement | ✅ | `AgentExecutionFlags.no_agent` at `crates/rhei-cli/src/main.rs:5498`; fallback branch at `crates/rhei-cli/src/main.rs:7926–7929` (push to `callback_tasks` when invocations empty under `--no-agent`) |
| 4.2 | `--agent <AGENT>`: override the agent for this run | ✅ | `crates/rhei-cli/src/main.rs:5500-5501`; consumed by agent resolution path |
| 4.3 | `--agent-mode <MODE>`: override agent mode | ✅ | `crates/rhei-cli/src/main.rs:5503-5504` |
| 4.4 | `--model <MODEL>`: override the model for this run | ✅ | `crates/rhei-cli/src/main.rs:5506-5507` |

---

## 5. Options — Snapshots (spec lines 38–44)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 5.1 | `--from-snapshot <ref>` parses correctly | ✅ | `SnapshotExecutionFlags.from_snapshot` at `crates/rhei-cli/src/main.rs:5531-5532`; test `crates/rhei-cli/src/main.rs:11925-11957` |
| 5.2 | `--override-inherit` parses correctly | ✅ | `crates/rhei-cli/src/main.rs:5535-5536` |
| 5.3 | `--override-inherit` requires `--from-snapshot` (clap `requires`) | ✅ | `crates/rhei-cli/src/main.rs:5535` (`requires = "from_snapshot"`); test `crates/rhei-cli/src/main.rs:11960-11968` |
| 5.4 | `--task <id>`: select the task for an ambiguous snapshot override | ✅ | `crates/rhei-cli/src/main.rs:5538-5539` |
| 5.5 | `--target <slug>`: select the fanout target for an ambiguous snapshot override | ✅ | `crates/rhei-cli/src/main.rs:5541-5542` |
| 5.6 | `--from-snapshot` actually **overrides** the source snapshot selected by an authored `snapshot.inherit:` at runtime | ❌ | `preload_snapshot_inherit_before_spawn` at `crates/rhei-cli/src/main.rs:9525-9543` is a no-op stub: it takes `opts.from_snapshot()` etc. but discards them (`let _ = (…)`). No source-snapshot resolution, no compat evaluation, no staging into the inheritor's generation directory. The implementation notes acknowledge this is deferred to `impl-rhei-snapshots`. |
| 5.7 | `--override-inherit` actually bypasses source-selection and compatibility constraints | ❌ | Same stub at `crates/rhei-cli/src/main.rs:9525-9543` — no compat or source-selection logic exists to override. |
| 5.8 | When `--override-inherit` is set, the target state must still declare `snapshot.inherit:` (runtime guard, beyond the clap pair check) | ❌ | No runtime guard found; the hook body would have to enforce it. Notes defer this to `impl-rhei-snapshots`. |
| 5.9 | Help heading renders these four flags under a dedicated `Snapshots` group | ✅ | `crates/rhei-cli/src/main.rs:5527` (`#[command(next_help_heading = "Snapshots")]`); test `crates/rhei-cli/src/main.rs:11990` |

---

## 6. Options — Program Execution (spec lines 48–51)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 6.1 | `--no-program`: disable program spawning; use callback-only advancement for program states | ✅ | `ProgramExecutionFlags.no_program` at `crates/rhei-cli/src/main.rs:5515-5516`; fallback at `crates/rhei-cli/src/main.rs:7913-7917` |
| 6.2 | `--program-timeout <DURATION>`: override the program timeout for this run | ✅ | `crates/rhei-cli/src/main.rs:5518-5519`; consumed by program-timeout resolution |

---

## 7. Execution Loop — Mode selection paragraph (spec line 57)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 7.1 | Use orchestrated subprocess execution whenever any reachable non-terminal, non-gating state declares autonomous work via `program`, `agent`, `target`, `all_targets`, `model`, or `all_models` | ✅ | `state_declares_autonomous_execution` at `crates/rhei-cli/src/main.rs:9360-9367` checks all six fields; mode selector at `crates/rhei-cli/src/main.rs:7764-7773` |
| 7.2 | Callback-only advancement entered only when no autonomous state exists, **or** when the caller explicitly disables spawning with `--no-agent` / `--no-program` | 🟡 | The outer mode switch at `crates/rhei-cli/src/main.rs:7775-7779` chooses agent mode whenever any state declares autonomous work and ignores `--no-agent`/`--no-program` at the outer-mode level; instead, those flags trigger per-state fallback at `crates/rhei-cli/src/main.rs:7913-7917` (program) and `crates/rhei-cli/src/main.rs:7926-7929` (agent). The net effect equates to callback-only for those states, but the implementation does *not* literally enter "callback-only advancement" via `run_callback_mode` when both flags are set. Functionally equivalent for spec intent, but the dispatch path is different. |
| 7.3 | If a state declares model/target-driven work but no agent transport resolves, `rhei run` fails with a missing-agent configuration error; it does NOT silently fall back to callback-only transitions for that state | ✅ | `crates/rhei-cli/src/main.rs:7930-7935` returns `miette!("no agent configured. …")` unless `--no-agent` is set |

---

## 8. Execution Loop — Step 1: load and validate (spec line 59)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 8.1 | Load state machine and plan, then validate | ✅ | `crates/rhei-cli/src/main.rs:7738-7762` — `load_plan`, `resolve_state_machine_for_loaded_plan`, then `validate_with_machine` |
| 8.2 | Validation errors abort with a non-zero exit before any agent spawn | ✅ | `crates/rhei-cli/src/main.rs:7760-7762` returns `Err` which `main` converts to exit 1 (`crates/rhei-cli/src/main.rs:463-469`) |

---

## 9. Execution Loop — Step 2: ready-set computation (spec line 60)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 9.1 | Ready set = tasks whose `**Prior:**` are all in terminal states | ✅ | `find_ready_tasks` at `crates/rhei-cli/src/main.rs:9200-9237` builds `state_map` and walks `task.prior` |
| 9.2 | Ready set excludes tasks in terminal states | ✅ | `crates/rhei-cli/src/main.rs:9220-9223` |
| 9.3 | Ready set excludes tasks in gating states | ✅ | Same site, `def.gating` check at `crates/rhei-cli/src/main.rs:9221` |
| 9.4 | Ready set requires the current state's required `inputs:` to all exist on disk | ❌ | `find_ready_tasks` does not call `ensure_state_inputs_exist`. Inputs are only enforced at transition time inside `execute_transition` (`crates/rhei-cli/src/main.rs:5336`), not when computing the ready set. Tasks missing `inputs:` are still scheduled and only fail at the transition step. |
| 9.5 | Ready set excludes tasks whose current state declares `poll:` and whose `metadata.tasks.<id>.pollNextAttemptAt.<state-name>` is in the future | ❌ | No `pollNextAttemptAt` metadata key is read or written anywhere in `crates/rhei-cli/src/main.rs` (grep returns zero hits). `find_ready_tasks` has no poll-deadline filter. |
| 9.6 | Dependency-satisfaction rule mirrors `rhei next` (terminal cancellation does not satisfy dependencies) | ✅ | `dependency_is_satisfied` at `crates/rhei-cli/src/main.rs:9192-9194` — `"cancelled"` returns false |

---

## 10. Execution Loop — Step 3: per-task spawn (spec lines 61–65)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 10.1 | Up to `--parallel` tasks from the ready set are executed concurrently | ✅ | Sequential branch at `crates/rhei-cli/src/main.rs:8339-8570`, parallel branch at `crates/rhei-cli/src/main.rs:8571-8801` |
| 10.2 | Concurrent-state rule: at most one ready task per non-`concurrent` state is scheduled per pass | ✅ | Filter at `crates/rhei-cli/src/main.rs:8271-8315` (`state_claimant` map) |
| 10.3 | Resolve the state's target (agent subprocess or program) | ✅ | `resolve_program` at `crates/rhei-cli/src/main.rs:7919`; `resolve_agent_invocations` at `crates/rhei-cli/src/main.rs:7923` |
| 10.4 | If the state declares `snapshot.inherit:`, resolve and preload the source snapshot **before** spawning the agent | 🟡 | Call site is correctly placed before `spawn_and_wait_agent` in both branches: `crates/rhei-cli/src/main.rs:8388-8394` (sequential) and `crates/rhei-cli/src/main.rs:8624-8630` (parallel). But the hook body at `crates/rhei-cli/src/main.rs:9525-9543` is a deliberate no-op stub — no source resolution, no compat evaluation, no staging. Result: the spec's preload step does not actually happen. Implementation notes defer this to `impl-rhei-snapshots`. |
| 10.5 | Polling states reject `snapshot.inherit` in v1 | ❌ | No runtime guard in `rhei run`. Implementation notes acknowledge this is a validator concern owned by `impl-rhei-snapshots` / `impl-rhei-states`. No guard exists today. |
| 10.6 | Spawn the subprocess with state's resolved instructions, `RHEI_*` environment variables, and timeout | ✅ | `compose_agent_prompt` at `crates/rhei-cli/src/main.rs:8364`; `build_agent_command` and `spawn_and_wait_agent` at `crates/rhei-cli/src/main.rs:7280-7451`; env-var population in `build_agent_command` |
| 10.7 | Wait for subprocess to exit or timeout to fire | ✅ | `crates/rhei-cli/src/main.rs:7399-7428` |
| 10.8 | On timeout: send `SIGTERM`, 10 s grace, then `SIGKILL` | ✅ | Agent path: `crates/rhei-cli/src/main.rs:7406-7418` with `AGENT_TERMINATE_GRACE = 10s` at `crates/rhei-cli/src/main.rs:7182`. Program path: `crates/rhei-cli/src/main.rs:7607-7619` (hard-coded 10 s sleep). |
| 10.9 | Orchestrator timeout requirement: spawned agent invocations must resolve to a finite timeout | ✅ | `ensure_orchestrator_timeout` at `crates/rhei-cli/src/main.rs:6684-6699`, invoked at `crates/rhei-cli/src/main.rs:7967` |

---

## 11. Execution Loop — Step 4: Completion Condition (spec line 66)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 11.1 | On subprocess exit, evaluate the state's Completion Condition: exit code `0` plus every required `outputs:` artifact present on disk | ✅ | `state_outputs_exist_for_resolved_invocation` at `crates/rhei-cli/src/main.rs:6710-6732` checks artifact presence; exit-code 0 is checked at `crates/rhei-cli/src/main.rs:8453` (`status.success()`) before invoking `try_auto_advance_task` |
| 11.2 | If outputs are missing for any pending invocation, task stays in its current state (the auto-advance branch is gated on `task_has_pending_agent_invocations` returning false) | ✅ | `crates/rhei-cli/src/main.rs:8485-8504` and `crates/rhei-cli/src/main.rs:8725-8748` |

---

## 12. Execution Loop — Step 5: select transition without applying (spec lines 67–73)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 12.1 | After subprocess exit, **select** the outgoing transition without applying it yet | ✅ | `try_auto_advance_task` at `crates/rhei-cli/src/main.rs:9420-9477`: `find_next_transition` runs first, returns `to_state`, application happens later in the same function |
| 12.2 | If the Completion Condition holds, select the first declared transition whose `condition` / `exit_code` matches | ✅ | `find_next_transition` at `crates/rhei-cli/src/main.rs:9374-9418` iterates `machine.transitions()` and returns the first applicable match (exact-`from` preferred, wildcard last) |
| 12.3 | If the Completion Condition fails (non-zero exit or missing outputs), route through the state's error or timeout transition per Agents Specification | 🟡 | For agents, `fire_timeout_transition` at `crates/rhei-cli/src/main.rs:9106-9154` fires only when a rule with a `timeout:` field exists. There is no separate "error" transition lookup — the implementation treats the timeout transition as the only error-path edge. For program states, `find_program_exit_transition` at `crates/rhei-cli/src/main.rs:7669-7726` routes via `exit_code:` clauses (including `"nonzero"`). The combined coverage matches the spec for both kinds, but the `mcp_unavailable` / `skill_unavailable` triggers documented elsewhere are not routed by `rhei run` step 5 itself. |
| 12.4 | When no error transition is declared and `--continue-on-error` is unset, `rhei run` aborts with a non-zero exit code | ✅ | Agent path: `crates/rhei-cli/src/main.rs:8553-8561` and `crates/rhei-cli/src/main.rs:8786-8791` return `Err(miette!(...))` which propagates to `dispatch` and exit 1. Program path: `crates/rhei-cli/src/main.rs:8233-8238`. |

---

## 13. Execution Loop — Step 6: emit snapshots between select and apply (spec lines 74–79)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 13.1 | For agent-bearing states with supported snapshot sessions, write auto-emitted `_state` snapshots and matching named `snapshot.emit:` after transition selection and before the transition is applied | 🟡 | Call site placement is correct: `try_auto_advance_task` at `crates/rhei-cli/src/main.rs:9442-9456` invokes `emit_snapshots_after_transition_selection` *between* `find_next_transition` and `execute_transition`. But the hook body at `crates/rhei-cli/src/main.rs:9494-9505` is a no-op stub: it discards all arguments via `let _ = (...)` and writes nothing. No snapshot manifests are written today. Implementation notes defer the body to `impl-rhei-snapshots`. |
| 13.2 | Poll self-loop attempts do not emit | ❌ | The hook is a no-op, so trivially the rule is not enforced. The hook signature does include `(current_state, selected_to_state)` so the future implementation can detect self-loops, but currently nothing is emitted in either direction so the suppression rule is vacuous rather than enforced. |
| 13.3 | Terminal poll exits may emit | ❌ | Same — vacuous because nothing is emitted. |

---

## 14. Execution Loop — Step 7: apply transition; subprocess must not call `rhei transition` (spec lines 80–81)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 14.1 | Apply the selected transition after step 6 | ✅ | `crates/rhei-cli/src/main.rs:9466-9474` invokes `execute_transition` |
| 14.2 | The orchestrator owns the transition; the subprocess must not call `rhei transition` or `rhei complete` | ⚪ | Spec contract on subprocess behaviour; not enforceable from the orchestrator side. The orchestrator does drive transitions itself, so the documented contract is upheld from the engine end. |

---

## 15. Execution Loop — Step 8: loop until no progress; exit codes (spec line 82)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 15.1 | Repeat passes until no pass makes progress | ✅ | `loop { … if !advanced_any { break; } }` at `crates/rhei-cli/src/main.rs:7860-8808` |
| 15.2 | Exit `0` when the plan reaches a state where every task is terminal | ✅ (de facto) | `run_agent_mode` returns `Ok(())` on natural loop exit (`crates/rhei-cli/src/main.rs:8872`), which propagates to `main` exit 0 (`crates/rhei-cli/src/main.rs:463-470`). The exit-0 path is taken whether or not all tasks are terminal. |
| 15.3 | Exit **non-zero** when progress halts with non-terminal tasks remaining and no further advancement is possible | ❌ | `run_agent_mode` returns `Ok(())` whenever the loop breaks without progress (`crates/rhei-cli/src/main.rs:8252-8264`, `crates/rhei-cli/src/main.rs:8806-8808`). The summary print logs "No tasks could be advanced." but the process still exits 0. The spec's distinction between "fully terminal" and "stuck with non-terminal tasks" is not honoured. |

---

## 16. Gating-state handling (spec lines 84–92)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 16.1 | `rhei run` does not transition out of gating states; exiting one requires an explicit human-initiated `rhei transition` | ✅ | Gating states are excluded from the ready set at `crates/rhei-cli/src/main.rs:9220-9223`. No `rhei run` codepath selects a transition out of a gating state. |
| 16.2 | Gating states are a barrier, not an immediate global abort | ✅ | The ready-set filter excludes gating tasks but other ready tasks still proceed in the same pass |
| 16.3 | If a gating task arrives while other non-gating tasks are running or ready, `rhei run` lets remaining non-gating work finish | ✅ | Implicit from the ready-set semantics — other tasks remain in `ready` and are scheduled normally |
| 16.4 | The run halts for human input only when remaining non-terminal tasks are all in gating states or blocked behind a gating dependency | 🟡 | The loop naturally halts when no advanceable tasks remain (ready set empty after gating filter). The implementation does not surface a specific "waiting for human input" message at this terminal condition; gating tasks are only logged inside the main pass via `crates/rhei-cli/src/main.rs:7898-7906` ("Task X is in gating state ... Waiting for human action."), and that branch is now unreachable in practice because gating tasks are already filtered from `find_ready_tasks`. The behavioural outcome matches the spec (run halts; gating tasks remain) but the dedicated end-of-run halt message described in §86–92 is absent. |

---

## 17. Dry Run (spec lines 94–102)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 17.1 | With `--dry-run`, the same scan and selection logic runs but no subprocess or callback executes | ✅ | Dry-run branches at `crates/rhei-cli/src/main.rs:7995-8002`, `8048-8067`, `8310-8336`, `8989-8997`, `9059-9065` |
| 17.2 | Each planned transition is printed in the form `would transition: Task <ID>  <from> -> <to>` | 🟡 | The literal output string is `"Would transition Task {} from '{}' to '{}'"` (capital-W, no leading colon, `from '..' to '..'` instead of `<from> -> <to>`). See `crates/rhei-cli/src/main.rs:7996-8001` (program/agent path) and `crates/rhei-cli/src/main.rs:8989-8995` (callback path). For agent dry-run, the message is `"\nWould spawn: {agent} (model: {model})"` at `crates/rhei-cli/src/main.rs:8340-8345`. The spec's exact rendering is not produced. |
| 17.3 | No file lock is acquired in dry-run | ✅ | Dry-run branches return before `execute_transition` is invoked |
| 17.4 | No markdown is rewritten in dry-run | ✅ | Same — `execute_transition` is the only writer and it is skipped |
| 17.5 | No runtime artifacts are created in dry-run | 🟡 | Markdown is not rewritten and subprocesses are not spawned, but the TUI frontend and `JournalSink` are still constructed (`crates/rhei-cli/src/main.rs:7803`) and may create `runtime/transitions.log` (empty), `runtime/` directory, etc. Strictly speaking, "no runtime artifacts are created" is partially violated by the frontend setup. The dashboard sink and journal sink are created before the dry-run check at `crates/rhei-cli/src/main.rs:7802-7810`. |

---

## 18. Parallel Execution — orchestrator behaviour (spec lines 104–112)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 18.1 | With `--parallel N`, up to N subprocesses run concurrently | ✅ | Parallel spawn loop at `crates/rhei-cli/src/main.rs:8572-8702` uses `std::thread::spawn` |
| 18.2 | Orchestrator assigns each spawn a slot index | ✅ | `slot_idx` at `crates/rhei-cli/src/main.rs:8575` and `slot` at `crates/rhei-cli/src/main.rs:8632` |
| 18.3 | One line is written to `runtime/transitions.log` per `SlotAssigned` | ✅ | `JournalSink::emit` at `crates/rhei-tui/src/journal.rs:78-89` writes the assignment line |
| 18.4 | One line is written to `runtime/transitions.log` per `SlotReleased` | ✅ | `crates/rhei-tui/src/journal.rs:90-120` writes the release line |
| 18.5 | Every state write is serialised through its own file lock — two agents completing at once cannot corrupt the plan | ✅ | `execute_transition` acquires `lock_exclusive()` on the metadata + task files at `crates/rhei-cli/src/main.rs:5065-5074` |
| 18.6 | Tasks whose transitions would race on the same task node are never scheduled in parallel: scheduling is driven by the ready set, which excludes tasks already in flight | 🟡 | Within a single pass, the same task never appears twice in `agent_tasks` (each ready task is processed once). Across passes the ready set is re-read after the previous pass joins all handles (`crates/rhei-cli/src/main.rs:8705-8801`), so by construction no two passes overlap on the same task. The spec phrasing "excludes tasks already in flight" suggests a stronger in-flight tracker; here it is implicit from the join-before-next-pass synchronisation rather than an explicit exclusion. Functionally correct. |
| 18.7 | Single-file plans force `--parallel 1` with a warning | ✅ (not in this spec, but referenced from `rhei-agents.spec.md`) | `crates/rhei-cli/src/main.rs:7747-7755` |

---

## 19. Polling States (spec lines 115–128)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 19.1 | Each poll attempt spawns one subprocess and the engine evaluates transitions | 🟡 | Subprocess spawning works for any state including polling states. Transition evaluation occurs via `find_next_transition`. But: |
| 19.2 | A self-loop transition means "retry after `poll.interval`" | ❌ | `find_next_transition` selects self-loop transitions but the orchestrator does not treat them specially. No "retry after interval" sleep / re-spawn cadence exists. The notion of a poll retry never reaches the runtime. |
| 19.3 | Between attempts, persist `metadata.tasks.<id>.pollNextAttemptAt.<state-name> = now() + interval` | ❌ | `pollNextAttemptAt` does not exist anywhere in `crates/rhei-cli/src/main.rs` (grep returns zero hits). |
| 19.4 | Between attempts, increment `metadata.tasks.<id>.stateVisits.<state-name>` | 🟡 | `stateVisits` is written generally for any transition (`update_metadata_for_transition` at `crates/rhei-cli/src/main.rs:4198-4216`), but it is incremented on the *destination* state of every transition, not specifically on poll attempts. For a self-loop poll attempt the increment would happen because `to_state == from_state`; however since the poll-loop semantics aren't implemented, this is incidental rather than intentional. |
| 19.5 | Between attempts, the `--parallel` slot is released so other ready tasks may run | ❌ | No slot-release-without-completion mechanism. A poll attempt either advances the task (slot released as normal) or doesn't (no specific handling). The orchestrator does not hold a slot across poll cycles. |
| 19.6 | The orchestrator does not hold a timer thread; the next pass re-scans and picks the task up only once `pollNextAttemptAt` is in the past | ❌ | No `pollNextAttemptAt` filter exists in `find_ready_tasks`. |
| 19.7 | If, at the end of a pass, every remaining non-terminal task is in a gating state, blocked behind a gating dependency, or blocked by pending `pollNextAttemptAt`, `rhei run` sleeps until the earliest `pollNextAttemptAt` (lower bound 1 s) | ❌ | No such sleep-until-earliest-deadline logic exists. The main loop simply breaks when `ready.is_empty()` or no advancement occurs (`crates/rhei-cli/src/main.rs:7863-7865, 8806-8808`). |
| 19.8 | Once `stateVisits.<state-name>` reaches `poll.max_attempts`, refuse self-loop transitions and pick the first matching non-self-loop instead | ❌ | `transition_rule_is_applicable` consults a generic `visits` cap (`loop_reentry_allowed`, `crates/rhei-cli/src/main.rs:3987-4004`) but does NOT distinguish self-loop vs non-self-loop nor key off `poll.max_attempts`. |
| 19.9 | If no non-self-loop transition matches at exhaustion, halt that task with a `polling exhausted with no matching non-self-loop transition` error; `--continue-on-error` applies | ❌ | No such error path exists. |
| 19.10 | A non-self-loop exit at any attempt clears both `pollNextAttemptAt.<state-name>` and `stateVisits.<state-name>` | ❌ | `clear_runtime_state_visits` at `crates/rhei-cli/src/main.rs:4218-4234` clears `stateVisits` only in specific reset contexts; nothing in the run loop ties this to a non-self-loop poll exit, and `pollNextAttemptAt` is not tracked at all. |
| 19.11 | `snapshot.inherit` is rejected on polling states in v1 | ❌ | No validator or runtime guard found. Implementation notes defer this to `impl-rhei-snapshots` / `impl-rhei-states`. |
| 19.12 | Snapshot emit (including auto-emit) is suppressed for self-loop attempts | ❌ | Vacuous — `emit_snapshots_after_transition_selection` is a no-op so nothing is emitted in either direction. The hook signature does receive `(current_state, selected_to_state)` which would let a future implementation detect self-loops. |
| 19.13 | Snapshot emit runs only on a terminal non-self-loop exit when the state is otherwise snapshot-capable | ❌ | Vacuous — see 19.12. |

---

## 20. Concurrent vs. Serial States (spec lines 131–137)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 20.1 | `concurrent: true` — any number of ready tasks in this state may be scheduled together (bounded by `--parallel`) | ✅ | `is_concurrent` check at `crates/rhei-cli/src/main.rs:8276-8281` adds matching entries unconditionally to `filtered` |
| 20.2 | `concurrent: false` (default) — at most one ready task in this state is scheduled per pass | ✅ | `state_claimant` map at `crates/rhei-cli/src/main.rs:8282-8291` enforces one-per-state |
| 20.3 | Additional non-concurrent tasks remain ready and are picked up on the next pass | ✅ | Deferred set tracked at `crates/rhei-cli/src/main.rs:8285, 8293-8302`; ready set is recomputed on each pass at `crates/rhei-cli/src/main.rs:7862` |
| 20.4 | The flag does not change state entry/exit semantics or transitions | ✅ | `concurrent` is consulted only at `crates/rhei-cli/src/main.rs:8276-8281` for scheduling, nowhere else |
| 20.5 | The flag does not affect within-task fanout (`all_targets` / `all_models`): every resolved invocation for a single scheduled task is still spawned together | ✅ | The fanout invocations are kept together by the entry-per-invocation push at `crates/rhei-cli/src/main.rs:7971-7978`; the same task may have multiple agent_tasks entries, and the state-claimant check at `crates/rhei-cli/src/main.rs:8282-8290` keeps them all (`claimant == &entry.0`) |

---

## 21. Relationship to other commands (spec lines 139–143)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 21.1 | `rhei run` is mutually exclusive per execution with the manual-worker flow (`next` / `transition` / `complete`) | 🟡 | No explicit cross-process lock prevents a user from invoking `rhei next` while `rhei run` is mid-pass. Mutual exclusion relies entirely on the file lock inside `execute_transition`, which serialises individual writes but does not block a `rhei next` invocation from claiming a task. The spec's stronger phrasing "they never overlap on the same task because `rhei run` holds transition responsibility" is enforced at the write boundary but not at the command-orchestration boundary. |

---

## 22. Related Specifications (spec lines 145–153)

| # | Requirement | Status | Evidence / Gap |
|---|---|---|---|
| 22.1 | Cross-references only | ⚪ | Not normative |

---

## Aggregate gap summary

**Counts (normative requirements only):**

- ✅ covered: 36
- 🟡 partial: 13
- ❌ missing: 18
- ⚪ not-normative: 4

**High-impact missing items (in spec-section order):**

1. **§5 Snapshot flag runtime behaviour (5.6, 5.7, 5.8):** flags parse but `preload_snapshot_inherit_before_spawn` is a no-op stub — `--from-snapshot` / `--override-inherit` have no effect at runtime.
2. **§9 Ready-set rules (9.4, 9.5):** ready set ignores `inputs:` presence and `pollNextAttemptAt`.
3. **§10 Snapshot inherit preload step (10.4) and polling-state inherit rejection (10.5):** call sites are wired but bodies are stubs / absent.
4. **§13 Snapshot emit between select and apply (13.1, 13.2, 13.3):** call site is wired but body is a stub.
5. **§15 Exit-code semantics (15.3):** `rhei run` exits 0 even when non-terminal tasks remain stuck.
6. **§17 Dry-run output format (17.2) and zero-artifact rule (17.5):** prints a differently-worded line; frontend/journal artifacts are created even in dry-run.
7. **§19 Polling-state runtime (19.2–19.13):** essentially the entire polling-state runtime is missing — no `pollNextAttemptAt` tracking, no inter-pass sleep, no exhaustion handling, no self-loop refusal at the cap, no clearing on exit, no snapshot-inherit rejection.

**Notes on auditing scope.** The implementation notes at `runtime/implement/impl-rhei-run-implementation-notes.md` correctly flag the snapshot preload, snapshot emit, and polling-state validator gaps as deferred to `impl-rhei-snapshots` / `impl-rhei-states`. The notes also claim the polling-state runtime (`pollNextAttemptAt`, sleep-until-earliest, exhaustion, clearing on non-self-loop exit) is "preexisting" — that claim could not be substantiated by grep or by reading `find_ready_tasks`, `transition_rule_is_applicable`, `evaluate_transition_condition`, `update_metadata_for_transition`, or the `run_agent_mode` main loop. The polling-state items above are reported as ❌ on the strength of those negative searches.
