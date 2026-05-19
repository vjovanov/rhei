
/// Parse a task ID string into a [`TaskId`].
///
/// Accepts both single-segment ids (`1`, `api`) and dotted paths (`1.2`,
/// `api.cache`). Malformed input is treated as a single named segment so
/// downstream lookups fail cleanly with a "not found" message.
fn parse_task_id(s: &str) -> TaskId {
    if s.is_empty() {
        return TaskId::named(s);
    }
    let mut segments = Vec::new();
    for part in s.split('.') {
        if part.is_empty() {
            return TaskId::named(s);
        }
        if let Ok(n) = part.parse::<u32>() {
            segments.push(rhei_core::ast::TaskIdSegment::Number(n));
        } else {
            segments.push(rhei_core::ast::TaskIdSegment::Named(part.to_string()));
        }
    }
    TaskId::from_segments(segments)
}

/// Insert a `**Assignee:** <value>` metadata line for a specific task.
///
/// Locates the `### Task <id>:` header, walks through its metadata block
/// (`**State:**`, optional `**Prior:**`), and inserts the Assignee line at
/// the end of that block, matching the task grammar order. A duplicate
/// insertion is a no-op if the task
/// already has an assignee line; the caller is responsible for ensuring the
/// field is absent first (use claim-time eligibility checks).
// §FS-rhei-plan-language.2: Task metadata grammar order.
fn insert_task_assignee(raw: &str, task_id: &str, assignee: &str) -> MietteResult<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 1);

    let task_prefix = format!("### Task {}:", task_id);
    let mut in_target_task = false;
    let mut last_metadata_idx: Option<usize> = None;
    let mut already_present = false;
    let mut inserted = false;

    for line in lines.iter() {
        if line.starts_with("### Task ") {
            if let Some(meta_idx) = last_metadata_idx.take() {
                // Leaving previous target without finding a home for the
                // assignee line — insert immediately after its last metadata
                // line before appending the subsequent task header.
                insert_after(&mut result, meta_idx, &format_assignee(assignee));
                inserted = true;
            }
            in_target_task = line.starts_with(&task_prefix);
        }

        if in_target_task && line.starts_with("**Assignee:**") {
            already_present = true;
        }
        if in_target_task && (line.starts_with("**State:**") || line.starts_with("**Prior:**")) {
            last_metadata_idx = Some(result.len());
        }

        result.push((*line).to_string());
    }

    if already_present {
        // Nothing to do — preserve input verbatim to keep trailing newline.
        return Ok(raw.to_string());
    }
    if inserted {
        let mut output = result.join("\n");
        if raw.ends_with('\n') {
            output.push('\n');
        }
        return Ok(output);
    }

    let Some(meta_idx) = last_metadata_idx else {
        return Err(miette!(
            "could not find **State:**/**Prior:** metadata line for Task {} in the markdown",
            task_id
        ));
    };
    insert_after(&mut result, meta_idx, &format_assignee(assignee));

    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

fn format_assignee(value: &str) -> String {
    format!("**Assignee:** {}", value)
}

fn insert_after(lines: &mut Vec<String>, idx: usize, value: &str) {
    let insert_at = idx + 1;
    if insert_at >= lines.len() {
        lines.push(value.to_string());
    } else {
        lines.insert(insert_at, value.to_string());
    }
}

/// Rewrite the `**State:**` line for a specific task in the raw markdown.
///
/// Locates the `### Task <id>:` header and replaces the immediately following
/// `**State:**` line with the new state value.
fn rewrite_task_state(raw: &str, task_id: &str, new_state: &str) -> MietteResult<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result = Vec::with_capacity(lines.len());

    // Build the task header prefix to match.
    let task_prefix = format!("### Task {}:", task_id);

    let mut in_target_task = false;
    let mut state_replaced = false;

    for line in &lines {
        if !state_replaced && line.starts_with("### Task ") {
            in_target_task = line.starts_with(&task_prefix);
        }

        if in_target_task && !state_replaced && line.starts_with("**State:**") {
            let formatted = format!("**State:** {}", format_state_metadata_value(new_state));
            result.push(formatted);
            state_replaced = true;
            continue;
        }

        result.push(line.to_string());
    }

    if !state_replaced {
        return Err(miette!("could not find **State:** line for Task {} in the markdown", task_id));
    }

    // Preserve trailing newline if original had one.
    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// Execute the `next` subcommand: transition the next ready task to the next state,
/// and print the task details with instructions.
fn next_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_filter: Option<&str>,
    as_json: bool,
    no_callbacks: bool,
    peek: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machine = resolved.machine;
    let callback_paths = resolve_callback_paths(resolved.path.as_deref(), input)?;
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);

    // Validate the plan first.
    let report = rhei_validator::validate_with_machine(&loaded.rhei, &machine);
    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    // Find the target task to claim.
    let (task_id_str, current_state_raw, current_state) = if let Some(tid) = task_id_filter {
        let target_id = parse_task_id(tid);
        let task = loaded
            .rhei
            .tasks
            .iter()
            .find(|t| t.id == target_id)
            .ok_or_else(|| miette!("task '{}' not found in the plan", tid))?;
        let state_name = normalized_state_name(task.state.as_str(), &machine);
        let is_initial = task_is_in_initial_state(task, &state_name, &machine);
        if is_initial {
            let state_map: HashMap<&TaskId, String> = loaded
                .rhei
                .tasks
                .iter()
                .map(|t| (&t.id, normalized_state_name(t.state.as_str(), &machine)))
                .collect();
            let all_priors_done = task.prior.iter().all(|dep_id| {
                state_map.get(dep_id).map(|s| dependency_is_satisfied(s, &machine)).unwrap_or(false)
            });
            if !all_priors_done {
                return Err(miette!("Task {} is blocked by incomplete prerequisites", tid));
            }
        }
        let state_def = machine
            .states
            .get(&state_name)
            .ok_or_else(|| miette!("state '{}' missing from loaded machine", state_name))?;
        let settings = load_merged_settings(&workspace_root)?;
        ensure_state_inputs_exist_for_transition(
            &workspace_root,
            tid,
            &state_name,
            state_def,
            Some(render_visit_count(
                loaded.rhei.metadata.as_ref(),
                &task.id,
                &state_name,
                task.state.as_str(),
                &machine,
            )),
            &machine,
            &settings,
            &format!("Task {} cannot be claimed in state {}.", tid, state_name),
        )?;
        (tid.to_string(), task.state.as_str().to_string(), state_name)
    } else {
        let ready = find_claimable_tasks(&loaded.rhei, &machine, &workspace_root);
        if ready.is_empty() {
            return Err(miette!("{}", diagnose_no_claimable(&loaded.rhei, &machine)));
        }
        let task = ready.into_iter().next().unwrap();
        let state_name = normalized_state_name(task.state.as_str(), &machine);
        let state_def = machine
            .states
            .get(&state_name)
            .ok_or_else(|| miette!("state '{}' missing from loaded machine", state_name))?;
        let settings = load_merged_settings(&workspace_root)?;
        ensure_state_inputs_exist_for_transition(
            &workspace_root,
            &task.id.to_string(),
            &state_name,
            state_def,
            Some(render_visit_count(
                loaded.rhei.metadata.as_ref(),
                &task.id,
                &state_name,
                task.state.as_str(),
                &machine,
            )),
            &machine,
            &settings,
            &format!("Task {} cannot be claimed in state {}.", task.id, state_name),
        )?;
        (task.id.to_string(), task.state.to_string(), state_name)
    };

    // Determine whether we need a state transition.
    // Tasks in an initial state (e.g. draft) are transitioned forward.
    let target_id = parse_task_id(&task_id_str);
    let selected_task = loaded
        .rhei
        .tasks
        .iter()
        .find(|task| task.id == target_id)
        .ok_or_else(|| miette!("task '{}' not found in the plan", task_id_str))?;
    let is_initial = task_is_in_initial_state(selected_task, &current_state, &machine);
    let current_state_def = machine
        .states
        .get(&current_state)
        .ok_or_else(|| miette!("state '{}' missing from loaded machine", current_state))?;
    let auto_transition_initial =
        is_initial && !state_declares_autonomous_execution(current_state_def);

    let task_file = loaded.task_file(&task_id_str, input);
    let metadata_file = if workspace::is_workspace(input) {
        input.join("index.rhei.md")
    } else {
        task_file.clone()
    };

    let final_state = if auto_transition_initial && !peek {
        // Advance from the initial state (e.g. draft → pending).
        let target_id = parse_task_id(&task_id_str);
        let task = loaded
            .rhei
            .tasks
            .iter()
            .find(|task| task.id == target_id)
            .ok_or_else(|| miette!("task '{}' not found in the plan", task_id_str))?;
        let to_state = find_next_transition(task, &loaded.rhei, &machine)?.ok_or_else(|| {
            miette!("no forward transition available from state '{}'", current_state_raw)
        })?;
        execute_transition(
            TransitionFiles { task_file: &task_file, metadata_file: &metadata_file },
            &callback_paths,
            &machine,
            &task_id_str,
            &current_state,
            &to_state,
            no_callbacks,
        )?;
        to_state
    } else {
        current_state.clone()
    };

    // Re-load to get the updated task for output.
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(&task_id_str);
    let task = loaded
        .rhei
        .tasks
        .iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| miette!("task '{}' not found after transition", task_id_str))?;

    // Resolve agent/model for display. `next` should still print the next
    // task even when the state's agent is misconfigured, so demote resolution
    // errors to a stderr warning instead of failing the command outright.
    let settings = load_merged_settings(&workspace_root)?;
    let no_agent_opts = default_run_options();
    let resolved = match resolve_agent(&machine, &final_state, &settings, &no_agent_opts) {
        Ok(resolved) => resolved,
        Err(err) => {
            eprintln!(
                "warning: could not resolve agent for state '{}': {}",
                final_state, err
            );
            None
        }
    };
    let agent_id_str = resolved.as_ref().map(|r| r.agent.id().to_string());
    let model_id_str = resolved.as_ref().and_then(|r| r.model.clone());
    let model_provider_str = resolved.as_ref().and_then(|r| r.model_provider.clone());
    let model_name_str = resolved.as_ref().and_then(|r| r.model_name.clone());

    // Claim mode only: write `**Assignee:**` to the task file so a second
    // `rhei next` cannot re-claim the same task. Skipped in peek mode and
    // when the task already has an assignee set.
    if !peek && task.assignee.is_none() {
        if let Some(agent) = agent_id_str.as_deref() {
            write_task_assignee(&task_file, &task_id_str, agent)?;
        }
    }
    let tooling = resolve_tooling(&machine, &final_state, &settings);
    let render_context = RuntimeTemplateContext {
        workspace_root: &workspace_root,
        plan_path: &callback_paths.plan_path,
        state_machine_path: callback_paths.state_machine_path.as_deref(),
        plan_title: &loaded.rhei.title,
        task,
        state_name: &final_state,
        current_state_raw: task.state.as_str(),
        machine: &machine,
        metadata: loaded.rhei.metadata.as_ref(),
        target: resolved.as_ref().and_then(|r| r.target.as_ref()),
        model: model_id_str.as_deref(),
        model_provider: model_provider_str.as_deref(),
        model_name: model_name_str.as_deref(),
        agent: agent_id_str.as_deref(),
        agent_mode: resolved.as_ref().and_then(|r| r.mode.as_deref()),
        tooling: Some(&tooling),
    };
    let instructions = resolve_runtime_template_text(
        state_instructions(&machine, &final_state).as_str(),
        &render_context,
    );
    let personality = machine
        .states
        .get(final_state.as_str())
        .and_then(|def| def.personality.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|text| resolve_runtime_template_text(text, &render_context));

    print_next_output(NextOutput {
        as_json,
        peek,
        task,
        from_state: &current_state_raw,
        to_state: task.state.as_str(),
        personality: personality.as_deref(),
        instructions: &instructions,
        agent_id: agent_id_str.as_deref(),
        model_id: model_id_str.as_deref(),
    });

    Ok(())
}
