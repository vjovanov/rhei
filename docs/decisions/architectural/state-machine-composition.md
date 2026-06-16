# DA-state-machine-composition: State machines compose via explicit `extends`

## Status

proposed

## Context

Under Panta, the project supplies a default state machine that every rhei
inherits, and a rhei may override it by declaring its own `**States:**`
(§AR-rhei-panta.4, §FS-rhei-panta). The resolver treats inheritance as a
*default, not a merge*: an overriding rhei replaces the whole machine. The
deliberation that settled resolution order deliberately rejected merging to keep
resolution deterministic and non-search-based (point-state-machine-resolution).

Wholesale replacement has a real cost. A rhei that needs one extra phase — say a
`security-review` on top of an otherwise shared project workflow — must copy the
entire project machine and maintain it in parallel. The copies drift from the
project default over time, which undermines the point of a per-Panta default and
makes a project's effective behavior hard to reason about. §GOAL-rhei-outcomes

## Decision

Add an explicit, opt-in composition mechanism: a machine may declare a top-level
`extends: <base-machine-name>`. The effective machine is the union of the base
chain (`rhei` built-in ⊂ Panta default ⊂ rhei override) with the declaring
machine layered on top. Without `extends`, a declared machine fully replaces the
inherited default exactly as before — composition never happens implicitly.
§FS-rhei-states.12

The atomic unit of override is the **named entity** — one state, one transition,
one profile — never an individual field. A higher layer adds new entities and
replaces same-named ones wholesale; collections (`states`, `transitions`,
`profiles`, `node_policy` keys, `models`) compose by union. Validation runs on
the merged machine, and diagnostics report each entity's provenance.

Ownership is fixed so the project keeps the shared rollup: the Panta root always
resolves `node_policy.root` from the project default machine; a rhei's node and
tickets resolve through that rhei's effective (composed) machine.

## Alternatives considered

- **Keep wholesale replacement only.** Simplest and fully deterministic, but
  forces copy-paste of the project machine for any rhei tweak and lets copies
  drift. Rejected because the drift cost grows with every overriding rhei.

- **Implicit layering.** A rhei machine always layers on the resolved Panta
  default. Most ergonomic, but the same rhei file would resolve to a different
  effective machine standalone versus inside a project, reintroducing the
  context-dependent, search-like behavior the resolution decision rejected.
  Rejected in favour of explicit `extends`, which names the base and stays local.

- **Field-level deep merge.** A higher layer overrides individual fields of an
  inherited state. Maximally terse, but breaks the wholesale-by-name philosophy
  the profiles model already relies on (§FS-rhei-states.8) and makes "what is the
  effective state?" hard to answer. Rejected for whole-entity override.

## Consequences

- **Revises, does not contradict, the resolution decision.** Inheritance of the
  default remains a non-merging default; composition is a separate, explicit
  layer triggered only by `extends`.
- **Merge-time validation and provenance.** The validator must fold the chain
  before checking reachability, terminal coverage, and node-policy validity, and
  must attribute each entity to its source layer for diagnostics.
- **Cycle and termination rules.** The `extends` graph must be acyclic, finite,
  and rooted at the built-in `rhei`; violations are validation errors.
- **Transition removal.** Because transitions compose by union, a higher layer
  needs an explicit `remove:` directive to drop an inherited transition.
