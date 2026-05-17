# Functional spec

Rhei user-visible behavior and requirements live here as grund declarations.
Each textual spec file keeps the repo's `.spec.md` suffix convention and declares
one `FS-<slug>` ID at its H1.

| ID | Subject |
|---|---|
| §FS-rhei-plan-language | Rhei plan language grammar and semantics |
| §FS-rhei-usage | Roles, coordination patterns, and agent workflows |
| §FS-rhei-authoring | Practical plan authoring guide |
| §FS-rhei-states | State machine format and default states |
| §FS-rhei-transitions | Transition system, callbacks, and YAML schema |
| §FS-rhei-callbacks | Transition callback examples |
| §FS-rhei-agents | Agent configuration, execution, and timeout behavior |
| §FS-rhei-programs | Deterministic program states |
| §FS-rhei-run | `rhei run` command behavior |
| §FS-rhei-run-tui | `rhei run` TUI and transition journal |
| §FS-rhei-next | `rhei next` command behavior |
| §FS-rhei-transition-cmd | `rhei transition` command behavior |
| §FS-rhei-complete | `rhei complete` command behavior |
| §FS-rhei-reset | `rhei reset` command behavior |
| §FS-rhei-list | `rhei list` command behavior |
| §FS-rhei-viz | `rhei viz` command behavior |
| §FS-rhei-templates | Rhei template format and instantiation behavior |
| §FS-rhei-completions | Shell completion UX |
| §FS-rhei-install-skills | `rhei install-skills` command behavior |
| §FS-rhei-state-machine-writer | State machine writer guidance |

This index is navigational. Normative citations should target the specific
declaration ID rather than this file.

Supporting product documents in this folder:

- [Project purpose](grund.md)
- [Goals](goals.md)
- [Roadmap](roadmap.md) §RM-rhei-roadmap
- [Comparison](comparison.md)
- [Rhei vs. beads](rhei-vs-beads.md)
- [Tab completions setup](tab-completions.md)
- [PM review notes](pm-review-2026-04-22.md)
