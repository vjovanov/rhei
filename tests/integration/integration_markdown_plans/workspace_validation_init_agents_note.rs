// §AR-source-file-size.3: the agent-discovery note (§FS-rhei-init.4) has cases
// of its own — which instruction file it picks, where it is anchored, how a
// mangled note heals — split from `rhei init`'s scaffolding and refusal cases.

/// §FS-rhei-init.4: a repository whose agent instructions live only in
/// CLAUDE.md gets the note there — a fresh AGENTS.md next to it would land
/// where the resident agent never looks. Re-runs rewrite the note in place.
#[test]
fn init_writes_agent_note_into_claude_md_when_it_is_the_only_instruction_file() {
    let dir = unique_temp_dir("init-claude-md");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    fs::write(dir.join("CLAUDE.md"), "# My project rules\n\nBe nice.\n").expect("write claude");

    let output = rhei_command()
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
    let output = rhei_command()
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
}

/// §FS-rhei-init.4: with both instruction files present, AGENTS.md stays the
/// canonical target (the common CLAUDE.md → AGENTS.md symlink reads through).
#[test]
fn init_prefers_agents_md_when_both_instruction_files_exist() {
    let dir = unique_temp_dir("init-both-notes");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    fs::write(dir.join("AGENTS.md"), "# House rules\n").expect("write agents");
    fs::write(dir.join("CLAUDE.md"), "# Claude rules\n").expect("write claude");

    let output = rhei_command()
        .arg("init")
        .current_dir(&dir)
        .output()
        .expect("init runs");
    assert!(output.status.success(), "init should succeed");
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    let claude = fs::read_to_string(dir.join("CLAUDE.md")).expect("claude untouched");
    assert!(agents.contains("<!-- rhei:begin -->"), "AGENTS.md should carry the note: {agents}");
    assert!(!claude.contains("<!-- rhei:begin -->"), "CLAUDE.md must stay untouched: {claude}");
}

#[test]
fn init_force_heals_a_mangled_agents_note() {
    let dir = unique_temp_dir("init-heal-agents");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    // A merge ate the begin marker, leaving a marker-less note body plus an
    // orphaned end marker, followed by an intact duplicate.
    fs::write(
        dir.join("AGENTS.md"),
        "# House rules\n\nBe kind.\n\n## Rhei\n\nThis directory is a Rhei (Panta) project. Old text.\n<!-- rhei:end -->\n\n<!-- rhei:begin -->\n## Rhei\n\nThis directory is a Rhei (Panta) project. Old text.\n<!-- rhei:end -->\n",
    )
    .expect("write mangled agents");
    assert!(
        rhei_command()
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
}

#[test]
fn init_strips_an_orphaned_begin_marker_without_eating_user_content() {
    let dir = unique_temp_dir("init-orphaned-begin");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    // A merge lost the end marker; the user's own sections follow the note.
    fs::write(
        dir.join("AGENTS.md"),
        "# House rules\n\n<!-- rhei:begin -->\n## Rhei\n\nThis directory is a Rhei (Panta) project. Old text.\n\n## Deployment\n\nAlways deploy on Fridays.\n",
    )
    .expect("write mangled agents");

    assert!(
        rhei_command()
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
}
