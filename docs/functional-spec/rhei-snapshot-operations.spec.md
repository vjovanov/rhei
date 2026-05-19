# FS-rhei-snapshot-operations: Rhei Snapshot Operations Specification

This document defines the operational surfaces around session snapshots: the
`rhei snapshot` command family, `rhei run --from-snapshot`, snapshot
settings, redaction, and rollout sequencing. It depends on the lineage,
storage, manifest, compatibility, and runtime emit/preload model in
§FS-rhei-snapshots; the run lifecycle in §FS-rhei-run; and agent session
profiles and settings precedence in §FS-rhei-agents.
§GOAL-rhei-outcomes

## 1. CLI Surface

### 1.1. Snapshot Reference Parser

All snapshot commands that accept a reference use the same parser and
precedence table:

| Form | Meaning | Precedence |
|------|---------|------------|
| Full path under `.rhei/cache/snapshots/` | Exact generation directory. | Highest. |
| `<task>:<name>[:<state>][@<visit>][:<target>][/g<N>]` | Named or explicit `_state` snapshot reference. | Named snapshot matches win over shorthand auto-emit. |
| `<task>:<state>` | Auto-emit shorthand for `<task>:_state:<state>`. | Used only when the second segment does not resolve as a named snapshot; use explicit `_state` to force auto-emit. |

Unresolved positional ambiguity is an error. The command prints the matching
candidates and guidance to retry with explicit `--task`, `--name`, `--state`,
`--target`, or `--generation` selectors rather than applying command-specific
tie-breakers.

### 1.2. `rhei snapshot list`

Prints the snapshot cache contents. Options:

- `--task <id>`: filter by task.
- `--name <snapshot-name>`: filter by snapshot name. Pass `_state` to limit
  to auto-emitted snapshots; pass an author name to limit to that named
  lineage.
- `--state <state-name>`: filter by emitting state. Combines with `--task`
  for the common "what intermediate states can I continue from?" query.
- `--produced-by orchestrator|operator|all`: filter by emission origin. The
  default is `orchestrator`; operator generations require an explicit filter.
- `--orphaned`: show only snapshots whose task, emitting state, or target no
  longer resolves in the current plan and state machine.
- `--format text|json`: output format.

Default columns: task, snapshot name, emitting state, visit, target slug,
generation, created_at, transcript bytes, completion, produced_by.

### 1.3. `rhei snapshot show <ref>`

Prints a manifest in full and a transcript head/tail preview. It uses the
shared snapshot reference parser above. Worked example:

```
1.2.3:implementation:pending@2:claude-code-anthropic-claude-opus-4-7/g3
```

resolves to task `1.2.3`, snapshot name `implementation`, emitting state
`pending`, visit `2`, target slug `claude-code-anthropic-claude-opus-4-7`,
generation `3`. Trailing positional segments may be omitted to broaden the
match; ambiguous shorthand prints all matches and exits non-zero.

### 1.4. `rhei snapshot gc`

Deletes snapshots by policy. Options:

- `--task <id>`: filter by task.
- `--name <snapshot-name>`: filter by snapshot name.
- `--older-than <duration>`: e.g. `7d`, `4h`.
- `--keep-generations <n>`: for each
  `(task_id, snapshot_name, emitting_state, visit, target_slug)` identity,
  retain the newest `n` orchestrator-produced generations and delete older
  generations that also satisfy the other filters. `n` must be at least `1`.
- `--include-operator`: include operator-produced generations in retention and
  deletion decisions. Without this flag, operator generations are ignored by
  `--keep-generations` and by unqualified deletion.
- `--orphaned`: delete orphans only.
- `--dry-run`: print what would be deleted.
- `--force`: bypass the live-run interlock (see below).

v1 GC is operator-driven only. Automatic GC is deferred. Operators can scope
retention with existing filters such as `--task`, `--name`, and `--orphaned`
before applying `--keep-generations`.

**Live-run interlock.** GC refuses to delete a snapshot if any of the
following hold for the same plan workspace:

1. An orchestrator process holds an active `.rhei/run.lock` (the lock
   §FS-rhei-run uses to serialize `rhei run` invocations).
2. The snapshot's generation is reachable from any active
   `snapshot.inherit.select.generation` on a state whose task is currently in a
   non-terminal state. This includes explicit integer generations and
   `latest`, not only `current`.

Retention is evaluated per `(task_id, snapshot_name, emitting_state, visit,
target_slug)` identity.

The check is best-effort, not transactional: an operator who starts `rhei
run` between the check and the delete can still race. Operators who need
deterministic behavior must stop the orchestrator first, or pass `--force`
to acknowledge the risk. `--force` is required to GC any snapshot in a
workspace whose `run.lock` is held.

### 1.5. `rhei snapshot continue <ref>`

Drops the operator into an interactive agent session preloaded with the
referenced snapshot. The operator drives the conversation; on agent exit,
the resulting transcript is captured as a sibling generation under the
*same identity* as the source snapshot, with `produced_by: operator`. The
`current` pointer is not advanced and the plan's task runtime state is
not modified — `continue` is read-mostly from the plan's perspective.

The intended use is analysis: ask the agent why it made a decision,
explore alternatives, inspect tool output that the orchestrator did not
preserve as an artifact. The operator's transcript stays in the cache and
is reachable by full ref or by filtering on `produced_by: operator` in
`rhei snapshot list`.

Resolution rules for `<ref>`:

- Full path: a directory under `.rhei/cache/snapshots/`.
- Shorthand: `<task>:<name>[:<state>][@<visit>][:<target>][/g<N>]` as in
  `rhei snapshot show`. Generations resolve to `current` when omitted.
- Auto-emit shorthand: `<task>:<state>` is interpreted as
  `<task>:_state:<state>`, so the common case (continue from this task at
  this state) needs no `_state` literal unless a named snapshot of the same
  segment exists, in which case the shared parser requires explicit `_state`.

Options:

- `--target <slug>`: required when the source state has multiple
  target-slug snapshots and the operator has not pinned one in the
  shorthand.
- `--generation <N>`: continue from a specific generation rather than
  `current`. Useful for revisiting an earlier orchestrator emission after
  a re-run.
- `--no-capture`: discard the operator's resulting transcript instead of
  writing a sibling generation. The interactive session still runs; on
  exit nothing is added to the cache.

The command requires the resolved agent to expose a `ResumeStrategy` other
than `None`, a usable `SessionLayout`, and an
`InteractiveContinuationProfile`; otherwise it errors before spawn with
`unsupported-snapshot-session`. The interactive profile must preserve TTY
pass-through, not the headless `-p`-style invocation that `rhei run` uses.
It may provide an alternate command when the agent exposes a distinct TTY
binary or subcommand; otherwise Rhei reuses the profile's base command with
the interactive arguments appended.
Agents whose built-in profiles offer only a headless transport in earlier
phases cannot be used with `continue` until that gap is closed (see
[Phased Rollout](#3-phased-rollout)).

If the referenced manifest has `completion: timeout`, `continue` may proceed
only after warning that the native transcript may be truncated.

**Live-run interlock.** `continue` takes the same `.rhei/run.lock` the
orchestrator uses for `rhei run`. If the lock is held, `continue` exits
with a clear error directing the operator to stop the run first. No
`--force` override exists: running an interactive analysis session
concurrently with a live orchestrator is unsafe (both could write the
same agent's session-output path mid-conversation), and the spec does
not offer a way to opt into the race.

The operator's generation manifest records:

- `produced_by: operator`.
- `parent_ref` pointing at the source snapshot whose transcript was
  preloaded into the interactive session, per the same rule the
  orchestrator follows for `rhei run` emissions.
- `completion: success` if the agent exited cleanly; `completion:
  failure` on a nonzero exit. There is no `timeout` classification —
  the operator drives pacing and the agent-timeout guardian does not run.

The operator's sibling generation is allocated by the same atomic-write
procedure as any other emission. Because `current` is not advanced, the
orchestrator's next emission for the same identity still resolves the
next free `g<N>` slot via the existing collision-retry rule; operator
generations interleave with orchestrator generations without disturbing
the `current` pointer chain.

## 2. Run Override

For an ad-hoc run, `--from-snapshot <ref>` overrides the concrete source
snapshot after the target state's authored `snapshot.inherit:` constraints have
been applied. The target state must still declare `snapshot.inherit:` and the
override must satisfy the declared name, `from`, `select.state`, target,
visit/generation, `required`, and `compat` constraints unless the operator also
passes `--override-inherit`.

`--override-inherit` is an explicit bypass for debugging source-selection and
compatibility constraints. It does not bypass the authored contract: the target
state must still declare `snapshot.inherit:`. Without `--override-inherit`,
`--from-snapshot` is rejected when the authored source would be `compat: none`
or when the referenced snapshot is not natively compatible; with
`--override-inherit`, those source and compatibility checks may be bypassed,
but the missing-`snapshot.inherit` rejection remains.

When the run context is ambiguous, the operator must provide selectors such as
`--task <id>` and `--target <slug>` so the command identifies exactly one task
and one fanout invocation. Ambiguous overrides exit non-zero with candidate
matches. These selectors scope only the inheriting run invocation; they do not
filter the source snapshot reference. Source snapshot target selection comes
from the reference itself and from the target state's authored
`snapshot.inherit.select.target` constraint.

## 3. Phased Rollout

| Phase | Scope | Deliverable |
|-------|-------|-------------|
| 1 | Spec + YAML grammar + validator | The snapshot specs, parser changes, validation rules including parse-time errors for unsupported `compat` values and `select.target: all`. Orphan detection is dormant unless a cache directory already exists. No runtime snapshot writes yet. |
| 1.5 | Settings, redaction, and inspection foundation | `snapshots` settings, redactor process contract, manifest readers, and minimal `rhei snapshot list/show/gc` support before any default runtime auto-emit writes transcripts. |
| 2 | Per-agent adapter spikes | Small prove-out scripts that exercise `claude --session-id`/`--resume`, `codex` resume surface, Gemini resume/path layout, and `pi --fork`. Findings written back into this spec; built-in profile blanks filled. |
| 3 | claude-code end-to-end | Manifest + atomic writes + claude profile + spawn-time wiring + emit on completion. Integration tests for a same-task two-state inheritance flow and a sub-task inheritance flow. Active orphan diagnostics begin with this first cache-writing runtime phase. |
| 4 | pi end-to-end | Pi profile with native `--fork`, underlying-provider/model parsing from JSONL headers, fanout per-target snapshots. |
| 5 | codex end-to-end | Per phase 2 spike outcome: either profile additions or new transport variant. |
| 6 | Interactive and run override surface | `rhei snapshot continue`, `--from-snapshot`, and the interactive-transport profile work `continue` needs for any agent whose built-in profile uses headless invocation by default in earlier phases. |
| 7+ | Deferred | Snapshot summarizer helpers, richer retention automation, and other operator tooling that does not turn snapshots into cross-agent transcript replay. |

Each phase ships standalone. Plans that do not reference snapshots are
unaffected at every phase.

Auto-emit lights up for an agent only after settings, redaction, and
list/show/gc inspection are available and that agent's `SessionLayout` is
resolved and wired up (phases 3–5 for supported built-in agents). There is
no discrete "auto-emit phase" — once an agent can be snapshotted at all,
it is snapshotted on every state exit by default, and `rhei snapshot
continue` becomes available later when the interactive transport is supported.

## 4. Configuration

### 4.1. Settings Block

Project and global `settings.json` may include a `snapshots` block:

```jsonc
{
  "snapshots": {
    "cache_dir": ".rhei/cache/snapshots",   // path relative to plan root
    "experimental": { "gemini_snapshots": false },
    "provider_cache_ttl": {
      "anthropic": "<provider-default>",
      "openai":    "<provider-default>",
      "google":    "<provider-default>"
    },
    "redactor": null                         // path to a redactor program; see Privacy
  }
}
```

`provider_cache_ttl` populates the `cache_beneficial` predicate's TTL term per
provider. The authoritative defaults live in the shipped settings template at
release time; placeholder values in examples are illustrative, not normative.

### 4.2. Privacy: Redaction Hook

If `snapshots.redactor` is set, rhei executes the named program on every
transcript file before writing it to the cache. The program receives the
staged transcript on stdin and must write the redacted form on stdout. A
non-zero exit aborts the snapshot write with a clear error. Redaction runs
inside the atomic-write window, before sha256 computation.

Redactor process contract:

- The redactor runs with cwd set to the plan workspace root.
- By default it receives a minimal environment containing only variables
  needed to locate rhei, the workspace, and the configured settings path:
  `RHEI_EXECUTABLE_PATH`, `RHEI_WORKSPACE_ROOT`,
  `RHEI_PROJECT_SETTINGS_PATH`, and `RHEI_GLOBAL_SETTINGS_PATH`.
  Projects may opt into additional environment variables through settings; rhei
  applies those allowlist overrides after setting the defaults and does not
  forward the full parent environment implicitly.
- Rhei applies a finite timeout. On timeout it sends the platform's normal
  termination signal, waits a short grace period, then kills the process and
  aborts the snapshot write.
- Stdin is the staged transcript bytes. Stdout is captured as the complete
  replacement transcript and is subject to the same best-effort size/resource
  limits as other runtime-captured output.
- Stderr is captured for diagnostics and may be truncated in logs; it is never
  written into `manifest.json`.
- The redactor path, exit status, timeout/truncation outcome, and stderr
  summary are logged to the run log. The manifest does not record whether a
  redactor ran.

The redaction hook is the supported privacy boundary. Gitignore alone is not
considered sufficient for transcripts containing secrets surfaced through
tool output.

**Redaction is intentionally opaque.** Because sha256 is computed on the
redacted bytes, downstream readers and inheritors cannot tell from the
manifest whether redaction ran, nor distinguish redacted from original
content. Two consequences follow:

1. Snapshots produced under different redactor configurations are not
   interchangeable. A snapshot whose transcript was scrubbed of tool output
   may preload "successfully" but yield a degraded preload because the
   downstream agent is missing context the upstream agent had. Operators who
   change `redactor` should treat the change as cache-invalidating.
2. There is no audit trail of redaction in v1. If an audit story is needed,
   write the redactor as a logging filter that records redaction events to a
   separate sink keyed by the eventual `transcript_sha256` (which the hook
   can compute by mirroring its own output before emitting). Building this
   into the manifest is reserved for a future schema version.

## 5. Open Questions

These are unresolved as of v1 of the snapshot specs and will be revisited as the
phased rollout progresses.

1. Whether `--session-id` is the correct flag for assigning a session id on
   claude-code. Determined by the adapter spike.
2. Whether `codex exec` supports session resume or whether a separate
   transport variant is required. Determined by the adapter spike.
3. Whether `gemini --resume` accepts a UUID directly or only an integer index
   from `--list-sessions`. Determined by the adapter spike.
4. The default `provider_cache_ttl` table contents at release time. The
   canonical defaults live in the `snapshots` block of the global
   `settings.json` template shipped with rhei; per-provider values are derived
   from each provider's published cache TTL immediately before v1 ships. This
   spec deliberately uses placeholders in examples so there is exactly one
   source of truth.
5. Whether `snapshot.emit.on: timeout` is worth exposing as a distinct policy
   alongside `failure` (timeout is currently bundled into `failure`).
6. Whether automatic GC by terminal task state is preferable to TTL-based GC
   for v2.
7. Whether auto-emit should be opt-out per state (e.g., `snapshot.auto:
   false`) for states where the operator should not be able to attach an
   analysis session — for example, states whose transcripts are known to
   contain credentials beyond what the redactor can scrub. Not in v1; the
   redactor hook plus path-level access controls on `.rhei/cache/snapshots/`
   are the supported privacy surface for now.
