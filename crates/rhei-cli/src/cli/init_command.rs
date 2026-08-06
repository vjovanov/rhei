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

/// Create or update the marked Rhei block in `AGENTS.md`. An existing block
/// between the markers is replaced in place, so the note is idempotent and
/// removable as one unit. §FS-rhei-init.4
fn write_agents_note(dir: &Path) -> MietteResult<()> {
    let path = dir.join("AGENTS.md");
    let block = format!("{AGENTS_NOTE_BEGIN}\n{AGENTS_NOTE_BODY}\n{AGENTS_NOTE_END}\n");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let updated = match (existing.find(AGENTS_NOTE_BEGIN), existing.find(AGENTS_NOTE_END)) {
        (Some(begin), Some(end)) if end >= begin => {
            let after = existing[end..].split_once('\n').map(|(_, rest)| rest).unwrap_or("");
            format!("{}{}{}", &existing[..begin], block, after)
        }
        _ if existing.is_empty() => block,
        _ => {
            let mut out = existing;
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
            out.push_str(&block);
            out
        }
    };
    fs::write(&path, updated).map_err(|err| miette!("failed to write {}: {err}", path.display()))
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
