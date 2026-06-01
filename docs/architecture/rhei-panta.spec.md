# AR-rhei-panta: Panta root architecture

This document specifies how Panta — the per-project virtual root above all rheis
(§FS-rhei-panta) — is laid out on disk, loaded into a single graph, and
addressed. It realizes the decision recorded in §DA-panta-root and extends the
node hierarchy and virtual-root model of §FS-rhei-plan-language.3.

## 1. On-disk layout

A Panta is a project directory. It reuses the Directory Workspace machinery
(§FS-rhei-plan-language.1.2) one level up: where a Directory Workspace merges
task files into one plan, a Panta merges rheis into one project.

```
project/                 ← Panta (the project root)
  index.panta.md         ← Panta manifest: project title, default **States:**
  rheis/                 ← one entry per rhei, discovered like tasks/
    auth.rhei.md         ← a single-file rhei
    billing/             ← or a rhei as a Directory Workspace
      index.rhei.md
      tasks/
      runtime/           ← this rhei's own per-ticket artifacts
  basin/                 ← optional: project basin rhei for unfiled tickets
  rheis/runtime/         ← per-ticket artifacts for the single-file rheis here
  runtime/               ← project-level ROLLUPS only (accounting, snapshots, viz)
```

Per-ticket artifacts live under each rhei's own execution root, not at the
project root; the project `runtime/` holds only cross-rhei rollups (§6).

`index.panta.md` is the Panta manifest. It plays the role `index.rhei.md` plays
for a workspace: project title, default state-machine declaration, and content
sections; it contains no authored nodes. Rhei discovery under `rheis/` follows
the same recursive, deterministic, non-hidden, `/`-normalized ordering rules
that task-file discovery already uses (§FS-rhei-plan-language.1.2), except that
`runtime/` directories are artifact trees and are never discovered as rheis even
when they contain `.rhei.md` files or `index.rhei.md`. A directory is a Panta
when it contains `index.panta.md`.

## 2. Load model

Loading a Panta produces one graph rooted at the virtual `panta` node:

1. Parse `index.panta.md` for project title, default states, and content.
2. Discover the rheis under `rheis/` in deterministic order. Each rhei is loaded
   by the existing single-file or Directory Workspace loader.
3. If `basin/` is present, load it as a synthetic Directory Workspace rhei with
   id `basin`. It has no authored `index.rhei.md`; its files parse as workspace
   task files, inherit the Panta default state declaration, and use `basin/` as
   their execution root.
4. Synthesize the virtual `panta` root and attach each loaded rhei, including the
   synthetic basin rhei, as a level-1 child.
5. Merge into one task graph and resolve `**Prior:**` across the whole project,
   exactly as workspace task files merge today.

The virtual root is materialized in memory only; it is never written back. A
source map records, for each node, the rhei (and file) that defines it, so
targeted rewrites during transitions still target the owning file — the same
contract `task_sources` provides for workspace task files.

The target model is that a bare rhei loaded directly (a `.rhei.md` file or a
workspace with no enclosing `index.panta.md`) is treated as the single rhei of an
implicit Panta, so every load path yields a Panta-rooted graph. In the current
staged implementation a bare rhei still loads through the existing single-file or
Directory Workspace path and is *not* wrapped in a synthetic Panta; only a
directory containing `index.panta.md` loads as a Panta project. Unifying the bare
rhei load path under an implicit Panta is deferred (§FS-rhei-panta.6, roadmap).

## 3. Identity and id namespacing

Ids are dotted paths rooted at Panta. A rhei contributes its id as the prefix for
its tickets:

- A rhei has a single-segment id (`auth`, `billing`).
- A ticket's project-wide id is its rhei id joined with its rhei-local id:
  rhei-local `1` under rhei `auth` is the project id `auth.1`; rhei-local `1.2`
  is `auth.1.2`.
- A ticket captured without an owning rhei is authored under `basin/`, which is
  loaded as the `basin` rhei, so rhei-local `3` becomes project id `basin.3`.
- Project ids must be unique across the whole Panta. Because the rhei id prefixes
  every ticket beneath it, authors only need uniqueness within a rhei, plus
  unique rhei ids across the project.
- `basin` is a permanently reserved rhei id. A discovered rhei under `rheis/`
  with id `basin` is a load/validation error whether or not `basin/` content
  exists, so the synthetic basin rhei can never collide with a domain rhei
  (§FS-rhei-panta.2).

Within a rhei, tickets are authored and validated exactly as today
(§FS-rhei-plan-language.3.4): the rhei-local id space is unchanged, including
`structure.maxLevels` (1–4) counted from the rhei-local level 1. The Panta prefix
is applied at merge time and is not authored into the rhei's task headings.
`**Prior:**` references may be written as project ids to point across rheis;
within a rhei, rhei-local references continue to resolve locally.

## 4. State-machine binding

`index.panta.md` supplies a default state-machine declaration for any rhei that
omits its own `**States:**`. That inherited declaration resolves from the Panta
project root; a rhei-local declaration still resolves from the rhei's own source
location (§FS-rhei-plan-language.1.3). Panta inheritance is a default, not a
merge: an inherited declaration is used only when the rhei omits `**States:**`,
and a child rhei-local `states.yaml` never shadows the project default unless the
rhei redeclares `**States:**`. Validation and execution share this one resolver
and surface the resolved source in diagnostics — CLI override path, rhei-local
declaration, inherited `index.panta.md` declaration, or built-in `rhei`
fallback.

The state-machine profile that previously resolved the level-0 `rhei` root now
resolves the `panta` root: Panta resolves through `node_policy.root`. A rhei node
resolves through the dedicated `node_policy.rhei` key when declared; when it is
omitted, the rhei is a pure structural rollup with no profile-driven state. The
reserved names `panta` and `rhei` may not appear in `structure.nodeKinds` or as
`by_type` keys. Panta carries no stored state; any project-level status is a
rollup derived from its rheis (§FS-rhei-panta.3). The full resolution and
validation rules are specified in the states spec node-policy section
(§FS-rhei-states).

## 5. Execution root and per-rhei runtime

Because each rhei may run its own state machine (§FS-rhei-panta.6), artifact and
relative-link resolution is **per rhei**, not per project. A rhei's execution
root is defined exactly as a standalone plan's is today
(§FS-rhei-plan-language.3.10): the containing directory for a Single-File Plan,
the workspace directory for a Directory Workspace rhei. State-machine `inputs`,
`outputs`, `> **Result:**` paths, and content links inside a rhei all resolve
against that rhei's root.

Result and artifact paths use the project-qualified ticket id
(`runtime/results/auth.1.md`), so single-file rheis that share the `rheis/`
directory — and therefore one execution root — never collide on artifact names.
A rhei authored as a workspace directory gets a fully isolated runtime tree; that
is the escape hatch when an operator wants per-rhei isolation rather than a shared
`rheis/runtime/`.

The project (Panta) root owns only cross-rhei rollups: aggregate cost
accounting, project-level snapshots, and the unified visualization. It never
holds per-ticket artifacts.

## 6. Command scope mechanics

`rhei run` resolves and caches one state machine per rhei in scope and applies a
ticket's machine when transitioning it; cross-rhei dependency readiness consults
the prior ticket's own machine and requires a successful terminal state
(`final: true` and a normalized state name other than `cancelled`). This is the
same predicate normal in-rhei scheduling uses; implementations may share one
predicate, resolve the prior directly, or read an exported/cached readiness
result, but only when that result is behaviorally equivalent and fresh. A
dependency fails closed — the dependent stays blocked — whenever the prior
state, the prior's state-machine meaning, or a cached readiness result cannot be
resolved reliably. The ready-set
scan, claim selection, and rollup all walk the single merged graph (§2), so
project-wide is the natural default and `--rhei` is a filter applied to candidate
nodes after the merge. Mutating commands report the resolved scope and the set of
rheis they will touch before acting (§FS-rhei-panta.6).

## 7. Invisibility surface

Panta is excluded from default output by the layers that present nodes:
listing, `rhei next` claim selection, rendering, and visualization treat rheis as
the top level (§FS-rhei-panta.4). Tooling may expose the root behind an explicit
opt-in. Because the root is virtual and derived, no command may claim,
transition, complete, cancel, or reset it.

The synthetic `basin` rhei is de-emphasized rather than excluded: the same
presentation layers order it last and render it in a de-emphasized form, but it
participates normally in readiness, claim selection, execution, and rollup. The
de-emphasis is presentational only and never alters scheduling
(§FS-rhei-panta.4).

## Related Specifications

- [Panta (functional)](../functional-spec/rhei-panta.spec.md) §FS-rhei-panta
- [Plan Language](../functional-spec/rhei-plan-language.spec.md) §FS-rhei-plan-language.3
- [Panta Root Decision](../decisions/architectural/panta-root.md) §DA-panta-root
