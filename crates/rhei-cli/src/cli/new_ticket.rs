// `rhei new <title> --under <parent>`: a new ticket inside a rhei, or under an
// existing ticket.
//
// This part decides *what* the ticket is — owning rhei, id, depth, kind,
// state. Where the markdown lands is `new_ticket_write.rs`.

// §FS-rhei-new.3

/// The resolved answer to "under what?", with everything the id allocation
/// needs. §FS-rhei-new.3
struct TicketParent {
    rhei_id: String,
    /// Rhei-local id of the parent ticket; `None` for a top-level ticket.
    parent_local: Option<String>,
    /// Rhei-local ids of the new ticket's existing siblings.
    siblings: Vec<String>,
    /// How to name the parent in an error.
    label: String,
}

fn new_ticket_write(
    target: &Path,
    options: &NewOptions,
    parent: &str,
    description: Option<&str>,
) -> MietteResult<NewWrite> {
    let loaded = load_plan(target)?;
    let placement = resolve_ticket_parent(&loaded, parent.trim())?;
    let entry = resolve_rhei_entry(target, &loaded, &placement.rhei_id)?;
    let structure = rhei_entry_structure(&entry, target)?;

    let segment =
        resolve_new_ticket_segment(options.id.as_deref(), &placement.siblings, &placement.label)?;
    let local_id = match &placement.parent_local {
        Some(parent_local) => format!("{parent_local}.{segment}"),
        None => segment,
    };
    let depth = local_id.split('.').count() as u8;
    reject_excess_depth(&placement, depth, structure.max_levels)?;
    let kind = resolve_ticket_kind(options.kind.as_deref(), &structure, &placement.rhei_id)?;

    let qualified = format!("{}.{}", placement.rhei_id, local_id);
    let machines = resolve_state_machines_for_loaded_plan(target, &loaded, None)?;
    let machine = machines.machine_for_task_str(&qualified);
    let state = resolve_ticket_state(options.state.as_deref(), machine, &kind, depth)?;

    let block = render_ticket(&TicketFields {
        kind: &kind,
        local_id: &local_id,
        title: &options.title,
        state: &state,
        prior: &options.prior,
        provides: &options.provides,
        consumes: &options.consumes,
        assignee: options.assignee.as_deref(),
        model: options.model.as_deref(),
        target: options.target.as_deref(),
        description,
    });

    let placed = place_ticket(&entry, &placement, &local_id, &loaded, target, &options.title, &block)?;

    Ok(NewWrite {
        kind: "ticket",
        id: qualified,
        title: options.title.trim().to_string(),
        path: placed.path,
        state: Some(state),
        contents: placed.contents,
        preview: block,
        dirs: placed.dirs,
        next_hint: None,
    })
}

/// Read `--under`: a single segment names the owning rhei, anything dotted
/// names a parent ticket. Ticket ids are project-qualified, so they always
/// carry at least two segments — the two forms can never collide.
// §FS-rhei-new.3 §AR-rhei-panta.3
fn resolve_ticket_parent(loaded: &LoadedPlan, parent: &str) -> MietteResult<TicketParent> {
    if parent.is_empty() {
        return Err(miette!(
help = "name a rhei (`--under auth`) or a ticket (`--under auth.3`).",
            "--under needs a rhei id or a ticket id"
        ));
    }
    if !parent.contains('.') {
        let known = loaded.rhei_ids.iter().any(|id| id == parent);
        // The basin is created on demand, so it is a legal parent before it
        // exists. §FS-rhei-panta.2
        if !known && parent != workspace::BASIN_RHEI_ID {
            return Err(unknown_parent_error(loaded, parent));
        }
        let prefix = format!("{parent}.");
        let siblings = loaded
            .rhei
            .tasks
            .iter()
            .filter_map(|task| task.id.to_string().strip_prefix(&prefix).map(ToOwned::to_owned))
            .filter(|local| !local.contains('.'))
            .collect();
        return Ok(TicketParent {
            rhei_id: parent.to_string(),
            parent_local: None,
            siblings,
            label: format!("rhei '{parent}'"),
        });
    }

    let Some(task) = find_task_by_id_str(&loaded.rhei.tasks, parent) else {
        return Err(unknown_parent_error(loaded, parent));
    };
    let (rhei_id, parent_local) = parent
        .split_once('.')
        .map(|(rhei, local)| (rhei.to_string(), local.to_string()))
        .expect("a dotted id splits");
    let siblings = task
        .children
        .iter()
        .filter_map(|child| {
            child.id.to_string().rsplit_once('.').map(|(_, last)| last.to_string())
        })
        .collect();
    Ok(TicketParent {
        rhei_id,
        parent_local: Some(parent_local),
        siblings,
        label: format!("ticket {parent}"),
    })
}

fn unknown_parent_error(loaded: &LoadedPlan, parent: &str) -> miette::Report {
    let mut known: Vec<String> = loaded.rhei_ids.clone();
    if !known.iter().any(|id| id == workspace::BASIN_RHEI_ID) {
        known.push(workspace::BASIN_RHEI_ID.to_string());
    }
    miette!(
        help = did_you_mean(parent, &known)
            .unwrap_or_else(|| "pass a rhei id, or a ticket id to nest under.".to_string()),
        "'{parent}' names no rhei or ticket in this project. --under takes a rhei id \
         ({}) for a top-level ticket, or a ticket id like `{}.1` for a subtask",
        known.join(", "),
        known.first().map(String::as_str).unwrap_or("auth")
    )
}

/// Refuse a subtask deeper than the rhei allows, before anything is written.
// §FS-rhei-new.3.3 §FS-rhei-plan-language.3.4
fn reject_excess_depth(placement: &TicketParent, depth: u8, max_levels: u8) -> MietteResult<()> {
    if depth <= max_levels {
        return Ok(());
    }
    Err(miette!(
help = "raise `structure.maxLevels` in the rhei's frontmatter, or add the ticket higher up.",

        "a ticket under {} would sit at depth {depth}, but rhei '{}' allows {max_levels} \
         (`structure.maxLevels`)",
        placement.label,
        placement.rhei_id
    ))
}

/// The heading keyword, checked against what the rhei declares.
// §FS-rhei-new.3.3 §FS-rhei-plan-language.3.7
fn resolve_ticket_kind(
    requested: Option<&str>,
    structure: &rhei_core::ast::Structure,
    rhei_id: &str,
) -> MietteResult<String> {
    let kind = requested.unwrap_or("task").trim().to_ascii_lowercase();
    if structure.node_kinds.iter().any(|declared| declared.eq_ignore_ascii_case(&kind)) {
        return Ok(kind);
    }
    Err(miette!(
        help = did_you_mean(&kind, &structure.node_kinds)
            .unwrap_or_else(|| "declare the kind in `structure.nodeKinds` first.".to_string()),
        "rhei '{rhei_id}' does not declare the node kind '{kind}'. It declares: {}",
        structure.node_kinds.join(", ")
    ))
}

/// The state the ticket is created in: the machine's initial state for this
/// node, or the one `--state` names, checked against that same machine.
// §FS-rhei-new.3.2
fn resolve_ticket_state(
    requested: Option<&str>,
    machine: &rhei_validator::StateMachine,
    kind: &str,
    depth: u8,
) -> MietteResult<String> {
    let Some(requested) = requested else {
        return initial_state_for_node(machine, kind, depth);
    };
    let normalized = normalized_state_name(requested.trim(), machine);
    if machine.is_valid_state(&normalized) {
        return Ok(normalized);
    }
    let known: Vec<String> = machine.allowed_states().map(ToOwned::to_owned).collect();
    Err(miette!(
        help = did_you_mean(requested.trim(), &known)
            .unwrap_or_else(|| "omit --state to start in the machine's initial state.".to_string()),
        "state machine '{}' has no state '{}'. It declares: {}",
        machine.name,
        requested.trim(),
        known.join(", ")
    ))
}
