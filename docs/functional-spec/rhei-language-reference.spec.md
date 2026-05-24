# FS-rhei-language-reference: Rhei Language Reference

Rhei must have one canonical language-reference entry point so humans and
agents can quickly answer "what files and syntax make up a valid Rhei
workflow?" without reconstructing the model from scattered feature specs. This
supports readable, reviewable plans and predictable execution. §GOAL-rhei-outcomes

This page is the entry point for the Rhei language surface. It does not replace
the narrower normative specs; it maps each language surface to the document
that owns it.

## 1. Language Surfaces

Rhei has four user-authored language surfaces:

- Plan markdown: `*.rhei.md`, `index.rhei.md`, and workspace `tasks/*.md`.
  Owned by §FS-rhei-plan-language.
- State machines: `states.yaml`. Owned by §FS-rhei-states and
  §FS-rhei-transitions.
- Templates: template directories with `template.yaml` plus rendered plan and
  state files. Owned by §FS-rhei-templates.
- Execution references: agent, model, MCP server, skill, snapshot, and program
  references. Owned by §FS-rhei-agents, §FS-rhei-programs, and
  §FS-rhei-snapshots.

The plan markdown surface is the primary source of truth for task state,
dependencies, hierarchy, assignees, and result links. State machines constrain
which states and transitions are legal. Templates are a preprocessing layer that
materializes ordinary plan markdown and optional state machines before runtime
parsing.

## 2. Reading Path

Use this order when learning or auditing the language:

1. Read §FS-rhei-authoring for practical authoring patterns.
2. Read §FS-rhei-plan-language for the formal markdown grammar and semantic
   constraints.
3. Read §FS-rhei-states for the state-machine schema and default states.
4. Read §FS-rhei-transitions when a workflow depends on explicit transition
   rules, callbacks, visits, polling, or artifact enforcement.
5. Read §FS-rhei-templates when the authored source is a reusable template
   rather than a concrete plan workspace.

Command specs such as §FS-rhei-validate, §FS-rhei-next, §FS-rhei-transition-cmd,
§FS-rhei-complete, and §FS-rhei-run define command behavior over the language;
they are not the primary grammar reference.

## 3. Ownership Rules

Language changes must preserve a single discoverable entry point:

- New syntax in plan markdown belongs in §FS-rhei-plan-language and must be
  linked from this page.
- New state-machine fields belong in §FS-rhei-states or §FS-rhei-transitions
  and must be linked from this page when users author them directly.
- New template syntax or manifest fields belong in §FS-rhei-templates and must
  be linked from this page.
- New execution references that appear in authored files must identify their
  owner spec from this page.

If a feature changes what a valid Rhei workflow can contain, this reference must
be updated in the same change. The goal is not to centralize every rule here;
the goal is to make the authoritative rule easy to find.
