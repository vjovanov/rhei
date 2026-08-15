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

# `ci-watch` can route straight into `heal-done` (green) or `poll-gave-up`
# (budget spent), and a `final: true` state is not entered without a result.
# The program is the worker here, so it records the probe's verdict on every
# attempt; whichever attempt ends the ticket, the reason is already written.
# §FS-rhei-states.3.3 §FS-rhei-programs.2
record_result() {
  [[ -n "${RHEI_RESULT_PATH:-}" ]] || return 0
  mkdir -p "$(dirname "$RHEI_RESULT_PATH")"
  printf '## Result\n\n%s Report: `%s`.\n' "$1" "$REPORT_PATH" >"$RHEI_RESULT_PATH"
}

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
  record_result "No CI run found for \`$BRANCH\` within the poll budget."
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
  record_result "CI on \`$BRANCH\` at \`$sha\` never returned a verdict (last status: $status)."
  exit 75
fi

case "$conclusion" in
  success)
    record_result "CI is green on \`$BRANCH\` at \`$sha\`."
    exit 0
    ;;
  *)
    record_result "CI failed on \`$BRANCH\` at \`$sha\` ($conclusion)."
    exit 1  # failure, cancelled, timed_out, action_required, ...
    ;;
esac
