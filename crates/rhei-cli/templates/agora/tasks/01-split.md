### Task split: Split discussion into decision points
**State:** split-points

Discussion title: {{discussion_title}}

Input discussion:

```text
{{discussion}}
```

Break the discussion into separate points that can be resolved independently.
Prefer fewer, sharper points over many overlapping ones. Use at most
{{max_points}} points unless the input contains clearly independent decisions
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
### Task final-solution: Produce final solution for {{discussion_title}}
**State:** final-solution
**Prior:** Task point-<slug-1>, Task point-<slug-2>

Synthesize the resolved point decisions into one final solution and prepare the
human-facing presentation.
```

The final task's `**Prior:**` list must include every concrete point task id you
created and no placeholders.
