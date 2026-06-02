### Task point-inbox-hierarchy: Resolve Inbox ticket placement in the Panta hierarchy
**State:** completed
**Prior:** Task split

Resolve how inbox tickets fit into the Panta hierarchy without merging this
with dependency readiness or documentation decisions.

Source evidence:

> The current Panta specs must not make tickets direct children of Panta if
> the plan-language hierarchy says level-1 children under Panta are rheis and
> tickets are level 2 or deeper.

> The model needs clear ids, levels, and state-policy resolution for inbox work.

Question:

How should inbox tickets be identified, leveled, parented, and assigned state
policy within the Panta hierarchy?

Constraints:

- Level-1 children under Panta must remain rheis if that is what the
  plan-language hierarchy defines.
- Inbox tickets must be modeled as level 2 or deeper, not as direct children of
  Panta.
- The resolution must define clear ids, hierarchy levels, and state-policy
  lookup behavior for inbox work.
