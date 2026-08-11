### Task fetch-issues: Fetch and classify human-intervention issues
**State:** fetch

Fetch every open GitHub issue in `{{repo}}` labeled `{{label}}`.

Use GitHub as the source of truth. Include the issue number, title, URL,
labels, assignees, author, body, recent comments, linked pull requests when
available, and any Forge human-intervention comment or log reference.

Classify issues into actionable root-cause classes. Prefer classes that share
one investigation and one fix path, such as:

- CI failure issues from GitHub Actions or status checks; create one child
  `ci-failure-triage` task per issue so each can be restarted and rechecked
- metadata or test change in `{{forge_checkout}}`
- Forge automation defect in `{{forge_checkout}}/forge`
- Native Image or GraalVM behavior in `{{graalvm_ce_checkout}}` or `{{graalvm_ee_checkout}}`
- missing external information or product decision requiring a human GitHub issue/comment

Append one child task under this task for each non-empty non-CI class, using
state `deep-analysis` and `**Prior:** Task fetch-issues`. Append one child
task per CI failure issue using state `ci-failure-triage` and `**Prior:** Task
fetch-issues`.
