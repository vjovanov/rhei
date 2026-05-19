
fn instantiate_execute_args_from_env() -> Vec<String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(command_index) = args.iter().position(|arg| arg == "instantiate") else {
        return Vec::new();
    };
    let command_args = &args[command_index + 1..];
    let Some(separator_index) = command_args.iter().position(|arg| arg == "--") else {
        return Vec::new();
    };
    if !command_args[..separator_index].iter().any(|arg| arg == "--execute") {
        return Vec::new();
    }
    command_args[separator_index + 1..].to_vec()
}

#[allow(clippy::too_many_arguments)]
fn ensure_state_inputs_exist_for_transition(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    visit_count: Option<u64>,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    context: &str,
) -> MietteResult<()> {
    let invocations =
        resolve_agent_invocations(machine, state_name, settings, &default_run_options())
            .unwrap_or_default();
    for (target, model, model_provider, model_name, agent, agent_mode) in
        transition_contexts_for_state(state_def, &invocations)
    {
        ensure_state_inputs_exist(
            workspace_root,
            task_id,
            state_name,
            state_def,
            visit_count,
            target,
            model,
            model_provider,
            model_name,
            agent,
            agent_mode,
            context,
        )?;
    }

    Ok(())
}

fn ensure_state_outputs_exist_for_transition(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    visit_count: Option<u64>,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
) -> MietteResult<()> {
    let invocations =
        resolve_agent_invocations(machine, state_name, settings, &default_run_options())
            .unwrap_or_default();
    for (target, model, model_provider, model_name, agent, agent_mode) in
        transition_contexts_for_state(state_def, &invocations)
    {
        ensure_state_outputs_exist(
            workspace_root,
            task_id,
            state_name,
            state_def,
            visit_count,
            target,
            model,
            model_provider,
            model_name,
            agent,
            agent_mode,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn task_has_pending_agent_invocations(
    workspace_root: &Path,
    task: &rhei_core::ast::Task,
    state_name: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    state_def: &rhei_validator::StateDef,
    settings: &RheiSettings,
) -> MietteResult<bool> {
    if state_def.outputs.is_empty() {
        return Ok(false);
    }

    let invocations =
        resolve_agent_invocations(machine, state_name, settings, &default_run_options())?;
    Ok(invocations.iter().any(|resolved| {
        !state_outputs_exist_for_resolved_invocation(
            workspace_root,
            task,
            state_name,
            current_state_raw,
            machine,
            metadata,
            state_def,
            resolved,
        )
    }))
}

fn parse_program_spec(value: &YamlValue) -> MietteResult<ProgramSpec> {
    match value {
        YamlValue::String(command) => Ok(ProgramSpec {
            command: ProgramCommand::Shell(command.clone()),
            env: BTreeMap::new(),
            working_directory: None,
            shell: true,
        }),
        YamlValue::Mapping(mapping) => {
            let command = mapping
                .get(yaml_key("command"))
                .ok_or_else(|| miette!("program object must include a 'command' field"))?;
            let command = match command {
                YamlValue::String(value) => ProgramCommand::Shell(value.clone()),
                YamlValue::Sequence(items) => ProgramCommand::Exec(
                    items
                        .iter()
                        .map(|item| {
                            item.as_str()
                                .map(str::to_string)
                                .ok_or_else(|| miette!("program.command entries must be strings"))
                        })
                        .collect::<MietteResult<Vec<_>>>()?,
                ),
                _ => return Err(miette!("program.command must be a string or string array")),
            };

            let env = mapping
                .get(yaml_key("env"))
                .map(|value| match value {
                    YamlValue::Mapping(values) => values
                        .iter()
                        .map(|(key, value)| {
                            let key = key
                                .as_str()
                                .ok_or_else(|| miette!("program.env keys must be strings"))?;
                            let value = match value {
                                YamlValue::Null => String::new(),
                                YamlValue::Bool(value) => value.to_string(),
                                YamlValue::Number(value) => value.to_string(),
                                YamlValue::String(value) => value.clone(),
                                _ => {
                                    return Err(miette!(
                                        "program.env values must be strings, numbers, booleans, or null"
                                    ))
                                }
                            };
                            Ok((key.to_string(), value))
                        })
                        .collect::<MietteResult<BTreeMap<_, _>>>(),
                    _ => Err(miette!("program.env must be a mapping")),
                })
                .transpose()?
                .unwrap_or_default();

            let working_directory = mapping
                .get(yaml_key("working_directory"))
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| miette!("program.working_directory must be a string"))
                })
                .transpose()?;

            let shell = mapping
                .get(yaml_key("shell"))
                .and_then(YamlValue::as_bool)
                .unwrap_or(matches!(command, ProgramCommand::Shell(_)));

            Ok(ProgramSpec { command, env, working_directory, shell })
        }
        _ => Err(miette!("program must be a string or object")),
    }
}

fn resolve_program(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
    opts: &RunOptions,
) -> MietteResult<Option<ResolvedProgram>> {
    if opts.no_program() {
        return Ok(None);
    }

    let state_def = machine
        .states
        .get(state_name)
        .ok_or_else(|| miette!("state '{}' missing from loaded machine", state_name))?;
    let Some(program_value) = state_def.program.as_ref() else {
        return Ok(None);
    };

    let timeout_secs = opts
        .program_timeout_override()
        .and_then(rhei_validator::parse_duration_secs)
        .or_else(|| {
            state_def.program_timeout.as_deref().and_then(rhei_validator::parse_duration_secs)
        })
        .or_else(|| {
            settings
                .defaults
                .program_timeout
                .as_deref()
                .and_then(rhei_validator::parse_duration_secs)
        })
        .or_else(|| {
            settings.program_timeout.as_deref().and_then(rhei_validator::parse_duration_secs)
        });

    Ok(Some(ResolvedProgram { program: parse_program_spec(program_value)?, timeout_secs }))
}

/// Compose the prompt that will be sent to the agent.
fn compose_agent_prompt(render_context: &RuntimeTemplateContext<'_>) -> String {
    let state_def = render_context.machine.states.get(render_context.state_name);
    let instructions = resolve_runtime_template_text(
        state_def.and_then(|d| d.instructions.as_deref()).unwrap_or("").trim(),
        render_context,
    );
    let personality = state_def
        .and_then(|d| d.personality.as_deref())
        .map(str::trim)
        .map(|text| resolve_runtime_template_text(text, render_context));

    // Build available transitions list.
    let mut transitions_list = String::new();
    for rule in &render_context.machine.transitions {
        if rule.from.0 == render_context.state_name || rule.from.0 == "*" {
            transitions_list.push_str(&format!("- {} -> {}", render_context.state_name, rule.to.0));
            if let Some(cond) = &rule.condition {
                transitions_list.push_str(&format!(" (when {})", cond));
            }
            transitions_list.push('\n');
        }
    }

    let plan_path_str = render_context.plan_path.display().to_string();
    let state_machine_label = render_context
        .state_machine_path
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "the built-in default".to_string());
    let task_id = render_context.task.id.to_string();

    let mut prompt = format!(
        "# Task {task_id}: {}\n\n## State: {}\n",
        render_context.task.title, render_context.state_name
    );
    if let Some(p) = personality {
        prompt.push_str(&format!("\n{p}\n"));
    }
    prompt.push_str(&format!("\n## Instructions\n\n{instructions}\n"));
    if !render_context.task.content.trim().is_empty() {
        prompt.push_str(&format!("\n## Task Content\n\n{}\n", render_context.task.content.trim()));
    }
    if !render_context.task.children.is_empty() {
        prompt.push_str("\n## Child Tasks\n\n");
        for child in &render_context.task.children {
            prompt.push_str(&format!(
                "- {} {}: {} [{}]\n",
                title_case_kind(&child.kind),
                child.id,
                child.title,
                child.state
            ));
        }
    }
    prompt.push_str(&format!(
        "\n## Rhei Commands\n\n\
         You are working in a rhei-managed plan at `{plan_path_str}`.\n\
         The active state machine is `{state_machine_label}`.\n\
         The `rhei run` process that spawned you is responsible for advancing the task after this invocation completes.\n\
         Do not call `rhei transition` or `rhei complete`, and do not modify `**State:**` lines directly, unless you are launching a nested execution that manages its own state.\n\n\
         Available transitions from `{}`:\n{transitions_list}",
        render_context.state_name
    ));
    prompt
}
