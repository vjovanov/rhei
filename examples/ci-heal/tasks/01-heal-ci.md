### Task heal-ci: Watch CI and heal the branch
**State:** ci-watch

Poll GitHub CI for this task's `branch` metadata, declared under
`metadata.tasks.heal-ci` in `index.rhei.md` — that block is what `{meta.branch}`
reads; a `metadata:` block written into a task body is prose and is never
parsed. While at least one check is still running, stay in `ci-watch` (the
`poll:` block releases the slot between attempts). On a failing verdict, the task
transitions to `analyze-and-fix`, an agent writes the smallest fix, and
`push-fix` commits and pushes; the task then re-enters `ci-watch` with
fresh poll counters. Terminal when either every check is green
(`heal-done`) or the poll or fix budgets are exhausted
(`poll-gave-up` / `fix-exhausted`).
