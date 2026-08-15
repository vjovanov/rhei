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

/// A machine that declares an artifact contract, so a reset can be checked
/// against the paths a ticket actually writes. §FS-rhei-states.6
const ARTIFACT_CONTRACT_STATE_MACHINE: &str = r#"name: workspace-test-machine
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
    outputs:
      - name: notes
        path: runtime/notes/{task_id}.md
  in-progress:
    description: Task currently being worked on
    inputs:
      - name: notes
        path: runtime/notes/{task_id}.md
        optional: true
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

/// A path broken across lines cannot be copied, clicked, or grepped, and the
/// CLI prints one in nearly every diagnostic. miette's defaults offered a
/// break at every `/` and `-`; the handler installed in `main` removes them.
#[test]
fn diagnostics_never_break_a_file_path_across_lines() {
    let root = unique_temp_dir("diag-long-path");
    let deep = root
        .join("a-directory-with-hyphens")
        .join("and-another-long-segment")
        .join("plus-one-more-to-overflow-the-wrap-column");
    fs::create_dir_all(&deep).expect("create nested dirs");
    let plan_path = write_fixture_file(&deep, "broken.rhei.md", "# Not A Rhei Heading\n");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&plan_path)
        .output()
        .expect("validate command should run");
    assert!(!output.status.success(), "the malformed plan must fail validation");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let wanted = plan_path.display().to_string();
    assert!(
        stderr.lines().any(|line| line.contains(&wanted)),
        "the path must appear intact on one line so it stays copy-pasteable.\n\
         wanted: {wanted}\ngot:\n{stderr}"
    );

    fs::remove_dir_all(root).expect("cleanup");
}

/// §FS-rhei-plan-language.1.2: one freshly created workspace directory used to
/// fail the whole project's load with an error that named no file at all.
#[test]
fn empty_workspace_rhei_does_not_break_the_project_and_is_warned_about() {
    let project = create_panta_project(
        "panta-empty-rhei",
        "# Panta: Product Suite\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            ("growth/index.rhei.md", "# Rhei: Growth\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("empty rhei must not fail the load");
    assert_eq!(loaded.rhei_ids, vec!["auth", "growth"]);
    assert_eq!(loaded.rhei.tasks.len(), 1, "the sibling rhei's tickets must still load");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "an empty rhei is valid\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("rhei 'growth' holds no tickets"),
        "validate must name the empty rhei so a mistyped `tasks/` is not silent, got:\n{combined}"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

/// The duplicate-id diagnostic used to be masked by the empty-workspace error,
/// so a collision surfaced as an unattributed "workspace contains no tasks".
#[test]
fn duplicate_rhei_id_is_reported_even_when_one_side_is_empty() {
    let project = create_panta_project(
        "panta-dup-empty",
        "# Panta: Product Suite\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            ("auth/index.rhei.md", "# Rhei: Auth Dir\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let error = workspace::load_panta_project(&project).expect_err("duplicate ids must fail");
    assert!(
        error.message.contains("duplicate rhei id 'auth'"),
        "expected the collision to surface, got:\n{}",
        error.message
    );

    fs::remove_dir_all(project).expect("cleanup");
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
            ("billing/index.rhei.md", "# Rhei: Billing\n\n## Notes\nBilling context.\n"),
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
            ("auth/tasks/login.md", "### Task 1: Login\n**State:** pending\n"),
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
            ("auth/tasks/login.md", "### Task 1: Login\n**State:** pending\n"),
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
        stderr.contains("section 'Rhei auth / Overview'") && stderr.contains("docs/missing.md"),
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

// A rhei restating the project machine is the degenerate override — the same
// machine governs it either way. §FS-rhei-plan-language.1.3
#[test]
fn panta_rhei_may_restate_the_project_state_machine() {
    let project = create_panta_project(
        "panta-rhei-states-match",
        "# Panta: Product Suite\n**States:** workspace-test-machine\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n**States:** workspace-test-machine\n\n## Tasks\n\n\
             ### Task 1: Login\n**State:** pending\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("restating the default loads");
    assert_eq!(loaded.rhei.tasks[0].id.to_string(), "auth.1");

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_basin_loads_as_reserved_last_rhei() {
    let project = create_panta_project(
        "panta-basin",
        "# Panta: Captures\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
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

/// §AR-rhei-panta.1: the basin's manifest is synthetic, so an authored index
/// can never load. Skipping it silently vanished unfiled tickets behind a green
/// validate — what the basin exists to prevent (§FS-rhei-panta.4).
#[test]
fn panta_basin_index_file_is_a_load_error_not_a_silent_skip() {
    let project = create_panta_project(
        "panta-basin-index",
        "# Panta: Captures\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "basin/index.rhei.md",
                "# Rhei: Basin\n\n## Tasks\n\n### Task 3: Triage later\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let error = workspace::load_panta_project(&project)
        .expect_err("an authored basin index must fail the load");
    let message = error.message;
    assert!(
        message.contains("basin/index.rhei.md") || message.contains("basin\\index.rhei.md"),
        "the error must name the offending file, got:\n{message}"
    );
    assert!(
        message.contains("the basin has no authored index"),
        "the error must explain why the file cannot load, got:\n{message}"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        !output.status.success(),
        "validate must not report success while basin tickets are unloadable\nstdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_basin_ignores_runtime_markdown_artifacts() {
    let project = create_panta_project(
        "panta-basin-runtime",
        "# Panta: Captures\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
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
        err.message.contains("`basin` is reserved")
            && err.message.contains("basin.rhei.md")
            && err.message.contains("Rename"),
        "error should state the rule, the offending path, and the fix: {}",
        err.message
    );

    fs::remove_dir_all(project).expect("cleanup");
}

/// A second machine with disjoint state names, so a project mixing it with
/// `workspace-test-machine` proves each ticket is judged under the machine of
/// the rhei that owns it. §DA-per-rhei-state-machines
const CHILD_FLOW_STATE_MACHINE: &str = r#"name: child-flow
version: 1
states:
  open:
    description: Ticket waiting for work
    initial: true
  done:
    description: Ticket finished
    final: true
transitions:
  - from: open
    to: done
"#;

/// §FS-rhei-plan-language.1.3: a member rhei's own `**States:**` declaration
/// overrides the project default; the two machines govern side by side.
#[test]
fn panta_child_rhei_state_machine_override_loads_and_validates() {
    let project = create_panta_project(
        "panta-child-states",
        "# Panta: Mixed Machines\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            ("payments/index.rhei.md", "# Rhei: Payments\n**States:** child-flow\n"),
            ("payments/tasks/one.md", "### Task 1: Charge\n**State:** open\n"),
            ("payments/states.yaml", CHILD_FLOW_STATE_MACHINE),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("mixed machines load");
    assert_eq!(loaded.rhei_machines.get("payments").map(String::as_str), Some("child-flow"));
    assert!(!loaded.rhei_machines.contains_key("auth"), "a silent rhei stays on the default");

    // `pending` exists only in the default machine and `open` only in
    // child-flow, so a green validate proves per-ticket dispatch.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "mixed machines should validate\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_profile_resolution_uses_rhei_local_task_depth() {
    let project = create_panta_project(
        "panta-profile-depth",
        "# Panta: Profile Depth\n**States:** panta-profile-machine\n",
        &[("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n")],
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
fn panta_transition_routes_rewrite_to_owning_rhei_file() {
    let project = create_panta_project(
        "panta-mutate",
        "# Panta: Mutable\n**States:** workspace-test-machine\n",
        &[("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n")],
        WORKSPACE_STATE_MACHINE,
    );

    // Project-scoped mutation targets the qualified ticket id and rewrites the
    // owning rhei file with its rhei-local heading. §FS-rhei-panta.6.1
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
    assert!(
        output.status.success(),
        "transition should succeed for Panta projects\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = fs::read_to_string(project.join("auth.rhei.md")).expect("read child rhei file");
    assert!(
        rewritten.contains("### Task 1: Login\n**State:** in-progress"),
        "child rhei file should carry the new state under its local heading: {rewritten}"
    );

    // The transition ledger lands in the owning rhei's runtime, keyed by the
    // project-qualified id. §AR-rhei-panta.2
    let ledger = fs::read_to_string(project.join("runtime/state-transitions.log"))
        .expect("read transition ledger");
    assert!(
        ledger.contains("auth.1 pending@in-progress"),
        "ledger should record the qualified ticket id: {ledger}"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_next_peek_reads_and_claim_writes_owning_rhei() {
    let project = create_panta_project(
        "panta-next-peek",
        "# Panta: Peek\n**States:** workspace-test-machine\n",
        &[("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n")],
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

    // Claim mode writes `**Assignee:**` into the owning rhei's file, resolved
    // through the source map. §FS-rhei-panta.6.1
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("next")
        .arg(&project)
        .arg("--no-callbacks")
        .output()
        .expect("next claim command should run");
    assert!(
        output.status.success(),
        "next claim should succeed for Panta projects\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let rewritten = fs::read_to_string(project.join("auth.rhei.md")).expect("read child rhei file");
    assert!(
        rewritten.contains("**Assignee:**"),
        "claim should write the assignee into the owning rhei file: {rewritten}"
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
    assert!(stdout.contains("Task workspace.1: Alpha"));

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
/// §FS-rhei-plan-language.1.2: an empty workspace is a valid, empty rhei. It
/// loads, and validate warns so a mistyped `tasks/` is not mistaken for a
/// deliberately empty one.
fn workspace_empty_tasks_directory_loads_and_is_warned_about() {
    let (ws, _machine_path) =
        create_workspace("ws-empty", "# Rhei: Empty Test\n", &[], fixtures::TEST_STATE_MACHINE);

    let loaded = workspace::load_workspace(&ws).expect("an empty workspace is valid");
    assert!(loaded.rhei.tasks.is_empty());

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&ws)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "an empty workspace must validate\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("holds no tickets"),
        "validate must name the emptiness rather than report a bare green, got:\n{combined}"
    );

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

#[test]
fn panta_rhei_narrowing_scopes_candidates_and_spares_other_rhei_runtime() {
    let project = create_panta_project(
        "panta-narrow",
        "# Panta: Narrow\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // An unknown rhei is rejected and names what is available. §FS-rhei-panta.6
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .arg(&project)
        .arg("--rhei")
        .arg("nope")
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "unknown rhei must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown rhei 'nope'") && stderr.contains("auth, billing"),
        "error should name available rheis: {stderr}"
    );

    // `--rhei` narrows the listing to the named rhei.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .arg(&project)
        .arg("--rhei")
        .arg("auth")
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("auth.1"), "auth ticket should be listed: {stdout}");
    assert!(!stdout.contains("billing.1"), "billing ticket should be filtered out: {stdout}");

    // Complete one ticket in each rhei so both own runtime artifacts. The
    // fixture machine reaches a terminal state via `in-progress`.
    for task in ["auth.1", "billing.1"] {
        let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("transition")
            .arg(&project)
            .args(["--task", task, "--from", "pending", "--to", "in-progress", "--no-callbacks"])
            .output()
            .expect("transition runs");
        assert!(output.status.success(), "transition {task} should succeed");
        let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("complete")
            .arg(&project)
            .args(["--task", task, "--result", "done", "--no-callbacks"])
            .output()
            .expect("complete runs");
        assert!(
            output.status.success(),
            "complete {task} should succeed\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // A narrowed reset must not destroy the other rhei's runtime state — these
    // sibling single-file rheis share one execution root. §FS-rhei-panta.6.4
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .arg("--rhei")
        .arg("auth")
        .output()
        .expect("reset runs");
    assert!(
        output.status.success(),
        "narrowed reset should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("narrowed to 1 rhei: auth"),
        "reset should report the narrowed scope"
    );

    assert!(
        !project.join("runtime/results/auth.1.md").exists(),
        "in-scope result artifact should be removed"
    );
    assert!(
        project.join("runtime/results/billing.1.md").exists(),
        "out-of-scope rhei must keep its result artifact"
    );
    let billing = fs::read_to_string(project.join("billing.rhei.md")).expect("read billing");
    assert!(billing.contains("**State:** completed"), "billing must stay completed: {billing}");
    let auth = fs::read_to_string(project.join("auth.rhei.md")).expect("read auth");
    assert!(auth.contains("**State:** pending"), "auth must be reset: {auth}");

    // A reset ticket's ledger history goes with it, so the recorded history
    // cannot claim a completion the plan no longer holds. §FS-rhei-panta.6.4
    let ledger = fs::read_to_string(project.join("runtime/state-transitions.log"))
        .expect("read transition ledger");
    assert!(!ledger.contains("auth.1 "), "reset ticket's ledger lines should be pruned: {ledger}");
    assert!(
        ledger.contains("billing.1 "),
        "out-of-scope ticket must keep its ledger lines: {ledger}"
    );

    // The operator is told what a narrowed reset cannot speak for, rather than
    // discovering a silent partial reset later. §FS-rhei-panta.6.4
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Kept run-scoped output"),
        "narrowed reset should name what it kept"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

/// A narrowed reset removes every artifact keyed by an in-scope ticket —
/// including declared artifact contracts, whose stale outputs would satisfy a
/// required input on the next run — and nothing else. §FS-rhei-reset.2.1
#[test]
fn panta_narrowed_reset_clears_ticket_owned_artifacts_without_touching_siblings() {
    let project = create_panta_project(
        "panta-narrow-artifacts",
        "# Panta: Narrow Artifacts\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        ARTIFACT_CONTRACT_STATE_MACHINE,
    );

    // Runtime artifacts as the run surfaces write them. `auth.10` guards the
    // prefix match: it must survive a reset narrowed to `auth.1`'s rhei only
    // because it belongs to no in-scope ticket id.
    let runtime = project.join("runtime");
    for dir in
        ["logs", "results", "worktree-refs", "accounting/tasks", "accounting/captures", "notes"]
    {
        fs::create_dir_all(runtime.join(dir)).expect("create runtime dir");
    }
    fs::create_dir_all(runtime.join("snapshot-sessions/auth.1-pending-slug-7"))
        .expect("create snapshot session");
    for file in [
        "logs/task-auth.1-pending.log",
        "logs/task-auth.10-pending.log",
        "logs/task-billing.1-pending.log",
        "results/auth.1.md",
        "results/billing.1.md",
        "worktree-refs/auth.1.yaml",
        "accounting/tasks/auth.1.json",
        "accounting/captures/auth.1-pending-1.json",
        "accounting/captures/billing.1-pending-1.json",
        "notes/auth.1.md",
        "notes/billing.1.md",
    ] {
        fs::write(runtime.join(file), "x").expect("seed runtime artifact");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .args(["--rhei", "auth"])
        .output()
        .expect("reset runs");
    assert!(
        output.status.success(),
        "narrowed reset should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for gone in [
        "logs/task-auth.1-pending.log",
        "results/auth.1.md",
        "worktree-refs/auth.1.yaml",
        "accounting/tasks/auth.1.json",
        "accounting/captures/auth.1-pending-1.json",
        "notes/auth.1.md",
        "snapshot-sessions/auth.1-pending-slug-7",
    ] {
        assert!(!runtime.join(gone).exists(), "{gone} is owned by auth.1 and should be removed");
    }
    for kept in [
        "logs/task-auth.10-pending.log",
        "logs/task-billing.1-pending.log",
        "results/billing.1.md",
        "accounting/captures/billing.1-pending-1.json",
        "notes/billing.1.md",
    ] {
        assert!(runtime.join(kept).exists(), "{kept} is not owned by an in-scope ticket");
    }

    fs::remove_dir_all(project).expect("cleanup");
}

/// `--rhei` narrows candidates but never prior resolution, so the diagnostic
/// must name the out-of-scope prior rather than report the out-of-scope ticket
/// as work in progress. §FS-rhei-panta.6.1
#[test]
fn panta_narrowed_next_explains_a_prior_outside_the_scope() {
    let project = create_panta_project(
        "panta-narrow-blocked",
        "# Panta: Narrow Blocked\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n\
                 **Prior:** Task auth.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("next")
        .arg(&project)
        .args(["--rhei", "billing", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(!output.status.success(), "the only candidate is blocked");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--rhei scope (billing)")
            && stderr.contains("billing.1")
            && stderr.contains("outside the --rhei scope"),
        "diagnostic should name the scope and the blocking prior outside it: {stderr}"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

/// A project machine whose initial state carries autonomous agent work, so
/// `rhei run` takes the orchestrated (agent-mode) scheduling path.
const AGENT_WORK_STATE_MACHINE: &str = r#"name: workspace-test-machine
version: 1
states:
  pending:
    description: Task not yet started
    initial: true
    agent: fake
  completed:
    description: Task finished successfully
    final: true
transitions:
  - from: pending
    to: completed
"#;

#[test]
fn panta_run_rhei_narrowing_skips_out_of_scope_work_in_agent_mode() {
    let project = create_panta_project(
        "panta-run-narrow",
        "# Panta: Narrow Run\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        AGENT_WORK_STATE_MACHINE,
    );

    // The fake agent records every ticket it is spawned for.
    let script = project.join("fake-agent.sh");
    fs::write(
        &script,
        // §FS-rhei-states.3.3: `pending -> completed` is terminal, so the agent
        // writes the ticket's result before it exits.
        "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$RHEI_TASK_ID\" >> \"$RHEI_ROOT/agent-invocations.txt\"\nmkdir -p \"$(dirname \"$RHEI_RESULT_PATH\")\"\nprintf '## Result\\n\\nDone.\\n' > \"$RHEI_RESULT_PATH\"\n",
    )
    .expect("write fake agent");
    make_run_agent_script_executable(&script);
    write_run_agent_settings(
        &project,
        &format!(
            r#"{{ "agents": {{ "fake": {{ "command": [{}], "timeout": "5s" }} }} }}"#,
            serde_json::to_string(&script.display().to_string()).expect("script path json")
        ),
    );

    // §FS-rhei-run.2.5: the sequential agent loop schedules in-scope work only.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("run")
        .arg(&project)
        .args(["--rhei", "auth", "--no-callbacks"])
        .output()
        .expect("run runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // §FS-rhei-panta.6.1: out-of-scope tickets left non-terminal are not a
    // run failure for a narrowed invocation.
    assert!(
        output.status.success(),
        "narrowed run should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("narrowed to") && stdout.contains("auth"),
        "run should report its narrowed scope: {stdout}"
    );

    let invocations =
        fs::read_to_string(project.join("agent-invocations.txt")).expect("agent ran for auth");
    assert_eq!(invocations, "auth.1\n", "only the in-scope ticket may spawn an agent");

    let auth = fs::read_to_string(project.join("auth.rhei.md")).expect("read auth");
    assert!(auth.contains("**State:** completed"), "in-scope ticket should finish: {auth}");
    let billing = fs::read_to_string(project.join("billing.rhei.md")).expect("read billing");
    assert!(
        billing.contains("**State:** pending"),
        "out-of-scope ticket must stay untouched: {billing}"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn list_indents_and_reports_depth_rhei_locally_despite_qualified_ids() {
    let dir = unique_temp_dir("list-depth");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Depth\n\n## Tasks\n\n### Task 1: Parent\n**State:** pending\n\n#### Task 1.1: Child\n**State:** pending\n",
    )
    .expect("write plan");

    // §FS-rhei-list.4.1: top-level tickets are flush-left; the qualification
    // segment adds no indentation.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .arg(dir.join("plan.rhei.md"))
        .output()
        .expect("list runs");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| line.starts_with("Task plan.1: Parent")),
        "top-level ticket must be flush-left: {stdout}"
    );
    assert!(
        stdout.lines().any(|line| line.starts_with("  Task plan.1.1: Child")),
        "child ticket must be indented one level: {stdout}"
    );

    // §FS-rhei-list.4.2: `depth` is 1-based within the owning rhei.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .arg(dir.join("plan.rhei.md"))
        .arg("--json")
        .output()
        .expect("list --json runs");
    assert!(output.status.success());
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json payload");
    let depths: Vec<(String, u64)> = payload
        .as_array()
        .expect("array")
        .iter()
        .map(|task| {
            (task["id"].as_str().expect("id").to_string(), task["depth"].as_u64().expect("depth"))
        })
        .collect();
    assert_eq!(
        depths,
        vec![("plan.1".to_string(), 1), ("plan.1.1".to_string(), 2)],
        "depth must not count the qualification segment"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn invalid_derived_rhei_id_error_states_rule_and_suggests_rename() {
    let dir = unique_temp_dir("invalid-rhei-id");
    fs::write(
        dir.join("My Plan.rhei.md"),
        "# Rhei: Spaces\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");

    // §AR-rhei-panta.3: the derived id is a load error with the rule and a
    // concrete rename, on every command including read-only ones.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .arg(dir.join("My Plan.rhei.md"))
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "invalid derived id must fail");
    // miette wraps report lines, so collapse the decoration before matching.
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    // The wrap may split the suggested filename, so drop spaces entirely for
    // the rename fragment.
    let compact: String = stderr.chars().filter(|ch| !ch.is_whitespace()).collect();
    assert!(
        stderr.contains("rhei id 'My Plan'")
            && stderr.contains("must start with a letter")
            && compact.contains("Renamethefileto`My-Plan.rhei.md`"),
        "error should state the rule and suggest a rename: {stderr}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn duplicate_rhei_id_error_names_both_sources() {
    let project = create_panta_project(
        "panta-dup-id",
        "# Panta: Duplicate\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth A\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n"),
            ("auth/index.rhei.md", "# Rhei: Auth B\n\n## Notes\nWorkspace variant.\n"),
            ("auth/tasks/one.md", "### Task 1: B\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let err = workspace::load_panta_project(&project).expect_err("duplicate id should fail");
    assert!(
        err.message.contains("duplicate rhei id 'auth'")
            && err.message.contains("auth.rhei.md")
            && err.message.matches("auth").count() >= 2
            && err.message.contains("Rename"),
        "error should name both colliding sources and the fix: {}",
        err.message
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn implicit_panta_rejects_basin_named_single_file_rhei() {
    let dir = unique_temp_dir("implicit-basin");
    fs::write(
        dir.join("basin.rhei.md"),
        "# Rhei: Basin\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");

    // §FS-rhei-panta.4: the reservation also guards the implicit-Panta path.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .arg(dir.join("basin.rhei.md"))
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "basin id must be rejected on the implicit path");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`basin` is reserved") && stderr.contains("Rename"),
        "error should state the reservation and the fix: {stderr}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn unknown_task_id_error_suggests_closest_qualified_ids() {
    let dir = unique_temp_dir("unknown-task-hint");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Hints\n\n## Tasks\n\n### Task cache-key: Alpha\n**State:** pending\n",
    )
    .expect("write plan");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("next")
        .arg(dir.join("plan.rhei.md"))
        .args(["--task", "cache", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(!output.status.success(), "unknown task must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("task 'cache' not found") && stderr.contains("plan.cache-key"),
        "error should suggest the closest qualified id: {stderr}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn missing_input_artifact_error_names_pre_qualification_file() {
    let dir = unique_temp_dir("legacy-artifact-hint");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Legacy\n**States:** panta-input-machine\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");
    fs::write(dir.join("states.yaml"), PANTA_INPUT_STATE_MACHINE).expect("write machine");
    // The artifact exists under its pre-qualification (rhei-local) name only.
    fs::create_dir_all(dir.join("runtime")).expect("mkdir runtime");
    fs::write(dir.join("runtime/1.md"), "brief\n").expect("write legacy artifact");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("next")
        .arg(dir.join("plan.rhei.md"))
        .args(["--task", "1", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(!output.status.success(), "missing qualified input must fail");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        stderr.contains("Missing required input artifact: brief (runtime/plan.1.md)")
            && stderr.contains("pre-qualification artifact exists at 'runtime/1.md'"),
        "error should name the legacy file and the rename: {stderr}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-viz.7.3: a project renders as one merged graph — every rhei's
/// tickets in one plan, cross-rhei edges intact — and a member rhei renders that
/// graph narrowed to itself, keeping the far end of its cross-rhei priors.
#[test]
fn viz_renders_a_panta_project_as_one_graph_and_narrows_to_a_member() {
    let project = create_panta_project(
        "panta-viz-merged",
        "# Panta: Viz\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n\
                 **Prior:** auth.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let render = |target: &Path, out_name: &str| -> String {
        let out_file = project.join(out_name);
        let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("viz")
            .arg(target)
            .arg("--output")
            .arg(&out_file)
            .output()
            .expect("viz runs");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "viz should render\nstderr: {stderr}");
        assert!(
            !stderr.contains("not Panta-aware"),
            "a project renders as one graph, so nothing is caveated: {stderr}"
        );
        fs::read_to_string(&out_file).expect("read viz page")
    };

    // One page, one plan bundle: both rheis' tickets, and the cross-rhei edge.
    let whole = render(&project, "viz.html");
    assert!(whole.contains("auth.1"), "the project graph holds auth's ticket");
    assert!(whole.contains("billing.1"), "the project graph holds billing's ticket");

    // Narrowed to `billing`: its own ticket, plus the auth ticket its prior
    // points at, so the dependency is scoped rather than erased.
    let narrowed = render(&project.join("billing.rhei.md"), "viz-billing.html");
    assert!(narrowed.contains("billing.1"), "the named rhei's ticket stays");
    assert!(narrowed.contains("auth.1"), "the far end of a cross-rhei prior stays");

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn scope_report_prints_project_wide_line_and_stays_quiet_for_bare_rhei() {
    let project = create_panta_project(
        "panta-scope-line",
        "# Panta: Scope\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // §FS-rhei-panta.6: an un-narrowed project-scoped reset announces the
    // rheis it will touch before acting.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .output()
        .expect("reset runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "reset should succeed: {stdout}");
    assert!(
        stdout.contains("Scope: `rhei reset` operates project-wide across 2 rheis: auth, billing"),
        "project-wide reset should report its scope: {stdout}"
    );
    fs::remove_dir_all(project).expect("cleanup");

    // §FS-rhei-panta.6.2: a bare rhei is a one-rhei implicit Panta with no
    // fan-out to report — no scope line.
    let dir = unique_temp_dir("bare-scope-quiet");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Quiet\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("reset")
        .arg("-y")
        .arg(dir.join("plan.rhei.md"))
        .output()
        .expect("reset runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "reset should succeed: {stdout}");
    assert!(!stdout.contains("Scope:"), "one-rhei project must stay quiet: {stdout}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn ambiguous_rhei_local_shorthand_names_qualified_candidates() {
    let project = create_panta_project(
        "panta-ambiguous-shorthand",
        "# Panta: Ambiguous\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // §FS-rhei-panta.6: a shorthand matching more than one rhei is an error
    // that names the qualified candidates.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("next")
        .arg(&project)
        .args(["--task", "1", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(!output.status.success(), "ambiguous shorthand must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ambiguous across rheis")
            && stderr.contains("auth.1")
            && stderr.contains("billing.1"),
        "error should name the qualified candidates: {stderr}"
    );

    // A --rhei narrowing disambiguates the same shorthand.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("next")
        .arg(&project)
        .args(["--task", "1", "--rhei", "auth", "--peek", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(
        output.status.success(),
        "narrowed shorthand should resolve\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("auth.1"),
        "resolved ticket should be qualified in output"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn omitted_plan_target_resolves_from_current_directory() {
    // §FS-rhei-panta.6: invoked inside a project, a command with no target
    // operates on the whole project.
    let project = create_panta_project(
        "panta-cwd-resolve",
        "# Panta: Cwd\n**States:** workspace-test-machine\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&project)
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("auth.1") && stdout.contains("billing.1"),
        "bare `rhei list` inside a project should list it\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Nested inside the project (a subdirectory with no plan of its own), the
    // upward walk still finds the project.
    let nested = project.join("notes");
    fs::create_dir_all(&nested).expect("mkdir nested");
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&nested)
        .output()
        .expect("list runs");
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("auth.1"),
        "the upward walk should find the enclosing project"
    );
    fs::remove_dir_all(project).expect("cleanup");

    // A lone `.rhei.md` in the directory resolves to that rhei.
    let dir = unique_temp_dir("cwd-lone-rhei");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Lone\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("plan.1"),
        "a lone rhei file should resolve"
    );

    // Several bare rheis with no manifest are ambiguous, and the error names
    // both fixes.
    fs::write(
        dir.join("second.rhei.md"),
        "# Rhei: Second\n\n## Tasks\n\n### Task 1: Beta\n**State:** pending\n",
    )
    .expect("write second plan");
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "ambiguous directory must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("plan.rhei.md")
            && stderr.contains("second.rhei.md")
            && stderr.contains("index.panta.md"),
        "ambiguity error should name the candidates and the project fix: {stderr}"
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn init_creates_project_with_manifest_gitignore_and_agents_note() {
    let dir = unique_temp_dir("init-fresh");
    let host = dir.join("my-cool_project");

    // §FS-rhei-init.2: manifest in panta/, ignore rules, agent note at the
    // host, empty-project hint.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .arg(&host)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(
        stdout.contains("Initialized Panta project \"My Cool Project\" at panta/")
            && stdout.contains("no rheis yet"),
        "init should report the derived title, location, and empty state: {stdout}"
    );
    assert_eq!(
        fs::read_to_string(host.join("panta/index.panta.md")).expect("manifest"),
        "# Panta: My Cool Project\n"
    );
    // §FS-rhei-init.3: the project folder is ignored at the host; generated
    // output stays self-contained inside it.
    let host_ignore = fs::read_to_string(host.join(".gitignore")).expect("host gitignore");
    assert!(
        host_ignore.lines().any(|line| line.trim() == "panta/"),
        "host gitignore should ignore the project folder: {host_ignore}"
    );
    let project_ignore =
        fs::read_to_string(host.join("panta/.gitignore")).expect("project gitignore");
    assert!(
        project_ignore.contains("runtime/") && project_ignore.contains(".rhei/cache/"),
        "project gitignore should cover generated output: {project_ignore}"
    );
    let agents = fs::read_to_string(host.join("AGENTS.md")).expect("agents note");
    assert!(
        agents.contains("<!-- rhei:begin -->")
            && agents.contains("lives in `panta/`")
            && agents.contains("<!-- rhei:end -->"),
        "AGENTS.md should carry the marked note naming the location: {agents}"
    );

    // §FS-rhei-init.2: an existing project is refused untouched.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .arg(&host)
        .output()
        .expect("init runs");
    assert!(!output.status.success(), "re-init must fail");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(stderr.contains("already a Panta project"), "refusal should say why: {stderr}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn init_adopts_existing_bare_rheis_and_unblocks_bare_commands() {
    let dir = unique_temp_dir("init-adopt");
    fs::write(
        dir.join("auth.rhei.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    fs::write(
        dir.join("billing.rhei.md"),
        "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
    )
    .expect("write billing");
    // An existing AGENTS.md is appended to, not clobbered.
    fs::write(dir.join("AGENTS.md"), "# House rules\n\nBe kind.\n").expect("write agents");

    // The ambiguity error names `rhei init` as the fix. §FS-rhei-panta.6
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("rhei init"),
        "ambiguity error should point at init"
    );

    // §FS-rhei-init.2: default mode refuses to shadow existing plans and
    // names both fixes.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .current_dir(&dir)
        .output()
        .expect("init runs");
    assert!(!output.status.success(), "default init over bare rheis must refuse");
    let refusal: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let refusal = refusal.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        refusal.contains("--here") && refusal.contains("auth.rhei.md"),
        "refusal should name the stranded rheis and the adoption flag: {refusal}"
    );

    // §FS-rhei-init.5: adoption (--here) reports the discovered rheis.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .args(["--here", "--title", "Adopted"])
        .current_dir(&dir)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(
        stdout.contains("Initialized Panta project \"Adopted\" with 2 rheis: auth, billing"),
        "init should report discovered rheis: {stdout}"
    );
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    assert!(
        agents.starts_with("# House rules") && agents.contains("<!-- rhei:begin -->"),
        "existing AGENTS.md content should be preserved: {agents}"
    );

    // The bare invocation now resolves the new project.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("auth.1") && stdout.contains("billing.1"),
        "bare list should work after init: {stdout}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-init.4: a repository whose agent instructions live only in
/// CLAUDE.md gets the note there — a fresh AGENTS.md next to it would land
/// where the resident agent never looks. Re-runs rewrite the note in place.
#[test]
fn init_writes_agent_note_into_claude_md_when_it_is_the_only_instruction_file() {
    let dir = unique_temp_dir("init-claude-md");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    fs::write(dir.join("CLAUDE.md"), "# My project rules\n\nBe nice.\n").expect("write claude");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .current_dir(&dir)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(!dir.join("AGENTS.md").exists(), "no AGENTS.md should be created: {stdout}");
    let claude = fs::read_to_string(dir.join("CLAUDE.md")).expect("claude note");
    assert!(
        claude.starts_with("# My project rules") && claude.contains("<!-- rhei:begin -->"),
        "CLAUDE.md should keep its content and gain the note: {claude}"
    );
    assert!(
        stdout.contains("Also changed in the host directory: .gitignore, CLAUDE.md"),
        "init should name CLAUDE.md as the changed file: {stdout}"
    );

    // A forced re-run rewrites the note in CLAUDE.md instead of creating a
    // sibling AGENTS.md or duplicating the block.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .args(["init", "--force"])
        .current_dir(&dir)
        .output()
        .expect("init re-runs");
    assert!(output.status.success(), "forced re-init should succeed");
    assert!(!dir.join("AGENTS.md").exists(), "re-run must not create AGENTS.md");
    let claude = fs::read_to_string(dir.join("CLAUDE.md")).expect("claude note");
    assert_eq!(
        claude.matches("<!-- rhei:begin -->").count(),
        1,
        "note must not duplicate: {claude}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// §FS-rhei-init.4: with both instruction files present, AGENTS.md stays the
/// canonical target (the common CLAUDE.md → AGENTS.md symlink reads through).
#[test]
fn init_prefers_agents_md_when_both_instruction_files_exist() {
    let dir = unique_temp_dir("init-both-notes");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    fs::write(dir.join("AGENTS.md"), "# House rules\n").expect("write agents");
    fs::write(dir.join("CLAUDE.md"), "# Claude rules\n").expect("write claude");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .current_dir(&dir)
        .output()
        .expect("init runs");
    assert!(output.status.success(), "init should succeed");
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    let claude = fs::read_to_string(dir.join("CLAUDE.md")).expect("claude untouched");
    assert!(agents.contains("<!-- rhei:begin -->"), "AGENTS.md should carry the note: {agents}");
    assert!(!claude.contains("<!-- rhei:begin -->"), "CLAUDE.md must stay untouched: {claude}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn init_no_agents_skips_note_and_bad_plans_surface_as_warning() {
    let dir = unique_temp_dir("init-warn");
    fs::write(
        dir.join("My Plan.rhei.md"),
        "# Rhei: Broken\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write bad plan");

    // §FS-rhei-init.5: a discovery failure is a warning, not an init failure.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .args(["--here", "--no-agents"])
        .current_dir(&dir)
        .output()
        .expect("init runs");
    assert!(output.status.success(), "init should still succeed");
    assert!(dir.join("index.panta.md").is_file(), "manifest should be written");
    assert!(!dir.join("AGENTS.md").exists(), "--no-agents should skip the note");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not load cleanly") && stderr.contains("My Plan"),
        "load error should surface as a warning: {stderr}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn init_leaves_the_manifest_bare_over_rhei_declared_machines() {
    let dir = unique_temp_dir("init-declared-machines");
    fs::write(dir.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write machine");
    fs::write(
        dir.join("auth.rhei.md"),
        "# Rhei: Auth\n**States:** workspace-test-machine\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    fs::write(
        dir.join("billing.rhei.md"),
        "# Rhei: Billing\n**States:** workspace-test-machine\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
    )
    .expect("write billing");

    // §FS-rhei-init.2: the manifest stays bare — each rhei keeps the machine
    // it declares, so nothing needs hoisting into the project default.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .args(["--here", "--no-agents"])
        .current_dir(&dir)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(
        stdout.contains("with 2 rheis: auth, billing"),
        "init should discover both rheis: {stdout}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("does not load cleanly"),
        "rhei-declared machines load without any project default"
    );
    let manifest = fs::read_to_string(dir.join("index.panta.md")).expect("manifest");
    assert!(!manifest.contains("**States:**"), "the manifest stays bare: {manifest}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn project_machine_file_resolves_from_a_rhei_root_by_name() {
    // A single workspace rhei keeps its machine file in its own root; the
    // project declares that machine but has no root states.yaml.
    let dir = unique_temp_dir("panta-machine-in-rhei-root");
    fs::write(
        dir.join("index.panta.md"),
        "# Panta: Machine In Rhei\n**States:** workspace-test-machine\n",
    )
    .expect("write manifest");
    let ws = dir.join("flow");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Flow\n**States:** workspace-test-machine\n")
        .expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Alpha\n**State:** pending\n")
        .expect("write task");
    fs::write(ws.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write machine");

    // §AR-rhei-panta.4: a name-matching states.yaml in a rhei root resolves
    // the project machine when the project root has none.
    let output =
        Command::new(env!("CARGO_BIN_EXE_rhei")).arg("list").arg(&dir).output().expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("flow.1"),
        "project should load with the rhei-root machine file\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn init_force_overwrites_manifest_without_duplicating_companions() {
    let dir = unique_temp_dir("init-force");
    let run_init = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("init")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("init runs")
    };
    assert!(run_init(&[]).status.success(), "first init should succeed");

    // §FS-rhei-init.2: --force rewrites the manifest in place…
    let output = run_init(&["--force", "--title", "Renamed"]);
    assert!(
        output.status.success(),
        "forced re-init should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(dir.join("panta/index.panta.md")).expect("manifest"),
        "# Panta: Renamed\n"
    );

    // …and the idempotent companion files are updated, never duplicated.
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    assert_eq!(
        agents.matches("<!-- rhei:begin -->").count(),
        1,
        "AGENTS.md block must not duplicate: {agents}"
    );
    let host_ignore = fs::read_to_string(dir.join(".gitignore")).expect("host gitignore");
    assert_eq!(
        host_ignore.matches("panta/").count(),
        1,
        "host gitignore entry must not duplicate: {host_ignore}"
    );
    let project_ignore =
        fs::read_to_string(dir.join("panta/.gitignore")).expect("project gitignore");
    assert_eq!(
        project_ignore.matches("runtime/").count(),
        1,
        "project gitignore entries must not duplicate: {project_ignore}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn init_force_heals_a_mangled_agents_note() {
    let dir = unique_temp_dir("init-heal-agents");
    // A merge ate the begin marker, leaving a marker-less note body plus an
    // orphaned end marker, followed by an intact duplicate.
    fs::write(
        dir.join("AGENTS.md"),
        "# House rules\n\nBe kind.\n\n## Rhei\n\nThis directory is a Rhei (Panta) project. Old text.\n<!-- rhei:end -->\n\n<!-- rhei:begin -->\n## Rhei\n\nThis directory is a Rhei (Panta) project. Old text.\n<!-- rhei:end -->\n",
    )
    .expect("write mangled agents");
    assert!(
        Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("init")
            .current_dir(&dir)
            .output()
            .expect("init runs")
            .status
            .success(),
        "init should succeed"
    );

    // §FS-rhei-init.4: every trace is stripped and exactly one block remains.
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    assert!(agents.starts_with("# House rules"), "unrelated content preserved: {agents}");
    assert_eq!(agents.matches("<!-- rhei:begin -->").count(), 1, "one begin: {agents}");
    assert_eq!(agents.matches("<!-- rhei:end -->").count(), 1, "one end: {agents}");
    assert_eq!(agents.matches("## Rhei").count(), 1, "one section: {agents}");
    assert!(!agents.contains("Old text."), "stale bodies removed: {agents}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn omitted_plan_target_resolves_conventional_panta_child() {
    // §FS-rhei-panta.6: the `panta/` child rhei init creates resolves from
    // the host directory and anywhere under it.
    let host = unique_temp_dir("panta-child-resolve");
    let project = host.join("panta");
    fs::create_dir_all(&project).expect("mkdir panta");
    fs::write(project.join("index.panta.md"), "# Panta: Child\n").expect("write manifest");
    fs::write(
        project.join("auth.rhei.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    let nested = host.join("src/deep");
    fs::create_dir_all(&nested).expect("mkdir nested");

    for cwd in [&host, &nested] {
        let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("list")
            .current_dir(cwd)
            .output()
            .expect("list runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("auth.1"),
            "bare list from {} should resolve the panta/ child: {stdout}\nstderr: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fs::remove_dir_all(host).expect("cleanup");
}

#[test]
fn empty_project_is_valid_and_list_exits_successfully() {
    let host = unique_temp_dir("empty-project-ok");
    assert!(
        Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("init")
            .arg("--no-agents")
            .current_dir(&host)
            .output()
            .expect("init runs")
            .status
            .success(),
        "init should succeed"
    );

    // §FS-rhei-panta.6: a just-initialized project lists successfully and
    // says how to grow, instead of erroring.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&host)
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "empty project must not be a list error\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("no tickets yet") && stdout.contains("index.panta.md"),
        "empty listing should say how to grow the project: {stdout}"
    );

    // Machine consumers get an empty array, not prose.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .arg("--json")
        .current_dir(&host)
        .output()
        .expect("list --json runs");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "[]");

    fs::remove_dir_all(host).expect("cleanup");
}

#[test]
fn empty_project_validate_warns_that_discovery_found_nothing() {
    let host = unique_temp_dir("empty-project-validate");
    fs::write(host.join("index.panta.md"), "# Panta: Empty\n").expect("write manifest");
    // A plan missing the `.rhei.md` suffix is invisible to discovery; validate
    // must not report the project green without saying so. §FS-rhei-panta.6
    fs::write(
        host.join("auth.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n",
    )
    .expect("write misnamed plan");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .current_dir(&host)
        .output()
        .expect("validate runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "an empty project validates successfully\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("holds no rheis") && stdout.contains("*.rhei.md"),
        "validate should warn that discovery found nothing: {stdout}"
    );

    fs::remove_dir_all(host).expect("cleanup");
}

#[test]
fn reset_never_infers_an_omitted_target() {
    // §FS-rhei-panta.6: reset destroys runtime state, so it is excluded from
    // omitted-target resolution even inside a resolvable project.
    let project = create_panta_project(
        "reset-explicit-target",
        "# Panta: Reset\n**States:** workspace-test-machine\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** in-progress\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("reset")
        .current_dir(&project)
        .output()
        .expect("reset runs");
    assert!(!output.status.success(), "bare `rhei reset` must refuse");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        stderr.contains("never infers its target"),
        "the refusal should say why and how: {stderr}"
    );
    let plan = fs::read_to_string(project.join("auth.rhei.md")).expect("plan intact");
    assert!(
        plan.contains("**State:** in-progress"),
        "a refused reset must not touch ticket states: {plan}"
    );

    // The explicit form still works.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .output()
        .expect("reset runs");
    assert!(
        output.status.success(),
        "explicit reset should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = fs::read_to_string(project.join("auth.rhei.md")).expect("plan reset");
    assert!(plan.contains("**State:** pending"), "explicit reset applies: {plan}");

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn omitted_target_counts_workspace_rheis_and_skips_dotfiles() {
    // A bare file next to a workspace directory is ambiguous — the walk counts
    // rheis exactly as project discovery does. §FS-rhei-panta.6
    let dir = unique_temp_dir("cwd-workspace-ambiguity");
    fs::write(
        dir.join("auth.rhei.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    let ws = dir.join("billing");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Billing\n").expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Invoice\n**State:** pending\n")
        .expect("write task");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "file + workspace rhei must be ambiguous");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("auth.rhei.md") && stderr.contains("billing"),
        "ambiguity error should name both rheis: {stderr}"
    );
    fs::remove_dir_all(dir).expect("cleanup");

    // A lone workspace directory resolves; a hidden dotfile is not a rhei and
    // neither creates ambiguity nor resolves. §FS-rhei-panta.6
    let dir = unique_temp_dir("cwd-workspace-lone");
    let ws = dir.join("billing");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Billing\n").expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Invoice\n**State:** pending\n")
        .expect("write task");
    fs::write(dir.join("._junk.rhei.md"), "AppleDouble metadata, not markdown")
        .expect("write dotfile");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("billing.1"),
        "the workspace rhei should resolve despite the dotfile\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn omitted_target_never_adopts_a_loose_plan_from_an_ancestor() {
    // §FS-rhei-panta.6: a loose plan resolves in the invocation directory
    // only; ancestors are adopted solely through explicit manifests.
    let parent = unique_temp_dir("ancestor-loose-plan");
    fs::write(
        parent.join("notes.rhei.md"),
        "# Rhei: Notes\n\n## Tasks\n\n### Task 1: Idea\n**State:** pending\n",
    )
    .expect("write stray plan");
    let nested = parent.join("some/unrelated");
    fs::create_dir_all(&nested).expect("mkdir nested");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .current_dir(&nested)
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "the stray ancestor plan must not resolve");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        stderr.contains("no Rhei plan found"),
        "the error should say nothing was found: {stderr}"
    );

    fs::remove_dir_all(parent).expect("cleanup");
}

#[test]
fn init_force_without_here_refuses_when_the_host_is_the_project() {
    let dir = unique_temp_dir("init-force-host-project");
    let run_init = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("init")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("init runs")
    };
    assert!(run_init(&["--here", "--no-agents"]).status.success(), "adoption succeeds");

    // §FS-rhei-init.2: force means re-initialize, never nest a shadowed
    // `panta/` project inside the existing one.
    let output = run_init(&["--force"]);
    assert!(!output.status.success(), "default-mode --force over a --here project must refuse");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        stderr.contains("--force --here"),
        "the refusal should name the re-init path: {stderr}"
    );
    assert!(!dir.join("panta").exists(), "no shadowed nested project may be created");

    assert!(
        run_init(&["--force", "--here", "--no-agents"]).status.success(),
        "--force --here re-initializes the host project"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn init_loads_a_mixed_declared_and_silent_machine_set_cleanly() {
    let dir = unique_temp_dir("init-mixed-machines");
    fs::write(dir.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write machine");
    fs::write(
        dir.join("auth.rhei.md"),
        "# Rhei: Auth\n**States:** workspace-test-machine\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    fs::write(
        dir.join("billing.rhei.md"),
        "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
    )
    .expect("write billing");

    // §FS-rhei-init.2: a silent rhei runs the built-in default while a
    // declaring sibling keeps its own machine — no conflict to surface.
    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .args(["--here", "--no-agents"])
        .current_dir(&dir)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(
        stdout.contains("with 2 rheis: auth, billing"),
        "both rheis should load despite differing machines: {stdout}"
    );
    let manifest = fs::read_to_string(dir.join("index.panta.md")).expect("manifest");
    assert!(!manifest.contains("**States:**"), "the manifest stays bare: {manifest}");
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("does not load cleanly"),
        "a mixed declared/silent set loads cleanly"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn init_strips_an_orphaned_begin_marker_without_eating_user_content() {
    let dir = unique_temp_dir("init-orphaned-begin");
    // A merge lost the end marker; the user's own sections follow the note.
    fs::write(
        dir.join("AGENTS.md"),
        "# House rules\n\n<!-- rhei:begin -->\n## Rhei\n\nThis directory is a Rhei (Panta) project. Old text.\n\n## Deployment\n\nAlways deploy on Fridays.\n",
    )
    .expect("write mangled agents");

    assert!(
        Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("init")
            .current_dir(&dir)
            .output()
            .expect("init runs")
            .status
            .success(),
        "init should succeed"
    );

    // §FS-rhei-init.4: the orphaned marker and stale note body go; the user's
    // sections after them stay.
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    assert!(
        agents.contains("## Deployment") && agents.contains("Always deploy on Fridays."),
        "user content after an orphaned begin marker must survive: {agents}"
    );
    assert!(agents.starts_with("# House rules"), "leading content preserved: {agents}");
    assert_eq!(agents.matches("<!-- rhei:begin -->").count(), 1, "one begin: {agents}");
    assert_eq!(agents.matches("<!-- rhei:end -->").count(), 1, "one end: {agents}");
    assert!(!agents.contains("Old text."), "stale note body removed: {agents}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn project_machine_in_rhei_root_beats_a_mismatched_project_root_file() {
    // §AR-rhei-panta.4: the rhei-root fallback applies when the project-root
    // states.yaml names a *different* machine, not only when it is absent.
    let dir = unique_temp_dir("panta-machine-mismatch-fallback");
    fs::write(
        dir.join("index.panta.md"),
        "# Panta: Mismatch Fallback\n**States:** workspace-test-machine\n",
    )
    .expect("write manifest");
    fs::write(
        dir.join("states.yaml"),
        "name: unrelated-machine\nversion: 1\nstates:\n  open:\n    description: Open\n    initial: true\n  done:\n    description: Done\n    final: true\ntransitions:\n  - from: open\n    to: done\n",
    )
    .expect("write unrelated machine");
    let ws = dir.join("flow");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Flow\n**States:** workspace-test-machine\n")
        .expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Alpha\n**State:** pending\n")
        .expect("write task");
    fs::write(ws.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write machine");

    let output =
        Command::new(env!("CARGO_BIN_EXE_rhei")).arg("list").arg(&dir).output().expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("flow.1"),
        "the name-matching rhei-root machine file should win over the mismatched root file\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn project_machine_resolution_errors_when_several_rhei_roots_match() {
    // §AR-rhei-panta.4: only a *unique* name match resolves; two rhei roots
    // holding files that declare the machine is an ambiguity error, not a
    // silent first-match — one could be a stale copy.
    let dir = unique_temp_dir("panta-machine-ambiguous");
    fs::write(
        dir.join("index.panta.md"),
        "# Panta: Ambiguous Machine\n**States:** workspace-test-machine\n",
    )
    .expect("write manifest");
    for name in ["alpha", "beta"] {
        let ws = dir.join(name);
        fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
        fs::write(ws.join("index.rhei.md"), "# Rhei: Flow\n**States:** workspace-test-machine\n")
            .expect("write index");
        fs::write(ws.join("tasks/one.md"), "### Task 1: Alpha\n**State:** pending\n")
            .expect("write task");
        fs::write(ws.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write machine");
    }

    let output =
        Command::new(env!("CARGO_BIN_EXE_rhei")).arg("list").arg(&dir).output().expect("list runs");
    assert!(!output.status.success(), "ambiguous machine files must fail");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        stderr.contains("more than one rhei root")
            && stderr.contains("alpha")
            && stderr.contains("beta")
            && stderr.contains("--state-machine"),
        "error should name the candidates and the fixes: {stderr}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn project_machine_resolution_surfaces_a_broken_rhei_root_states_file() {
    // §AR-rhei-panta.4: an unloadable rhei-root candidate is an error, not a
    // silent non-match hiding behind "no states file found".
    let dir = unique_temp_dir("panta-machine-broken-candidate");
    fs::write(
        dir.join("index.panta.md"),
        "# Panta: Broken Candidate\n**States:** workspace-test-machine\n",
    )
    .expect("write manifest");
    let ws = dir.join("flow");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Flow\n**States:** workspace-test-machine\n")
        .expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Alpha\n**State:** pending\n")
        .expect("write task");
    fs::write(ws.join("states.yaml"), "name: [unclosed\n").expect("write broken machine");

    let output =
        Command::new(env!("CARGO_BIN_EXE_rhei")).arg("list").arg(&dir).output().expect("list runs");
    assert!(!output.status.success(), "a broken candidate machine file must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to parse state machine"),
        "the parse failure should surface, not a misleading not-found: {stderr}"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn init_here_refuses_to_shadow_an_existing_panta_child_project() {
    // §FS-rhei-init.2: adopting the host must not shadow a default-mode
    // project at panta/ — target resolution prefers the host manifest, so
    // the child project would become unreachable by inference.
    let dir = unique_temp_dir("init-here-shadow");
    let first = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("init")
        .current_dir(&dir)
        .output()
        .expect("init runs");
    assert!(first.status.success(), "default init should succeed");

    for args in [&["--here"][..], &["--here", "--force"][..]] {
        let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("init")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("init runs");
        assert!(
            !output.status.success(),
            "--here over an existing panta/ project must refuse ({args:?})"
        );
        let stderr: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .filter(|ch| *ch != '│' && *ch != '\n')
            .collect();
        let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            stderr.contains("would shadow") && stderr.contains("panta"),
            "refusal should explain the shadowing: {stderr}"
        );
    }
    assert!(
        !dir.join("index.panta.md").exists(),
        "the refused adoption must not write a host manifest"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn empty_project_reset_is_a_noop_success() {
    // §FS-rhei-panta.6: an empty project — exactly what `rhei init` creates —
    // is a valid state for every command; reset has nothing to rewrite and
    // reports a no-op instead of failing on the project directory.
    let dir = unique_temp_dir("panta-empty-reset");
    fs::write(dir.join("index.panta.md"), "# Panta: Empty\n").expect("write manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("reset")
        .arg("-y")
        .arg(&dir)
        .output()
        .expect("reset runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "reset on an empty project should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Reset 0 task(s)"), "reset should report a no-op: {stdout}");

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn panta_narrowed_reset_clears_workspace_index_metadata_and_legacy_records() {
    // §FS-rhei-panta.6.4: the owning index's runtime ticket metadata is
    // ticket-owned state, and legacy rhei-local records are swept at a root
    // whose every rhei is in scope.
    let project = create_panta_project(
        "panta-narrow-metadata",
        "# Panta: Narrow Metadata\n**States:** workspace-test-machine\n",
        &[
            (
                "auth/index.rhei.md",
                "# Rhei: Auth\n\n---\nmetadata:\n  tasks:\n    1:\n      stateVisits:\n        in-progress: 2\n---\n\n## Overview\nAuth.\n",
            ),
            ("auth/tasks/one.md", "### Task 1: Login\n**State:** in-progress\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );
    // Legacy pre-qualification runtime state under the workspace rhei's own
    // root, keyed by the rhei-local id.
    let auth_runtime = project.join("auth/runtime");
    fs::create_dir_all(auth_runtime.join("results")).expect("mkdir results");
    fs::write(auth_runtime.join("results/1.md"), "## Result\n\nlegacy\n").expect("seed result");
    fs::write(auth_runtime.join("state-transitions.log"), "1 pending@in-progress\n")
        .expect("seed ledger");
    // A legacy record at the *shared* project root must survive: `billing`
    // also roots there and is out of scope, so a bare local id is ambiguous.
    let project_runtime = project.join("runtime");
    fs::create_dir_all(project_runtime.join("results")).expect("mkdir project results");
    fs::write(project_runtime.join("results/1.md"), "## Result\n\nambiguous\n")
        .expect("seed shared result");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .args(["--rhei", "auth"])
        .output()
        .expect("reset runs");
    assert!(
        output.status.success(),
        "narrowed reset should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let index = fs::read_to_string(project.join("auth/index.rhei.md")).expect("read index");
    assert!(
        !index.contains("stateVisits"),
        "the in-scope workspace index must lose its runtime visit counters: {index}"
    );
    assert!(
        !auth_runtime.join("results/1.md").exists(),
        "the legacy local-id result at the rhei's own root should be swept"
    );
    assert!(
        !auth_runtime.join("state-transitions.log").exists(),
        "the legacy local-id ledger line should be pruned (file emptied)"
    );
    assert!(
        project_runtime.join("results/1.md").exists(),
        "a local-id record at the shared root is ambiguous and must survive"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

#[test]
fn panta_run_locks_every_member_rhei_execution_root() {
    // §FS-rhei-run.2.6: a project-level run locks the project root *and*
    // each member rhei's execution root, so a direct `rhei run <member>` and
    // the project run contend on the same lock.
    let project = create_panta_project(
        "panta-run-locks",
        "# Panta: Run Locks\n**States:** workspace-test-machine\n",
        &[
            ("auth/index.rhei.md", "# Rhei: Auth\n"),
            ("auth/tasks/one.md", "### Task 1: Login\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("run")
        .arg(&project)
        .arg("--no-callbacks")
        .output()
        .expect("run runs");
    assert!(
        output.status.success(),
        "project run should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.join(".rhei/run.lock").is_file(), "the project root must be locked");
    assert!(
        project.join("auth/.rhei/run.lock").is_file(),
        "the member workspace rhei's root must be locked too"
    );

    fs::remove_dir_all(project).expect("cleanup");
}

/// Basin tickets keep runtime metadata in the project manifest, so commands
/// that advance a ticket can read and write it. Parsing their bare task files
/// as whole plans made the whole basin unworkable. §FS-rhei-panta.6.1
#[test]
fn basin_tickets_transition_and_complete() {
    let project = create_panta_project(
        "panta-basin-transition",
        "# Panta: Basin Work\n**States:** workspace-test-machine\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
            ),
            ("basin/quick.md", "### Task 1: Fix typo\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let transition = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("transition")
        .arg(&project)
        .args(["--task", "basin.1", "--from", "pending", "--to", "in-progress"])
        .output()
        .expect("transition command should run");
    assert!(
        transition.status.success(),
        "basin ticket should transition\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&transition.stdout),
        String::from_utf8_lossy(&transition.stderr)
    );

    let complete = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("complete")
        .arg(&project)
        .args(["--task", "basin.1", "--result", "done"])
        .output()
        .expect("complete command should run");
    assert!(
        complete.status.success(),
        "basin ticket should complete\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&complete.stdout),
        String::from_utf8_lossy(&complete.stderr)
    );

    let listed = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("list")
        .arg(&project)
        .args(["--rhei", "basin"])
        .output()
        .expect("list command should run");
    let stdout = String::from_utf8_lossy(&listed.stdout);
    assert!(
        stdout.contains("Task basin.1: Fix typo [completed]"),
        "basin ticket should be completed, got:\n{stdout}"
    );
}

/// §FS-rhei-states-cmd.3: `rhei states` reports the machine the project runs
/// under. Printing the built-in default while the project declared another
/// named every state wrong.
#[test]
fn states_command_resolves_the_projects_declared_machine() {
    let project = create_panta_project(
        "panta-states-cmd",
        "# Panta: Declared\n**States:** workspace-test-machine\n",
        &[("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n")],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("states")
        .arg(&project)
        .output()
        .expect("states command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "states should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("State machine: workspace-test-machine"),
        "states should report the declared machine, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Source: ") && stdout.contains("states.yaml"),
        "states should name the resolved source file, got:\n{stdout}"
    );
}

/// A parse error in a rhei inside a project keeps the code frame that file
/// gets when validated directly — the project form is the one `rhei init`
/// steers new authors toward. §FS-rhei-panta.6
#[test]
fn project_parse_errors_keep_their_line_and_code_frame() {
    let project = create_panta_project(
        "panta-parse-frame",
        "# Panta: Broken\n**States:** workspace-test-machine\n",
        &[("broken.rhei.md", "# Onboarding\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n")],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "validate should fail on the malformed rhei");
    for fragment in ["PARSE ERROR", "line 1", "# Onboarding", "broken.rhei.md"] {
        assert!(
            stderr.contains(fragment),
            "project parse error should include {fragment:?}, got:\n{stderr}"
        );
    }
}

/// §FS-rhei-reset.1.2: `--dry-run` reports the damage and changes nothing.
#[test]
fn reset_dry_run_changes_nothing() {
    let project = create_panta_project(
        "panta-reset-dry-run",
        "# Panta: Reset\n**States:** workspace-test-machine\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** completed\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );
    let plan = project.join("auth.rhei.md");
    let before = fs::read_to_string(&plan).expect("read plan before reset");

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("reset")
        .arg(&project)
        .arg("--dry-run")
        .output()
        .expect("reset command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "dry run should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Would reset") && stdout.contains("Dry run"),
        "dry run should preview the reset, got:\n{stdout}"
    );
    assert_eq!(
        fs::read_to_string(&plan).expect("read plan after dry run"),
        before,
        "dry run must not rewrite the plan"
    );
}

/// Reaching a plan through its project must not cost the author diagnostics.
/// Task headings under a content section fail first as "Metadata field appears
/// outside a task", on a line that is not the mistake; only the structural
/// diagnostic explains it, and it must survive both scopes.
// §FS-rhei-validate.4.2: both scopes report every problem in the file.
#[test]
fn project_scoped_parse_errors_match_file_scoped_ones() {
    let plan = "# Rhei: A\n\n## Overview\n\n### Task 1: one\n\n**State:** pending\n\n\
                ### Task 2: two\n\n**State:** pending\n";
    let project = create_panta_project(
        "panta-parse-parity",
        "# Panta: Parity\n**States:** workspace-test-machine\n",
        &[("c.rhei.md", plan)],
        WORKSPACE_STATE_MACHINE,
    );

    let run = |target: &Path| -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("validate")
            .arg(target)
            .output()
            .expect("validate command should run");
        assert!(!output.status.success(), "the malformed plan must fail validation");
        let stderr: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .filter(|ch| *ch != '│' && *ch != '\n')
            .collect();
        normalize_for_assertions(&stderr)
    };

    let by_project = run(&project);
    let by_file = run(&project.join("c.rhei.md"));

    for scope in [&by_project, &by_file] {
        assert!(scope.contains("(3 problems)"), "every problem must be reported, got:\n{scope}");
        assert!(
            scope.contains("Tasks section must be the final '##' chapter"),
            "the structural diagnostic explains the mistake, got:\n{scope}"
        );
        assert!(
            scope.contains("line 7:") && scope.contains("line 11:"),
            "each problem keeps its line, got:\n{scope}"
        );
    }
}

/// §FS-rhei-validate.3: a `**Prior:**` naming an unknown rhei is reported under
/// the id the author wrote — never re-qualified with the citing rhei — and names
/// both the unknown rhei and the near miss.
#[test]
fn missing_prior_names_the_unknown_rhei_and_suggests_the_near_miss() {
    let project = create_panta_project(
        "panta-prior-typo",
        "# Panta: Typo\n**States:** workspace-test-machine\n",
        &[
            (
                "onboarding.rhei.md",
                "# Rhei: Onboarding\n\n## Tasks\n\n### Task 1: Research\n**State:** pending\n",
            ),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Pick\n**State:** pending\n\
                 **Prior:** onbaording.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    // miette wraps report lines, so collapse the decoration before matching.
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = normalize_for_assertions(&stderr);
    assert!(!output.status.success(), "validate should fail on the unknown rhei");
    assert!(
        stderr.contains("no rhei named 'onbaording'"),
        "error should name the unknown rhei, got:\n{stderr}"
    );
    assert!(
        stderr.contains("Did you mean 'onboarding.1'?"),
        "error should suggest the near miss, got:\n{stderr}"
    );
    assert!(
        stderr.contains("missing Task onbaording.1"),
        "error should quote the authored id, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("billing.onbaording.1"),
        "error must not re-qualify the authored id with the citing rhei, got:\n{stderr}"
    );
}

/// §FS-rhei-validate.3: a correction is only offered when it resolves to some
/// other ticket. A one-character rhei name is within one edit of every other, so
/// the near-miss search must not propose the citing task as its own prior.
#[test]
fn missing_prior_never_suggests_the_citing_task_as_its_own_prior() {
    let project = create_panta_project(
        "panta-prior-self-suggest",
        "# Panta: Short\n**States:** workspace-test-machine\n",
        &[
            ("b.rhei.md", "# Rhei: B\n\n## Tasks\n\n### Task 1: Other\n**State:** pending\n"),
            (
                "a.rhei.md",
                "# Rhei: A\n\n## Tasks\n\n### Task 1: Pick\n**State:** pending\n\
                 **Prior:** c.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = normalize_for_assertions(&stderr);
    assert!(!output.status.success(), "validate should fail on the unknown rhei");
    assert!(
        stderr.contains("missing Task c.1"),
        "error should quote the authored id, got:\n{stderr}"
    );
    assert!(!stderr.contains("Did you mean"), "no correction should be offered, got:\n{stderr}");
    assert!(
        stderr.contains("rhei 'a' has no ticket 'c.1'"),
        "error should rule out the local-id reading too, got:\n{stderr}"
    );
}

/// §FS-rhei-render.3.4: a merged project renders rhei by rhei, not as one flat
/// task list under a run of headings, and the progress format leads with the
/// completion summary it is named for.
#[test]
fn project_render_groups_tickets_under_their_rhei() {
    let project = create_panta_project(
        "panta-render-groups",
        "# Panta: Store\n**States:** workspace-test-machine\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Authentication\n\n## Overview\n\nWho gets in.\n\n## Tasks\n\n\
                 ### Task 1: Login\n**State:** completed\n",
            ),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let render = |format: &str| -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_rhei"))
            .arg("render")
            .arg(&project)
            .arg("--format")
            .arg(format)
            .arg("--no-color")
            .output()
            .expect("render runs");
        assert!(
            output.status.success(),
            "render should succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let github = render("github");
    // §FS-rhei-render.3.4: the heading carries the rhei id when it differs
    // from the title (`Authentication (auth)`); `Billing` already names its
    // id and stays bare.
    let auth_heading = github.find("## Authentication (auth)").expect("auth heading");
    let billing_heading = github.find("## Billing\n").expect("billing heading");
    let auth_ticket = github.find("auth.1").expect("auth ticket");
    let billing_ticket = github.find("billing.1").expect("billing ticket");
    assert!(
        auth_heading < auth_ticket && auth_ticket < billing_heading,
        "each rhei's tickets belong under its own heading:\n{github}"
    );
    assert!(billing_heading < billing_ticket, "billing's ticket follows its heading:\n{github}");
    assert!(
        !github.contains("Rhei auth / Overview"),
        "the merge prefix is not part of the document:\n{github}"
    );
    assert!(!github.contains("\n\n\n"), "no runs of blank lines:\n{github:?}");

    let progress = render("progress");
    assert!(
        progress.contains("1/2 tickets done (50%)"),
        "progress leads with the completion summary:\n{progress}"
    );
    let auth_heading = progress.find("\nAuthentication (auth)\n").expect("auth group");
    let billing_heading = progress.find("\nBilling\n").expect("billing group");
    assert!(
        auth_heading < progress.find("auth.1").expect("auth ticket")
            && progress.find("auth.1").expect("auth ticket") < billing_heading,
        "progress groups tickets under their rhei too:\n{progress}"
    );

    fs::remove_dir_all(project).expect("cleanup");
}
