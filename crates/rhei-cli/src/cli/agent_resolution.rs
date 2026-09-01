fn resolve_target_agent(
    selector: &str,
    state_def: Option<&rhei_validator::StateDef>,
    settings: &RheiSettings,
) -> MietteResult<ResolvedAgent> {
    let target = parse_execution_target(selector)
        .map_err(|err| miette!(help = err, "invalid target selector '{}'", selector))?;
    let agent = AgentConfig::from(target.agent.clone());
    let profile = settings.agents.get(agent.id()).cloned().ok_or_else(|| {
        // §FS-rhei-errors.1.3: `settings.agents` is the merged registry and is
        // already seeded with the built-ins, so it is the whole candidate set.
        let known = settings.agents.keys().cloned().collect::<Vec<_>>();
        miette!(help = unknown_agent_help(agent.id(), &known), "agent '{}' is not defined", agent.id())
    })?;

    if let Some(mode) = target.mode.as_deref() {
        if !profile.modes.contains_key(mode) {
            let modes = profile.modes.keys().cloned().collect::<Vec<_>>();
            return Err(miette!(
                help = match did_you_mean(mode, &modes) {
                    Some(hint) => hint,
                    None => format!(
                        "agent '{}' declares no modes; drop the brackets from the selector.",
                        agent.id()
                    ),
                },
                "agent '{}' has no mode '{}'",
                agent.id(),
                mode
            ));
        }
    }

    // A target selector carries an explicit `(provider, model)` already; the
    // `models` registry is consulted for the optional per-binding `timeout`
    // override when the target's model id happens to also be a registered
    // profile.
    let model_profile = settings.models.get(target.model.as_str());
    let binding = model_profile.and_then(|p| p.agents.get(agent.id()));

    let timeout_secs = state_def
        .and_then(|d| d.agent_timeout.as_deref())
        .and_then(rhei_validator::parse_duration_secs)
        .or_else(|| {
            binding.and_then(|b| b.timeout.as_deref()).and_then(rhei_validator::parse_duration_secs)
        })
        .or_else(|| profile.timeout.as_deref().and_then(rhei_validator::parse_duration_secs))
        .or_else(|| settings.agent_timeout.as_deref().and_then(rhei_validator::parse_duration_secs))
        .or_else(|| {
            settings.defaults.agent_timeout.as_deref().and_then(rhei_validator::parse_duration_secs)
        });

    let autonomous_args = binding.map(|b| b.autonomous_args.clone()).unwrap_or_default();

    Ok(ResolvedAgent {
        agent,
        profile,
        mode: target.mode.clone(),
        target: Some(target.clone()),
        model: Some(target.model.clone()),
        model_provider: target.provider.clone(),
        model_name: Some(target.model.clone()),
        timeout_secs,
        autonomous_args,
    })
}

fn resolve_target_agent_with_model_override(
    selector: &str,
    state_def: Option<&rhei_validator::StateDef>,
    settings: &RheiSettings,
    model_override: &str,
) -> MietteResult<ResolvedAgent> {
    let mut target = parse_execution_target(selector)
        .map_err(|err| miette!(help = err, "invalid target selector '{}'", selector))?;
    target.model = model_override.to_string();
    // §FS-rhei-plan-language.3.11: `**Model:**` preserves state target agent/mode/provider.
    resolve_target_agent(&target.selector(), state_def, settings)
}

fn resolve_legacy_agent_with_model(
    state_def: Option<&rhei_validator::StateDef>,
    settings: &RheiSettings,
    opts: &RunOptions,
    model_override: Option<String>,
) -> MietteResult<Option<ResolvedAgent>> {
    // Resolve the model id first so that step 5 of the agent resolution chain
    // — `models.<id>.default_agent` — has something to look up. Precedence
    // matches the resolution order: CLI > state > nested `defaults.model` >
    // legacy top-level `model`.

    // §FS-rhei-agents.1.4: Agent/model resolution precedence.
    let model = if let Some(ovr) = model_override {
        Some(ovr)
    } else if let Some(ovr) = opts.model_override() {
        Some(ovr.to_string())
    } else if let Some(m) = state_def.and_then(|d| d.model.clone()) {
        Some(m)
    } else if let Some(m) = settings.defaults.model.clone() {
        Some(m)
    } else {
        settings.model.clone()
    };

    let model_profile = match model.as_deref() {
        Some(id) => Some(settings.models.get(id).ok_or_else(|| {
            let known = settings.models.keys().cloned().collect::<Vec<_>>();
            miette!(
                help = format!(
                    "{}Add a `models.{id}` entry to .agents/rhei/settings.json or \
                     ~/.config/rhei/settings.json, or drop the model selection.",
                    did_you_mean(id, &known).map(|hint| format!("{hint} ")).unwrap_or_default()
                ),
                "model '{}' is not defined in settings.models",
                id
            )
        })?),
        None => None,
    };

    // Agent id resolution: CLI > state > nested `defaults.agent` > legacy
    // top-level `agent` > `models.<id>.default_agent`.
    let agent = if let Some(ovr) = opts.agent_override() {
        Some(AgentConfig::from(ovr))
    } else if let Some(a) = state_def.and_then(|d| d.agent.clone()) {
        Some(a)
    } else if let Some(a) = settings.defaults.agent.clone() {
        Some(a)
    } else if let Some(a) = settings.agent.clone() {
        Some(a)
    } else {
        model_profile.and_then(|p| p.default_agent.clone()).map(AgentConfig::from)
    };

    let Some(agent) = agent else {
        return Ok(None);
    };

    let profile = settings.agents.get(agent.id()).cloned().ok_or_else(|| {
        // §FS-rhei-errors.1.2: a value carrying a mode or a model is a flag
        // mistake, not a missing settings entry.
        let known = settings.agents.keys().cloned().collect::<Vec<_>>();
        let help = agent_flag_selector_help(agent.id(), &known)
            .unwrap_or_else(|| unknown_agent_help(agent.id(), &known));
        miette!(help = help, "agent '{}' is not defined", agent.id())
    })?;

    let mode = if let Some(ovr) = opts.agent_mode_override() {
        Some(ovr.to_string())
    } else if let Some(m) = state_def.and_then(|d| d.agent_mode.clone()) {
        Some(m)
    } else if let Some(m) = settings.defaults.agent_mode.clone() {
        Some(m)
    } else if let Some(m) = settings.agent_mode.clone() {
        Some(m)
    } else {
        profile.modes.keys().next().cloned()
    };

    if let Some(name) = &mode {
        if !profile.modes.is_empty() && !profile.modes.contains_key(name) {
            let modes = profile.modes.keys().cloned().collect::<Vec<_>>();
            return Err(miette!(
                help = match did_you_mean(name, &modes) {
                    Some(hint) => hint,
                    None => format!(
                        "agent '{}' declares no modes; drop the brackets from the selector.",
                        agent.id()
                    ),
                },
                "agent '{}' has no mode '{}'",
                agent.id(),
                name
            ));
        }
    }

    let binding = model_profile.and_then(|p| p.agents.get(agent.id()));

    let timeout_secs = state_def
        .and_then(|d| d.agent_timeout.as_deref())
        .and_then(rhei_validator::parse_duration_secs)
        .or_else(|| {
            binding.and_then(|b| b.timeout.as_deref()).and_then(rhei_validator::parse_duration_secs)
        })
        .or_else(|| profile.timeout.as_deref().and_then(rhei_validator::parse_duration_secs))
        .or_else(|| settings.agent_timeout.as_deref().and_then(rhei_validator::parse_duration_secs))
        .or_else(|| {
            settings.defaults.agent_timeout.as_deref().and_then(rhei_validator::parse_duration_secs)
        });

    let model_provider = model_profile.and_then(|p| p.provider.clone());
    let model_name = model_profile.and_then(|p| p.model.clone()).or_else(|| model.clone());

    let autonomous_args = binding.map(|b| b.autonomous_args.clone()).unwrap_or_default();

    Ok(Some(ResolvedAgent {
        agent,
        profile,
        mode,
        target: None,
        model,
        model_provider,
        model_name,
        timeout_secs,
        autonomous_args,
    }))
}

/// Resolve the agent/model/mode/timeout for a task's current state.
fn resolve_agent_invocations(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
    opts: &RunOptions,
) -> MietteResult<Vec<ResolvedAgent>> {
    resolve_agent_invocations_for_task(machine, state_name, settings, opts, None)
}

fn resolve_agent_invocations_for_task(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
    opts: &RunOptions,
    task: Option<&rhei_core::ast::Task>,
) -> MietteResult<Vec<ResolvedAgent>> {
    if opts.no_agent() {
        return Ok(Vec::new());
    }

    let state_def = machine.states.get(state_name);
    if let Some(state_def) = state_def {
        let apply_task_override = state_declares_autonomous_agent_work(state_def);
        let task_target_override =
            apply_task_override.then(|| task.and_then(|task| task.target.as_deref())).flatten();
        let task_model_override = if apply_task_override && opts.model_override().is_none() {
            task.and_then(|task| task.model.as_deref())
        } else {
            None
        };

        if task_target_override.is_some() || task_model_override.is_some() {
            if !state_def.all_targets.is_empty() || !state_def.all_models.is_empty() {
                return Err(miette!(
                    help = "a fanout state runs one pass per declared target, so a per-task \
                            override has nothing to override. Remove the task's execution \
                            override, or point the task at a non-fanout state.",
                    "Task {} declares a task execution override but state '{}' is a fanout state",
                    task.map(|task| task.id.to_string())
                        .unwrap_or_else(|| "<unknown>".to_string()),
                    state_name
                ));
            }
            if state_def.target_locked {
                return Err(miette!(
                    help = format!(
                        "remove the task's execution override, or set `target_locked: false` \
                         on state '{state_name}' in the state machine."
                    ),
                    "Task {} declares a task execution override but state '{}' has target_locked: true",
                    task.map(|task| task.id.to_string())
                        .unwrap_or_else(|| "<unknown>".to_string()),
                    state_name
                ));
            }
        }

        if let Some(selector) = task_target_override {
            // §FS-rhei-plan-language.3.11: `**Target:**` replaces the full execution identity.
            if let Some(model) = opts.model_override() {
                return Ok(vec![resolve_target_agent_with_model_override(
                    selector,
                    Some(state_def),
                    settings,
                    model,
                )?]);
            }
            return Ok(vec![resolve_target_agent(selector, Some(state_def), settings)?]);
        }
        if let Some(model) = task_model_override {
            if let Some(selector) = state_def.target.as_deref() {
                return Ok(vec![resolve_target_agent_with_model_override(
                    selector,
                    Some(state_def),
                    settings,
                    model,
                )?]);
            }
            return Ok(resolve_legacy_agent_with_model(
                Some(state_def),
                settings,
                opts,
                Some(model.to_string()),
            )?
            .into_iter()
            .collect());
        }

        if !state_def.all_targets.is_empty() {
            let mut resolved = Vec::with_capacity(state_def.all_targets.len());
            for selector in &state_def.all_targets {
                resolved.push(resolve_target_agent(selector, Some(state_def), settings)?);
            }
            return Ok(resolved);
        }
        if let Some(selector) = state_def.target.as_deref() {
            return Ok(vec![resolve_target_agent(selector, Some(state_def), settings)?]);
        }
        if !state_def.all_models.is_empty() {
            let mut resolved = Vec::with_capacity(state_def.all_models.len());
            for model in &state_def.all_models {
                if let Some(agent) = resolve_legacy_agent_with_model(
                    Some(state_def),
                    settings,
                    opts,
                    Some(model.clone()),
                )? {
                    resolved.push(agent);
                }
            }
            return Ok(resolved);
        }
    }

    Ok(resolve_legacy_agent_with_model(state_def, settings, opts, None)?.into_iter().collect())
}

fn state_declares_autonomous_agent_work(state_def: &rhei_validator::StateDef) -> bool {
    state_def.agent.is_some()
        || state_def.model.is_some()
        || !state_def.all_models.is_empty()
        || state_def.target.is_some()
        || !state_def.all_targets.is_empty()
}

fn resolve_agent_for_task(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
    opts: &RunOptions,
    task: &rhei_core::ast::Task,
) -> MietteResult<Option<ResolvedAgent>> {
    Ok(resolve_agent_invocations_for_task(machine, state_name, settings, opts, Some(task))?
        .into_iter()
        .next())
}

type TransitionInvocationContext<'a> =
    (
        Option<&'a ExecutionTarget>,
        Option<&'a str>,
        Option<&'a str>,
        Option<&'a str>,
        Option<&'a str>,
        Option<&'a str>,
    );

fn transition_contexts_for_state<'a>(
    state_def: &'a rhei_validator::StateDef,
    resolved_invocations: &'a [ResolvedAgent],
) -> Vec<TransitionInvocationContext<'a>> {
    if !resolved_invocations.is_empty() {
        return resolved_invocations
            .iter()
            .map(|resolved| {
                (
                    resolved.target.as_ref(),
                    resolved.model.as_deref(),
                    resolved.model_provider.as_deref(),
                    resolved.model_name.as_deref(),
                    Some(resolved.agent.id()),
                    resolved.mode.as_deref(),
                )
            })
            .collect();
    }

    if !state_def.all_models.is_empty() {
        return state_def
            .all_models
            .iter()
            .map(|model| (None, Some(model.as_str()), None, None, None, None))
            .collect();
    }

    if let Some(model) = state_def.model.as_deref() {
        return vec![(None, Some(model), None, None, None, None)];
    }

    vec![(None, None, None, None, None, None)]
}

fn callback_contexts_for_state<'a>(
    state_def: &'a rhei_validator::StateDef,
    resolved_invocations: &'a [ResolvedAgent],
) -> Vec<(Option<&'a str>, Option<&'a str>)> {
    transition_contexts_for_state(state_def, resolved_invocations)
        .into_iter()
        .map(|(_, model, _, _, agent, _)| (model, agent))
        .collect()
}

/// Enforce the orchestrator Completion Authority contract:
/// every agent invocation that `rhei run` is about to spawn must resolve to
/// a finite timeout through the chain
/// `state.agent_timeout > models.<id>.agents.<agent>.timeout > agents.<id>.timeout > defaults.agent_timeout`.
///
/// This is the runtime counterpart to the Completion Authority / Completion
/// Condition rules. A missing timeout
/// would mean the subprocess could hang indefinitely without a deterministic
/// fallback, which defeats deterministic completion under `rhei run`.
// §FS-rhei-agents.3.1 §FS-rhei-agents.3.2: Orchestrator completion timeout.
fn ensure_orchestrator_timeout(resolved: &ResolvedAgent, state_name: &str) -> MietteResult<()> {
    if resolved.timeout_secs.is_some() {
        return Ok(());
    }
    Err(miette!(
        help = format!(
            "set `agent_timeout` on the state, on `models.<id>.agents.{}.timeout`, on \
             `agents.{}.timeout`, or on `defaults.agent_timeout` in settings.json.",
            resolved.agent.id(),
            resolved.agent.id()
        ),
        "state '{}' is driven by `rhei run` (orchestrator completion authority) \
         but no `agent_timeout` resolves for agent '{}'. Deterministic completion \
         requires a finite timeout.",
        state_name,
        resolved.agent.id(),
    ))
}

// §FS-rhei-agents.3.2 condition (2): declared `outputs:` resolve against the
// owning rhei's execution root, not the run-level workspace root.
#[allow(clippy::too_many_arguments)]
fn state_outputs_exist_for_resolved_invocation(
    artifact_root: &Path,
    task: &rhei_core::ast::Task,
    state_name: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    state_def: &rhei_validator::StateDef,
    resolved: &ResolvedAgent,
) -> bool {
    ensure_state_outputs_exist(
        artifact_root,
        &task.id.to_string(),
        state_name,
        state_def,
        Some(render_visit_count(metadata, &task.id, state_name, current_state_raw, machine)),
        resolved.target.as_ref(),
        resolved.model.as_deref(),
        resolved.model_provider.as_deref(),
        resolved.model_name.as_deref(),
        Some(resolved.agent.id()),
        resolved.mode.as_deref(),
        // A readiness probe, not a refusal anyone reads: the help never renders.
        false,
    )
    .is_ok()
}

fn default_run_options() -> RunOptions {
    RunOptions {
        standalone: StandaloneExecutionFlags {
            json: false,
            json_agent_output: false,
            headless: false,
            rhei: Vec::new(),
            dry_run: false,
            no_callbacks: false,
            continue_on_error: false,
            parallel: 1,
            tui: false,
            no_tui: false,
            dashboard: false,
            no_dashboard: false,
        },
        agent: AgentExecutionFlags { no_agent: false, agent: None, agent_mode: None, model: None },
        program: ProgramExecutionFlags::default(),
        snapshot: SnapshotExecutionFlags::default(),
    }
}
