/// `rhei summary`: a read-only Markdown account of a run, compact enough to
/// paste into a pull request body — a lead line naming the workflow, one
/// numbered entry per recorded agent invocation, and the aggregate token
/// accounting. §FS-rhei-summary
///
/// It loads the plan and reads `runtime/accounting/`, writes no file, spawns
/// nothing, and estimates nothing: a fact that was not recorded is omitted
/// rather than guessed. §FS-rhei-summary.1
fn summary_command(input: &Path, state_machine: Option<&Path>, details: bool) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let loaded = load_plan(&input_buf)?;
    let resolved = resolve_state_machine_for_loaded_plan(&input_buf, &loaded, state_machine)?;
    let accounting_root = execution_workspace_root(&input_buf).join("runtime/accounting");
    let inspection = read_cost_inspection(&accounting_root);
    // A record that would not parse names a local file, so the warning goes to
    // stderr and stdout stays publishable verbatim. §FS-rhei-summary.4
    for error in &inspection.errors {
        eprintln!("warning: {error}");
    }
    print!("{}", render_summary(&loaded.rhei, &resolved.machine, &inspection, details));
    Ok(())
}

/// The whole document, in the three parts of §FS-rhei-summary.2, optionally
/// wrapped in the collapsed block of §FS-rhei-summary.3.
fn render_summary(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    inspection: &CostInspection,
    details: bool,
) -> String {
    let tail = summary_lead_tail(rhei, machine, inspection);
    let name = &machine.name;
    let mut out = String::new();
    if details {
        // The blank line after `</summary>` is what makes GitHub render the
        // Markdown inside the block. §FS-rhei-summary.3
        out.push_str("<details>\n");
        out.push_str(&format!("<summary>AI workflow: `{name}`, {tail}</summary>\n\n"));
    } else {
        out.push_str(&format!("`{name}` workflow: {tail}\n\n"));
    }
    let steps = summary_steps(inspection);
    if !steps.is_empty() {
        out.push_str(&steps);
        out.push('\n');
    }
    out.push_str(&summary_accounting(inspection));
    if details {
        out.push('\n');
        out.push_str("</details>\n");
    }
    out
}

/// The lead line after the workflow name: invocation count, distinct models,
/// and the task tally. §FS-rhei-summary.2.1
fn summary_lead_tail(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    inspection: &CostInspection,
) -> String {
    let invocations = inspection.invocations.len();
    let models: BTreeSet<&str> = inspection
        .invocations
        .iter()
        .filter_map(|(_, record)| record.model.as_deref())
        .collect();
    format!(
        "{invocations} agent invocation{} across {} model{}; {}.",
        plural_s(invocations),
        models.len(),
        plural_s(models.len()),
        summary_task_tally(rhei, machine)
    )
}

/// Tasks per terminal state in machine declaration order, with the
/// in-progress remainder appended so a mid-run summary says it is one.
/// §FS-rhei-summary.2.1
fn summary_task_tally(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> String {
    let tasks = flatten_tasks(rhei);
    let mut parts: Vec<(usize, &str)> = Vec::new();
    for (state, def) in &machine.states {
        if !def.terminal {
            continue;
        }
        let count = tasks.iter().filter(|task| task.state == *state).count();
        if count > 0 {
            parts.push((count, state.as_str()));
        }
    }
    let terminal: HashSet<&str> = machine
        .states
        .iter()
        .filter(|(_, def)| def.terminal)
        .map(|(state, _)| state.as_str())
        .collect();
    let in_progress = tasks.iter().filter(|task| !terminal.contains(task.state.as_str())).count();
    if in_progress > 0 {
        parts.push((in_progress, "in progress"));
    }
    if parts.is_empty() {
        return "no tasks".to_string();
    }
    let mut tally = String::new();
    for (index, (count, label)) in parts.iter().enumerate() {
        if index == 0 {
            tally.push_str(&format!("{count} task{} {label}", plural_s(*count)));
        } else {
            tally.push_str(&format!(", {count} {label}"));
        }
    }
    tally
}

/// One numbered entry per invocation record. `read_cost_inspection` already
/// sorted them, so the numbering is the `started_at` order the spec asks for.
/// §FS-rhei-summary.2.2
fn summary_steps(inspection: &CostInspection) -> String {
    let mut per_task: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, record) in &inspection.invocations {
        *per_task.entry(record.task_id.as_str()).or_default() += 1;
    }
    let mut out = String::new();
    for (index, (_, record)) in inspection.invocations.iter().enumerate() {
        // A repeated visit and a task with several records both need the visit
        // spelled out; a one-shot step stays clean. §FS-rhei-summary.2.2
        let sibling_records = per_task.get(record.task_id.as_str()).copied().unwrap_or(0);
        let repeated = record.visit > 1 || sibling_records > 1;
        let visit = if repeated { format!(" (visit {})", record.visit) } else { String::new() };
        out.push_str(&format!(
            "{}. `{}` {}{visit} — {}",
            index + 1,
            record.task_id,
            record.state,
            summary_step_actor(record)
        ));
        if let Some(duration) = summary_step_duration(record) {
            out.push_str(&format!(" — {duration}"));
        }
        if let Some(tokens) = summary_step_tokens(record, &inspection.books) {
            out.push_str(&format!(" — {tokens}"));
        }
        out.push('\n');
    }
    out
}

/// `<agent>, <provider>/<model>`, dropping whatever the record did not carry.
fn summary_step_actor(record: &AccountingInvocationRecord) -> String {
    match (record.provider.as_deref(), record.model.as_deref()) {
        (Some(provider), Some(model)) => format!("{}, {provider}/{model}", record.agent),
        (None, Some(model)) => format!("{}, {model}", record.agent),
        _ => record.agent.clone(),
    }
}

/// `ended_at - started_at`, humanized; `None` when either timestamp is
/// missing or unparseable, because a duration is not worth guessing.
/// §FS-rhei-summary.2.2
fn summary_step_duration(record: &AccountingInvocationRecord) -> Option<String> {
    let started = parse_rfc3339_utc(&record.started_at)?;
    let ended = parse_rfc3339_utc(&record.ended_at)?;
    let elapsed = ended.duration_since(started).ok()?;
    Some(format_duration_short(u64::try_from(elapsed.as_millis()).ok()?))
}

/// Humanized `in`/`out` counts, and only the sides the record measured.
/// §FS-rhei-summary.2.2
fn summary_step_tokens(
    record: &AccountingInvocationRecord,
    books: &ReachablePriceBooks,
) -> Option<String> {
    // A record written before the convention existed is read under the one its
    // own `agent` implies here too. §FS-rhei-cost-accounting.5.2
    let usage = usage_summary_from_record(record, books);
    let mut parts = Vec::new();
    if usage.input_total.value.is_some() {
        parts.push(format!("{} in", format_dimension_value(&usage.input_total)));
    }
    if usage.output_total.value.is_some() {
        parts.push(format!("{} out", format_dimension_value(&usage.output_total)));
    }
    (!parts.is_empty()).then(|| parts.join(" / "))
}

/// The aggregate strip, in the shape the per-run report uses; one line
/// instead when no record carried a measured total, because an empty table
/// reads like a zero. §FS-rhei-summary.2.3
fn summary_accounting(inspection: &CostInspection) -> String {
    let Some(summary) = inspection.summary.as_ref().filter(|it| it.total.value.is_some()) else {
        return "Token accounting was not measured for this run.\n".to_string();
    };
    let mut out = String::new();
    out.push_str("| Accounting | Value |\n| --- | ---: |\n");
    // Cost only when the run was priced: this command adds no pricing of its
    // own, and an unpriced run must show no cost. §FS-rhei-summary.4
    if summary.cost_micro.or(summary.priced_cost_micro).is_some() {
        out.push_str(&format!("| cost | {} |\n", md_cell(&format_summary_cost(summary))));
    }
    for (label, dimension) in [
        ("total tokens", &summary.total),
        ("input tokens", &summary.input_total),
        ("input cached", &summary.input_cached_read),
        ("output tokens", &summary.output_total),
        ("output cached", &summary.output_cached_read),
    ] {
        out.push_str(&format!("| {label} | {} |\n", format_dimension_value(dimension)));
    }
    out.push_str(&format!("| coverage | {:?} |\n", summary.coverage));
    out
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}
