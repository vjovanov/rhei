#!/usr/bin/env bash
# gh-ci-status.sh — tri-state GitHub CI status probe for rhei `ci-watch`.
#
# Contract (see ../index.rhei.md §Status-check contract):
#   exit 0  — every required check passed
#   exit 1  — at least one required check failed
#   exit 75 — checks are still running; retry after poll.interval
#
# Inputs (environment):
#   BRANCH       — branch under observation (required)
#   REPORT_PATH  — where to write the JSON report (required; from
#                  `{output.ci-report.path}` in states.yaml)
#
# Requires: gh (authenticated), jq.

set -euo pipefail

: "${BRANCH:?BRANCH is required}"
: "${REPORT_PATH:?REPORT_PATH is required}"

mkdir -p "$(dirname "$REPORT_PATH")"

# Latest run on this branch. `gh run list` returns most recent first.
run_json="$(gh run list \
  --branch "$BRANCH" \
  --limit 1 \
  --json databaseId,headSha,status,conclusion)"

if [[ "$(jq 'length' <<<"$run_json")" -eq 0 ]]; then
  jq -n --arg branch "$BRANCH" \
    '{branch: $branch, sha: null, jobs: [], note: "no runs found"}' \
    >"$REPORT_PATH"
  # No run yet — treat as still-pending so the poll loop keeps waiting.
  exit 75
fi

run_id="$(jq -r '.[0].databaseId' <<<"$run_json")"
sha="$(jq -r '.[0].headSha' <<<"$run_json")"
status="$(jq -r '.[0].status' <<<"$run_json")"
conclusion="$(jq -r '.[0].conclusion' <<<"$run_json")"

jobs_json="$(gh run view "$run_id" --json jobs \
  | jq '[.jobs[] | {name: .name, status: .conclusion // .status, log_url: .url}]')"

jq -n \
  --arg branch "$BRANCH" \
  --arg sha "$sha" \
  --argjson jobs "$jobs_json" \
  '{branch: $branch, sha: $sha, jobs: $jobs}' \
  >"$REPORT_PATH"

# Still running: queued | in_progress | waiting | requested | pending
if [[ "$status" != "completed" ]]; then
  exit 75
fi

case "$conclusion" in
  success) exit 0 ;;
  *)       exit 1 ;;  # failure, cancelled, timed_out, action_required, ...
esac
