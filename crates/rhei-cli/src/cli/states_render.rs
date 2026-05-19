fn render_state_machine_text(machine: &rhei_validator::StateMachine) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "State machine: {} (version: {})\n",
        machine.name,
        format_version(&machine.version)
    ));
    if !machine.models.is_empty() {
        out.push_str(&format!("Models: {}\n", machine.models.join(", ")));
    }
    if let Some(profiles) = machine.profiles.as_ref() {
        out.push_str("Profiles:\n");
        for (name, profile) in profiles {
            out.push_str(&format!(
                "  {name}: initial={}, allowed=[{}]\n",
                profile.initial,
                profile.allowed.join(", ")
            ));
        }
    }
    if let Some(policy) = machine.node_policy.as_ref() {
        out.push_str(&format!(
            "Node policy: root={}, default={}\n",
            policy.root, policy.default
        ));
    }

    out.push_str("\nStates:\n");
    if machine.states.is_empty() {
        out.push_str("  (none defined)\n");
    } else {
        for (idx, (name, def)) in machine.states.iter().enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            let mut flags = Vec::new();
            if def.terminal {
                flags.push("final");
            }
            if def.gating {
                flags.push("gating");
            }
            if def.concurrent {
                flags.push("concurrent");
            }
            let flag_suffix =
                if flags.is_empty() { String::new() } else { format!(" [{}]", flags.join(", ")) };
            let description = def.description.as_deref().unwrap_or("");
            out.push_str(&format!("  {name}{flag_suffix}"));
            if !description.is_empty() {
                out.push_str(&format!(" — {description}"));
            }
            out.push('\n');
            if let Some(visits) = def.visits {
                out.push_str(&format!("      Visits: {visits}\n"));
            }
            if let Some(poll) = def.poll.as_ref() {
                out.push_str(&format!(
                    "      Poll: interval={}, max_attempts={}\n",
                    poll.interval, poll.max_attempts
                ));
            }
            if let Some(target) = def.target.as_deref() {
                out.push_str(&format!("      Target: {target}\n"));
            }
            if !def.all_targets.is_empty() {
                out.push_str(&format!("      Targets: {}\n", def.all_targets.join(", ")));
            }
            if !def.all_models.is_empty() {
                out.push_str(&format!("      Models: {}\n", def.all_models.join(", ")));
            } else if let Some(model) = def.model.as_deref() {
                out.push_str(&format!("      Model: {model}\n"));
            }
            if let Some(agent) = def.agent.as_ref() {
                out.push_str(&format!("      Agent: {}\n", agent.id()));
            }
            if let Some(mode) = def.agent_mode.as_deref() {
                out.push_str(&format!("      Agent mode: {mode}\n"));
            }
            if let Some(timeout) = def.agent_timeout.as_deref() {
                out.push_str(&format!("      Agent timeout: {timeout}\n"));
            }
            if def.program.is_some() {
                out.push_str("      Program: configured\n");
            }
            if let Some(timeout) = def.program_timeout.as_deref() {
                out.push_str(&format!("      Program timeout: {timeout}\n"));
            }
            if let Some(mcp_servers) = def.mcp_servers.as_ref() {
                let ids = mcp_servers.iter().map(|entry| entry.id()).collect::<Vec<_>>();
                out.push_str(&format!("      MCP servers: {}\n", ids.join(", ")));
            }
            if let Some(skills) = def.skills.as_ref() {
                let ids = skills.iter().map(|entry| entry.id()).collect::<Vec<_>>();
                out.push_str(&format!("      Skills: {}\n", ids.join(", ")));
            }
            if def.snapshot.is_some() {
                out.push_str("      Snapshot: configured\n");
            }
            if !def.inputs.is_empty() {
                out.push_str("      Inputs:\n");
                for artifact in &def.inputs {
                    out.push_str(&format!("        - {}: {}\n", artifact.name, artifact.path));
                }
            }
            if !def.outputs.is_empty() {
                out.push_str("      Outputs:\n");
                for artifact in &def.outputs {
                    out.push_str(&format!("        - {}: {}\n", artifact.name, artifact.path));
                }
            }
            if let Some(personality) =
                def.personality.as_deref().map(str::trim).filter(|s| !s.is_empty())
            {
                out.push_str(&format!("      Personality: {personality}\n"));
            }
            if let Some(instructions) = def.instructions.as_deref() {
                for line in instructions.lines() {
                    out.push_str(&format!("      {line}\n"));
                }
            }
        }
    }

    out.push_str("\nTransitions:\n");
    if machine.transitions.is_empty() {
        out.push_str("  (none declared)\n");
    } else {
        for rule in &machine.transitions {
            out.push_str(&format!("  {} -> {}", rule.from.0, rule.to.0));
            let mut annotations = Vec::new();
            if let Some(cb) = rule.on_leave.as_ref() {
                annotations.push(format!("on_leave={}", cb.0));
            }
            if let Some(cb) = rule.on_enter.as_ref() {
                annotations.push(format!("on_enter={}", cb.0));
            }
            if let Some(cond) = rule.condition.as_ref() {
                annotations.push(format!("when={cond}"));
            }
            if let Some(t) = rule.timeout.as_ref() {
                annotations.push(format!("timeout={t}"));
            }
            if !annotations.is_empty() {
                out.push_str(&format!(" ({})", annotations.join(", ")));
            }
            out.push('\n');
        }
    }

    out
}

fn render_state_machine_json(machine: &rhei_validator::StateMachine) -> Result<String> {
    let states: Vec<serde_json::Value> = machine
        .states
        .iter()
        .map(|(name, def)| {
            serde_json::json!({
                "name": name,
                "description": &def.description,
                "instructions": &def.instructions,
                "personality": &def.personality,
                "final": def.terminal,
                "gating": def.gating,
                "concurrent": def.concurrent,
                "poll": &def.poll,
                "visits": def.visits,
                "target": &def.target,
                "all_targets": &def.all_targets,
                "all_models": &def.all_models,
                "model": &def.model,
                "agent": def.agent.as_ref().map(|agent| agent.id()),
                "agent_mode": &def.agent_mode,
                "agent_timeout": &def.agent_timeout,
                "program": &def.program,
                "program_timeout": &def.program_timeout,
                "mcp_servers": &def.mcp_servers,
                "skills": &def.skills,
                "snapshot": &def.snapshot,
                "inputs": &def.inputs,
                "outputs": &def.outputs,
            })
        })
        .collect();

    let transitions =
        serde_json::to_value(&machine.transitions).context("serialize transitions")?;
    let version =
        serde_json::to_value(&machine.version).context("serialize state machine version")?;

    let payload = serde_json::json!({
        "name": machine.name,
        "models": &machine.models,
        "profiles": &machine.profiles,
        "node_policy": &machine.node_policy,
        "version": version,
        "states": states,
        "transitions": transitions,
    });

    serde_json::to_string_pretty(&payload).context("render state machine as JSON")
}

fn format_version(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.clone(),
        other => serde_yaml::to_string(other)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
    }
}

/// Read the markdown plan source file from disk.
fn read_input_file(path: &Path) -> MietteResult<String> {
    fs::read_to_string(path).map_err(|err| file_io_report(path, "failed to read input file", err))
}

/// A loaded plan with optional workspace task-to-file mapping.
struct LoadedPlan {
    rhei: rhei_core::ast::Rhei,
    /// For directory workspaces: maps task ID string → source file path.
    /// Empty for single-file plans.
    task_sources: HashMap<String, PathBuf>,
}

impl LoadedPlan {
    /// Return the file path that contains the given task.
    /// For single-file plans, returns `fallback` (the plan file itself).
    fn task_file(&self, task_id: &str, fallback: &Path) -> PathBuf {
        self.task_sources.get(task_id).cloned().unwrap_or_else(|| fallback.to_path_buf())
    }
}

/// Load a plan from a file or directory workspace.
fn load_plan(path: &Path) -> MietteResult<LoadedPlan> {
    if let Some(ws_dir) = workspace::workspace_dir(path) {
        let ws = workspace::load_workspace(&ws_dir).map_err(|err| miette!("{}", err.message))?;
        Ok(LoadedPlan { rhei: ws.rhei, task_sources: ws.task_sources })
    } else {
        let input = read_input_file(path)?;
        let rhei = rhei_core::parse(&input).map_err(|err| parse_report(path, &input, &err))?;
        Ok(LoadedPlan { rhei, task_sources: HashMap::new() })
    }
}

/// Read and parse a markdown plan file into a [`rhei_core::ast::Rhei`].
fn parse_input_file(path: &Path) -> MietteResult<rhei_core::ast::Rhei> {
    Ok(load_plan(path)?.rhei)
}

/// Execute the `validate` subcommand once or in watch mode.
fn validate_command(input: &Path, state_machine: Option<&Path>, watch: bool) -> MietteResult<()> {
    if watch {
        watch_validation_command(input, state_machine)
    } else {
        run_validation_once(input, state_machine)
    }
}

/// Parse a plan, load the selected states, and print validation results.
fn run_validation_once(input: &Path, state_machine: Option<&Path>) -> MietteResult<()> {
    // For single-file plans, use the multi-error parser so the user sees
    // every recoverable parse problem in one run instead of fix-and-retry.
    // Workspace loads still go through the single-error path today; that's
    // a scoped follow-up when per-task files need the same treatment.
    let loaded = if workspace::workspace_dir(input).is_some() {
        load_plan(input)?
    } else {
        let raw = read_input_file(input)?;
        let (maybe_rhei, errs) = rhei_core::parser::parse_collect(&raw);
        match (maybe_rhei, errs.is_empty()) {
            (Some(rhei), true) => LoadedPlan { rhei, task_sources: HashMap::new() },
            (_, false) | (None, _) => {
                return Err(parse_errors_report(input, &raw, &errs));
            }
        }
    };

    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine)?;
    let base_path = input.parent().unwrap_or(Path::new("."));
    let mut report =
        rhei_validator::validate_with_machine_and_base(&loaded.rhei, &resolved.machine, base_path);
    let workspace_root = execution_workspace_root(input);
    let settings = load_merged_settings(&workspace_root)?;
    report.errors.extend(validate_machine_settings_references(&resolved.machine, &settings));
    report.errors.extend(validate_snapshot_plan_context(&loaded, &resolved.machine));
    report.warnings.extend(snapshot_orphan_validation_warnings(
        &workspace_root,
        &loaded,
        &resolved.machine,
        &settings,
    )?);

    if report.has_errors() {
        return Err(validation_report(input, resolved.path.as_deref(), &report.errors));
    }

    print_validation_report(&report.warnings);

    Ok(())
}

/// Print success output and any non-fatal validation warnings.
fn print_validation_report(warnings: &[String]) {
    println!("Validation succeeded");
    for warning in warnings {
        println!("warning: {warning}");
    }
}

/// Watch the plan and states files and re-run validation on relevant changes.
fn watch_validation_command(input: &Path, state_machine: Option<&Path>) -> MietteResult<()> {
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machine_for_loaded_plan(input, &loaded, state_machine)?;
    let watched_paths = match resolved.path.as_deref() {
        Some(sm) => canonical_watched_paths(input, sm),
        None => canonical_watched_paths(input, input), // only watch the plan itself
    };
    let watch_roots = match resolved.path.as_deref() {
        Some(sm) => watch_roots(input, sm),
        None => watch_roots(input, input),
    };

    println!(
        "Watch mode started for '{}' (states: {})",
        input.display(),
        state_machine_label(resolved.path.as_deref()),
    );

    run_validation_pass(input, state_machine);

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )
    .map_err(|err| miette!("failed to initialize file watcher: {err}"))?;

    for root in &watch_roots {
        watcher
            .watch(root, RecursiveMode::NonRecursive)
            .map_err(|err| miette!("failed to watch '{}': {err}", root.display()))?;
    }

    loop {
        let event = match rx.recv() {
            Ok(Ok(event)) => event,
            Ok(Err(err)) => {
                eprintln!("watch error: {err}");
                continue;
            }
            Err(err) => return Err(miette!("watch channel disconnected: {err}")),
        };

        if !should_revalidate(&event, &watched_paths) {
            continue;
        }

        while debounce_has_relevant_event(&rx, &watched_paths) {}

        println!("--- change detected, revalidating ---");
        run_validation_pass(input, state_machine);
    }
}

/// Run one validation pass in watch mode, writing any failure to stderr.
fn run_validation_pass(input: &Path, state_machine: Option<&Path>) {
    if let Err(err) = run_validation_once(input, state_machine) {
        eprintln!("{err:?}");
    }
}

fn debounce_has_relevant_event(
    rx: &mpsc::Receiver<notify::Result<Event>>,
    watched_paths: &[PathBuf],
) -> bool {
    match rx.recv_timeout(Duration::from_millis(250)) {
        Ok(Ok(event)) => should_revalidate(&event, watched_paths),
        Ok(Err(err)) => {
            eprintln!("watch error: {err}");
            false
        }
        Err(RecvTimeoutError::Timeout) => false,
        Err(RecvTimeoutError::Disconnected) => false,
    }
}

fn should_revalidate(event: &Event, watched_paths: &[PathBuf]) -> bool {
    if !is_relevant_event_kind(&event.kind) {
        return false;
    }

    event.paths.iter().any(|path| path_matches(path, watched_paths))
}

fn is_relevant_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
    )
}

fn path_matches(path: &Path, watched_paths: &[PathBuf]) -> bool {
    watched_paths.iter().any(|watched| paths_equivalent(path, watched))
}

fn paths_equivalent(candidate: &Path, watched: &Path) -> bool {
    if let Some(normalized_candidate) = normalize_path(candidate) {
        return normalized_candidate == watched;
    }

    let candidate_file_name = candidate.file_name();
    let watched_file_name = watched.file_name();

    candidate_file_name.is_some()
        && candidate_file_name == watched_file_name
        && candidate.components().last() == watched.components().last()
}

fn canonical_watched_paths(input: &Path, state_machine: &Path) -> Vec<PathBuf> {
    [input, state_machine]
        .into_iter()
        .map(|path| normalize_path(path).unwrap_or_else(|| path.to_path_buf()))
        .collect()
}

fn watch_roots(input: &Path, state_machine: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for path in [input, state_machine] {
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        let normalized = normalize_path(root).unwrap_or_else(|| root.to_path_buf());
        if !roots.iter().any(|existing| existing == &normalized) {
            roots.push(normalized);
        }
    }

    roots
}

fn normalize_path(path: &Path) -> Option<PathBuf> {
    path.canonicalize().ok()
}
