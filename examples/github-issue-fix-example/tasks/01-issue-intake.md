### Task issue-intake: Analyze and route issue 1234
**State:** issue-intake

Create the issue worktree, fetch `vjovanov/rhei` issue `1234`, discover the
target repository's contributor and grounding instructions, analyze whether the
requested change fits the repository's goals/specs/non-goals/decisions, and
write exactly one follow-up task file under `tasks/`.

The follow-up task must start in one of these states:

- `implement-fix` when the issue is compatible and no human gate is required.
- `human-review` when the issue is compatible but human review is required.
- `github-handoff` when the issue conflicts with repo guidance, is too vague or
  underspecified to implement safely, lacks required information, or needs an
  external/product decision before implementation.

Use the configured publication mode `no-pr`. Do not perform any
external GitHub writes when it is `no-pr`: do not push, open or update a PR, or
post or update issue comments.


