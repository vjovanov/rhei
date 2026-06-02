# Recommended Solution

A Panta project contains only rheis at level 1. Inbox work lives under a reserved synthetic level-1 rhei named `inbox`, so inbox tickets are ordinary tickets with ids like `inbox.<local-id>` and follow the same validation, execution, and state-policy paths as any other ticket. State-machine resolution uses a deterministic two-phase resolver (rhei-local `**States:**`, then inherited `index.panta.md` default, then built-in fallback); a CLI override may redirect the loaded file but must still match any authored or inherited declaration name. Cross-rhei readiness uses the same successful-terminal predicate as normal scheduling (resolved state is `final: true` and not `cancelled`), evaluated against the prior rhei's own state machine. The canonical language reference gets one compact file-kind map covering `index.panta.md`, `rheis/`, `*.rhei.md`, `index.rhei.md` with `tasks/**/*.md`, and optional `inbox/`, while `states.yaml` stays under the state-machine language surface.

## Why This Was Chosen
- Keeps the required hierarchy intact: level 1 is always rheis, tickets are level 2 or deeper, with no special Panta-level ticket namespace.
- Reserves `inbox` permanently as a project-level rhei id, so a domain rhei can never collide with inbox later and there is no delayed-migration trap.
- Treats Panta state-machine inheritance as a default rather than a merge, so authored or inherited policy can't be silently masked by overrides or shadowed by child files.
- Keeps scheduling semantics identical for local and cross-rhei dependencies while still allowing shared, direct, or cached readiness implementations across process or repository boundaries.
- Makes Panta syntax discoverable from one entry point in the language reference without duplicating the owning functional specs.

## What Was Not Chosen
- Direct Panta-child inbox tickets — violates the level-1-is-rheis hierarchy rule.
- Conditional `inbox` reservation (only when inbox content exists) — creates a delayed-migration trap if a domain rhei is later named `inbox`.
- Inbox-specific state-policy tier — duplicates ordinary ticket policy lookup and risks conflicting with the generic resolver.
- Override-first state-machine resolution — can silently mask authored or inherited policy.
- Letting child rhei-local `states.yaml` shadow Panta defaults without an explicit `**States:**` declaration — makes inheritance search-based instead of a deterministic project-root default.
- Any-terminal cross-rhei readiness — would let `cancelled` prerequisites incorrectly unblock dependents.
- Requiring one shared in-process readiness predicate everywhere — over-constrains cross-process and API-boundary implementations.
- A separate Panta language reference, or a broad map that pulls root `states.yaml` into the project-markdown surface — blurs the boundary between markdown syntax and the state-machine language.

## Human Check
- Confirm that reserving `inbox` as a permanent project-level rhei id (even when no inbox content exists) is the desired tradeoff over conditional reservation.
- Confirm that inbox-ticket promotion to a domain rhei is accepted as an id-changing reparenting operation, with the follow-up decision on dependency rewrites, aliases, artifact migration, logs, and external references handled separately.
- Confirm the CLI `--state-machine <path>` override semantics: it may redirect the loaded file, but the file's `name` must still match any authored or inherited declaration.
- Confirm that cached or exported cross-rhei readiness must keep dependents blocked when freshness, state-machine version, or the prior state cannot be resolved reliably.
- Confirm that diagnostics should surface the resolved state-machine source (override path, rhei-local declaration, inherited Panta declaration, or built-in fallback).
