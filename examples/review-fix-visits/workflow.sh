#!/usr/bin/env bash

set -euo pipefail

command_name="${1:-}"

if [[ -z "${RHEI_PLAN_PATH:-}" ]]; then
    echo "RHEI_PLAN_PATH is required" >&2
    exit 1
fi

if [[ -d "$RHEI_PLAN_PATH" ]]; then
    workspace_root="$RHEI_PLAN_PATH"
else
    workspace_root="$(dirname "$RHEI_PLAN_PATH")"
fi

runtime_dir="$workspace_root/runtime"
logs_dir="$runtime_dir/logs"
reviews_dir="$runtime_dir/reviews"
fixes_dir="$runtime_dir/fixes"

mkdir -p "$logs_dir" "$reviews_dir" "$fixes_dir"

team_log="$logs_dir/team.log"
task_log="$logs_dir/task-${RHEI_TASK_ID:-unknown}.log"
review_file="$reviews_dir/task-${RHEI_TASK_ID:-unknown}-review.md"
fix_file="$fixes_dir/task-${RHEI_TASK_ID:-unknown}-fix.md"

timestamp() {
    date -Iseconds
}

log_line() {
    local message="$1"
    printf '%s task=%s %s -> %s %s\n' \
        "$(timestamp)" \
        "${RHEI_TASK_ID:-unknown}" \
        "${RHEI_FROM_STATE:-unknown}" \
        "${RHEI_TO_STATE:-unknown}" \
        "$message" | tee -a "$team_log" >> "$task_log"
}

review_pass_count() {
    if [[ ! -f "$review_file" ]]; then
        printf '0\n'
        return 0
    fi

    grep -c '^## Review pass ' "$review_file"
}

append_review() {
    local current_pass
    current_pass="$(review_pass_count)"
    local next_pass=$((current_pass + 1))

    log_line "appended review pass ${next_pass}"

    if [[ ! -f "$review_file" ]]; then
        cat > "$review_file" <<EOF
# Review Artifact for Task ${RHEI_TASK_ID:-unknown}

This file is appended once per exit from the counted \`review\` state.
EOF
    fi

    cat >> "$review_file" <<EOF

## Review pass ${next_pass}

- Transition: ${RHEI_FROM_STATE:-unknown} -> ${RHEI_TO_STATE:-unknown}
- Observation: review pass ${next_pass} captured a concrete finding for the fix step.
- Output file: runtime/reviews/task-${RHEI_TASK_ID:-unknown}-review.md
EOF
}

write_fix() {
    log_line "updated fix artifact from review file"

    if [[ ! -f "$review_file" ]]; then
        echo "missing review artifact for task ${RHEI_TASK_ID:-unknown}" >&2
        exit 1
    fi

    local pass_count
    pass_count="$(review_pass_count)"
    if [[ "$pass_count" -lt 1 || "$pass_count" -gt 2 ]]; then
        echo "expected 1 or 2 review passes, found ${pass_count}" >&2
        exit 1
    fi

    cat > "$fix_file" <<EOF
# Fix Artifact for Task ${RHEI_TASK_ID:-unknown}

Source artifact: runtime/reviews/task-${RHEI_TASK_ID:-unknown}-review.md
Review passes consumed: ${pass_count}

## Applied fix

- Read the shared review artifact.
- ${pass_count} review pass(es) were available when this fix step ran.
- Produced the current fix artifact revision from the accumulated review findings.
EOF
}

cancel_task() {
    log_line "task cancelled"
    cat > "$fix_file" <<EOF
# Cancelled Task ${RHEI_TASK_ID:-unknown}

The workflow stopped before completion at $(timestamp).
EOF
}

case "$command_name" in
    append-review)
        append_review
        ;;
    write-fix)
        write_fix
        ;;
    cancel)
        cancel_task
        ;;
    *)
        echo "unknown workflow command: $command_name" >&2
        exit 1
        ;;
esac
