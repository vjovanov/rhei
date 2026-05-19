    #[test]
    fn reports_missing_named_dependency() {
        let input = r#"# Rhei: Example
## Tasks

### Task build: Build step
**State:** pending
**Prior:** Task deploy

### Task test: Test step
**State:** in-progress
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected missing named dependency error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task build depends on missing Task deploy"),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn rejects_child_prior_to_parent() {
        let input = r#"# Rhei: Example
## Tasks

### Task fetch-prs: Fetch pull requests
**State:** completed

#### Task fetch-prs.ci-failure-5227: Triage CI failure
**State:** pending
**Prior:** Task fetch-prs
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected parent-as-prior validation error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains(
                "Task fetch-prs.ci-failure-5227 cannot list ancestor Task fetch-prs as **Prior:**"
            ),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn rejects_descendant_prior_to_ancestor() {
        let input = r#"# Rhei: Example
---
structure:
  maxLevels: 3
---

## Tasks

### Task release: Release
**State:** pending

#### Task release.notes: Notes
**State:** pending

##### Task release.notes.diff: Diff notes
**State:** pending
**Prior:** Task release
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected ancestor-as-prior validation error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains(
                "Task release.notes.diff cannot list ancestor Task release as **Prior:**"
            ),
            "did not find expected message; got:\n{}",
            joined
        );
    }

    #[test]
    fn ok_when_all_dependencies_exist_named_and_numeric() {
        let input = r#"# Rhei: Example
## Tasks

### Task init: Initialize
**State:** pending

### Task 2: B
**State:** in-progress
**Prior:** Task init

### Task 1: A
**State:** completed
**Prior:** Task 2, Task init
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn missing_state_is_parse_error() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
"#;
        let err = parse(input).unwrap_err();
        assert!(
            err.message.contains("missing mandatory **State:**"),
            "expected parse error about missing state; got: {}",
            err.message
        );
    }

    #[test]
    fn reports_invalid_state_with_allowed_list() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** invalid_state
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected invalid state error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("invalid state"),
            "did not find 'invalid state' in errors:\n{}",
            joined
        );
        assert!(
            joined.contains("Allowed: ["),
            "did not include 'Allowed: [...]' list:\n{}",
            joined
        );
        for s in ["pending", "in-progress", "completed"] {
            assert!(joined.contains(s), "allowed list missing state '{}'; errors:\n{}", s, joined);
        }
    }

    #[test]
    fn accepts_valid_states_and_escaped_spaces() {
        // Custom states definition with a state containing a space
        let yaml = r#"
name: sm-escaped
version: 1
states:
  "in progress": { description: "with space" }
  done: { description: "done", final: true }
"#;
        let machine = StateMachine::from_yaml_str(yaml).expect("states load");
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** `in progress`
"#;
        let rhei = parse(input).expect("parse ok");
        let report = validate_with_machine(&rhei, &machine);

        assert!(
            !report.has_errors(),
            "unexpected errors validating escaped-space state: {:?}",
            report.errors
        );
    }

    #[test]
    fn ok_when_all_tasks_have_valid_state() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
**State:** in-progress

### Task 3: C
**State:** completed
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn detects_two_node_cycle() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 2

### Task 2: B
**State:** pending
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected cycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Circular dependency detected"),
            "expected circular dependency message; got:\n{}",
            joined
        );
        assert!(joined.contains("1"), "should mention task 1; got:\n{}", joined);
        assert!(joined.contains("2"), "should mention task 2; got:\n{}", joined);
    }

    #[test]
    fn detects_three_node_cycle() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 2

### Task 2: B
**State:** in-progress
**Prior:** Task 3

### Task 3: C
**State:** completed
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected cycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Circular dependency detected"),
            "expected circular dependency message; got:\n{}",
            joined
        );
        // At least two task ids should be mentioned; typically all three.
        assert!(joined.contains("1"), "should mention task 1; got:\n{}", joined);
        assert!(joined.contains("2"), "should mention task 2; got:\n{}", joined);
    }

    #[test]
    fn detects_self_cycle() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 1
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected self-cycle error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Circular dependency detected"),
            "expected circular dependency message; got:\n{}",
            joined
        );
        assert!(joined.contains("1"), "should mention task 1; got:\n{}", joined);
    }

    #[test]
    fn passes_on_dag() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending

### Task 2: B
**State:** in-progress
**Prior:** Task 1

### Task 3: C
**State:** completed
**Prior:** Task 2
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors in DAG case: {:?}", report.errors);
    }

    #[test]
    fn no_false_cycle_with_missing_dependency() {
        let input = r#"# Rhei: Ex
## Tasks

### Task 1: A
**State:** pending
**Prior:** Task 9

### Task 2: B
**State:** in-progress
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected missing dependency error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 1 depends on missing Task 9"),
            "did not find expected missing-dep message; got:\n{}",
            joined
        );
        assert!(
            !joined.contains("Circular dependency detected"),
            "should not report a cycle when only a dependency is missing; got:\n{}",
            joined
        );
    }

    // ---- Child/parent id-extension semantics ----
    //
    // The "subtask numbering" validator has been removed; the rule that a
    // child id must extend its parent's id by exactly one segment is now
    // enforced by the parser (see `crates/rhei-core/src/parser.rs`), which
    // rejects malformed child headings with a parse error before validation
    // runs. The old `mismatched_parent_number_errors`,
    // `named_task_subtasks_produce_error`, `mixed_tasks_ok_and_error`, and
    // `multiple_subtasks_some_bad` tests were deleted accordingly — their
    // inputs no longer parse, so there's nothing left for the validator to
    // check.

    #[test]
    fn valid_subtask_numbering_ok() {
        let input = r#"# Rhei: Example
## Tasks

### Task 3: C
**State:** pending

#### Task 3.1: First
**State:** pending
#### Task 3.2: Second
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(!report.has_errors(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn terminal_parent_with_non_terminal_subtask_errors() {
        let input = r#"# Rhei: Example
## Tasks

### Task 2: Parent
**State:** completed

#### Task 2.1: Still open
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = StateMachine::from_yaml_str(
            r#"
name: terminal-parent-test
version: 1.0
states:
  pending: { description: "not started" }
  completed: { description: "done", final: true }
"#,
        )
        .expect("states load");
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "expected terminal parent coherence error");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Task 2 is in terminal state 'completed'"),
            "expected terminal parent state in error; got:\n{}",
            joined
        );
        assert!(
            joined
                .contains("descendant Task 2.1 ('Still open') is in non-terminal state 'pending'"),
            "expected non-terminal descendant in error; got:\n{}",
            joined
        );
    }

    #[test]
    fn terminal_parent_with_terminal_subtasks_is_valid() {
        let input = r#"# Rhei: Example
## Tasks

### Task 2: Parent
**State:** completed

#### Task 2.1: Done
**State:** completed
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = StateMachine::from_yaml_str(
            r#"
name: terminal-parent-test
version: 1.0
states:
  pending: { description: "not started" }
  completed: { description: "done", final: true }
"#,
        )
        .expect("states load");
        let report = validate_with_machine(&rhei, &sm);

        assert!(
            !report.has_errors(),
            "terminal parent with terminal subtasks should validate: {:?}",
            report.errors
        );
    }

    #[test]
    fn duplicate_sibling_child_id_is_rejected() {
        // The new validator checks that sibling ids under a common parent are
        // unique, replacing the old ad-hoc "subtask uniqueness" rule.
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**State:** pending

#### Task 1.1: First
**State:** pending

#### Task 1.1: Duplicate
**State:** pending
"#;
        let rhei = parse(input).expect("parse ok");
        let sm = sample_machine();
        let report = validate_with_machine(&rhei, &sm);

        assert!(report.has_errors(), "duplicate sibling id should be rejected");
        let joined = report.errors.join("\n");
        assert!(
            joined.contains("Duplicate sibling task id: Task 1.1")
                && joined.contains("under Task 1"),
            "expected duplicate-sibling message; got:\n{}",
            joined
        );
    }

    #[test]
    fn prior_without_state_is_parse_error() {
        let input = r#"# Rhei: Example
## Tasks

### Task 1: A
**Prior:** Task 2

### Task 2: B
**State:** pending
"#;
        let err = parse(input).unwrap_err();
        assert!(
            err.message.contains("**State:** must appear before **Prior:**"),
            "expected parse error about ordering; got: {}",
            err.message
        );
    }
