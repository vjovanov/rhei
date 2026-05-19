
/// Outcome of a single agent spawn cycle.
///
/// `timed_out` is set when the engine's watchdog fired before the agent
/// exited cleanly. The caller uses this to decide whether to route a
/// non-zero exit through the timeout transition path (with `triggeredBy:
/// 'system'` and `transitionData.timeout`) or through the generic non-zero
/// exit path.
#[derive(Debug, Clone)]
struct AgentSpawnOutcome {
    status: std::process::ExitStatus,
    timed_out: bool,
    timeout_secs: Option<u64>,
}

#[cfg(not(test))]
const AGENT_TERMINATE_GRACE: Duration = Duration::from_secs(10);
#[cfg(test)]
const AGENT_TERMINATE_GRACE: Duration = Duration::from_millis(50);
#[cfg(not(test))]
const AGENT_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(100);
#[cfg(test)]
const AGENT_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(20);

fn with_agent_log<T>(
    log_file: &Arc<Mutex<fs::File>>,
    write: impl FnOnce(&mut fs::File) -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut guard = log_file
        .lock()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "agent log lock poisoned"))?;
    write(&mut guard)
}

fn output_line(buf: &[u8]) -> String {
    let line = buf.strip_suffix(b"\n").unwrap_or(buf);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    String::from_utf8_lossy(line).into_owned()
}

fn agent_stream_label(stream: rhei_tui::AgentStream) -> &'static str {
    match stream {
        rhei_tui::AgentStream::Stdout => "stdout",
        rhei_tui::AgentStream::Stderr => "stderr",
    }
}

fn spawn_agent_output_reader<R>(
    reader: R,
    stream: rhei_tui::AgentStream,
    log_file: Arc<Mutex<fs::File>>,
    sink: Arc<dyn rhei_tui::EventSink>,
    slot: rhei_tui::Slot,
    task_id: String,
) -> std::thread::JoinHandle<std::io::Result<()>>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            let read = reader.read_until(b'\n', &mut buf)?;
            if read == 0 {
                break;
            }

            with_agent_log(&log_file, |f| {
                f.write_all(&buf)?;
                f.flush()
            })?;

            sink.emit(rhei_tui::RunEvent::AgentOutput {
                slot,
                task: task_id.clone(),
                stream,
                line: output_line(&buf),
                wall_clock: std::time::SystemTime::now(),
            });
        }
        Ok(())
    })
}

fn drain_agent_output_reader(
    handle: std::thread::JoinHandle<std::io::Result<()>>,
    stream: rhei_tui::AgentStream,
) -> MietteResult<()> {
    let deadline = Instant::now() + AGENT_OUTPUT_DRAIN_GRACE;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            // A descendant may still hold the inherited pipe open after the
            // direct agent process exits. Detach the reader instead of
            // blocking run completion forever; future bytes may still be
            // captured best-effort until process exit.
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    match handle.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => {
            Err(miette!("failed to capture agent {}: {err}", agent_stream_label(stream)))
        }
        Err(_) => Err(miette!("agent {} capture thread panicked", agent_stream_label(stream))),
    }
}

/// Spawn an agent, capture output to a log file, and wait with timeout.
///
/// Returns [`AgentSpawnOutcome`] describing the exit status and whether the
/// process was killed by the engine's timeout watchdog (so the caller can
/// route a SIGTERM-induced non-zero exit through the timeout transition
/// path rather than the generic non-zero exit path). `runtime_dir` is used
/// for the generated `mcp_config_flag` JSON file (see [`build_agent_command`]).
#[allow(clippy::too_many_arguments)]
fn spawn_and_wait_agent(
    resolved: &ResolvedAgent,
    prompt: &str,
    working_dir: &Path,
    plan_path: &Path,
    state_machine_path: Option<&Path>,
    task_id: &str,
    state_name: &str,
    tooling: &ResolvedTooling,
    log_path: &Path,
    runtime_dir: &Path,
    snapshot_preload: Option<&SnapshotPreload>,
    slot: rhei_tui::Slot,
    sink: Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<AgentSpawnOutcome> {
    // Ensure log directory exists.
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!("failed to create log directory '{}': {e}", parent.display()))?;
    }

    let log_file = Arc::new(Mutex::new(
        fs::File::create(log_path)
            .map_err(|e| miette!("failed to create log file '{}': {e}", log_path.display()))?,
    ));

    // §FS-rhei-agents.8: Agent log header format.

    // Write log header. The `v1` suffix is the structural-format version:
    // any future change to the header/footer layout must bump it.
    let started_wall = std::time::SystemTime::now();
    with_agent_log(&log_file, |f| {
        writeln!(f, "=== rhei agent log v1 ===")?;
        writeln!(f, "agent: {}", resolved.agent.id())?;
        if let Some(mode) = &resolved.mode {
            writeln!(f, "mode: {mode}")?;
        }
        if let Some(target) = &resolved.target {
            writeln!(f, "target: {}", target.selector())?;
        }
        if let Some(m) = &resolved.model {
            writeln!(f, "model: {m}")?;
        }
        if let Some(provider) = &resolved.model_provider {
            writeln!(f, "provider: {provider}")?;
        }
        if let Some(model_name) = &resolved.model_name {
            writeln!(f, "model_name: {model_name}")?;
        }
        writeln!(f, "task: {task_id}")?;
        writeln!(f, "state: {state_name}")?;
        writeln!(f, "started: {}", format_iso8601_utc(started_wall))?;
        if let Some(t) = resolved.timeout_secs {
            writeln!(f, "timeout: {}", format_duration_human(t))?;
        }
        writeln!(f, "plan: {}", plan_path.display())?;
        let mcp_line = format_tooling_log_line(&tooling.mcp_servers, |e| {
            (e.id.as_str(), e.optional, e.definition.is_some())
        });
        if let Some(line) = mcp_line {
            writeln!(f, "mcp_servers: {line}")?;
        }
        let skill_line = format_tooling_log_line(&tooling.skills, |e| {
            (e.id.as_str(), e.optional, e.definition.is_some())
        });
        if let Some(line) = skill_line {
            writeln!(f, "skills: {line}")?;
        }
        writeln!(f, "===\n")?;
        f.flush()
    })
    .map_err(|e| miette!("failed to write log header '{}': {e}", log_path.display()))?;

    // Emit spawn-time warnings for tooling the agent profile cannot wire.
    // §FS-rhei-agents.1.1.5 §FS-rhei-agents.6: Spawn-time tooling warnings.
    for warning in collect_unsupported_tooling_warnings(resolved, tooling) {
        let _ = with_agent_log(&log_file, |f| writeln!(f, "{warning}"));
        eprintln!("{warning}");
    }

    let mut cmd = build_agent_command(
        resolved,
        prompt,
        working_dir,
        plan_path,
        state_machine_path,
        task_id,
        state_name,
        tooling,
        runtime_dir,
    );
    if let Some(snapshot_preload) = snapshot_preload {
        for arg in &snapshot_preload.extra_args {
            cmd.arg(arg);
        }
        if let Some(session_dir) = snapshot_preload.session_dir.as_ref() {
            cmd.env("RHEI_SNAPSHOT_SESSION_DIR", session_dir);
        }
        if let Some(parent_ref) = snapshot_preload.parent_ref.as_ref() {
            cmd.env("RHEI_SNAPSHOT_PARENT_REF", parent_ref.to_string());
        }
    }
    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());

    let mut child =
        cmd.spawn().map_err(|e| miette!("failed to spawn agent '{}': {e}", resolved.agent.id()))?;

    let stdout_handle = child.stdout.take().map(|stdout| {
        spawn_agent_output_reader(
            stdout,
            rhei_tui::AgentStream::Stdout,
            log_file.clone(),
            sink.clone(),
            slot,
            task_id.to_string(),
        )
    });
    let stderr_handle = child.stderr.take().map(|stderr| {
        spawn_agent_output_reader(
            stderr,
            rhei_tui::AgentStream::Stderr,
            log_file.clone(),
            sink.clone(),
            slot,
            task_id.to_string(),
        )
    });

    // If stdin_prompt, write prompt to stdin.
    if resolved.profile.stdin_prompt {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write as _;
            let _ = stdin.write_all(prompt.as_bytes());
            drop(stdin);
        }
    }

    let start = Instant::now();
    let mut timed_out = false;

    // Wait with optional timeout.
    let status = if let Some(timeout_secs) = resolved.timeout_secs {
        let timeout = Duration::from_secs(timeout_secs);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    if start.elapsed() > timeout {
                        timed_out = true;
                        // Send SIGTERM.
                        let pid = Pid::from_raw(child.id() as i32);
                        let _ = signal::kill(pid, Signal::SIGTERM);
                        // Grace period.
                        std::thread::sleep(AGENT_TERMINATE_GRACE);
                        match child.try_wait() {
                            Ok(Some(status)) => break Ok(status),
                            _ => {
                                let _ = child.kill(); // SIGKILL
                                break child.wait().map_err(|e| {
                                    miette!("failed to wait for agent after kill: {e}")
                                });
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
                Err(e) => break Err(miette!("error waiting for agent: {e}")),
            }
        }
    } else {
        child.wait().map_err(|e| miette!("failed to wait for agent: {e}"))
    }?;

    if let Some(handle) = stdout_handle {
        drain_agent_output_reader(handle, rhei_tui::AgentStream::Stdout)?;
    }
    if let Some(handle) = stderr_handle {
        drain_agent_output_reader(handle, rhei_tui::AgentStream::Stderr)?;
    }

    // Write log footer. The `ended:` ISO timestamp and human-readable
    // `duration:` mirror the header. When the engine
    // killed the agent for exceeding its timeout, we emit the spec-required
    // `agent timed out after {duration}` line so operators see the cause
    // without inferring it from the exit code alone.

    // §FS-rhei-agents.8: Agent log footer and timeout cause.
    let timeout_message =
        if timed_out { resolved.timeout_secs.map(format_duration_human) } else { None };
    with_agent_log(&log_file, |f| {
        if let Some(duration) = &timeout_message {
            writeln!(f, "\nagent timed out after {duration}")?;
            writeln!(f, "\n=== exit ===")?;
        } else {
            writeln!(f, "\n=== exit ===")?;
        }
        writeln!(f, "code: {}", status.code().unwrap_or(-1))?;
        let elapsed = start.elapsed();
        writeln!(f, "duration: {}", format_duration_human(elapsed.as_secs()))?;
        writeln!(f, "ended: {}", format_iso8601_utc(std::time::SystemTime::now()))?;
        if timed_out {
            writeln!(f, "timed_out: true")?;
        }
        writeln!(f, "===")?;
        f.flush()
    })
    .map_err(|e| miette!("failed to append to log file '{}': {e}", log_path.display()))?;

    Ok(AgentSpawnOutcome { status, timed_out, timeout_secs: resolved.timeout_secs })
}
