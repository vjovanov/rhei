# Final Solution - Resolve Panta project spec conflicts

## Recommendation

Adopt a single Panta model where a project contains only rheis at level 1,
including a reserved synthetic `inbox` rhei when inbox content exists. Inbox
tickets are ordinary ticket nodes under that synthetic rhei, use canonical ids
such as `inbox.<local-id>`, and resolve state policy through the same
rhei-ticket machinery as other work.

Use one deterministic state-machine resolver for validation and execution:
bind the effective declaration name from rhei-local `**States:**`, then an
inherited `index.panta.md` default, then no declaration. A CLI
`--state-machine <path>` override may redirect the loaded file, but if an
authored or inherited declaration exists, the loaded file's `name` must match
that declaration. Without an override, rhei-local declarations resolve from the
rhei sibling or Directory Workspace `states.yaml`; inherited Panta defaults
resolve only from `<project>/states.yaml`; omitted declarations fall back to the
compiled built-in `rhei` machine and ignore discovered files.

Define cross-rhei readiness with the same successful-terminal predicate used by
normal scheduling: the prior's resolved state must have `final: true` and its
normalized state name must not be `cancelled`. Evaluate that predicate using
the prior rhei's own state-machine semantics. Implementations may use a shared
predicate, direct resolution, or an exported/cached readiness result only if the
result is behaviorally equivalent and fresh or auditable.

Update the canonical language reference so Panta authoring syntax is
discoverable from one entry point. Replace the compressed plan/project-markdown
description with a compact file-kind map for `index.panta.md`, Panta
`rheis/` entries, `*.rhei.md`, `index.rhei.md` plus workspace `tasks/**/*.md`,
optional `inbox/` task files, and bare-rhei loading where relevant. Keep
`states.yaml` in the separate state-machine language surface, and link Panta
rows to the state-resolution/defaulting rules and architecture mechanics.

## Why This Solution

- It preserves the required hierarchy: Panta level-1 children are rheis, and
  tickets are level 2 or deeper.
- It avoids a special Panta-level ticket namespace by making inbox work use the
  same ids, validation, execution, artifacts, and state-policy paths as
  ordinary rhei work.
- It gives `inbox` one stable namespace rule by reserving the id at the project
  rhei-id level, instead of making validity depend on whether inbox content
  currently exists.
- It treats Panta state-machine inheritance as a default, not a merge, and
  prevents overrides from silently masking authored or inherited policy.
- It keeps scheduling semantics identical across local and cross-rhei
  dependencies while allowing practical implementation choices across process,
  repository, or API boundaries.
- It makes Panta syntax findable in the canonical language reference without
  duplicating the owning functional specs or moving `states.yaml` out of the
  state-machine surface.

## Point Decisions

- point-inbox-hierarchy: Choose a reserved synthetic level-1 rhei with id
  `inbox`. Inbox tickets are ordinary level-2-or-deeper tickets under that
  rhei, with ids like `inbox.<local-id>`. The synthetic rhei appears only when
  inbox content exists, has no authored `index.rhei.md`, stores no state, and
  uses the generic state-machine resolver before applying ordinary ticket
  policy lookup.
- point-cross-rhei-readiness: Choose the normal successful-terminal readiness
  rule for cross-rhei dependencies. A prerequisite unblocks dependent work only
  when its resolved state is final and not normalized `cancelled`; unresolved,
  stale, non-final, or cancelled prerequisites keep dependents blocked.
- point-state-machine-resolution: Choose the hybrid two-phase resolver.
  Effective declaration binding checks rhei-local `**States:**`, then inherited
  `index.panta.md` default, then no declaration. File loading follows the
  selected source and requires name matches for authored or inherited
  declarations; omitted declarations use the built-in `rhei` fallback.
- point-language-reference: Choose a compact structured file-kind map in the
  canonical language reference. It explicitly names Panta files and directories,
  relates them to ordinary rhei/workspace syntax, routes behavior to functional
  specs, routes mechanics to architecture, and keeps `states.yaml` documented
  under the state-machine language surface.

## Alternatives Rejected

- Direct Panta-child inbox tickets: Rejected because it violates the required
  hierarchy where level-1 Panta children are rheis and tickets are level 2 or
  deeper.
- Conditional `inbox` reservation: Rejected because it creates a delayed
  migration trap; a domain rhei named `inbox` could be valid until inbox
  content later appears.
- Inbox-specific state-policy tier: Rejected because it duplicates ordinary
  rhei-ticket policy lookup and risks conflict with the generic state-machine
  resolver.
- Deferring all inbox promotion/id-change implications: Rejected because the
  chosen id scheme necessarily makes filing an inbox ticket into a domain rhei
  an id-changing reparenting operation.
- Override-first state-machine resolution: Rejected because it can silently
  mask authored or inherited state policy and conflicts with name-matching
  behavior.
- Letting child rhei-local `states.yaml` shadow inherited Panta defaults
  without redeclaring `**States:**`: Rejected because it makes inheritance
  search-based instead of a deterministic project-root default.
- Any-terminal cross-rhei readiness: Rejected because normalized `cancelled`
  prerequisites must not unblock dependent work.
- Requiring one shared in-process readiness predicate everywhere: Rejected
  because it over-constrains cross-process, cross-repository, and serialized/API
  implementations, even though sharing the predicate is preferred where
  practical.
- A separate Panta language reference: Rejected because Panta is an extension of
  the plan/project markdown surface, not an independent grammar source.
- A broad language-reference map that includes root `states.yaml`: Rejected
  because it blurs the boundary between project/rhei markdown syntax and the
  state-machine language surface.

## Risks And Open Questions

- Inbox ticket promotion still needs a follow-up promotion/refactoring decision
  for dependency rewrites, aliases, artifact migration, logs, and external
  references when ids change from `inbox.<local-id>` to
  `<target-rhei>.<local-id>`.
- Cached or exported cross-rhei readiness needs a concrete freshness and
  invalidation contract. If freshness, state-machine version, or the prior
  state cannot be resolved reliably, the dependency must remain blocked.
- Diagnostics should expose state-machine resolver source metadata: override
  path, rhei-local declaration, inherited `index.panta.md` declaration, or
  built-in fallback.
- Future scoped Panta defaults would require an additional inherited
  source-selection rule before file lookup.
