# FS-rhei-snapshots: Rhei Session Snapshots Specification

This document defines the *session snapshot* model: how `rhei run` captures
an agent's transcript at supported agent-bearing state exits — for same-agent
state inheritance, ancestor-chain sub-task branching, operator analysis
sessions that attach to past states, and (when the provider permits it) for
prompt-cache benefits. §GOAL-rhei-outcomes

Snapshots come in two flavors that share the same storage and manifest
schema:

- *Auto-emitted* snapshots fire at every supported agent-state exit so the
  operator can attach an interactive analysis session to any past state via
  `rhei snapshot continue`. They do not participate in `inherit:`
  resolution.
- *Named* snapshots are produced when a state declares `snapshot.emit:`
  and consumed when a state declares `snapshot.inherit:`. They define
  explicit same-agent lineage between states on the same task and from
  ancestor tasks into descendant sub-tasks.

Plans that do not declare `snapshot.emit:` or `snapshot.inherit:` are
unaffected by the named-lineage grammar in this spec. They still receive
auto-emitted snapshots for supported agent states, so operator analysis is
available without state-machine configuration; this writes local transcript
cache data under `.rhei/cache/snapshots/` by default. The default redactor is
`null` unless project or global settings configure one, so "unaffected" means
no authored lineage grammar change, not "no on-disk transcript effect."

This spec depends on:

- §FS-rhei-states for state-machine schema and `target` selectors
- §FS-rhei-run for the orchestrator lifecycle and Completion Condition
- §FS-rhei-agents for agent transport profiles and the agents registry
- §FS-rhei-plan-language for task identifiers and the plan hierarchy

The CLI, run override, settings, redaction, rollout sequencing, and snapshot
cache maintenance commands are specified in §FS-rhei-snapshot-operations.

## 1. Goals

- Let one agent invocation reuse conversational state from the same agent's
  prior same-task history or from an ancestor-chain branch when the
  orchestrator can do so without surprising cost.
- Express snapshot lineage explicitly in the state machine so plan authors,
  validators, and the orchestrator all agree on what should happen.
- Treat each captured transcript as immutable; multiple inheritors must each
  receive an independent copy so branching is the natural shape.
- Keep session inheritance distinct from cross-task information flow:
  snapshots resume one agent's own transcript lineage; `outputs:` and
  `inputs:` carry facts, summaries, and other artifacts between arbitrary
  tasks or agents.
- Capture every supported agent-bearing state exit automatically so the
  operator can attach an interactive analysis session to past states via
  `rhei snapshot continue`, without anticipating intervention points in the
  state machine.

## 2. Non-Goals (v1)

- Cross-agent transcript translation or replay (Claude JSONL ↔ Gemini JSON ↔
  Pi JSONL ↔ Codex session state). When different agents need shared
  information, use task `outputs:` / `inputs:`.
- Transcript summarization as a snapshot transform. Authors can model this as a
  separate state that writes a summary as an `outputs:` artifact; snapshot
  preload stays single-source and same-agent.
- Working-tree snapshots, git ref capture, or file-state restoration. Snapshots
  carry transcript bytes and metadata only.
- Cross-workspace snapshot sharing. Snapshots are scoped to one plan workspace.

## 3. Core Model

A *snapshot* is an immutable record of one completed agent invocation, carrying
enough state to start a new agent invocation as a continuation or branch.

A snapshot is identified by the tuple:

```
(task_id, snapshot_name, emitting_state, visit, target_slug, generation)
```

- `task_id` is the plan-task identifier of the state that emitted the snapshot.
- `snapshot_name` is a stable name chosen by the state-machine author. Multiple
  states may share a name; the lineage rules below resolve which one applies.
- `emitting_state` is the state name that produced the snapshot. It is part of
  the identity so two states on the same task may intentionally emit the same
  logical snapshot name without racing for the same storage slot.
- `visit` is the counted-loop visit number of the emitting state (`1` for
  uncounted states).
- `target_slug` is the slug of the execution target that produced the snapshot
  (one per fanout invocation). See [Target Slug](#71-target-slug).
- `generation` is the re-emission counter; `1` for the first emission at a
  given `(task_id, snapshot_name, emitting_state, visit, target_slug)`,
  incremented if the state is re-executed.

Snapshots are immutable. Re-running a state never overwrites a snapshot;
instead it writes a new `generation` and updates a `current` pointer.

### 3.1. Auto-Emitted vs Named Snapshots

The orchestrator emits an *auto* snapshot at the exit of every agent-bearing
state (`final:`, `gating:`, and `program:` states are excluded — they have no
agent transcript). Auto snapshots use the reserved `snapshot_name` value
`_state`; the leading underscore makes the reservation unambiguous because
author-chosen names must match `^[a-z][a-z0-9-]*$`. Auto-emit fires with
`on: always` semantics, so failures and timeouts are captured. Auto
snapshots exist solely so `rhei snapshot continue` can reach any past state
for analysis; they are *not* candidates for `snapshot.inherit:` resolution.

A *named* snapshot is produced when a state declares `snapshot.emit:`. The
author chooses `name` and `on:`, and the snapshot is the source for any
`snapshot.inherit:` reference using that name. Named-emit fires *in
addition to* the auto-emit on the same state exit; the two snapshots share
the underlying agent invocation but are stored under independent identities
and may have different `on:` outcomes (an `on: success` named-emit on a
failed state does not fire while the auto-emit still does).
Until transcript-level deduplication exists, firing both writes a second
cached copy of the same transcript bytes under the named snapshot identity.
Cache-size impact and the GC controls that bound it are specified in §FS-rhei-snapshot-operations.

Auto-emit is best-effort by design: if the resolved agent profile has no
supported `SessionLayout`, auto-emit is silently skipped for that state —
operators simply cannot `rhei snapshot continue` into invocations of that
agent. Named-emit, by contrast, remains a hard author contract; the
defined `unsupported-snapshot-session` failure mode applies only to
named-emit.

## 4. State-Machine YAML Grammar

Auto-emit requires no state-machine configuration. The two per-state fields
below are optional and govern *named* lineage; declaring neither leaves the
state with only its auto-emit.

### 4.1. `snapshot.emit`

```yaml
states:
  pending:
    agent: claude-code
    snapshot:
      emit:
        name: implementation     # required
        on: success              # success | failure | always; default success
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Stable snapshot name. Must match `^[a-z][a-z0-9-]*$`, max 64 chars. |
| `on` | enum | No | Emit policy relative to the orchestrator's Completion Condition (§FS-rhei-run step 4). `success` emits only when exit code is 0 *and* required `outputs:` artifacts exist. `failure` emits whenever the Completion Condition fails, *including* the timeout case (a timed-out subprocess never satisfied the Completion Condition). `always` emits on any subprocess exit. Default: `success`. |

A state that does not declare `snapshot.emit:` produces no named snapshot
regardless of agent behavior; its auto-emitted `_state` snapshot still follows
the best-effort rules above when the state is a supported agent-bearing state.

`emit.on: failure` is the right choice when the downstream consumer is itself
an analysis or recovery flow for the same agent lineage — for example, a
forensic state that inspects what a failed invocation did, or a retry state
that wants to inherit the failed transcript as context. Timeouts are bundled
into `failure` and distinguished by the manifest's `completion` field rather
than by a separate `emit.on` value; see [Manifest Schema](#8-manifest-schema).

### 4.2. `snapshot.inherit`

```yaml
states:
  agent-review:
    agent: claude-code
    snapshot:
      inherit:
        name: implementation
        from: self                                # self | ancestor; default self
        compat: native                            # native | none; default native
        required: false                           # false | true; default false
        select:
          state: pending
          target: same                            # same | <target slug>
          visit: latest                           # latest | <integer>; default latest
          generation: current                     # current | latest | <integer>; default current
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Snapshot name to look up. |
| `from` | enum | No | Lineage axis. `self` walks the same task's prior state history. `ancestor` walks the plan ancestor chain. Default: `self`. |
| `compat` | enum | No | Preload policy. `native` preloads only when the snapshot's agent identity and session layout match the inheritor. Mismatches run cold or fail according to `required`. `none` never preloads even on match. Default: `native`. |
| `required` | boolean | No | When `false`, missing snapshots, incompatible snapshots, or unsupported agent session profiles are warnings and the agent runs cold. When `true`, those conditions are runtime errors before spawn. Default: `false`. |
| `select` | object | No | Disambiguates among multiple matching snapshots or overrides the default visit/generation choice. |
| `select.state` | string | Conditional | Emitting state name to select. Required when static analysis finds multiple possible emitters with the same `name` on the same task or ancestor chain. |
| `select.target` | string | Conditional | Target slug to select, or `same` to use the inheritor invocation's resolved target slug. Required when the source state declares `all_targets` or `all_models`. |
| `select.visit` | enum or integer | No | `latest` (default) selects the highest-numbered visit; an integer selects a specific visit. |
| `select.generation` | enum or integer | No | `current` (default) follows the live pointer; `latest` selects the highest-numbered orchestrator-produced generation independent of the pointer; an integer selects a specific generation. |

`select.target: same` is resolved per inheritor invocation. For a state with
`all_targets`, each fanout invocation receives the source snapshot whose
`target.slug` equals that invocation's own slug. For a state with one
`target`, `same` resolves to that target's slug. `same` is an error on a state
with neither `target`, `all_targets`, nor a legacy model/agent selection that
resolves to an effective target tuple. The comparison is slug-exact, including
mode when mode appears in the slug. It is intentionally stricter than
`native_compatible`, which ignores mode for file-format compatibility;
cross-mode reuse requires an explicit source target slug. §FS-rhei-states

`select.target: all` is not accepted in v1 because it would require
aggregating multiple fanout snapshots into one preload. Any other unsupported
`select.*` value is a parse error.

Selector defaults and fanout requirements:

| Source shape | Inheritor shape | `select.target` omitted | `select.target: same` |
|--------------|-----------------|-------------------------|------------------------|
| single resolved target (`target` or legacy `agent`/`model`) | single resolved target | Selects the source's only target when the remaining selectors are unambiguous. | Valid when the inheritor has a resolved target tuple; slug-exact. |
| `all_targets` source | any agent-bearing inheritor | Validation error; the source has multiple target identities. | Valid only when the inheritor has a resolved target tuple for the current invocation. |
| `all_models` source | any agent-bearing inheritor | Validation error; each resolved model invocation emits under its own target slug. | Valid only when the inheritor has a resolved target tuple for the current invocation. |
| any source | `all_targets` or `all_models` inheritor | Omission follows the source-shape rule per inheritor invocation. | Resolved separately for each fanout invocation. |

`select.visit` defaults to `latest`; `select.generation` defaults to
`current`. Integer selectors are 1-based and must be greater than or equal to
`1`.

### 4.3. Lineage Resolution

For `from: self`, resolution builds an ordered candidate set from the same
task's prior runtime history:

1. Scope candidates to completed prior visits of states on the same task.
2. Keep emissions whose `snapshot.emit.name` equals `inherit.name`.
3. Keep emissions whose emitting state matches `select.state` when provided.
4. Keep emissions whose `target.slug` matches `select.target`; `same` is
   replaced by the inheritor invocation's resolved target slug before this
   filter.
5. Apply `select.visit` (`latest` keeps the highest-numbered visit among the
   remaining candidates; an integer keeps only that visit).
6. Apply `select.generation` (`current` follows the identity's current
   symlink, `latest` keeps the highest orchestrator-produced generation, and
   an integer keeps only that generation).

After filtering, zero candidates trigger the normal missing-snapshot fallback.
One candidate is the resolved source. More than one candidate is an
ambiguous-lineage error; resolution never guesses between states, targets,
visits, or generations.

A state may declare both `snapshot.emit:` and `snapshot.inherit:` of the same
name; this is the natural shape for self-iterating counted loops. On visit `N`
of such a state, `select.visit: latest` resolves to the most recent prior
emission — typically visit `N - 1` — so each iteration inherits the previous
iteration's transcript. Visit `1` has no prior emission, so the rules for
"missing snapshot" apply (cold start under `required: false`, error under
`required: true`).

For `from: ancestor`, walk the plan hierarchy from the inheriting task's
parent upward. The first phase chooses the nearest ancestor that has at least
one completed emitter matching `inherit.name`, optional `select.state`, and
the emitter's `emit.on` policy. Ancestors with no such emitter are skipped.
Once that ancestor is chosen, apply `select.target`, `select.visit`, and
`select.generation` exactly as in the `from: self` chain. If those
post-filters leave zero candidates, resolution stops at that ancestor and
uses the normal missing-snapshot fallback; it does not climb farther looking
for a looser match. If they leave multiple candidates, resolution fails with
an ambiguous-lineage error and the author must add a more specific
`select:` clause.

Counted loops emit one snapshot per visit. `select.visit: latest` selects the
highest-numbered visit at lookup time.

With the default `emit.on: success`, failed visits in a counted loop do not
produce named snapshots. A later `select.visit: latest` may therefore resolve
to an older successful visit rather than the immediately preceding attempt.
Use `emit.on: always` when every iteration must be inheritable.

### 4.4. Inheritance Timing

Snapshot inheritance happens immediately before the state invocation that
declares `snapshot.inherit:`:

1. `rhei run` selects a ready task whose `Prior:` dependencies are terminal
   and whose required `inputs:` already exist. §FS-rhei-run
2. The orchestrator resolves the task's current state and execution target.
3. If that state declares `snapshot.inherit:`, the orchestrator resolves and
   preloads the source snapshot before spawning the agent.
4. The agent runs the state work.
5. After the agent exits and the Completion Condition is evaluated, the same
   state may emit a new snapshot if it declares `snapshot.emit:`.

Inheritance is therefore a property of the state being executed, not of the
transition that led into that state and not of the next state after it. A state
that declares both `inherit` and `emit` consumes first and emits after its own
invocation.

### 4.5. Cross-Task Information Flow

Snapshots are not an arbitrary task-to-task messaging mechanism. They preserve
same-agent session lineage for `from: self` and parent-to-child branching via
`from: ancestor`.

When one task needs facts, decisions, summaries, diffs, or other durable
content produced by another task, the producer should declare `outputs:` and
the consumer should declare `inputs:` plus an ordinary `Prior:` dependency.
That artifact path is the supported way to communicate across siblings,
cousins, unrelated tasks, and different agents. The snapshot grammar
therefore has no `from: task` or `from: prior` form in v1. §FS-rhei-states §FS-rhei-plan-language

### 4.6. Fallback Behavior

`required: false` has exactly one fallback: run the state cold. It does not
try a second snapshot name, a farther ancestor, or another emitting state.
Authors who need a real fallback chain should model it explicitly as states
and artifacts, so the plan records which branch was taken and why.

## 5. Compatibility Predicates

Native compatibility and cache compatibility are evaluated independently and
report different things.

```
native_compatible(snapshot, inheritor) :=
       snapshot.target.resolved.agent == inheritor.target.resolved.agent
    && snapshot_layout_matches(snapshot, inheritor)

snapshot_layout_matches(snapshot, inheritor) :=
       same_variant(snapshot.session_layout.kind,
                    layout_kind(inheritor.profile.session.layout))
    && same_layout_fields_that_affect_resume(snapshot.session_layout,
                                             inheritor.profile.session.layout)

cache_beneficial(snapshot, inheritor) :=
       native_compatible(snapshot, inheritor)
    && snapshot.observed_provider == inheritor.resolved_provider
    && snapshot.observed_model    == inheritor.resolved_model
    && (now() - snapshot.created_at) < provider_cache_ttl(snapshot.observed_provider)
```

`snapshot_layout_matches` checks that the snapshot's recorded
`session_layout.kind` and the inheritor profile's `SessionLayout` variant
match (`FlatById` ↔ `FlatById`, `PerProjectJson` ↔ `PerProjectJson`) and that
same-variant fields that affect resume compatibility also match. For
`FlatById`, the transcript extension must match. For `PerProjectJson`, the
extension, root-template expansion for the current project, and project-hash
derivation source must match. Diagnostic fields, display-only templates after
successful root expansion, and cache metadata that does not affect the native
resume path are intentionally ignored. Two agents with the same agent id but
mismatched compatible-layout fields — e.g. an upgrade that switched an agent
from JSONL files to a different extension or project-keyed root — are not
natively compatible. Snapshot inheritance across different agent ids is never
natively compatible in v1; those workflows must communicate through `outputs:`
and `inputs:` artifacts instead.

The `mode` portion of `target.resolved` (e.g. `[yolo]` vs. `[safe]`) is
*not* part of `native_compatible`. Mode affects spawn arguments and runtime
permissions, not transcript file format, so a snapshot produced under one
mode is reusable by a sibling state running the same agent in a different
mode. Operators who want stricter mode-matching can express it with
`select.target`, which is slug-exact.

`compat: native` gates preload on `native_compatible`. `cache_beneficial` is
advisory: it does not gate behavior but is logged at spawn time so the operator
sees whether the preload is expected to save tokens.

`compat: none` short-circuits to no preload regardless of either predicate.

Snapshots whose manifest records `completion: timeout` are not preloadable by
authored `snapshot.inherit:` and are not `cache_beneficial` by default, because
their native transcript may be truncated. Operators may still inspect or
continue from such snapshots explicitly through the CLI; the command must warn
that the source may be incomplete before spawning an interactive continuation.

Pi is multi-provider: `snapshot.observed_provider` and `observed_model` are
read from the pi session JSONL header at emit time and may differ from the
declared provider/model in the state machine. For other agents, declared and
observed are typically identical, but both are recorded.

## 6. Sub-Task Inheritance

Sub-task inheritance is the `from: ancestor` axis of the `inherit:` block.

A child sub-task entering a state with `snapshot.inherit.from: ancestor` looks
up the requested snapshot name across its ancestor chain. The match yields a
single source snapshot. The orchestrator copies the transcript into the
child's snapshot store and rewrites identifiers as needed so the child runs as
an independent branch:

- The child snapshot is *not* a re-emission of the source; it is a new
  invocation's own snapshot lineage. Its `parent_ref` records the source.
- Multiple children inheriting from the same source each get their own copy.
- Sibling sub-tasks running in parallel both inheriting from the same source
  is supported and constitutes a fork — each child receives a distinct
  session id under its own snapshot directory.

Sub-task inheritance never crosses workspace boundaries.

### 6.1. `parent_ref` Semantics

`parent_ref` is determined by the snapshot that was successfully preloaded
into the invocation that produced this emission:

- If the emitting state declares `snapshot.inherit:` and the orchestrator
  preloaded a source (native-compatible, not gated off by `compat: none`),
  `parent_ref` records that source's full identity tuple.
- If the emitting state declares `snapshot.inherit:` but the orchestrator
  ran cold (missing snapshot under `required: false`, incompatible agent,
  unsupported session profile, or `compat: none`), `parent_ref` is `null`.
  Running cold breaks the lineage chain by design.
- If the emitting state does not declare `snapshot.inherit:`, `parent_ref`
  is `null`.
- A sub-task branch created by `snapshot.inherit.from: ancestor` records the
  selected ancestor snapshot in the child's first emitted generation after the
  successful preload. The copied transcript in the child's store is an
  implementation staging artifact, not a separate author-declared emission.
- An operator generation produced by `rhei snapshot continue` records the
  selected source snapshot in `parent_ref` even though no state declares
  `snapshot.inherit:` for that interactive session.

Lineage in this spec is therefore a forest of transcripts whose edges
correspond to *successful* preloads, not to author intent. A subsequent
`rhei snapshot show` walking `parent_ref` always lands on a snapshot that
was actually loaded as context, not on a snapshot that was merely declared
as a source.

The sub-task copy and operator-continuation cases are explicit exceptions to
the normal state-declaration gate. They still require a real successful preload
and never synthesize a parent edge from a failed or skipped preload.

There is no `parent_refs` (plural) field in v1; the design only supports
single-source preload, so the parent edge is at most one.

## 7. Storage Layout

All snapshots live under the plan workspace cache:

```
.rhei/cache/snapshots/
  <task-id>/
    <snapshot-name>/                  # `_state` for auto-emits, author name for named
      <emitting-state>/
        <visit>/
          <target.slug>/
            g<N>/                     # generation directory, immutable
              manifest.json
              transcript.<ext>
            current -> g<N>           # symlink to the live generation
```

`<task-id>` follows the encoding in §FS-rhei-plan-language (single segment or
dotted form). `<snapshot-name>` is the reserved literal `_state` for
auto-emitted snapshots, or the value of `snapshot.emit.name` for named
snapshots; the leading underscore in `_state` guarantees no collision with
author names (which must match `^[a-z][a-z0-9-]*$`). `<emitting-state>` is
the canonical unsuffixed state name. `<visit>` is the integer visit count,
never `0`. `<N>` in `g<N>` starts at `1`.

`current` is a relative symlink to the active generation directory and is
updated atomically (write `current.tmp`, rename to `current`).

The cache root is gitignored by default. Plans may opt to commit selected
snapshots; this is a workspace-level decision outside the scope of `rhei run`.

### 7.1. Target Slug

The `<target.slug>` segment is derived from the resolved execution target by
this normalization:

1. Render the target as `<agent>[-<mode>]-<provider>-<model>`. Omit `<mode>`
   when no mode is declared.
2. Lowercase the result.
3. Replace every character not in `[a-z0-9._-]` with `-`.
4. Collapse runs of `-` to a single `-`.
5. Trim leading and trailing `-`.

Example: target `codex[safe]:openai:gpt-5-codex` becomes
`codex-safe-openai-gpt-5-codex`. Target `claude-code:anthropic:claude-opus-4-7`
becomes `claude-code-anthropic-claude-opus-4-7`. Target
`pi:openai:openai/gpt-4o` becomes `pi-openai-openai-gpt-4o`.

The raw selector string is recorded in `manifest.target.selector` so the
unnormalized form survives.

Validation rejects normalized target slug collisions only within a single
fanout state (`all_targets` or legacy `all_models`) because those invocations
can write the same `(task_id, snapshot_name, emitting_state, visit,
target_slug)` identity. If two raw selectors in that fanout set normalize to
the same `<target.slug>`, the state machine is invalid for snapshot-capable
execution; the raw selectors are preserved only for diagnostics and are not
used to disambiguate storage. The same target slug may appear elsewhere in the
resolved plan when another identity component differs.

### 7.2. Atomic Writes

A snapshot generation is written by this procedure:

1. Take an advisory lock scoped to the snapshot identity directory
   `(task_id, snapshot_name, emitting_state, visit, target_slug)`.
2. Allocate the smallest unused generation number `N` under that identity,
   ignoring stale temporary directories.
3. Create a unique staging directory named `g<N>.tmp-<nonce>/` as a sibling of
   the destination generation directory.
4. Copy or rename the native agent transcript into the staging directory as the
   canonical `transcript.<ext>` file.
5. Write `manifest.json` inside the staging directory with `generation: N`,
   compute `manifest.transcript_sha256`, and verify before finalization.
6. `rename(g<N>.tmp-<nonce>, g<N>)`. On POSIX this is atomic for directories
   when the target does not exist.
7. For orchestrator-produced generations only, update the `current` symlink:
   write `current.tmp-<nonce>` pointing at `g<N>`, then rename it to
   `current` atomically. Operator-produced generations do not advance
   `current`.

Writers may remove stale `g<N>.tmp-*` directories for the same identity when
they can prove no live writer owns them. Stale-temp cleanup is best-effort and
must not delete finalized `g<N>` directories.

Readers may observe `current` mid-update only across the two atomic renames;
the directory it points at is always complete. A reader that opens `g<N>/`
directly never sees a partial state because step 4 never overwrites.

Readers that need both the manifest and the transcript should resolve
`current` once with `realpath` and use the resulting absolute path
(`.../g<N>/manifest.json`, `.../g<N>/transcript.<ext>`) for subsequent
opens. Reading `current/manifest.json` and then `current/transcript.<ext>`
in two syscalls is not safe — a concurrent re-emission can repoint
`current` between the opens, returning a manifest and transcript from
different generations.

If finalizing `g<N>` fails because the destination already exists, the writer
discards the staging directory, re-reads the identity directory under the lock,
allocates the next smallest unused generation, rewrites the manifest with the
new `generation`, and retries. This covers crashes, stale temps, and
operator-driven concurrency without ever reusing a generation number.

## 8. Manifest Schema

`manifest.json`, version `1`:

```jsonc
{
  "version": 1,
  "rhei_version": "x.y.z",

  "snapshot_name": "implementation",
  "task_id": "1.2.3",
  "emitting_state": "pending",
  "visit": 1,
  "generation": 1,

  "target": {
    "selector": "claude-code[yolo]:anthropic:claude-opus-4-7",
    "slug": "claude-code-yolo-anthropic-claude-opus-4-7",
    "resolved": {
      "agent": "claude-code",
      "mode": "yolo",
      "provider": "anthropic",
      "model": "claude-opus-4-7"
    }
  },

  "declared_provider": "anthropic",
  "declared_model": "claude-opus-4-7",
  "observed_provider": "anthropic",
  "observed_model": "claude-opus-4-7",

  "session_id": "01910c5e-7b3a-7c4d-9e1f-1f9b3a4d5c6e",
  "session_layout": {
    "kind": "FlatById",
    "ext": "jsonl"
  },
  "transcript_path": "transcript.jsonl",
  "transcript_sha256": "0a3f...",
  "transcript_bytes": 142387,

  "parent_ref": {
    "task_id": "1.2",
    "snapshot_name": "research",
    "emitting_state": "draft",
    "visit": 1,
    "target_slug": "claude-code-anthropic-claude-opus-4-7",
    "generation": 1
  },

  "created_at": "2026-05-18T08:14:22Z",
  "completion": "success",
  "produced_by": "orchestrator"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `version` | integer | Yes | Manifest schema version. `1` for this spec. |
| `rhei_version` | string | Yes | The rhei build that wrote this manifest. Diagnostic only. |
| `snapshot_name` | string | Yes | Matches the path component. |
| `task_id` | string | Yes | Plan-task id. |
| `emitting_state` | string | Yes | The state name that declared `snapshot.emit:`. Useful when one snapshot name is emitted by multiple states. |
| `visit` | integer | Yes | Counted-loop visit number, ≥ 1. |
| `generation` | integer | Yes | Generation counter, ≥ 1. |
| `target.selector` | string | Yes | Raw target selector verbatim. |
| `target.slug` | string | Yes | Normalized slug, matches the path component. |
| `target.resolved` | object | Yes | Resolved execution identity. `mode` is omitted when not declared. |
| `declared_provider` | string | Yes | Provider as declared by the state machine. |
| `declared_model` | string | Yes | Model as declared by the state machine. |
| `observed_provider` | string | Yes | Provider observed at runtime (e.g., parsed from a pi session header). Equal to `declared_provider` when the agent does not route. |
| `observed_model` | string | Yes | Model observed at runtime. |
| `session_id` | string | Yes | The session id used by the agent for this invocation. May be agent-defined or rhei-assigned. |
| `session_layout.kind` | enum | Yes | The `SessionLayout` variant used to locate and copy the transcript: `FlatById` or `PerProjectJson` in v1. |
| `session_layout.ext` | string | Yes | Transcript file extension produced by the layout, without a leading dot. |
| `transcript_path` | string | Yes | Relative path within the generation directory. |
| `transcript_sha256` | string | Yes | SHA-256 of the transcript file contents. |
| `transcript_bytes` | integer | Yes | Transcript size in bytes. Used for CLI listings, diagnostics, and retention decisions. |
| `parent_ref` | object or null | Yes | Reference to the snapshot whose transcript was successfully preloaded into this invocation, including `task_id`, `snapshot_name`, `emitting_state`, `visit`, `target_slug`, and `generation`. `null` when the invocation ran cold (no `inherit:`, missing source, incompatible agent, unsupported session profile, or `compat: none`). See [`parent_ref` Semantics](#61-parent_ref-semantics). |
| `created_at` | string (RFC 3339) | Yes | Wall-clock time of generation finalization. |
| `completion` | enum | Yes | `success`, `failure`, or `timeout`. Matches the orchestrator's classification of the underlying subprocess exit. Operator emissions use `success` or `failure` only — the agent-timeout guardian does not run for `rhei snapshot continue` sessions. |
| `produced_by` | enum | Yes | `orchestrator` for emissions produced by `rhei run`. `operator` for emissions produced by `rhei snapshot continue`. Operator emissions never advance `current`; they are sibling generations under the same identity as the source snapshot. |

## 9. Agent Transport Integration

### 9.1. `CustomAgentProfile.session`

A new optional nested field on `CustomAgentProfile` (§FS-rhei-agents):

```rust
pub struct CustomAgentProfile {
    // ... existing fields ...
    #[serde(default)]
    pub session: Option<AgentSessionProfile>,
}

pub struct AgentSessionProfile {
    pub resume:           ResumeStrategy,
    pub fork:             Option<ForkStrategy>,
    pub interactive:      Option<InteractiveContinuationProfile>,
    pub assign_id_flag:   Option<String>,
    pub session_dir_flag: Option<String>,
    pub no_session_flag:  Option<String>,
    pub layout:           SessionLayout,
}

pub enum ResumeStrategy {
    /// Agent does not expose resume. Snapshot inheritance for this agent
    /// cannot preload and follows the state's `required` behavior.
    None,
    /// Pass the captured session id back with the given flag, in place.
    Native { flag: String },
    /// Rhei copies and rewrites the session file before spawn, then resumes.
    CopyAndResume { flag: String },
}

pub enum ForkStrategy {
    /// Agent exposes a native fork flag taking a path or partial id.
    Native { flag: String },
    /// Rhei copies the session file itself; agent does not need a fork flag.
    RheiCopy,
}

pub struct InteractiveContinuationProfile {
    /// Optional command override for `rhei snapshot continue`. When absent,
    /// the agent profile's base `command` is used.
    pub command: Option<Vec<String>>,
    /// Arguments that preserve TTY pass-through for `rhei snapshot continue`;
    /// headless prompt transports alone are not sufficient. Defaults to an
    /// empty list.
    pub args: Vec<String>,
}

pub enum SessionLayout {
    /// One file per session at `<dir>/<id>.<ext>`. Claude Code, Codex, Pi.
    FlatById { dir_template: String, ext: String },
    /// Per-project subdirectory with one JSON file per session and an
    /// agent-internal project hash. Gemini.
    PerProjectJson { root_template: String, project_hash: ProjectHashSource, ext: String },
}

pub enum ProjectHashSource {
    /// Read the agent's projects index (e.g. `~/.gemini/projects.json`) to
    /// learn the hash for the current workspace.
    AgentProjectsFile { path: String },
    /// Compute the hash with a built-in function.
    Computed { algorithm: ProjectHashAlgorithm },
}
```

The enum-over-flags shape forces every profile to declare a *strategy* rather
than relying on flag presence. A `ResumeStrategy::None` profile is
self-documenting: snapshot preload cannot run for that profile unless a
`ForkStrategy` is declared, and `rhei validate` emits a hint when state
machines reference a profile with neither source-loading strategy under an
optional `inherit:` block.

`interactive.command` is only needed when the agent's TTY continuation command
differs from its headless `rhei run` command. `interactive.args` defaults to
an empty list and is appended after profile mode/model flags and before
snapshot resume/fork/session-dir flags. `rhei snapshot continue` inherits
stdin, stdout, and stderr so the operator talks to the agent directly.

`session_dir_flag` and `no_session_flag` are profile-level affordances. Rhei
uses `session_dir_flag` when provided to redirect the agent's session output
into the active `g<N>.tmp-*` staging directory so emit becomes a directory scan
rather than a hunt across the user's home directory.

### 9.2. Built-in Profiles

| Agent | `resume` | `fork` | `interactive` | `assign_id_flag` | `session_dir_flag` | `no_session_flag` | `layout` |
|-------|----------|--------|---------------|------------------|--------------------|--------------------|----------|
| claude-code | unsupported in v1 built-in profile | unsupported in v1 built-in profile | unsupported in v1 built-in profile | unsupported in v1 built-in profile | none | none | none |
| codex | unsupported in v1 built-in profile | unsupported in v1 built-in profile | unsupported in v1 built-in profile | unsupported in v1 built-in profile | none | none | none |
| cursor | unsupported in v1 | unsupported | unsupported | none | none | none | none |
| gemini | unsupported in v1 built-in profile | unsupported in v1 built-in profile | unsupported in v1 built-in profile | unsupported in v1 built-in profile | none | none | none |
| kilocode | unsupported in v1 | unsupported | unsupported | none | none | none | none |
| pi | `Native { flag: --continue }` | `Native { flag: --fork }` | base `pi` TTY command with no extra args | none | `--session-dir` | `--no-session` | `FlatById { dir: <session_dir>/, ext: jsonl }` |

The v1 built-in support boundary is intentionally conservative. Pi has a
complete native surface for Rhei-managed snapshot sessions: an interactive TTY
mode, `--fork <path>`, `--session-dir <dir>`, and a flat JSONL transcript
layout. Other built-in agents remain usable through custom session-capable
profiles, but their built-in profiles do not declare snapshot sessions until a
Rhei-readable transcript layout and safe continuation transport are proven.

Emit and preload have independent profile requirements:

- *Emit* requires only a `SessionLayout` (rhei needs to know where the agent
  wrote its session file). An agent with `ResumeStrategy::None` but a valid
  `SessionLayout` can still emit snapshots; those snapshots become preload
  sources only if the profile also declares a `ForkStrategy`.
- *Preload* requires both a `SessionLayout` and a usable source-loading
  strategy: either a `ResumeStrategy` other than `None` or a `ForkStrategy`.

An agent whose profile has no `session` block, or whose `SessionLayout` is
unset, is unsupported for *either* emit or preload. An agent with a layout
but neither resume nor fork is unsupported for preload only. Unsupported
preload follows the same rule as a missing or incompatible snapshot:
`required: false` runs cold with a warning, and `required: true` fails
before spawn. Unsupported emit fails the spawn with
`unsupported-snapshot-session`.

Claude Code, Codex, Gemini, Cursor, and Kilocode have no built-in snapshot
session profile in v1. Auto-emit is skipped for their built-in profiles, and
explicit `snapshot.emit:`, required preload, or `rhei snapshot continue` fails
with `unsupported-snapshot-session` unless the user replaces the built-in
profile with a custom snapshot-capable session block.

### 9.3. Per-Agent Runtime Behavior

#### 9.3.1. Pi

Pi has the most complete native session surface. The flow on inheritance:

1. Compute the target generation directory under the inheritor's snapshot
   cache path. Create the unique `g<N>.tmp-*` staging directory.
2. Render the spawn command with `--session-dir <staging-dir> --fork <source
   transcript path>` appended. Pi creates a new JSONL session in the target
   staging directory with a fresh UUID, seeded from the source.
3. After the subprocess exits and the Completion Condition is evaluated,
   locate the newest JSONL in `<staging-dir>`, rename or copy it to canonical
   `transcript.jsonl`, compute its sha256, write `manifest.json`, rename
   `g<N>.tmp-*` to `g<N>`, and update `current`.

Pi's session JSONL header carries the underlying provider and model. The emit
path parses that header to populate `observed_provider` and `observed_model`.
The parser scans a small leading window and skips ordinary non-header JSONL
records until a provider/model header is found. If the header is absent or
unparsable, the snapshot is still written with `observed_*` set equal to
`declared_*` and a warning is logged; downstream inheritors that require
`cache_beneficial` will see the same advisory.

Interactive continuation uses the same built-in session profile but spawns the
base `pi` command without the headless `-p` prompt flag. Rhei appends the model
flag, `--session-dir <runtime snapshot session dir>`, and `--fork <source
transcript>` so the operator enters a TTY session seeded from the selected
snapshot. Captured continuations read the newest JSONL written to that session
directory and store it as an operator generation. §FS-rhei-snapshot-operations.1.5

#### 9.3.2. Gemini

Gemini snapshot support is unsupported for the v1 built-in profile. The CLI
has resume/session concepts, but the Rhei adapter does not yet have a stable
session directory hook and per-project transcript layout. The built-in Gemini
profile therefore has no supported `SessionLayout` or `ResumeStrategy`;
auto-emit is skipped, and explicit snapshot operations fail with
`unsupported-snapshot-session`.

The candidate design remains a copy-and-rewrite flow using Gemini's project
hash and JSON session files, but those details are non-normative until the
spike resolves whether `--resume`, `--session-id`, and project-hash lookup are
stable enough to support. Upstream feature request: a `--session-file <path>`
flag analogous to pi's.

#### 9.3.3. Claude Code

Claude Code snapshot support is unsupported for the v1 built-in profile. The
CLI exposes interactive mode, `--resume`, and `--session-id`, but Rhei does not
have a built-in `session_dir_flag` or a stable transcript layout for capturing
the new native transcript. The built-in claude-code profile leaves the
`session` block unset. Snapshot inheritance for that agent therefore runs cold
unless the inheriting state sets `required: true`, in which case the run fails
before spawn with `unsupported-snapshot-session`; `rhei snapshot continue`
fails with the same diagnostic.

#### 9.3.4. Codex

The current rhei transport for codex is `codex exec` (§FS-rhei-agents). The
adapter spike must determine whether `codex exec` supports session resume,
or whether snapshot integration requires a separate transport variant
(`codex resume`, or a future explicit subcommand).

Emit and inherit have independent dependencies for codex:

- *Emit* requires only that `codex exec` writes a session transcript to a
  known location. If it does, `SessionLayout` can be populated after the spike
  and `snapshot.emit:` becomes supported for codex without any resume work.
- *Inherit* requires a working `ResumeStrategy`, which is the unknown the
  spike is investigating.

The built-in codex profile leaves the `session` block unset in v1, which makes
emit, inherit, and `rhei snapshot continue` unsupported (per the rules in
[Built-in Profiles](#92-built-in-profiles)). Codex exposes interactive
`resume` and `fork` subcommands, but the built-in Rhei adapter does not yet
have a stable transcript capture layout or `session_dir_flag` equivalent.

## 10. Runtime Behavior

### 10.1. Spawn-Time Preload

For each spawn of a state declaring `snapshot.inherit:`:

1. Resolve the snapshot reference per the lineage rules. If no match exists
   and `required: true`, fail with `missing-snapshot`. If no match exists and
   `required: false`, log "no snapshot found for inherit: <name>; running
   cold" and proceed without preload.
2. If `compat: none`, log "snapshot preload disabled by compat: none" and
   spawn without preload.
3. If the resolved snapshot has `completion: timeout`, fail before spawn when
   `required: true`; otherwise warn that timed-out snapshots are not
   preloadable and run cold.
4. Evaluate `native_compatible`. If `false` and `compat: native`, fail with
   `incompatible-snapshot` when `required: true`; otherwise log "preload
   skipped: incompatible agent (<reason>); running cold" and proceed without
   preload.
5. Evaluate `cache_beneficial`. If `false`, log the specific reason
   (provider mismatch, model mismatch, TTL exceeded) at info level so the
   operator sees the cost implications. This is advisory; the preload still
   proceeds.
6. If the inheritor's agent profile has no supported session strategy, fail
   with `unsupported-snapshot-session` when `required: true`; otherwise warn
   and proceed without preload.
7. Apply the agent's `ResumeStrategy` / `ForkStrategy` to stage the session
   into the inheritor's generation directory.
8. Spawn the subprocess with the strategy-defined flags appended.

### 10.2. Emit on Exit

After every agent-state subprocess exits, the orchestrator:

1. Evaluates the orchestrator's Completion Condition (§FS-rhei-run step 4).
2. Selects the outgoing transition according to §FS-rhei-run step 5, including
   normal success transitions, error or timeout routing, and poll self-loop or
   exhaustion behavior. The transition is not applied until after snapshot
   emit decisions are complete. This ordering is what lets a poll self-loop
   selection suppress both auto- and named-emit for the attempt — see
   [Counted Loops, Fanout, and Polling](#103-counted-loops-fanout-and-polling).
3. Classifies completion: `success` if exit code is `0` and required outputs
   exist; `timeout` if the subprocess was killed by the agent-timeout
   guardian; `failure` otherwise.
4. Writes the *auto-emit* snapshot under `snapshot_name = "_state"` with
   `produced_by: orchestrator`, regardless of completion classification.
   If the resolved agent profile has no supported `SessionLayout`, the
   auto-emit is silently skipped (logged at info level) — operators simply
   cannot `rhei snapshot continue` into that state. Auto-emit is implicit
   and best-effort by design; absence of an explicit contract means
   absence of an explicit failure.
5. If the state declares `snapshot.emit:` and the classification matches
   `emit.on`, writes the *named* snapshot under `snapshot_name =
   <emit.name>` with `produced_by: orchestrator`. If the resolved agent
   profile has no supported `SessionLayout`, fails the spawn with
   `unsupported-snapshot-session`; `snapshot.emit:` is an explicit author
   contract, not a best-effort hint.
6. Both writes use the atomic-write procedure. When both fire for the same
   invocation they produce independent generation directories under their
   respective identities; the underlying transcript bytes are typically
   identical but are written twice to keep the two snapshots independently
   addressable. Future versions may collapse the duplication with
   hardlinks once a portability story exists.

Skipped cases. Auto-emit is suppressed for `final:`, `gating:`, and
`program:` states (these have no agent transcript). On poll self-loop
attempts, both auto- and named-emit are suppressed; they fire only on the
terminal exit transition, matching the rule in
[Counted Loops, Fanout, and Polling](#103-counted-loops-fanout-and-polling).

The orchestrator owns this step; agents do not invoke snapshot emit
directly. This matches §FS-rhei-run's invariant that the subprocess never
calls `rhei transition`.

### 10.3. Counted Loops, Fanout, and Polling

Counted-loop states (`visits: n`) emit a separate snapshot per visit at
`<visit>/...`. Each visit's generation counter starts at `1` independently.

Fanout states (`all_targets: [...]` and legacy `all_models: [...]`) emit one
snapshot per resolved target slug. The state's `emit.on` policy applies per
target invocation; the snapshot for a target whose invocation failed is omitted
(under `on: success`) without affecting the snapshots for siblings that
succeeded. `all_models` is normalized through the same effective target tuple
as `all_targets`, so each model-specific invocation writes under its own
`target.slug`.

A fanout state that also declares `snapshot.inherit:` resolves `select:` for
each per-target invocation. With an explicit `select.target: <slug>`, every
inheritor target preloads that same source snapshot. With `select.target:
same`, each inheritor target preloads the source snapshot whose `target.slug`
matches its own resolved target slug. Each per-target invocation receives its
own copy under its own `target.slug` generation directory; copies are
independently addressable lineage roots even when their source content is the
same.

Polling states (`poll:`) do not emit snapshots on self-loop attempts. Emit
fires only on the terminal exit transition (the poll succeeded, or the
exhaustion transition fired). `snapshot.inherit` on a polling state is rejected
in v1: preserving a staged native session across delayed attempts would require
a broader lifecycle contract than this spec defines.

## 11. Validation Rules

The validator (§FS-rhei-states) extends the per-state checks with the
following rules. Violations are errors unless marked otherwise.

- `snapshot.emit.name` and `snapshot.inherit.name` must match
  `^[a-z][a-z0-9-]*$` and be ≤ 64 characters.
- `snapshot.emit` and `snapshot.inherit` are independently optional; either,
  both, or neither may appear on a state.
- `snapshot`, `snapshot.emit`, `snapshot.inherit`, and
  `snapshot.inherit.select` are closed objects. Unknown keys are validation
  errors.
- `snapshot.emit.on`, when present, must be one of `success`, `failure`, or
  `always`.
- `snapshot.inherit.from`, when present, must be `self` or `ancestor`.
- `snapshot.inherit.compat`, when present, must be `native` or `none`.
- `snapshot.inherit.select.visit`, when present, must be `latest` or an
  integer greater than or equal to `1`.
- `snapshot.inherit.select.generation`, when present, must be `current`,
  `latest`, or an integer greater than or equal to `1`.
- `snapshot.emit` on a `final: true` state is an error (terminal states have
  no work).
- `snapshot.emit` on a `gating: true` state is an error (gating states have
  no autonomous execution).
- `snapshot.emit` on a state with `program:` set is an error (programs are
  not agents; they have no transcript to snapshot).
- `snapshot.inherit` on a `final: true` state is an error (terminal states
  have no work).
- `snapshot.inherit` on a `gating: true` state is an error (gating states
  have no autonomous execution).
- `snapshot.inherit` on a state with `program:` set is an error (programs do
  not consume agent transcripts).
- `snapshot.inherit` on a state with `poll:` set is an error in v1. Polling
  states may emit only on their terminal exit transition.
- `snapshot.inherit.from: ancestor` on a root-task state is an error. There
  is no ancestor.
- `snapshot.inherit.required`, when present, must be a boolean.
- `snapshot.inherit.required: true` with `snapshot.inherit.compat: none` is
  an error because the state both requires and disables preload.
- `snapshot.inherit.select.state`, when present, must name a defined state.
- `snapshot.inherit.select.target: same` is valid only when the inheriting
  state declares `target`, `all_targets`, or legacy `all_models`, or when
  legacy `agent`/`model` fields resolve to an effective target tuple.
- `snapshot.inherit.select.target: all` is an error in v1.
- A `snapshot.inherit:` whose `from`/`name` combination resolves to no
  possible emitter under static analysis of the state machine is an error
  (unresolvable reference).
- A `snapshot.inherit:` whose static resolution is ambiguous is an error.
  For `from: self`, ambiguity means the same task contains two states that
  could both be the source under the declared `name` and `emit.on` policy
  with no `select.state` to disambiguate. For `from: ancestor`, ambiguity is
  evaluated at the *nearest matching ancestor* per the
  [Lineage Resolution](#43-lineage-resolution) walk: an ancestor with two
  matching emitters errors here even if a farther ancestor has exactly one
  match, because the runtime walk stops at the nearest matching ancestor
  rather than falling through.
- A `snapshot.inherit:` whose static analysis shows the emitter's agent does
  not match the inheritor's agent is an error when `required: true`;
  otherwise it emits a warning ("preload will be skipped: snapshot agent does
  not match inheritor agent; running cold"). Snapshot inheritance is same-agent
  only; use `outputs:` and `inputs:` to communicate across agents.
- A `snapshot.emit:` whose statically resolved agent profile has no supported
  session layout is an error. If the agent can be resolved only at runtime,
  the same condition fails the spawn with `unsupported-snapshot-session`.
  This rule applies only to named-emit. Auto-emit on an agent with no
  supported session layout is silently skipped at runtime and produces no
  validation diagnostic; the leading underscore in `_state` keeps the
  reserved auto-emit name unambiguous and the absence of an explicit
  author contract makes a hard error inappropriate.
- The snapshot name `_state` is reserved for auto-emits. The
  `snapshot.emit.name` regex `^[a-z][a-z0-9-]*$` already forbids the
  leading underscore, so authors cannot accidentally collide with it; a
  YAML using `_state` as a snapshot name is rejected at parse time by the
  existing regex check.
- A `snapshot.inherit:` referencing a fanout source (an emitter with
  `all_targets` or `all_models`) must declare `select.target`. The value may
  be an explicit target slug or `same`; both forms satisfy this rule.
  Omitting `select.target` entirely is an error.
- Snapshot operations require a resolved effective target tuple `(agent, mode?,
  provider, model)`. `target` and `all_targets` provide that tuple directly.
  Legacy `agent`/`model` or `all_models` states may use snapshots only when
  normal resolution can derive agent, optional mode, provider, and model. When
  no effective tuple exists, auto-emit is skipped and explicit
  `snapshot.emit:` or `snapshot.inherit:` is rejected with
  `snapshot-requires-target`.
- Normalized `target.slug` values must be unique within the resolved plan and
  fanout set. A collision between two raw selectors is a validation error.
- A snapshot whose manifest records `completion: timeout` is not a valid
  authored inheritance source. Static checks reject known timeout-only sources;
  runtime checks reject timed-out generations selected dynamically.
- Manifest validation requires `completion` to be `success`, `failure`, or
  `timeout` when `produced_by: orchestrator`, and only `success` or `failure`
  when `produced_by: operator`.
- `snapshot.inherit:` may not depend on a snapshot in a different plan
  workspace.

Orphaned-snapshot detection runs at validation time when the cache directory
exists: each manifest is compared against the current plan and state machine.
Snapshots for tasks that no longer exist, emitting states that no longer exist,
or target slugs that no longer resolve emit informational warnings, never
errors.
