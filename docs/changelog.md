# Changelog

## Unreleased

### Fixed

- **`rhei cost` and `rhei summary` read the accounting of the rhei they were
  pointed at.** Both resolved their accounting root one level above where a run
  laid into a Panta work root writes its records, so every spelling of a member
  — the directory, its `index.rhei.md`, `.` or `..` from inside it, or nothing
  at all — reported the project directory's records rather than the member's,
  and the project spelling reported only whatever the project directory
  happened to hold. A member now reads its own accounting root and the project
  reads the union of the run root and every rhei execution root, `basin`
  included, deduplicated by canonicalized path so a root two single-file rheis
  share is read once and each record counted once. Both commands take
  `--rhei <ID>` (repeatable), spelled as `rhei list`'s is, so one ticket's cost
  is reachable from the project spelling. An empty `rhei cost` keeps its first
  line and names the roots it searched beneath it, so an answer of zero is
  distinguishable from a miss, and `--json` carries a `roots` array on every
  reading. `rhei summary`'s task tally narrows with its records and never
  prints the roots line, because its output is publishable verbatim. A
  `rhei cost --task <ID>` naming a ticket the narrowed reading does not cover
  is now refused, naming the scope it read and the rhei the ticket belongs to,
  rather than answered with the empty totals of a ticket that genuinely cost
  nothing; an id no rhei holds anywhere is still reported as unknown. Where
  records are written is unchanged. (PR #219)

### Changed

- **A continued snapshot session lands under the rhei that owns the ticket.**
  `rhei snapshot continue` created its agent session directory under the
  project root, so in a Panta project one ticket's live transcript sat beside
  the shared snapshot cache and a narrowed `rhei reset --rhei <id>` never swept
  it. The session directory now resolves against the owning rhei's execution
  root, where `rhei run` already puts it, while the fixed-location
  `dir_template` and the locator that reads it back keep resolving against the
  directory the continuation's own agent runs in — two roots where one value
  used to stand for both. A single-file plan is unaffected, because the two
  roots are the same directory there. (PR #N)

- **A reading reports how long an invocation took, even when the record
  predates the field.** `rhei cost --json --task` now derives an invocation's
  elapsed time from `started_at` and `ended_at` when the stored `duration_ms`
  is absent, so records written before that field existed no longer publish
  wall-clock time as nothing — a 31-minute invocation read as zero. A record
  carrying its own duration is published with that number untouched, because it
  was measured in milliseconds while the agent ran while the endpoints are only
  accurate to the second. Nothing on disk is rewritten, `duration_ms` stays
  optional in `rhei.accounting.invocation.v1`, and `rhei summary` now shares
  the one derivation instead of keeping a second copy of it. (PR #215)

- **An end-to-end assertion no longer depends on where miette wrapped.**
  Captured stderr is not a tty, so every diagnostic is rendered wrapped at
  eighty columns, and a `contains` on a phrase that straddles the break fails
  on whichever machine pushes it over — a developer's, never CI's. The e2e
  harness now undoes the soft wrap at one seam, `stderr(&output)`, and every
  assertion reads stderr through it; the private normalizer in `next_tests.rs`
  and the whitespace-collapsing match in `assert_stderr_contains` are gone with
  it. Rejoining inserts the single space the wrap removed and never merges two
  rendered blocks, so a token broken mid-word and a phrase stitched out of a
  message and its help both stay visible as the faults they are. The inverse is
  close but not exact: a newline a message carries itself, falling where the
  line was already full, renders identically to a wrap and is joined, because
  miette splits a message at its own newlines before wrapping each piece. A
  test runs the two shipped diagnostics that meet this and records it as a
  known limit. The harness also clears `FORCE_COLOR` and `CLICOLOR_FORCE`
  before every spawn, so an ANSI gutter forced by the operator's shell cannot
  make the same assertions machine-dependent again. No product code changes.
  (PR #218)

- **A mangled agent note no longer takes user content with it.** `rhei init`
  paired the first of two begin markers with the *second* block's end marker,
  so anything a merge had stranded between them was read as note material and
  deleted without a word; and a marker-less `## Rhei` section ran to the end of
  the file, swallowing trailing user content that carried no heading of its
  own. Both boundaries are now written into the agent-note section of the init
  specification and pinned by tests, including one that combines a nested host
  with a malformed instruction file — the combination #187 reported and no
  case covered. (PR #220)

- The repository moved to the `agent-grounds` GitHub organization, along with
  `ephor`, `fissile` and `grund`, and every live reference now names it: the
  crate's `repository`, the four npm and Python package manifests, CI's
  `GRUND_REPOSITORY` clone URL, one program path and eleven test fixtures
  carrying a full `owner/repo` key. The crate, npm and PyPI names are unchanged,
  so nothing an installer names moves. `scripts/check-registry-names.sh` now
  accepts either owner, because a package already published carries the
  repository URL it was released with until its next release, and a pattern
  naming only the new owner reads those as names taken by a stranger. The
  generated `AGENTS.md` block is left alone: `grund init` writes it from grund's
  own template, so it corrects itself when that template ships. (PR #213)

- **Supervisor handoffs link broad project context instead of pasting it.** A
  task in a state that declares `execute_on` still receives its position,
  operative handoff memory, and direct navigation to its rhei and project, but
  no longer rereads their potentially repository-scale standing context merely
  to brief one selected step. Ordinary worker prompts retain both context
  blocks unchanged. (PR #211)

- **A retained headless lock no longer makes a dead supervisor look live.** On
  Linux, `rhei runs` now verifies that the recorded process owns a contended
  run lock, so an inherited lock held after that process exits is classified as
  ended while inconclusive ownership checks remain unknown. (PR #212)

- **`rhei next` can claim a task in a workspace that declares its own node
  kinds.** Claim mode re-reads the selected task's file under the lock before
  writing `**Assignee:**`, and that re-read now parses under the kinds the
  workspace index declares in `structure.nodeKinds` instead of the
  omitted-`structure` default of `Task` alone. A directory workspace whose
  kinds omit `task` selected its root and then failed to claim it with
  `task '<id>' not found in <task-file>`, which left the manual workflow
  usable only through `rhei run`. A basin ticket, which has no index of its
  own, was broken the same way and is fixed the same way: its kinds now come
  from the project manifest. (PR #214)

- **A test run given its own target directory rebuilds `rhei` there.** The E2E
  and integration harnesses derive the profile directory they check from the
  running test binary, but spawned their `cargo build -p rhei-cli` without it,
  so a run invoked with `--target-dir` built into the checkout's default
  `target/` and then panicked over the binary missing from the directory it was
  given. The nested build now carries `--target-dir` for that same directory,
  keeping the release profile it already followed, and a build that succeeds
  while leaving nothing behind now quotes the command it ran beside the path it
  checked. (PR #221)

## 2. [0.4.1] - 2026-09-07

- **Parallel refills preserve each task's requested execution identity.** When
  a freed `--parallel` slot schedules newly ready work, the reloaded task's full
  `**Target:**` override now continues to select its agent, mode, provider, and
  model instead of silently falling back to the state's target. (PR #208)
- **Every new issue arrives carrying its kind.** `.github/ISSUE_TEMPLATE/` adds
  four GitHub issue forms — bug report, feature request, usability report, and
  token or time waste — which apply `bug`, `enhancement`, `usability` and
  `tokens` as the issue is opened, and `config.yml` disables blank issues so
  nothing can be filed without a kind. Each form asks for the fields that make a
  report actionable: the context (command, directory, version), what happened,
  what was expected, an optional workaround, and, on the token form, the cost.
  (PR #196)
- **Run summaries show every cache token dimension without double-counting it.**
  The durable accounting strip, its Task Costs table, the TTY end-of-run strip,
  and `rhei summary` now place cache reads and cache writes beside explicitly
  inclusive input and output totals. Unsupported dimensions remain `-`, while
  a measured zero remains `0`; cache parts are not added again to the inclusive
  total. (PR #195)
- **CI pins grund 0.13.0.** `GRUND_VERSION` in `.github/workflows/ci.yml` moves
  from `0.12.3` to `0.13.0`; the gate-tools cache key names that variable, so it
  rekeys and builds the new binary rather than restoring the old one from
  cache. Grund 0.13.0 regenerates AGENTS.md's managed grounding block from v7
  to v8; `grund init` produced that diff and nothing else — the hand-written
  prose around the block and the `CLAUDE.md` symlink to `AGENTS.md` are
  untouched. `grund check` was otherwise already clean under the new rules.
  (PR #194)

## 3. Older releases

- [0.4.0](changelog/0.4.0.md) - 2026-09-05: - **The fissile config lives at `.agent-grounds/fissile.toml`, where an agent working in the repository can still reach it.** `.agents/` is where agent *instructions* live, and a managed permission profile mounts it read-only inside a checkout, so an agent that hit a size finding could not adjust a budget or record an exception without leaving its sandbox — tool config had ended up in the one directory it was least able to repair.
- [0.3.3](changelog/0.3.3.md) - 2026-08-31: - **`dir_template` can now name a per-working-directory session store.** A `FlatById` layout's `dir_template` may contain the placeholder `{cwd_dashed}`, which expands to this spawn's own canonicalized working directory with every character outside `[A-Za-z0-9-]` replaced by `-` — the convention Claude Code uses for its own per-project session directories — so a template like `~/.claude/projects/{cwd_dashed}` names the directory a supervised checkout actually writes into, instead of one literal path shared across every checkout.
- [0.3.2](changelog/0.3.2.md) - 2026-08-30: - **The re-spawn note on a poll state names its own `poll.max_attempts` instead of an internal sentinel.** A poll state is exempt from the visit attempt budget — `poll.max_attempts` already bounds it — and that exemption was encoded internally as `u64::MAX`, which `rhei run` then printed verbatim: `attempt 4 of 18446744073709551615`.
- [0.3.1](changelog/0.3.1.md) - 2026-08-30: - **The release commit stages `xtask/Cargo.toml`.** `Auto bump` had failed on its last three runs, always at `release.yml`'s version check and always before the publish step, with `xtask/Cargo.toml internal dependency requirement is stale: ...
- [0.3.0](changelog/0.3.0.md) - 2026-08-23: - Give a cold invocation the project's **mid-term memory**.
- [0.2.0](changelog/0.2.0.md) - 2026-08-22: - Separate a run from the surface that watches it.
- [0.1.0](changelog/0.1.0.md) - 2026-05-21: - Initial alpha release line for the Rhei CLI, Rust crates, npm wrappers, and PyPI wrappers.
