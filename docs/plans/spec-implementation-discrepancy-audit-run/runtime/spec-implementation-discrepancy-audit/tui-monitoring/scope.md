# Scope Inventory: `tui-monitoring`

Task partition: audit the `rhei run` monitoring surface: TUI, transition journal, stdout compatibility, agent traffic capture, slot lifecycle, log tailing, non-TTY behavior, `--tui` / `--no-tui`, and failure/timeout display.

Do not expand this partition into general `rhei run` scheduling, transition correctness, callback semantics, artifact validation, settings merge order, or agent registry resolution except where those behaviors directly feed monitoring events or user-visible run output.

## Normative Spec Files

- `docs/specs/rhei-run-tui.spec.md`
  - Full file belongs to this partition.
  - Key sections: Goals, Non-Goals, Architecture, Event Surface, Live Agent Traffic, Sink Implementations, Frontend Selection, Layout Rules, Journal Format, Failure Modes, Reuse, CLI Changes, Backward Compatibility, Implementation Surface, Dependencies.
- `docs/specs/rhei-run.spec.md`
  - Only TUI, journal, stdout, slot, timeout/failure display, and run-command flag references belong here.
  - Key sections: Options / Standalone, Execution Loop items 3, 6, and 7 only where they affect monitoring, Dry Run output format, Parallel Execution bullets about slot assignment and `runtime/transitions.log`, Related Specifications link to Run TUI.

## User-Facing Commands In Scope

- `rhei run <RHEI_PLAN_OR_WORKSPACE> [flags]`
  - `--parallel <N>`
  - `--dry-run`
  - `--continue-on-error`
  - `--agent`, `--agent-mode`, `--model`
  - `--no-agent`
  - `--no-program`, `--program-timeout <DURATION>`
  - `--tui`
  - `--no-tui`
- Shell observation command called out by spec: `tail -f runtime/transitions.log`.

## Claim Map

### A. Goals And Compatibility

Normative claims:

- `docs/specs/rhei-run-tui.spec.md:7-12`: interactive `rhei run --parallel N` must show up to N active agents with task id, current state, elapsed time, and log tail; every state transition must produce exactly one persistent journal line with transition plus detailed log path; non-TTY stdout must remain current line-oriented output; the event/frontend surface must be reusable.
- `docs/specs/rhei-run-tui.spec.md:14-18`: TUI is additive, not a replacement; agents keep per-task log files; visualization is local-terminal only.
- `docs/specs/rhei-run-tui.spec.md:173-177`: non-TTY output without `--tui` must match current line-oriented format byte-for-byte; `runtime/transitions.log` is additive and must not alter plan state; existing flags keep meaning.
- `docs/specs/rhei-run.spec.md:71-79`: `--dry-run` prints planned transitions and creates no runtime artifacts.

Implementation surfaces to compare:

- `crates/rhei-cli/src/main.rs:7286-7353` (`run_command`) selects agent/program vs callback mode.
- `crates/rhei-cli/src/main.rs:7355-8435` (`run_agent_mode`) is the main event-emitting path.
- `crates/rhei-cli/src/main.rs:8437-8578` (`run_callback_mode`) remains legacy callback stdout path and should be checked for whether TUI/journal requirements apply or are intentionally agent-mode only.
- `crates/rhei-tui/src/lib.rs:1-17` exposes reusable crate API.
- `crates/rhei-tui/src/stdout.rs:1-34` (`StdoutSink`) defines non-TTY frontend behavior.
- `crates/rhei-cli/tests/e2e/run_tests.rs` and `crates/rhei-cli/tests/integration_markdown_plans.rs` contain stdout compatibility assertions for `rhei run`.
- `crates/rhei-cli/tests/integration_markdown_plans.rs:2001` (`run_dry_run_shows_transitions_without_changes`) is the dry-run no-mutation coverage surface.

### B. Architecture, Event Surface, And Sink Composition

Normative claims:

- `docs/specs/rhei-run-tui.spec.md:20-33`: execution engine emits `RunEvent`s via `EventSink`; engine writes through `Tee` to `JournalSink` plus frontend; frontend is `StdoutSink` for non-TTY or `TuiSink` for TTY; slot-oriented events update one tile; engine assigns and releases slot indices.
- `docs/specs/rhei-run-tui.spec.md:35-64`: `RunEvent` variants must include run/pass lifecycle, `SlotAssigned`, `SlotReleased`, `AgentOutput`, and `RunFinished`; `AgentStream` identifies stdout/stderr; `TaskOutcome` includes completed, failed, cancelled, timed out; `EventSink` is `Send + Sync`; `Tee` forwards to fixed inner sinks.
- `docs/specs/rhei-run.spec.md:81-89`: with `--parallel N`, orchestrator assigns each spawn a slot index and writes one journal line per `SlotAssigned` and `SlotReleased`.

Implementation surfaces to compare:

- `crates/rhei-tui/src/event.rs:5-137`
  - `Slot`
  - `TaskOutcome`
  - `RunSummary`
  - `MessageLevel`
  - `AgentStream`
  - `RunEvent`
  - `EventSink`
  - `Tee`
  - `NullSink`
- `crates/rhei-tui/src/frontend.rs:39-80` (`select_frontend`) composes `JournalSink` and `StdoutSink`/`TuiSink`.
- `crates/rhei-cli/src/main.rs:7370-7386` emits `RunStarted`.
- `crates/rhei-cli/src/main.rs:7450-7453`, `7641`, `7831-7838`, `7908`, `8366`, `8423-8430` emit pass and finish events.
- `crates/rhei-cli/src/main.rs:7671-7713`, `7957-8011`, `8182-8260` emit slot lifecycle events for programs and agents.
- Unit tests: `crates/rhei-tui/src/journal.rs:177` (`writes_assigned_and_released_lines`), `crates/rhei-tui/src/journal.rs:216` (`appends_on_second_open`), `crates/rhei-tui/src/tui.rs:552` (`agent_output_is_added_to_slot_and_journal`), `crates/rhei-tui/src/tui.rs:568` (`agent_output_retention_is_bounded`), `crates/rhei-tui/src/tui.rs:612` (`slot_lines_reserve_rows_for_later_slots`).

### C. Live Agent Traffic Interception

Normative claims:

- `docs/specs/rhei-run-tui.spec.md:66-78`: built-in `claude-code`, `codex`, and `pi` intercept stdout/stderr through a shared capture path; prompt delivery differs by agent (`claude-code` and `pi` use `-p <prompt>`, `codex` uses stdin after `--` separator); capture is transport-agnostic after spawn; captured output is piped, logged, and emitted as line-oriented `AgentOutput`; lines are ordered per stream with best-effort inter-stream ordering; per-task log is durable and complete; TUI may drop display events but not log writes; rendering may sanitize/truncate display text while logs preserve raw bytes.

Implementation surfaces to compare:

- `crates/rhei-cli/src/main.rs:5579-5688` (`built_in_agents`) for built-in prompt transport and supported agent ids.
- `crates/rhei-cli/src/main.rs:6764-6823`
  - `with_agent_log`
  - `output_line`
  - `agent_stream_label`
  - `spawn_agent_output_reader`
- `crates/rhei-cli/src/main.rs:6825-6848` (`drain_agent_output_reader`) for pipe-drain behavior.
- `crates/rhei-cli/src/main.rs:6850-7012` (`spawn_and_wait_agent`) for stdout/stderr piping, log file creation, timeout, and footer.
- `crates/rhei-cli/src/main.rs:6914-6924` (`build_agent_command` call plus `stdout(Stdio::piped())` / `stderr(Stdio::piped())`) and `6950-6957` (`stdin_prompt` handling).
- `crates/rhei-tui/src/tui.rs:112-128` applies display sanitization and traffic/journal buffering for `AgentOutput`.
- `crates/rhei-tui/src/tui.rs:223-233` makes `AgentOutput` best-effort via `try_send` while lifecycle events use blocking send.
- Tests in `crates/rhei-cli/src/main.rs`:
  - `output_reader_logs_and_emits_complete_and_partial_lines`
  - `supported_agents_keep_expected_prompt_transports`
  - `fake_claude_profile_streams_prompt_flag_output`
  - `fake_codex_profile_streams_stdin_prompt_output`
  - `fake_pi_profile_streams_prompt_flag_output`
  - `fake_agent_timeout_keeps_output_and_writes_footer`
  - `inherited_output_pipe_does_not_block_agent_completion`
- Fixture-like helpers in those tests: `write_fake_agent`, `write_sleeping_fake_agent`, `write_inherited_pipe_fake_agent`.

### D. Sink Implementations And Frontend Selection

Normative claims:

- `docs/specs/rhei-run-tui.spec.md:80-84`: `JournalSink` opens `runtime/transitions.log` in append mode and writes one line per `SlotAssigned`/`SlotReleased` in every mode; `StdoutSink` reproduces current `println!` output exactly and is default when stdout is not a TTY; `TuiSink` owns bounded `crossbeam_channel` and render thread.
- `docs/specs/rhei-run-tui.spec.md:86-96`: frontend is decided once at `run_plan`/run entry; `--no-tui` or non-TTY selects `StdoutSink`; `--tui` forces `TuiSink`; default TTY selects `TuiSink`; auto-detection uses `std::io::IsTerminal`.
- `docs/specs/rhei-run-tui.spec.md:160-171`: `rhei run` has mutually exclusive `--tui` and `--no-tui`; neither changes existing flag semantics.
- `docs/specs/rhei-run.spec.md:13-27`: Standalone options include `--tui` and `--no-tui` with auto defaults.

Implementation surfaces to compare:

- `crates/rhei-cli/src/main.rs:5401-5420` (`StandaloneExecutionFlags`) for Clap flag definitions and mutual exclusion.
- `crates/rhei-cli/src/main.rs:5453-5484` (`RunOptions::frontend_kind`).
- `crates/rhei-cli/src/main.rs:7374-7380` calls `rhei_tui::select_frontend`.
- `crates/rhei-tui/src/frontend.rs:10-19` (`FrontendKind`).
- `crates/rhei-tui/src/frontend.rs:39-80` (`select_frontend`) for IsTerminal, fallback, journal composition.
- `crates/rhei-tui/src/stdout.rs:25-33` (`StdoutSink::emit`).
- `crates/rhei-tui/src/tui.rs:155-235` (`TuiSink`, channel behavior, drop/finish).
- `crates/rhei-cli/src/main.rs:11263-11284` parses run flags in unit tests; add/compare tests for `--tui`/`--no-tui` mutual exclusion if absent.

### E. TUI Layout, Slot Display, Journal Pane, Resize, And Log Tailing

Normative claims:

- `docs/specs/rhei-run-tui.spec.md:98-118`: renderer allocates a fixed pool of N slots matching `--parallel N`, reuses slots, and does not grow unbounded; layout rules are single full-width pane for N=1, 2x2 for N=2-4 with rows-per-tile >= 6, 3x3 for N=5-9 with rows-per-tile >= 6, compact list when rows-per-tile < 6 or N >= 10; persistent bottom journal pane always shows recent transitions; layout recomputes on `crossterm::event::Event::Resize`; each tile shows task id plus short title, current state from `SlotAssigned.to`, elapsed time updated once per second, and last 5 lines of log file at `log_path` tailed via `notify` with a bounded 50-line ring buffer; idle slots show `— idle —`.
- `docs/specs/rhei-run-tui.spec.md:143-144`: too-small terminals degrade to compact list and never crash; slow log growth tailer uses bounded 50-line ring buffer and never blocks the engine thread.

Implementation surfaces to compare:

- `crates/rhei-tui/src/tui.rs:24-28` channel/journal/traffic buffer constants.
- `crates/rhei-tui/src/tui.rs:29-52` (`SlotState`, `TrafficLine`, `UiState`).
- `crates/rhei-tui/src/tui.rs:55-63` fixed slot vector initialization.
- `crates/rhei-tui/src/tui.rs:73-151` event application to slots and journal.
- `crates/rhei-tui/src/tui.rs:237-296` `render_loop`, including input polling and redraw tick.
- `crates/rhei-tui/src/tui.rs:317-345` `draw`, including too-small terminal handling and pane layout.
- `crates/rhei-tui/src/tui.rs:348-361` `render_header`.
- `crates/rhei-tui/src/tui.rs:364-428` `render_slots` / `slot_lines`.
- `crates/rhei-tui/src/tui.rs:438-480` truncation and control-sequence sanitization.
- `crates/rhei-tui/src/tui.rs:483-494` `render_journal`.
- `crates/rhei-tui/src/tui.rs:505-514` `clone_snapshot`.
- `crates/rhei-tui/Cargo.toml:7-14` dependencies; `Cargo.toml` workspace membership.
- `crates/rhei-cli/Cargo.toml:20` has `notify`, but TUI crate dependency and log tail implementation need direct comparison.
- Unit tests in `crates/rhei-tui/src/tui.rs`: `agent_output_retention_is_bounded`, `unknown_slot_output_does_not_panic`, `sanitizes_control_sequences_for_display`, `truncates_with_ellipsis`, `slot_lines_reserve_rows_for_later_slots`.

### F. Transition Journal Format

Normative claims:

- `docs/specs/rhei-run-tui.spec.md:120-137`: `runtime/transitions.log` is UTF-8, append-only, newline-delimited; one line per event; columns are space-separated with columns 1-3 fixed-width, column 4 path, optional trailing comma-separated key=value metadata; timestamps are UTC RFC 3339 second precision; transition uses UTF-8 arrow `→`; paths are workspace-relative if inside workspace, else absolute; metadata only on `SlotReleased` (`exit`, `duration`, `outcome`); file is safe to `tail -f`; `SlotAssigned` writes one line and paired `SlotReleased` writes a second line on same state with exit status and duration; `all_targets`/multi-invocation states produce distinct pairs with target suffix visible in log path.
- `docs/specs/rhei-run.spec.md:83-87`: parallel run writes one line to `runtime/transitions.log` per assignment and release.

Implementation surfaces to compare:

- `crates/rhei-tui/src/journal.rs:24-37` (`JournalSink::open`) for append mode and directory creation.
- `crates/rhei-tui/src/journal.rs:39-55` (`write_line`) for warnings and non-aborting write/flush failures.
- `crates/rhei-tui/src/journal.rs:57-63` (`format_path`) for relative/absolute paths.
- `crates/rhei-tui/src/journal.rs:75-115` (`EventSink for JournalSink`) for assignment/release line format and metadata fields.
- `crates/rhei-tui/src/journal.rs:118-164` timestamp and duration formatting.
- `crates/rhei-cli/src/main.rs:6305-6311` (`resolved_agent_log_suffix`) for target/model suffixes.
- `crates/rhei-cli/src/main.rs:6742` (`agent_log_path`), `7014-7016` (`program_log_path`), and agent log path call sites at `7895-7900`, `7939-7944`, `8162-8167`.
- Tests: `crates/rhei-tui/src/journal.rs:177` and `216`; add/compare e2e coverage for `runtime/transitions.log` after `rhei run`, append behavior across invocations, and `tail -f` safety if absent.

### G. Failure, Timeout, Ctrl+C, And Lifecycle Preservation

Normative claims:

- `docs/specs/rhei-run-tui.spec.md:139-145`: `TuiSink` panic hook restores terminal before re-raising; Ctrl+C in raw mode restores terminal, re-raises `SIGINT`, then exits render loop; too-small terminal never crashes; slow log tailer is bounded and non-blocking; journal write errors warn to stderr and never abort.
- `docs/specs/rhei-run.spec.md:52-59`: subprocess timeout sends `SIGTERM`, waits 10 seconds, then `SIGKILL`; failed subprocesses route through error/timeout transition or abort depending on `--continue-on-error`; repeated passes finish with success only when all tasks terminal and nonzero if progress halts with non-terminal tasks.
- `docs/specs/rhei-run-tui.spec.md:35-64`: lifecycle events must be preserved enough that slot state remains accurate; `TaskOutcome::TimedOut` and `TaskOutcome::Failed(String)` must be distinguishable.

Implementation surfaces to compare:

- `crates/rhei-tui/src/tui.rs:174-201` (`TuiSink::start`) raw mode, alternate screen, panic hook.
- `crates/rhei-tui/src/tui.rs:203-219` `finish`/`Drop`.
- `crates/rhei-tui/src/tui.rs:223-233` preserves lifecycle events using blocking send.
- `crates/rhei-tui/src/tui.rs:270-286`, `298-315` Ctrl+C handling, terminal restore, and `SIGINT` forwarding.
- `crates/rhei-tui/src/tui.rs:317-345` too-small terminal branch.
- `crates/rhei-cli/src/main.rs:6961-6991` agent timeout and termination.
- `crates/rhei-cli/src/main.rs:7162-7189` program timeout and termination.
- `crates/rhei-cli/src/main.rs:7687-7701`, `7985-7999`, `8233-8248` outcome mapping.
- `crates/rhei-cli/src/main.rs:7734-7762`, `8030-8113`, `8579-8626` timeout transition handling.
- `crates/rhei-cli/src/main.rs:7803-7823`, `8096-8129`, `8341-8360` failure display and `--continue-on-error` behavior.
- Tests: `crates/rhei-tui/src/tui.rs:529` (`ctrl_c_requests_sigint_forwarding`), `crates/rhei-cli/src/main.rs:12237` (`fake_agent_timeout_keeps_output_and_writes_footer`), `crates/rhei-cli/src/main.rs:12287` (`inherited_output_pipe_does_not_block_agent_completion`).

### H. Reuse And Dependency Boundaries

Normative claims:

- `docs/specs/rhei-run-tui.spec.md:147-158`: `rhei-tui` is standalone with no dependency on `rhei-cli`; future parallel subcommands can construct a helper like `rhei_tui::run_with_frontend(...)`; `rhei-cli` depends on `rhei-tui` event types/helper and should not directly see `ratatui` or `crossterm`.
- `docs/specs/rhei-run-tui.spec.md:183-191`: `rhei-tui` depends on `ratatui`, `crossterm`, and `crossbeam-channel`; `notify` is reused for log tailing.

Implementation surfaces to compare:

- `Cargo.toml:1-10` workspace members include `crates/rhei-tui`.
- `crates/rhei-tui/Cargo.toml:7-14` direct TUI dependencies.
- `crates/rhei-cli/Cargo.toml` for `rhei-tui` dependency and direct terminal/TUI dependencies.
- `crates/rhei-tui/src/lib.rs:11-17` public API.
- Search targets during comparison: `run_with_frontend`, `RunParams`, `ratatui`, `crossterm`, `notify`, `rhei_tui::select_frontend`.

## Tests, Fixtures, Templates, And Skills To Include

Primary Rust tests:

- `crates/rhei-tui/src/journal.rs` unit tests:
  - `writes_assigned_and_released_lines`
  - `appends_on_second_open`
- `crates/rhei-tui/src/tui.rs` unit tests:
  - `ctrl_c_requests_sigint_forwarding`
  - `non_ctrl_c_input_is_ignored`
  - `agent_output_is_added_to_slot_and_journal`
  - `agent_output_retention_is_bounded`
  - `unknown_slot_output_does_not_panic`
  - `sanitizes_control_sequences_for_display`
  - `truncates_with_ellipsis`
  - `slot_lines_reserve_rows_for_later_slots`
- `crates/rhei-cli/src/main.rs` internal tests around agent output capture and prompt transport:
  - `output_reader_logs_and_emits_complete_and_partial_lines`
  - `supported_agents_keep_expected_prompt_transports`
  - `fake_claude_profile_streams_prompt_flag_output`
  - `fake_codex_profile_streams_stdin_prompt_output`
  - `fake_pi_profile_streams_prompt_flag_output`
  - `fake_agent_timeout_keeps_output_and_writes_footer`
  - `inherited_output_pipe_does_not_block_agent_completion`
- `crates/rhei-cli/tests/e2e/run_tests.rs`
  - Existing stdout and end-to-end run assertions, including program states, human-review barrier, model-declared missing-agent behavior, and fixture runs.
- `crates/rhei-cli/tests/integration_markdown_plans.rs`
  - Legacy/integration stdout behavior for `rhei run`, including dry-run and workspace run cases.

Fixtures and examples:

- `crates/rhei-cli/tests/e2e/fixtures/bash-agent-team/`
  - `team-states.yaml`
  - `workflow.sh`
  - `runtime/logs/team.log` assertions in tests
- `crates/rhei-cli/tests/e2e/fixtures/living-review-loop/`
  - `team-states.yaml`
  - `workflow.sh`
- `examples/changeset-review-example/states.yaml`
  - `all_targets` fanout and target suffix surface for journal/log naming.
- `examples/living-review-loop/team-states.yaml`
  - `all_models` fanout surface.
- `examples/ci-heal/states.yaml`
  - program-state monitoring surface.
- `examples/spec-implementation-discrepancy-audit-example/`
  - generated example of this audit workflow, including `tasks/tui-monitoring.md`.

Templates:

- `.agents/rhei/templates/spec-implementation-discrepancy-audit/tasks/tui-monitoring.md`
  - Source partition definition.
- `.agents/rhei/templates/spec-implementation-discrepancy-audit/states.yaml:56-73`
  - `scope-spec` output contract for this file.
- `.agents/rhei/templates/changeset-review/states.yaml`
  - `all_targets` fanout target suffix surface.
- `.agents/rhei/templates/multi-model-analysis/states.yaml`
  - `all_targets`/target execution surface for parallel runs.
- `.agents/rhei/templates/hourly-human-intervention/states.yaml`
  - broad target-driven run surface.

Skills:

- `skills/rhei-plan-worker/SKILL.md`
  - Relevant only for contrasting manual-worker flow with `rhei run`; note its statement that under `rhei run` the orchestrator spawns a subprocess and performs transitions after subprocess exit.
- `skills/rhei-state-machine-writer/SKILL.md`
  - Relevant to state-machine authoring guidance for `all_models`, `agent`, `target`, `condition`, `exit_code`, and timeout fields consumed by `rhei run`.
- `skills/rhei-template-writer/SKILL.md`
  - Relevant to template smoke guidance using `rhei run <workspace> --dry-run` and bundled `.rhei/settings.json` / timeout expectations.

## Comparison Checklist For Next State

- Confirm whether `run_callback_mode` is intentionally out of TUI/journal scope or whether the spec requires journal/frontend behavior for callback-only `rhei run` too.
- Compare spec event shape to actual `RunEvent`; note intentional additions like `Message`, `agent`, `wall_clock`, `exit_code`, and `duration_ms`, and type differences like `u8` vs `u16`.
- Verify `StdoutSink` plus `run_message!` actually preserves byte-for-byte non-TTY output and does not redirect warnings/errors differently than prior stdout behavior.
- Verify `--tui` and `--no-tui` mutual exclusion and auto-detection behavior with CLI tests or command probes.
- Verify `JournalSink` line format against fixed-column/tail-friendly claim, metadata rules, timestamp precision, relative paths, append behavior, and actual `rhei run` integration.
- Verify all `SlotAssigned` events have paired `SlotReleased` events for agents, programs, parallel branches, failures, panics, and timeouts.
- Verify `TaskOutcome::TimedOut` is not inferred merely from any nonzero exit when a timeout was configured.
- Verify live agent traffic capture is shared after command construction and preserves raw log bytes while sanitizing only display text.
- Verify program subprocess output is intentionally not `AgentOutput`, or mark as ambiguous if spec's event surface says any worker pool/subprocess should use the same frontend.
- Verify layout rules exactly: grid dimensions, compact list threshold, resize handling, idle slot text, task title display, elapsed-time tick, bottom journal pane, and log tail from `log_path` via `notify`.
- Verify lifecycle events are never dropped under channel pressure, while `AgentOutput` may be dropped only for display.
- Verify terminal restore behavior on panic, normal drop, and Ctrl+C, including whether `ratatui::restore()` specifically exists/is used or equivalent crossterm cleanup is acceptable.
- Verify dependency boundary: `rhei-cli` should not import `ratatui`/`crossterm` directly; `rhei-tui` should not depend on `rhei-cli`.
