const WORKSPACE_STATE_MACHINE: &str = r#"name: workspace-test-machine
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
  in-progress:
    description: Task currently being worked on
  completed:
    description: Task finished successfully
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

const PANTA_PROFILE_STATE_MACHINE: &str = r#"name: panta-profile-machine
version: 3.0
states:
  pending:
    description: Task not yet started
  completed:
    description: Task finished
    final: true
transitions:
  - from: pending
    to: completed
profiles:
  top-ticket:
    initial: pending
    allowed: [pending, completed]
  nested-ticket:
    initial: completed
    allowed: [completed]
node_policy:
  root: top-ticket
  default: nested-ticket
  overrides:
    - match:
        level: 1
      profile: top-ticket
"#;

const PANTA_LEVEL_TWO_OVERRIDE_MACHINE: &str = r#"name: panta-level-two-machine
version: 3.0
states:
  pending:
    description: Task not yet started
  completed:
    description: Task finished
    final: true
transitions:
  - from: pending
    to: completed
profiles:
  default-ticket:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default-ticket
  default: default-ticket
  overrides:
    - match:
        level: 2
      profile: default-ticket
"#;

const PANTA_INPUT_STATE_MACHINE: &str = r#"name: panta-input-machine
version: 1
states:
  pending:
    description: Needs an input from the owning rhei runtime
    initial: true
    inputs:
      - name: brief
        path: runtime/{task_id}.md
  in-progress:
    description: Task currently being worked on
  completed:
    description: Task finished successfully
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

/// Helper: create a directory workspace with the given index content and
/// a set of task files. Returns the workspace root directory.
fn create_workspace(
    prefix: &str,
    index: &str,
    task_files: &[(&str, &str)],
    state_machine: &str,
) -> (PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    for (name, content) in task_files {
        let path = tasks_dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create task parent dir");
        }
        fs::write(path, content).expect("write task file");
    }
    let machine_path = write_fixture_file(&dir, "states.yaml", state_machine);
    (ws, machine_path)
}

fn create_panta_project(
    prefix: &str,
    manifest: &str,
    files: &[(&str, &str)],
    state_machine: &str,
) -> PathBuf {
    let dir = unique_temp_dir(prefix);
    fs::write(dir.join("index.panta.md"), manifest).expect("write panta manifest");
    for (name, content) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create panta parent dir");
        }
        fs::write(path, content).expect("write panta file");
    }
    fs::write(dir.join("states.yaml"), state_machine).expect("write panta states");
    dir
}

#[test]
fn panta_project_loads_qualifies_and_validates_cross_rhei_priors() {
    let project = create_panta_project(
        "panta-valid",
        "# Panta: Product Suite\n**States:** workspace-test-machine\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** completed\n",
            ),
            (
                "billing/index.rhei.md",
                "# Rhei: Billing\n\n## Notes\nBilling context.\n",
            ),
            (
                "billing/tasks/invoice.md",
                "### Task 1: Invoice\n**State:** pending\n**Prior:** Task auth.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei.title, "Product Suite");
    assert_eq!(loaded.rhei_ids, vec!["auth", "billing"]);
    assert!(loaded.task_sources.contains_key("auth.1"));
    assert!(loaded.task_sources.contains_key("billing.1"));
    assert_eq!(loaded.rhei.tasks[0].id.to_string(), "auth.1");
    assert_eq!(loaded.rhei.tasks[1].id.to_string(), "billing.1");
    assert_eq!(loaded.rhei.tasks[1].prior[0].to_string(), "auth.1");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "validate should succeed for panta project\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validation succeeded"));

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .arg(project.join("index.panta.md"))
        .output()
        .expect("list command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "list should succeed for panta manifest path\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Task auth.1: Login [completed]"));
    assert!(stdout.contains("Task billing.1: Invoice [pending] (prior: auth.1)"));

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_discovery_skips_runtime_artifact_trees() {
    let project = create_panta_project(
        "panta-skip-runtime",
        "# Panta: Runtime Artifacts\n**States:** workspace-test-machine\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
            ),
            (
                "runtime/generated.rhei.md",
                "# Rhei: Generated Artifact\n\n## Tasks\n\n### Task 1: Artifact\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // Runtime artifact trees in the project directory are not rhei discovery inputs. §AR-rhei-panta.1
    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei_ids, vec!["auth"]);
    assert!(loaded.task_sources.contains_key("auth.1"));
    assert!(!loaded.task_sources.contains_key("generated.1"));

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_preserves_ambiguous_local_priors_before_cross_rhei_resolution() {
    let project = create_panta_project(
        "panta-local-prior",
        "# Panta: Ambiguous Local Prior\n**States:** workspace-test-machine\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n### Task auth: Auth root\n**State:** completed\n\n#### Task auth.1: Local setup\n**State:** completed\n\n### Task 2: Depends locally\n**State:** pending\n**Prior:** Task auth.1\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei.tasks[1].id.to_string(), "auth.2");
    assert_eq!(loaded.rhei.tasks[1].prior[0].to_string(), "auth.auth.1");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "validate should resolve ambiguous local prior\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_next_peek_resolves_inputs_from_owning_rhei_root() {
    let project = create_panta_project(
        "panta-peek-input-root",
        "# Panta: Peek Inputs\n**States:** panta-input-machine\n",
        &[
            ("auth/index.rhei.md", "# Rhei: Auth\n\n"),
            (
                "auth/tasks/login.md",
                "### Task 1: Login\n**State:** pending\n",
            ),
        ],
        PANTA_INPUT_STATE_MACHINE,
    );
    let runtime_dir = project.join("auth/runtime");
    fs::create_dir_all(&runtime_dir).expect("create owning rhei runtime");
    fs::write(runtime_dir.join("auth.1.md"), "ready").expect("write input artifact");

    // Panta readiness checks required inputs at the owning rhei root, not the project root. §AR-rhei-panta.5
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("next")
        .arg(&project)
        .arg("--peek")
        .arg("--no-callbacks")
        .output()
        .expect("next --peek command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "next --peek should resolve inputs from the child rhei root\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("auth.1"), "peek should report the claimable ticket: {stdout}");

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_validates_task_links_from_owning_rhei_root() {
    let project = create_panta_project(
        "panta-link-root",
        "# Panta: Link Root\n**States:** workspace-test-machine\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Read spec\n**State:** pending\n\nSee [spec](docs/spec.md).\n",
            ),
            ("docs/spec.md", "Auth spec\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "validate should resolve task links relative to rhei root\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_validates_child_rhei_content_links() {
    let project = create_panta_project(
        "panta-child-content-link",
        "# Panta: Child Content Links\n**States:** workspace-test-machine\n",
        &[
            (
                "auth/index.rhei.md",
                "# Rhei: Auth\n\n## Overview\nSee [missing](docs/missing.md).\n",
            ),
            (
                "auth/tasks/login.md",
                "### Task 1: Login\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Project validation checks child rhei content links against the child root. §AR-rhei-panta.5
    assert!(!output.status.success(), "validate should reject broken child rhei content link");
    assert!(
        stderr.contains("section 'Rhei auth / Overview'")
            && stderr.contains("docs/missing.md"),
        "unexpected stderr: {stderr}"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_explicit_max_levels_one_is_not_raised_to_default() {
    let project = create_panta_project(
        "panta-max-levels",
        "# Panta: Max Levels\n**States:** panta-level-two-machine\n\n---\nstructure:\n  maxLevels: 1\n  nodeKinds: [task]\n---\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n---\nstructure:\n  maxLevels: 1\n  nodeKinds: [task]\n---\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
        )],
        PANTA_LEVEL_TWO_OVERRIDE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "validate should reject level 2 policy override");
    assert!(
        stderr.contains("match.level is 2") && stderr.contains("levels must be in 1..=1"),
        "unexpected stderr: {stderr}"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_basin_loads_as_reserved_last_rhei() {
    let project = create_panta_project(
        "panta-basin",
        "# Panta: Captures\n**States:** workspace-test-machine\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
            ),
            ("basin/loose.md", "### Task 3: Triage later\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei_ids, vec!["auth", "basin"]);
    assert_eq!(loaded.rhei.tasks[0].id.to_string(), "auth.1");
    assert_eq!(loaded.rhei.tasks[1].id.to_string(), "basin.3");
    assert!(loaded.task_sources["basin.3"].ends_with("basin/loose.md"));

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_basin_ignores_runtime_markdown_artifacts() {
    let project = create_panta_project(
        "panta-basin-runtime",
        "# Panta: Captures\n**States:** workspace-test-machine\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
            ),
            ("basin/loose.md", "### Task 3: Triage later\n**State:** pending\n"),
            ("basin/runtime/result.md", "# Runtime Result\n\nNot a task file.\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei_ids, vec!["auth", "basin"]);
    assert!(loaded.task_sources.contains_key("basin.3"));
    // Basin runtime artifacts are ignored rather than parsed as basin tasks. §FS-rhei-panta.2
    assert!(!loaded.task_sources.values().any(|path| path.ends_with("basin/runtime/result.md")));

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "validate should ignore basin runtime markdown\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_rejects_domain_rhei_named_basin() {
    let project = create_panta_project(
        "panta-basin-reserved",
        "# Panta: Captures\n",
        &[(
            "basin.rhei.md",
            "# Rhei: Basin Domain\n\n## Tasks\n\n### Task 1: Invalid\n**State:** pending\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );

    let err = workspace::load_panta_project(&project).expect_err("reserved basin should fail");
    assert!(
        err.message.contains("reserved for the synthetic basin rhei"),
        "unexpected error: {}",
        err.message
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_rejects_child_rhei_state_machine_declaration_that_differs_from_project() {
    let project = create_panta_project(
        "panta-child-states",
        "# Panta: Mixed Machines\n**States:** workspace-test-machine\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n**States:** child-flow\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );

    let err = workspace::load_panta_project(&project).expect_err("mixed machines should fail");
    assert!(
        err.message.contains("declares state machine 'child-flow'")
            && err.message.contains("project-wide state machine 'workspace-test-machine'"),
        "unexpected error: {}",
        err.message
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_profile_resolution_uses_rhei_local_task_depth() {
    let project = create_panta_project(
        "panta-profile-depth",
        "# Panta: Profile Depth\n**States:** panta-profile-machine\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
        )],
        PANTA_PROFILE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "top-level ticket should resolve as level 1 despite project-qualified id\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_mutating_commands_are_rejected_until_project_rewrites_are_supported() {
    let project = create_panta_project(
        "panta-read-only",
        "# Panta: Read Only\n**States:** workspace-test-machine\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("transition")
        .arg(&project)
        .arg("--task")
        .arg("auth.1")
        .arg("--from")
        .arg("pending")
        .arg("--to")
        .arg("in-progress")
        .arg("--no-callbacks")
        .output()
        .expect("transition command should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "transition should fail for Panta projects");
    assert!(
        stderr.contains("Panta projects are currently read-only for `rhei transition`"),
        "unexpected stderr: {stderr}"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_next_peek_is_read_only_and_claim_is_rejected() {
    let project = create_panta_project(
        "panta-next-peek",
        "# Panta: Peek\n**States:** workspace-test-machine\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );

    // `--peek` does not mutate child rhei files, so it works project-wide. §FS-rhei-panta.6.1
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("next")
        .arg(&project)
        .arg("--peek")
        .arg("--no-callbacks")
        .output()
        .expect("next --peek command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "next --peek should succeed for Panta projects\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("auth.1"), "peek should report the claimable ticket: {stdout}");

    // Claim mode would write `**Assignee:**` into a child rhei file, so it is rejected.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("next")
        .arg(&project)
        .arg("--no-callbacks")
        .output()
        .expect("next claim command should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "next claim should fail for Panta projects");
    assert!(
        stderr.contains("Panta projects are currently read-only for `rhei next`"),
        "unexpected stderr: {stderr}"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn workspace_loads_and_validates_correctly() {
    let (ws, machine_path) = create_workspace(
        "ws-valid",
        "# Rhei: Workspace Test\n\n## Context\nSome context here.\n",
        &[
            ("alpha.md", "### Task 1: Alpha\n**State:** pending\n\nAlpha description.\n"),
            (
                "beta.md",
                "### Task 2: Beta\n**State:** completed\n**Prior:** Task 1\n\nBeta description.\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // is_workspace detection
    assert!(workspace::is_workspace(&ws));

    // load_workspace produces merged plan
    let loaded = workspace::load_workspace(&ws).expect("load workspace");
    assert_eq!(loaded.rhei.title, "Workspace Test");
    assert_eq!(loaded.rhei.tasks.len(), 2);
    assert_eq!(loaded.task_sources.len(), 2);
    assert!(loaded.task_sources.contains_key("1"));
    assert!(loaded.task_sources.contains_key("2"));

    // CLI validate succeeds
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&ws)
        .output()
        .expect("validate command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "validate should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validation succeeded"));

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_validate_accumulates_parse_errors_across_task_files() {
    let (ws, machine_path) = create_workspace(
        "ws-parse-errors",
        "# Rhei: Workspace Parse Errors\n",
        &[
            (
                "a.md",
                "### Task 1: Missing state\n\n### Task 2: Valid fallback\n**State:** pending\n",
            ),
            (
                "b.md",
                "### Task 3: Bad state field\n**State** pending\n\n### Task 4: Valid fallback\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&ws)
        .output()
        .expect("validate command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "validate should fail\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        stderr
    );
    assert!(stderr.contains("PARSE ERROR"), "expected parse header, got:\n{stderr}");
    assert!(stderr.contains("2 problems"), "expected problem count, got:\n{stderr}");
    assert!(stderr.contains("a.md"), "expected first task file, got:\n{stderr}");
    assert!(stderr.contains("b.md"), "expected second task file, got:\n{stderr}");
    assert!(stderr.contains("line 1"), "expected first line hint, got:\n{stderr}");
    assert!(stderr.contains("line 2"), "expected second line hint, got:\n{stderr}");
    assert!(
        stderr.contains("missing mandatory **State:**")
            && stderr.contains("Malformed metadata field"),
        "expected both parse errors, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("VALIDATION ERROR"),
        "parse failures should not fall through to validation output, got:\n{stderr}"
    );

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn validate_and_list_accept_workspace_index_file_path() {
    let (ws, machine_path) = create_workspace(
        "ws-index-path",
        "# Rhei: Workspace Index Path\n\n## Context\nIndex addressed directly.\n",
        &[("alpha.md", "### Task 1: Alpha\n**State:** pending\n\nDescription.\n")],
        WORKSPACE_STATE_MACHINE,
    );

    let index_path = ws.join("index.rhei.md");

    // workspace_dir resolves both directory and index file paths.
    assert!(workspace::is_workspace(&ws));
    assert!(workspace::workspace_dir(&ws).is_some());
    assert!(workspace::workspace_dir(&index_path).is_some());

    // CLI validate succeeds against the index file path.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("validate")
        .arg(&index_path)
        .output()
        .expect("validate command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "validate should succeed for index.rhei.md\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validation succeeded"));

    // CLI list also succeeds.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("list")
        .arg(&index_path)
        .output()
        .expect("list command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "list should succeed for index.rhei.md\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Task 1: Alpha"));

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_discovers_task_files_recursively_and_skips_hidden_paths() {
    let (ws, _machine_path) = create_workspace(
        "ws-recursive",
        "# Rhei: Workspace Recursive\n\n## Context\nSome context here.\n",
        &[
            ("alpha.md", "### Task 1: Alpha\n**State:** pending\n"),
            ("group/beta.md", "### Task 2: Beta\n**State:** pending\n"),
            (".ignored.md", "### Task bad: Hidden\n**State:** not-a-state\n"),
            ("group/.ignored/gamma.md", "### Task bad2: Hidden dir\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_workspace(&ws).expect("load workspace");
    assert_eq!(loaded.rhei.tasks.len(), 2);
    assert_eq!(loaded.rhei.tasks[0].id.to_string(), "1");
    assert_eq!(loaded.rhei.tasks[1].id.to_string(), "2");
    assert!(loaded.task_sources["2"].ends_with("group/beta.md"));
    assert!(!loaded.task_sources.contains_key("bad"));
    assert!(!loaded.task_sources.contains_key("bad2"));

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn validate_auto_discovers_workspace_root_state_machine_from_states_declaration() {
    let dir = unique_temp_dir("ws-auto-states");
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(
        ws.join("index.rhei.md"),
        "# Rhei: Workspace Auto States\n**States:** workspace-test-machine\n",
    )
    .expect("write index");
    fs::write(tasks_dir.join("alpha.md"), "### Task 1: Alpha\n**State:** pending\n")
        .expect("write task file");
    write_fixture_file(&ws, "states.yaml", WORKSPACE_STATE_MACHINE);

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&ws)
        .output()
        .expect("validate command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "validate should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validation succeeded"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn validate_reports_mismatched_auto_discovered_state_machine_name() {
    let dir = unique_temp_dir("auto-states-mismatch");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Auto States Mismatch\n**States:** custom-review\n\n## Tasks\n\n### Task 1: Review docs\n**State:** draft\n",
    );
    write_fixture_file(
        &dir,
        "states.yaml",
        "name: wrong-machine\nversion: 1\nstates:\n  draft:\n    initial: true\n    description: Start\n  completed:\n    final: true\n    description: Done\ntransitions:\n  - from: draft\n    to: completed\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "validate should fail when auto-discovered machine name mismatches\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        stderr
    );
    assert!(
        stderr.contains("plan declares state machine 'custom-review'"),
        "expected mismatch diagnostic, got:\n{}",
        stderr
    );
    assert!(
        stderr.contains("declares 'wrong-machine'"),
        "expected discovered machine name in diagnostic, got:\n{}",
        stderr
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn workspace_render_json_includes_all_tasks() {
    let (ws, machine_path) = create_workspace(
        "ws-render",
        "# Rhei: Render Test\n",
        &[
            ("a.md", "### Task 1: First\n**State:** pending\n"),
            ("b.md", "### Task 2: Second\n**State:** completed\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("render")
        .arg(&ws)
        .arg("--format")
        .arg("json")
        .arg("--pretty")
        .output()
        .expect("render command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "render should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("\"title\": \"Render Test\""));
    assert!(stdout.contains("\"First\""));
    assert!(stdout.contains("\"Second\""));

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_duplicate_task_id_across_files_is_reported() {
    let (ws, _machine_path) = create_workspace(
        "ws-dup",
        "# Rhei: Dup Test\n",
        &[
            ("a.md", "### Task 1: First\n**State:** pending\n"),
            ("b.md", "### Task 1: Duplicate\n**State:** pending\n"),
        ],
        fixtures::TEST_STATE_MACHINE,
    );

    let err = workspace::load_workspace(&ws).expect_err("should fail on duplicate");
    assert!(
        err.message.contains("duplicate task ID '1'"),
        "error should mention duplicate: {}",
        err.message
    );

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_missing_index_is_not_detected_as_workspace() {
    let dir = unique_temp_dir("ws-no-index");
    let ws = dir.join("workspace");
    fs::create_dir_all(ws.join("tasks")).expect("create dirs");

    assert!(!workspace::is_workspace(&ws));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn workspace_empty_tasks_directory_is_reported() {
    let (ws, _machine_path) =
        create_workspace("ws-empty", "# Rhei: Empty Test\n", &[], fixtures::TEST_STATE_MACHINE);

    let err = workspace::load_workspace(&ws).expect_err("should fail on empty");
    assert!(err.message.contains("no tasks"), "error should mention no tasks: {}", err.message);

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}

#[test]
fn workspace_transition_updates_correct_task_file() {
    let (ws, machine_path) = create_workspace(
        "ws-transition",
        "# Rhei: Transition Test\n",
        &[
            ("a.md", "### Task 1: Alpha\n**State:** pending\n"),
            ("b.md", "### Task 2: Beta\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("transition")
        .arg(&ws)
        .arg("--task")
        .arg("1")
        .arg("--from")
        .arg("pending")
        .arg("--to")
        .arg("in-progress")
        .arg("--no-callbacks")
        .output()
        .expect("transition command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "transition should succeed\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify Task 1's file was updated.
    let a_content = fs::read_to_string(ws.join("tasks/a.md")).expect("read a.md");
    assert!(
        a_content.contains("**State:** in-progress"),
        "a.md should have updated state: {}",
        a_content
    );

    // Verify Task 2's file was NOT modified.
    let b_content = fs::read_to_string(ws.join("tasks/b.md")).expect("read b.md");
    assert!(b_content.contains("**State:** pending"), "b.md should be untouched: {}", b_content);

    fs::remove_dir_all(ws.parent().unwrap()).expect("cleanup");
}
