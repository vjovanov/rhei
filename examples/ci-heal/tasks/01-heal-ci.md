### Task heal-ci: Watch CI and heal the branch
**State:** ci-watch

metadata:
  branch: feature/retry-cleanup

Poll GitHub CI for the branch declared in `metadata.branch`. While at
least one check is still running, stay in `ci-watch` (the `poll:` block
releases the slot between attempts). On a failing verdict, the task
transitions to `analyze-and-fix`, an agent writes the smallest fix, and
`push-fix` commits and pushes; the task then re-enters `ci-watch` with
fresh poll counters. Terminal when either every check is green
(`heal-done`) or the poll or fix budgets are exhausted
(`poll-gave-up` / `fix-exhausted`).
