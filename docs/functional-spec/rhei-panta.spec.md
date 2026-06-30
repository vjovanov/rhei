# FS-rhei-panta: Panta, the project root above all rheis

Panta is the single, invisible root of a Rhei project. It sits above every rhei
and every ticket, gives new rheis a default home, and is the one anchor from
which an operator can see the whole project as a single graph. Making the whole
project visible from one root, and keeping "add a rhei" a zero-friction action,
serve Rhei's monitoring and predictability goals. §GOAL-rhei-outcomes

The name follows *panta rhei* ("everything flows"): Panta is the still point that
contains all the flows. The decision and its rationale are recorded in
§DA-panta-root; the load model, on-disk layout, and id rules are specified in
§AR-rhei-panta.

## 1. What Panta is

A Rhei **project** has exactly one Panta. Panta is a *virtual* node: it is never
authored, never written to a file as a node, and has no `**State:**`,
`**Prior:**`, `**Assignee:**`, or `> **Result:**`. It is the level-0 root of the
node hierarchy:

```
Panta            the project        (virtual, exactly one, kind `panta`)
├── Rhei  auth   a flow / plan       (kind `rhei`)
│   ├── Task auth.1   a ticket       (kind `task`)
│   └── Task auth.2
├── Rhei  billing
│   └── Task billing.1
└── Rhei  basin  the project basin   (synthetic when `basin/` exists)
    └── Task basin.3   an unfiled ticket
```

A **rhei** is a plan — a self-contained flow with its own tasks. A **ticket** is
a unit of work inside a rhei (the node kind is `task` by default; "ticket" is the
user-facing name for a work item). Panta owns the rheis; rheis own their tickets.

`panta` is a reserved node kind. It can never be authored, can never appear in
`structure.nodeKinds`, and there is never more than one Panta in a project.

## 2. Default home for new rheis

Creating a rhei without specifying where it goes places it under Panta. Panta is
the implicit default parent, so adding a rhei takes no location argument:

```bash
rhei new "Authentication"        # creates a rhei under Panta
rhei new "Billing" --under auth  # opt out of the default to nest elsewhere
```

A ticket created with no owning rhei is placed in the project **basin**. The
basin is loaded as a level-1 rhei with id `basin`, so quick captures do not
require choosing a domain rhei first while the hierarchy remains Panta -> rhei
-> ticket. Basin tickets use ordinary rhei-local ids and project-wide ids such
as `basin.3`.

`basin` is a permanently reserved rhei id, independent of whether any basin
content currently exists: a discovered domain rhei with id `basin` is a
load/validation error. Reserving it unconditionally avoids a
delayed-migration trap where a domain rhei named `basin` is valid until the
first unfiled ticket appears. Filing a basin ticket into a domain rhei is a
reparenting operation that changes its project id from `basin.<local-id>` to
`<target-rhei>.<local-id>`.

## 3. One unified view

Because every rhei hangs off the same Panta, a project loads and renders as one
graph rather than as many disconnected plans:

- **Status rolls up** Panta ← rheis ← tickets through one tree. Panta's status is
  always derived from its rheis and is never stored.
- **Dependencies are rhei-level.** A rhei may depend on another rhei (§7.2);
  the project executes rheis in dependency order. Ticket-level `**Prior:**`
  resolves only *within* a rhei — a `**Prior:**` that crosses a rhei boundary is
  a validation error (§7.2). §AR-rhei-panta
- **Listing and monitoring** treat the set of rheis as the top level, so an
  operator sees the whole project from a single root.

## 4. Invisibility

Panta is not shown to users as a node. Default listing, claim selection
(`rhei next`), rendering, and monitoring present rheis as the top level and omit
Panta. Runtime commands must never claim, transition, complete, cancel, or reset
Panta — it has no state to move. Tooling may reveal the root only behind an
explicit opt-in flag (for example `--show-root`) for debugging; the default
output never mentions it.

The synthetic `basin` rhei is **de-emphasized, not hidden**. Unlike Panta, it is
a real rhei that participates fully in readiness, scheduling, execution, and
rollup — `rhei run` and `rhei next` treat its tickets like any other rhei's. But
because its tickets are unfiled quick-captures rather than planned work, default
listing and visualization order it last and present it in a de-emphasized form
(for example dimmed or collapsed) so it never competes with planned rheis. It is
never placed behind an opt-in flag the way Panta is: unfiled work must stay one
glance away so it gets triaged rather than silently accumulating unseen.

## 5. Identity

A rhei is addressed by its id (for example `auth`). A ticket is addressed by its
project-wide path, formed by joining its rhei id with its rhei-local id
(`auth.1`, `auth.1.2`, `basin.3`). This makes ticket identities unique across
the whole project without authors coordinating ids by hand. The exact
id-extension and grammar rules are specified in §AR-rhei-panta.

## 6. Project scope and command behavior

Every command resolves a **scope** from the target it is given. Pointed at a
project — a directory containing `index.panta.md`, or invoked inside one — a
command operates on the whole project. Pointed at a single rhei (a `.rhei.md`
file or a rhei workspace directory) it operates on that rhei alone. `--rhei <id>`
(repeatable) narrows a project-scoped invocation to named rheis.

Within a project, read-only commands operate **project-wide by default**:
loading, validation, listing, rendering, and visualization run over the merged
project graph.

Project-wide **execution** is performed by a dedicated command, `rhei panta`
(§7), rather than by overloading the single-rhei mutating commands. `rhei panta`
does not rewrite recipe entries or rhei files in place; it instantiates each
rhei into an isolated per-run directory and drives it there (§7.4, §7.5). The
in-place single-rhei mutating commands (`rhei next`, `rhei transition`, `rhei
complete`, `rhei reset`, `rhei run <rhei>`) continue to target an individual rhei;
pointed at a Panta project directory they reject the input and point at the
project-scoped command instead. Because project execution fans out across every
rhei, `rhei panta` reports its scope and the affected rheis before acting.

Each rhei may declare its own state machine via `**States:**`; the
`index.panta.md` manifest supplies the project default for rheis that do not.
Commands resolve and apply the correct machine per rhei (§AR-rhei-panta).

### 6.1. Readiness and `rhei next`

Within a single rhei, ticket readiness is unchanged: a ticket is ready when it is
a claimable leaf and every `**Prior:**` is terminal-and-not-cancelled, resolved
against that rhei's own graph. Because ticket-level `**Prior:**` never crosses a
rhei boundary (§7.2), ticket readiness is always rhei-local.

Cross-rhei sequencing is expressed at the **rhei** level, not the ticket level:
a rhei becomes ready to execute when all the rheis it `depends-on` are terminal
(§7.2). Project-scoped execution and that readiness model are specified under
[Panta orchestration](#7-panta-orchestration).

### 6.2. `rhei run` and `rhei panta`

`rhei run <rhei>` drives a single rhei to terminal states using that rhei's own
state machine, exactly as for a standalone plan.

Project-scoped execution is performed by `rhei panta`, which instantiates and
runs the project's rheis in dependency order. It is specified in full under
[Panta orchestration](#7-panta-orchestration). `rhei run` pointed directly at a
Panta project directory is rejected with a message pointing at `rhei panta`,
because project execution has its own command and run-isolation model.

### 6.3. Completion and rollup

`rhei complete` finishes a leaf ticket. A rhei is done when all its tickets are
terminal, and Panta when all rheis are done, but this status is **derived, not
stored**: unprofiled rheis and the virtual Panta have no `**State:**` to write,
so no cascade stamps `completed` up the tree — doneness is computed on read. A
rhei given an explicit profile through `node_policy.rhei` does carry state; for
such a rhei the non-leaf rule applies — it may move to a terminal state only
after all its tickets are terminal, and `rhei run` or `rhei complete` may roll it
up automatically.

### 6.4. Reset, validate, list, viz

- `rhei reset` is intended to be project-wide by default; because it destroys
  runtime state across every in-scope rhei, it surfaces the scope and the
  affected rheis before acting. `--rhei` narrows it. Under the current staged
  mutation boundary, project-scoped reset must reject Panta inputs instead of
  deleting child rhei runtime state.
- `rhei validate` always checks the whole project graph: project-qualified id
  uniqueness, rhei-id validity, the reserved `panta`/`rhei` kinds, that every
  ticket `**Prior:**` resolves *within its own rhei* (cross-rhei ticket priors
  are errors, §7.2), and — when a recipe is present — recipe integrity: unique
  recipe ids, resolvable `template` references, `depends-on` targets that exist,
  and an acyclic rhei dependency graph (§7).
- `rhei list` is project-wide with rheis as the top level; existing filters
  (`--ready`, `--state`, `--assignee`, kind) apply across the project, and
  `--rhei` filters to a rhei. The `basin` rhei is ordered last and de-emphasized
  in default output (§4).
- Panta-aware `rhei viz` is planned but not part of the current staged CLI
  boundary: the existing visualization path is not yet wired to the Panta loader,
  so Panta project inputs must not be advertised as rendering a merged project
  graph until that path is implemented. The intended rendering remains Panta as
  the implicit canvas (never a drawn root box), rheis as top-level groups, and
  cross-rhei dependency edges between them; the `basin` group is placed last and
  de-emphasized (§4).

## 7. Panta orchestration

Panta is not only a read-only view over rheis; it is a **singleton orchestrator**
that stores a project's rheis as a recipe and runs the ready ones in dependency
order. This is the concrete realization of "everything flows": one project file
declares the rheis, and one command executes them.

### 7.1. The rhei recipe manifest

The `index.panta.md` frontmatter carries an ordered `rheis:` list — the project
**recipe**. Each entry declares one rhei to instantiate and run:

```yaml
---
rheis:
  - id: auth
    template: spec-review
    inputs:
      spec: docs/auth.spec.md
    depends-on: []
  - id: billing
    template: code-review
    inputs:
      target: src/billing
    depends-on: [auth]
---
# Panta: Release 2.0
**States:** rhei
```

Entry fields:

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Stable, unique rhei id — the identity used by `depends-on` and runtime paths. Must be a valid rhei id and not the reserved `basin` or `panta`. |
| `template` | yes | A template name resolved against the library, or a path to a template directory — the same resolution as `rhei instantiate`. |
| `inputs` | no | A map of template input values, equivalent to `rhei instantiate --set key=value`. |
| `depends-on` | no | A list of other recipe `id`s that must be terminal before this rhei runs. |

The recipe is a **durable definition, never run state**. `rhei panta` never
writes back to it; the manifest changes only by hand-editing or `rhei panta add`.
This is the "manifest = recipe, each run = instance" model.

### 7.2. Dependencies are rhei-level only

The recipe's `depends-on` is the **only** cross-rhei dependency mechanism. A rhei
is **ready** when every rhei listed in its `depends-on` has reached a terminal
rollup — that is, all of that rhei's tickets are terminal in its own state
machine (terminal-and-not-cancelled counts as satisfied; a cancelled dependency
does not unblock dependents).

Ticket-level `**Prior:**` is strictly **rhei-local**: a ticket may only depend on
tickets within its own rhei. A `**Prior:**` whose target resolves outside the
ticket's rhei is a **validation error**, with a message directing the author to
express the relationship as a rhei-level `depends-on` instead.

The dependency graph over recipe entries must be **acyclic**; a cycle is a
validation error.

### 7.3. `rhei panta add`

Append a rhei entry to the recipe manifest of the current project.

```
rhei panta add <id> --template <name> [options]

Arguments:
  <id>                         Stable, unique rhei id for the new recipe entry

Options:
  --template <name|path>       Template to instantiate (resolved like
                                 `rhei instantiate`)
  --set <key>=<value>          Set a template input value (repeatable)
  --depends-on <id>            Declare a dependency on an existing recipe entry
                                 (repeatable)
```

Behavior:

1. Resolve the project (`index.panta.md` in or above the cwd, or `--project`).
2. Reject a duplicate or invalid `<id>`, an unresolvable `--template`, a
   `--depends-on` target that is not already in the recipe, and any edit that
   would introduce a dependency cycle — before writing.
3. Append the new entry to the `rheis:` frontmatter list, preserving the rest of
   the manifest (body, other entries, ordering).

`rhei panta add` only edits the recipe; it does not instantiate or run anything.

### 7.4. `rhei panta`

Execute the recipe: instantiate and run every rhei in dependency order.

```
rhei panta [options]

Options:
  --rhei <id>                  Restrict the run to the named rhei(s) and their
                                 transitive dependencies (repeatable)
  --dry-run                    Instantiate and validate each rhei without
                                 executing agents
```

1. **Allocate a run.** Choose a fresh monotonic run id `panta-<n>` and create
   `runtime/panta-<n>/` under the project root.
2. **Report scope.** Before doing work, print the run id and the rheis that will
   execute, with each rhei's dependencies (§6).
3. **Order by dependency.** Rheis execute in a topological order of the
   `depends-on` graph (§7.2), so a rhei always runs after the rheis it depends
   on. `--rhei` restricts the set to the named rheis and their transitive
   dependencies.
4. **Instantiate + run each rhei.** For each rhei `<id>` in order, instantiate its
   `template` with its `inputs` into `runtime/panta-<n>/<id>/`, then run that
   workspace with its own state machine via the standard per-rhei run path. With
   `--dry-run`, each rhei is instantiated and validated but no agents run.

Rheis currently execute **sequentially** in dependency order. Concurrent
execution of independent rheis (launching all ready rheis at once, with a
configurable cap) is a planned enhancement; sequential dependency-ordered
execution is the correct subset and the foundation for it.

A `panta-<n>` directory is a **self-contained, durable record** of one execution:
the instantiated workspaces and their runtime artifacts. Re-running the recipe
produces `panta-<n+1>/`, so run history accumulates rather than overwriting.
Tracking follows the same convention as a standalone rhei — instantiated plan
files and `runtime/results` are durable/tracked, caches are ignored — and each
instance runs through the standard execution loop (§FS-rhei-run.3). Each spawned
unit is attributed to its owning rhei in logs and accounting.

### 7.5. Runtime layout

```
<project>/
├── index.panta.md            # recipe (durable, tracked)
├── states.yaml               # project default state machine
└── runtime/
    ├── panta-1/              # first execution
    │   ├── auth/             # instantiated + run rhei instance
    │   │   ├── index.rhei.md
    │   │   └── runtime/      # that instance's results, logs, reports
    │   └── billing/
    └── panta-2/              # second execution (re-run)
        └── ...
```

Each `runtime/panta-<n>/<id>/` is an ordinary instantiated rhei workspace; every
`rhei` command (`run`, `next`, `list`, `validate`, `viz`) works on it directly,
so a past run can be inspected exactly like any hand-instantiated workspace.

## Related Specifications

- [Plan Language](rhei-plan-language.spec.md) — grammar, the node hierarchy, and the virtual-root model §FS-rhei-plan-language.3
- [Panta Architecture](../architecture/rhei-panta.spec.md) — load model, on-disk layout, id rules §AR-rhei-panta
- [Panta Root Decision](../decisions/architectural/panta-root.md) — why Panta is a unified virtual root §DA-panta-root
