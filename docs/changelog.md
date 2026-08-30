# Changelog

## Unreleased

## 2. [0.3.2] - 2026-08-30

- **The re-spawn note on a poll state names its own `poll.max_attempts`
  instead of an internal sentinel.** A poll state is exempt from the visit
  attempt budget — `poll.max_attempts` already bounds it — and that exemption
  was encoded internally as `u64::MAX`, which `rhei run` then printed verbatim:
  `attempt 4 of 18446744073709551615`. The exemption was correct; only the
  rendering was wrong. The note now reads `attempt 4 of 96 (poll.max_attempts)`
  for a poll state, and is unchanged for every other state. (PR #119)
- **The root `CHANGELOG.md` no longer claims the release maintains it.** It
  opened by saying this file's `Unreleased` section is what "release automation
  promotes into a numbered section here at release time", citing
  §FS-rhei-distribution.5 — which describes a promotion *within*
  `docs/changelog.md`, archiving the displaced release under `docs/changelog/`,
  and says nothing about the root file. `prepare_changelog_release.py` never
  opens it. Documenting a behaviour that never ran, it drifted the whole way: no
  `0.2.0`, `0.3.0` or `0.3.1` section, and an `Unreleased` note still announcing
  project-qualified ticket ids as a forthcoming breaking change eight days after
  they shipped in 0.2.0 — a note that nearly sent 0.3.1 out as a minor bump. The
  header now says where the changelog is and that this file is not maintained by
  the release, and the stale note is gone. The `0.1.0` section stays:
  `docs/changelog/0.1.0.md` is a four-line summary, so this is the only record of
  the initial release's feature list and its crates.io naming limitation.
  (PR #115)

## 3. Older releases

- [0.3.1](changelog/0.3.1.md) - 2026-08-30: - **The release commit stages `xtask/Cargo.toml`.** `Auto bump` had failed on its last three runs, always at `release.yml`'s version check and always before the publish step, with `xtask/Cargo.toml internal dependency requirement is stale: ...
- [0.3.0](changelog/0.3.0.md) - 2026-08-23: - Give a cold invocation the project's **mid-term memory**.
- [0.2.0](changelog/0.2.0.md) - 2026-08-22: - Separate a run from the surface that watches it.
- [0.1.0](changelog/0.1.0.md) - 2026-05-21: - Initial alpha release line for the Rhei CLI, Rust crates, npm wrappers, and PyPI wrappers.
