# Review Fix Visits Example

This workspace demonstrates a custom state machine with a counted review loop
that produces a review artifact before running a fix step. It also showcases
template variables in state instructions — `{task_id}`, `{visit_count}`,
`{visits}`, `{input.<name>.path}`, and `{output.<name>.path}` — which
`rhei next` resolves before printing to the agent.

The flow is:

1. `review-loop` starts in `review`.
2. The workflow begins in `review`, which declares `visits: 2`.
3. The `append-review` callback runs on every exit from `review` and appends to
   `runtime/reviews/task-review-loop-review.md`.
4. After each review pass the task transitions to `fix`.
5. The `fix` state also declares `visits: 2`, so the first fix pass returns to
   `review-2` and the second fix pass transitions to `completed`.
6. The `write-fix` callback reads the accumulated review artifact and updates
   `runtime/fixes/task-review-loop-fix.md` on each exit from `fix`.

Validate the workspace from the repository root:

```bash
cargo run -p rhei-cli -- --state-machine examples/review-fix-visits/states.yaml validate examples/review-fix-visits
```

Run the example in a disposable copy so runtime artifacts stay untracked:

```bash
tmp_dir="$(mktemp -d)"
cp -R examples/review-fix-visits "$tmp_dir/review-fix-visits"
cargo run -p rhei-cli -- --state-machine "$tmp_dir/review-fix-visits/states.yaml" run "$tmp_dir/review-fix-visits"
```
