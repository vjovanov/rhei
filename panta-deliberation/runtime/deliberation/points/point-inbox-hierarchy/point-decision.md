# Point Decision - Resolve Inbox ticket placement in the Panta hierarchy

- Chosen solution: Model the project inbox as a reserved synthetic level-1
  rhei with id `inbox`. Inbox tickets are ordinary level-2-or-deeper ticket
  nodes under that rhei, with project ids such as `inbox.<local-id>` and
  `inbox.<local-id>.<child-id>`. Top-level inbox tickets have the synthetic
  inbox rhei as parent; nested inbox tickets have their authored parent ticket.
  The synthetic inbox rhei is materialized only when inbox content exists, has
  no authored `index.rhei.md`, and carries no stored state. The id `inbox` is
  always reserved at the project rhei-id level, so any discovered domain rhei
  with id `inbox` is a validation/load error. Inbox tickets use the ordinary
  rhei-ticket state-policy lookup: first bind the effective state machine using
  the generic state-machine resolution rule for a rhei without its own
  `**States:**` declaration, then resolve the ticket policy through
  `node_policy.overrides`, `node_policy.by_type[<kind>]`, and
  `node_policy.default`. Moving an inbox ticket into a domain rhei is a
  reparenting/id-change operation from `inbox.<local-id>` to
  `<target-rhei>.<local-id>`, while reference rewrite, alias, and artifact
  migration guarantees remain outside this point.
- Why chosen: This is stronger than the alternatives because it preserves the
  non-negotiable Panta -> rhei -> ticket hierarchy, avoids a special Panta-level
  ticket namespace, and reuses the same id, validation, execution, artifact,
  and state-policy paths as ordinary rhei work. Unconditionally reserving
  `inbox` gives validation one stable language-level namespace rule instead of
  making legality depend on whether quick-capture content currently exists.
  Stating ticket-level policy lookup here gives inbox work clear behavior
  without conflicting with the separate state-machine-resolution point, because
  this decision does not choose the generic precedence between Panta defaults
  and built-in fallbacks. Making promotion an id-changing reparenting
  consequence is the honest result of the chosen `<rhei>.<local>` id scheme;
  leaving that unstated would hide an operator-visible effect.
- Alternatives considered: A conditional reservation rule for `inbox` was
  viable but weaker: it would allow a domain rhei named `inbox` until the inbox
  feature appears, creating a delayed migration trap and a conditional
  validation rule. A direct Panta-child inbox ticket model was rejected because
  it violates the hierarchy constraint that level-1 Panta children are rheis
  and tickets are level 2 or deeper. A separate inbox-specific state-policy tier
  was rejected because it duplicates ordinary rhei-ticket machinery and risks
  conflicting with state-machine resolution. Deferring every promotion/id-change
  implication was rejected because the canonical inbox id is part of this
  hierarchy decision, but detailed reference rewrites and alias guarantees are
  left for a promotion/refactoring decision.
- Remaining uncertainty: The separate state-machine-resolution decision still
  owns the generic rule for selecting the effective state machine when a rhei,
  including the synthetic inbox rhei, has no local `**States:**` declaration.
  A later promotion/refactoring decision should define whether dependencies,
  artifacts, logs, and external references are rewritten, aliased, or migrated
  when an inbox ticket is filed into a domain rhei.
- Effect on final solution: The final answer must treat inbox as a reserved
  synthetic level-1 rhei, never as a direct Panta ticket container. Inbox ticket
  ids, parents, levels, validation, and state-policy lookup must follow ordinary
  rhei-ticket rules, with no separate inbox namespace or policy tier. Any
  filing workflow must account for the fact that moving work out of `inbox`
  changes its canonical project id.
