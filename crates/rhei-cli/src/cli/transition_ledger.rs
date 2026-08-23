// The record one applied transition leaves behind: the central
// `runtime/state-transitions.log` line every move appends, and the terminal
// result a `final: true` entry finalizes.
//
// Its own part because recording a move is what happens *after* the rewrite
// next door decided it, and every verb that moves a ticket goes through it.

// §AR-source-file-size.3 §FS-rhei-complete.3 §FS-rhei-viz.4

/// Append a state-transition entry to the central transition ledger and, when a
/// completion message is present, to `runtime/results/<task-id>.md`.
///
/// State history is centralized in `runtime/state-transitions.log`. Result
/// files are task-specific completion artifacts, not the state-history source.
fn append_result_entry(
    workspace_root: &Path,
    task_id: &str,
    from: &str,
    to: &str,
    message: Option<&str>,
) -> MietteResult<()> {
    append_state_transition_log_entry(workspace_root, task_id, from, to)?;

    let Some(msg) = message else {
        return Ok(());
    };

    let results_dir = workspace_root.join("runtime").join("results");
    fs::create_dir_all(&results_dir)
        .map_err(|err| miette!(
            help = runtime_dir_help(),
            "failed to create runtime/results directory: {err}"
        ))?;
    let result_file = results_dir.join(format!("{}.md", task_id));

    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&result_file)
        .map_err(|err| miette!(
            help = runtime_results_help(),
            "failed to open result file: {err}"
        ))?;

    writeln!(file, "## Result")
        .map_err(|err| miette!(
            help = runtime_results_help(),
            "failed to write result entry: {err}"
        ))?;
    writeln!(file).map_err(|err| miette!(
        help = runtime_results_help(),
        "failed to write result entry: {err}"
    ))?;
    writeln!(file, "{}", msg).map_err(|err| miette!(
        help = runtime_results_help(),
        "failed to write result entry: {err}"
    ))?;
    writeln!(file).map_err(|err| miette!(
        help = runtime_results_help(),
        "failed to write result entry: {err}"
    ))?;

    Ok(())
}

/// Append one timestamp-free `<task-id> <source>@<destination>` transition line.
/// §FS-rhei-viz.4 §FS-rhei-run.3
fn append_state_transition_log_entry(
    workspace_root: &Path,
    task_id: &str,
    from: &str,
    to: &str,
) -> MietteResult<()> {
    let runtime_dir = workspace_root.join("runtime");
    fs::create_dir_all(&runtime_dir)
        .map_err(|err| miette!(
            help = runtime_dir_help(),
            "failed to create runtime directory: {err}"
        ))?;
    let transitions_file = runtime_dir.join("state-transitions.log");

    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&transitions_file)
        .map_err(|err| miette!(
            help = transition_log_help(),
            "failed to open state transition log: {err}"
        ))?;

    writeln!(file, "{} {}@{}", task_id, from, to)
        .map_err(|err| miette!(
            help = transition_log_help(),
            "failed to write state transition log entry: {err}"
        ))?;

    Ok(())
}

/// Record one applied transition: history for every move, plus the terminal
/// result finalization when the destination is `final: true`.
///
/// Finalization used to be `rhei complete`'s own epilogue. It is a property of
/// entering a terminal state, so it lives on the shared transition path and is
/// the only implementation: cancellation, failure, timeout, a callback
/// redirect, and a successful completion leave the same artifacts behind, and
/// no caller can apply a transition and skip them.
// §FS-rhei-complete.3: every terminal path writes the result artifacts.
#[allow(clippy::too_many_arguments)]
fn record_transition_result(
    artifact_root: &Path,
    task_file: &Path,
    local_id: &str,
    machine: &rhei_validator::StateMachine,
    task_id: &str,
    from: &str,
    to: &str,
    message: Option<&str>,
) -> MietteResult<()> {
    append_result_entry(artifact_root, task_id, from, to, message)?;
    if is_terminal_state(to, machine) {
        // A message already created the file; a terminal move satisfied by an
        // existing result must not link a path that does not exist.
        ensure_result_file(artifact_root, task_id)?;
        let result_link = format!("runtime/results/{}.md", task_id);
        rewrite_task_completion(task_file, local_id, task_id, &result_link, true)?;
    }
    Ok(())
}

/// Create an empty `runtime/results/<task-id>.md` when the task has none yet.
fn ensure_result_file(workspace_root: &Path, task_id: &str) -> MietteResult<()> {
    let results_dir = workspace_root.join("runtime").join("results");
    fs::create_dir_all(&results_dir)
        .map_err(|err| {
            miette!(help = runtime_results_help(), "failed to create runtime/results directory: {err}")
        })?;
    let result_file = results_dir.join(format!("{}.md", task_id));
    if result_file.exists() {
        return Ok(());
    }
    fs::write(&result_file, "")
        .map_err(|err| file_io_report(&result_file, "failed to create result file", err))
}

/// Write `**Assignee:** <value>` into the given task's metadata block on disk.
///
/// The rewrite is atomic (temp file + rename) and holds an exclusive lock on
/// the file for the duration of the operation. While locked, it re-checks the
/// task state and existing assignee so a stale claim cannot overwrite another
/// worker's claim.
// §FS-rhei-next.3.1: Re-check claimability under the file lock before claiming.
struct TaskAssigneeClaimContext<'a> {
    workspace_root: &'a Path,
    metadata: Option<&'a Metadata>,
    state_def: &'a rhei_validator::StateDef,
    settings: &'a RheiSettings,
}

fn write_task_assignee(
    task_file: &Path,
    task_id: &str,
    qualified_id: &str,
    expected_state: &str,
    machine: &rhei_validator::StateMachine,
    claim: TaskAssigneeClaimContext<'_>,
    assignee: &str,
) -> MietteResult<()> {
    let handle = fs::File::open(task_file)
        .map_err(|err| file_io_report(task_file, "failed to open plan file", err))?;
    handle
        .lock_exclusive()
        .map_err(|err| file_io_report(task_file, "failed to acquire file lock", err))?;

    let raw = fs::read_to_string(task_file)
        .map_err(|err| file_io_report(task_file, "failed to read plan file", err))?;
    let target = parse_task_id(task_id);
    let task = parse_claim_task_from_raw(&raw, task_file, &target, task_id)?;
    let current_state = normalized_state_name(task.state.as_str(), machine);
    if current_state != expected_state {
        let _ = fs2::FileExt::unlock(&handle);
        return Err(miette!(
            help = task_moved_help(),
            "conflict: Task {} is in state '{}', expected '{}'",
            qualified_id,
            task.state,
            expected_state
        ));
    }
    if let Some(existing) = task.assignee.as_deref() {
        let _ = fs2::FileExt::unlock(&handle);
        // §FS-rhei-release.1: hand-editing the plan is not the remedy.
        return Err(miette!(
            help = format!(
                "release it with: rhei release {qualified_id} — or work on a different task."
            ),
            "Task {} is already assigned to {}", qualified_id, existing
        ));
    }
    // §AR-rhei-panta.2: `{task_id}` artifact templates render the qualified
    // id — the same paths transition-time checks and agents see.
    ensure_state_inputs_exist_for_transition(
        claim.workspace_root,
        Some(&task),
        qualified_id,
        &current_state,
        claim.state_def,
        // `claim.metadata` is the merged project graph's, so `stateVisits`
        // is keyed by the qualified id — not the rhei-local id the raw file
        // parse yields. §AR-rhei-panta.2
        Some(render_visit_count(
            claim.metadata,
            &parse_task_id(qualified_id),
            &current_state,
            task.state.as_str(),
            machine,
        )),
        machine,
        claim.settings,
        &format!("Task {} cannot be claimed in state {}.", qualified_id, current_state),
    )?;

    let rewritten = insert_task_assignee(&raw, task_id, assignee)?;

    let parent = task_file.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!(
            help = temp_write_help(),
            "failed to create temp file: {err}"
        ))?;
    tmp.write_all(rewritten.as_bytes())
        .map_err(|err| miette!(
            help = temp_write_help(),
            "failed to write temp file: {err}"
        ))?;
    tmp.persist(task_file).map_err(|err| miette!(
        help = temp_write_help(),
        "failed to persist temp file: {err}"
    ))?;

    let _ = fs2::FileExt::unlock(&handle);
    Ok(())
}

fn parse_claim_task_from_raw(
    raw: &str,
    task_file: &Path,
    target: &TaskId,
    task_id: &str,
) -> MietteResult<rhei_core::ast::Task> {
    if let Ok(rhei) = rhei_core::parse(raw) {
        if let Some(task) = find_task_by_id(&rhei.tasks, target) {
            return Ok(task.clone());
        }
    }

    if let Ok(tasks) = rhei_core::parser::parse_workspace_tasks(raw) {
        if let Some(task) = find_task_by_id(&tasks, target) {
            return Ok(task.clone());
        }
    }

    Err(miette!(
        help = task_id_help(),
        "task '{}' not found in {}", task_id, task_file.display()
    ))
}

/// Rewrite a task's markdown after completion: remove `**Assignee:**` and,
/// Drop blank lines from the end of `lines` so the caller controls the exact
/// separation it wants.
fn trim_trailing_blank_lines(lines: &mut Vec<String>) {
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
}

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

    let mut in_target_task = false;
    let mut target_found = false;
    let mut link_inserted = !insert_link; // skip insertion when not requested
    let result_line = format!("> **Result:** [{}]({})", link_text, link_path);
    let mut in_code_block = false;

    for line in &lines {
        let heading = node_heading_outside_code(line, &mut in_code_block);
        if in_target_task && !link_inserted && heading.is_some() {
            // Exactly one blank line on each side: the task body already ends
            // with the blank that separates it from the next heading, so
            // pushing another produced a double blank above the result block
            // and left the following heading butted against it.
            trim_trailing_blank_lines(&mut result_lines);
            result_lines.push(String::new());
            result_lines.push(result_line.clone());
            result_lines.push(String::new());
            link_inserted = true;
        }

        if let Some((_, id)) = heading {
            in_target_task = id == task_id;
            target_found |= in_target_task;
        }

        // Strip the assignee line from the target task.
        if !in_code_block && in_target_task && line.starts_with("**Assignee:**") {
            continue;
        }
        if !in_code_block && in_target_task && line.starts_with("> **Result:**") {
            // §FS-rhei-panta.6.3: completion owns this ticket's result link.
            // An existing (possibly legacy rhei-local) link is refreshed to
            // the file this completion actually wrote.
            if !link_inserted {
                result_lines.push(result_line.clone());
                link_inserted = true;
                continue;
            }
            link_inserted = true;
        }

        result_lines.push(line.to_string());
    }

    // If the target task is the last element in the file, append here. No
    // trailing blank: the final newline is restored from the source below.
    if in_target_task && !link_inserted {
        trim_trailing_blank_lines(&mut result_lines);
        result_lines.push(String::new());
        result_lines.push(result_line);
    }
    if !target_found {
        return Err(miette!(
            help = task_id_help(),
            "task '{}' not found in {}", task_id, task_file.display()
        ));
    }

    let mut output = result_lines.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }

    // Atomic write.
    let parent = task_file.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!(
            help = temp_write_help(),
            "failed to create temp file: {err}"
        ))?;
    tmp.write_all(output.as_bytes()).map_err(|err| miette!(
        help = temp_write_help(),
        "failed to write temp file: {err}"
    ))?;
    tmp.persist(task_file).map_err(|err| miette!(
        help = temp_write_help(),
        "failed to persist temp file: {err}"
    ))?;

    Ok(())
}

/// Get the effective instructions text for a state from reusable and inline prompts.
// §FS-rhei-states.4.4: Template prompt text is emitted before inline state text.
fn state_instructions(machine: &rhei_validator::StateMachine, state: &str) -> String {
    machine
        .states
        .get(state)
        .and_then(|def| machine.effective_instructions(def))
        .unwrap_or_default()
}

/// Get the effective personality text for a state.
fn state_personality(machine: &rhei_validator::StateMachine, state: &str) -> Option<String> {
    machine.effective_personality(machine.states.get(state)?)
}
