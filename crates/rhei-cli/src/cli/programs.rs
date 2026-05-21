fn program_log_path(runtime_dir: &Path, task_id: &str, state_name: &str) -> PathBuf {
    runtime_dir.join("logs").join(format!("task-{task_id}-{state_name}.log"))
}

#[derive(Debug, Clone)]
struct ProgramSpawnOutcome {
    status: std::process::ExitStatus,
    timed_out: bool,
    timeout_secs: Option<u64>,
}

#[cfg(not(test))]
const PROGRAM_TERMINATE_GRACE: Duration = Duration::from_secs(10);
#[cfg(test)]
const PROGRAM_TERMINATE_GRACE: Duration = Duration::from_millis(50);

fn build_program_command(
    resolved: &ResolvedProgram,
    render_context: &RuntimeTemplateContext<'_>,
) -> MietteResult<std::process::Command> {
    let working_dir = resolved
        .program
        .working_directory
        .as_deref()
        .map(|path| resolve_runtime_template_text(path, render_context))
        .map(|path| render_context.workspace_root.join(path))
        .unwrap_or_else(|| render_context.workspace_root.to_path_buf());

    let mut cmd = if resolved.program.shell {
        let mut command = std::process::Command::new("sh");
        command.arg("-c");
        let rendered = match &resolved.program.command {
            ProgramCommand::Shell(command) => {
                resolve_runtime_template_text(command, render_context)
            }
            ProgramCommand::Exec(args) => args
                .iter()
                .map(|arg| resolve_runtime_template_text(arg, render_context))
                .collect::<Vec<_>>()
                .join(" "),
        };
        command.arg(rendered);
        command
    } else {
        let ProgramCommand::Exec(args) = &resolved.program.command else {
            return Err(miette!("program command cannot disable shell when command is a string"));
        };
        let rendered = args
            .iter()
            .map(|arg| resolve_runtime_template_text(arg, render_context))
            .collect::<Vec<_>>();
        let mut command = std::process::Command::new(&rendered[0]);
        for arg in &rendered[1..] {
            command.arg(arg);
        }
        command
    };

    cmd.current_dir(working_dir)
        .env("RHEI_PLAN_PATH", render_context.plan_path)
        .env("RHEI_TASK_ID", render_context.task.id.to_string())
        .env("RHEI_STATE", render_context.state_name)
        .env(
            "RHEI_VISIT_COUNT",
            render_visit_count(
                render_context.metadata,
                &render_context.task.id,
                render_context.state_name,
                render_context.current_state_raw,
                render_context.machine,
            )
            .to_string(),
        );
    if let Some(path) = render_context.state_machine_path {
        cmd.env("RHEI_STATE_MACHINE_PATH", path);
    }
    if let Some(model) = render_context.model {
        cmd.env("RHEI_MODEL", model);
    }

    // Expose declared input artifact paths and existence flags so programs
    // can branch on optional inputs without shelling out to test -f.
    let input_visit_count = Some(render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    ));
    if let Some(state_def) = render_context.machine.states.get(render_context.state_name) {
        for artifact in &state_def.inputs {
            let env_base = artifact.name.to_uppercase().replace(['-', ' '], "_");
            let (relative, path) = resolve_artifact_path(
                render_context.workspace_root,
                artifact,
                &render_context.task.id.to_string(),
                render_context.state_name,
                input_visit_count,
                render_context.target,
                render_context.model,
                render_context.model_provider,
                render_context.model_name,
                render_context.agent,
                render_context.agent_mode,
            );
            if artifact_relative_path_escapes_root(&relative) {
                return Err(miette!(
                    "input artifact '{}' expands to '{}' which escapes the workspace root",
                    artifact.name,
                    relative
                ));
            }
            cmd.env(format!("RHEI_INPUT_{env_base}_EXISTS"), path.exists().to_string());
            cmd.env(format!("RHEI_INPUT_{env_base}_PATH"), relative);
        }
    }

    for (key, value) in &resolved.program.env {
        cmd.env(key, resolve_runtime_template_text(value, render_context));
    }

    Ok(cmd)
}

fn spawn_and_wait_program(
    resolved: &ResolvedProgram,
    render_context: &RuntimeTemplateContext<'_>,
    log_path: &Path,
) -> MietteResult<ProgramSpawnOutcome> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create log directory '{}': {e}", parent.display()))?;
    }

    let log_file = fs::File::create(log_path)
        .map_err(|e| miette!("failed to create log file '{}': {e}", log_path.display()))?;
    {
        use std::io::Write as _;
        let mut f = &log_file;
        let command_label = match &resolved.program.command {
            ProgramCommand::Shell(command) => {
                resolve_runtime_template_text(command, render_context)
            }
            ProgramCommand::Exec(args) => args
                .iter()
                .map(|arg| resolve_runtime_template_text(arg, render_context))
                .collect::<Vec<_>>()
                .join(" "),
        };
        let _ = writeln!(f, "=== rhei program log v1 ===");
        let _ = writeln!(f, "program: {command_label}");
        let _ = writeln!(f, "task: {}", render_context.task.id);
        let _ = writeln!(f, "state: {}", render_context.state_name);
        if let Some(timeout) = resolved.timeout_secs {
            let _ = writeln!(f, "timeout: {timeout}s");
        }
        let _ = writeln!(f, "plan: {}", render_context.plan_path.display());
        let _ = writeln!(f, "===\n");
    }

    let log_stdout =
        log_file.try_clone().map_err(|e| miette!("failed to clone log file handle: {e}"))?;
    let log_stderr =
        log_file.try_clone().map_err(|e| miette!("failed to clone log file handle: {e}"))?;
    let mut cmd = build_program_command(resolved, render_context)?;
    cmd.stdout(log_stdout).stderr(log_stderr);
    let mut child = cmd.spawn().map_err(|e| miette!("failed to spawn program: {e}"))?;
    let start = Instant::now();
    let mut timed_out = false;

    let status = if let Some(timeout_secs) = resolved.timeout_secs {
        let timeout = Duration::from_secs(timeout_secs);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    if start.elapsed() > timeout {
                        timed_out = true;
                        terminate_child_gracefully(&mut child);
                        std::thread::sleep(PROGRAM_TERMINATE_GRACE);
                        match child.try_wait() {
                            Ok(Some(status)) => break Ok(status),
                            _ => {
                                let _ = child.kill();
                                break child.wait().map_err(|e| {
                                    miette!("failed to wait for program after kill: {e}")
                                });
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => break Err(miette!("error waiting for program: {e}")),
            }
        }
    } else {
        child.wait().map_err(|e| miette!("failed to wait for program: {e}"))
    }?;

    {
        use std::io::Write as _;
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(log_path)
            .map_err(|e| miette!("failed to append to log file: {e}"))?;
        if timed_out {
            if let Some(timeout_secs) = resolved.timeout_secs {
                let _ = writeln!(
                    f,
                    "\nprogram timed out after {}",
                    format_duration_human(timeout_secs)
                );
            }
            let _ = writeln!(f, "\n=== exit ===");
        } else {
            let _ = writeln!(f, "\n=== exit ===");
        }
        let _ = writeln!(f, "code: {}", status.code().unwrap_or(-1));
        let _ = writeln!(f, "duration: {}s", start.elapsed().as_secs());
        if timed_out {
            let _ = writeln!(f, "timed_out: true");
        }
        let _ = writeln!(f, "===");
    }

    Ok(ProgramSpawnOutcome { status, timed_out, timeout_secs: resolved.timeout_secs })
}

fn transition_matches_exit_code(rule: &rhei_core::ast::TransitionRule, exit_code: i32) -> bool {
    match rule.exit_code.as_ref() {
        Some(YamlValue::Number(number)) => number.as_i64() == Some(i64::from(exit_code)),
        Some(YamlValue::Sequence(values)) => {
            values.iter().filter_map(YamlValue::as_i64).any(|value| value == i64::from(exit_code))
        }
        Some(YamlValue::String(value)) if value == "nonzero" => exit_code != 0,
        _ => false,
    }
}

fn transition_has_exact_exit_code(rule: &rhei_core::ast::TransitionRule) -> bool {
    matches!(rule.exit_code, Some(YamlValue::Number(_)) | Some(YamlValue::Sequence(_)))
}

fn transition_is_nonzero_exit_code(rule: &rhei_core::ast::TransitionRule) -> bool {
    matches!(rule.exit_code, Some(YamlValue::String(ref value)) if value == "nonzero")
}

fn program_transition_is_applicable(
    rule: &rhei_core::ast::TransitionRule,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    current_state: &str,
) -> bool {
    transition_rule_is_applicable(
        rule,
        machine,
        metadata,
        &task.id,
        current_state,
        task.state.as_str(),
    )
    .unwrap_or(false)
}

fn find_program_exit_transition(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    current_state: &str,
    exit_code: i32,
) -> MietteResult<Option<String>> {
    let applicable_exact_match_exists = exit_code != 0
        && machine
            .transitions()
            .iter()
            .filter(|rule| rule.from.0 == current_state)
            .filter(|rule| transition_has_exact_exit_code(rule))
            .filter(|rule| transition_matches_exit_code(rule, exit_code))
            .any(|rule| {
                program_transition_is_applicable(rule, machine, metadata, task, current_state)
            });

    let ordered_match = machine
        .transitions()
        .iter()
        .filter(|rule| rule.from.0 == current_state)
        .filter(|rule| {
            rule.exit_code.is_none()
                || transition_matches_exit_code(rule, exit_code)
        });

    for rule in ordered_match {
        if applicable_exact_match_exists && transition_is_nonzero_exit_code(rule) {
            continue;
        }
        if program_transition_is_applicable(rule, machine, metadata, task, current_state) {
            return Ok(Some(rule.to.0.clone()));
        }
    }

    Ok(None)
}
