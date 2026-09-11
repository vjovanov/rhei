// §AR-source-file-size.3: the agent-discovery note (§FS-rhei-init.4) has cases
// of its own — which instruction file it picks, where it is anchored, how a
// mangled note heals — split from `rhei init`'s scaffolding and refusal cases.

/// The hint's opening words. Every case below asserts on this one constant,
/// so a reworded hint breaks the positive and the negative cases together
/// rather than quietly passing the ones that only check for its absence.
const HINT: &str = "Hint: init only writes files it chose inside the host directory.";

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
    // §FS-rhei-init.4: the host *is* the repository root here, so there is
    // nothing above it to point from and the hint must stay silent.
    assert!(!stdout.contains(HINT), "a host that is the root gets no hint: {stdout}");

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

/// A host inside a repository it does not own keeps its own note: the
/// enclosing root's hand-written instruction file is never modified, and a
/// printed hint takes the place of the write it used to make. §FS-rhei-init.4
fn assert_the_note_stays_in_the_host(prefix: &str, mode: &[&str], location: &str) {
    const ROOT_RULES: &str = "# House rules\n\nBe kind.\n";
    let repo = unique_temp_dir(prefix);
    fs::create_dir_all(repo.join(".git")).expect("mark repo root");
    fs::write(repo.join("AGENTS.md"), ROOT_RULES).expect("write root agents");
    let host = repo.join("host");
    fs::create_dir_all(&host).expect("create host");
    // The hint names the *root's* file; only the enclosing directory name
    // tells it apart from the note written into the host.
    let root_note = format!(
        "{}{}AGENTS.md",
        repo.file_name().and_then(|name| name.to_str()).expect("repo directory name"),
        std::path::MAIN_SEPARATOR
    );

    let output = rhei_command().arg("init").arg(&host).args(mode).output().expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    let note = fs::read_to_string(host.join("AGENTS.md")).expect("host agent note");
    assert!(
        note.contains("<!-- rhei:begin -->") && note.contains(location),
        "the note belongs in the host and names the project location: {note}"
    );
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).expect("root agents"),
        ROOT_RULES,
        "the enclosing repository's AGENTS.md must be left byte-identical"
    );
    assert!(
        stdout.contains("Also changed in the host directory: .gitignore, AGENTS.md")
            && stdout.contains(HINT)
            && stdout.contains(&root_note),
        "init should name the host change and hint at the root's file: {stdout}"
    );
}

/// Adopting a subdirectory of someone else's repository (#116).
#[test]
fn init_here_writes_the_note_in_the_host_not_the_enclosing_repository_root() {
    assert_the_note_stays_in_the_host(
        "init-enclosing-here",
        &["--here"],
        "This directory is a Rhei (Panta) project.",
    );
}

/// The same host, in default mode, where the project is the `panta/` child.
#[test]
fn init_writes_the_note_in_the_host_not_the_enclosing_repository_root() {
    assert_the_note_stays_in_the_host("init-enclosing-default", &[], "lives in `panta/`");
}

/// §FS-rhei-init.4: `--no-agents` writes no note, so there is no project to
/// advertise upward and the hint has nothing to suggest. Same layout as the
/// cases above, which do print it — only the flag differs.
#[test]
fn init_prints_no_enclosing_repository_hint_when_the_note_is_skipped() {
    let repo = unique_temp_dir("init-enclosing-no-agents");
    fs::create_dir_all(repo.join(".git")).expect("mark repo root");
    fs::write(repo.join("AGENTS.md"), "# House rules\n").expect("write root agents");
    let host = repo.join("host");
    fs::create_dir_all(&host).expect("create host");

    let output = rhei_command()
        .arg("init")
        .arg(&host)
        .args(["--here", "--no-agents"])
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(!host.join("AGENTS.md").exists(), "--no-agents should skip the note");
    assert!(!stdout.contains(HINT), "no note written means no hint: {stdout}");
}

/// §FS-rhei-init.4: the upgrade case. An earlier revision anchored the note at
/// the enclosing root, so the pointer the hint asks for is already there —
/// init leaves that note alone and says nothing about it.
#[test]
fn init_prints_no_hint_when_the_enclosing_root_already_carries_a_note() {
    let repo = unique_temp_dir("init-enclosing-migrated");
    fs::create_dir_all(repo.join(".git")).expect("mark repo root");
    // What the old version left at the root: a note pointing at the host.
    let root_note = "# House rules\n\n<!-- rhei:begin -->\n## Rhei\n\nThe Rhei (Panta) project for this repository lives in `panta/`.\n<!-- rhei:end -->\n";
    fs::write(repo.join("AGENTS.md"), root_note).expect("write root agents");
    let host = repo.join("host");
    fs::create_dir_all(&host).expect("create host");

    let output =
        rhei_command().arg("init").arg(&host).arg("--here").output().expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(!stdout.contains(HINT), "the pointer already exists, so no hint: {stdout}");
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).expect("root agents"),
        root_note,
        "the stale note stays where it is — removing it is the user's call"
    );
    assert!(
        fs::read_to_string(host.join("AGENTS.md"))
            .expect("host agent note")
            .contains("<!-- rhei:begin -->"),
        "the host still gets its own note"
    );
}

/// §FS-rhei-init.4: two begin markers before one end marker leave the *first*
/// orphaned — it is removed alone, and the user content a merge stranded
/// between the two stays. Pairing the first begin with the second block's end
/// reads that content as note material and deletes it silently (#187).
#[test]
fn init_pairs_a_begin_marker_with_an_end_only_when_no_second_begin_intervenes() {
    let dir = unique_temp_dir("init-two-begins");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    // A merge left a second begin marker above an intact block, with the
    // user's own line between the two.
    fs::write(
        dir.join("AGENTS.md"),
        "# House rules\n\n<!-- rhei:begin -->\nKeep the staging secrets out of the repo.\n<!-- rhei:begin -->\n## Rhei\n\nThis directory is a Rhei (Panta) project. Old text.\n<!-- rhei:end -->\n\n## Deployment\n\nAlways deploy on Fridays.\n",
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

    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    assert!(
        agents.contains("Keep the staging secrets out of the repo."),
        "user content between the two begin markers must survive: {agents}"
    );
    assert!(agents.starts_with("# House rules"), "leading content preserved: {agents}");
    assert!(
        agents.contains("## Deployment") && agents.contains("Always deploy on Fridays."),
        "user content after the block must survive: {agents}"
    );
    assert_eq!(agents.matches("<!-- rhei:begin -->").count(), 1, "one begin: {agents}");
    assert_eq!(agents.matches("<!-- rhei:end -->").count(), 1, "one end: {agents}");
    assert_eq!(agents.matches("## Rhei").count(), 1, "one section: {agents}");
    assert!(!agents.contains("Old text."), "stale note body removed: {agents}");
}

/// §FS-rhei-init.4: a marker-less `## Rhei` section ends where the note's
/// prose ends, never at the end of the file — trailing user content under no
/// heading of its own is the user's and survives (#187).
#[test]
fn init_keeps_trailing_user_content_that_carries_no_heading_of_its_own() {
    let dir = unique_temp_dir("init-mangled-tail");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    // A merge split the end delimiter across a line boundary, and the user's
    // closing paragraph sits under no heading that could bound the section.
    fs::write(
        dir.join("AGENTS.md"),
        "# House rules\n\n<!-- rhei:begin -->\n## Rhei\n\nThis directory is a Rhei (Panta) project. Old text.\n<!-- rhei:en\nd -->\n\nAlways deploy on Fridays.\n",
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

    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    assert!(
        agents.contains("Always deploy on Fridays."),
        "trailing user content under no heading must survive: {agents}"
    );
    assert!(agents.starts_with("# House rules"), "leading content preserved: {agents}");
    assert!(!agents.contains("Old text."), "stale note body removed: {agents}");
    assert!(
        !agents.contains("<!-- rhei:en\n"),
        "the split delimiter's remains go with the note they belonged to: {agents}"
    );
    assert_eq!(agents.matches("<!-- rhei:begin -->").count(), 1, "one begin: {agents}");
    assert_eq!(agents.matches("<!-- rhei:end -->").count(), 1, "one end: {agents}");
}

/// The combination the ticket named and no case covered: a host nested inside
/// a repository it does not own, whose *own* instruction file carries a
/// malformed block. Healing that file is never a licence to write above the
/// host — the enclosing root keeps its bytes and gets a hint. §FS-rhei-init.4
#[test]
fn init_leaves_the_enclosing_repository_untouched_when_the_host_note_is_mangled() {
    const ROOT_RULES: &str = "# House rules\n\nBe kind.\n";
    let repo = unique_temp_dir("init-enclosing-mangled");
    fs::create_dir_all(repo.join(".git")).expect("mark repo root");
    fs::write(repo.join("AGENTS.md"), ROOT_RULES).expect("write root agents");
    fs::write(repo.join("CLAUDE.md"), ROOT_RULES).expect("write root claude");
    let host = repo.join("scratch").join("host");
    fs::create_dir_all(&host).expect("create host");
    fs::write(
        host.join("AGENTS.md"),
        "# Host instructions\n\n<!-- rhei:begin -->\nKeep the staging secrets out of the repo.\n<!-- rhei:begin -->\n## Rhei\n\nThe Rhei (Panta) project for this repository lives in `panta/`. Old text.\n<!-- rhei:end -->\n",
    )
    .expect("write mangled host agents");

    let output = rhei_command().arg("init").arg(&host).output().expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).expect("root agents"),
        ROOT_RULES,
        "the enclosing repository's AGENTS.md must be left byte-identical"
    );
    assert_eq!(
        fs::read_to_string(repo.join("CLAUDE.md")).expect("root claude"),
        ROOT_RULES,
        "the enclosing repository's CLAUDE.md must be left byte-identical"
    );
    assert!(!repo.join(".gitignore").exists(), "nothing new at the enclosing root");
    assert!(!repo.join("panta").exists(), "the project belongs under the host");
    let note = fs::read_to_string(host.join("AGENTS.md")).expect("host agent note");
    assert!(
        note.contains("lives in `panta/`") && !note.contains("Old text."),
        "the healed note belongs in the host: {note}"
    );
    assert!(stdout.contains(HINT), "the enclosing root gets a hint, never a write: {stdout}");
}
