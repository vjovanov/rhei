# Proposal - claude-code[yolo]:anthropic:claude-opus-4-7

- Recommendation: Model the inbox as a **synthetic level-1 rhei** with the
  reserved id `inbox`, materialized from an optional project-root `inbox/`
  directory. Inbox tickets are ordinary `task`-kind nodes at level 2..N inside
  that synthetic rhei.

  Concretely:

  - **Hierarchy / level.** `inbox` is a level-1 child of the virtual Panta root,
    exactly like every other rhei. Inbox tickets are level 2 at minimum and may
    nest to the inbox rhei's `structure.maxLevels`. Tickets are never direct
    children of Panta.
  - **Id.** The rhei id is the reserved name `inbox` (single segment). A ticket
    authored in `inbox/<file>.md` as rhei-local id `3` has project-wide id
    `inbox.3`; nested children extend it (`inbox.3.1`). `inbox` may not appear
    as a discovered rhei under `rheis/` — that is a load-time error, not silent
    shadowing.
  - **Parent.** The synthetic `inbox` rhei's parent is the Panta virtual root.
    Inbox tickets' parent is the synthetic `inbox` rhei (for top-level inbox
    tickets) or their authored parent ticket (for nested ones). The Panta root
    has no ticket children, ever.
  - **State-policy resolution.** Because the synthetic inbox rhei has no
    authored `index.rhei.md`, it carries no rhei-local `**States:**` and
    therefore inherits the Panta-level default state-machine declaration from
    `index.panta.md` (or the built-in fallback if neither is declared —
    resolution order itself is point-state-machine-resolution's concern). Inbox
    tickets then resolve their per-node policy through that inherited machine's
    `node_policy.overrides → node_policy.by_type[<kind>] → node_policy.default`,
    exactly as tickets in any other rhei do. The synthetic rhei node itself is
    a pure structural rollup with no stored state — it has no `node_policy.rhei`
    profile, so nothing tries to claim or transition it.

- Reasons:
  - **Preserves the single hierarchy invariant.** Panta → rhei → ticket holds
    without an exception. Every cross-cutting tool (`rhei validate`, `rhei
    list`, `rhei viz`, `rhei run`, `rhei complete`, readiness scans) keeps one
    code path, because the inbox is "just another rhei" at the graph level.
  - **Reuses existing id rules.** `inbox.3` follows the same `<rhei>.<local>`
    pattern as `auth.1`. Readers, parsers, and authors learn one rule. No new
    id syntax, no second namespace.
  - **No new state-policy tier.** State-policy resolution for inbox tickets is
    the ordinary "rhei omits `**States:**` → inherit Panta default" path that
    every other rhei without an authored machine already follows. Adding the
    inbox does not require a new `node_policy` key or a special case in the
    resolver.
  - **Confirms (and locks in) the cleanest reading of the existing spec.**
    §AR-rhei-panta.2 step 3 and §FS-rhei-plan-language.1.5 item 3 already
    describe a synthetic inbox rhei; the open issue is that the load rule,
    id rule, and state-policy rule must be stated together as a single
    normative paragraph so future spec edits can't drift them apart.
  - **Optional and zero-cost when unused.** Projects that don't want loose
    capture simply omit the `inbox/` directory; nothing is synthesized and the
    user is never shown an empty `inbox` rhei.

- Tradeoffs:
  - **`inbox` becomes a reserved rhei id** alongside the reserved kinds `panta`
    and `rhei`. Any project that already names a domain rhei `inbox` would
    have to rename it. Mitigation: enforce as a `rhei validate` error with a
    clear message at the rhei-id stage, before any cross-rhei merge.
  - **Synthetic-but-visible node.** The inbox is "invisible on disk" (no
    `index.rhei.md`) but visible in listings, viz, and ids. Operators must
    learn that one rhei has no manifest file. Mitigation: list and viz surface
    it identically to authored rheis so the distinction lives only at the load
    layer.
  - **Inbox cannot override `**States:**` locally.** Because there is no
    authored manifest, an inbox ticket cannot run under a different state
    machine than the Panta default without first being moved into a real rhei.
    Acceptable for "loose capture" semantics; if a ticket grows up, the
    operator promotes it.
  - **Two-segment id at the CLI surface.** Quick captures still produce ids
    like `inbox.3` rather than bare `3`. Slightly more verbose than a
    Panta-direct ticket would be; offset by the consistency win.

- Assumptions:
  - The plan-language hierarchy invariant "level-1 children under Panta are
    rheis" is non-negotiable (per the task constraints).
  - `node_policy.rhei` remains optional; structural-rollup rheis (including
    the synthetic inbox) are valid without a declared rhei profile.
  - The Panta default-state-machine declaration in `index.panta.md` is a
    feature the project will keep (the precedence order is decided by
    [[point-state-machine-resolution]], not here).
  - Cross-rhei readiness is decided in
    [[point-cross-rhei-readiness]]; this proposal does not depend on its
    outcome — inbox tickets follow whatever rule applies to all tickets.
  - Authors and tools tolerate `inbox.N`-style ids; if not, a CLI affordance
    can default-elide the prefix for inbox-scoped commands without changing
    the model.

- Rejection criteria:
  - **Drop the "level-1 = rhei" invariant.** If the team decides Panta may
    legitimately host ticket-kind direct children (a flat, two-level model),
    this proposal is moot — the inbox would then be a Panta-direct ticket
    container with a different parenting and id scheme.
  - **Drop the inbox feature.** If quick-capture without a chosen rhei is cut
    from v1 (a defensible scope reduction, see P1 in the discussion notes),
    this proposal does not apply; tickets must always be authored under a
    named rhei.
  - **Require an authored `index.rhei.md` for every rhei.** If the team
    forbids synthetic rheis on uniformity grounds, replace this with either
    (a) a templated, authored `inbox/index.rhei.md` written by `rhei init`
    (then the rhei is no longer synthetic, but everything else here still
    holds), or (b) drop the inbox.
  - **Adopt a flat global id scheme (P3a).** If project-wide ticket ids are
    chosen to be globally unique without rhei prefixes, the id segment of
    this proposal (`inbox.3`) must be rewritten to whatever the global scheme
    produces; the levelling and parenting arguments still hold.
