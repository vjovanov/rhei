# Proposal - codex[yolo]:openai:gpt-5.5

- Recommendation: Model the project inbox as a reserved synthetic level-1 rhei
  with id `inbox`; every inbox ticket is a normal task below that rhei, so its
  project id is `inbox.<local-task-id>`, its parent is `Rhei inbox`, and its
  hierarchy level is 2 or deeper.
- Reasons: This preserves the required `Panta -> rhei -> ticket` hierarchy while
  keeping quick-capture work available without choosing a domain rhei. It also
  gives inbox work the same identity, validation, dependency, execution-root,
  and artifact rules as ordinary rhei work instead of creating a special direct
  child case under Panta.
- Tradeoffs: The id `inbox` becomes reserved at the project rhei level, so a user
  cannot create a domain rhei with that id while the project inbox exists. The
  loader must synthesize a rhei node with no authored `index.rhei.md`, and UI
  code must label it as an inbox without treating its tickets differently. Moving
  a ticket out of the inbox becomes a reparenting/id-change operation from
  `inbox.<id>` to `<target-rhei>.<id>`.
- Assumptions: Panta is level 0, rheis are level 1, and tickets are level 2 or
  deeper. The optional `inbox/` directory contains task files authored in the
  normal workspace-task format. A project-level `**States:**` declaration may be
  inherited by rheis that omit their own declaration.
- Rejection criteria: Do not use this proposal if inbox tickets must keep stable
  project ids after being filed into a domain rhei, if users must be allowed to
  create a normal rhei named `inbox` alongside project inbox capture, or if the
  final state-policy decision rejects inheritance from `index.panta.md` for
  synthetic rheis.
