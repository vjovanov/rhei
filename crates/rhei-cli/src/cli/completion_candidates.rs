fn completions_command(
    shell: CompletionShell,
    install: bool,
    system: bool,
    output: Option<&Path>,
    dry_run: bool,
) -> MietteResult<()> {
    if install || output.is_some() || dry_run {
        let path = match output {
            Some(path) => path.to_path_buf(),
            None => completion_install_path(shell, system)?,
        };
        if dry_run {
            println!("Would install {} completions to {}", shell.as_str(), path.display());
            return Ok(());
        }

        write_completion_file(shell, &path)?;
        println!("Installed {} completions to {}", shell.as_str(), path.display());
        return Ok(());
    }

    let mut stdout = std::io::stdout();
    write_completion_registration(shell, &mut stdout)?;
    Ok(())
}

fn write_completion_file(shell: CompletionShell, path: &Path) -> MietteResult<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create completions directory", err))?;
    }

    let mut buffer = Vec::new();
    write_completion_registration(shell, &mut buffer)?;

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|err| {
        file_io_report(parent, "failed to create temporary completions file", err)
    })?;
    temp.write_all(&buffer)
        .map_err(|err| file_io_report(path, "failed to write completions file", err))?;
    temp.flush().map_err(|err| file_io_report(path, "failed to flush completions file", err))?;
    temp.persist(path)
        .map_err(|err| file_io_report(path, "failed to install completions file", err.error))?;
    Ok(())
}

fn write_completion_registration(
    shell: CompletionShell,
    writer: &mut dyn std::io::Write,
) -> MietteResult<()> {
    let command = cli_command();
    let completer = std::env::current_exe()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .unwrap_or_else(|| "rhei".to_string());
    completion_env_completer(shell)
        .write_registration("COMPLETE", command.get_name(), "rhei", &completer, writer)
        .map_err(|err| miette!("failed to generate {} completions: {err}", shell.as_str()))
}

fn completion_env_completer(shell: CompletionShell) -> &'static dyn EnvCompleter {
    match shell {
        CompletionShell::Bash => &CompletionBash,
        CompletionShell::Zsh => &CompletionZsh,
        CompletionShell::Fish => &CompletionFish,
        CompletionShell::PowerShell => &CompletionPowerShell,
        CompletionShell::Elvish => &CompletionElvish,
    }
}

fn completion_install_path(shell: CompletionShell, system: bool) -> MietteResult<PathBuf> {
    if system {
        return Ok(match shell {
            CompletionShell::Bash => {
                PathBuf::from("/usr/local/share/bash-completion/completions/rhei")
            }
            CompletionShell::Zsh => PathBuf::from("/usr/local/share/zsh/site-functions/_rhei"),
            CompletionShell::Fish => {
                PathBuf::from("/usr/local/share/fish/vendor_completions.d/rhei.fish")
            }
            CompletionShell::PowerShell => {
                PathBuf::from("/usr/local/share/powershell/Completions/rhei-completions.ps1")
            }
            CompletionShell::Elvish => {
                PathBuf::from("/usr/local/share/elvish/lib/rhei-completions.elv")
            }
        });
    }

    Ok(match shell {
        CompletionShell::Bash => xdg_data_home()?.join("bash-completion/completions/rhei"),
        CompletionShell::Zsh => home_dir()?.join(".zfunc/_rhei"),
        CompletionShell::Fish => xdg_config_home()?.join("fish/completions/rhei.fish"),
        CompletionShell::PowerShell => xdg_config_home()?.join("powershell/rhei-completions.ps1"),
        CompletionShell::Elvish => xdg_config_home()?.join("elvish/lib/rhei-completions.elv"),
    })
}

fn complete_any_path(current: &OsStr) -> Vec<CompletionCandidate> {
    PathCompleter::any().complete(current)
}

fn complete_yaml_path(current: &OsStr) -> Vec<CompletionCandidate> {
    complete_path_with_extensions(current, &["yaml", "yml"])
}

fn complete_values_path(current: &OsStr) -> Vec<CompletionCandidate> {
    complete_path_with_extensions(current, &["yaml", "yml", "json"])
}

fn complete_rhei_plan_path(current: &OsStr) -> Vec<CompletionCandidate> {
    let current_path = Path::new(current);
    let parent = current_path.parent().filter(|p| !p.as_os_str().is_empty());
    let file_prefix =
        current_path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    let dir = parent.unwrap_or_else(|| Path::new("."));
    let mut candidates = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&file_prefix) {
                continue;
            }
            let include = if path.is_dir() { true } else { name.ends_with(".rhei.md") };
            if include {
                candidates.push(path_completion_candidate(parent, &name, path.is_dir()));
            }
        }
    }

    candidates
}

fn complete_path_with_extensions(current: &OsStr, extensions: &[&str]) -> Vec<CompletionCandidate> {
    let current_path = Path::new(current);
    let parent = current_path.parent().filter(|p| !p.as_os_str().is_empty());
    let file_prefix =
        current_path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();
    let dir = parent.unwrap_or_else(|| Path::new("."));
    let mut candidates = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&file_prefix) {
                continue;
            }
            let include = if path.is_dir() {
                true
            } else {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| extensions.iter().any(|allowed| ext == *allowed))
            };
            if include {
                candidates.push(path_completion_candidate(parent, &name, path.is_dir()));
            }
        }
    }

    candidates
}

fn path_completion_candidate(
    parent: Option<&Path>,
    name: &str,
    is_dir: bool,
) -> CompletionCandidate {
    let mut value = parent.map(|p| p.join(name)).unwrap_or_else(|| PathBuf::from(name));
    if is_dir {
        value.push("");
    }
    CompletionCandidate::new(value.into_os_string())
}

fn complete_template_source(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("all", "Project and user templates"),
            ("project", "Project templates only"),
            ("user", "User templates only"),
        ],
    )
}

fn complete_parallel(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("1", "One task at a time"),
            ("2", "Two concurrent tasks"),
            ("4", "Four concurrent tasks"),
            ("8", "Eight concurrent tasks"),
            ("0", "Unlimited concurrency"),
        ],
    )
}

fn complete_duration(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("30s", "Thirty seconds"),
            ("1m", "One minute"),
            ("5m", "Five minutes"),
            ("15m", "Fifteen minutes"),
            ("1h", "One hour"),
        ],
    )
}

fn complete_limit(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("10", "Ten tasks"),
            ("25", "Twenty-five tasks"),
            ("50", "Fifty tasks"),
            ("100", "One hundred tasks"),
            ("0", "No limit"),
        ],
    )
}

fn complete_skill_name(current: &OsStr) -> Vec<CompletionCandidate> {
    static_completion(
        current,
        &[
            ("rhei-plan-writer", "Create and refactor Rhei Plan documents"),
            ("rhei-plan-worker", "Execute tasks in Rhei Plan documents"),
            ("rhei-state-machine-writer", "Design custom Rhei state machines"),
        ],
    )
}

fn static_completion(current: &OsStr, values: &[(&str, &str)]) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    values
        .iter()
        .filter(|(value, _)| value.starts_with(prefix.as_ref()))
        .map(|(value, help)| {
            CompletionCandidate::new((*value).to_string()).help(Some((*help).to_string().into()))
        })
        .collect()
}

fn complete_agent_name(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let settings = load_merged_settings_for_completion(&completion_workspace_root());
    settings
        .agents
        .keys()
        .filter(|name| name.starts_with(prefix.as_ref()))
        .map(|name| CompletionCandidate::new(name.clone()).help(Some("Configured agent".into())))
        .collect()
}

fn complete_agent_mode(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let settings = load_merged_settings_for_completion(&completion_workspace_root());
    let selected_agent = completion_option_value("agent");
    let mut modes = BTreeSet::new();
    if let Some(agent) = selected_agent.as_deref().and_then(|agent| settings.agents.get(agent)) {
        modes.extend(agent.modes.keys().cloned());
    } else {
        for agent in settings.agents.values() {
            modes.extend(agent.modes.keys().cloned());
        }
    }
    modes
        .into_iter()
        .filter(|mode| mode.starts_with(prefix.as_ref()))
        .map(|mode| CompletionCandidate::new(mode).help(Some("Agent mode".into())))
        .collect()
}

fn complete_model_name(current: &OsStr) -> Vec<CompletionCandidate> {
    let prefix = current.to_string_lossy();
    let mut models = BTreeSet::new();
    if let Some(model) = load_merged_settings_for_completion(&completion_workspace_root()).model {
        models.insert(model);
    }
    if let Some(machine) = completion_state_machine() {
        models.extend(machine.models);
        for state in machine.states.values() {
            if let Some(model) = state.model.as_ref() {
                models.insert(model.clone());
            }
            models.extend(state.all_models.iter().cloned());
        }
    }
    models
        .into_iter()
        .filter(|model| model.starts_with(prefix.as_ref()))
        .map(|model| CompletionCandidate::new(model).help(Some("Configured model".into())))
        .collect()
}

fn complete_assignee(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(plan) = completion_plan_path() else {
        return Vec::new();
    };
    let prefix = current.to_string_lossy();
    let Ok(loaded) = load_plan(&plan) else {
        return Vec::new();
    };
    let mut counts = BTreeMap::<String, usize>::new();
    for task in flatten_tasks(&loaded.rhei) {
        if let Some(assignee) = &task.assignee {
            *counts.entry(assignee.clone()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(assignee, _)| assignee.starts_with(prefix.as_ref()))
        .map(|(assignee, count)| {
            CompletionCandidate::new(assignee).help(Some(task_count_help(count).into()))
        })
        .collect()
}

fn complete_node_kind(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(plan) = completion_plan_path() else {
        return Vec::new();
    };
    let prefix = current.to_string_lossy().to_ascii_lowercase();
    let Ok(loaded) = load_plan(&plan) else {
        return Vec::new();
    };
    let mut counts = BTreeMap::<String, usize>::new();
    for task in flatten_tasks(&loaded.rhei) {
        *counts.entry(task.kind.clone()).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(kind, _)| kind.starts_with(&prefix))
        .map(|(kind, count)| {
            CompletionCandidate::new(kind).help(Some(task_count_help(count).into()))
        })
        .collect()
}

fn task_count_help(count: usize) -> String {
    match count {
        1 => "1 matching task".to_string(),
        n => format!("{n} matching tasks"),
    }
}

fn complete_task_id(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(plan) = completion_plan_path() else {
        return Vec::new();
    };
    let prefix = current.to_string_lossy();
    let Ok(loaded) = load_plan(&plan) else {
        return Vec::new();
    };
    flatten_tasks(&loaded.rhei)
        .into_iter()
        .filter_map(|task| {
            let id = task.id.to_string();
            id.starts_with(prefix.as_ref()).then(|| {
                CompletionCandidate::new(id)
                    .help(Some(format!("{} [{}]", task.title, task.state).into()))
            })
        })
        .collect()
}

fn complete_transition_from_state(current: &OsStr) -> Vec<CompletionCandidate> {
    if let (Some(plan), Some(task_id)) = (completion_plan_path(), completion_option_value("task")) {
        if let Ok(state) = current_task_state(&plan, &task_id) {
            if state.starts_with(current.to_string_lossy().as_ref()) {
                return vec![
                    CompletionCandidate::new(state).help(Some("Current task state".into()))
                ];
            }
        }
    }
    complete_state_name(current)
}

fn complete_transition_to_state(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(machine) = completion_state_machine() else {
        return Vec::new();
    };
    let from = completion_option_value("from").or_else(|| {
        completion_plan_path()
            .zip(completion_option_value("task"))
            .and_then(|(plan, task)| current_task_state(&plan, &task).ok())
    });
    let mut targets = BTreeSet::new();
    if let Some(from) = from {
        let normalized = normalized_state_name(&from, &machine);
        for rule in machine.transitions() {
            if rule.from.0 == normalized || rule.from.0 == "*" {
                targets.insert(rule.to.0.clone());
            }
        }
    } else {
        targets.extend(machine.states.keys().cloned());
    }
    let prefix = current.to_string_lossy();
    targets
        .into_iter()
        .filter(|state| state.starts_with(prefix.as_ref()))
        .map(|state| {
            let help = machine.states.get(&state).and_then(|def| def.description.clone());
            CompletionCandidate::new(state).help(help.map(Into::into))
        })
        .collect()
}

fn complete_comma_state_name(current: &OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let (base, prefix) = match current.rsplit_once(',') {
        Some((base, prefix)) => (format!("{base},"), prefix),
        None => (String::new(), current.as_ref()),
    };
    complete_state_name_with_prefix(prefix)
        .into_iter()
        .map(|(state, help)| {
            CompletionCandidate::new(format!("{base}{state}")).help(help.map(Into::into))
        })
        .collect()
}

fn complete_state_name(current: &OsStr) -> Vec<CompletionCandidate> {
    complete_state_name_with_prefix(current.to_string_lossy().as_ref())
        .into_iter()
        .map(|(state, help)| CompletionCandidate::new(state).help(help.map(Into::into)))
        .collect()
}

fn complete_state_name_with_prefix(prefix: &str) -> Vec<(String, Option<String>)> {
    let Some(machine) = completion_state_machine() else {
        return Vec::new();
    };
    machine
        .states
        .iter()
        .filter(|(state, _)| state.starts_with(prefix))
        .map(|(state, def)| (state.clone(), def.description.clone()))
        .collect()
}

fn completion_state_machine() -> Option<rhei_validator::StateMachine> {
    let state_machine = completion_option_value("state-machine").map(PathBuf::from);
    let plan = completion_plan_path();
    match (plan.as_deref(), state_machine.as_deref()) {
        (Some(plan), sm) => load_plan(plan)
            .ok()
            .and_then(|loaded| resolve_state_machine_for_loaded_plan(plan, &loaded, sm).ok())
            .map(|resolved| resolved.machine),
        (None, Some(sm)) => load_state_machine(Some(sm)).ok(),
        (None, None) => Some(rhei_validator::StateMachine::builtin_default()),
    }
}

fn completion_workspace_root() -> PathBuf {
    completion_plan_path()
        .map(|path| execution_workspace_root(&path))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}
