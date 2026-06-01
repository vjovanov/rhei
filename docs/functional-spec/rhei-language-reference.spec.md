# FS-rhei-language-reference: Rhei Language Reference

Rhei must have one canonical language-reference entry point so humans and
agents can quickly answer "what files and syntax make up a valid Rhei
workflow?" without reconstructing the model from scattered feature specs. This
supports readable, reviewable plans and predictable execution. §GOAL-rhei-outcomes

This page is the entry point for the Rhei language surface. It does not replace
the narrower normative specs; it maps each language surface to the document
that owns it.

## 1. Language Surfaces

Rhei has four user-authored language surfaces. The first — plan and project
markdown — spans several file kinds; the file-kind map below names each one,
its role, and the spec that owns its grammar and behavior:

| File kind | Role | Owner |
|-----------|------|-------|
| `index.panta.md` | Panta project manifest: title, optional default `**States:**`, content; no authored nodes | §FS-rhei-panta.1, §FS-rhei-plan-language.1.5 |
| `rheis/` entry | One rhei per entry — a `*.rhei.md` or a Directory Workspace — discovered at project scope | §FS-rhei-panta.1, §FS-rhei-plan-language.1.5 |
| `*.rhei.md` | Single-File Plan: a rhei with its `## Tasks` inline | §FS-rhei-plan-language.1.1 |
| `index.rhei.md` + `tasks/**/*.md` | Directory Workspace rhei: manifest plus merged workspace task files | §FS-rhei-plan-language.1.2 |
| `basin/` task files (optional) | Unfiled tickets loaded as the reserved synthetic `basin` rhei | §FS-rhei-panta.2, §FS-rhei-plan-language.1.5 |

A bare rhei — a lone `*.rhei.md` or workspace with no enclosing
`index.panta.md` — loads as a one-rhei project. Load order, id namespacing,
execution roots, and state-machine binding for all of the above are specified in
§AR-rhei-panta. The `**States:**` declaration in these files resolves to
`states.yaml`, which belongs to the state-machine surface below — not to this
map.

The remaining three surfaces:

- State machines: `states.yaml`. Owned by §FS-rhei-states and
  §FS-rhei-transitions.
- Templates: template directories with `template.yaml` plus rendered plan and
  state files. Owned by §FS-rhei-templates.
- Execution references: agent, model, MCP server, skill, snapshot, and program
  references. Owned by §FS-rhei-agents, §FS-rhei-programs, and
  §FS-rhei-snapshots.

The plan and project markdown surface is the primary source of truth for project
membership, task state, dependencies, hierarchy, assignees, and result links.
State machines constrain which states and transitions are legal. Templates are a
preprocessing layer that materializes ordinary plan markdown and optional state
machines before runtime parsing.

## 2. Reading Path

Use this order when learning or auditing the language:

1. Read §FS-rhei-authoring for practical authoring patterns.
2. Read §FS-rhei-plan-language for the formal markdown grammar and semantic
   constraints, including Panta Project layout.
3. Read §FS-rhei-panta for project-root behavior and command scope.
4. Read §FS-rhei-states for the state-machine schema and default states.
5. Read §FS-rhei-transitions when a workflow depends on explicit transition
   rules, callbacks, visits, polling, or artifact enforcement.
6. Read §FS-rhei-templates when the authored source is a reusable template
   rather than a concrete plan workspace.

Command specs such as §FS-rhei-validate, §FS-rhei-next, §FS-rhei-transition-cmd,
§FS-rhei-complete, and §FS-rhei-run define command behavior over the language;
they are not the primary grammar reference.

## 3. Ownership Rules

Language changes must preserve a single discoverable entry point:

- New syntax in plan markdown belongs in §FS-rhei-plan-language and must be
  linked from this page.
- Adding, removing, or renaming a user-authored project or rhei file kind or
  directory must update the file-kind map in §1 in the same change as the
  owning spec edit.
- New state-machine fields belong in §FS-rhei-states or §FS-rhei-transitions
  and must be linked from this page when users author them directly.
- New template syntax or manifest fields belong in §FS-rhei-templates and must
  be linked from this page.
- New execution references that appear in authored files must identify their
  owner spec from this page.

If a feature changes what a valid Rhei workflow can contain, this reference must
be updated in the same change. The goal is not to centralize every rule here;
the goal is to make the authoritative rule easy to find.
