// §AR-source-file-size.3: which state machine a project resolves to, and what it
// says when several rhei roots or a broken states file make that ambiguous.
// Plan-target resolution is a sibling; fixtures live in `common.rs`.

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
        rhei_command().arg("list").arg(&dir).output().expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("flow.1"),
        "project should load with the rhei-root machine file\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
        rhei_command().arg("list").arg(&dir).output().expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("flow.1"),
        "the name-matching rhei-root machine file should win over the mismatched root file\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
        rhei_command().arg("list").arg(&dir).output().expect("list runs");
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
        rhei_command().arg("list").arg(&dir).output().expect("list runs");
    assert!(!output.status.success(), "a broken candidate machine file must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to parse state machine"),
        "the parse failure should surface, not a misleading not-found: {stderr}"
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

    let output = rhei_command()
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
