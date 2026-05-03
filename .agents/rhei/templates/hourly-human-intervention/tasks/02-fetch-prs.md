### Task fetch-prs: Fetch and classify human-intervention pull requests
**State:** fetch

Fetch every open GitHub pull request in `{{repo}}` labeled `{{label}}`.

Use GitHub as the source of truth. Include the PR number, title, URL, labels,
assignees, author, body, review state, CI state, head branch, changed files,
recent comments, review comments, and any Forge human-intervention comment or
log reference.

Classify pull requests into actionable root-cause classes. Prefer classes that
share one investigation and one fix path, such as:

- CI failure PRs from GitHub Actions or status checks; create one child
  `ci-failure-triage` task per PR so each can be restarted and rechecked
- PR needs metadata, test, or Forge workflow changes in `{{forge_checkout}}`
- PR exposes a Forge automation defect in `{{forge_checkout}}/forge`
- PR exposes Native Image or GraalVM behavior in `{{graalvm_ce_checkout}}` or `{{graalvm_ee_checkout}}`
- PR needs human product/reviewer input on GitHub before automation continues

Append one child task under this task for each non-empty non-CI class, using
state `deep-analysis` and `**Prior:** Task fetch-prs`. Append one child task
per CI failure PR using state `ci-failure-triage` and `**Prior:** Task
fetch-prs`.
