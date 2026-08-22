    // §FS-rhei-supervision.1.2: the `supervise:` validation rules.

    fn supervise_machine(states: &str, transitions: &str) -> String {
        format!(
            r#"
name: supervise-test
version: 1.0
states:
{states}
transitions:
{transitions}
profiles:
  default:
    initial: supervise
    allowed: [supervise, review, human-review, completed, cancelled]
node_policy:
  root: default
  default: default
"#
        )
    }

    /// The canonical supervisor from §FS-rhei-supervision.7.
    fn canonical_states() -> &'static str {
        r#"  supervise:
    description: Supervise the subtree
    supervise: task
    agent: pi
    visits: 12
  review:
    description: Review
    agent: claude-code
  human-review:
    description: Human call
    gating: true
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true"#
    }

    fn canonical_transitions() -> &'static str {
        r#"  - { from: supervise, to: human-review, description: Budget exhausted, condition: visitCount >= visits }
  - { from: supervise, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervise, to: supervise, description: Released }
  - { from: review, to: completed, description: Reviewed }
  - { from: "*", to: cancelled, description: Dropped }"#
    }

    #[test]
    fn accepts_the_canonical_supervisor() {
        let yaml = supervise_machine(canonical_states(), canonical_transitions());
        let machine = StateMachine::from_yaml_str(&yaml).expect("canonical supervisor is valid");
        assert_eq!(
            machine.states.get("supervise").and_then(|def| def.supervise_kind()),
            Some(SuperviseKind::Task)
        );
        assert_eq!(machine.states.get("review").and_then(|def| def.supervise_kind()), None);
    }

    #[test]
    fn accepts_state_granularity() {
        let yaml = supervise_machine(
            &canonical_states().replace("supervise: task", "supervise: state"),
            canonical_transitions(),
        );
        let machine = StateMachine::from_yaml_str(&yaml).expect("state granularity is valid");
        assert_eq!(
            machine.states.get("supervise").and_then(|def| def.supervise_kind()),
            Some(SuperviseKind::State)
        );
    }

    #[test]
    fn rejects_an_unknown_supervise_value() {
        let yaml = supervise_machine(
            &canonical_states().replace("supervise: task", "supervise: subtree"),
            canonical_transitions(),
        );
        let err = StateMachine::from_yaml_str(&yaml).expect_err("bad supervise value");
        assert!(
            err.to_string().contains("expected 'task' or 'state'"),
            "message names the legal values; got: {err}"
        );
    }

    #[test]
    fn rejects_supervise_on_a_state_with_no_executor() {
        let yaml =
            supervise_machine(&canonical_states().replace("    agent: pi\n", ""), canonical_transitions());
        let err = StateMachine::from_yaml_str(&yaml).expect_err("no executor");
        assert!(err.to_string().contains("not agent-bearing"), "got: {err}");
    }

    #[test]
    fn rejects_supervise_on_a_final_state() {
        // A `target:` rather than an `agent:`, so the earlier final-plus-agent
        // rule cannot answer first and mask the supervise rule.
        let states = canonical_states()
            .replace("    agent: pi\n", "    target: pi:openai:gpt-5\n    final: true\n");
        let err = StateMachine::from_yaml_str(&supervise_machine(&states, canonical_transitions()))
            .expect_err("final supervisor");
        assert!(err.to_string().contains("is final and cannot declare 'supervise'"), "got: {err}");
    }

    #[test]
    fn rejects_supervise_on_a_gating_state() {
        let states = canonical_states().replace("    supervise: task\n", "    supervise: task\n    gating: true\n");
        let err = StateMachine::from_yaml_str(&supervise_machine(&states, canonical_transitions()))
            .expect_err("gating supervisor");
        assert!(err.to_string().contains("is gating and cannot declare 'supervise'"), "got: {err}");
    }

    #[test]
    fn rejects_supervise_on_a_program_state() {
        let states = canonical_states().replace("    agent: pi\n", "    program: \"./check.sh\"\n");
        let err = StateMachine::from_yaml_str(&supervise_machine(&states, canonical_transitions()))
            .expect_err("program supervisor");
        assert!(err.to_string().contains("declares both 'program' and 'supervise'"), "got: {err}");
    }

    #[test]
    fn rejects_supervise_on_a_poll_state() {
        // `poll` and `visits` are mutually exclusive, so the budget goes too.
        let states = canonical_states().replace(
            "    visits: 12\n",
            "    poll: { interval: 5m, max_attempts: 3 }\n",
        );
        let err = StateMachine::from_yaml_str(&supervise_machine(&states, canonical_transitions()))
            .expect_err("poll supervisor");
        assert!(err.to_string().contains("declares both 'poll' and 'supervise'"), "got: {err}");
    }

    #[test]
    fn rejects_supervise_combined_with_fanout() {
        let states =
            canonical_states().replace("    agent: pi\n", "    all_targets: [\"pi:openai:gpt-5\", \"codex:openai:gpt-5\"]\n");
        let err = StateMachine::from_yaml_str(&supervise_machine(&states, canonical_transitions()))
            .expect_err("fanout supervisor");
        assert!(err.to_string().contains("not a fanout"), "got: {err}");
    }

    #[test]
    fn rejects_a_supervising_state_without_a_self_loop() {
        let transitions = canonical_transitions()
            .replace("  - { from: supervise, to: supervise, description: Released }\n", "");
        let err = StateMachine::from_yaml_str(&supervise_machine(canonical_states(), &transitions))
            .expect_err("no release edge");
        assert!(err.to_string().contains("no self-loop transition"), "got: {err}");
    }

    fn supervision_warnings_for(yaml: &str) -> Vec<String> {
        let machine = StateMachine::from_yaml_str(yaml).expect("machine loads");
        let rhei =
            rhei_core::parse("# Rhei: T\n\n## Tasks\n\n### Task 1: Root\n**State:** supervise\n")
                .expect("plan parses");
        validate_with_machine(&rhei, &machine).warnings
    }

    #[test]
    fn warns_when_no_transition_finishes_the_supervisor() {
        let transitions = canonical_transitions().replace(
            "  - { from: supervise, to: completed, description: Subtree done, condition: openDescendants < 1 }\n",
            "",
        );
        let warnings = supervision_warnings_for(&supervise_machine(canonical_states(), &transitions));
        assert!(
            warnings.iter().any(|w| w.contains("no way to finish")),
            "expected the no-terminal-edge warning; got: {warnings:?}"
        );
    }

    #[test]
    fn warns_when_neither_visits_nor_an_exhaustion_edge_is_declared() {
        let states = canonical_states().replace("    visits: 12\n", "");
        let transitions = canonical_transitions().replace(
            "  - { from: supervise, to: human-review, description: Budget exhausted, condition: visitCount >= visits }\n",
            "",
        );
        let warnings = supervision_warnings_for(&supervise_machine(&states, &transitions));
        assert!(
            warnings.iter().any(|w| w.contains("no safety valve")),
            "expected the unbounded-supervisor warning; got: {warnings:?}"
        );
    }

    /// A self-loop with no budget and no counted exit is warned about.
    ///
    /// Visits of such a state are counted so an authored `visitCount` exit
    /// works; nothing counts down for a machine that authored neither.
    // §FS-rhei-supervision.4.2 §FS-rhei-states.1.3
    #[test]
    fn warns_when_a_self_loop_has_neither_a_budget_nor_a_counted_exit() {
        let states = canonical_states().replace("    visits: 12\n", "");
        let transitions = canonical_transitions().replace(
            "  - { from: supervise, to: human-review, description: Budget exhausted, condition: visitCount >= visits }\n",
            "",
        );
        let warnings = supervision_warnings_for(&supervise_machine(&states, &transitions));
        assert!(
            warnings.iter().any(|w| w.contains("nothing ends the loop")),
            "expected the unbounded-self-loop warning; got: {warnings:?}"
        );

        // The same machine with only the budget back is bounded again.
        let budgeted =
            supervision_warnings_for(&supervise_machine(canonical_states(), &transitions));
        assert!(
            budgeted.iter().all(|w| !w.contains("nothing ends the loop")),
            "a `visits:` budget ends the loop; got: {budgeted:?}"
        );
    }

    #[test]
    fn the_canonical_supervisor_warns_about_nothing() {
        let warnings =
            supervision_warnings_for(&supervise_machine(canonical_states(), canonical_transitions()));
        assert!(
            warnings.iter().all(|w| !w.contains("supervise")),
            "the canonical supervisor is warning-free; got: {warnings:?}"
        );
    }
