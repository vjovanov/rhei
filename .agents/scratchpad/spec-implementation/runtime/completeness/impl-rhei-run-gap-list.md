# Aggregated Completeness Gaps: impl-rhei-run

Spec: `docs/functional-spec/rhei-run.spec.md`

Inputs:

- `runtime/completeness/impl-rhei-run-gaps-claude-code-yolo-anthropic-claude-opus-4-7.md`
- `runtime/completeness/impl-rhei-run-gaps-codex-xhigh-openai-gpt-5-5.md`

Aggregation rule: retained items marked `missing` or `partial` by any reviewer unless all reviewers marked the same requirement covered with concrete evidence. Where reviewers disagree, the disagreement is preserved for `completeness-fix`.

## Framing And Usage

- `partial`: the broad end-to-end `rhei run` contract remains incomplete until the concrete readiness, polling, snapshot, parallelism, and exit-status gaps below are fixed. One reviewer marked the overview covered, while another marked it partial based on those downstream gaps. Evidence: `docs/functional-spec/rhei-run.spec.md:3`, `crates/rhei-cli/src/main.rs:8313`, `crates/rhei-cli/src/main.rs:8357`, `crates/rhei-cli/src/main.rs:8711`, `crates/rhei-cli/src/main.rs:9048`, `crates/rhei-cli/src/main.rs:10309`.

## Options

- `partial` / disputed: `--dry-run` exists and skips many actions, but it does not consistently print the spec's planned-transition output and may still create runtime artifacts through frontend/journal setup. Evidence: `docs/functional-spec/rhei-run.spec.md:21`, `docs/functional-spec/rhei-run.spec.md:96`, `docs/functional-spec/rhei-run.spec.md:98`, `docs/functional-spec/rhei-run.spec.md:102`, `crates/rhei-cli/src/main.rs:8598`, `crates/rhei-cli/src/main.rs:8661`, `crates/rhei-cli/src/main.rs:8943`, `crates/rhei-cli/src/main.rs:9695`, `crates/rhei-tui/src/frontend.rs:51`, `crates/rhei-tui/src/journal.rs:24`.

- `partial` / disputed: `--parallel <N>` is parsed and agent batching honors it, including `0 = unlimited`, but program states appear to execute sequentially rather than running up to N agents or programs concurrently. One reviewer marked the parallel behavior covered; the other cited the sequential program loop. Evidence: `docs/functional-spec/rhei-run.spec.md:24`, `docs/functional-spec/rhei-run.spec.md:61`, `docs/functional-spec/rhei-run.spec.md:106`, `crates/rhei-cli/src/main.rs:8672`, `crates/rhei-cli/src/main.rs:8910`, `crates/rhei-cli/src/main.rs:9218`.

- `partial` / `missing`: snapshot override flags parse, but runtime behavior is effectively a no-op. `--from-snapshot` does not override/preload the authored `snapshot.inherit:` source, `--override-inherit` does not bypass source-selection or compatibility constraints, no runtime guard requires the target state to declare `snapshot.inherit:`, and `--task` / `--target` do not resolve ambiguous overrides. Evidence: `docs/functional-spec/rhei-run.spec.md:41`, `docs/functional-spec/rhei-run.spec.md:42`, `docs/functional-spec/rhei-run.spec.md:43`, `docs/functional-spec/rhei-run.spec.md:44`, `crates/rhei-cli/src/main.rs:5607`, `crates/rhei-cli/src/main.rs:5613`, `crates/rhei-cli/src/main.rs:5615`, `crates/rhei-cli/src/main.rs:5618`, `crates/rhei-cli/src/main.rs:10375`, `crates/rhei-cli/src/main.rs:10380`, `crates/rhei-cli/src/main.rs:10383`.

## Execution Loop

- `partial` / disputed: callback-only advancement is not selected globally when spawning is disabled with `--no-agent` and/or `--no-program`; those flags are applied per state inside agent mode. Reviewers considered the behavior functionally close, but not literal to the mode-selection contract. Evidence: `docs/functional-spec/rhei-run.spec.md:57`, `crates/rhei-cli/src/main.rs:8357`, `crates/rhei-cli/src/main.rs:8495`, `crates/rhei-cli/src/main.rs:8507`.

- `missing`: the ready set does not require the current state's required `inputs:` artifacts to exist before scheduling. Inputs are checked elsewhere during transition entry, so tasks can be spawned before satisfying ready-set input eligibility. Evidence: `docs/functional-spec/rhei-run.spec.md:60`, `crates/rhei-cli/src/main.rs:5412`, `crates/rhei-cli/src/main.rs:10043`, `crates/rhei-cli/src/main.rs:10061`, `crates/rhei-cli/src/main.rs:10069`.

- `missing`: the ready set does not exclude polling states whose `metadata.tasks.<id>.pollNextAttemptAt.<state-name>` is in the future. No `pollNextAttemptAt` runtime path was found. Evidence: `docs/functional-spec/rhei-run.spec.md:60`, `crates/rhei-cli/src/main.rs:4198`, `crates/rhei-cli/src/main.rs:10043`.

- `partial`: `snapshot.inherit:` preload is called before agent spawn, but the hook discards its arguments and does not resolve, check compatibility, override, or stage a source snapshot. Evidence: `docs/functional-spec/rhei-run.spec.md:63`, `crates/rhei-cli/src/main.rs:9022`, `crates/rhei-cli/src/main.rs:9274`, `crates/rhei-cli/src/main.rs:10368`, `crates/rhei-cli/src/main.rs:10375`.

- `missing`: polling states do not reject `snapshot.inherit` in v1. Reviewers found no validator or runtime guard tying `poll:` to snapshot inheritance rejection. Evidence: `docs/functional-spec/rhei-run.spec.md:63`, `docs/functional-spec/rhei-run.spec.md:126`, `crates/rhei-validator/src/lib.rs:485`, `crates/rhei-validator/src/lib.rs:948`.

- `partial`: completion condition handling is incomplete for missing outputs. One reviewer marked output gating covered because the task stays in its current state; another found missing outputs only warn/stall and do not route through the state's error transition or abort when no error transition exists. Evidence: `docs/functional-spec/rhei-run.spec.md:66`, `docs/functional-spec/rhei-run.spec.md:69`, `docs/functional-spec/rhei-run.spec.md:72`, `crates/rhei-cli/src/main.rs:6987`, `crates/rhei-cli/src/main.rs:9110`, `crates/rhei-cli/src/main.rs:9148`, `crates/rhei-cli/src/main.rs:9578`.

- `partial`: successful transition selection mostly selects before applying, but program exit-code transitions can error on multiple matching rules instead of selecting the first declared matching rule. Evidence: `docs/functional-spec/rhei-run.spec.md:67`, `docs/functional-spec/rhei-run.spec.md:69`, `crates/rhei-cli/src/main.rs:8238`, `crates/rhei-cli/src/main.rs:8265`, `crates/rhei-cli/src/main.rs:10224`, `crates/rhei-cli/src/main.rs:10284`.

- `partial`: non-zero or failed completion does not fully route through the state's error or timeout transition. Program `exit_code` / `nonzero` and timeout paths exist, but agent non-timeout non-zero exits do not look for a generic error transition, and missing outputs stall. Evidence: `docs/functional-spec/rhei-run.spec.md:69`, `docs/functional-spec/rhei-run.spec.md:71`, `crates/rhei-cli/src/main.rs:8279`, `crates/rhei-cli/src/main.rs:9148`, `crates/rhei-cli/src/main.rs:9193`, `crates/rhei-cli/src/main.rs:9940`.

- `partial`: when no error transition is declared and `--continue-on-error` is unset, agent/program non-zero exits abort, but missing-output failures may only warn/stall and later return success. Evidence: `docs/functional-spec/rhei-run.spec.md:72`, `docs/functional-spec/rhei-run.spec.md:73`, `crates/rhei-cli/src/main.rs:8837`, `crates/rhei-cli/src/main.rs:9148`, `crates/rhei-cli/src/main.rs:9199`, `crates/rhei-cli/src/main.rs:9488`, `crates/rhei-cli/src/main.rs:9578`.

- `partial`: snapshot emission is ordered correctly between transition selection and application, but `emit_snapshots_after_transition_selection` is a no-op and writes no auto `_state` snapshots or named `snapshot.emit:` outputs. Evidence: `docs/functional-spec/rhei-run.spec.md:74`, `docs/functional-spec/rhei-run.spec.md:79`, `crates/rhei-cli/src/main.rs:10284`, `crates/rhei-cli/src/main.rs:10309`, `crates/rhei-cli/src/main.rs:10337`.

- `missing`: poll self-loop snapshot suppression and terminal poll-exit emission are not implemented. The emit hook receives current and selected states but discards them. Evidence: `docs/functional-spec/rhei-run.spec.md:76`, `docs/functional-spec/rhei-run.spec.md:78`, `docs/functional-spec/rhei-run.spec.md:126`, `docs/functional-spec/rhei-run.spec.md:128`, `crates/rhei-cli/src/main.rs:10337`, `crates/rhei-cli/src/main.rs:10343`.

- `partial` / disputed: the subprocess prohibition on calling `rhei transition` or `rhei complete` is only partially represented. The agent prompt includes the prohibition and the orchestrator applies transitions, but program subprocesses do not receive a comparable instruction and there is no runtime guard against external CLI calls during execution. Another reviewer treated the subprocess side as a contract not enforceable by the orchestrator. Evidence: `docs/functional-spec/rhei-run.spec.md:80`, `docs/functional-spec/rhei-run.spec.md:81`, `crates/rhei-cli/src/main.rs:7300`, `crates/rhei-cli/src/main.rs:10309`.

- `missing`: run modes return success when progress halts even if non-terminal tasks remain and no further advancement is possible. The spec requires a non-zero exit in this stuck state. Evidence: `docs/functional-spec/rhei-run.spec.md:82`, `crates/rhei-cli/src/main.rs:9522`, `crates/rhei-cli/src/main.rs:9578`, `crates/rhei-cli/src/main.rs:9808`.

## Gating

- `partial` / disputed: gating states are behaviorally excluded from autonomous transition, but one reviewer found the dedicated waiting-for-human message path absent or unreachable because gating tasks are filtered from `find_ready_tasks`. Another reviewer marked the gating barrier semantics covered. Evidence: `docs/functional-spec/rhei-run.spec.md:84`, `docs/functional-spec/rhei-run.spec.md:86`, `docs/functional-spec/rhei-run.spec.md:92`, `crates/rhei-cli/src/main.rs:7898`, `crates/rhei-cli/src/main.rs:7906`, `crates/rhei-cli/src/main.rs:10061`.

## Dry Run

- `partial`: `--dry-run` does not always perform the same transition selection presentation required by the spec. Callback dry-run prints transitions, but agent/program dry-run can print spawn plans instead of planned transition lines. Evidence: `docs/functional-spec/rhei-run.spec.md:96`, `crates/rhei-cli/src/main.rs:8595`, `crates/rhei-cli/src/main.rs:8598`, `crates/rhei-cli/src/main.rs:8661`, `crates/rhei-cli/src/main.rs:8943`, `crates/rhei-cli/src/main.rs:9688`.

- `missing`: dry-run output does not match the exact required format `would transition: Task <ID>  <from> -> <to>`. Implemented strings use `Would transition Task ... from ... to ...` and `Would spawn ...`. Evidence: `docs/functional-spec/rhei-run.spec.md:98`, `docs/functional-spec/rhei-run.spec.md:100`, `crates/rhei-cli/src/main.rs:8598`, `crates/rhei-cli/src/main.rs:8661`, `crates/rhei-cli/src/main.rs:8943`, `crates/rhei-cli/src/main.rs:9695`.

- `missing`: dry-run may create runtime artifacts because frontend and journal setup happen before dry-run branches and can create `runtime/` and `runtime/transitions.log`. Evidence: `docs/functional-spec/rhei-run.spec.md:102`, `crates/rhei-cli/src/main.rs:8383`, `crates/rhei-cli/src/main.rs:9594`, `crates/rhei-tui/src/frontend.rs:51`, `crates/rhei-tui/src/journal.rs:24`.

## Parallel Execution

- `partial` / disputed: up to N concurrent subprocesses is implemented for agents, but program subprocesses appear sequential. This duplicates the `--parallel` option concern at the parallel behavior level. Evidence: `docs/functional-spec/rhei-run.spec.md:106`, `crates/rhei-cli/src/main.rs:8672`, `crates/rhei-cli/src/main.rs:9218`.

- `partial` / disputed: tasks that would race on the same task node are not scheduled across overlapping passes because the run joins a batch before the next scan, but there is no explicit in-flight marker in the ready set and `rhei run` does not claim tasks while subprocesses are running. Reviewers treated this as either functionally correct or only partially matching the "ready set excludes tasks already in flight" wording. Evidence: `docs/functional-spec/rhei-run.spec.md:112`, `crates/rhei-cli/src/main.rs:9361`, `crates/rhei-cli/src/main.rs:10043`.

## Polling States

- `partial`: polling states can spawn through the normal state execution path, but the time-triggered attempt lifecycle is not implemented. Evidence: `docs/functional-spec/rhei-run.spec.md:116`, `crates/rhei-validator/src/lib.rs:511`.

- `missing`: a self-loop from a poll state is treated like an ordinary transition rather than "retry after `poll.interval`". Evidence: `docs/functional-spec/rhei-run.spec.md:116`, `crates/rhei-cli/src/main.rs:10224`.

- `missing`: between poll attempts, the orchestrator does not persist `metadata.tasks.<id>.pollNextAttemptAt.<state-name> = now() + interval`. Evidence: `docs/functional-spec/rhei-run.spec.md:118`, `crates/rhei-cli/src/main.rs:4198`.

- `missing` / disputed: between poll attempts, `metadata.tasks.<id>.stateVisits.<state-name>` is not reliably maintained as the poll attempt counter. One reviewer noted generic state visit metadata may increment on self-loop transitions; another found `poll.max_attempts` is ignored by the visit-limit path, so polling attempts are not intentionally counted. Evidence: `docs/functional-spec/rhei-run.spec.md:118`, `crates/rhei-cli/src/main.rs:3873`, `crates/rhei-cli/src/main.rs:4198`.

- `missing`: the runtime has no distinct poll slot-release lifecycle between attempts. Evidence: `docs/functional-spec/rhei-run.spec.md:119`, `crates/rhei-cli/src/main.rs:9081`, `crates/rhei-cli/src/main.rs:9344`.

- `missing`: the orchestrator does not implement the "no timer thread; re-scan only after `pollNextAttemptAt` is in the past" rule because the ready-set scan has no poll-deadline filter. Evidence: `docs/functional-spec/rhei-run.spec.md:120`, `crates/rhei-cli/src/main.rs:8444`, `crates/rhei-cli/src/main.rs:10043`.

- `missing`: when all remaining non-terminal tasks are gating, gating-blocked, or poll-blocked, `rhei run` does not sleep until the earliest poll deadline. The loops break immediately when no ready work is found. Evidence: `docs/functional-spec/rhei-run.spec.md:122`, `crates/rhei-cli/src/main.rs:8444`, `crates/rhei-cli/src/main.rs:9647`.

- `missing`: once poll attempts reach `poll.max_attempts`, the engine does not refuse self-loops and choose the first matching non-self-loop. Existing self-loop visit checks use generic visit limits rather than `poll.max_attempts`. Evidence: `docs/functional-spec/rhei-run.spec.md:124`, `crates/rhei-cli/src/main.rs:3873`, `crates/rhei-cli/src/main.rs:4077`.

- `missing`: if poll attempts are exhausted and no non-self-loop transition matches, there is no `polling exhausted with no matching non-self-loop transition` error path and no `--continue-on-error` handling for it. Evidence: `docs/functional-spec/rhei-run.spec.md:124`, `crates/rhei-cli/src/main.rs:4069`.

- `missing`: a non-self-loop poll exit does not clear both `pollNextAttemptAt.<state-name>` and `stateVisits.<state-name>`. `pollNextAttemptAt` is absent, and visit clearing is not tied to non-self-loop poll exits. Evidence: `docs/functional-spec/rhei-run.spec.md:124`, `crates/rhei-cli/src/main.rs:4218`, `crates/rhei-cli/src/main.rs:4234`, `crates/rhei-cli/src/main.rs:5388`.

- `missing`: `snapshot.inherit` rejection for polling states and polling snapshot emit rules are absent. This is also listed under the execution-loop snapshot gaps because both sections state the requirement. Evidence: `docs/functional-spec/rhei-run.spec.md:126`, `docs/functional-spec/rhei-run.spec.md:128`, `crates/rhei-validator/src/lib.rs:485`, `crates/rhei-validator/src/lib.rs:948`, `crates/rhei-cli/src/main.rs:10337`.

## Relationship To Other Commands

- `partial`: `rhei run` is not fully mutually exclusive with the manual-worker flow during execution. Final state writes are locked and compare-and-swap protected, but `rhei run` does not claim or mark tasks in-flight while subprocesses run, and `find_ready_tasks` ignores assignee/in-flight state. Evidence: `docs/functional-spec/rhei-run.spec.md:141`, `crates/rhei-cli/src/main.rs:5133`, `crates/rhei-cli/src/main.rs:5179`, `crates/rhei-cli/src/main.rs:10043`.
