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

task_id="${RHEI_TASK_ID:-unknown}"
state="${RHEI_STATE:-unknown}"
visit_count="${RHEI_VISIT_COUNT:-1}"
step_delay="${MOCK_NODE_DELAY_SECONDS:-0.1}"

mkdir -p "$workspace_root/runtime/logs"
log_path="$workspace_root/runtime/logs/mock-program.log"

timestamp() {
    date -Iseconds
}

write_file() {
    local path="$1"
    local content="$2"
    mkdir -p "$(dirname "$path")"
    printf '%s\n' "$content" > "$path"
}

append_log() {
    printf '%s task=%s state=%s visit=%s command=%s %s\n' \
        "$(timestamp)" "$task_id" "$state" "$visit_count" "$command_name" "$1" >> "$log_path"
}

sleep "$step_delay"
append_log "started"

case "$command_name" in
    normalize)
        artifact_dir="$workspace_root/runtime/artifacts/$task_id"
        write_file "$artifact_dir/normalized.json" "{\"task\":\"${task_id}\",\"scenario\":\"${MOCK_SCENARIO:-unknown}\",\"normalized\":true}"
        write_file "$artifact_dir/io-map.md" "# IO Map ${task_id}

- input: ${RHEI_INPUT_RAW_INPUTS_PATH:-runtime/artifacts/${task_id}/inputs.md}
- notes: ${RHEI_INPUT_RAW_NOTES_PATH:-runtime/artifacts/${task_id}/notes.json}
- output: runtime/artifacts/${task_id}/normalized.json"
        ;;
    build)
        write_file "$workspace_root/runtime/build/${task_id}-report.md" "# Build Report ${task_id}

- implementation: ${RHEI_INPUT_IMPLEMENTATION_PATH:-runtime/implementation/${task_id}.md}
- scenario: ${MOCK_SCENARIO:-unknown}
- status: passed"
        write_file "$workspace_root/runtime/build/${task_id}-bundle.txt" "bundle for ${task_id}"
        ;;
    aggregate)
        mkdir -p "$workspace_root/runtime/aggregate"
        aggregate_md="$workspace_root/runtime/aggregate/${task_id}.md"
        {
            printf '# Aggregate %s\n\n' "$task_id"
            printf -- '- scenario: %s\n' "${MOCK_SCENARIO:-unknown}"
            printf -- '- review files:\n'
            find "$workspace_root/runtime/reviews" -maxdepth 1 -type f -name "${task_id}-*.md" 2>/dev/null | sort | while read -r review; do
                printf '  - %s\n' "${review#$workspace_root/}"
            done
        } > "$aggregate_md"
        write_file "$workspace_root/runtime/aggregate/${task_id}.json" "{\"task\":\"${task_id}\",\"aggregated\":true}"
        ;;
    poll)
        mkdir -p "$workspace_root/runtime/poll"
        if [[ "$visit_count" -lt "2" ]]; then
            write_file "$workspace_root/runtime/poll/${task_id}-attempt-${visit_count}.md" "# Poll Attempt ${visit_count}

Mock external system still running for ${task_id}."
            append_log "poll pending"
            exit 75
        fi
        write_file "$workspace_root/runtime/poll/${task_id}-ready.json" "{\"task\":\"${task_id}\",\"ready\":true,\"attempt\":${visit_count}}"
        ;;
    check)
        write_file "$workspace_root/runtime/checks/${task_id}.md" "# Check ${task_id}

- state: ${state}
- scenario: ${MOCK_SCENARIO:-unknown}
- status: passed"
        ;;
    fail)
        write_file "$workspace_root/runtime/failures/${task_id}.md" "# Failure ${task_id}

- state: ${state}
- scenario: ${MOCK_SCENARIO:-unknown}
- status: failed (deterministic mock failure for UI testing)"
        append_log "failed exit=42"
        exit 42
        ;;
    poll-exhaust)
        mkdir -p "$workspace_root/runtime/poll"
        write_file "$workspace_root/runtime/poll/${task_id}-pending.json" "{\"task\":\"${task_id}\",\"ready\":false,\"attempt\":${visit_count}}"
        append_log "poll never ready"
        exit 75
        ;;
    *)
        echo "unknown mock program command: $command_name" >&2
        exit 1
        ;;
esac

append_log "completed"
