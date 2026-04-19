#!/usr/bin/env bash
#
# Thin bridge: reads the state machine + task file, calls the right agent.
# All behavior is defined in team-states.yaml — this script just dispatches.
#
set -euo pipefail

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
from_state="${RHEI_FROM_STATE:-unknown}"
to_state="${RHEI_TO_STATE:-unknown}"
model="${RHEI_MODEL:-claude}"

mkdir -p "$workspace_root/runtime/findings" \
         "$workspace_root/runtime/verifications" \
         "$workspace_root/runtime/fixes" \
         "$workspace_root/tasks"

# Cancel needs no agent call
if [[ "$to_state" == "cancelled" ]]; then
    exit 0
fi

# Find the task file for this task id
task_file="$(find "$workspace_root/tasks" -name "*-${task_id}.md" -print -quit 2>/dev/null || true)"
task_content=""
if [[ -n "$task_file" && -f "$task_file" ]]; then
    task_content="$(cat "$task_file")"
fi

# Build prompt from state machine + task context
prompt="$(cat <<PROMPT
You are working in a Rhei workflow. Here is your context:

Task: ${task_id}
Transition: ${from_state} -> ${to_state}
Model: ${model}
Workspace: ${workspace_root}

State machine definition:
$(cat "$workspace_root/team-states.yaml")

Task description:
${task_content}

Execute the instructions for the '${from_state}' state as described in the
state machine. Follow the state's personality and instructions exactly.
PROMPT
)"

# Dispatch to the right agent
case "$model" in
    claude)
        claude -p \
            --output-format text \
            --permission-mode bypassPermissions \
            --add-dir "$workspace_root" \
            "$prompt"
        ;;
    codex)
        codex exec \
            --sandbox networking-off \
            --skip-git-repo-check \
            --cd "$workspace_root" \
            "$prompt"
        ;;
    *)
        echo "unknown model: $model" >&2
        exit 1
        ;;
esac
