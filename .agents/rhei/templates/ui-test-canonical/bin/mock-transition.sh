#!/usr/bin/env bash

set -euo pipefail

command_name="${1:-log}"

if [[ -z "${RHEI_PLAN_PATH:-}" ]]; then
    echo "RHEI_PLAN_PATH is required" >&2
    exit 1
fi

if [[ -d "$RHEI_PLAN_PATH" ]]; then
    workspace_root="$RHEI_PLAN_PATH"
    plan_file="$workspace_root/index.rhei.md"
    tasks_dir="$workspace_root/tasks"
else
    workspace_root="$(dirname "$RHEI_PLAN_PATH")"
    plan_file="$RHEI_PLAN_PATH"
    tasks_dir=""
fi

task_id="${RHEI_TASK_ID:-unknown}"
from_state="${RHEI_FROM_STATE:-unknown}"
to_state="${RHEI_TO_STATE:-unknown}"
transition_root="$workspace_root/runtime/transitions"
include_generated="{{ include_generated_followup }}"

mkdir -p "$transition_root"

timestamp() {
    date -Iseconds
}

safe_task_id() {
    printf '%s' "$task_id" | tr -c 'A-Za-z0-9_-' '-'
}

log_transition() {
    printf '%s task=%s %s -> %s command=%s\n' \
        "$(timestamp)" "$task_id" "$from_state" "$to_state" "$command_name" \
        >> "$transition_root/transitions.log"
}

append_generated_followup() {
    if [[ "$include_generated" != "true" ]]; then
        return 0
    fi

    safe_id="$(safe_task_id)"
    marker="Task generated-followup-${safe_id}:"
    if [[ -n "$tasks_dir" ]]; then
        mkdir -p "$tasks_dir"
        followup="$tasks_dir/99-generated-followup-${safe_id}.md"
        if [[ -e "$followup" ]]; then
            return 0
        fi
        cat > "$followup" <<EOF
### Task generated-followup-${safe_id}: Generated follow-up for ${task_id}
**State:** script-check

This task was appended by the aggregate transition callback so the UI can show
workspace expansion during a live run.
EOF
        return 0
    fi

    if grep -q "$marker" "$plan_file"; then
        return 0
    fi

    cat >> "$plan_file" <<EOF

### Task generated-followup-${safe_id}: Generated follow-up for ${task_id}
**State:** script-check

This task was appended by the aggregate transition callback so the UI can show
workspace expansion during a live run.
EOF
}

case "$command_name" in
    log)
        log_transition
        ;;
    aggregate)
        log_transition
        append_generated_followup
        ;;
    *)
        echo "unknown transition command: $command_name" >&2
        exit 1
        ;;
esac
