#!/usr/bin/env bash
# commit-and-push.sh — commit the agent's fix and push to the branch
# under observation. Paired with the `push-fix` state in states.yaml.
#
# Inputs (environment):
#   BRANCH        — branch to push to (required)
#   SUMMARY_PATH  — path to the fix-summary markdown the agent wrote
#                   (required; used as the commit body)
#
# Exit 0 on success; non-zero on any git failure.

set -euo pipefail

: "${BRANCH:?BRANCH is required}"
: "${SUMMARY_PATH:?SUMMARY_PATH is required}"

if [[ ! -s "$SUMMARY_PATH" ]]; then
  echo "commit-and-push: fix summary missing or empty: $SUMMARY_PATH" >&2
  exit 2
fi

current_branch="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$current_branch" != "$BRANCH" ]]; then
  echo "commit-and-push: expected branch '$BRANCH', on '$current_branch'" >&2
  exit 2
fi

if git diff --quiet && git diff --cached --quiet; then
  echo "commit-and-push: nothing to commit; skipping push." >&2
  exit 0
fi

first_line="$(head -n 1 "$SUMMARY_PATH")"
subject="${first_line#\# }"
subject="ci-heal: ${subject:-apply fix}"

git add -A
git commit --file=- <<EOF
$subject

$(cat "$SUMMARY_PATH")
EOF

git push origin "$BRANCH"
