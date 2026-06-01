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
content currently exists: a discovered domain rhei under `rheis/` with id
`basin` is a load/validation error. Reserving it unconditionally avoids a
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

Within a project, read-only commands operate **project-wide by default**. The
current Panta implementation supports loading, validation, listing, rendering,
and visualization over the merged project graph. Mutating commands are staged:
until project-wide rewrites can route every state, assignee, result, and runtime
artifact back to the owning rhei with that rhei's state machine, `rhei run`,
`rhei next`, `rhei transition`, `rhei complete`, and `rhei reset` must reject
project-scoped Panta inputs with an actionable message. Operators can still
target an individual rhei directly when they need mutation.

The target behavior for full Panta execution remains project-wide mutation: the
project is the unit an operator drives. Because a future mutating invocation can
fan out across every rhei, any command that spawns work or destroys runtime
state must report its scope and the affected rheis before acting.

Each rhei may declare its own state machine via `**States:**`; the
`index.panta.md` manifest supplies the project default for rheis that do not.
Commands resolve and apply the correct machine per rhei (§AR-rhei-panta).

### 6.1. Readiness and `rhei next`

Readiness is **project-global**. A ticket is ready when it is a claimable leaf
and every `**Prior:**` is terminal-and-not-cancelled, resolved across the whole
project graph — a ticket in one rhei may be blocked by a ticket in another. Each
prior's terminal status is judged against *that prior's own* rhei state machine.
Rheis and Panta are structural rollups and are never claimable. `--rhei` narrows
the candidate tickets but never narrows where their priors resolve: a candidate
may still be blocked by a prior outside the named rheis.

Project-scoped `rhei next` follows the staged mutation boundary above: the
readiness model is specified here so future execution is deterministic, but the
current CLI must reject Panta project inputs for claim mode rather than writing
assignments into child rhei files.

### 6.2. `rhei run`

At project scope, `rhei run` orchestrates ready tickets across all in-scope rheis
under one loop, applying each ticket's own rhei state machine. It drives tickets
to terminal states; it never writes state to a rhei or Panta node. Concurrency
across rheis is bounded, and each spawned unit is attributed to its rhei in logs
and accounting. The loop stops when no eligible ticket remains in scope or a
gating state requires a human.

This is the intended project-wide execution behavior. Until the staged mutation
boundary is lifted, the current CLI must reject `rhei run <panta-project>` and
ask the operator to use read-only project commands or target one child rhei.

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
- `rhei validate` always checks the whole project graph: cross-rhei dependency
  resolution, project-qualified id uniqueness, rhei-id validity, and the reserved
  `panta`/`rhei` kinds.
- `rhei list` is project-wide with rheis as the top level; existing filters
  (`--ready`, `--state`, `--assignee`, kind) apply across the project, and
  `--rhei` filters to a rhei. The `basin` rhei is ordered last and de-emphasized
  in default output (§4).
- `rhei viz` renders Panta as the implicit canvas (never a drawn root box), rheis
  as top-level groups, with cross-rhei dependency edges drawn between them. The
  `basin` group is placed last and de-emphasized (§4).

## Related Specifications

- [Plan Language](rhei-plan-language.spec.md) — grammar, the node hierarchy, and the virtual-root model §FS-rhei-plan-language.3
- [Panta Architecture](../architecture/rhei-panta.spec.md) — load model, on-disk layout, id rules §AR-rhei-panta
- [Panta Root Decision](../decisions/architectural/panta-root.md) — why Panta is a unified virtual root §DA-panta-root
