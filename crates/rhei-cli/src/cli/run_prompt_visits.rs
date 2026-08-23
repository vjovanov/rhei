// `## Previous Visits`: what already happened to *this* task — the trail it
// took through the machine, every verdict recorded against it, and where the
// last transcript is.
//
// Its own part because this is the one section composed from a single task's
// own runtime record rather than from the graph around it.

// §AR-source-file-size.3 §FS-rhei-memory.3.3 §FS-rhei-memory.4.4

/// The states this task has been through, ending in the visit being composed.
///
/// The ledger records each hop as `from@to`, so the trail is the first hop's
/// source followed by every destination. The visit that is starting has no
/// ledger line yet: when the trail already ends in the state being entered —
/// the engine wrote the line that moved the task here — that last state is
/// annotated rather than repeated, which is what `pending → review → review`
/// used to say about a task that had been through `review` exactly once.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
fn render_visit_trail(
    render_context: &RuntimeTemplateContext<'_>,
    ledger: &[(String, String, String)],
) -> String {
    let task_id = render_context.task.id.to_string();
    let mut steps: Vec<String> = Vec::new();
    for (entry_task, from, to) in ledger {
        if entry_task != &task_id {
            continue;
        }
        if steps.is_empty() {
            steps.push(from.clone());
        }
        steps.push(to.clone());
    }
    let visit = render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    );
    let here = format!("{} (this visit, visit {visit})", render_context.state_name);
    match steps.last_mut() {
        // A self-loop leaves `fix` twice in the ledger, and both belong: the
        // second is the visit before this one, not this one.
        Some(last) if last.as_str() == render_context.state_name => *last = here,
        _ => steps.push(here),
    }
    format!("Trail for this task: {}.\n", steps.join(" \u{2192} "))
}

/// The log file of the previous visit of this same state, when it is on disk.
///
/// The path is enough: a transcript is not worth its tokens, but an agent
/// retrying a state that stalled has to be able to find out how.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4 §FS-rhei-agents.8.1
fn render_previous_log(render_context: &RuntimeTemplateContext<'_>) -> String {
    let Some(memory) = render_context.memory else { return String::new() };
    let visit = render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    );
    if visit <= 1 {
        return String::new();
    }
    let path = agent_log_path(
        &memory.runtime_dir,
        &render_context.task.id.to_string(),
        render_context.state_name,
        agent_log_suffix(render_context.target, render_context.model, Some(visit - 1)).as_deref(),
    );
    if !path.exists() {
        return String::new();
    }
    format!("\nPrevious log: `{}`\n", memory_path(render_context, &path))
}

/// Every verdict recorded against this task so far, pasted whole.
///
/// This is where a worker's `--result` message and the engine's own failure
/// entries both land, so a retry can read why the last attempt did not stand.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4 §FS-rhei-agents.3.2.1
fn render_result_entries(
    render_context: &RuntimeTemplateContext<'_>,
    body: &str,
) -> String {
    let (kept, truncated) = tail_lines(body, memory_caps::RESULT_LINES);
    // The file the body came from — the legacy link's, when a pre-qualification
    // plan is what carries this ticket's account. §FS-rhei-memory.4.4
    let path = resolved_result_path(render_context, &render_context.task.id).unwrap_or_else(|| {
        task_result_path(
            export_root_for_task(render_context, &render_context.task.id),
            &render_context.task.id,
        )
    });
    let overflow = if truncated {
        format!(
            "\u{2026} earlier entries omitted; read {}\n\n",
            memory_path(render_context, &path)
        )
    } else {
        String::new()
    };
    format!("\nResult entries so far:\n\n{overflow}{}\n", fenced_markdown(&kept))
}

/// `## Previous Visits` — omitted on a task's first invocation, which has no
/// ledger line and no result file, and therefore nothing to say.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
fn render_previous_visits(render_context: &RuntimeTemplateContext<'_>) -> MietteResult<String> {
    let Some(memory) = render_context.memory else { return Ok(String::new()) };
    let Some(rhei_id) = owning_rhei_id(render_context) else { return Ok(String::new()) };
    let root = memory
        .rhei_roots
        .get(&rhei_id)
        .map(PathBuf::as_path)
        .unwrap_or(render_context.workspace_root);
    let ledger = read_ledger(root)?;
    let task_id = render_context.task.id.to_string();
    let has_trail = ledger.iter().any(|(entry_task, _, _)| entry_task == &task_id);
    let result = read_task_result(render_context, &render_context.task.id)?;
    if !has_trail && result.is_none() {
        return Ok(String::new());
    }
    let mut out = String::from("\n## Previous Visits\n\n");
    out.push_str(&render_visit_trail(render_context, &ledger));
    if let Some(body) = result {
        out.push_str(&render_result_entries(render_context, &body));
    }
    out.push_str(&render_previous_log(render_context));
    Ok(out)
}
