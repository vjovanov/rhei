# changeset-review — example

A pre-rendered instantiation of the [`changeset-review`](../../.agents/rhei/templates/changeset-review/)
template used as a smoke test that the template produces a valid workspace.

Inputs used when rendering (chosen to exercise every optional branch —
`review_focus`, `fix_prepare`, and `fix_commit`):

| Input | Value |
|---|---|
| `change_ref` | `PR#42` |
| `review_targets` | `["claude-code[yolo]:anthropic:claude-opus-4-7", "codex[yolo]:openai:gpt-5.4"]` |
| `review_focus` | `["performance", "security", "concurrency"]` |
| `aggregator_target` | `claude-code[yolo]:anthropic:claude-opus-4-7` (default) |
| `fix_target` | `claude-code[yolo]:anthropic:claude-opus-4-7` (default) |
| `fix_prepare` | `worktree` |
| `fix_commit` | `pr` |

Validate from the repository root:

```bash
cargo run -p rhei-cli -- validate examples/changeset-review-example
```

To regenerate this example from the template:

```bash
rm -rf examples/changeset-review-example
cargo run -p rhei-cli -- instantiate changeset-review \
  --set change_ref=PR#42 \
  --set review_targets='["claude-code[yolo]:anthropic:claude-opus-4-7","codex[yolo]:openai:gpt-5.4"]' \
  --set review_focus='["performance","security","concurrency"]' \
  --set fix_prepare=worktree \
  --set fix_commit=pr \
  --output examples/changeset-review-example
```

The example also ships `.rhei/settings.json`, which provides the default
`agent_timeout` required for orchestrated target execution. The checked-in
seed remains otherwise static: `index.rhei.md`, `states.yaml`,
`.rhei/settings.json`, and `tasks/01-coordinate.md`. At run time the
coordinator appends the per-part review tasks (code parts plus
`pr-description`, `commit-messages`, and — depending on its decisions —
`documentation` and `spec`) and an aggregator task under `tasks/`; those
are not checked in.
