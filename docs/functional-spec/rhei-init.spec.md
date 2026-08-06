# FS-rhei-init: `rhei init`

Make a directory a Panta project. `rhei init` writes the one file a project
requires — the `index.panta.md` manifest — plus the two low-cost conveniences
that keep a project healthy from day one: ignore rules for generated output,
and an agent-discovery note so coding agents working in the directory find the
Rhei workflow on their own. Everything else about a project (rheis, tickets,
state machines) is added by creating files, per §FS-rhei-panta.2.

Initialization is a convenience, not a gate: a project is defined by its
files, and a hand-written `index.panta.md` is exactly as valid as a generated
one (§AR-rhei-panta.1). This deliberately contrasts with database-backed
trackers, where `init` must provision storage before anything works.

## 1. Usage

```bash
rhei init [DIR] [--title <title>] [--no-agents]
```

`DIR` defaults to the current directory and is created when missing.

### 1.1. Options

| Flag                | Default            | Description                                             |
|---------------------|--------------------|---------------------------------------------------------|
| `--title <title>`   | from the directory | Project title written to the manifest heading           |
| `--no-agents`       | off                | Skip the `AGENTS.md` agent-discovery note (§4)          |

The default title is derived from the directory name: `-` and `_` become
spaces and each word is capitalized (`my-project` → `My Project`).

## 2. Behavior

1. **Refuse an existing project.** When `DIR` already contains
   `index.panta.md`, the command fails stating the directory is already a
   project and changes nothing.
2. **Warn about an enclosing project.** When an ancestor directory contains
   `index.panta.md`, init proceeds but warns on stderr: nested projects are
   almost always a mistake, and the outer project will not discover the inner
   one (§AR-rhei-panta.1 discovery does not recurse into rhei roots).
3. **Write the manifest**: `# Panta: <title>` and nothing else. A minimal
   manifest keeps the file honest — everything a project does comes from
   discovery, not from init-time configuration.
4. **Seed ignore rules** (§3).
5. **Write the agent-discovery note** unless `--no-agents` (§4).
6. **Report what became visible** (§5).

## 3. Ignore rules

Generated output must not be committed: `runtime/` trees are per-run state
and `.rhei/cache/` holds snapshot caches keyed by ticket id. Init appends the
two entries to `DIR/.gitignore` — creating the file when absent, and adding
only the entries that are missing, so a hand-maintained ignore file is never
rewritten or reordered:

```gitignore
runtime/
.rhei/cache/
```

## 4. Agent-discovery note (`AGENTS.md`)

Rhei projects are driven by coding agents, and an agent dropped into a
directory has no way to know the markdown around it is an executable plan.
Init creates `DIR/AGENTS.md` — or appends to an existing one — with a short
note between stable markers so a re-run can update it in place and a removal
is one block deletion:

```markdown
<!-- rhei:begin -->
## Rhei

This directory is a Rhei (Panta) project. Plans are `*.rhei.md` files and
workspace directories; ticket ids are project-qualified (`<rhei>.<id>`).
Drive work with `rhei list`, `rhei next`, `rhei complete`, and `rhei run`;
validate edits with `rhei validate`. Run `rhei --help` for the full surface.
<!-- rhei:end -->
```

When the markers already exist in `AGENTS.md`, the block between them is
replaced rather than appended again, so init is idempotent. Richer per-agent
integration (skills for Claude Code, Cursor, …) stays with
`rhei install-skills` (§FS-rhei-install-skills); init's final output points
at it rather than duplicating it.

## 5. Discovery report

After writing, init loads the project through the ordinary discovery pass
(§AR-rhei-panta.1) and reports what the project now contains:

```text
Initialized Panta project "My Project" with 2 rheis: auth, billing
```

A directory with no rheis yet reports `with no rheis yet` and says how to add
one (drop a `<id>.rhei.md` file or a workspace directory next to the
manifest). When discovery fails — for example a plan file whose stem is not a
valid rhei id — init still succeeds (the manifest is written) and surfaces
the load error as a warning, so the first `rhei init` in a directory of
existing plans doubles as a first validation.

This makes adoption the primary flow, not greenfield: pointed at a directory
that already holds several bare rheis — the ambiguous case an omitted plan
target refuses to guess about (§FS-rhei-panta.6) — `rhei init` is the
one-command fix, and that ambiguity error names it.

## 6. What init does not do

- It does not create rheis or tickets. `rhei new` is the planned verb for
  that (roadmap) and composes: `rhei init && rhei new "Auth"`.
- It does not scaffold from templates — that is `rhei instantiate`
  (§FS-rhei-templates).
- It does not touch git beyond `.gitignore`: no hooks, no commits. Markdown
  in the working tree is already git-native.
- It does not write a state machine or a `**States:**` line; the built-in
  `rhei` machine is the default (§FS-rhei-states), and machine declarations
  are authored where they apply.
