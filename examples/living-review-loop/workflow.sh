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
findings_dir="$runtime_dir/findings"
verifications_dir="$runtime_dir/verifications"
fixes_dir="$runtime_dir/fixes"
tasks_dir="$workspace_root/tasks"

mkdir -p "$logs_dir" "$findings_dir" "$verifications_dir" "$fixes_dir" "$tasks_dir"

team_log="$logs_dir/team.log"
task_log="$logs_dir/task-${RHEI_TASK_ID:-unknown}.log"

timestamp() {
    date -Iseconds
}

log_line() {
    local message="$1"
    printf '%s task=%s model=%s %s -> %s %s\n' \
        "$(timestamp)" \
        "${RHEI_TASK_ID:-unknown}" \
        "${RHEI_MODEL:-none}" \
        "${RHEI_FROM_STATE:-unknown}" \
        "${RHEI_TO_STATE:-unknown}" \
        "$message" | tee -a "$team_log" >> "$task_log"
}

write_file() {
    local path="$1"
    local content="$2"
    printf '%s\n' "$content" > "$path"
}

write_task_if_missing() {
    local path="$1"
    local content="$2"

    if [[ -e "$path" ]]; then
        return 0
    fi

    printf '%s\n' "$content" > "$path"
}

review_output_path() {
    local model="$1"
    printf '%s/%s-findings.md\n' "$findings_dir" "$model"
}

review_source_root() {
    if [[ -n "${RHEI_LIVING_REVIEW_SOURCE_ROOT:-}" ]]; then
        printf '%s\n' "$RHEI_LIVING_REVIEW_SOURCE_ROOT"
        return 0
    fi

    if git_root="$(git -C "$workspace_root" rev-parse --show-toplevel 2>/dev/null)"; then
        printf '%s\n' "$git_root"
        return 0
    fi

    printf '%s\n' "$workspace_root"
}

ensure_review_source_root() {
    local root
    root="$(review_source_root)"

    if [[ -d "$root/docs/functional-spec" ]]; then
        return 0
    fi

    cat >&2 <<EOF
live review requires access to the repository docs/functional-spec directory.
Set RHEI_LIVING_REVIEW_SOURCE_ROOT to the project root before running the copied workspace.
Current review source root: $root
EOF
    exit 1
}

review_specs_root() {
    printf '%s/docs/functional-spec\n' "$(review_source_root)"
}

review_specs_manifest() {
    local specs_root
    specs_root="$(review_specs_root)"
    (
        cd "$specs_root"
        find . -maxdepth 1 -type f | sort
    )
}

render_review_corpus() {
    local specs_root
    specs_root="$(review_specs_root)"

    while IFS= read -r relpath; do
        local path
        path="${relpath#./}"
        printf '\n## FILE: %s\n' "$path"
        sed -n '1,4000p' "$specs_root/$path"
        printf '\n'
    done < <(review_specs_manifest)
}

review_prompt() {
    local model="$1"
    cat <<EOF
Review the following Rhei specification documents for gaps, contradictions, and ambiguities.
Focus on problems that would mislead an implementor or confuse a user.

Files to review:
$(review_specs_manifest)

Respond with markdown only in this exact shape:
# Review Findings: Model ${model}

- F-...: ...
- F-...: ...

Keep the findings concise and distinct from the other reviewer.

Here are the spec files:
$(render_review_corpus)
EOF
}

use_live_reviewers() {
    [[ "${RHEI_LIVING_REVIEW_MODE:-mock}" == "live" ]]
}

write_mock_review() {
    local model="$1"
    local output_path
    output_path="$(review_output_path "$model")"

    case "$model" in
        claude)
            write_file "$output_path" "# Review Findings: Model claude

- F-001: cache invalidation key appears to omit the project identifier
- F-002: release example help text may still mention a stale flag"
            ;;
        codex)
            write_file "$output_path" "# Review Findings: Model codex

- F-001: cache key composition looks incomplete around project scoping
- F-003: retry path may swallow the upstream timeout detail"
            ;;
        *)
            echo "unknown model: $model" >&2
            exit 1
            ;;
    esac
}

run_claude_review() {
    local model="$1"
    local output_path
    output_path="$(review_output_path "$model")"

    ensure_review_source_root

    review_prompt "$model" | claude -p \
        --output-format text \
        --permission-mode bypassPermissions > "$output_path"
}

run_codex_review() {
    local model="$1"
    local output_path
    output_path="$(review_output_path "$model")"

    ensure_review_source_root

    review_prompt "$model" | codex exec \
        --sandbox danger-full-access \
        --skip-git-repo-check \
        --cd "$workspace_root" \
        --add-dir "$workspace_root" \
        --output-last-message "$output_path" \
        -
}

write_review() {
    local model="${RHEI_MODEL:-unknown}"
    log_line "wrote review findings"

    if ! use_live_reviewers; then
        write_mock_review "$model"
        return 0
    fi

    case "$model" in
        claude)
            run_claude_review "$model"
            ;;
        codex)
            run_codex_review "$model"
            ;;
        *)
            echo "unknown model: $model" >&2
            exit 1
            ;;
    esac
}

consolidate() {
    log_line "consolidated multi-model findings and spawned verification tasks"

    {
        printf '# Review Findings\n'
        for model in claude codex; do
            printf '\n## Model %s\n' "$model"
            if [[ -f "$findings_dir/${model}-findings.md" ]]; then
                grep '^-' "$findings_dir/${model}-findings.md" || true
            fi
        done
        printf '\n## Consolidated review points\n'
        printf '1. F-001: Verify whether cache invalidation can cross project boundaries.\n'
        printf '2. F-002: Verify whether the stale CLI help text is still reproducible.\n'
        printf '3. F-003: Verify whether timeout details are lost during retries.\n'
    } > "$findings_dir/review-findings.md"

    write_task_if_missing "$tasks_dir/02-verify-cache-key.md" "### Task verify-cache-key: Verify and reproduce finding F-001
**State:** prove
**Prior:** Task review-seed

Check whether cache invalidation can reproduce across project boundaries and
record whether the finding is relevant enough to justify a fix."

    write_task_if_missing "$tasks_dir/03-verify-cli-help.md" "### Task verify-cli-help: Verify and reproduce finding F-002
**State:** prove
**Prior:** Task review-seed

Check whether the stale CLI help wording still exists in the current workspace
and record whether the finding is relevant to the current scope."

    write_task_if_missing "$tasks_dir/04-verify-timeout-details.md" "### Task verify-timeout-details: Verify and reproduce finding F-003
**State:** prove
**Prior:** Task review-seed

Check whether retry handling hides the upstream timeout detail and record
whether the finding is relevant enough to justify a fix."
}

verify_cache_key() {
    log_line "verified F-001 as relevant and spawned a fix task"

    write_file "$verifications_dir/F-001.md" "# Verification F-001

- Reproduced: yes
- Relevant: yes
- Summary: a missing project identifier in the cache key could let one project
  observe another project's invalidation behavior."

    write_task_if_missing "$tasks_dir/11-fix-cache-key.md" "### Task fix-cache-key: Fix finding F-001 after verified reproduction
**State:** prove
**Prior:** Task verify-cache-key

Apply the smallest fix that keeps cache invalidation scoped to one project now
that the issue is reproduced and confirmed relevant."
}

verify_cli_help() {
    log_line "verified F-002 as not relevant and skipped fix expansion"

    write_file "$verifications_dir/F-002.md" "# Verification F-002

- Reproduced: no
- Relevant: no
- Summary: the current workspace no longer contains the stale help text, so the
  review note came from an older snapshot and does not justify a fix task."
}

verify_timeout_details() {
    log_line "verified F-003 as relevant and spawned a fix task"

    write_file "$verifications_dir/F-003.md" "# Verification F-003

- Reproduced: yes
- Relevant: yes
- Summary: retry handling drops the original timeout context, which makes
  production diagnosis harder and merits a focused fix."

    write_task_if_missing "$tasks_dir/12-fix-timeout-details.md" "### Task fix-timeout-details: Fix finding F-003 after verified reproduction
**State:** prove
**Prior:** Task verify-timeout-details

Preserve the upstream timeout detail through the retry path now that the issue
is reproduced and confirmed relevant."
}

fix_cache_key() {
    log_line "completed fix task for F-001"

    write_file "$fixes_dir/F-001.md" "# Fix F-001

- Status: completed
- Action: include the project identifier in the cache invalidation key."
}

fix_timeout_details() {
    log_line "completed fix task for F-003"

    write_file "$fixes_dir/F-003.md" "# Fix F-003

- Status: completed
- Action: preserve the original timeout details when retries exhaust."
}

cancel_task() {
    log_line "task cancelled"
    write_file "$runtime_dir/cancelled-${RHEI_TASK_ID:-unknown}.txt" \
        "task ${RHEI_TASK_ID:-unknown} cancelled at $(timestamp)"
}

case "$command_name" in
    write-review)
        write_review
        ;;
    consolidate)
        consolidate
        ;;
    execute-task)
        case "${RHEI_TASK_ID:-}" in
            verify-cache-key)
                verify_cache_key
                ;;
            verify-cli-help)
                verify_cli_help
                ;;
            verify-timeout-details)
                verify_timeout_details
                ;;
            fix-cache-key)
                fix_cache_key
                ;;
            fix-timeout-details)
                fix_timeout_details
                ;;
            *)
                echo "unknown task id for execute-task: ${RHEI_TASK_ID:-}" >&2
                exit 1
                ;;
        esac
        ;;
    cancel)
        cancel_task
        ;;
    *)
        echo "unknown workflow command: $command_name" >&2
        exit 1
        ;;
esac
