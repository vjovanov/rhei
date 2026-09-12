    // Which edge a program's exit selects, case by case, for every shape the
    // evaluation order has to tell apart.

    /// A program state declaring `edges`, plus the two gating states and the
    /// terminal one those edges land on.
    ///
    /// The cases below pin *selection*, which is what
    /// [`find_program_exit_transition`] returns today. Whether the exit was a
    /// declared route is the same question asked of the same rule
    /// (§FS-rhei-programs.3.2), and the e2e suite
    /// (`tests/e2e/declared_exit_route_tests.rs`) pins it where it is
    /// observable — in the ticket's result file. When the helper starts
    /// returning how the edge matched alongside the target, each case here
    /// gains the matching assertion on that value; none of them changes its
    /// expected target.
    fn exit_route_machine(edges: &str) -> rhei_validator::StateMachine {
        rhei_validator::StateMachine::from_yaml_str(&format!(
            r#"name: routing-exit
version: 1
states:
  route:
    description: Route on the exit code
    program:
      command: ["true"]
  checked:
    description: Routed here by the declared edge
    gating: true
  build-failed:
    description: The program died
    gating: true
  completed:
    description: Done
    final: true
transitions:
{edges}
  - from: checked
    to: completed
  - from: build-failed
    to: completed
"#
        ))
        .expect("valid routing machine")
    }

    /// One leaf ticket sitting in `route`, which is all the helper reads a task
    /// for: its id, its state, and its subtree.
    fn routing_task() -> rhei_core::ast::Task {
        let rhei = rhei_core::parse(
            "# Rhei: Routing exit\n\n## Tasks\n\n### Task 1: Route\n**State:** route\n",
        )
        .expect("parse routing plan");
        rhei.tasks.into_iter().next().expect("one task")
    }

    fn selected_edge(edges: &str, exit_code: i32) -> Option<String> {
        let machine = exit_route_machine(edges);
        find_program_exit_transition(&machine, None, &routing_task(), "route", exit_code)
            .expect("selection should not error")
    }

    const EXACT: &str = "  - from: route\n    to: checked\n    exit_code: 3\n";
    const ARRAY: &str = "  - from: route\n    to: checked\n    exit_code: [2, 3]\n";
    const CATCH_ALL: &str = "  - from: route\n    to: build-failed\n    exit_code: nonzero\n";
    const DISQUALIFIED: &str =
        "  - from: route\n    to: checked\n    exit_code: 3\n    condition: visitCount >= 2\n";

    #[test]
    fn an_exact_integer_exit_code_selects_its_own_edge() {
        assert_eq!(selected_edge(EXACT, 3).as_deref(), Some("checked"));
    }

    #[test]
    fn an_exact_integer_array_exit_code_selects_its_own_edge() {
        assert_eq!(selected_edge(ARRAY, 3).as_deref(), Some("checked"));
        assert_eq!(selected_edge(ARRAY, 2).as_deref(), Some("checked"));
        assert_eq!(selected_edge(ARRAY, 4), None);
    }

    #[test]
    fn a_catch_all_alone_selects_the_failure_edge() {
        assert_eq!(selected_edge(CATCH_ALL, 3).as_deref(), Some("build-failed"));
    }

    #[test]
    fn an_exact_edge_beats_a_catch_all_declared_beside_it() {
        let both = format!("{CATCH_ALL}{EXACT}");
        assert_eq!(selected_edge(&both, 3).as_deref(), Some("checked"));
        // A code the exact rule does not name still falls to the catch-all, so
        // the precedence is per exit code rather than per state.
        assert_eq!(selected_edge(&both, 4).as_deref(), Some("build-failed"));
    }

    /// The case a fix that re-derived the classification from the machine would
    /// get wrong: the exact rule matches the code, its `condition:` is false, so
    /// the catch-all is what fired. Asking the machine afterwards whether an
    /// exact rule from `route` to `build-failed` exists cannot reproduce that —
    /// the answer lives in the selection, not in the machine.
    #[test]
    fn an_exact_edge_its_condition_disqualified_leaves_the_catch_all_to_fire() {
        let both = format!("{DISQUALIFIED}{CATCH_ALL}");
        assert_eq!(selected_edge(&both, 3).as_deref(), Some("build-failed"));
    }
