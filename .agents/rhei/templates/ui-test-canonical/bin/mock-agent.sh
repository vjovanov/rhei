#!/usr/bin/env bash

set -euo pipefail

prompt=""
mode="default"
model="${RHEI_MODEL_NAME:-${RHEI_MODEL:-mock-model}}"
session_dir=""
skills=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --prompt)
            prompt="${2:-}"
            shift 2
            ;;
        --model)
            model="${2:-$model}"
            shift 2
            ;;
        --mode)
            mode="${2:-$mode}"
            shift 2
            ;;
        --skill)
            skills+=("${2:-}")
            shift 2
            ;;
        --session-dir)
            session_dir="${2:-}"
            shift 2
            ;;
        --resume|--fork)
            shift 2
            ;;
        --)
            shift
            if [[ -z "$prompt" ]]; then
                prompt="$(cat)"
            fi
            ;;
        *)
            shift
            ;;
    esac
done

if [[ -z "$prompt" && ! -t 0 ]]; then
    prompt="$(cat)"
fi

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
target_slug="${RHEI_TARGET_SLUG:-${RHEI_AGENT:-mock-agent}-${model}}"
step_delay="${MOCK_NODE_DELAY_SECONDS:-{{ step_delay_seconds }}}"

mkdir -p "$workspace_root/runtime/logs"
log_path="$workspace_root/runtime/logs/mock-agent.log"

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
    printf '%s task=%s state=%s target=%s mode=%s model=%s %s\n' \
        "$(timestamp)" "$task_id" "$state" "$target_slug" "$mode" "$model" "$1" >> "$log_path"
}

write_snapshot_transcript() {
    local dir="${session_dir:-${RHEI_SNAPSHOT_SESSION_DIR:-}}"
    if [[ -z "$dir" ]]; then
        return 0
    fi
    mkdir -p "$dir"
    cat > "$dir/mock-session.jsonl" <<EOF
{"session_id":"mock-${task_id}-${state}-${target_slug}","provider":"${RHEI_MODEL_PROVIDER:-mock}","model":"${model}"}
{"role":"user","content":"${state} prompt for ${task_id}"}
{"role":"assistant","content":"mock ${state} output for ${task_id}"}
EOF
}

sleep "$step_delay"
append_log "started"
write_snapshot_transcript
prompt_bytes="$(printf '%s' "$prompt" | wc -c)"

case "$state" in
    collect-inputs)
        artifact_dir="$workspace_root/runtime/artifacts/$task_id"
        write_file "$artifact_dir/inputs.md" "# Inputs for ${task_id}

- scenario: {{ scenario_name }}
- target: ${target_slug}
- prompt-bytes: ${prompt_bytes}
- generated-by: mock-agent"
        write_file "$artifact_dir/notes.json" "{\"task\":\"${task_id}\",\"state\":\"${state}\",\"target\":\"${target_slug}\",\"scenario\":\"{{ scenario_name }}\"}"
        ;;
    mock-implement)
        write_file "$workspace_root/runtime/implementation/${task_id}.md" "# Implementation ${task_id}

- target: ${target_slug}
- model: ${model}
- mode: ${mode}
- skills: ${skills[*]:-none}
- normalized input consumed: yes
- snapshot session: ${session_dir:-${RHEI_SNAPSHOT_SESSION_DIR:-none}}"
        ;;
    parallel-review)
        write_file "$workspace_root/runtime/reviews/${task_id}-${target_slug}.md" "# Review ${task_id} ${target_slug}

- finding: ${target_slug} accepts the deterministic fixture output.
- build report consumed: yes
- recommendation: continue to aggregate."
        ;;
    fix-loop)
        # Prefer the runtime-provided visit counter. When the runtime does not
        # expose it to agents, fall back to counting the fix artifacts already
        # on disk so the file we write matches the declared `{visit_count}`
        # output path (otherwise the counted loop never satisfies its contract).
        visit="${RHEI_VISIT_COUNT:-}"
        if [[ -z "$visit" ]]; then
            mkdir -p "$workspace_root/runtime/fixes"
            existing_count="$(find "$workspace_root/runtime/fixes" -maxdepth 1 -type f -name "${task_id}-visit-*.md" 2>/dev/null | wc -l)"
            visit=$((existing_count + 1))
        fi
        write_file "$workspace_root/runtime/fixes/${task_id}-visit-${visit}.md" "# Fix Loop ${task_id} Visit ${visit}

- target: ${target_slug}
- inherited snapshot parent: ${RHEI_SNAPSHOT_PARENT_REF:-none}
- aggregate consumed: yes
- action: deterministic fix note for UI testing."
        ;;
    inherit-ancestor)
        write_file "$workspace_root/runtime/inherit/${task_id}.md" "# Ancestor Inheritance ${task_id}

- target: ${target_slug}
- inherited snapshot parent: ${RHEI_SNAPSHOT_PARENT_REF:-none}
- result: ancestor implementation snapshot ${RHEI_SNAPSHOT_PARENT_REF:+preloaded}${RHEI_SNAPSHOT_PARENT_REF:-absent (continuing without it)}"
        ;;
    *)
        write_file "$workspace_root/runtime/artifacts/${task_id}/${state}-agent.md" "# Mock Agent Output

- task: ${task_id}
- state: ${state}
- target: ${target_slug}"
        ;;
esac

append_log "completed"
printf 'mock agent completed task=%s state=%s target=%s\n' "$task_id" "$state" "$target_slug"
