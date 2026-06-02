### Task split: Split discussion into decision points
**State:** completed

Discussion title: Resolve Panta project spec conflicts

Input discussion:

```text
We need resolve the Panta spec discussion around:

1. How inbox tickets fit the Panta hierarchy.
   The current Panta specs must not make tickets direct children of Panta if
   the plan-language hierarchy says level-1 children under Panta are rheis and
   tickets are level 2 or deeper. The model needs clear ids, levels, and
   state-policy resolution for inbox work.

2. How dependency readiness works across rheis.
   Cross-rhei dependencies must use the same readiness rule as normal
   scheduling: a prior must be in a successful terminal state, meaning
   `final: true` and not normalized `cancelled`. A cancelled prerequisite
   should not unblock dependent work.

3. How Panta project defaults affect state-machine resolution.
   If a rhei inside a project omits `**States:**`, `index.panta.md` may supply
   the project default. The state-machine resolution rules need one
   non-conflicting lookup order for explicit overrides, rhei-local
   declarations, inherited Panta defaults, and built-in fallback.

4. How the canonical language reference should expose Panta syntax.
   `index.panta.md`, Panta `rheis/` entries, and optional `inbox/` task files
   are user-authored language surface and should be discoverable from the
   canonical language reference.

The goal is one coherent normative model for validation, execution, ids,
state-policy lookup, project defaults, and user-facing documentation.

```

Break the discussion into separate points that can be resolved independently.
Prefer fewer, sharper points over many overlapping ones. Use at most
4 points unless the input contains clearly independent decisions
that would be unsafe to merge.

For every point, append one task file under `tasks/` named
`NN-point-<slug>.md`:

```markdown
### Task point-<slug>: Resolve <point title>
**State:** propose-solutions
**Prior:** Task split

<neutral statement of the point, the relevant source text, and constraints>
```

After all point tasks are created, append one final task file under `tasks/`
named `99-final-solution.md`:

```markdown
### Task final-solution: Produce final solution for Resolve Panta project spec conflicts
**State:** final-solution
**Prior:** Task point-<slug-1>, Task point-<slug-2>

Synthesize the resolved point decisions into one final solution and prepare the
human-facing presentation.
```

The final task's `**Prior:**` list must include every concrete point task id you
created and no placeholders.
