# Discussion Response - claude-code[yolo]:anthropic:claude-opus-4-7

- Changed position: Yes — on D-001, D-002, and D-003 I now favor specifying the
  rule semantically and treating "one shared predicate", "direct state-machine
  resolution", and "synchronous lookup" as preferred implementations rather
  than as requirements. The semantic core of S-001 is unchanged.

- Remaining disagreements: None substantive. D-001, D-002, and D-003 collapse
  once the spec is written as a semantic contract with named, testable
  obligations; what remains is wording, not divergent intent.

- Concessions: codex[yolo]:openai:gpt-5.5 is right that mandating a single
  in-process predicate over-constrains cross-rhei boundaries, which may cross
  processes, repositories, or services where one callable function is not
  always reachable. They are also right that an exported normalized readiness
  result is acceptable provided it was computed against the prior rhei's own
  state machine, and that the spec should not bake in synchronous lookup as
  the only legal mechanism.

- Final recommendation for this point: Adopt S-001 as the normative rule and
  S-003 as the implementation latitude, with three explicit obligations that
  make the rule testable regardless of implementation:
  1. **Rule:** A cross-rhei dependency unblocks dependent work only when the
     prior task's resolved state has `final: true` and its normalized state
     name is not `cancelled`. This is the same rule as normal scheduling.
  2. **Resolution authority (resolves D-002):** The prior task's state must be
     interpreted against the prior rhei's own state machine — its
     `final` flags and its normalization to `cancelled`. The dependent side
     may either (a) read the prior rhei's state machine and apply the rule
     itself, or (b) consume a normalized readiness result that was computed
     against the prior rhei's state machine. Either path is conformant; both
     must yield the same answer for the same `(prior state, prior state
     machine)` input.
  3. **Implementation guidance (resolves D-001):** When local scheduling and
     cross-rhei readiness run in the same process and codebase, they SHOULD
     call one shared predicate (e.g. `is_successful_terminal(prior)`) so the
     rule has one definition, one test surface, and one citation site. When
     they cross a process/repo/service boundary, a shared predicate is not
     required, but the boundary MUST be covered by a contract test that
     exercises `final: true`, non-final, and normalized-`cancelled` priors
     and asserts behavioral equivalence with local scheduling.
  4. **Freshness (resolves D-003):** Synchronous resolution at check time is
     not required. Cached, exported, or asynchronously delivered readiness
     data is acceptable iff the spec or implementation states (a) what
     invalidates the cached value when the prior's state changes, and (b) a
     bound on staleness; absent both, the readiness check must resolve the
     prior's state at check time.
  5. **Out of scope (carried from A-005):** Additional unsuccessful terminal
     states beyond normalized `cancelled` are not addressed here; if a state
     machine introduces them, it must normalize them to `cancelled` or open
     a separate point to extend the rule.

- Reason: This wording resolves all three tagged disagreements without
  weakening the semantic guarantee both proposals agree on. D-001 is resolved
  by separating the normative rule (one semantic definition) from its
  implementation (shared predicate where co-resident, contract test
  otherwise), which captures codex's flexibility concern without losing the
  enforceability I argued for. D-002 is resolved by naming the prior rhei's
  state machine as the authority while letting either side perform the
  resolution, which matches codex's "raw state or equivalent normalized
  result" framing. D-003 is resolved by making freshness an explicit
  obligation rather than baking synchronous lookup into the rule, which keeps
  the door open for cached or exported readiness without permitting silent
  drift. The net effect: the rule is one sentence, the obligations are
  testable, and cross-rhei readiness cannot diverge from local scheduling
  whether they share code or a wire format.
