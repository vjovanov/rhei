
/// Compute a relative path from `from_dir` to `to_path`.
///
/// Makes both paths absolute without resolving symlinks (to avoid
/// symlink targets collapsing path differences). Then walks back from
/// `from_dir` and forward to `to_path` via the common ancestor.
fn relative_path(from_dir: &Path, to_path: &Path) -> PathBuf {
    // Make absolute without canonicalizing (no symlink resolution).
    let make_absolute = |p: &Path| -> PathBuf {
        if p.is_absolute() {
            p.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(p)
        } else {
            p.to_path_buf()
        }
    };

    let from = make_absolute(from_dir);
    let to = make_absolute(to_path);

    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    // Find the common prefix length.
    let common =
        from_components.iter().zip(to_components.iter()).take_while(|(a, b)| a == b).count();

    let mut result = PathBuf::new();
    // Go up from `from_dir` to the common ancestor.
    for _ in common..from_components.len() {
        result.push("..");
    }
    // Go down to `to_path` from the common ancestor.
    for component in &to_components[common..] {
        result.push(component);
    }

    result
}

/// The path a checkout or packaged asset directory offers for a named skill,
/// if either has one; otherwise the embedded copy. §FS-rhei-install-skills.4.3
fn filesystem_skill_source(skill_name: &str) -> Option<PathBuf> {
    // A directory only counts when it holds `SKILL.md`, so a leftover empty
    // directory cannot shadow the embedded copy with an unreadable install.
    let usable = |dir: PathBuf| -> Option<PathBuf> {
        if dir.join("SKILL.md").is_file() {
            dir.canonicalize().ok()
        } else {
            None
        }
    };

    // 1. A packaged asset directory installed alongside the binary.
    let exe = std::env::current_exe().ok();
    if let Some(bin_dir) = exe.as_deref().and_then(Path::parent) {
        if let Some(found) = usable(bin_dir.join("../share/rhei/skills").join(skill_name)) {
            return Some(found);
        }
    }

    // 2. A checkout, found from the binary (a dev build under `target/`) or
    //    from the current directory (an installed binary run inside a
    //    checkout, where the skills being edited are the ones to install).
    let starts = [exe.as_deref().and_then(Path::parent).map(Path::to_path_buf), std::env::current_dir().ok()];
    for start in starts.into_iter().flatten() {
        let mut dir = Some(start);
        while let Some(d) = dir {
            if let Some(found) = usable(d.join("crates/rhei-cli/skills").join(skill_name)) {
                return Some(found);
            }
            dir = d.parent().map(Path::to_path_buf);
        }
    }

    None
}

/// Skill directories resolved for one `install-skills` run.
///
/// Embedded skills are materialized into `_extraction`, which must outlive the
/// install: dropping it deletes the very directories `sources` points at.
struct ResolvedSkills {
    sources: Vec<(String, PathBuf)>,
    _extraction: Option<tempfile::TempDir>,
}

/// Locate every requested skill, preferring a filesystem copy and falling back
/// to the copy compiled into this binary. §FS-rhei-install-skills.4.3
fn resolve_skill_sources(skills: &[String], link: bool) -> MietteResult<ResolvedSkills> {
    let mut sources = Vec::new();
    let mut extraction: Option<tempfile::TempDir> = None;

    for skill in skills {
        if let Some(found) = filesystem_skill_source(skill) {
            sources.push((skill.clone(), found));
            continue;
        }
        if !builtin_skill_exists(skill) {
            return Err(unknown_skill_error(skill));
        }
        // A symlink into a temporary extraction dangles as soon as the command
        // exits, so `--link` needs a real source. §FS-rhei-install-skills.4.4
        if link {
            return Err(miette!(
                "--link needs skill files on disk, but '{skill}' is only available as the copy \
                 embedded in this binary. Searched '<binary>/../share/rhei/skills/{skill}/' and \
                 'crates/rhei-cli/skills/{skill}/' up from both the binary and the current \
                 directory. Run from a rhei checkout, or drop --link to copy the embedded skill."
            ));
        }

        let temp = match extraction {
            Some(ref t) => t,
            None => extraction.insert(
                tempfile::Builder::new()
                    .prefix("rhei-builtin-skills-")
                    .tempdir()
                    .map_err(|err| miette!("failed to create a temporary directory: {err}"))?,
            ),
        };
        sources.push((skill.clone(), materialize_builtin_skill(skill, temp.path())?));
    }

    Ok(ResolvedSkills { sources, _extraction: extraction })
}

/// Find the project root by walking up from the current directory.
///
/// Looks for common project markers (`.git`, `Cargo.toml`, `package.json`,
/// `pyproject.toml`, `go.mod`). Falls back to the current working directory
/// if no marker is found.
fn find_project_root() -> MietteResult<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette!("failed to determine working directory: {e}"))?;

    let markers = [".git", "Cargo.toml", "package.json", "pyproject.toml", "go.mod"];
    let mut dir = Some(cwd.as_path());
    while let Some(d) = dir {
        for marker in &markers {
            if d.join(marker).exists() {
                return Ok(d.to_path_buf());
            }
        }
        dir = d.parent();
    }

    // Fallback: current working directory.
    Ok(cwd)
}

/// Append or replace a delimited content block in a text file.
///
/// The block is wrapped between `<!-- rhei:start -->` and `<!-- rhei:end -->`
/// markers. If these markers already exist in the file, the content between
/// them is replaced. Otherwise the block is appended. The file is created if
/// it doesn't exist.
fn inject_marked_section(file: &Path, content: &str, dry_run: bool) -> MietteResult<()> {
    let start_marker = "<!-- rhei:start -->";
    let end_marker = "<!-- rhei:end -->";

    let existing = if file.exists() {
        fs::read_to_string(file).map_err(|e| miette!("failed to read '{}': {e}", file.display()))?
    } else {
        String::new()
    };

    let block = format!("{start_marker}\n{content}\n{end_marker}");

    let new_content = if let (Some(start), Some(end)) =
        (existing.find(start_marker), existing.find(end_marker))
    {
        // Replace existing block.
        let before = &existing[..start];
        let after = &existing[end + end_marker.len()..];
        format!("{before}{block}{after}")
    } else {
        // Append.
        if existing.is_empty() {
            block
        } else if existing.ends_with('\n') {
            format!("{existing}\n{block}\n")
        } else {
            format!("{existing}\n\n{block}\n")
        }
    };

    if dry_run {
        println!("  [dry-run] would write {} ({} bytes)", file.display(), new_content.len());
        return Ok(());
    }

    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create directory '{}': {e}", parent.display()))?;
    }
    fs::write(file, &new_content)
        .map_err(|e| miette!("failed to write '{}': {e}", file.display()))?;

    Ok(())
}

/// Whether `line` opens a `# rhei` / `## rhei` registration block.
fn is_rhei_heading(line: &str) -> bool {
    (line.starts_with("# rhei") || line.starts_with("## rhei")) && !line.starts_with("###")
}

/// Index of the first line after the `# rhei` block starting at `heading`. The
/// block is contiguous, so a blank line ends it; scanning on to the next
/// heading would delete what a user keeps between. §FS-rhei-install-skills.4.5
fn claude_md_block_end(lines: &[&str], heading: usize) -> usize {
    let heading_level = lines[heading].chars().take_while(|&c| c == '#').count();
    lines
        .iter()
        .enumerate()
        .skip(heading + 1)
        .find(|(_, line)| {
            let level = line.chars().take_while(|&c| c == '#').count();
            line.trim().is_empty() || (level > 0 && level <= heading_level)
        })
        .map_or(lines.len(), |(index, _)| index)
}

/// Remove the `<!-- rhei:start -->` … `<!-- rhei:end -->` block from a file.
///
/// Also handles the `# rhei` heading-based block used by Claude Code's
/// `CLAUDE.md`: removes from `# rhei` (or `## rhei`) to the next heading of
/// equal or higher level, or end of file.
fn remove_marked_section(file: &Path, dry_run: bool) -> MietteResult<()> {
    if !file.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(file)
        .map_err(|e| miette!("failed to read '{}': {e}", file.display()))?;

    let start_marker = "<!-- rhei:start -->";
    let end_marker = "<!-- rhei:end -->";

    let mut result = content.clone();

    // Remove marker-delimited block.
    if let (Some(start), Some(end)) = (result.find(start_marker), result.find(end_marker)) {
        let block_end = end + end_marker.len();
        // Also consume the trailing newline if present.
        let block_end =
            if result[block_end..].starts_with('\n') { block_end + 1 } else { block_end };
        // Also consume a leading blank line before the block.
        let block_start = if start > 0 && result[..start].ends_with('\n') {
            // Check if there's a double newline before the block.
            if start >= 2 && result[..start].ends_with("\n\n") {
                start - 1
            } else {
                start
            }
        } else {
            start
        };
        result = format!("{}{}", &result[..block_start], &result[block_end..]);
    }

    // Remove the `# rhei` heading block (Claude Code), and only that block —
    // whatever follows it belongs to the user. §FS-rhei-install-skills.4.5
    let lines: Vec<&str> = result.lines().collect();
    let mut new_lines: Vec<&str> = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        if is_rhei_heading(lines[index]) {
            let end = claude_md_block_end(&lines, index);
            // Drop the blank line that separated the block from what follows,
            // so removing the section does not leave a growing gap behind.
            index = if lines.get(end).is_some_and(|line| line.trim().is_empty()) {
                end + 1
            } else {
                end
            };
            continue;
        }
        new_lines.push(lines[index]);
        index += 1;
    }

    let final_content = if new_lines.is_empty() {
        String::new()
    } else {
        let mut s = new_lines.join("\n");
        if content.ends_with('\n') {
            s.push('\n');
        }
        s
    };

    if final_content == content {
        return Ok(());
    }

    if dry_run {
        println!("  [dry-run] would update {}", file.display());
        return Ok(());
    }

    fs::write(file, &final_content)
        .map_err(|e| miette!("failed to write '{}': {e}", file.display()))?;

    Ok(())
}

/// Render a plan path for a diagnostic header.
///
/// Prefers the form relative to the invocation directory when it is shorter:
/// a project loader hands back absolute paths, and an absolute path wraps
/// across the miette gutter into something no editor jump-to-file recognises.
fn display_path(path: &Path) -> String {
    let absolute = path.display().to_string();
    let Ok(cwd) = std::env::current_dir() else {
        return absolute;
    };
    let relative = relative_path(&cwd, path).display().to_string();
    // An empty relative path means the target *is* the invocation directory.
    if relative.is_empty() {
        return ".".to_string();
    }
    if relative.len() < absolute.len() {
        relative
    } else {
        absolute
    }
}

/// Convert a parser error into an Elm-style diagnostic report.
fn parse_report(path: &Path, input: &str, err: &rhei_core::parser::ParseError) -> Report {
    miette!("{}", render_parse_diagnostic(path, input, err))
}

/// Convert one or more parser errors into an Elm-style diagnostic report.
///
/// For a single error this is equivalent to [`parse_report`]. For multiple
/// errors the header, code-frame, and hint are printed once; each error is
/// listed numerically in the body.
fn parse_errors_report(
    path: &Path,
    input: &str,
    errors: &[rhei_core::parser::ParseError],
) -> Report {
    miette!("{}", render_multi_parse_diagnostic(path, input, errors))
}

struct ParseErrorGroup {
    path: PathBuf,
    input: String,
    errors: Vec<rhei_core::parser::ParseError>,
}

fn workspace_parse_errors_report(groups: &[ParseErrorGroup]) -> Report {
    miette!("{}", render_workspace_parse_diagnostic(groups))
}

fn render_workspace_parse_diagnostic(groups: &[ParseErrorGroup]) -> String {
    let error_count: usize = groups.iter().map(|group| group.errors.len()).sum();
    let file_count = groups.len();
    let problem_word = if error_count == 1 { "problem" } else { "problems" };
    let file_word = if file_count == 1 { "file" } else { "files" };
    let mut lines = vec![
        "-- PARSE ERROR ----------------------------".to_string(),
        format!("in Directory Workspace task files ({error_count} {problem_word}, {file_count} {file_word})"),
    ];
    lines.push(String::new());
    lines.push("I got stuck while reading this workspace's markdown task files.".to_string());

    let mut index = 1usize;
    for group in groups {
        lines.push(String::new());
        lines.push(format!("{}:", display_path(&group.path)));
        for err in &group.errors {
            match err.line {
                Some(line_number) => {
                    lines.push(format!("{index}. line {line_number}: {}", err.message));
                    if let Some(source_line) = line_text(&group.input, line_number) {
                        lines.push(format!("   {line_number}| {source_line}"));
                    }
                }
                None => {
                    lines.push(format!("{index}. {}", err.message));
                }
            }
            index += 1;
        }
    }

    lines.push(String::new());
    lines.push(
        "Hint: fix the problems above — each one refers to a distinct file, line, or task."
            .to_string(),
    );
    lines.join("\n")
}

fn render_multi_parse_diagnostic(
    path: &Path,
    input: &str,
    errors: &[rhei_core::parser::ParseError],
) -> String {
    if errors.len() == 1 {
        return render_parse_diagnostic(path, input, &errors[0]);
    }

    let mut lines = vec![
        "-- PARSE ERROR ----------------------------".to_string(),
        format!("in {}", display_path(path)),
    ];
    lines.push(String::new());
    lines
        .push(format!("I got stuck while reading this markdown plan ({} problems).", errors.len()));
    for (i, err) in errors.iter().enumerate() {
        lines.push(String::new());
        let prefix = format!("{}.", i + 1);
        match err.line {
            Some(line_number) => {
                lines.push(format!("{prefix} line {line_number}: {}", err.message));
                if let Some(source_line) = line_text(input, line_number) {
                    lines.push(format!("   {line_number}| {source_line}"));
                }
            }
            None => {
                lines.push(format!("{prefix} {}", err.message));
            }
        }
    }
    lines.push(String::new());
    lines.push(
        "Hint: fix the problems above — each one refers to a distinct line or task.".to_string(),
    );
    lines.join("\n")
}

/// Report a parse error raised while loading a multi-document plan (a Panta
/// project, a Directory Workspace).
///
/// When the error names the file its line belongs to and that file still
/// reads, this renders the same code frame a single-file plan gets, and
/// re-parses the file to recover the *full* error set. The point is that
/// `rhei validate` inside a project must not be less helpful than
/// `rhei validate <that same file>` — the project form is the one `rhei init`
/// steers every new author toward.
// §FS-rhei-validate.4.2: parse diagnostics read the same in both scopes.
fn nested_parse_report(err: &rhei_core::parser::ParseError) -> Report {
    let Some(path) = err.file.as_deref() else {
        return miette!("{}", err.message);
    };
    let Ok(source) = std::fs::read_to_string(path) else {
        // The file moved or is unreadable; the message still stands on its own.
        return miette!("{}: {}", path.display(), err.message);
    };
    // The project loader stops at the first error per entry, so the diagnostic
    // that actually explains the mistake — the structural one recovery reaches
    // last — would otherwise never be printed.
    let collected = collect_parse_errors(path, &source);
    if collected.len() > 1 && collected.iter().any(|other| other.message == err.message) {
        return parse_errors_report(path, &source, &collected);
    }
    parse_report(path, &source, err)
}

/// Re-parse `path` with the error-collecting parser that matches its role in a
/// project, so a nested failure can be reported with the same completeness as
/// `rhei validate <path>`.
///
/// Returns an empty vector when the file's role is not one this can reproduce;
/// the caller then falls back to the single error it already has.
fn collect_parse_errors(path: &Path, source: &str) -> Vec<rhei_core::parser::ParseError> {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();

    // A single-file rhei parses as a whole plan.
    if name.ends_with(".rhei.md") && name != "index.rhei.md" {
        return rhei_core::parser::parse_collect(source).1;
    }

    // A workspace task file or a basin ticket file parses as bare task nodes,
    // against the structure of the document that owns it.
    let Some(structure) = owning_structure(path) else {
        return Vec::new();
    };
    rhei_core::parser::parse_workspace_tasks_collect_with_structure(source, &structure).1
}

/// Structure governing a bare task file: its Directory Workspace index, or the
/// project manifest when the file is a basin ticket. §FS-rhei-panta.2
fn owning_structure(path: &Path) -> Option<rhei_core::ast::Structure> {
    let parent = path.parent()?;
    // `tasks/` files may nest, so walk up to the workspace root.
    let mut dir = parent;
    loop {
        let index = dir.join("index.rhei.md");
        if index.is_file() {
            let raw = std::fs::read_to_string(&index).ok()?;
            return Some(rhei_core::parser::parse_workspace_index(&raw).ok()?.structure);
        }
        let manifest = dir.join(rhei_core::workspace::PANTA_INDEX_FILE);
        if manifest.is_file() {
            let raw = std::fs::read_to_string(&manifest).ok()?;
            return Some(rhei_core::parser::parse_panta_manifest(&raw).ok()?.structure);
        }
        dir = dir.parent()?;
    }
}

/// Convert file I/O failures into a consistent diagnostic message.
fn file_io_report(path: &Path, action: &str, err: impl std::fmt::Display) -> Report {
    miette!("{action} '{}': {err}", path.display())
}

/// Convert validation errors into a single CLI-facing diagnostic report.
fn validation_report(input: &Path, state_machine: Option<&Path>, errors: &[String]) -> Report {
    miette!("{}", render_validation_diagnostic(input, state_machine, errors))
}

fn render_parse_diagnostic(
    path: &Path,
    input: &str,
    err: &rhei_core::parser::ParseError,
) -> String {
    let mut lines = vec![
        "-- PARSE ERROR ----------------------------".to_string(),
        format!("in {}", display_path(path)),
    ];
    lines.push(String::new());
    lines.push("I got stuck while reading this markdown plan.".to_string());

    if let Some(line_number) = err.line {
        lines.push(String::new());
        lines.push(format!("I was partway through line {line_number} when the problem showed up."));

        if let Some(source_line) = line_text(input, line_number) {
            lines.push(String::new());
            lines.push(format!("{line_number}| {source_line}"));
            lines.push(format!("{}{}", " ".repeat(line_number.to_string().len() + 2), "^"));
        }
    }

    lines.push(String::new());
    lines.push(err.message.replace(" before task content", "\nbefore task content"));
    lines.push(String::new());
    lines.push(
        "Hint: check the markdown structure around the highlighted line and try again.".to_string(),
    );

    lines.join("\n")
}

fn render_validation_diagnostic(
    input: &Path,
    state_machine: Option<&Path>,
    errors: &[String],
) -> String {
    let mut lines = vec![
        "-- VALIDATION ERROR ----------------------".to_string(),
        format!("in {}", display_path(input)),
    ];
    lines.push(String::new());
    lines.push(format!(
        "I validated this plan using {}, but found a problem.",
        state_machine_label(state_machine),
    ));
    lines.push(String::new());
    lines.push(format_validation_errors(errors));
    lines.push(String::new());
    lines.push("I recommend fixing the problems above and running the command again.".to_string());

    lines.join("\n")
}

fn format_validation_errors(errors: &[String]) -> String {
    if errors.len() == 1 {
        format!("The problem is:\n\n    {}", errors[0])
    } else {
        let mut lines = vec![format!("I found {} problems:", errors.len()), String::new()];
        lines.extend(
            errors.iter().enumerate().map(|(index, error)| format!("{}. {}", index + 1, error)),
        );
        lines.join("\n")
    }
}

fn line_text(input: &str, line_number: usize) -> Option<&str> {
    input.lines().nth(line_number.saturating_sub(1))
}

/// One line naming both routes to a first rhei, for messages with no room.
/// Saying only *where* the file goes left the harder half — what belongs
/// inside it — unstated just when an author needs it. §FS-rhei-panta.6
fn add_a_rhei_hint() -> &'static str {
    "add one with `rhei instantiate <template>` (`rhei templates` lists them), or by hand as a \
     `<id>.rhei.md` file next to index.panta.md"
}

/// The same two routes with room to show what a hand-written plan looks like.
/// §FS-rhei-panta.6
fn add_a_rhei_help() -> String {
    [
        "Add a rhei either way:",
        "  from a template  `rhei templates` lists them; `rhei instantiate <name>` writes one",
        "                   into this project, keeping the template's own state machine",
        "  by hand          create `<id>.rhei.md` next to index.panta.md:",
        "",
        "                       # Rhei: <title>",
        "",
        "                       ## Tasks",
        "",
        "                       ### Task 1: <first ticket>",
        "                       **State:** pending",
    ]
    .join("\n")
}
