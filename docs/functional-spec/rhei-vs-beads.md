# Rhei vs. beads — Command Surface Comparison

A focused comparison of the command surfaces of [beads](https://github.com/gastownhall/beads)
(Steve Yegge's Dolt-DB-backed agent issue tracker) and Rhei. The general
multi-system overview lives in [`comparison.md`](comparison.md); this document
zooms in on the CLI verbs and the gaps Rhei has by comparison.

## At a glance

| Aspect | Rhei | beads |
|---|---|---|
| Storage | Markdown files (`.rhei.md` or `tasks/*.md`) | Dolt SQL DB in `.beads/` |
| Source of truth | The file | The DB |
| Top-level verbs (count) | ~12 | ~50+ across ~190 command files |
| Mutation model | Edit the markdown; CLI orchestrates state | Every field mutates via `bd update` |
| Concurrency primitive | `rhei transition` (CAS on state) + Directory Workspace | `bd update --claim` (atomic assignee+status) |
| External-tracker bridge | None | `bd github`, `bd gitlab`, `bd jira`, `bd linear`, `bd ado`, `bd notion` |
| Memory decay | None | `bd admin compact` (tiered summarization of old issues) |
| Async waiters | Callbacks (synchronous on transition) | `bd gate {check,resolve,discover}` (PR runs, timers, beads) |
| Templating | `rhei instantiate` (MiniJinja, typed inputs) | `bd formula`/`bd mol pour|wisp|bond|squash|burn` |
| Hierarchy | Heading depth + dotted ids (configurable 1–4 levels) | Typed `parent-child` edges (3 levels) |
| State machine | Pluggable YAML, profiles, artifact contracts, counted visits | Fixed `open`/`in_progress`/`closed` (+`blocked`,`deferred`) |

## Rhei's command surface (today)

```
rhei validate <plan>                Schema + DAG + artifact + link checks
rhei render <plan> --format ...     JSON / GitHub markdown / progress
rhei states                         Print configured state machine
rhei transition <plan> --task ... --from ... --to ...
                                    Atomic CAS state change
rhei run <plan>                     Orchestrator: drives plan through SM
rhei next <plan> [--peek] [--task]  Claim and advance the next ready task
rhei complete <plan> --task --result
                                    Terminal-state + write result file
rhei reset <plan>                   Reset plan/workspace to initial state
rhei templates                      List discovered templates
rhei instantiate <tpl> --set ...    Materialize template into plan/workspace
rhei install-skills                 Install agent skills (Claude Code, Cursor, …)
rhei version
```

Plus `cargo xtask viz` for graph rendering.

## beads' command surface (grouped)

**Issue lifecycle:** `create`, `update` (with `--claim`), `close`, `reopen`,
`show` (`--current`), `edit`, `delete`, `defer`/`undefer`, `promote`, `assign`,
`comment`, `note`, `tag`, `duplicate`.

**Dependencies & structure:** `dep add|remove|tree` (typed: `blocks`,
`related`, `parent-child`, `discovered-from`), `label add|remove|list`,
`state`/`set-state` (operational state via `dim:val` labels), `epic`,
`children`, `link`, `relate`, `graph`.

**Views & reports:** `ready` (the headline command), `list` (extensive
filters: status, priority, label, title/desc-contains, no-assignee,
empty-description, created/updated/closed-after/before, stale, spec, parent),
`stale --days N`, `search`, `query`, `count`, `status`, `statuses`, `history`,
`audit`, `orphans` (mentioned in commits but never closed), `duplicates`,
`find-duplicates`, `where`, `context`, `last-touched`.

**Sync & data:** `dolt push|pull`, `export`/`import` (JSONL),
`export-obsidian`, `graph export|apply|visual`, `backup init|sync|restore|status`,
`sync`, `branch`, `vc {log,diff,commit}`, `merge --into` (duplicate consolidation).

**Setup & config:** `init` (`--server`, `--stealth`, `--from-jsonl`), `config`,
`setup`, `onboard`, `hooks install`, `rules`, `kv set|get|list`,
`completions`, `info --schema`, `repo`, `quickstart`.

**Maintenance:** `doctor` (`--fix`, `--health`, `--pollution`, `--conventions`),
`admin compact` (memory decay), `admin cleanup`, `admin reset`, `migrate`,
`rename-prefix`, `restore` (revive compacted), `prune`, `gc`, `lint`, `purge`,
`flatten`.

**Integrations:** `github`, `gitlab`, `jira`, `linear`, `ado`, `notion`,
`federation`, `routed`, `swarm`, `worktree`, `cook`, `ship`, `prime`,
`preflight`, `mail`, `ping`.

**Gates (async waits):** `gate list|show|check|resolve|discover|add-waiter`
(types: `gh:pr`, `gh:run`, `timer`, `bead`).

**Molecules (template/chemistry):** `formula list`; `mol show|distill|pour|wisp|bond|squash|burn|seed|stale|progress|current|last-activity|ready-gated`.

## What Rhei is missing (relative to beads)

These are gaps in Rhei's CLI surface, organized by whether they are
philosophically aligned with Rhei (worth adding), tangential (probably never
adding), or already covered by another mechanism.

### Gaps worth considering

1. **Read-only ready queue.** `rhei next --peek` shows one task; there is no
   `rhei ready` that lists *all* currently eligible tasks. Useful for
   dashboards and orchestrators that want to schedule across a queue.

2. **`rhei list` with filters.** No way to ask "what tasks are in `review`?"
   or "which tasks have no assignee?" without rendering and grepping. beads
   has ~15 filter flags on `bd list`. Even a minimal `rhei list --state X
   --assignee Y --has-prior Z` would be widely useful.

3. **Stale detection.** No `rhei stale --days N` to find tasks that have sat
   in a non-terminal state too long. The data is already in markdown
   (timestamps in workspace events) but there's no surface for it.

4. **History / audit.** beads has `bd history <id>` and `bd audit`. Rhei
   relies on `git log` + Directory Workspace event files, which works but is
   not a first-class verb. A `rhei history --task <id>` reading the event
   log would be a small, focused addition.

5. **Dependency CLI.** `**Prior:**` is edited in the markdown today. A
   `rhei dep add <task> --prior <other>` (and `remove`, `tree`) would let
   agents manipulate the DAG without re-emitting the surrounding markdown,
   reducing diff churn and merge conflicts.

6. **`rhei doctor`.** Rhei's `validate` covers schema, but doesn't surface
   things like "tasks with no Description body", "results referenced by tasks
   that don't exist on disk", or "events written but never reflected in the
   plan". A health/lint pass would be a natural extension of the validator.

7. **Async gates.** beads' `gate` model — wait for a PR check / GH workflow
   run / timer / another bead — fills a real gap for long-running work. Rhei
   has callbacks fired *during* a transition; it has no concept of "this
   task is parked until external signal X arrives." The closest current
   pattern is polling states, but no CLI plumbing for it.

8. **Multi-id / batch operations.** Most beads verbs accept multiple ids
   (`bd close BD-1 BD-2 BD-3 --reason ...`). Rhei verbs target one task at a
   time.

9. **`rhei merge --into` (duplicate consolidation).** Inevitable once plans
   get long enough that two tasks describe overlapping work.

10. **Graph export / visual.** `xtask viz` exists but is not part of the user
    CLI; promoting it to `rhei graph` (with `--format dot|svg|mermaid`) makes
    the structure of a plan as easy to share as the plan itself.

### Gaps that are philosophical, not accidental

- **`bd update`-style field mutation.** beads has CLI for every field
  (priority, description, title, design, notes, …). Rhei deliberately treats
  the markdown file as the source — humans and agents edit it directly. A
  `rhei update` would only make sense for fields with a cross-cutting
  invariant (e.g., assignee, prior).

- **External tracker bridges (`bd github`, `bd jira`, …).** beads' value
  proposition is "be the issue tracker"; Rhei's is "be the plan format."
  GitHub-issue sync is plausibly a side tool, not core CLI.

- **`bd admin compact` (memory decay).** Closed tasks in Rhei live in the
  markdown forever or get archived by file deletion. Compaction-as-CLI
  presupposes a DB; the markdown analogue is an editor pass or a separate
  `rhei archive` verb if it ever becomes necessary.

- **Molecules / protos / wisps.** beads' four-tier instance model
  (formula → mol → wisp → digest, with `pour`/`squash`/`burn` lifecycle)
  serves a specific orchestration philosophy. Rhei templates cover the
  parameterized-skeleton case; ephemeral nested instances are out of scope
  and a different design.

- **`bd kv`** (per-user K/V synced via Dolt). Without a DB, Rhei has nowhere
  to put this; agents that need scratch state should write it to plan files
  or `runtime/`.

- **`bd federation` / `bd routed` / `bd mail`** (cross-repo delegation).
  Solves a problem Rhei does not yet claim to solve. Could become relevant
  once multi-plan workspaces are real.

### Gaps already covered by another mechanism

- **Concurrency / `--claim`.** `rhei transition` is the CAS primitive;
  Directory Workspace shards mutations into per-task files for parallel
  agents. Different shape, equivalent guarantee.

- **Templates.** `rhei instantiate` covers parameterized workflow capture
  (state machine + plan skeleton + typed inputs). beads' `formula`/`mol`
  layer is richer but solves a different problem.

- **Hooks.** beads has `bd hooks install`; Rhei has callbacks declared in the
  state machine YAML and `rhei install-skills` for agent integration.

- **Worktree-based parallel agents.** Directory Workspace is Rhei's answer.

## Summary

Rhei's CLI is small and orchestrator-shaped: validate, render, transition,
run, next, complete. beads' CLI is large and tracker-shaped: every field
mutates through the CLI, every report is a flag combination on `bd list`,
and there are explicit verbs for stale-detection, audit, duplicates, async
gates, external trackers, and memory compaction.

The most defensible additions to Rhei in the near term — ones that fit the
"markdown is source of truth, agent is operator" model — are:

- `rhei list` (filter the plan)
- `rhei ready` (read-only queue, distinct from `rhei next`)
- `rhei stale` and `rhei history`
- `rhei doctor`
- `rhei dep add|remove`
- Promoting `xtask viz` to `rhei graph`
- A gate / external-signal mechanism for long-running tasks

Everything else is either a different design (DB-backed mutation, memory
compaction, molecules) or an integration surface (trackers, federation) that
is a deliberate non-goal until a concrete use case demands it.
