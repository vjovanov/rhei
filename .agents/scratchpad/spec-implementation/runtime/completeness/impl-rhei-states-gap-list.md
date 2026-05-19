# Aggregated Completeness Gaps: impl-rhei-states

Spec: `docs/functional-spec/rhei-states.spec.md`

Inputs:

- `runtime/completeness/impl-rhei-states-gaps-codex-xhigh-openai-gpt-5-5.md`

Aggregation rule: retained items marked `missing` or `partial` unless concrete file:line evidence showed the requirement was already covered. Only one reviewer inventory was present, so there are no cross-reviewer disagreements.

## Model And Target Resolution

- `partial`: legacy model profile provider/name values are not fully available to template and artifact resolution. Runtime model resolution derives provider/model names and logs/env include them, but artifact `{model.provider}` only reads inline targets and template `{model.provider}` reads `context.target.provider`; `{model.name}` returns the semantic model id rather than the resolved settings model name. Evidence: spec lines 29-36, 247-259, 368-390; `crates/rhei-cli/src/main.rs:7305`, `crates/rhei-cli/src/main.rs:7390`, `crates/rhei-cli/src/main.rs:7975`, `crates/rhei-cli/src/main.rs:8529`, `crates/rhei-cli/src/main.rs:4750`, `crates/rhei-cli/src/main.rs:4764`, `crates/rhei-cli/src/main.rs:4809`, `crates/rhei-cli/src/main.rs:4836`.

- `partial`: target selector modes may be rejected for agents with no declared modes, although the analogous `agent_mode` rule permits that case. Evidence: spec lines 120-123, 138; `crates/rhei-validator/src/lib.rs:444`, `crates/rhei-cli/src/main.rs:6710`, `crates/rhei-cli/src/main.rs:6726`, `crates/rhei-cli/src/main.rs:6691`, `crates/rhei-cli/src/main.rs:7358`.

- `partial`: `agent_mode` on an agent with no modes is treated as an error. The spec permits the mode when the resolved agent declares no modes. Evidence: spec line 138; `crates/rhei-validator/src/lib.rs:986`, `crates/rhei-cli/src/main.rs:6690`, `crates/rhei-cli/src/main.rs:6691`, `crates/rhei-cli/src/main.rs:7358`.

## Profiles And Initial States

- `partial`: `profiles` and `node_policy` are required only for schema v3+ machines; older schema versions may omit both despite the spec requiring them. Evidence: spec lines 55-61, 94-101; `crates/rhei-validator/src/lib.rs:779`, `crates/rhei-validator/src/lib.rs:782`, `crates/rhei-validator/src/lib.rs:792`, `crates/rhei-validator/src/lib.rs:799`, `crates/rhei-validator/src/lib.rs:1409`, `crates/rhei-validator/src/lib.rs:1410`.

- `partial`: per-state `initial: true` remains in the schema and is still used by `rhei next` claim/auto-transition paths instead of resolved profile initial states. Reset uses profile initial correctly, but `rhei next` still consults legacy state initial flags. Evidence: spec lines 63-66, 102-103, 727-739, 820-822; `crates/rhei-validator/src/lib.rs:502`, `crates/rhei-validator/src/lib.rs:1583`, `crates/rhei-cli/src/main.rs:12878`, `crates/rhei-cli/src/main.rs:13913`, `crates/rhei-cli/src/main.rs:14218`, `crates/rhei-cli/src/main.rs:14229`, `crates/rhei-cli/src/main.rs:14353`.

## Validation

- `partial`: explicit `all_targets: []` is not rejected. `all_targets` is stored as a defaulted `Vec<String>`, so an explicit empty list is indistinguishable from omission. Evidence: spec lines 68-90, 108-109; `crates/rhei-validator/src/lib.rs:537`, `crates/rhei-validator/src/lib.rs:934`.

- `partial`: top-level `models` entries are checked for uniqueness/non-empty strings but not validated against merged `settings.models` during `rhei validate`; missing model profiles fail later at runtime resolution. Evidence: spec line 126; `crates/rhei-validator/src/lib.rs:895`, `crates/rhei-cli/src/main.rs:6669`, `crates/rhei-cli/src/main.rs:7305`.

- `missing`: `state.agent` on a `gating: true` state should produce a validation warning, but no warning path is implemented. Evidence: spec line 137; `crates/rhei-validator/src/lib.rs:973`, `crates/rhei-validator/src/lib.rs:2184`.

- `partial`: artifact paths are statically checked before expansion, but runtime-expanded paths are simply joined to the workspace root without a canonical post-expansion escape check. Evidence: spec line 146; `crates/rhei-validator/src/lib.rs:2055`, `crates/rhei-validator/src/lib.rs:2060`, `crates/rhei-cli/src/main.rs:4784`.

- `partial`: state `mcp_servers` / `skills` registry ids are not reported by `rhei validate`; unresolved required ids are caught only at spawn time. Evidence: spec line 151; `crates/rhei-cli/src/main.rs:6643`, `crates/rhei-cli/src/main.rs:7080`, `crates/rhei-cli/src/main.rs:7098`, `crates/rhei-cli/src/main.rs:8171`.

- `partial`: nested template conditionals are not explicitly rejected even though v1 conditionals may not nest. Evidence: spec line 399; `crates/rhei-validator/src/lib.rs:1613`, `crates/rhei-cli/src/main.rs:4934`, `crates/rhei-cli/src/main.rs:4977`, `crates/rhei-cli/src/main.rs:5042`, `crates/rhei-cli/src/main.rs:5047`.

## Gating And Completion

- `partial` / `missing`: `rhei complete` does not block completion from gating states, including default `human-review`, even though only explicit human transitions may exit gating states. Evidence: spec lines 73, 897; `crates/rhei-validator/src/default-states.yaml:97`, `crates/rhei-cli/src/main.rs:14061`, `crates/rhei-cli/src/main.rs:14079`, `crates/rhei-cli/src/main.rs:14409`.

## Concurrency

- `partial`: `concurrent: true` scheduling is applied to agent tasks, but program tasks and callback tasks are processed sequentially in separate loops. This also affects concurrent poll-state behavior for program/callback states. Evidence: spec lines 74, 229; `crates/rhei-cli/src/main.rs:10862`, `crates/rhei-cli/src/main.rs:10969`, `crates/rhei-cli/src/main.rs:11183`, `crates/rhei-cli/src/main.rs:4358`.

## Polling

- `missing`: poll transition operands `pollAttempts` and `pollMaxAttempts` are not implemented or scoped to poll-state transitions. Evidence: spec lines 75, 203, 216-223; `crates/rhei-cli/src/main.rs:4014`, `crates/rhei-cli/src/main.rs:4369`.

- `missing`: visit and poll counters for `all_targets` / `all_models` fanout are keyed only by task id and state name, not by target/model, so per-target/model visit budgets and poll slots are not independent. Evidence: spec lines 75, 77-80, 159, 171-173, 228; `crates/rhei-cli/src/main.rs:3930`, `crates/rhei-cli/src/main.rs:4369`.

- `partial`: poll attempt metadata starts at no persisted count before the first self-loop, while `task_visit_count` defaults to `0`; the reviewer flagged this against the "attempts start at 1" contract. Evidence: spec line 203; `crates/rhei-validator/src/lib.rs:1330`, `crates/rhei-cli/src/main.rs:3930`, `crates/rhei-cli/src/main.rs:4369`.

- `partial`: exhausted poll states produce the specified clear error in the agent auto-advance path, but program/callback paths can warn or stall instead. Evidence: spec lines 75, 206-212, 227; `crates/rhei-cli/src/main.rs:4109`, `crates/rhei-cli/src/main.rs:11138`, `crates/rhei-cli/src/main.rs:13020`, `crates/rhei-cli/src/main.rs:13080`.

- `partial`: poll terminal-exit snapshot behavior was not fully proven for fanout/poll-specific cases. Static rejection of `poll` plus `snapshot.inherit` is covered, and run code calls snapshot emission after agent exit, but the reviewer left poll terminal-exit snapshot completeness to the snapshot implementation. Evidence: spec lines 161-163; `crates/rhei-validator/src/lib.rs:1335`, `crates/rhei-validator/src/lib.rs:1149`, `crates/rhei-cli/src/main.rs:11457`.

## Tooling

- `partial`: MCP availability is registry/support based, not an actual server start/handshake availability check at agent spawn time. Evidence: spec lines 654-655; `crates/rhei-cli/src/main.rs:6936`, `crates/rhei-cli/src/main.rs:11254`, `crates/rhei-cli/src/main.rs:11601`.

## CLI Surfaces

- `partial`: `rhei validate` does not fully apply settings-reference validation for top-level machine models, state tooling ids, and the gating-agent warning. Evidence: `crates/rhei-cli/src/main.rs:3723`, `crates/rhei-cli/src/main.rs:6643`, `crates/rhei-cli/src/main.rs:6669`.

- `partial`: `rhei states` omits newer state-machine fields such as `profiles`, `node_policy`, `gating`, `concurrent`, `poll`, `target`, `all_targets`, `agent`, `agent_mode`, `program`, `mcp_servers`, `skills`, and `snapshot`; JSON still emits legacy `initial`. The reviewer noted this as a surface completeness gap because the spec only explicitly mentions machine printing in the default YAML comment. Evidence: `crates/rhei-cli/src/main.rs:3515`, `crates/rhei-cli/src/main.rs:3611`, `crates/rhei-cli/src/main.rs:3621`.

- `partial`: `rhei next` resolves instructions/personality, but claimability and automatic initial-state behavior still use legacy per-state `initial`, not the node's resolved profile initial. Evidence: `crates/rhei-cli/src/main.rs:12878`, `crates/rhei-cli/src/main.rs:13913`, `crates/rhei-cli/src/main.rs:13996`, `crates/rhei-cli/src/main.rs:14008`.

- `partial`: `rhei run` still has incomplete program/callback concurrency, fanout-scoped visits/poll counters, poll condition aliases, and true MCP availability checks. Evidence: `crates/rhei-cli/src/main.rs:10574`, `crates/rhei-cli/src/main.rs:10627`.

- `partial`: `rhei complete` implements one-hop non-cancelled terminal selection, but does not block gating states before selecting the terminal transition. Evidence: `crates/rhei-cli/src/main.rs:14061`, `crates/rhei-cli/src/main.rs:14409`.

## Triage Notes

- The reviewer marked spec lines 768-787 `partial`, but the cited evidence covers the actionable whole-list semantics for profile `allowed` lists and identifies the rest as examples/guidance. No separate fix item is retained. Evidence: `crates/rhei-validator/src/lib.rs:719`, `crates/rhei-validator/src/lib.rs:849`.
