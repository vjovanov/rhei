//! Directory Workspace loader for multi-file Rhei plans.
//!
//! A Directory Workspace consists of:
//! - `index.rhei.md`: root configuration with title, states, and content sections.
//! - `tasks/`: a directory containing `.md` files, each with one or more task definitions.
//!
//! All tasks are merged into a single global task graph. Task IDs must be
//! unique across the entire `tasks/` directory.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{ContentSection, Metadata, Rhei, Structure, Task, TaskId, TaskIdSegment};
use crate::parser::{self, ParseError};

pub const PANTA_INDEX_FILE: &str = "index.panta.md";
pub const BASIN_RHEI_ID: &str = "basin";

/// A loaded directory workspace: the merged plan plus a map from each task ID
/// to the file it was parsed from (needed for targeted file rewrites during
/// transitions).
#[derive(Debug)]
pub struct Workspace {
    pub rhei: Rhei,
    /// Maps task ID (as string) → the file path that defines it.
    pub task_sources: HashMap<String, PathBuf>,
}

/// A loaded Panta project, flattened into a project-qualified task graph for
/// the existing task execution pipeline. §AR-rhei-panta.2 §AR-rhei-panta.3
#[derive(Debug)]
pub struct PantaProject {
    pub rhei: Rhei,
    /// Maps project-qualified task ID (`auth.1`) → the file path that defines it.
    pub task_sources: HashMap<String, PathBuf>,
    /// Maps project-qualified task ID (`auth.1`) → the owning rhei execution root. §AR-rhei-panta.5
    pub task_roots: HashMap<String, PathBuf>,
    /// Link-validation base directory for each merged content section, in
    /// `rhei.content_sections` order. §AR-rhei-panta.5
    pub content_section_roots: Vec<PathBuf>,
    /// Rhei ids in presentation order; `basin` is always last when present.
    pub rhei_ids: Vec<String>,
    /// State-machine name each rhei declared with its own `**States:**` line.
    /// Absent for rheis that declare nothing — they run the project default.
    /// The merge records ownership instead of discarding it, so every consumer
    /// can resolve a ticket's machine through its owning rhei.
    // §DA-per-rhei-state-machines: the machine is per-rhei, defaulted by the manifest.
    pub rhei_machines: HashMap<String, String>,
    /// Execution root of each rhei, keyed by rhei id — where a self-declared
    /// machine's `states.yaml` resolves first. §AR-rhei-panta.4
    pub rhei_roots: HashMap<String, PathBuf>,
    /// Rheis skipped by a lenient load, one message each. Always empty for the
    /// strict load, which fails on the first unloadable rhei instead.
    pub unloadable: Vec<String>,
}

/// Re-raise a parse error from a nested document, recording which file its
/// line belongs to.
///
/// The path travels as data rather than as a message prefix so diagnostics can
/// still open the file and render a code frame. Flattening it into the message
/// cost every nested error its line number and source excerpt — exactly the
/// errors a project author hits first, since `rhei init` puts their plans one
/// level down.
fn nested_parse_error(err: ParseError, path: &Path) -> ParseError {
    // An error that already names its origin keeps it: the innermost file is
    // the one holding the line.
    if err.file.is_some() {
        return err;
    }
    err.in_file(path)
}

/// Returns `true` if `path` is a directory workspace
/// (a directory containing `index.rhei.md`).
pub fn is_workspace(path: &Path) -> bool {
    path.is_dir() && path.join("index.rhei.md").is_file()
}

/// Returns `true` if `path` is a Panta project directory.
pub fn is_panta_project(path: &Path) -> bool {
    path.is_dir() && path.join(PANTA_INDEX_FILE).is_file()
}

/// Resolve a Panta project directory from either the project directory or its
/// `index.panta.md` manifest path. §FS-rhei-panta.6
pub fn panta_project_dir(path: &Path) -> Option<PathBuf> {
    if is_panta_project(path) {
        return Some(path.to_path_buf());
    }
    if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(PANTA_INDEX_FILE) {
        return path.parent().map(Path::to_path_buf);
    }
    None
}

/// Resolve the workspace directory for `path`, accepting either:
/// - a workspace directory (containing `index.rhei.md`), or
/// - the `index.rhei.md` file itself, when its parent directory contains
///   a `tasks/` subdirectory.
///
/// Callers that need the workspace root regardless of which form the user
/// supplied should prefer this over `is_workspace`.
pub fn workspace_dir(path: &Path) -> Option<PathBuf> {
    if is_workspace(path) {
        return Some(path.to_path_buf());
    }
    if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some("index.rhei.md") {
        if let Some(parent) = path.parent() {
            if parent.join("tasks").is_dir() {
                return Some(parent.to_path_buf());
            }
        }
    }
    None
}

/// Discover workspace task files recursively in deterministic plan order.
pub fn discover_task_files(tasks_dir: &Path) -> parser::Result<Vec<PathBuf>> {
    fn is_hidden(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with('.'))
    }

    fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> parser::Result<()> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| ParseError::new(format!("failed to read {}: {e}", dir.display()), None))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                ParseError::new(format!("failed to read {}: {e}", dir.display()), None)
            })?;
            let path = entry.path();
            if is_hidden(&path) {
                continue;
            }
            let file_type = entry.file_type().map_err(|e| {
                ParseError::new(format!("failed to inspect {}: {e}", path.display()), None)
            })?;
            if file_type.is_dir() {
                visit(&path, out)?;
            } else if file_type.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("md")
            {
                out.push(path);
            }
        }

        Ok(())
    }

    let mut files = Vec::new();
    if tasks_dir.is_dir() {
        visit(tasks_dir, &mut files)?;
    }
    files.sort_by(|a, b| {
        let a_key = a.strip_prefix(tasks_dir).unwrap_or(a).to_string_lossy().replace('\\', "/");
        let b_key = b.strip_prefix(tasks_dir).unwrap_or(b).to_string_lossy().replace('\\', "/");
        a_key.cmp(&b_key)
    });
    Ok(files)
}

/// Discover rhei entries among a Panta project directory's immediate children.
///
/// A rhei entry is either a direct-child `*.rhei.md` file (a Single-File Plan)
/// or a direct-child subdirectory that is a Directory Workspace (contains
/// `index.rhei.md`). Discovery does **not** descend into other subdirectories:
/// rheis live directly in the project directory, so a stray `*.rhei.md` buried
/// in, say, `docs/` is not silently promoted to a rhei. The `runtime/` artifact
/// tree and the reserved `basin/` directory are skipped here and handled
/// separately. Entries are returned in deterministic, `/`-normalized order.
pub fn discover_rhei_entries(project_dir: &Path) -> parser::Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    if !project_dir.is_dir() {
        return Ok(entries);
    }

    let read = std::fs::read_dir(project_dir).map_err(|e| {
        ParseError::new(format!("failed to read {}: {e}", project_dir.display()), None)
    })?;
    for entry in read {
        let entry = entry.map_err(|e| {
            ParseError::new(format!("failed to read {}: {e}", project_dir.display()), None)
        })?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type().map_err(|e| {
            ParseError::new(format!("failed to inspect {}: {e}", path.display()), None)
        })?;
        if file_type.is_dir() {
            // The `runtime/` artifact tree and the synthetic `basin/` are not
            // discoverable domain rheis; a non-workspace subdirectory is not a
            // rhei either, as rheis are not nested in grouping folders. §AR-rhei-panta.1
            if name == "runtime" || name == BASIN_RHEI_ID {
                continue;
            }
            if is_workspace(&path) {
                entries.push(path);
            }
        } else if file_type.is_file() && name.ends_with(".rhei.md") {
            entries.push(path);
        }
    }

    entries.sort_by(|a, b| {
        let a_key = a.strip_prefix(project_dir).unwrap_or(a).to_string_lossy().replace('\\', "/");
        let b_key = b.strip_prefix(project_dir).unwrap_or(b).to_string_lossy().replace('\\', "/");
        a_key.cmp(&b_key)
    });
    Ok(entries)
}

/// Load a Panta project, merging all contained rheis into one graph with
/// project-qualified task ids. §AR-rhei-panta.2 §AR-rhei-panta.3
pub fn load_panta_project(dir: &Path) -> parser::Result<PantaProject> {
    load_panta_project_with(dir, false)
}

/// Load a project, skipping rheis that fail to load instead of failing the
/// whole project, and recording why in [`PantaProject::unloadable`].
/// §FS-rhei-panta.6
pub fn load_panta_project_lenient(dir: &Path) -> parser::Result<PantaProject> {
    load_panta_project_with(dir, true)
}

fn load_panta_project_with(dir: &Path, lenient: bool) -> parser::Result<PantaProject> {
    let manifest_path = dir.join(PANTA_INDEX_FILE);
    let manifest_content = std::fs::read_to_string(&manifest_path).map_err(|e| {
        ParseError::new(format!("failed to read {}: {e}", manifest_path.display()), None)
    })?;
    let manifest = parser::parse_panta_manifest(&manifest_content)
        .map_err(|e| nested_parse_error(e, &manifest_path))?;

    let mut rheis = Vec::new();
    let mut unloadable: Vec<String> = Vec::new();
    let mut seen_ids: HashMap<String, PathBuf> = HashMap::new();
    let entries = discover_rhei_entries(dir)?;
    for entry in entries {
        // An unusable *id* — malformed, reserved, or already taken — keeps an
        // entry out of the project exactly as a parse failure does, so a
        // lenient load skips it the same way. §FS-rhei-panta.6
        let id = match rhei_id_for_entry(&entry) {
            Ok(id) => id,
            Err(err) if lenient => {
                unloadable.push(format!(
                    "{} could not be loaded: {}",
                    entry.display(),
                    err.message
                ));
                continue;
            }
            Err(err) => return Err(err),
        };
        let id_conflict = match validate_rhei_id(&id, &entry) {
            Ok(()) if id == BASIN_RHEI_ID => Some(basin_id_reserved_error(&entry)),
            Ok(()) => seen_ids.get(&id).map(|first| {
                ParseError::new(
                    format!(
                        "duplicate rhei id '{id}' in Panta project: derived from both {} and {}. \
                         Rename one of them — the id comes from the file stem or directory name",
                        first.display(),
                        entry.display()
                    ),
                    None,
                )
            }),
            Err(err) => Some(err),
        };
        if let Some(err) = id_conflict {
            if !lenient {
                return Err(err);
            }
            unloadable.push(format!("rhei '{id}' could not be loaded: {}", err.message));
            continue;
        }
        seen_ids.insert(id.clone(), entry.clone());
        let root = rhei_execution_root(&entry);
        // A rhei's own `**States:**` declaration is recorded, not policed:
        // the machine is a per-rhei property defaulted by the manifest.
        // §DA-per-rhei-state-machines §AR-rhei-panta.4
        let entry_result = load_rhei_entry(&entry);
        let loaded = match entry_result {
            Ok(loaded) => loaded,
            Err(err) if lenient => {
                seen_ids.remove(&id);
                let where_ = match err.line {
                    Some(line) => format!("{}:{line}", entry.display()),
                    None => entry.display().to_string(),
                };
                unloadable
                    .push(format!("rhei '{id}' could not be loaded ({where_}): {}", err.message));
                continue;
            }
            Err(err) => return Err(err),
        };
        rheis.push((id, loaded.rhei, loaded.task_sources, root));
    }

    let basin_dir = dir.join(BASIN_RHEI_ID);
    if basin_dir.is_dir() {
        if seen_ids.insert(BASIN_RHEI_ID.to_string(), basin_dir.clone()).is_some() {
            return Err(ParseError::new("duplicate synthetic basin rhei id", None));
        }
        let loaded = load_basin_rhei(&basin_dir, &manifest.structure, &manifest.states)?;
        rheis.push((BASIN_RHEI_ID.to_string(), loaded.rhei, loaded.task_sources, basin_dir));
    }

    let rhei_ids: Vec<String> = rheis.iter().map(|(id, _, _, _)| id.clone()).collect();
    let mut all_tasks = Vec::new();
    let mut task_sources = HashMap::new();
    let mut task_roots = HashMap::new();
    let mut rhei_machines: HashMap<String, String> = HashMap::new();
    let mut rhei_roots: HashMap<String, PathBuf> = HashMap::new();
    let mut merged_structure = manifest.structure.clone();
    let mut merged_metadata = manifest.metadata.clone();
    let mut content_sections = manifest.content_sections.clone();
    let mut content_section_roots = vec![dir.to_path_buf(); content_sections.len()];
    for (rhei_id, mut rhei, sources, root) in rheis {
        // Machine ownership survives the merge: a declared `**States:**` is
        // the rhei's own machine; silence means the project default. The
        // synthetic basin is built on the manifest machine and records no
        // declaration.

        // §DA-per-rhei-state-machines
        if rhei.states_declared && rhei_id != BASIN_RHEI_ID {
            rhei_machines.insert(rhei_id.clone(), rhei.states.trim().to_string());
        }
        rhei_roots.insert(rhei_id.clone(), root.clone());
        // Child runtime metadata joins the merged graph under qualified keys
        // so counted loops and poll timers resolve project-wide. §AR-rhei-panta.2
        merge_task_metadata(
            &mut merged_metadata,
            qualify_task_metadata(rhei.metadata.take(), &rhei_id),
        );
        merge_structure(&mut merged_structure, &rhei.structure);
        content_sections.push(ContentSection {
            title: format!("Rhei {rhei_id}: {}", rhei.title),
            content: String::new(),
            rhei: Some(rhei_id.clone()),
        });
        content_section_roots.push(root.clone());
        for section in &rhei.content_sections {
            content_sections.push(ContentSection {
                title: format!("Rhei {rhei_id} / {}", section.title),
                content: section.content.clone(),
                rhei: Some(rhei_id.clone()),
            });
            content_section_roots.push(root.clone());
        }
        let local_ids = collect_task_ids(&rhei.tasks);
        qualify_tasks(&mut rhei.tasks, &rhei_id, &local_ids);
        for task in &rhei.tasks {
            let source = source_for_task(&sources, task)?;
            collect_task_sources(task, source.as_path(), &mut task_sources)?;
            collect_task_roots(task, &root, &mut task_roots)?;
        }
        all_tasks.extend(rhei.tasks);
    }

    // An empty project — a manifest with no rheis yet — is a valid project;
    // `rhei init` creates exactly this state. §FS-rhei-panta.6
    Ok(PantaProject {
        rhei: Rhei {
            title: manifest.title,
            states: manifest.states,
            states_declared: manifest.states_declared,
            structure: merged_structure,
            metadata: merged_metadata,
            content_sections,
            tasks: all_tasks,
        },
        task_sources,
        task_roots,
        content_section_roots,
        rhei_ids,
        rhei_machines,
        rhei_roots,
        unloadable,
    })
}

fn rhei_execution_root(path: &Path) -> PathBuf {
    workspace_dir(path)
        .unwrap_or_else(|| path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf())
}

/// Load a bare rhei (single `.rhei.md` file or Directory Workspace) as the
/// single rhei of an implicit Panta: same graph shape as an explicit project,
/// no manifest, ids derived from the source location. §AR-rhei-panta.2
pub fn load_implicit_panta(path: &Path) -> parser::Result<PantaProject> {
    let entry = workspace_dir(path).unwrap_or_else(|| path.to_path_buf());
    let loaded = load_rhei_entry(&entry)?;
    wrap_rhei_as_implicit_panta(loaded, &entry)
}

/// Wrap a parsed single-file rhei as its implicit Panta. §AR-rhei-panta.2
pub fn implicit_panta_from_file_rhei(rhei: Rhei, file: &Path) -> parser::Result<PantaProject> {
    let mut task_sources = HashMap::new();
    for task in &rhei.tasks {
        collect_task_sources(task, file, &mut task_sources)?;
    }
    wrap_rhei_as_implicit_panta(Workspace { rhei, task_sources }, file)
}

/// Wrap an already-loaded bare rhei as its implicit Panta. §AR-rhei-panta.2:
/// the rhei is the sole level-1 child; §AR-rhei-panta.3: its id derives from
/// the file stem or directory name and project-qualifies every ticket.
pub fn wrap_rhei_as_implicit_panta(
    loaded: Workspace,
    entry: &Path,
) -> parser::Result<PantaProject> {
    let id = rhei_id_for_entry(entry)?;
    validate_rhei_id(&id, entry)?;
    if id == BASIN_RHEI_ID {
        return Err(basin_id_reserved_error(entry));
    }
    let root = rhei_execution_root(entry);
    let mut rhei = loaded.rhei;
    let rhei_ids = vec![id.clone()];
    let local_ids = collect_task_ids(&rhei.tasks);
    qualify_tasks(&mut rhei.tasks, &id, &local_ids);
    // On-disk frontmatter keys stay rhei-local; merged-graph reads resolve
    // through project-qualified keys. §AR-rhei-panta.2
    rhei.metadata = qualify_task_metadata(rhei.metadata.take(), &id);
    let mut task_sources = HashMap::new();
    let mut task_roots = HashMap::new();
    for task in &rhei.tasks {
        let source = source_for_task(&loaded.task_sources, task)?;
        collect_task_sources(task, source.as_path(), &mut task_sources)?;
        collect_task_roots(task, &root, &mut task_roots)?;
    }
    let rhei_roots = HashMap::from([(id.clone(), root.clone())]);
    let content_section_roots = vec![root; rhei.content_sections.len()];
    // The implicit Panta has no manifest: the single rhei's own `**States:**`
    // declaration is the project's effective machine, so it needs no per-rhei
    // entry. §AR-rhei-panta.2
    Ok(PantaProject {
        rhei,
        task_sources,
        task_roots,
        content_section_roots,
        rhei_ids,
        rhei_machines: HashMap::new(),
        rhei_roots,
        unloadable: Vec::new(),
    })
}

/// Runtime task metadata lives at `metadata.tasks` inside the frontmatter
/// root mapping. Returns the `tasks` mapping, if present.
fn frontmatter_tasks(metadata: &Metadata) -> Option<Metadata> {
    let section = metadata.get(serde_yaml::Value::String("metadata".to_string()))?;
    match section.get("tasks") {
        Some(serde_yaml::Value::Mapping(tasks)) => Some(tasks.clone()),
        _ => None,
    }
}

/// Store a `tasks` mapping back at `metadata.tasks` in the frontmatter root.
fn set_frontmatter_tasks(metadata: &mut Metadata, tasks: Metadata) {
    let metadata_key = serde_yaml::Value::String("metadata".to_string());
    let mut section = match metadata.get(&metadata_key).cloned() {
        Some(serde_yaml::Value::Mapping(section)) => section,
        _ => Metadata::new(),
    };
    section
        .insert(serde_yaml::Value::String("tasks".to_string()), serde_yaml::Value::Mapping(tasks));
    metadata.insert(metadata_key, serde_yaml::Value::Mapping(section));
}

/// Merge one rhei's qualified `metadata.tasks` metadata into the project metadata.
fn merge_task_metadata(project: &mut Option<Metadata>, child: Option<Metadata>) {
    let Some(child) = child else { return };
    let Some(child_tasks) = frontmatter_tasks(&child) else { return };
    if child_tasks.is_empty() {
        return;
    }
    let project = project.get_or_insert_with(Metadata::new);
    let mut merged = frontmatter_tasks(project).unwrap_or_default();
    for (key, value) in child_tasks {
        merged.insert(key, value);
    }
    set_frontmatter_tasks(project, merged);
}

/// Re-key a rhei's frontmatter `metadata.tasks` entries under project-qualified
/// ids so merged-graph reads resolve; write-back stays rhei-local. §AR-rhei-panta.2
fn qualify_task_metadata(metadata: Option<Metadata>, rhei_id: &str) -> Option<Metadata> {
    let mut metadata = metadata?;
    if let Some(tasks) = frontmatter_tasks(&metadata) {
        let mut qualified = Metadata::new();
        for (key, value) in tasks {
            let local = match &key {
                serde_yaml::Value::String(s) => s.clone(),
                serde_yaml::Value::Number(n) => n.to_string(),
                _ => {
                    qualified.insert(key, value);
                    continue;
                }
            };
            qualified.insert(serde_yaml::Value::String(format!("{rhei_id}.{local}")), value);
        }
        set_frontmatter_tasks(&mut metadata, qualified);
    }
    Some(metadata)
}

fn load_rhei_entry(path: &Path) -> parser::Result<Workspace> {
    if let Some(ws_dir) = workspace_dir(path) {
        load_workspace(&ws_dir)
    } else {
        let content = std::fs::read_to_string(path).map_err(|e| {
            ParseError::new(format!("failed to read {}: {e}", path.display()), None)
        })?;
        let rhei = parser::parse(&content).map_err(|e| nested_parse_error(e, path))?;
        let mut task_sources = HashMap::new();
        for task in &rhei.tasks {
            collect_task_sources(task, path, &mut task_sources)?;
        }
        Ok(Workspace { rhei, task_sources })
    }
}

fn load_basin_rhei(dir: &Path, structure: &Structure, states: &str) -> parser::Result<Workspace> {
    let mut tasks = Vec::new();
    let mut task_sources = HashMap::new();
    for path in discover_basin_task_files(dir)? {
        // The basin's manifest is synthetic, so an authored index can never
        // load. Skipping it silently vanished its tickets behind a green
        // validation — what the basin exists to prevent. §AR-rhei-panta.1
        if path.file_name().and_then(|name| name.to_str()) == Some("index.rhei.md") {
            return Err(ParseError::new(
                format!(
                    "{}: the basin has no authored index — its manifest is synthetic, so this \
                     file would never load. Move its tickets into task files under {} (any \
                     `*.md` file works), or rename the directory to make it an ordinary rhei \
                     with its own id.",
                    path.display(),
                    dir.display()
                ),
                None,
            ));
        }
        let content = std::fs::read_to_string(&path).map_err(|e| {
            ParseError::new(format!("failed to read {}: {e}", path.display()), None)
        })?;
        let parsed = parser::parse_workspace_tasks_with_structure(&content, structure)
            .map_err(|e| nested_parse_error(e, &path))?;
        for task in &parsed {
            collect_task_sources(task, &path, &mut task_sources)?;
        }
        tasks.extend(parsed);
    }
    Ok(Workspace {
        rhei: Rhei {
            title: "Basin".to_string(),
            states: states.to_string(),
            states_declared: false,
            structure: structure.clone(),
            metadata: None,
            content_sections: Vec::new(),
            tasks,
        },
        task_sources,
    })
}

fn discover_basin_task_files(dir: &Path) -> parser::Result<Vec<PathBuf>> {
    fn is_hidden(path: &Path) -> bool {
        path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with('.'))
    }

    fn visit(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> parser::Result<()> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| ParseError::new(format!("failed to read {}: {e}", dir.display()), None))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                ParseError::new(format!("failed to read {}: {e}", dir.display()), None)
            })?;
            let path = entry.path();
            if is_hidden(&path) {
                continue;
            }
            let file_type = entry.file_type().map_err(|e| {
                ParseError::new(format!("failed to inspect {}: {e}", path.display()), None)
            })?;
            if file_type.is_dir() {
                // Basin runtime artifacts are not authored basin task files. §FS-rhei-panta.2
                if path
                    .strip_prefix(root)
                    .ok()
                    .and_then(|relative| relative.components().next())
                    .is_some_and(|component| component.as_os_str() == "runtime")
                {
                    continue;
                }
                visit(root, &path, out)?;
            } else if file_type.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("md")
            {
                out.push(path);
            }
        }

        Ok(())
    }

    let mut files = Vec::new();
    if dir.is_dir() {
        visit(dir, dir, &mut files)?;
    }
    files.sort_by(|a, b| {
        let a_key = a.strip_prefix(dir).unwrap_or(a).to_string_lossy().replace('\\', "/");
        let b_key = b.strip_prefix(dir).unwrap_or(b).to_string_lossy().replace('\\', "/");
        a_key.cmp(&b_key)
    });
    Ok(files)
}

/// The `basin` id belongs to the synthetic catch-all rhei that a Panta
/// project's `basin/` directory feeds; a user rhei may not claim it.
/// §FS-rhei-panta.4
fn basin_id_reserved_error(entry: &Path) -> ParseError {
    ParseError::new(
        format!(
            "`{BASIN_RHEI_ID}` is reserved: a Panta project's `{BASIN_RHEI_ID}/` directory \
             feeds the synthetic catch-all rhei, so {} cannot use that id. Rename the file \
             or directory to any other id",
            entry.display()
        ),
        None,
    )
}

/// Resolve `path` as a rhei entry inside a Panta project, returning the project
/// directory and the entry's rhei id.
///
/// A rhei that belongs to a project cannot be understood without it: its
/// `**Prior:**` may point across rheis and its state machine comes from the
/// manifest. Commands therefore load the project and narrow to this id, rather
/// than loading the file alone.
// §FS-rhei-panta.6: pointing at a member rhei is `--rhei <id>` on its project.
pub fn panta_member(path: &Path) -> Option<(PathBuf, String)> {
    // `index.rhei.md` inside a workspace stands for the workspace directory.
    let entry = workspace_dir(path).unwrap_or_else(|| path.to_path_buf());
    // A bare `billing.rhei.md` has no parent component, so resolve against the
    // invocation directory before asking what encloses it.
    let absolute = if entry.is_absolute() {
        entry.clone()
    } else {
        std::env::current_dir().ok()?.join(&entry)
    };
    let parent = absolute.parent()?;
    if !is_panta_project(parent) {
        return None;
    }
    let name = absolute.file_name()?;
    if absolute.is_dir() && name == BASIN_RHEI_ID {
        return Some((parent.to_path_buf(), BASIN_RHEI_ID.to_string()));
    }
    // Only a real rhei entry qualifies: a stray `notes.md` beside the manifest
    // is not one, and neither is the `runtime/` tree.
    let entries = discover_rhei_entries(parent).ok()?;
    if !entries.iter().any(|candidate| candidate.file_name() == Some(name)) {
        return None;
    }
    let id = rhei_id_for_entry(&absolute).ok()?;
    Some((parent.to_path_buf(), id))
}

fn rhei_id_for_entry(path: &Path) -> parser::Result<String> {
    if path.is_dir() {
        return path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| ParseError::new(format!("invalid rhei path {}", path.display()), None));
    }

    let name = path.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
        ParseError::new(format!("invalid rhei filename {}", path.display()), None)
    })?;
    // §AR-rhei-panta.3: the rhei id is the file stem, so a single-file rhei
    // must carry the `.rhei.md` suffix for its id to exist.
    let Some(stem) = name.strip_suffix(".rhei.md") else {
        return Err(ParseError::new(
            format!(
                "'{}' is not a rhei plan file: a single-file rhei must be named `<id>.rhei.md`, \
                 because the id every ticket is prefixed with comes from the file stem",
                path.display()
            ),
            None,
        ));
    };
    Ok(stem.to_string())
}

fn validate_rhei_id(id: &str, path: &Path) -> parser::Result<()> {
    let valid = id.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if valid {
        return Ok(());
    }
    let rename_hint = match suggest_rhei_id(id) {
        Some(suggestion) if path.is_dir() => {
            format!(" Rename the directory to `{suggestion}/` to fix the id.")
        }
        Some(suggestion) => {
            format!(" Rename the file to `{suggestion}.rhei.md` to fix the id.")
        }
        None => String::new(),
    };
    Err(ParseError::new(
        format!(
            "rhei id '{id}' derived from {} is not valid: a rhei id must start with a \
             letter and contain only letters, digits, `_`, or `-`, because it prefixes \
             every ticket id in the project.{rename_hint}",
            path.display()
        ),
        None,
    ))
}

/// Best-effort legal rhei id for a rename suggestion: invalid characters
/// become `-`, and anything before the first letter is dropped.
fn suggest_rhei_id(id: &str) -> Option<String> {
    let mut out = String::new();
    for ch in id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String =
        out.trim_matches('-').chars().skip_while(|ch| !ch.is_ascii_alphabetic()).collect();
    (!out.is_empty()).then_some(out)
}

fn merge_structure(into: &mut Structure, from: &Structure) {
    // Keep authored max-level constraints while merging per-rhei node kinds. §FS-rhei-states.9.3
    into.max_levels = into.max_levels.max(from.max_levels);
    for kind in &from.node_kinds {
        if !into.node_kinds.iter().any(|existing| existing.eq_ignore_ascii_case(kind)) {
            into.node_kinds.push(kind.clone());
        }
    }
}

fn collect_task_ids(tasks: &[Task]) -> HashSet<TaskId> {
    fn visit(task: &Task, out: &mut HashSet<TaskId>) {
        out.insert(task.id.clone());
        for child in &task.children {
            visit(child, out);
        }
    }

    let mut ids = HashSet::new();
    for task in tasks {
        visit(task, &mut ids);
    }
    ids
}

fn qualify_tasks(tasks: &mut [Task], rhei_id: &str, local_ids: &HashSet<TaskId>) {
    for task in tasks {
        qualify_task(task, rhei_id, local_ids);
    }
}

fn qualify_task(task: &mut Task, rhei_id: &str, local_ids: &HashSet<TaskId>) {
    task.id = qualify_local_id(&task.id, rhei_id);
    task.profile_depth_offset = task.profile_depth_offset.saturating_add(1);
    for prior in &mut task.prior {
        // A dotted `<name>.<rest>` naming no local ticket is a cross-rhei
        // reference, kept as authored so a dangling one is never reported under
        // an id nobody wrote. §AR-rhei-panta.3
        if local_ids.contains(prior) || !is_cross_rhei_reference(prior) {
            *prior = qualify_local_id(prior, rhei_id);
        }
    }
    for child in &mut task.children {
        qualify_task(child, rhei_id, local_ids);
    }
}

fn qualify_local_id(id: &TaskId, rhei_id: &str) -> TaskId {
    let mut segments = Vec::with_capacity(id.segments.len() + 1);
    segments.push(TaskIdSegment::Named(rhei_id.to_string()));
    segments.extend(id.segments.clone());
    TaskId::from_segments(segments)
}

/// Whether `id` has the shape of a reference into another rhei: a dotted id
/// whose leading segment is a name rather than a number.
///
/// The leading segment is *not* checked against the project's rhei ids. A typo'd
/// rhei name is shaped like a cross-rhei reference and reads like one to the
/// author, so it stays as written and validation explains it. Prefixing it with
/// the citing rhei instead would report an id that appears in no file.
// §AR-rhei-panta.3: a dotted, name-led prior is kept as authored.
fn is_cross_rhei_reference(id: &TaskId) -> bool {
    id.segments.len() > 1 && matches!(id.segments.first(), Some(TaskIdSegment::Named(_)))
}

fn source_for_task(sources: &HashMap<String, PathBuf>, task: &Task) -> parser::Result<PathBuf> {
    let local = TaskId::from_segments(task.id.segments.iter().skip(1).cloned().collect());
    sources.get(&local.to_string()).cloned().ok_or_else(|| {
        // The rhei loader always records a source for every task it parses, so a
        // miss here means an internal qualification/source-map inconsistency, not
        // a user error. Fail loudly instead of pointing rewrites at an empty path.
        ParseError::new(
            format!(
                "internal: no source file recorded for task '{}' (rhei-local id '{}')",
                task.id, local
            ),
            None,
        )
    })
}

fn collect_task_roots(
    task: &Task,
    root: &Path,
    task_roots: &mut HashMap<String, PathBuf>,
) -> parser::Result<()> {
    let id = task.id.to_string();
    task_roots.insert(id, root.to_path_buf());
    for child in &task.children {
        collect_task_roots(child, root, task_roots)?;
    }
    Ok(())
}

/// Load a directory workspace, merging all task files into a single plan.
///
/// Reads `index.rhei.md` for plan metadata, then discovers and parses every
/// `.md` file inside the `tasks/` subdirectory. Reports duplicate task IDs
/// across files and missing structure.
pub fn load_workspace(dir: &Path) -> parser::Result<Workspace> {
    let index_path = dir.join("index.rhei.md");
    let index_content = std::fs::read_to_string(&index_path).map_err(|e| {
        ParseError::new(format!("failed to read {}: {e}", index_path.display()), None)
    })?;

    let index = parser::parse_workspace_index(&index_content)
        .map_err(|e| nested_parse_error(e, &index_path))?;

    let tasks_dir = dir.join("tasks");
    let mut all_tasks: Vec<Task> = Vec::new();
    let mut task_sources: HashMap<String, PathBuf> = HashMap::new();

    if tasks_dir.is_dir() {
        for path in discover_task_files(&tasks_dir)? {
            let content = std::fs::read_to_string(&path).map_err(|e| {
                ParseError::new(format!("failed to read {}: {e}", path.display()), None)
            })?;

            let tasks = parser::parse_workspace_tasks_with_structure(&content, &index.structure)
                .map_err(|e| nested_parse_error(e, &path))?;

            for task in &tasks {
                collect_task_sources(task, &path, &mut task_sources)?;
            }

            all_tasks.extend(tasks);
        }
    }

    // No task files is a valid, empty rhei. Failing here let one freshly
    // created directory break loading for every sibling rhei; `rhei validate`
    // warns instead. §FS-rhei-plan-language.1.2

    Ok(Workspace {
        rhei: Rhei {
            title: index.title,
            states: index.states,
            states_declared: index.states_declared,
            structure: index.structure,
            metadata: index.metadata,
            content_sections: index.content_sections,
            tasks: all_tasks,
        },
        task_sources,
    })
}

/// Line of the *last* heading in `path` declaring ticket `id_str`.
///
/// That is the redeclaration in both shapes this serves: the second of two
/// headings when a single file repeats an id, and the colliding heading in the
/// newly loaded file when two files share one. Runs only on the error path, so
/// re-reading the file costs nothing in the common case; a file that no longer
/// reads simply yields no line rather than masking the duplicate itself.
fn duplicate_heading_line(path: &Path, id_str: &str) -> Option<usize> {
    let source = std::fs::read_to_string(path).ok()?;
    let mut found = None;
    for (index, line) in source.lines().enumerate() {
        let rest = line.trim_start_matches('#');
        if rest.len() == line.len() {
            continue;
        }
        // `<kind> <id>:` — the id sits between the kind keyword and the colon.
        let Some((head, _)) = rest.split_once(':') else {
            continue;
        };
        if head.split_whitespace().nth(1) == Some(id_str) {
            found = Some(index + 1);
        }
    }
    found
}

fn collect_task_sources(
    task: &Task,
    path: &Path,
    task_sources: &mut HashMap<String, PathBuf>,
) -> parser::Result<()> {
    let id_str = task.id.to_string();
    if let Some(existing) = task_sources.get(&id_str) {
        // Two tickets in one file read as "defined in both X and X" if the
        // paths are printed unconditionally; say which case it is, and point
        // at the offending heading so the fix is a jump away.
        let message = if existing == path {
            format!("duplicate task ID '{}': declared twice in {}", id_str, path.display())
        } else {
            format!(
                "duplicate task ID '{}': defined in both {} and {}",
                id_str,
                existing.display(),
                path.display()
            )
        };
        return Err(ParseError::new(message, duplicate_heading_line(path, &id_str)).in_file(path));
    }
    task_sources.insert(id_str, path.to_path_buf());

    for child in &task.children {
        collect_task_sources(child, path, task_sources)?;
    }

    Ok(())
}
