#!/bin/sh
session_dir=""
resume_value=""
fork_value=""
interactive=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --interactive)
      interactive=1
      ;;
    --session-dir)
      shift
      session_dir="${1:-}"
      ;;
    --resume)
      shift
      resume_value="${1:-}"
      ;;
    --fork)
      shift
      fork_value="${1:-}"
      ;;
    --prompt | --model)
      shift
      ;;
  esac
  shift || true
done

mkdir -p runtime
{
  printf 'task=%s state=%s target=%s resume=%s fork=%s interactive=%s parent=%s\n' \
    "$RHEI_TASK_ID" "$RHEI_STATE" "$RHEI_TARGET_SLUG" "$resume_value" \
    "$fork_value" "$interactive" "${RHEI_SNAPSHOT_PARENT_REF:-}"
} >> runtime/fake-analysis-agent.log

if [ -n "$session_dir" ]; then
  mkdir -p "$session_dir"
  session_id="${RHEI_TASK_ID}-${RHEI_STATE}-${RHEI_TARGET_SLUG:-target}"
  {
    printf '{"session":{"provider":"%s","model":"%s"}}\n' \
      "${RHEI_MODEL_PROVIDER:-acme}" "${RHEI_MODEL_NAME:-model-a}"
    printf '{"role":"assistant","content":"%s","interactive":%s}\n' \
      "$RHEI_STATE" "$interactive"
  } > "$session_dir/$session_id.jsonl"
fi
