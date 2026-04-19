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
artifacts_dir="$runtime_dir/artifacts/task-${RHEI_TASK_ID:-unknown}"
mkdir -p "$logs_dir" "$artifacts_dir"

team_log="$logs_dir/team.log"
task_log="$logs_dir/task-${RHEI_TASK_ID:-unknown}.log"

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

write_file() {
    local path="$1"
    local content="$2"
    printf '%s\n' "$content" > "$path"
}

case "$command_name" in
    kickoff-mock)
        log_line "mock kickoff command executed"
        write_file \
            "$artifacts_dir/00-kickoff.txt" \
            "mock-command: agent-team kickoff --task ${RHEI_TASK_ID}"
        ;;
    handoff-research)
        log_line "coordinator handed task to researcher"
        write_file \
            "$artifacts_dir/10-research-note.md" \
            "# Research Note for Task ${RHEI_TASK_ID}

- Source state: ${RHEI_FROM_STATE}
- Target state: ${RHEI_TO_STATE}
- Summary: gather context, keep it small, hand off clearly."
        ;;
    handoff-implementation)
        log_line "researcher handed task to implementer"
        write_file \
            "$artifacts_dir/20-implementation.txt" \
            "implementation artifact for task ${RHEI_TASK_ID}
based on: 10-research-note.md"
        ;;
    handoff-review)
        log_line "implementer handed task to reviewer"
        if [[ ! -f "$artifacts_dir/20-implementation.txt" ]]; then
            echo "missing implementation artifact for task ${RHEI_TASK_ID}" >&2
            exit 1
        fi
        write_file \
            "$artifacts_dir/30-review.txt" \
            "review prepared for task ${RHEI_TASK_ID}
artifact present: yes"
        ;;
    finalize)
        log_line "reviewer finalized task"
        write_file \
            "$artifacts_dir/40-complete.txt" \
            "task ${RHEI_TASK_ID} completed at $(timestamp)"
        ;;
    cancel)
        log_line "task cancelled"
        write_file \
            "$artifacts_dir/99-cancelled.txt" \
            "task ${RHEI_TASK_ID} cancelled at $(timestamp)"
        ;;
    *)
        echo "unknown workflow command: $command_name" >&2
        exit 1
        ;;
esac

