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
        // A ledger line carrying a `-<visit>` suffix names the same state as
        // the plain one; leaving it raw would both spell a state two ways and
        // defeat the annotate-in-place rule below. §FS-rhei-memory.3.1
        if steps.is_empty() {
            steps.push(normalized_state_name(from, render_context.machine));
        }
        steps.push(normalized_state_name(to, render_context.machine));
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
    // The previous visit's *last* attempt: where it was retried, that is the
    // one that ran, and the earlier ones are kept beside it. §FS-rhei-agents.8.1
    let Some(path) = latest_agent_log_path(
        &memory.runtime_dir,
        &render_context.task.id.to_string(),
        render_context.state_name,
        agent_log_suffix(render_context.target, render_context.model, Some(visit - 1)).as_deref(),
    ) else {
        return String::new();
    };
    format!("\nPrevious log: `{}`\n", memory_path(render_context, &path))
}

/// What this visit already tried, when it has already tried something.
///
/// A re-spawn used to receive the prompt of the attempt it was recovering from,
/// byte for byte: same `RHEI_VISIT_COUNT`, no attempt number, and no
/// `Previous log:` line, because that line keys off the *previous visit* and a
/// stalled ticket never left this one. So attempt two did what attempt one did
/// and left the same thing unwritten. This paragraph is the difference: it says
/// that this is a retry, which attempt it is, how the last one ended, and which
/// file that attempt was obliged to write and did not — the result path, which
/// the prompt already showed as where a finished task's result is *read from*,
/// and which agents read as description rather than as obligation.
///
/// Rendered only when the record belongs to *this* visit. A record from an
/// earlier stay in the state is not a retry, and telling a fresh entry that it
/// is one is the same untruth in the other direction.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4 §FS-rhei-agents.3.2.1
fn render_retry_notice(render_context: &RuntimeTemplateContext<'_>, task_root: &Path) -> String {
    let Some(memory) = render_context.memory else { return String::new() };
    let visit = render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    );
    let plan = plan_spawn_attempt(
        &memory.runtime_dir,
        task_root,
        &render_context.task.id.to_string(),
        render_context.state_name,
        agent_log_suffix(render_context.target, render_context.model, Some(visit)).as_deref(),
    );
    let Some(previous) = plan.previous.as_ref() else { return String::new() };
    let owed = match terminal_result_path_shown(render_context) {
        Some(path) => format!(
            " It did not write `{path}`, which a transition out of this state reads to finish \
             this task."
        ),
        None => String::new(),
    };
    format!(
        "\nRetrying this visit: attempt {}. The previous attempt {}.{owed} Its transcript is \
         `{}`.\n",
        plan.attempt,
        previous.ending_sentence(),
        memory_path(render_context, &previous.log)
    )
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
    // A ticket retried on its first visit has neither a ledger line nor a
    // result, and it is exactly the invocation that most needs to be told it is
    // a retry. §FS-rhei-memory.4.4
    let retry = render_retry_notice(render_context, root);
    if !has_trail && result.is_none() && retry.is_empty() {
        return Ok(String::new());
    }
    let mut out = String::from("\n## Previous Visits\n\n");
    out.push_str(&render_visit_trail(render_context, &ledger));
    if let Some(body) = result {
        out.push_str(&render_result_entries(render_context, &body));
    }
    out.push_str(&render_previous_log(render_context));
    out.push_str(&retry);
    Ok(out)
}
