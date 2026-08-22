#!/bin/sh
# Mock supervisor and worker for the subtree-supervision example.
#
# Stands in for a real coding agent so the example runs with no credentials:
# every state writes the artifact its contract declares, and the `supervise`
# state writes the brief the next step reads. `rhei run` does the rest — the
# hold/release barrier, the checkpoints, and the edge selection are the engine's.
set -eu

# The agent runs with the *repository* as its cwd, so resolve the workspace
# from RHEI_PLAN_PATH the way every callback in these examples does.
root="${RHEI_PLAN_PATH:-.}"
[ -f "$root" ] && root="$(dirname "$root")"
task="${RHEI_TASK_ID:-unknown}"
state="${RHEI_STATE:-unknown}"
visit="${RHEI_VISIT_COUNT:-1}"

mkdir -p "$root/runtime/logs" "$root/runtime/review" "$root/runtime/supervise"
printf 'task=%s state=%s visit=%s\n' "$task" "$state" "$visit" \
  >> "$root/runtime/logs/subtree-supervision.log"

# Every worker records why its ticket ends where it does; a `final: true` state
# is not entered until that file has content.
if [ -n "${RHEI_RESULT_PATH:-}" ]; then
  mkdir -p "$(dirname "$RHEI_RESULT_PATH")"
fi

case "$state" in
  review)
    printf '# Findings for %s\n\n- overflow on a 64-bit literal\n' "$task" \
      > "$root/runtime/review/$task.md"
    printf '## Result\n\nReviewed %s; findings recorded.\n' "$task" \
      > "$RHEI_RESULT_PATH"
    ;;
  fix)
    printf '## Result\n\nApplied the briefed fixes for %s.\n' "$task" \
      > "$RHEI_RESULT_PATH"
    ;;
  supervise)
    # One brief per visit, aimed at the child the release edge lets run next.
    # A real supervisor picks the target off `## Checkpoints`; the mock walks
    # the chain in order.
    case "$visit" in
      1) child="1.1" ;;
      2) child="1.2" ;;
      3) child="1.3" ;;
      4) child="1.4" ;;
      *) child="" ;;
    esac
    if [ -n "$child" ]; then
      printf 'Brief from the supervisor (visit %s): stay inside what the review asked for.\n' \
        "$visit" > "$root/runtime/supervise/${task}.${child#1.}.md"
    fi
    # Only the visit that finds the subtree closed writes a result; on every
    # other visit the engine takes the unconditional self-loop and releases.
    if [ -z "$child" ]; then
      printf '## Result\n\nSupervised %s across %s visits; every child is terminal.\n' \
        "$task" "$visit" > "$RHEI_RESULT_PATH"
    fi
    ;;
esac
