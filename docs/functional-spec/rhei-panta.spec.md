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
- **Dependencies resolve across rheis.** A ticket in one rhei may declare a
  `**Prior:**` on a ticket in another rhei; the reference resolves against the
  whole project graph. §AR-rhei-panta
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

Within a project, every command — read-only and mutating alike — operates
**project-wide by default**. Loading, validation, listing, and rendering read
the merged project graph; `rhei run`, `rhei next`, `rhei transition`,
`rhei complete`, and `rhei reset` mutate it, routing every state, assignee,
result, and runtime-artifact rewrite back to the owning rhei file. Because a
single rhei loaded directly is the sole rhei of an implicit Panta
(§AR-rhei-panta.2), there is no separate "bare rhei" command path: targeting one
rhei is simply a one-rhei project, and `--rhei <id>` narrows a multi-rhei project
to named rheis.

The project is the unit an operator drives. Because they fan out across every
in-scope rhei, `rhei run` and `rhei reset` report their resolved scope and the
affected rheis before acting. The report exists for fan-out, so a one-rhei
project — the implicit Panta wrapping a bare rhei — has nothing to disambiguate
and stays quiet: no `Scope:` line is printed unless the invocation reaches more
than one rhei or the target is an explicit Panta project.

An id passed to `--rhei` that names no rhei in the project is an error listing
the available rhei ids, rather than a silently empty scope.

A ticket target passed to a command (`rhei complete <id>`, `rhei transition
--task <id>`, …) is either the project-qualified id (`auth.1`) or a rhei-local
shorthand (`1`). A rhei-local target resolves only when exactly one in-scope
rhei contains that ticket; ambiguity across rheis is an error that names the
qualified candidates. Output, artifacts, and ledgers always use the qualified
id regardless of how the target was written.

One state machine governs the whole project: the `index.panta.md` declaration,
or the built-in `rhei` machine when the manifest declares none. A rhei may
restate that machine in its own `**States:**`, but declaring a *different*
machine is a load error (§AR-rhei-panta.4). Per-rhei machines are a deferred
capability, tracked on the roadmap.

### 6.1. Readiness and `rhei next`

Readiness is **project-global**. A ticket is ready when it is a claimable leaf
and every `**Prior:**` is terminal-and-not-cancelled, resolved across the whole
project graph — a ticket in one rhei may be blocked by a ticket in another.
Terminal status is judged against the project state machine (§AR-rhei-panta.4).
Rheis and Panta are structural rollups and are never claimable. `--rhei` narrows
the candidate tickets but never narrows where their priors resolve: a candidate
may still be blocked by a prior outside the named rheis. A ticket named
explicitly with `--task` must itself be in scope; targeting a ticket outside the
named rheis is an error rather than a silent widening.

Because that is the one interaction a narrowed invocation cannot show in its
own scope, the no-work diagnostic must explain it rather than describe
out-of-scope work: under `--rhei` it names the scope, reports only in-scope
tickets, and marks a blocking prior that lives outside the scope as such
(`Task auth.1 (pending, outside the --rhei scope)`). Reporting an out-of-scope
ticket as the work in progress is wrong — it reads as a bug in narrowing.

Claim mode writes the `**Assignee:**` into the owning rhei's file, resolved
through the source map (§AR-rhei-panta.2). `--peek` is read-only and never
writes.

### 6.2. `rhei run`

At project scope, `rhei run` orchestrates ready tickets across all in-scope rheis
under one loop, applying the project state machine (§AR-rhei-panta.4). It drives
tickets to terminal states; it never writes state to a rhei or Panta node. Concurrency
across rheis is bounded, and each spawned unit is attributed to its rhei in logs
and accounting. The loop stops when no eligible ticket remains in scope or a
gating state requires a human.

Before spawning, `rhei run` reports the resolved scope and the rheis it will
touch (§6). A bare rhei runs as the single rhei of its implicit Panta, so the
project-wide loop is the only execution path.

### 6.3. Completion and rollup

Result artifacts and their in-plan links are keyed by the **project-qualified**
ticket id: completing `auth.1` writes `runtime/results/auth.1.md` under the
owning rhei's execution root and links it as `[auth.1](runtime/results/auth.1.md)`.
Because every ticket gained its rhei prefix when bare rheis became implicit
Pantas, a plan completed before that change carries the rhei-local link
(`[1](runtime/results/1.md)`) beside a rhei-local artifact. Validation **accepts
that legacy form** so existing plans keep validating, and a ticket that is not
being completed keeps its existing link — an already-completed ticket keeps
pointing at the artifact that actually holds its result. Completing a ticket is
the one exception: `rhei complete` writes the qualified artifact, so it also
**refreshes that ticket's result link** to the file it just wrote — leaving a
legacy link beside a fresh qualified artifact would leave the plan green while
pointing at a file that no longer receives writes. There is no migration pass:
untouched tickets keep their form, and the two forms coexist in a long-lived
plan.

A result link is validated as a **pair**: text and target must describe the same
ticket id, both qualified (`[auth.1](runtime/results/auth.1.md)`) or both
rhei-local (`[1](runtime/results/1.md)`). A link that mixes the two forms, or
names any other id, is an error.

`rhei complete` finishes a leaf ticket. A rhei is done when all its tickets are
terminal, and Panta when all rheis are done, but this status is **derived, not
stored**: unprofiled rheis and the virtual Panta have no `**State:**` to write,
so no cascade stamps `completed` up the tree — doneness is computed on read. A
rhei given an explicit profile through `node_policy.rhei` does carry state; for
such a rhei the non-leaf rule applies — it may move to a terminal state only
after all its tickets are terminal, and `rhei run` or `rhei complete` may roll it
up automatically.

### 6.4. Reset, validate, list, viz

- `rhei reset` is project-wide by default; because it destroys runtime state
  across every in-scope rhei, it surfaces the scope and the affected rheis before
  acting. `--rhei` narrows it. A narrowed reset removes only the runtime
  artifacts owned by in-scope tickets rather than whole `runtime/` trees:
  sibling single-file rheis share one execution root, so removing the tree
  would destroy an out-of-scope rhei's state. "Owned" means keyed by an
  in-scope ticket id — its result file, logs, declared artifact-contract paths,
  snapshot sessions, worktree refs, accounting captures and task index, and its
  lines in the transition ledger. Leaving any of those behind would be a silent
  partial reset: a stale declared output can satisfy a required input on the
  next run, and a stale ledger line claims a completion the plan no longer
  holds. Run-scoped output — the run report, the dashboard, accounting rollups —
  is not ticket-owned; a narrowed reset keeps it and **says so**, rather than
  letting the operator discover the difference (§FS-rhei-reset.2.1).
- `rhei validate` always checks the whole project graph: cross-rhei dependency
  resolution, project-qualified id uniqueness, rhei-id validity, and the reserved
  `panta`/`rhei` kinds.
- `rhei list` is project-wide with rheis as the top level; existing filters
  (`--ready`, `--state`, `--assignee`, kind) apply across the project, and
  `--rhei` filters to a rhei. The `basin` rhei is ordered last and de-emphasized
  in default output (§4).
- `rhei viz` renders a **single rhei** — a `.rhei.md` file or a Directory
  Workspace — in the same id space the CLI uses, so its tickets carry their
  qualified ids (`auth.1`). It is **not yet Panta-aware**: pointed at a project
  directory it renders each `*.rhei.md` as a separate plan rather than one
  merged graph, draws no cross-rhei dependency edges, and skips Directory
  Workspace rheis inside the project. Project inputs must not be advertised as
  rendering a merged project graph until that path exists: `rhei viz` accepts a
  project directory but warns on stderr that the page is not the merged graph
  and points the operator at a single rhei (§FS-rhei-viz.7.3). The intended
  rendering remains Panta as the implicit
  canvas (never a drawn root box), rheis as top-level groups, and cross-rhei
  dependency edges between them; the `basin` group is placed last and
  de-emphasized (§4). Tracked on the roadmap.

## Related Specifications

- [Plan Language](rhei-plan-language.spec.md) — grammar, the node hierarchy, and the virtual-root model §FS-rhei-plan-language.3
- [Panta Architecture](../architecture/rhei-panta.spec.md) — load model, on-disk layout, id rules §AR-rhei-panta
- [Panta Root Decision](../decisions/architectural/panta-root.md) — why Panta is a unified virtual root §DA-panta-root
