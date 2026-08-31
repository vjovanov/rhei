# Changelog

## Unreleased

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
