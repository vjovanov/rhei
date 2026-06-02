# Disagreement Map - Resolve Inbox ticket placement in the Panta hierarchy

## Candidate Solutions

- S-001: Model the project inbox as a reserved synthetic level-1 rhei with id
  `inbox`; model all inbox tickets as ordinary ticket/task nodes at level 2 or
  deeper under that rhei, with project ids like `inbox.<local-id>` and parents
  resolved to either the synthetic inbox rhei or an authored parent ticket.
  - Proposed by: claude-code[yolo]:anthropic:claude-opus-4-7,
    codex[yolo]:openai:gpt-5.5
  - Reasons: Preserves the Panta -> rhei -> ticket hierarchy, avoids making
    tickets direct children of Panta, reuses the normal `<rhei>.<local>` id
    shape, and lets inbox work use the same validation, execution, artifact,
    dependency, and state-policy paths as ordinary rhei work.

## Agreements

- A-001: Panta remains level 0; level-1 children under Panta remain rheis; inbox
  tickets are never direct children of Panta.
- A-002: The inbox should be represented as a level-1 rhei-like node with the
  reserved id `inbox`.
- A-003: Inbox ticket ids should be project-scoped through the inbox rhei, such
  as `inbox.3` and `inbox.3.1`, instead of using a flat Panta-level ticket id.
- A-004: Top-level inbox tickets have the synthetic inbox rhei as their parent;
  nested inbox tickets have their authored parent ticket.
- A-005: The inbox rhei may be synthetic and need not have an authored
  `index.rhei.md`; inbox tickets are authored in normal task/ticket format under
  an optional `inbox/` workspace area.
- A-006: Inbox tickets should inherit or resolve state policy through the same
  machinery used by ordinary rhei tickets, rather than through a separate inbox
  policy tier.

## Disagreements

- D-001: Whether `inbox` is an unconditional reserved rhei id or reserved only
  when project inbox capture is present.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 requires `inbox` to be
    a reserved rhei id and treats `rheis/inbox` as a load-time error; codex[yolo]
    says users cannot create a domain rhei named `inbox` while the project inbox
    exists, leaving the no-inbox case less explicit.
  - Options: Always reserve `inbox` at the project rhei-id level; reserve
    `inbox` only when an inbox feature or `inbox/` directory is active.
  - Why it matters: This determines whether existing or future domain rheis can
    be named `inbox` in projects that do not use quick capture, and where
    validation must report collisions.
  - Evidence needed: The intended reserved-id rules for Panta/rhei ids, whether
    optional features may reserve ids even when unused, and any compatibility
    requirement for existing projects with a normal rhei named `inbox`.

- D-002: How much state-policy behavior belongs in this hierarchy resolution.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 gives a full lookup rule
    for inbox tickets: inherit the Panta default or built-in fallback, then apply
    `node_policy.overrides -> node_policy.by_type[<kind>] -> node_policy.default`,
    while treating the synthetic inbox rhei as a structural rollup with no
    stored state. codex[yolo]:openai:gpt-5.5 agrees with inheritance in
    principle but leaves rejection possible if the final state-policy decision
    rejects inheritance from `index.panta.md` for synthetic rheis.
  - Options: Normatively define the complete inherited ticket policy lookup here;
    define only that inbox tickets use ordinary rhei-ticket lookup and defer
    exact precedence/fallback behavior to the state-machine-resolution point.
  - Why it matters: The task requires clear state-policy lookup behavior for
    inbox work, but over-specifying precedence here could conflict with the
    separate state-machine-resolution decision.
  - Evidence needed: The current or accepted state-machine-resolution rule,
    whether synthetic rheis without `index.rhei.md` may inherit `index.panta.md`,
    and whether structural rhei nodes are expected to have claimable/stored
    state.

- D-003: Whether moving an inbox ticket into a domain rhei necessarily changes
  its project id.
  - Agents: codex[yolo]:openai:gpt-5.5 explicitly treats filing an inbox ticket
    into a domain rhei as a reparenting/id-change operation; claude-code[yolo]
    implies this through the `inbox.<local-id>` id scheme but does not make
    promotion semantics part of the recommendation.
  - Options: State that promotion from inbox to a domain rhei changes the
    project id from `inbox.<local-id>` to `<target-rhei>.<local-id>`; leave
    promotion/id stability out of this point and handle it in a separate
    filing/refactoring decision.
  - Why it matters: Stable external references, dependencies, artifacts, logs,
    and operator expectations may depend on whether inbox ids are permanent or
    temporary capture ids.
  - Evidence needed: Existing id stability requirements, dependency/reference
    rewrite behavior, and whether inbox filing is considered a hierarchy move or
    a separate migration workflow.

## Discussion Prompt

Both proposals converge on S-001: inbox is a synthetic level-1 rhei named
`inbox`, and inbox tickets are normal level-2-or-deeper ticket nodes under it.
Please address only D-001 through D-003: decide whether `inbox` is always
reserved or conditionally reserved, define the exact state-policy lookup without
conflicting with the state-machine-resolution point, and decide whether inbox
promotion must be specified here as an id-changing move. Converge where possible
and identify any evidence that still blocks a final resolution.
