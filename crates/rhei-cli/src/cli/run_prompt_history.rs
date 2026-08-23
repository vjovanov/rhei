// `## Plan History`: what finished before this invocation, one line per task,
// plus who else is working right now and who is waiting on this task.
//
// Its own part because the history is ordered by the runtime ledger and
// summarized from result files, while the orientation next door reads only the
// plan tree.

// §AR-source-file-size.3 §FS-rhei-memory.3.2 §FS-rhei-memory.4.3

/// The preamble that says what the list is and where the full text lives.
// §FS-rhei-memory.3.2
const PLAN_HISTORY_PREAMBLE: &str = "Finished work, oldest first. Full text: \
     `runtime/results/<id>.md` under the owning rhei's execution root.";

/// One rendered history line, kept structured until the cap has been applied.
struct HistoryEntry {
    /// `<Kind> <qualified id>`, the way every other surface names a node.
    // §FS-rhei-memory.3.2
    label: String,
    title: String,
    state: String,
    summary: String,
    /// The rhei tag a task outside the owning rhei carries.
    // §FS-rhei-memory.3.2
    foreign_rhei: Option<String>,
}

impl HistoryEntry {
    fn render(&self) -> String {
        let tag = self
            .foreign_rhei
            .as_ref()
            .map(|rhei| format!(" (rhei `{rhei}`, prior)"))
            .unwrap_or_default();
        format!(
            "- {}: {} \u{2014} {} \u{2014} {}{tag}\n",
            self.label, self.title, self.state, self.summary
        )
    }
}

/// Position in the ledger of each task's last entry into a terminal state.
///
/// That is the moment the task finished, and ordering by it is what makes the
/// list read oldest first without a timestamp anywhere in the prompt.
// §FS-rhei-memory.4.3 §FS-rhei-memory.1.2
fn terminal_ledger_positions(
    ledger: &[(String, String, String)],
    machine: &rhei_validator::StateMachine,
) -> HashMap<String, usize> {
    let mut positions = HashMap::new();
    for (index, (task_id, _, to)) in ledger.iter().enumerate() {
        if is_terminal_state(&normalized_state_name(to, machine), machine) {
            positions.insert(task_id.clone(), index);
        }
    }
    positions
}

/// The transitive closure of `Prior(task)`, in plan order.
// §FS-rhei-memory.4.3
fn transitive_priors<'a>(
    index: &HashMap<String, &'a rhei_core::ast::Task>,
    order: &[&'a rhei_core::ast::Task],
    task: &rhei_core::ast::Task,
) -> Vec<&'a rhei_core::ast::Task> {
    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<String> = task.prior.iter().map(TaskId::to_string).collect();
    while let Some(id) = queue.pop() {
        if !reached.insert(id.clone()) {
            continue;
        }
        let Some(found) = index.get(&id) else { continue };
        queue.extend(found.prior.iter().map(TaskId::to_string));
    }
    reached.remove(&task.id.to_string());
    order.iter().copied().filter(|candidate| reached.contains(&candidate.id.to_string())).collect()
}

/// The terminal tasks of the owning rhei, ordered by when they finished.
///
/// A task with no ledger line was never moved by this project — an imported
/// plan, or one completed before the ledger existed — so it cannot be placed in
/// time and comes first, in plan order.
// §FS-rhei-memory.4.3
fn own_history_tasks<'a>(
    render_context: &RuntimeTemplateContext<'a>,
    order: &[&'a rhei_core::ast::Task],
    rhei_id: &str,
    skip: &BTreeSet<String>,
) -> MietteResult<Vec<&'a rhei_core::ast::Task>> {
    let memory = render_context.memory.expect("history renders only with memory");
    let root = memory
        .rhei_roots
        .get(rhei_id)
        .map(PathBuf::as_path)
        .unwrap_or(render_context.workspace_root);
    let positions =
        terminal_ledger_positions(&read_ledger(root)?, render_context.machine);
    let own: Vec<&rhei_core::ast::Task> = order
        .iter()
        .copied()
        .filter(|candidate| rhei_id_of(candidate).as_deref() == Some(rhei_id))
        .filter(|candidate| candidate.id != render_context.task.id)
        .filter(|candidate| !skip.contains(&candidate.id.to_string()))
        .filter(|candidate| task_state_is_terminal(candidate, render_context.machine))
        .collect();
    let mut unrecorded = Vec::new();
    let mut recorded = Vec::new();
    for candidate in own {
        match positions.get(&candidate.id.to_string()) {
            Some(position) => recorded.push((*position, candidate)),
            None => unrecorded.push(candidate),
        }
    }
    recorded.sort_by_key(|(position, _)| *position);
    unrecorded.extend(recorded.into_iter().map(|(_, task)| task));
    Ok(unrecorded)
}

/// `### In Flight` — every other agent touching this project right now.
// §FS-rhei-memory.3.2 §FS-rhei-memory.4.3
fn render_in_flight(
    render_context: &RuntimeTemplateContext<'_>,
    order: &[&rhei_core::ast::Task],
) -> String {
    let memory = render_context.memory.expect("history renders only with memory");
    let claimed: Vec<(&rhei_core::ast::Task, String)> = order
        .iter()
        .copied()
        .filter(|candidate| candidate.id != render_context.task.id)
        .filter(|candidate| !task_state_is_terminal(candidate, render_context.machine))
        .filter_map(|candidate| match candidate.assignee.as_deref() {
            Some(assignee) => Some((candidate, assignee.to_string())),
            // `rhei run` claims by spawning, not by writing `**Assignee:**`, so
            // the pass's own set is the only witness for its workers.
            None if memory.run_in_flight.contains(&candidate.id.to_string()) => {
                Some((candidate, "this run".to_string()))
            }
            None => None,
        })
        .collect();
    if claimed.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n### In Flight\n\n");
    for (task, assignee) in claimed.iter().take(memory_caps::IN_FLIGHT) {
        out.push_str(&format!(
            "- {}: {} [{}] \u{2014} {assignee}\n",
            memory_node_label(task),
            task.title,
            memory_state_name(task, render_context.machine)
        ));
    }
    if claimed.len() > memory_caps::IN_FLIGHT {
        out.push_str(&format!(
            "\u{2026} {} more \u{2014} rhei list --non-terminal\n",
            claimed.len() - memory_caps::IN_FLIGHT
        ));
    }
    out
}

/// `### Dependents` — who reads what this task writes.
// §FS-rhei-memory.3.2 §FS-rhei-memory.4.3
fn render_dependents(
    render_context: &RuntimeTemplateContext<'_>,
    order: &[&rhei_core::ast::Task],
) -> String {
    let subject = render_context.task;
    let mut dependents = Vec::new();
    for candidate in order.iter().copied() {
        if candidate.id == subject.id {
            continue;
        }
        let mut relations = Vec::new();
        if candidate.prior.iter().any(|prior| prior == &subject.id) {
            relations.push("prior".to_string());
        }
        for consumed in &candidate.consumes {
            if consumed.task == subject.id && subject.provides.contains(&consumed.name) {
                relations.push(format!("consumes `{}`", consumed.name));
            }
        }
        if !relations.is_empty() {
            dependents.push((candidate, relations.join(", ")));
        }
    }
    if dependents.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n### Dependents\n\n");
    for (task, relation) in dependents.iter().take(memory_caps::DEPENDENTS) {
        out.push_str(&format!(
            "- {}: {} [{}] \u{2014} {relation}\n",
            memory_node_label(task),
            task.title,
            memory_state_name(task, render_context.machine)
        ));
    }
    if dependents.len() > memory_caps::DEPENDENTS {
        out.push_str(&format!(
            "\u{2026} {} more \u{2014} rhei list --has-prior {}\n",
            dependents.len() - memory_caps::DEPENDENTS,
            subject.id
        ));
    }
    out
}

/// `## Plan History` — the finished work, then who is working and who waits.
// §FS-rhei-memory.3.2 §FS-rhei-memory.4.3
fn render_plan_history(render_context: &RuntimeTemplateContext<'_>) -> MietteResult<String> {
    if render_context.memory.is_none() {
        return Ok(String::new());
    }
    let Some(plan_tasks) = render_context.plan_tasks else { return Ok(String::new()) };
    let Some(rhei_id) = owning_rhei_id(render_context) else { return Ok(String::new()) };
    let order = flatten_task_slice(plan_tasks);
    let index: HashMap<String, &rhei_core::ast::Task> =
        order.iter().map(|task| (task.id.to_string(), *task)).collect();

    let pasted_in_full = results_pasted_in_full(render_context)?;
    let skip = pasted_descendant_ids(render_context, &pasted_in_full);
    let own = own_history_tasks(render_context, &order, &rhei_id, &skip)?;
    let own_ids: BTreeSet<String> = own.iter().map(|task| task.id.to_string()).collect();
    let priors: Vec<&rhei_core::ast::Task> =
        transitive_priors(&index, &order, render_context.task)
            .into_iter()
            .filter(|task| !own_ids.contains(&task.id.to_string()))
            .collect();

    // Priors are kept; the cap eats the rhei's own backlog, oldest first —
    // decided first, so no dropped entry's result file is opened for nothing.
    // §FS-rhei-memory.4.3
    let dropped =
        (own.len() + priors.len()).saturating_sub(memory_caps::PLAN_HISTORY).min(own.len());

    let mut entries = Vec::new();
    for task in own.iter().skip(dropped) {
        entries.push(HistoryEntry {
            label: memory_node_label(task),
            title: task.title.clone(),
            state: memory_state_name(task, render_context.machine),
            summary: task_history_summary(render_context, &task.id, &pasted_in_full)?,
            foreign_rhei: None,
        });
    }
    for task in priors {
        entries.push(HistoryEntry {
            label: memory_node_label(task),
            title: task.title.clone(),
            state: memory_state_name(task, render_context.machine),
            summary: task_history_summary(render_context, &task.id, &pasted_in_full)?,
            foreign_rhei: rhei_id_of(task).filter(|owner| owner != &rhei_id),
        });
    }

    let in_flight = render_in_flight(render_context, &order);
    let dependents = render_dependents(render_context, &order);
    if entries.is_empty() && in_flight.is_empty() && dependents.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::from("\n## Plan History\n");
    // The preamble introduces the list; with nothing finished yet, the section
    // is carried by its sub-sections alone and the preamble would be a lie.
    if !entries.is_empty() {
        out.push_str(&format!("\n{PLAN_HISTORY_PREAMBLE}\n\n"));
        if dropped > 0 {
            out.push_str(&format!(
                "\u{2026} {dropped} earlier tasks not shown \u{2014} rhei list --rhei \
                 {rhei_id} --terminal\n"
            ));
        }
        for entry in &entries {
            out.push_str(&entry.render());
        }
    }
    out.push_str(&in_flight);
    out.push_str(&dependents);
    Ok(out)
}
