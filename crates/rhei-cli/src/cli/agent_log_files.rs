// How `rhei run` names the transcript of one agent invocation, and how anything
// that later wants to read one finds it.
//
// Four sides have to agree on the rule: the spawn that opens the file, the
// prompt that cites an earlier visit's, the reset that sweeps a ticket's
// runtime, and the engine's own account of a ticket it finished without
// spawning a worker. A second copy of the rule is how one of them drifts, so
// the rule lives here and only here.
//
// The name says which invocation and which attempt; *which* attempt is a fact
// about the ticket's history, not about the directory listing, and it is kept
// in the spawn record next door.

// §AR-source-file-size.3 §FS-rhei-agents.8.1 §FS-rhei-memory.4.4 §FS-rhei-run.3

fn resolved_agent_log_suffix(resolved: &ResolvedAgent, visit_count: Option<u64>) -> Option<String> {
    agent_log_suffix(resolved.target.as_ref(), resolved.model.as_deref(), visit_count)
}

/// The part of a log file name that follows `task-{task_id}-{state}`.
///
/// Split from the resolved-agent form so prompt composition can name the log of
/// an *earlier* visit, where no `ResolvedAgent` for that visit exists — only
/// the identity this one carries, which is the identity that wrote it.
// §FS-rhei-agents.8.1 §FS-rhei-memory.4.4
fn agent_log_suffix(
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
    visit_count: Option<u64>,
) -> Option<String> {
    let base = target
        .map(ExecutionTarget::slug)
        .or_else(|| model.map(str::to_string).filter(|value| !value.is_empty()));
    let visit_suffix = visit_count.filter(|count| *count > 1).map(|count| count.to_string());
    match (base, visit_suffix) {
        (Some(base), Some(visit)) => Some(format!("{base}-{visit}")),
        (Some(base), None) => Some(base),
        (None, Some(visit)) => Some(visit),
        (None, None) => None,
    }
}

/// The log file of one attempt at one visit of a state.
///
/// The first attempt of a visit keeps the plain `task-{id}-{state}{suffix}.log`
/// name every other reader already knows; a re-spawn *within the same visit*
/// appends `-attempt{n}` instead of truncating the file that says why the
/// attempt before it did not finish. The visit count cannot do that job on its
/// own: a ticket that stalls never leaves the state, so it is still on the same
/// visit when the run spawns it again — and, the other way round, a ticket that
/// leaves and returns is on a new visit that the count does not register
/// either, so it starts again at `-attempt1`'s plain name.
// §FS-rhei-agents.8.1
fn agent_log_attempt_path(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    suffix: Option<&str>,
    attempt: u64,
) -> PathBuf {
    let suffix = suffix
        .filter(|value| !value.is_empty())
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    let attempt = if attempt > 1 { format!("-attempt{attempt}") } else { String::new() };
    runtime_dir.join("logs").join(format!("task-{task_id}-{state_name}{suffix}{attempt}.log"))
}

/// The log of the *last thing that actually ran* for one invocation — whichever
/// attempt that was — and `None` when nothing did.
///
/// Read from the spawn record rather than by probing names: probing costs one
/// `exists()` per attempt ever made, and it names attempt files a spawn opened
/// and never wrote a line into. The unsuffixed name is still accepted when no
/// record answers, because a runtime written before records existed still has
/// transcripts worth citing, and citing a transcript is not a claim that a
/// worker ran — that claim has one source, and it is the record.
// §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
fn latest_agent_log_path(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    suffix: Option<&str>,
) -> Option<PathBuf> {
    let record = spawn_record_path(runtime_dir, task_id, state_name, suffix);
    if let Some(record) = read_spawn_record(&record) {
        if record.log.exists() {
            return Some(record.log);
        }
    }
    let first = agent_log_attempt_path(runtime_dir, task_id, state_name, suffix, 1);
    first.exists().then_some(first)
}
