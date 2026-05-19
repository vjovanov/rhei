fn reset_target_files(loaded: &LoadedPlan, input: &Path) -> Vec<PathBuf> {
    if loaded.task_sources.is_empty() {
        return vec![input.to_path_buf()];
    }

    let mut files = loaded.task_sources.values().cloned().collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files
}

fn reset_plan_file_states(path: &Path, machine: &rhei_validator::StateMachine) -> MietteResult<()> {
    let file = fs::File::open(path)
        .map_err(|err| file_io_report(path, "failed to open plan file", err))?;
    file.lock_exclusive()
        .map_err(|err| file_io_report(path, "failed to acquire file lock", err))?;

    let raw = fs::read_to_string(path)
        .map_err(|err| file_io_report(path, "failed to read plan file", err))?;
    let new_raw = rewrite_all_states_to_initial(&raw, machine)?;
    let new_raw = strip_result_links(&new_raw);
    let new_raw = match rhei_core::parse(&new_raw) {
        Ok(rhei) => {
            if let Some(metadata) = clear_runtime_state_visits(rhei.metadata.as_ref()) {
                rewrite_frontmatter(&new_raw, &metadata)?
            } else {
                new_raw
            }
        }
        Err(_) => new_raw,
    };

    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    tmp.write_all(new_raw.as_bytes()).map_err(|err| miette!("failed to write temp file: {err}"))?;
    tmp.persist(path).map_err(|err| miette!("failed to persist temp file: {err}"))?;

    let _ = file.unlock();
    Ok(())
}

fn clear_runtime_metadata_in_file(path: &Path, workspace_index: bool) -> MietteResult<()> {
    let file = fs::File::open(path)
        .map_err(|err| file_io_report(path, "failed to open plan file", err))?;
    file.lock_exclusive()
        .map_err(|err| file_io_report(path, "failed to acquire file lock", err))?;

    let raw = fs::read_to_string(path)
        .map_err(|err| file_io_report(path, "failed to read plan file", err))?;
    let metadata = if workspace_index {
        rhei_core::parser::parse_workspace_index(&raw)
            .map_err(|err| {
                miette!("failed to parse workspace index for metadata reset: {}", err.message)
            })?
            .metadata
    } else {
        rhei_core::parse(&raw)
            .map_err(|err| miette!("failed to parse plan for metadata reset: {}", err.message))?
            .metadata
    };

    let new_raw = if let Some(metadata) = clear_runtime_state_visits(metadata.as_ref()) {
        rewrite_frontmatter(&raw, &metadata)?
    } else {
        raw
    };

    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    tmp.write_all(new_raw.as_bytes()).map_err(|err| miette!("failed to write temp file: {err}"))?;
    tmp.persist(path).map_err(|err| miette!("failed to persist temp file: {err}"))?;

    let _ = file.unlock();
    Ok(())
}

/// Remove `> **Result:** …` lines (and a single leading blank line when
/// present) inserted by `rhei complete`. Used during `rhei reset` so the
/// plan returns to a clean authored state.
fn strip_result_links(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());

    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("> **Result:**") {
            // Drop a single trailing blank line accumulated before the result
            // link so we don't leave a pair of blank lines behind.
            if matches!(result.last(), Some(last) if last.trim().is_empty()) {
                result.pop();
            }
            continue;
        }
        result.push((*line).to_string());
    }

    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn rewrite_all_states_to_initial(
    raw: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result = Vec::with_capacity(lines.len());
    let mut expecting_state: Option<String> = None;
    let mut rewrites = 0usize;

    let task_heading_re = regex::Regex::new(
        r#"^(#{3,6})\s+([A-Za-z][A-Za-z0-9_-]*)\s+[A-Za-z0-9][A-Za-z0-9_.\-]*:\s+"#,
    )
    .expect("task heading regex compiles");

    for line in &lines {
        if let Some(captures) = task_heading_re.captures(line) {
            if expecting_state.is_some() {
                return Err(miette!("could not find **State:** line before the next task header"));
            }
            let heading = captures.get(1).expect("heading capture").as_str();
            let kind = captures.get(2).expect("kind capture").as_str().to_ascii_lowercase();
            let level = heading.len().saturating_sub(2) as u8;
            expecting_state = Some(initial_state_for_node(machine, &kind, level)?);
            result.push((*line).to_string());
            continue;
        }

        if let Some(initial_state) = expecting_state.as_deref() {
            if !line.starts_with("**State:**") {
                result.push((*line).to_string());
                continue;
            }
            let formatted = format!("**State:** {}", format_state_metadata_value(initial_state));
            result.push(formatted);
            expecting_state = None;
            rewrites += 1;
            continue;
        }

        result.push((*line).to_string());
    }

    if expecting_state.is_some() {
        return Err(miette!("could not find **State:** line at the end of the plan"));
    }
    if rewrites == 0 {
        return Err(miette!("found no task state metadata to reset"));
    }

    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// Find a terminal (non-cancelled) state reachable in one transition.
///
/// Prefers exact `from` matches over wildcards. Cancellation is not considered
/// a completion target for `rhei complete`.
fn find_completion_state(
    current_state: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<String> {
    // Exact from-state matches first.
    for rule in machine.transitions() {
        if rule.from.0 == current_state {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if is_terminal && rule.to.0 != "cancelled" {
                return Some(rule.to.0.clone());
            }
        }
    }

    // Fall back to wildcard transitions.
    for rule in machine.transitions() {
        if rule.from.0 == "*" {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if is_terminal && rule.to.0 != "cancelled" {
                return Some(rule.to.0.clone());
            }
        }
    }

    None
}

fn non_terminal_descendants(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> Vec<String> {
    fn recurse(
        task: &rhei_core::ast::Task,
        machine: &rhei_validator::StateMachine,
        out: &mut Vec<String>,
    ) {
        for child in &task.children {
            if !is_terminal_state(child.state.as_str(), machine) {
                out.push(format!(
                    "{} {} ('{}') [{}]",
                    title_case_kind(&child.kind),
                    child.id,
                    child.title,
                    child.state
                ));
            }
            recurse(child, machine, out);
        }
    }
    let mut out = Vec::new();
    recurse(task, machine, &mut out);
    out
}

fn title_case_kind(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len());
    let mut chars = kind.chars();
    if let Some(first) = chars.next() {
        for c in first.to_uppercase() {
            out.push(c);
        }
    }
    for c in chars {
        out.push(c);
    }
    out
}

/// Resolve the workspace root for result file placement.
fn result_workspace_root(input: &Path, task_file: &Path) -> PathBuf {
    if workspace::is_workspace(input) {
        input.to_path_buf()
    } else {
        task_file.parent().unwrap_or(Path::new(".")).to_path_buf()
    }
}

/// Append a state-transition entry to `runtime/results/<task-id>.md`.
///
/// Each entry is a markdown heading (`## from → to`) optionally followed by
/// a message body.  The file is created (with directories) on the first call.
fn append_result_entry(
    workspace_root: &Path,
    task_id: &str,
    from: &str,
    to: &str,
    message: Option<&str>,
) -> MietteResult<()> {
    let results_dir = workspace_root.join("runtime").join("results");
    fs::create_dir_all(&results_dir)
        .map_err(|err| miette!("failed to create runtime/results directory: {err}"))?;
    let result_file = results_dir.join(format!("{}.md", task_id));

    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&result_file)
        .map_err(|err| miette!("failed to open result file: {err}"))?;

    writeln!(file, "## {} \u{2192} {}", from, to)
        .map_err(|err| miette!("failed to write result entry: {err}"))?;
    if let Some(msg) = message {
        writeln!(file).map_err(|err| miette!("failed to write result entry: {err}"))?;
        writeln!(file, "{}", msg).map_err(|err| miette!("failed to write result entry: {err}"))?;
    }
    writeln!(file).map_err(|err| miette!("failed to write result entry: {err}"))?;

    Ok(())
}

/// Write `**Assignee:** <value>` into the given task's metadata block on disk.
///
/// The rewrite is atomic (temp file + rename) and holds an exclusive lock on
/// the file for the duration of the operation. No-op if the task already has
/// an assignee line.
fn write_task_assignee(task_file: &Path, task_id: &str, assignee: &str) -> MietteResult<()> {
    let handle = fs::File::open(task_file)
        .map_err(|err| file_io_report(task_file, "failed to open plan file", err))?;
    handle
        .lock_exclusive()
        .map_err(|err| file_io_report(task_file, "failed to acquire file lock", err))?;

    let raw = fs::read_to_string(task_file)
        .map_err(|err| file_io_report(task_file, "failed to read plan file", err))?;
    let rewritten = insert_task_assignee(&raw, task_id, assignee)?;

    let parent = task_file.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    tmp.write_all(rewritten.as_bytes())
        .map_err(|err| miette!("failed to write temp file: {err}"))?;
    tmp.persist(task_file).map_err(|err| miette!("failed to persist temp file: {err}"))?;

    let _ = handle.unlock();
    Ok(())
}

/// Rewrite a task's markdown after completion: remove `**Assignee:**` and,
/// when `insert_link` is true, append a `> **Result:** [link_text](link_path)`
/// line to the task body.
///
/// Operates on raw text lines so the parser does not need to know about
/// assignee or result fields.
fn rewrite_task_completion(
    task_file: &Path,
    task_id: &str,
    link_text: &str,
    link_path: &str,
    insert_link: bool,
) -> MietteResult<()> {
    let raw = fs::read_to_string(task_file)
        .map_err(|err| file_io_report(task_file, "failed to read plan file", err))?;

    let lines: Vec<&str> = raw.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len() + 2);
    let task_prefix = format!("### Task {}:", task_id);

    let mut in_target_task = false;
    let mut link_inserted = !insert_link; // skip insertion when not requested
    let result_line = format!("> **Result:** [{}]({})", link_text, link_path);

    // Any H4..=H6 heading marks a child/descendant node under the current
    // root task, regardless of node kind. Insert the result link just before
    // the first descendant heading we encounter while still inside the target.
    let descendant_heading_re = regex::Regex::new(r#"^#{4,6}\s+"#).expect("regex");

    for line in &lines {
        let is_new_task = line.starts_with("### Task ") && !line.starts_with(&task_prefix);
        let is_descendant = descendant_heading_re.is_match(line);

        if in_target_task && !link_inserted && (is_new_task || is_descendant) {
            result_lines.push(String::new());
            result_lines.push(result_line.clone());
            link_inserted = true;
        }

        if line.starts_with("### Task ") {
            in_target_task = line.starts_with(&task_prefix);
        }

        // Strip the assignee line from the target task.
        if in_target_task && line.starts_with("**Assignee:**") {
            continue;
        }

        result_lines.push(line.to_string());
    }

    // If the target task is the last element in the file, append here.
    if in_target_task && !link_inserted {
        result_lines.push(String::new());
        result_lines.push(result_line);
    }

    let mut output = result_lines.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }

    // Atomic write.
    let parent = task_file.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!("failed to create temp file: {err}"))?;
    tmp.write_all(output.as_bytes()).map_err(|err| miette!("failed to write temp file: {err}"))?;
    tmp.persist(task_file).map_err(|err| miette!("failed to persist temp file: {err}"))?;

    Ok(())
}

/// Get the instructions text for a given state from the state machine.
fn state_instructions(machine: &rhei_validator::StateMachine, state: &str) -> String {
    machine
        .states
        .get(state)
        .and_then(|def| def.instructions.as_deref())
        .unwrap_or("")
        .trim()
        .to_string()
}
