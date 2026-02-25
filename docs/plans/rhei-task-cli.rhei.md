# Rhei: Task Management CLI

## Context

Add a small, Rhei-native task-management command surface inspired by beads, without
turning Rhei into a database-backed issue tracker. Markdown remains the source
of truth. The CLI should handle structural edits that are easy to get wrong by
hand: adding tasks, adding child tasks, updating dependency edges, showing
dependency trees, listing children, and making limited title/body/note edits.

Do not add epic commands: a Rhei plan or Directory Workspace is already the
epic-level container. Do not add labels/tags, external tracker sync, reopen,
stale/history, duplicate merge, memory compaction, or full `bd update` parity in
this slice.

The current CLI entrypoint is `crates/rhei-cli/src/main.rs`. Existing read-only
listing behavior is specified in `docs/specs/rhei-list.spec.md`; new structural
mutation behavior should be specified in `docs/specs/rhei-task-cli.spec.md`.

## Command Surface

The intended v1 commands are:

- `rhei add <plan> --id <id> --title <title> [--state <state>] [--prior <id>]... [--body <text>|--body-file <path>]`
- `rhei add <plan> --parent <id> --id <child-id> --title <title> ...`
- `rhei dep add <plan> --task <id> --prior <id>`
- `rhei dep remove <plan> --task <id> --prior <id>`
- `rhei dep tree <plan> [--task <id>]`
- `rhei children <plan> --task <id>`
- `rhei edit <plan> --task <id> [--title <title>] [--body <text>|--body-file <path>]`
- `rhei note <plan> --task <id> --message <text>|--message-file <path>`

State changes stay under `rhei transition` and `rhei complete`. Runtime-owned
`**Assignee:**` and `> **Result:**` blocks must be preserved by every command.
All mutating commands should support `--dry-run`, which validates the requested
change and prints a unified diff without writing.

## Review Notes

Claude reviewed this plan after the first draft. Accepted changes:

- Make defaults and failure policy explicit for profile-based initial-state
  selection, forward references, locking, ID validation, and malformed input.
- Define note and body replacement boundaries up front.
- Keep `children` in v1 for discoverability, but keep `dep tree` text-only in
  v1 to avoid expanding the JSON surface unnecessarily.
- Add `--dry-run` for source-of-truth markdown mutations.
- Expand tests around runtime-owned block preservation, max-depth boundaries,
  cross-file cycles, and child-subtree preservation.

Rejected or deferred comments:

- `dep list` is deferred; `rhei list --has-prior` and `dep tree --task` cover
  enough v1 inspection.
- Full JSON for dependency trees is deferred until there is a concrete consumer.

## Tasks

### Task 1: Specify task-management command UX
**State:** pending

Write `docs/specs/rhei-task-cli.spec.md` covering the core CRUD command surface.
The spec should define usage, arguments, flags, human-readable output, JSON
output only for `children` compatibility with `rhei list --json`, error cases,
single-file behavior, Directory Workspace behavior, `--dry-run` behavior, and
the precise markdown mutation contract.

Include these policy decisions:

- New root tasks are appended at the end of the plan's `## Tasks` section, or as
  a new `tasks/<slug>.md` file for Directory Workspaces.
- New child tasks are appended under the parent task in that parent's source
  file.
- `--state` defaults to the new node's resolved profile initial state when
  omitted. Resolve the profile through the active state machine's `node_policy`
  using the new node kind, id, parent/depth, and plan structure; do not require
  or infer a machine-wide initial state.
- `--prior` may be repeated and writes one canonical `**Prior:**` line.
- `--prior` never accepts forward references; every referenced task must already
  exist in the merged plan or workspace.
- Task IDs must use the existing Rhei task-id grammar; child IDs must add
  exactly one dotted segment to the parent ID.
- `edit` may change title and body only; it must not mutate state, assignee,
  priors, or result links.
- `edit --body` replaces only the free-form task body between metadata and the
  first child heading; it preserves runtime-owned metadata and child subtrees.
- `note` appends a dated note without changing task state. The note format is:
  `> **Note <UTC RFC 3339 timestamp>:** <message>`, inserted at the end of the
  free-form body before any child task headings.
- `--body-file -` and `--message-file -` read from standard input. Inline text
  and file/stdin forms are mutually exclusive.
- `dep tree` is text-only in v1. `children --json` reuses the existing `rhei
  list --json` shape.
- Mutating commands acquire the same style of file lock used by existing task
  mutation commands. Concurrent writers must fail cleanly instead of silently
  overwriting each other.

### Task 2: Add shared task-location and markdown-rewrite helpers
**State:** pending
**Prior:** Task 1

Implement shared helpers in the CLI for locating a task's source file, heading
span, metadata span, body span, child insertion point, and workspace root. The
helpers must work for both single-file plans and Directory Workspaces.

The implementation should preserve unrelated markdown byte-for-byte whenever
possible. It must preserve runtime-owned `**Assignee:**` lines and `> **Result:**`
links, and it must keep `**State:**` as the first metadata line and `**Prior:**`
as the second metadata line when a prior list exists.

Make the helper contract stricter than "best effort":

- Validate the loaded plan before mutation and reject malformed metadata order
  instead of canonicalizing unrelated legacy content.
- Preserve bytes outside the exact replaced or inserted span, including existing
  spacing and line endings where practical.
- Reuse a shared post-write validation helper for every successful mutation.
- For Directory Workspaces, always load the merged workspace graph for
  validation and cycle detection, not just the file being edited.
- Compute child insertion depth before writing and reject `add --parent` when
  it would exceed `structure.maxLevels`.

### Task 3: Implement `rhei add`
**State:** pending
**Prior:** Task 1, Task 2

Add the `add` subcommand to create root tasks and child tasks. Validate before
writing and again after writing.

Required behavior:

- Reject duplicate task IDs.
- Reject missing parents when `--parent` is supplied.
- Reject child IDs that do not extend the parent by exactly one segment.
- Reject heading depth beyond the active `structure.maxLevels`.
- Reject invalid state names after state-machine alias normalization.
- Reject missing, invalid, or cycle-creating `--prior` references.
- Reject forward references in `--prior`.
- Support body text from `--body` or `--body-file`, with those flags mutually
  exclusive.
- Print the new task ID and source path on success. With `--dry-run`, print the
  would-be source path and a unified diff without writing.

### Task 4: Implement `rhei dep add|remove|tree`
**State:** pending
**Prior:** Task 1, Task 2

Add a `dep` command group with `add`, `remove`, and `tree` subcommands.

`dep add` and `dep remove` should mutate only the target task's `**Prior:**`
line. They must preserve existing prior order, avoid duplicates, delete the
line when the last prior is removed, reject missing task IDs, and reject
cycle-creating additions. Adding the same prior twice should be an idempotent
success that reports no change.

`dep tree` is read-only. It should print a deterministic dependency tree for a
specific task when `--task` is supplied, and a full dependency forest when it is
omitted. Do not add JSON output for `dep tree` in v1.

### Task 5: Implement `rhei children`
**State:** pending
**Prior:** Task 1

Add `rhei children <plan> --task <id>` as a discoverable alias for listing
direct child tasks. Reuse the existing task flattening and parent-filtering
logic from `rhei list --parent`.

The default text output should match `rhei list --parent <id>` unless the spec
requires a more specific heading. The JSON output should preserve the same
shape as `rhei list --json` for compatibility.

### Task 6: Implement limited `rhei edit` and `rhei note`
**State:** pending
**Prior:** Task 1, Task 2

Add `edit` for title/body edits and `note` for append-only notes.

`edit` must support title replacement and whole-body replacement from `--body`
or `--body-file`. It must not change state, priors, assignee, result links, or
children. It should reject invocations that specify no edit, and it should
reject simultaneous inline and file/stdin body input.

`note` should append the dated blockquote note format specified in Task 1. It
must support `--message` and `--message-file`, reject simultaneous message
inputs, preserve children under the task, and must not modify task metadata.

### Task 7: Update docs, completions, and tests
**State:** pending
**Prior:** Task 3, Task 4, Task 5, Task 6

Update command help, tab completions, user docs, and tests for the new command
surface.

Coverage should include:

- Root task creation in a single-file plan.
- Child task creation in a single-file plan.
- Root task creation in a Directory Workspace.
- Dependency add, remove, duplicate avoidance, and cycle rejection.
- Cross-file cycle rejection in a Directory Workspace.
- Dependency tree text output.
- Children alias text and JSON output.
- Edit title/body behavior while preserving metadata and children.
- Runtime-owned `**Assignee:**` and `> **Result:**` preservation for every
  mutating command.
- `add --parent` at the `structure.maxLevels` boundary.
- Invalid state name rejection after alias normalization.
- Default state selection from `node_policy` profiles, including a custom state
  machine where different node kinds resolve to different profile initials.
- Missing parent, child ID that skips a segment, and missing prior rejection.
- `dep remove` of the last prior removes the `**Prior:**` line entirely.
- Note append behavior.
- Validation after every successful mutation.
- `--dry-run` prints a diff and leaves tracked plan files unchanged.

Run the repository verification commands from `AGENTS.md` before finishing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets
cargo test --workspace --all-targets --no-fail-fast
```
