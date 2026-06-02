# Disagreement Map - Resolve Cross-rhei dependency readiness

## Candidate Solutions
- S-001: A cross-rhei dependency unblocks dependent work only when the prior task is in a successful terminal state: the prior's resolved state definition has `final: true`, and the normalized state name is not `cancelled`.
  - Proposed by: claude-code[yolo]:anthropic:claude-opus-4-7; codex[yolo]:openai:gpt-5.5
  - Reasons: This exactly matches normal scheduling readiness, preserves identical semantics across rhei boundaries, and prevents cancelled prerequisites from unblocking dependent work.
- S-002: Implement S-001 through a single normative predicate, such as `is_successful_terminal(prior)`, used by both local scheduling and cross-rhei readiness checks after resolving the prior state through the prior rhei's own state machine.
  - Proposed by: claude-code[yolo]:anthropic:claude-opus-4-7
  - Reasons: A shared predicate makes "same readiness rule" enforceable in code, centralizes future changes, and ensures cross-rhei checks honor the state definitions and normalization rules of the rhei that owns the prior.
- S-003: Implement S-001 by resolving the remote state definition and normalized state consistently with local scheduling, or by consuming an equivalent normalized readiness result.
  - Proposed by: codex[yolo]:openai:gpt-5.5
  - Reasons: This preserves the required semantics while allowing the integration boundary to expose either raw remote state-machine data or a precomputed readiness result.

## Agreements
- A-001: Cross-rhei dependencies must use the same readiness rule as normal scheduling.
- A-002: A prior can unblock dependent work only when its resolved state has `final: true`.
- A-003: A prior whose normalized state is `cancelled` must not unblock dependent work, even though it may be terminal for closure.
- A-004: Cross-rhei readiness requires reliable access to the prior task's current state and either the referenced rhei's state machine or an equivalent semantic result.
- A-005: Additional unsuccessful terminal states beyond normalized `cancelled` are out of scope for this point unless they normalize to `cancelled`.
- A-006: Cancelled prerequisites may leave dependents blocked until a human changes the dependency, reroutes the work, or supplies a different successful prerequisite.

## Disagreements
- D-001: Whether the final answer should require a single shared readiness predicate as the implementation mechanism, or specify only semantic equivalence with normal scheduling.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 requires one normative predicate; codex[yolo]:openai:gpt-5.5 allows any implementation that resolves the same remote state semantics or equivalent normalized readiness result.
  - Options: Require both local and cross-rhei scheduling to call the same predicate; or require cross-rhei checks to be behaviorally identical while permitting an equivalent readiness API/result.
  - Why it matters: A mandatory shared predicate is easier to test and audit, but may over-constrain implementations that cross process, repository, or service boundaries. Semantic equivalence is more flexible, but needs stronger contract tests to prove it cannot drift from local scheduling.
  - Evidence needed: Whether the current architecture can expose one shared predicate across local and cross-rhei scheduling paths, and whether cross-rhei checks run in the same codebase/process as normal scheduling or through a serialized/API boundary.
- D-002: Where the cross-rhei readiness checker must resolve state-machine meaning.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 says resolve the prior state with the prior rhei's own state machine before applying the predicate; codex[yolo]:openai:gpt-5.5 says the consumer may access the referenced rhei's state machine or an equivalent normalized readiness result.
  - Options: The dependent-side checker fetches and interprets the prior rhei's state machine directly; or the prior rhei exports a normalized readiness result that the dependent-side checker trusts.
  - Why it matters: Direct interpretation keeps the rule transparent but adds a cross-rhei lookup and couples the consumer to remote state-machine definitions. Exported readiness reduces coupling but requires a precise contract so `final: true` and normalized-not-`cancelled` remain the actual basis for unblocking.
  - Evidence needed: Existing cross-rhei dependency metadata, available APIs or file paths for remote state machines, and whether there is already a trusted normalized readiness output used by local scheduling.
- D-003: Whether readiness checking must be synchronous at check time.
  - Agents: claude-code[yolo]:anthropic:claude-opus-4-7 assumes the prior rhei state machine can be resolved synchronously, with caching possible; codex[yolo]:openai:gpt-5.5 only requires reliable resolution and does not specify synchronous lookup.
  - Options: Require synchronous resolution of the prior task state and state machine during readiness checks; or allow cached/eventual/serialized readiness data if it is reliable and preserves the same semantics.
  - Why it matters: Synchronous resolution gives fresh decisions but can make scheduling depend on remote availability. Cached or serialized results may be more robust operationally but need freshness and invalidation rules to avoid incorrect unblocking.
  - Evidence needed: How cross-rhei dependencies are evaluated today, whether readiness checks tolerate remote I/O, and what freshness guarantees exist for cached state or exported readiness artifacts.

## Discussion Prompt
The proposals agree that cross-rhei dependencies unblock only when the prior is in a successful terminal state: `final: true` and normalized state not `cancelled`. Please address only D-001 through D-003: should the final rule require a single shared predicate, must the dependent side directly resolve the prior rhei's state machine or may it consume an equivalent readiness result, and does the check need synchronous resolution or can cached/exported readiness be accepted with explicit guarantees?
