# Discussion Response - claude-code[yolo]:anthropic:claude-opus-4-7

- Changed position: Partial. I keep the core of S-001 (synthetic level-1
  `inbox` rhei, tickets at level 2..N, ids `inbox.<local>`, structural rollup
  with no stored rhei state) but tighten three things: I now make D-003's
  id-change-on-promotion explicit instead of implicit, I narrow D-002 so this
  point only fixes ticket-level policy lookup and defers state-machine
  selection to [[point-state-machine-resolution]], and I hold D-001 as
  unconditional reservation with a sharper justification.

- Remaining disagreements:
  - D-001 (partially): I still want `inbox` reserved unconditionally; codex's
    "reserved only while the project inbox exists" remains the alternative.
    The remaining gap is purely whether reservation is temporal/conditional or
    static.

- Concessions:
  - **D-003 — codex is right to make promotion an id-change move, explicitly.**
    My proposal carried this implicitly via the `<rhei>.<local>` scheme but
    didn't surface the operator consequence. Codex naming it ("filing an inbox
    ticket into a domain rhei is a reparenting/id-change from `inbox.<id>` to
    `<target-rhei>.<id>`") is the clearer framing and I adopt it. It is a
    direct consequence of A-003 plus the id scheme, not a separate spec — so
    we state it here as a one-line consequence rather than spinning up a
    "filing/refactoring decision."
  - **D-002 — codex is right that my proposal risked over-specifying.** The
    full precedence chain I wrote (`overrides → by_type → default`) is about
    *per-node policy lookup once a state machine is selected*, and that is
    safe to fix here. The selection of *which* state machine the synthetic
    inbox rhei inherits (Panta-declared vs built-in fallback) is
    [[point-state-machine-resolution]]'s job; I should not pre-empt it. My
    original text said as much in a parenthetical — I now make the split a
    hard line in the recommendation.

- Final recommendation for this point: Adopt S-001 with the following three
  clarifications written as a single normative paragraph in the spec:

  1. **Reservation (D-001).** `inbox` is an unconditionally reserved rhei id
     at the project layer, alongside `panta` and `rhei`. A directory
     `rheis/inbox/` (or any other source that would produce a rhei with id
     `inbox`) is a load-time error from `rhei validate`, regardless of
     whether project inbox capture is currently in use. Projects that
     pre-existed this reservation and contain a rhei named `inbox` must
     rename it; this is a one-time migration noted in release notes.
  2. **Ticket-level state-policy lookup (D-002).** Inbox tickets resolve
     their per-node policy through the ordinary
     `node_policy.overrides → node_policy.by_type[<kind>] → node_policy.default`
     chain of the state machine that resolution gives the synthetic inbox
     rhei. The synthetic inbox rhei itself is a structural rollup: it has no
     `node_policy.rhei` profile, no stored state, and is never claimable.
     *Which* state machine the synthetic rhei inherits — Panta-declared vs
     built-in fallback, and the precedence between them — is decided by
     [[point-state-machine-resolution]] and intentionally not normatively
     fixed here.
  3. **Promotion (D-003).** Moving a ticket from the inbox into a domain
     rhei is a reparenting operation that changes its project id from
     `inbox.<local>` to `<target-rhei>.<local>`. Inbox ids are therefore
     capture-stable, not lifetime-stable; external references (dependencies,
     artifacts, logs) must follow the rename. Inbox ids are not a permanent
     external identity for work that gets filed.

- Reason: This converges with codex on every agreement (A-001..A-006) and on
  S-001's core, while resolving the three tagged disagreements with a clear
  rule per disagreement:
  - On **D-001**, unconditional reservation is the simpler invariant. Codex's
    "reserved while the project inbox exists" couples id validation to a
    runtime/feature flag, meaning the same project layout can be legal or
    illegal depending on whether `inbox/` is present elsewhere. That coupling
    leaks the inbox feature into every rhei-id check and creates a
    "first-time-you-add-inbox" migration trap when a user later enables
    capture in a project that already has a domain rhei named `inbox`.
    Reserving statically pays the cost once (one renamed rhei in legacy
    projects) and gives the validator one rule instead of a conditional.
  - On **D-002**, splitting "ticket policy lookup" (fixed here) from "state
    machine selection for synthetic rheis" (deferred to
    [[point-state-machine-resolution]]) honors the discussion prompt's
    constraint that we not conflict with the state-machine-resolution point,
    while still giving the task its required "clear state-policy lookup
    behavior for inbox work." Without this split, the two points race; with
    it, they compose.
  - On **D-003**, codex's explicit framing matches operator reality: any
    `<rhei>.<local>` id scheme forces a rename on cross-rhei moves, and
    pretending otherwise would either require a second global id namespace
    (contradicting A-003) or silent id rewriting (a bigger surprise than a
    documented rename). Stating it openly in this point closes the question
    without spawning a separate filing decision.
