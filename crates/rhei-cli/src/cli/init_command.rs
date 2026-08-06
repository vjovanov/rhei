// `rhei init` — make a directory a Panta project: the manifest, ignore rules
// for generated output, an agent-discovery note, then a discovery report that
// doubles as a first validation. §FS-rhei-init

const AGENTS_NOTE_BEGIN: &str = "<!-- rhei:begin -->";
const AGENTS_NOTE_END: &str = "<!-- rhei:end -->";

/// The block written between the markers in `AGENTS.md`. §FS-rhei-init.4
const AGENTS_NOTE_BODY: &str = "## Rhei

This directory is a Rhei (Panta) project. Plans are `*.rhei.md` files and
workspace directories; ticket ids are project-qualified (`<rhei>.<id>`).
Drive work with `rhei list`, `rhei next`, `rhei complete`, and `rhei run`;
validate edits with `rhei validate`. Run `rhei --help` for the full surface.";

fn init_command(
    dir: Option<&Path>,
    title: Option<&str>,
    no_agents: bool,
    force: bool,
) -> MietteResult<()> {
    let dir = match dir {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|err| miette!("failed to read the current directory: {err}"))?,
    };
    fs::create_dir_all(&dir)
        .map_err(|err| miette!("failed to create {}: {err}", dir.display()))?;

    let manifest = dir.join("index.panta.md");
    // §FS-rhei-init.2: refuse an existing project untouched unless --force,
    // which rewrites the manifest and updates companion files in place.
    if manifest.is_file() && !force {
        return Err(miette!(
            "{} is already a Panta project: index.panta.md exists. Re-run with \
             `--force` to overwrite the manifest",
            dir.display()
        ));
    }
    // §FS-rhei-init.2: nested projects are almost always a mistake.
    if let Some(outer) = enclosing_panta_project(&dir) {
        eprintln!(
            "warning: {} is inside the Panta project at {}; the outer project will not \
             discover this one",
            dir.display(),
            outer.display()
        );
    }

    let title = match title {
        Some(title) => title.to_string(),
        None => default_project_title(&dir),
    };
    // §FS-rhei-init.2: adopt a unanimously declared machine as the project
    // default — a bare manifest would make such a project unloadable.
    let declared = workspace::discover_declared_state_machines(&dir);
    let contents = match declared.as_slice() {
        [machine] => {
            println!("Adopted state machine '{machine}' as the project default.");
            format!("# Panta: {title}\n**States:** {machine}\n")
        }
        _ => format!("# Panta: {title}\n"),
    };
    fs::write(&manifest, contents)
        .map_err(|err| miette!("failed to write {}: {err}", manifest.display()))?;

    seed_gitignore(&dir)?;
    if !no_agents {
        write_agents_note(&dir)?;
    }
    report_initialized_project(&dir, &title);
    println!("Next: `rhei list` shows the project; `rhei install-skills` wires agent skills.");
    Ok(())
}

/// Nearest ancestor (strictly above `dir`) that is a Panta project root.
fn enclosing_panta_project(dir: &Path) -> Option<PathBuf> {
    let start = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let mut current = start.parent();
    while let Some(dir) = current {
        if dir.join("index.panta.md").is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// Default title from the directory name: `-`/`_` become spaces and each
/// word is capitalized (`my-project` → `My Project`). §FS-rhei-init.1
fn default_project_title(dir: &Path) -> String {
    let name = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Project")
        .to_string();
    name.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Append the two generated-output entries to `.gitignore`, creating the file
/// when absent and never rewriting entries already present. §FS-rhei-init.3
fn seed_gitignore(dir: &Path) -> MietteResult<()> {
    let path = dir.join(".gitignore");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let missing: Vec<&str> = ["runtime/", ".rhei/cache/"]
        .into_iter()
        .filter(|entry| !existing.lines().any(|line| line.trim() == *entry))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str("# Rhei generated output\n");
    for entry in missing {
        out.push_str(entry);
        out.push('\n');
    }
    fs::write(&path, out).map_err(|err| miette!("failed to write {}: {err}", path.display()))
}

/// Create or update the marked Rhei block in `AGENTS.md`. Every trace of a
/// previous note is stripped first, so the note is idempotent even after a
/// third-party merge mangled the markers. §FS-rhei-init.4
fn write_agents_note(dir: &Path) -> MietteResult<()> {
    let path = dir.join("AGENTS.md");
    let block = format!("{AGENTS_NOTE_BEGIN}\n{AGENTS_NOTE_BODY}\n{AGENTS_NOTE_END}\n");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let cleaned = strip_rhei_note(&existing);
    let updated = if cleaned.trim().is_empty() {
        block
    } else {
        let mut out = cleaned.trim_end().to_string();
        out.push_str("\n\n");
        out.push_str(&block);
        out
    };
    fs::write(&path, updated).map_err(|err| miette!("failed to write {}: {err}", path.display()))
}

/// Remove every trace of a previously written agent note: marker-delimited
/// regions, orphaned markers, and a marker-less `## Rhei` section that still
/// carries the note body (a merge may have eaten the markers). §FS-rhei-init.4
fn strip_rhei_note(existing: &str) -> String {
    const SENTINEL: &str = "This directory is a Rhei (Panta) project.";
    let lines: Vec<&str> = existing.lines().collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == AGENTS_NOTE_BEGIN {
            while i < lines.len() && lines[i].trim() != AGENTS_NOTE_END {
                i += 1;
            }
            i += 1; // past the end marker (or EOF)
            continue;
        }
        if trimmed == AGENTS_NOTE_END {
            i += 1;
            continue;
        }
        if trimmed == "## Rhei" {
            let mut j = i + 1;
            let mut has_sentinel = false;
            while j < lines.len()
                && !lines[j].starts_with("## ")
                && lines[j].trim() != AGENTS_NOTE_BEGIN
            {
                has_sentinel |= lines[j].contains(SENTINEL);
                j += 1;
            }
            if has_sentinel {
                i = j;
                continue;
            }
        }
        out.push(lines[i]);
        i += 1;
    }
    let mut result = out.join("\n");
    if existing.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}

/// Load the fresh project and say what it contains; a discovery failure is a
/// warning, not an init failure, so init doubles as a first validation of a
/// directory of pre-existing plans. §FS-rhei-init.5
fn report_initialized_project(dir: &Path, title: &str) {
    match workspace::load_panta_project(dir) {
        Ok(loaded) => {
            let noun = if loaded.rhei_ids.len() == 1 { "rhei" } else { "rheis" };
            println!(
                "Initialized Panta project \"{}\" with {} {}: {}",
                title,
                loaded.rhei_ids.len(),
                noun,
                loaded.rhei_ids.join(", ")
            );
        }
        Err(err) if err.message.contains("contains no tasks") => {
            println!(
                "Initialized Panta project \"{title}\" with no rheis yet. Add one by \
                 dropping a `<id>.rhei.md` file or a workspace directory next to \
                 index.panta.md."
            );
        }
        Err(err) => {
            println!("Initialized Panta project \"{title}\".");
            eprintln!("warning: the new project does not load cleanly: {}", err.message);
        }
    }
}
