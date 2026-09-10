# Changelog

## Unreleased

### Changed

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
