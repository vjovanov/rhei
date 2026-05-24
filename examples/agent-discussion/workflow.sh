#!/usr/bin/env bash
#
# Callback driver for the agent-discussion example.
#
# By default it writes deterministic, canned positions so the checked-in example
# runs in CI without model credentials. Set RHEI_DISCUSSION_MODE=live to dispatch
# each participant to a real CLI instead. Set RHEI_DISCUSSION_FORCE_ESCALATE=1 to
# make the judge never converge, driving the discussion to the human gate.
#
# The discussion round is taken from the state being left (RHEI_FROM_STATE):
# r1-* is round 1, r2-* is round 2.

set -euo pipefail

command_name="${1:-}"

# Resolve the workspace root. Prefer RHEI_PLAN_PATH (set by the runtime); fall
# back to this script's own directory so a manual invocation still works.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
workspace_root="${RHEI_PLAN_PATH:-$script_dir}"
if [[ -f "$workspace_root" ]]; then
    workspace_root="$(dirname "$workspace_root")"
fi

runtime_dir="$workspace_root/runtime"
disc_dir="$runtime_dir/discussion"
digest_dir="$disc_dir/digest"
logs_dir="$runtime_dir/logs"

mkdir -p "$digest_dir" "$logs_dir"

log_file="$logs_dir/discussion.log"

# The four participants and the project goal each one champions.
participants=(claude codex gemini cursor)

goal_for() {
    case "$1" in
        claude) echo "Developer Experience — keep coordination frictionless and human-legible" ;;
        codex)  echo "Determinism & Auditability — every decision must be reproducible and recorded" ;;
        gemini) echo "Throughput & Scale — never put a human in the hot path of a parallel swarm" ;;
        cursor) echo "Safety & Human Oversight — irreversible decisions must have a human gate" ;;
        *)      echo "General project health" ;;
    esac
}

# Maximum discussion rounds before the judge escalates to a human.
CAP=3

timestamp() { date -Iseconds; }

# The current round number = number of digests already written + 1. The judge
# writes one digest at the end of each round, so every participant in the same
# round sees the same count.
current_round() {
    local n
    n="$(find "$digest_dir" -maxdepth 1 -name 'round-*.md' 2>/dev/null | wc -l | tr -d ' ')"
    echo $(( n + 1 ))
}

log_line() {
    printf '%s task=%s model=%s %s -> %s %s\n' \
        "$(timestamp)" \
        "${RHEI_TASK_ID:-unknown}" \
        "${RHEI_MODEL:-none}" \
        "${RHEI_FROM_STATE:-?}" \
        "${RHEI_TO_STATE:-?}" \
        "$1" >> "$log_file"
}

use_live() { [[ "${RHEI_DISCUSSION_MODE:-mock}" == "live" ]]; }
force_escalate() { [[ "${RHEI_DISCUSSION_FORCE_ESCALATE:-0}" == "1" ]]; }

# ---------------------------------------------------------------------------
# Mock positions: opening stances in round 1, convergence in round 2.
# ---------------------------------------------------------------------------

mock_position_round1() {
    case "$1" in
        claude) cat <<'EOF'
Auto-merge the decision. The plan is the single source of truth and git already
records every change, so a human can always read the diff. Forcing a gate on
*every* decision destroys the frictionless flow that makes a swarm usable.
EOF
            ;;
        codex) cat <<'EOF'
No silent auto-merge. The judge must record a structured ruling — the decision and
its rationale — so every outcome is reproducible and auditable later. Whether a
human is in the loop matters less than whether the decision is written down.
EOF
            ;;
        gemini) cat <<'EOF'
Never put a human in the hot path. With many agents running in parallel, a human
gate on each decision serializes the whole swarm and throughput collapses.
Auto-merge, and treat the judge's digest as the durable record.
EOF
            ;;
        cursor) cat <<'EOF'
Some decisions are irreversible — deleting data, shipping to production, rewriting
history. Those MUST pass a human gate. Auto-merging the irreversible subset is
exactly how an autonomous swarm causes real damage.
EOF
            ;;
    esac
}

mock_position_round2() {
    case "$1" in
        claude) cat <<'EOF'
codex is right that we need a record — but the judge's per-round digest already is
one, so auditability does not require a gate. I accept cursor's point: gate only
the irreversible subset. Low-risk decisions still auto-merge, so flow is preserved.
EOF
            ;;
        codex) cat <<'EOF'
Agreed with claude: the per-round digest plus an explicit risk classification
satisfies auditability without blocking. I withdraw the demand to gate everything —
recording the decision and its risk class is enough for reproducibility.
EOF
            ;;
        gemini) cat <<'EOF'
cursor's gate is acceptable *because* the irreversible subset is rare; the common
low-risk case still auto-merges, so the swarm is not serialized. Throughput is
preserved as long as the judge, not a human, classifies the routine cases.
EOF
            ;;
        cursor) cat <<'EOF'
If the judge classifies risk and the irreversible subset is gated, I am satisfied.
I concede that low-risk auto-merge is safe as long as the digest records what
happened and the classification is explicit.
EOF
            ;;
    esac
}

write_mock_position() {
    local model="$1" round="$2" out="$3"
    {
        printf '# Position — %s (round %s)\n' "$model" "$round"
        printf '**Champions:** %s\n\n' "$(goal_for "$model")"
        if [[ "$round" -le 1 ]]; then
            mock_position_round1 "$model"
        else
            mock_position_round2 "$model"
        fi
    } > "$out"
}

# Live mode: dispatch the participant to a real CLI with a stance-aware prompt.
write_live_position() {
    local model="$1" round="$2" out="$3"
    local goal prompt prior_digest
    goal="$(goal_for "$model")"
    prior_digest="$(ls -1 "$digest_dir"/round-*.md 2>/dev/null | sort | tail -1 || true)"

    prompt="You are ${model}, a participant in a structured discussion.
You champion this project goal: ${goal}.
Argue the point strictly from that goal. This is round ${round}.

The point under discussion:
When an agent discussion converges on a decision, how should that decision enter
the plan — auto-merge, a recorded judge ruling, or human escalation?
"
    if [[ -n "$prior_digest" ]]; then
        prompt+="
Here is the previous round's digest. Respond to the other participants by name,
concede what they got right, and sharpen where you still disagree:

$(cat "$prior_digest")
"
    fi
    prompt+="
Respond with a short markdown position (4-6 sentences)."

    case "$model" in
        claude)
            printf '%s' "$prompt" | claude -p --output-format text --permission-mode bypassPermissions > "$out"
            ;;
        codex)
            printf '%s' "$prompt" | codex exec --sandbox danger-full-access --skip-git-repo-check --cd "$workspace_root" --output-last-message "$out" -
            ;;
        gemini)
            printf '%s' "$prompt" | gemini --prompt - --yolo > "$out"
            ;;
        cursor)
            printf '%s' "$prompt" | cursor-agent --print --force > "$out"
            ;;
        *)
            echo "unknown participant for live mode: $model" >&2
            exit 1
            ;;
    esac
}

# ---------------------------------------------------------------------------
# Callbacks
# ---------------------------------------------------------------------------

# Fires once per participant (all_models fanout), on leaving a `*-collect` state.
write_position() {
    local model="${RHEI_MODEL:-unknown}"
    local round round_dir out
    round="$(current_round)"
    round_dir="$disc_dir/round-$round"
    mkdir -p "$round_dir"
    out="$round_dir/$model.md"

    log_line "wrote round $round position"

    if use_live; then
        write_live_position "$model" "$round" "$out"
    else
        write_mock_position "$model" "$round" "$out"
    fi
}

# Whether the round reached consensus. The mock converges in round 2 unless
# escalation is forced; live mode asks the judge CLI for a verdict.
round_converged() {
    local round="$1"
    if force_escalate; then
        return 1
    fi
    if use_live; then
        live_round_converged "$round"
        return $?
    fi
    [[ "$round" -ge 2 ]]
}

live_round_converged() {
    local round="$1"
    local verdict
    verdict="$(printf 'Have these discussion positions converged on a single decision? Answer exactly CONVERGED or CONTINUE.\n\n%s\n' \
        "$(cat "$disc_dir/round-$round"/*.md 2>/dev/null)" \
        | codex exec --sandbox read-only --skip-git-repo-check --output-last-message /dev/stdout - 2>/dev/null || true)"
    [[ "$verdict" == *CONVERGED* ]]
}

write_decision() {
    local round="$1"
    cat > "$disc_dir/decision.md" <<EOF
# Decision: D-merge-policy
**Converged:** round $round at $(timestamp)
**Participants:** ${participants[*]}

## Question
When an agent discussion converges on a decision, how should that decision enter
the plan — auto-merge, a recorded judge ruling, or human escalation?

## Decision (risk-tiered)
- Low-risk decisions auto-merge into the plan; the judge's per-round digest is the
  recorded audit trail. (Honors Developer Experience, Throughput, and Auditability.)
- Decisions the judge classifies as irreversible or destructive escalate to a human
  review gate before they take effect. (Honors Safety & Human Oversight.)
- The judge classifies and records each decision's risk class.

## How the competing goals were reconciled
- Determinism & Auditability: met by recording the digest + risk class, not by
  blocking every decision.
- Safety & Human Oversight: met by gating only the irreversible subset.
- Developer Experience & Throughput: met by auto-merging the common low-risk case
  with no human in the hot path.
EOF
}

# Fires once on leaving `judge`. Writes the round digest, then redirects via
# `nextState`: `converged` (consensus), `escalated` (round budget spent), or no
# redirect so the engine takes the declared default transition (judge -> collect).
judge_round() {
    local round digest
    round="$(current_round)"
    digest="$digest_dir/round-$round.md"

    {
        printf '# Discussion digest — round %s\n\n' "$round"
        printf 'Point: how should a converged discussion enter the plan?\n\n'
        for p in "${participants[@]}"; do
            local pf="$disc_dir/round-$round/$p.md"
            if [[ -f "$pf" ]]; then
                printf '## %s — %s\n\n' "$p" "$(goal_for "$p")"
                cat "$pf"
                printf '\n'
            fi
        done
    } > "$digest"

    if round_converged "$round"; then
        write_decision "$round"
        printf '\n## Outcome\nConverged on a risk-tiered merge policy. Decision recorded in decision.md.\n' >> "$digest"
        log_line "round $round converged -> decision recorded"
        printf '{"success": true, "nextState": "converged"}\n'
        return 0
    fi

    if [[ "$round" -ge "$CAP" ]]; then
        printf '\n## Outcome\nRound budget (%s) exhausted without consensus. Escalating to a human.\n' "$CAP" >> "$digest"
        log_line "round $round exhausted budget -> escalating"
        printf '{"success": true, "nextState": "escalated"}\n'
        return 0
    fi

    printf '\n## Outcome\nNo consensus yet — the safety/oversight axis and the throughput/DX axis are still in tension. Opening another round.\n' >> "$digest"
    log_line "round $round inconclusive -> another round"
    # No redirect: the engine applies the declared default transition (judge -> collect).
    printf '{"success": true}\n'
}

# Fires on leaving `apply` (the downstream task that depends on the decision).
apply_decision() {
    local decision="$disc_dir/decision.md"
    if [[ ! -f "$decision" ]]; then
        echo "decision.md not found; the discussion has not converged" >&2
        exit 1
    fi
    log_line "applied the converged decision"
    cat > "$disc_dir/applied.md" <<EOF
# Applied: D-merge-policy
**Applied:** $(timestamp)

The converged merge policy is now in effect:
- low-risk decisions auto-merge, with the judge digest as the audit trail
- irreversible decisions escalate to a human review gate

See runtime/discussion/decision.md for the full ruling.
EOF
}

cancel_task() {
    log_line "discussion cancelled"
    printf 'cancelled at %s\n' "$(timestamp)" > "$disc_dir/cancelled-${RHEI_TASK_ID:-unknown}.txt"
}

case "$command_name" in
    write-position) write_position ;;
    judge-round)    judge_round ;;
    apply-decision) apply_decision ;;
    cancel)         cancel_task ;;
    *)
        echo "unknown workflow command: $command_name" >&2
        exit 1
        ;;
esac
