#[test]
fn callback_redirect_via_next_state_retargets_declared_transition() {
    // `on_leave` returns a `nextState` that targets a different declared
    // transition from the same `from`; the CLI should follow the redirect.
    let machine_yaml = r#"name: spec-redirect
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
  rejected:
    description: Rejected outright
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: 'cli:printf ''{"success": true, "nextState": "rejected"}'''
  - from: pending
    to: rejected
"#;
    let dir = unique_temp_dir("callback-spec-redirect");
    let plan = r#"# Rhei: Spec Redirect Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(
        result.status.success(),
        "redirect to a declared target should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    // Task should end up in the redirected target, not the originally-requested one.
    let updated = fs::read_to_string(&plan_path).expect("read updated plan");
    let rhei = parse(&updated).expect("parse updated plan");
    let task = rhei.tasks.iter().find(|t| t.id == TaskId::number(1)).expect("Task 1 exists");
    assert_eq!(task.state.as_str(), "rejected");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_redirect_to_undeclared_transition_is_rejected() {
    let machine_yaml = r#"name: spec-redirect-undeclared
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
  elsewhere:
    description: A state with no transition declared from `pending`.
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: 'cli:printf ''{"success": true, "nextState": "elsewhere"}'''
"#;
    let dir = unique_temp_dir("callback-spec-redirect-bad");
    let plan = r#"# Rhei: Spec Redirect Bad Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(
        !result.status.success(),
        "redirect to an undeclared transition should fail the transition"
    );
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("elsewhere") && normalized.contains("no transition"),
        "stderr should explain the undeclared redirect; got:\n{}",
        result.stderr
    );

    // File unchanged — redirect was rejected before any write.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn states_profile_allowed_rejects_callback_redirect_destination() {
    let machine_yaml = r#"name: profile-redirect-guard
version: 3
states:
  pending:
    description: Not started
  in-progress:
    description: Allowed target
    final: true
  rejected:
    description: Globally valid but outside the resolved profile
    final: true
profiles:
  simple:
    initial: pending
    allowed: [pending, in-progress]
node_policy:
  root: simple
  default: simple
transitions:
  - from: pending
    to: in-progress
    on_leave: 'cli:printf ''{"success": true, "nextState": "rejected"}'''
  - from: pending
    to: rejected
"#;
    let dir = unique_temp_dir("states-profile-redirect-transition");
    let plan = r#"# Rhei: Profile Redirect Guard

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(
        !result.status.success(),
        "callback redirect into profile-disallowed state should fail"
    );
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("not allowed") && normalized.contains("resolved") && normalized.contains("profile"),
        "stderr should explain profile allowed-state guard; got:\n{}",
        result.stderr
    );

    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_receives_transition_context_on_stdin() {
    // The callback reads its stdin and writes it back into a file we
    // then inspect. This verifies the TransitionContext JSON payload is
    // actually delivered on stdin.
    let dir = unique_temp_dir("callback-spec-stdin");
    let capture_path = dir.join("captured.json");
    let capture_display = capture_path.display().to_string();

    let machine_yaml = format!(
        r#"name: spec-stdin
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
    final: true
transitions:
  - from: pending
    to: in-progress
    on_leave: "cli:cat > '{capture}'"
"#,
        capture = capture_display
    );

    let plan = r#"# Rhei: Spec Stdin Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", &machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);
    assert!(
        result.status.success(),
        "transition should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );

    let captured =
        fs::read_to_string(&capture_path).expect("callback should have written stdin payload");
    let parsed: serde_json::Value =
        serde_json::from_str(&captured).expect("captured payload should be JSON");
    assert_eq!(parsed["task"]["id"], "1");
    assert_eq!(parsed["task"]["title"], "Alpha");
    assert_eq!(parsed["transition"]["from"], "pending");
    assert_eq!(parsed["transition"]["to"], "in-progress");
    assert_eq!(parsed["transition"]["triggeredBy"], "user");
    assert_eq!(parsed["environment"]["platform"], "cli");
    assert!(parsed["transition"]["timestamp"].is_string());

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn callback_on_enter_failure_rolls_back_state_write() {
    // The on_leave callback approves; the on_enter callback crashes.
    // Per the spec, the state write must roll back to its pre-transition
    // contents rather than persisting the mid-transition state.
    let machine_yaml = r#"name: spec-on-enter-rollback
version: 1
states:
  pending:
    description: Not started
    initial: true
  in-progress:
    description: Working
    final: true
transitions:
  - from: pending
    to: in-progress
    on_enter: "cli:exit 1"
"#;
    let dir = unique_temp_dir("callback-spec-on-enter-rollback");
    let plan = r#"# Rhei: On-Enter Rollback Test

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine_yaml);

    let result =
        run_transition_with_flags(&plan_path, &machine_path, "1", "pending", "in-progress", &[]);

    assert!(!result.status.success(), "on_enter failure should fail the transition");
    let normalized = normalize_for_assertions(&result.stderr);
    assert!(
        normalized.contains("on_enter") && normalized.contains("failed"),
        "stderr should identify the on_enter failure; got:\n{}",
        result.stderr
    );

    // Most importantly: the plan file must be rolled back to its original state.
    let contents = fs::read_to_string(&plan_path).expect("read plan");
    assert_eq!(contents, plan, "on_enter failure must roll back the state write");

    fs::remove_dir_all(dir).expect("cleanup");
}

// ---- Run command integration tests ----
