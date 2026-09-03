# Changelog

## Unreleased

- **A supervising state in a Panta workspace resumes warm now, instead of
  rebuilding its context on every visit.** A state that pairs `snapshot.emit:`
  with `snapshot.inherit:` emitted into the project's cache but inherited from
  the execution root of the rhei that owns the ticket — the same directory only
  in a single-file workspace. In a Panta project, where a rhei is a Directory
  Workspace with a root of its own, every `inherit:` read a directory nothing
  had ever written to: the run logged `warning: no snapshot found for inherit:
  <name>; running cold` and carried on cold, so the most-visited state of a
  supervised plan paid for its context again each time and `supervisor_session:
  true` bought nothing (#174). Preload now resolves the cache against the
  project root, which is where emission already wrote it and where `rhei
  snapshot list|show|gc|continue` and orphan validation already read it, so
  nothing moved on disk and caches from earlier runs keep working. The snapshot
  session directory is unchanged, still under the owning rhei's
  `runtime/snapshot-sessions/`, so a narrowed `rhei reset` still sweeps it. A
  path-form `--from-snapshot` reference is now read relative to the project
  root too, the same root `rhei snapshot show` already reads it against.
  (PR #N)
- **A workspace can be named by the directory you are standing in: `.`, `./`,
  and its bare `index.rhei.md` all work now.** Every command that takes a plan
  path derives the rhei id from the last component of that path, and `.`, `./`,
  and a trailing `..` have no last component of their own — a bare
  `index.rhei.md` is reduced to `.` before it gets there. So the spelling an
  author reaches for first, from inside the workspace they are already in,
  was the one spelling that failed: `rhei list .` exited 1 with "invalid rhei
  path .", and so did `validate` and `next`, while the same workspace listed
  fine from its parent or by absolute path. A nameless path is now resolved
  before its name is read, so all four spellings name one rhei with one set of
  ids; paths that already carry a name are untouched, so a symlinked workspace
  keeps the id it has. The same resolution now decides which project encloses a
  target, so a member rhei addressed as `..` from its own `tasks/` loads through
  its project instead of alone, where its cross-rhei `**Prior:**` would be
  reported missing. And where a path genuinely names no id — it resolves to
  no usable directory name, the name it carries is not a valid id, or it is the
  reserved `basin` — the error now points at the path instead of sending the
  reader to check task metadata in a plan that is perfectly valid. (PR #171)
- **`rhei init` no longer writes outside the directory you gave it.** The
  agent-discovery note was anchored at the enclosing git repository root, so
  `rhei init <subdir>` inside somebody else's repository appended the note to
  that repository's tracked, hand-written `AGENTS.md` — a file the user never
  named, and one init announced on a line of its own rather than among the
  host files it said it had changed (#116). The walk-up could not tell a plans
  subdirectory of the repository the agent works in from a host that merely
  happens to sit inside an unrelated one. The note now always lands in the
  host directory, where it is reported like every other host change, and every
  path init writes is one it chose inside that directory — an enclosing
  repository's instruction file is read only to word the hint, never picked as
  a place to write, and takes bytes only where you symlinked a host file to it
  yourself. Where the host does sit inside a repository, init prints that hint
  naming the root's instruction file, so adding a pointer stays your decision.
  It stays quiet when that file already reads as carrying such a note: init
  will not judge whose note it is, so it leaves it where it is and removing it
  is yours to do. (PR #167)
- **The supervised-delivery supervisor is handed a cancel command that
  actually runs.** Its prompt, its plan notes and the template README all
  printed `rhei transition <id> --to cancelled --result "<why>"`, which clap
  rejects with exit 2 because `--from` is required — it is the compare-and-swap
  guard the command is built on, so inferring it from the task would give up
  the race protection the command exists for. Every cancel therefore cost a
  failed command and then a read of the child's task file to find the state to
  pass. The guidance now shows `--from <current-state>` and says where that
  state is already visible: in brackets beside the child in the supervisor's
  own task list, and in `rhei list --parent <id>`. An end-to-end guard fails if
  any built-in template prints a `rhei transition` invocation without `--from`.
  (PR #170)

- **A detached run's console log is appended to, so one run can no longer
  overwrite another's diagnostic in it.** The launcher opened
  `runtime/run.log` truncating and without `O_APPEND`, and the launch lock it
  relied on for safety only serialises launchers, not a live run's writes.
  Where the pre-check is blind — two launches sharing a root, so the run lock
  is what refuses the second — a launcher emptied the log a running child still
  held open, and that child's next write landed at its old offset, in the
  middle of the refusal the losing child had just rendered. The refusal reached
  the operator cut mid-path, naming a lock file that could not be found; on
  macOS CI it also flaked the run-lock end-to-end test that gates a release.
  The log is now emptied at launch and opened append-only, so two consoles
  interleave by lines and neither can destroy the other's bytes. (PR #163)

- **Every declaration is listed in its folder's index.** `grund check` warned
  that ten declarations were absent from their index README and that the warning
  becomes an error in grund 0.13.0; the entries are added now so the gate stays
  green across that release rather than failing every commit on the day it lands.
  (PR #162)

- **Main now carries a `-dev` version, so a build from main can no longer be
  mistaken for the release it is ahead of.** Between releases the checked-in
  version stayed at the last tag, and `rhei --version` on a binary built from
  main 44 commits past `v0.3.3` still said `0.3.3`. That is not cosmetic: a
  supervising fix, a live-run listing fix and a usage-measurement fix all sat
  merged and uninstalled for a day precisely because nothing on the machine
  could tell installed-from-main from installed-from-tag. Main now opens the
  next patch as `X.Y.Z-dev` the moment a release goes out, and
  `set-release-version.py` accepts and overwrites the suffix so the release path
  round-trips `0.3.4-dev` to `0.3.4` unchanged. (PR #161)

- **A poll state can now declare that it waits on a person, so a run parked on
  an author's reply stops reading as work in flight.** A poll and a gate are
  the two ways a Rhei workflow waits, and a workflow waiting for a reply could
  be neither: a gate must be moved by hand, and the reply arrives on its own,
  so the state had to resume itself — which made it a poll, indistinguishable
  from a CI watch on every surface. One approval poll held a concurrency slot
  for six and a half hours across 28 attempts that spent no model tokens,
  looking exactly like a running agent. `poll:` now takes an optional
  `waiting_on: <label>`, whose presence declares the wait as a person's turn
  and whose value names who. `rhei states` appends `waiting_on=<label>` to the
  `Poll:` line and carries it in `--json`; `rhei list` marks the row
  `(waiting on <label>)` and carries the same field additively in `--json`; the
  end-of-run summary and report put the ticket under **Waiting** with a calm
  marker instead of Attention, keep it out of `could not advance`, and give the
  ledger the label as its reason; and viz, the TUI, and the dashboard classify
  it as a pause and name the label in their detail panels. A claim or an
  unfinished prior still outranks the wait, because neither is something the
  person named can answer. Scheduling is untouched — same interval, same slot
  release, same attempt budget, same exit statuses — and every part of this is
  absent for a poll that does not declare the field. One thing does change for
  every machine: the durable report's transition ledger now explains a held
  descendant as `held by supervisor Task <P> (<state>)` instead of `stalled in
  non-terminal state <state>`, which is what the run's other surfaces already
  said about it.
  [§FS-rhei-states.2.5](functional-spec/rhei-states.spec.md#25-waiting-on-a-person)
  (PR #156)
- **A supervising visit that released nothing no longer strands the run.** An
  agent visit in an `execute_on:` state that exited `0` without moving its
  subtree or leaving it able to move used to fire the self-loop anyway, which
  released the subtree and spent the only edge that could ever wake the
  supervisor again — the descendants stayed blocked, no checkpoint could
  arrive, and every rerun invoked nothing while advising a rerun. `rhei run`
  now withholds that self-loop and holds the state exactly as it holds a failed
  visit: no transition, `phase: held`, checkpoints preserved, the visit
  unspent, and a warning naming the descendants left with nowhere to go, why
  each one is stuck — the file it waits for, or the `**Prior:**` that has not
  landed — and the one action that answers it. The test guards every agent-mode
  advance of that edge, including the one that spawns nothing because the
  state's declared `outputs:` are already on disk, so the leftovers of a held
  visit cannot release the subtree on the next run; a dry run reports the
  withheld edge rather than a transition it would not make. Because the engine
  withheld the edge rather than the worker failing to earn it, the visit's
  attempt budget is not charged, so every later `rhei run` visits the
  supervisor again and writing the missing file is enough to recover — no
  `rhei reset`. Within a run the ticket is re-visited only after something else
  advanced. A visit that moved a descendant, or left one able to move, releases
  as before, and conditioned exits are untouched; "able to move" now requires
  the descendant's declared `inputs:` in every case but a `gating:` state,
  which a human moves regardless. A supervisor a previous release already
  stranded gets a halt row that says so instead of "rerun to pick it up".
  [§FS-rhei-supervision.3.6](functional-spec/rhei-supervision.spec.md#36-empty-visits)
  (PR #155)
- **`rhei init` integration and end-to-end tests now contain repository
  discovery inside each test's unique temporary directory.** An unrelated `.git` marker above
  the test tree can no longer redirect agent-note writes into shared temporary
  state or make later init tests fail. (PR #153)
- **Accounting artifacts now have a published consumer contract.** Versioned,
  strict JSON Schemas for invocation, summary, usage, task, price-book, and
  `rhei cost --json` records ship with the crate and are embedded behind
  `rhei schema <schema-id>`; the bare command and `--list` enumerate the ids.
  Invocation records now carry elapsed `duration_ms` and the exact native agent
  session id when Claude Code, Codex, or Pi exposes one, while older v1 records
  without either optional field remain readable. Native transcript paths stay
  absent unless they can be derived without guessing.
  [§FS-rhei-cost-accounting.3.4](functional-spec/rhei-cost-accounting.spec.md#34-timing-and-agent-cli-session)
  [§FS-rhei-cost-accounting.8.1](functional-spec/rhei-cost-accounting.spec.md#81-published-accounting-schemas)
  (PR #154)
- **Measured agent runs can use a caller-supplied reproducible price book.**
  `rhei run ... --prices <PATH>` validates a local
  `rhei.accounting.prices.v1` book before starting agents, copies it into each
  participating accounting root, and uses its exact provider/model matches,
  currency, rates, and id for sequential and parallel invocation records.
  Omitting the flag retains the built-in book; missing matches remain
  explicitly unpriced, and `rhei cost` does not reprice old records. No price
  book is fetched over the network.
  [§FS-rhei-cost-accounting.5.1](functional-spec/rhei-cost-accounting.spec.md#51-price-book-selection)
  (PR #150)
- **Subprocess tests no longer trust an existing `target` binary as proof that
  it matches the checkout.** The shared E2E and integration helper now asks
  Cargo to verify or rebuild `rhei-cli` once per test-harness process before
  returning the executable path, so a stale binary cannot make a regression
  test pass against code other than the current sources. (PR #151)
- **`rhei runs` keeps a live recorded run visible when its lock pathname no
  longer names the inode the run holds.** Renaming or unlinking a held
  `.rhei/run.lock` and installing an unlocked replacement previously made both
  text and JSON listings report the machine idle even while the matching
  non-terminal descriptors and recorded process still owned the displaced
  lock. On Linux, liveness now verifies the process's stable start identity and
  its ownership of that exact recorded run lock when a free or missing pathname
  is inconclusive; mere pid reuse cannot resurrect a stale run. Registry and
  workspace descriptors must also agree on both id and pid. Terminal and
  superseded descriptors still win, and failed ownership inspection is
  reported as undecided. Before `rhei stop` signals on Linux, the same exact
  ownership proof is required even when the current lock pathname is held, so
  an unrelated lock holder cannot make a reused descriptor pid a valid signal
  target. The run descriptor and command output schemas are unchanged. (PR #147)
- **`rhei summary` prints a compact Markdown run summary, short enough to
  paste into a pull request body.** Nothing rhei printed could say what the
  agents actually did: `rhei render --format github` renders the plan with all
  its workspace boilerplate and task content, the per-run report is written
  only when `rhei run` ends and links local log paths that cannot leave the
  machine, and `rhei cost` gives the totals but no step list. The material was
  already durable under `runtime/accounting/invocations/`; nothing rendered it.
  `rhei summary [RHEI_PLAN_OR_WORKSPACE]` now writes three things to stdout: a
  lead line naming the resolved state machine, the invocation count, the
  distinct models, and the task tally — with `N in progress` appended, so a
  mid-run summary says it is one; one numbered entry per invocation record in
  `started_at` order, carrying the task, state, visit where a task has more
  than one, agent, `provider/model`, duration, and token counts only where the
  record measured them; and the aggregate accounting in the per-run report's
  table shape, replaced by a single line when nothing was measured. `--details`
  wraps the whole thing in one collapsed `<details>` block with the lead line
  as its `<summary>`. It writes no file, spawns nothing, carries no local path,
  and estimates nothing — a fact that was not recorded is omitted rather than
  guessed. A freshly instantiated workspace with no accounting directory
  summarizes to the lead line and exits 0. (PR #145)
- **The built-in `claude-code` profile now produces measurable usage
  accounting.** Ordinary one-shot launches request Claude's typed JSON result,
  extract its input, cache, and output token dimensions, preserve the result
  text in logs and live output, and roll the measured invocation into priced
  task and run totals. Existing stream-json intervention transport remains
  unchanged. [§FS-rhei-cost-accounting.4](functional-spec/rhei-cost-accounting.spec.md#4-extraction-flow) (PR #144)
- **`rhei run`'s completion condition resolves declared `outputs:` against the
  owning rhei's execution root, not the run-level workspace root.** In a Panta
  project whose member rhei sits below the directory `rhei run` was pointed
  at, the two roots differ. The agent's `RHEI_ROOT` and the transition-time
  check already used the owning rhei's root, so an agent that wrote its
  declared output exactly where it was told still stalled: the completion
  condition — both the post-exit check and the before-spawn skip check — kept
  looking under the wrong root, warned `required outputs are missing` for a
  file that existed, and spent the ticket's attempt budget re-spawning an
  agent whose work was already done. `rhei run` on a laid Panta workflow now
  transitions a state as soon as its declared outputs land, with no workaround
  needed. Single-rhei plans are unaffected: there the two roots already
  coincided. (PR #139)
- **Spec documents are measured under a size budget instead of being exempt
  from measurement.** `.agents/fissile.toml` kept `**/*.spec.md` in
  `[scan].exclude`, and §AR-source-file-size.1 put `.spec.md` outside the
  exception register outright, both on the reasoning that a spec is reached
  through grund declarations and read a section at a time rather than loaded as
  one undifferentiated file. The reasoning is right; the conclusion was not.
  Being read by citation argues for a larger budget, not for none — and with no
  budget the spec tree was never measured once. A `citable-spec` rule now covers
  `docs/**/*.md` and `**/*.spec.md` at 750 lines soft and 2000 hard: 750 is
  grund's own soft value and about three times the p95 of the foundation spec
  trees, and 2000 is the ceiling §AR-source-file-size.1 already declares for any
  hand-authored file. An `entrypoint-doc` rule covers `README.md`, `AGENTS.md`,
  `CLAUDE.md`, and `skills/**/*.md` at 250 and 500, because those are loaded
  whole into every session that starts from them. `docs/changelog.md` and
  `docs/changelog/` are excluded instead: an append-only release record has no
  split to ask for. Six documents now report a soft finding and nothing blocks;
  no exception was written to silence them. (PR #131)

## 2. [0.3.3] - 2026-08-31

- **`dir_template` can now name a per-working-directory session store.** A
  `FlatById` layout's `dir_template` may contain the placeholder
  `{cwd_dashed}`, which expands to this spawn's own canonicalized working
  directory with every character outside `[A-Za-z0-9-]` replaced by `-` — the
  convention Claude Code uses for its own per-project session directories —
  so a template like `~/.claude/projects/{cwd_dashed}` names the directory a
  supervised checkout actually writes into, instead of one literal path
  shared across every checkout. Literal templates and `~/` expansion are
  unaffected; an unrecognized `{name}` placeholder degrades to no
  fixed-location tracking rather than failing the spawn. (PR #129)
- **`snapshot.emit` no longer requires `session_dir_flag`.** The predicate
  gating both `rhei validate` and the runtime emit path demanded a
  session-directory redirect flag on top of a supported `SessionLayout`,
  contradicting the spec's "*Emit* requires only a `SessionLayout`" and
  making emit unavailable for any agent — including the built-in `claude-code`
  profile — that writes sessions to a location it derives itself, with no
  redirect flag. A `FlatById` layout's `dir_template` is now a valid
  alternative: with `assign_id_flag` declared, rhei assigns the session id at
  spawn and reads `<dir>/<id>.<ext>` after exit; otherwise it captures the
  newest matching transcript written no earlier than the spawn, so a leftover
  file from an earlier invocation is never mistaken for the current one. The
  `session_dir_flag` redirect path, including the built-in `pi` profile, is
  unchanged. (PR #127)
- **The e2e and integration suites move into grund's two non-citable test
  homes.** `grund.toml` declared a citable `E2E` kind at `e2e/cases` with the
  deprecated `prefix` key, and that directory held nothing but a `.gitkeep` —
  the real end-to-end suite lived at `crates/rhei-cli/tests/e2e/` and the
  markdown-plans integration suite at
  `crates/rhei-cli/tests/integration_markdown_plans/`, neither directed nor
  held to a citation obligation by grund. `prefix` also stops loading in grund
  0.13.0. Both suites move to workspace members grund recognizes by place —
  `tests/e2e/` (`rhei-e2e-tests`) and `tests/integration/`
  (`rhei-integration-tests`), each runnable alone via `cargo test -p
  <member>` — `grund.toml` renames every `prefix` to `kind` and adds
  `[citations.e2e]` / `[citations.integration]`, and CI's pinned `grund` moves
  from 0.9.0 to 0.12.3. No product behaviour changes. (PR #126)
- **A failed visit to an `execute_on` state no longer strands the run.** On a
  non-zero subprocess exit, `rhei run` selected the first transition with no
  `exit_code` field — for a supervising state, that is the release self-loop
  (or, when the subtree already looked closed, the `openDescendants < 1` edge
  to `completed`) — applying success semantics to a failure: the supervision
  block was released, its checkpoints dropped, and `stateVisits` bumped, so a
  rerun found nothing left to schedule. Exit-code routing now only ever
  selects a transition that declares `exit_code` — except a poll state's
  exhaustion edge, still selected once its attempt budget is spent even when
  it declares no `exit_code`, per its existing `pollAttempts >=
  pollMaxAttempts` routing. An unmatched non-zero exit otherwise fires no
  transition, so a failed visit leaves `phase: held`, its checkpoints, and
  `stateVisits` untouched, and a rerun re-spawns it. (PR #124)

## 3. Older releases

- [0.3.2](changelog/0.3.2.md) - 2026-08-30: - **The re-spawn note on a poll state names its own `poll.max_attempts` instead of an internal sentinel.** A poll state is exempt from the visit attempt budget — `poll.max_attempts` already bounds it — and that exemption was encoded internally as `u64::MAX`, which `rhei run` then printed verbatim: `attempt 4 of 18446744073709551615`.
- [0.3.1](changelog/0.3.1.md) - 2026-08-30: - **The release commit stages `xtask/Cargo.toml`.** `Auto bump` had failed on its last three runs, always at `release.yml`'s version check and always before the publish step, with `xtask/Cargo.toml internal dependency requirement is stale: ...
- [0.3.0](changelog/0.3.0.md) - 2026-08-23: - Give a cold invocation the project's **mid-term memory**.
- [0.2.0](changelog/0.2.0.md) - 2026-08-22: - Separate a run from the surface that watches it.
- [0.1.0](changelog/0.1.0.md) - 2026-05-21: - Initial alpha release line for the Rhei CLI, Rust crates, npm wrappers, and PyPI wrappers.
