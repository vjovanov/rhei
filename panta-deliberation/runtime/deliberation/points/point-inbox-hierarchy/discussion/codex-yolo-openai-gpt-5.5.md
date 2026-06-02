# Discussion Response - codex[yolo]:openai:gpt-5.5

- Changed position: yes. I now support treating `inbox` as an unconditional
  reserved project rhei id, not only as unavailable while an `inbox/` capture
  directory exists. I also agree the state-policy behavior can be stated more
  concretely here, provided this point references the ordinary state-machine
  resolution rule instead of creating a second inbox-specific precedence table.
- Remaining disagreements: D-003 only, and only as a boundary question. This
  point should define the canonical id while a ticket is in the inbox, but the
  filing workflow, reference rewrites, artifact migration, and external id
  stability guarantees belong in a separate promotion/refactoring decision.
  D-001 and D-002 should be resolved by this recommendation.
- Concessions: claude-code[yolo]:anthropic:claude-opus-4-7 is right that my
  original wording left the no-inbox case too vague. Conditional reservation
  makes validation depend on whether a feature is currently present, which is
  a weak rule for a language-level namespace. They are also right that the
  synthetic inbox rhei should be a structural rollup with no stored state, and
  that tickets beneath it should use the same `node_policy.overrides ->
  node_policy.by_type[<kind>] -> node_policy.default` ticket lookup used by
  ordinary rhei tickets.
- Final recommendation for this point: adopt S-001 with these clarifications.
  `inbox` is always a reserved level-1 rhei id in a Panta project. A discovered
  domain rhei with id `inbox` under `rheis/` is a validation/load error even
  when no `inbox/` directory is present. The synthetic inbox rhei is only
  materialized when project inbox content exists, has parent Panta, has level 1,
  has no authored `index.rhei.md`, and carries no stored state. Top-level inbox
  tickets have parent `inbox`, level 2, and project ids `inbox.<local-id>`;
  nested inbox tickets have their authored parent ticket, deeper levels, and
  ids such as `inbox.<local-id>.<child-id>`. Inbox tickets resolve state policy
  exactly as ordinary tickets in a rhei whose `**States:**` declaration is
  omitted: first bind the effective state machine using the state-machine
  resolution rule for such rheis, then resolve the ticket profile with
  `node_policy.overrides`, then `node_policy.by_type[<kind>]`, then
  `node_policy.default`. No separate inbox policy tier is introduced.
- Reason: This handles D-001 by making the namespace stable and easy to
  validate: `inbox` is a language-reserved rhei id, not a user rhei id that
  changes legality when quick capture appears. It handles D-002 by giving inbox
  work a clear lookup rule while still delegating the generic precedence between
  rhei-local declarations, Panta defaults, and built-in fallback to the
  state-machine-resolution point. It handles D-003 by separating the hierarchy
  invariant from filing semantics: while a ticket is under the inbox its
  canonical project id is `inbox.<local-id>`; if another decision defines
  promotion as a move into a domain rhei, the normal namespaced-id model implies
  a new `<target-rhei>.<local-id>` canonical id, but this point should not also
  decide reference-rewrite or id-alias guarantees.
