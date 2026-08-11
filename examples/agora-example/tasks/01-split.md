### Task split: Split discussion into decision points
**State:** split-points

Discussion title: Choose the default export format

Input discussion:

```text
The team needs to choose the default export format for generated reports.
One side wants Markdown because it is easy for agents and humans to review in
Git. Another side wants HTML because stakeholders can open it directly and it
supports richer layout. A third concern is that CI should be able to compare
outputs without noisy formatting diffs. The decision should keep the default
simple while leaving room for richer exports later.

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
### Task final-solution: Produce final solution for Choose the default export format
**State:** final-solution
**Prior:** Task point-<slug-1>, Task point-<slug-2>

Synthesize the resolved point decisions into one final solution and prepare the
human-facing presentation.
```

The final task's `**Prior:**` list must include every concrete point task id you
created and no placeholders.
