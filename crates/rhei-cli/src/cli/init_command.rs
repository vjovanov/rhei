// `rhei init` — set up a Panta project: a gitignored `panta/` folder by
// default, or the host itself with `--here` (adoption). §FS-rhei-init

const AGENTS_NOTE_BEGIN: &str = "<!-- rhei:begin -->";
const AGENTS_NOTE_END: &str = "<!-- rhei:end -->";

/// Note body shared by both modes after the location sentence. §FS-rhei-init.4
const AGENTS_NOTE_TAIL: &str = "Plans are
`*.rhei.md` files and workspace directories; ticket ids are
project-qualified (`<rhei>.<id>`). Work tickets with `rhei list`,
`rhei next`, and `rhei complete`; validate edits with `rhei validate`.
Orchestration (`rhei run`) is started by humans, never by agents.";

fn init_command(
    dir: Option<&Path>,
    title: Option<&str>,
    no_agents: bool,
    force: bool,
    here: bool,
) -> MietteResult<()> {
    let host = match dir {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|err| miette!("failed to read the current directory: {err}"))?,
    };
    let project = if here { host.clone() } else { host.join("panta") };

    // §FS-rhei-init.2: a host that is itself a project refuses default mode
    // even under --force — a fresh `panta/` child nested inside it would lose
    // every target resolution to the host manifest and never be reachable.
    if !here && host.join("index.panta.md").is_file() {
        return Err(miette!(
            "{} is already a Panta project itself: index.panta.md exists at the host. \
             Re-run with `--force --here` to re-initialize it in place; a new `panta/` \
             project inside it would be shadowed by the host manifest",
            host.display()
        ));
    }
    // §FS-rhei-init.2: refuse an existing project untouched unless --force,
    // which rewrites the manifest and updates companion files in place.
    if project.join("index.panta.md").is_file() && !force {
        return Err(miette!(
            "{} is already a Panta project: index.panta.md exists. Re-run with \
             `--force` to overwrite the manifest",
            project.display()
        ));
    }
    // §FS-rhei-init.2: a `panta/` project would not discover plans sitting in
    // the host — refuse rather than silently shadow them.
    if !here && !force {
        let stranded = workspace::discover_rhei_entries(&host).unwrap_or_default();
        if !stranded.is_empty() {
            let names: Vec<String> = stranded
                .iter()
                .filter_map(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .collect();
            return Err(miette!(
                "{} already holds {} rhei(s) ({}) that a `panta/` project would not \
                 discover. Re-run with `--here` to adopt them in place, or move them \
                 into panta/ first",
                host.display(),
                names.len(),
                names.join(", ")
            ));
        }
    }

    fs::create_dir_all(&project)
        .map_err(|err| miette!("failed to create {}: {err}", project.display()))?;

    // §FS-rhei-init.2: nested projects are almost always a mistake.
    if let Some(outer) = enclosing_panta_project(&project) {
        eprintln!(
            "warning: {} is inside the Panta project at {}; the outer project will not \
             discover this one",
            project.display(),
            outer.display()
        );
    }

    // §FS-rhei-init.1: the host names the project in both modes — `panta/`
    // is a location, not an identity.
    let title = match title {
        Some(title) => title.to_string(),
        None => default_project_title(&host),
    };
    // §FS-rhei-init.2: adopt a unanimously declared machine as the project
    // default — a bare manifest would make such a project unloadable. A rhei
    // declaring nothing runs the built-in default, so it blocks adoption too.
    let declared = workspace::discover_declared_state_machines(&project);
    let contents = match declared.as_slice() {
        [Some(machine), rest @ ..] if rest.iter().all(|d| d.as_ref() == Some(machine)) => {
            println!("Adopted state machine '{machine}' as the project default.");
            format!("# Panta: {title}\n**States:** {machine}\n")
        }
        _ => format!("# Panta: {title}\n"),
    };
    let manifest = project.join("index.panta.md");
    fs::write(&manifest, contents)
        .map_err(|err| miette!("failed to write {}: {err}", manifest.display()))?;

    // §FS-rhei-init.3: default mode ignores the whole project folder at the
    // host and self-contains the generated-output rules inside it, so
    // un-ignoring the plans later never starts committing runtime state.
    if here {
        seed_gitignore(&host, &["runtime/", ".rhei/cache/"])?;
    } else {
        seed_gitignore(&host, &["panta/"])?;
        seed_gitignore(&project, &["runtime/", ".rhei/cache/"])?;
    }
    if !no_agents {
        write_agents_note(&host, here)?;
    }
    report_initialized_project(&project, &title, here);
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

/// Default title from the host directory name: `-`/`_` become spaces and
/// each word is capitalized (`my-project` → `My Project`). §FS-rhei-init.1
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

/// Append missing entries to `dir/.gitignore`, creating the file when absent
/// and never rewriting entries already present. §FS-rhei-init.3
fn seed_gitignore(dir: &Path, entries: &[&str]) -> MietteResult<()> {
    let path = dir.join(".gitignore");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let missing: Vec<&str> = entries
        .iter()
        .copied()
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
    out.push_str("# Rhei\n");
    for entry in missing {
        out.push_str(entry);
        out.push('\n');
    }
    fs::write(&path, out).map_err(|err| miette!("failed to write {}: {err}", path.display()))
}

/// Create or update the marked Rhei block in the host's `AGENTS.md`. Every
/// trace of a previous note is stripped first, so the note is idempotent
/// even after a third-party merge mangled the markers. §FS-rhei-init.4
fn write_agents_note(host: &Path, here: bool) -> MietteResult<()> {
    let path = host.join("AGENTS.md");
    let location = if here {
        "This directory is a Rhei (Panta) project."
    } else {
        "The Rhei (Panta) project for this repository lives in `panta/`."
    };
    let block = format!(
        "{AGENTS_NOTE_BEGIN}\n## Rhei\n\n{location} {AGENTS_NOTE_TAIL}\n{AGENTS_NOTE_END}\n"
    );
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
    const SENTINEL: &str = "Rhei (Panta) project";
    let lines: Vec<&str> = existing.lines().collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == AGENTS_NOTE_BEGIN {
            // §FS-rhei-init.4: an orphaned begin marker (end marker lost) is
            // removed alone — the lines after it are user content, not the note.
            match lines[i + 1..].iter().position(|line| line.trim() == AGENTS_NOTE_END) {
                Some(end) => i += end + 2, // past the block and its end marker
                None => i += 1,
            }
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

/// Load the fresh project and say where it lives and what it contains; a
/// discovery failure is a warning, not an init failure, so init doubles as a
/// first validation of pre-existing plans. §FS-rhei-init.5
fn report_initialized_project(project: &Path, title: &str, here: bool) {
    let location = if here { String::new() } else { " at panta/".to_string() };
    match workspace::load_panta_project(project) {
        Ok(loaded) if loaded.rhei_ids.is_empty() => {
            println!(
                "Initialized Panta project \"{title}\"{location} with no rheis yet. Add \
                 one by dropping a `<id>.rhei.md` file or a workspace directory next to \
                 index.panta.md."
            );
        }
        Ok(loaded) => {
            let noun = if loaded.rhei_ids.len() == 1 { "rhei" } else { "rheis" };
            println!(
                "Initialized Panta project \"{}\"{} with {} {}: {}",
                title,
                location,
                loaded.rhei_ids.len(),
                noun,
                loaded.rhei_ids.join(", ")
            );
        }
        Err(err) => {
            println!("Initialized Panta project \"{title}\"{location}.");
            eprintln!("warning: the new project does not load cleanly: {}", err.message);
        }
    }
}
