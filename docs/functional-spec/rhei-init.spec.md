# FS-rhei-init: `rhei init`

Set up a Panta project. By default `rhei init` creates a **`panta/`** folder
inside the host directory and makes *that* the project: the plans and their
generated output live in one place, out of the host repository's way, and the
folder is gitignored — planning state is treated as working material of the
repository, not part of its content. `--here` instead makes the host
directory itself the project (the adoption mode for a directory that already
holds plans meant to be versioned).

Init writes the one file a project requires — the `index.panta.md` manifest —
plus the low-cost conveniences that keep a project healthy from day one:
ignore rules, and an agent-discovery note so coding agents working in the
repository find the Rhei workflow on their own. Everything else about a
project (rheis, tickets, state machines) is added by creating files, per
§FS-rhei-panta.2.

Initialization is a convenience, not a gate: a project is defined by its
files, and a hand-written `index.panta.md` is exactly as valid as a generated
one (§AR-rhei-panta.1). This deliberately contrasts with database-backed
trackers, where `init` must provision storage before anything works.

## 1. Usage

```bash
rhei init [DIR] [--here] [--title <title>] [--no-agents] [--force]
```

`DIR` is the **host** directory (default: the current directory; created when
missing). The project directory is `DIR/panta/`, or `DIR` itself with
`--here`.

### 1.1. Options

| Flag                | Default            | Description                                             |
|---------------------|--------------------|---------------------------------------------------------|
| `--here`            | off                | Make `DIR` itself the project instead of `DIR/panta/`. The mode for adopting a directory of existing, versioned plans |
| `--title <title>`   | from the host      | Project title written to the manifest heading           |
| `--no-agents`       | off                | Skip the `AGENTS.md` agent-discovery note (§4)          |
| `--force`           | off                | Re-initialize an existing project: overwrite the manifest (re-deriving the title and re-adopting the machine); companion files update in place |

The default title is derived from the **host** directory's name in both
modes — `panta/` is a location, not an identity: `-` and `_` become spaces
and each word is capitalized (`my-project` → `My Project`).

## 2. Behavior

1. **Refuse an existing project** unless `--force`. When the project
   directory already contains `index.panta.md` — and, in default mode, also
   when the host itself is already a project — the command fails stating the
   directory is already a project, names `--force` as the re-init path, and
   changes nothing. With `--force` the manifest is rewritten from scratch — a
   hand-edited manifest is deliberately clobbered, which is why force is
   opt-in — and the companion files update in place: the `.gitignore`
   entries and the marked `AGENTS.md` block are idempotent, so a forced
   re-init never duplicates them.
2. **Refuse to shadow existing plans** in default mode. When the host holds
   bare `*.rhei.md` files or workspace rheis, a `panta/` project would not
   discover them; the error offers both fixes — `--here` to adopt them in
   place, or moving them into `panta/` first. `--here` and `--force` skip
   this check.
3. **Warn about an enclosing project.** When an ancestor directory contains
   `index.panta.md`, init proceeds but warns on stderr: nested projects are
   almost always a mistake, and the outer project will not discover the inner
   one (§AR-rhei-panta.1 discovery does not recurse into rhei roots).
4. **Write the manifest**: `# Panta: <title>`, and nothing else — with one
   adoption exception. When the rheis discovery would find all agree on a
   single declared state machine, init writes that machine as the project
   default (`**States:** <machine>`) and says so: a rhei declaring a machine
   different from the project default is a load error (§FS-rhei-panta.6), so
   a bare manifest would create a project that cannot load. Rheis declaring
   *different* machines cannot be adopted; init proceeds and the conflict
   surfaces through the discovery report (§5) with the ordinary error.
5. **Seed ignore rules** (§3).
6. **Write the agent-discovery note** unless `--no-agents` (§4).
7. **Report what became visible** (§5).

## 3. Ignore rules

In default mode the whole project folder is ignored at the host —

```gitignore
panta/
```

— because a `panta/` project is working state by design. The project folder
additionally gets its own `.gitignore` covering generated output, so a user
who later decides to version their plans (by deleting the host's `panta/`
entry) does not start committing runtime state:

```gitignore
runtime/
.rhei/cache/
```

With `--here` only the generated-output entries are seeded, into the host's
`.gitignore` — adopted plans are assumed to be content worth versioning.

In both modes entries are appended one at a time and only when missing, so a
hand-maintained ignore file is never rewritten or reordered.

## 4. Agent-discovery note (`AGENTS.md`)

Rhei projects are driven by coding agents, and an agent dropped into a
repository has no way to know it plans work with Rhei. Init creates
`DIR/AGENTS.md` at the **host** — or appends to an existing one — with a
short note between stable markers naming where the project lives:

```markdown
<!-- rhei:begin -->
## Rhei

The Rhei (Panta) project for this repository lives in `panta/`. Plans are
`*.rhei.md` files and workspace directories; ticket ids are
project-qualified (`<rhei>.<id>`). Work tickets with `rhei list`,
`rhei next`, and `rhei complete`; validate edits with `rhei validate`.
Orchestration (`rhei run`) is started by humans, never by agents.
<!-- rhei:end -->
```

With `--here` the first sentence reads "This directory is a Rhei (Panta)
project." instead. The note deliberately names only the worker surface —
`list`, `next`, `complete`, `validate` — and marks `rhei run` as
human-initiated: orchestration spawns agent fleets and spends money, so an
agent must never be instructed to start it. Rewriting the note first strips every trace of a previous
one — marker-delimited regions, orphaned markers, and a marker-less `## Rhei`
section still carrying the note body — so init is idempotent even after a
third-party merge mangled the markers, and removal is one block deletion.
Richer per-agent integration (skills for Claude Code, Cursor, …) stays with
`rhei install-skills` (§FS-rhei-install-skills); init's final output points
at it rather than duplicating it.

## 5. Discovery report

After writing, init loads the project through the ordinary discovery pass
(§AR-rhei-panta.1) and reports where it lives and what it contains:

```text
Initialized Panta project "My Project" at panta/ with no rheis yet. Add one
by dropping a `<id>.rhei.md` file or a workspace directory next to
index.panta.md.
```

An adoption (`--here`) over existing plans reports the discovered rheis
(`with 2 rheis: auth, billing`). When discovery fails — for example a plan
file whose stem is not a valid rhei id — init still succeeds (the manifest is
written) and surfaces the load error as a warning, so init doubles as a first
validation.

The omitted-plan-target resolution knows the convention: a `panta/` child
containing `index.panta.md` resolves as the project for commands run in the
host directory or anywhere under it (§FS-rhei-panta.6), so after init, bare
`rhei list` works from the whole repository.

## 6. What init does not do

- It does not create rheis or tickets. `rhei new` is the planned verb for
  that (roadmap) and composes: `rhei init && rhei new "Auth"`.
- It does not move existing plan files into `panta/` — adopting in place is
  `--here`; moving content is left to the user (and note that moving a
  *tracked* plan into the gitignored `panta/` untracks it).
- It does not scaffold from templates — that is `rhei instantiate`
  (§FS-rhei-templates).
- It does not touch git beyond `.gitignore`: no hooks, no commits.
- It does not write a state machine or a `**States:**` line unless adoption
  requires one (§2); the built-in `rhei` machine is the default
  (§FS-rhei-states).
