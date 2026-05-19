struct SnapshotCommandContext {
    workspace_root: PathBuf,
    cache_root: PathBuf,
    loaded: LoadedPlan,
    machine: rhei_validator::StateMachine,
    settings: RheiSettings,
}

#[derive(Clone, Debug)]
struct SnapshotRecord {
    path: PathBuf,
    manifest: serde_json::Value,
    task_id: String,
    snapshot_name: String,
    emitting_state: String,
    visit: u64,
    target_slug: String,
    generation: u64,
    created_at: String,
    transcript_bytes: u64,
    completion: String,
    produced_by: String,
    is_current: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SnapshotIdentity {
    task_id: String,
    snapshot_name: String,
    emitting_state: String,
    visit: u64,
    target_slug: String,
}

#[derive(Clone, Debug, Default)]
struct SnapshotPreload {
    parent_ref: Option<serde_json::Value>,
    extra_args: Vec<String>,
    session_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapshotCompletion {
    Success,
    Failure,
    Timeout,
}

impl SnapshotCompletion {
    fn as_str(self) -> &'static str {
        match self {
            SnapshotCompletion::Success => "success",
            SnapshotCompletion::Failure => "failure",
            SnapshotCompletion::Timeout => "timeout",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum SnapshotProducedBy {
    Orchestrator,
    Operator,
}

impl SnapshotProducedBy {
    fn as_str(self) -> &'static str {
        match self {
            SnapshotProducedBy::Orchestrator => "orchestrator",
            SnapshotProducedBy::Operator => "operator",
        }
    }
}

impl SnapshotRecord {
    fn identity(&self) -> SnapshotIdentity {
        SnapshotIdentity {
            task_id: self.task_id.clone(),
            snapshot_name: self.snapshot_name.clone(),
            emitting_state: self.emitting_state.clone(),
            visit: self.visit,
            target_slug: self.target_slug.clone(),
        }
    }

    fn display_ref(&self) -> String {
        format!(
            "{}:{}:{}@{}:{}/g{}",
            self.task_id,
            self.snapshot_name,
            self.emitting_state,
            self.visit,
            self.target_slug,
            self.generation
        )
    }

    fn transcript_path(&self) -> PathBuf {
        let relative = self
            .manifest
            .get("transcript_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("transcript.jsonl");
        self.path.join(relative)
    }

    fn to_listing_json(&self, orphaned: bool) -> serde_json::Value {
        serde_json::json!({
            "task_id": self.task_id,
            "snapshot_name": self.snapshot_name,
            "emitting_state": self.emitting_state,
            "visit": self.visit,
            "target_slug": self.target_slug,
            "generation": self.generation,
            "created_at": self.created_at,
            "transcript_bytes": self.transcript_bytes,
            "completion": self.completion,
            "produced_by": self.produced_by,
            "current": self.is_current,
            "orphaned": orphaned,
            "path": self.path.display().to_string(),
        })
    }
}

fn snapshot_parent_ref(record: &SnapshotRecord) -> serde_json::Value {
    serde_json::json!({
        "task_id": record.task_id,
        "snapshot_name": record.snapshot_name,
        "emitting_state": record.emitting_state,
        "visit": record.visit,
        "target_slug": record.target_slug,
        "generation": record.generation,
    })
}

fn snapshot_session(resolved: &ResolvedAgent) -> Option<&serde_json::Value> {
    resolved.profile.session.as_ref()
}

fn snapshot_session_layout(session: &serde_json::Value) -> Option<&serde_json::Value> {
    session.get("layout")
}

fn snapshot_layout_kind(layout: &serde_json::Value) -> Option<String> {
    layout.get("kind").and_then(serde_json::Value::as_str).map(|kind| match kind {
        "flat_by_id" | "flat-by-id" | "FlatById" => "FlatById".to_string(),
        "per_project_json" | "per-project-json" | "PerProjectJson" => "PerProjectJson".to_string(),
        other => other.to_string(),
    })
}

fn snapshot_layout_ext(layout: &serde_json::Value) -> Option<String> {
    layout
        .get("ext")
        .or_else(|| layout.get("extension"))
        .and_then(serde_json::Value::as_str)
        .map(|ext| ext.trim_start_matches('.').to_string())
}

fn snapshot_session_string(session: &serde_json::Value, key: &str) -> Option<String> {
    session.get(key).and_then(serde_json::Value::as_str).map(str::to_string)
}

fn snapshot_strategy_flag(session: &serde_json::Value, key: &str) -> Option<String> {
    match session.get(key)? {
        serde_json::Value::String(value) if value != "none" && !value.trim().is_empty() => {
            Some(value.clone())
        }
        serde_json::Value::Object(map) => map
            .get("flag")
            .or_else(|| map.get("native").and_then(|value| value.get("flag")))
            .or_else(|| map.get("copy_and_resume").and_then(|value| value.get("flag")))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn snapshot_resume_supported(session: &serde_json::Value) -> bool {
    snapshot_strategy_flag(session, "resume").is_some()
}

fn snapshot_layout_manifest(session: &serde_json::Value) -> Option<serde_json::Value> {
    let layout = snapshot_session_layout(session)?;
    let kind = snapshot_layout_kind(layout)?;
    let ext = snapshot_layout_ext(layout)?;
    let mut object = serde_json::Map::new();
    object.insert("kind".to_string(), serde_json::Value::String(kind));
    object.insert("ext".to_string(), serde_json::Value::String(ext));
    for key in ["dir_template", "root_template", "project_hash"] {
        if let Some(value) = layout.get(key) {
            object.insert(key.to_string(), value.clone());
        }
    }
    Some(serde_json::Value::Object(object))
}

fn snapshot_emit_session_supported(session: &serde_json::Value) -> bool {
    snapshot_session_has_supported_layout(session)
        && snapshot_session_string(session, "session_dir_flag").is_some()
}

fn snapshot_session_has_supported_layout(session: &serde_json::Value) -> bool {
    let Some(layout) = snapshot_session_layout(session) else {
        return false;
    };
    snapshot_layout_ext(layout).is_some()
        && matches!(snapshot_layout_kind(layout).as_deref(), Some("FlatById"))
}

fn snapshot_preload_session_supported(session: &serde_json::Value) -> bool {
    snapshot_session_has_supported_layout(session)
        && (snapshot_resume_supported(session) || snapshot_strategy_flag(session, "fork").is_some())
}

fn snapshot_target_slug_or_err(resolved: &ResolvedAgent) -> MietteResult<String> {
    resolved_agent_target_slug(resolved).ok_or_else(|| {
        miette!(
            "snapshot-requires-target: agent '{}' does not resolve provider and model",
            resolved.agent.id()
        )
    })
}

fn snapshot_target_selector(resolved: &ResolvedAgent) -> String {
    resolved.target.as_ref().map(ExecutionTarget::selector).unwrap_or_else(|| {
        let provider = resolved.model_provider.as_deref().unwrap_or_default();
        let model =
            resolved.model_name.as_deref().or(resolved.model.as_deref()).unwrap_or_default();
        let mut selector = resolved.agent.id().to_string();
        if let Some(mode) = resolved.mode.as_deref() {
            selector.push('[');
            selector.push_str(mode);
            selector.push(']');
        }
        selector.push(':');
        selector.push_str(provider);
        selector.push(':');
        selector.push_str(model);
        selector
    })
}

fn snapshot_resolved_target_json(resolved: &ResolvedAgent) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert("agent".to_string(), serde_json::Value::String(resolved.agent.id().to_string()));
    if let Some(mode) = resolved.mode.as_deref() {
        object.insert("mode".to_string(), serde_json::Value::String(mode.to_string()));
    }
    if let Some(provider) = resolved.model_provider.as_deref() {
        object.insert("provider".to_string(), serde_json::Value::String(provider.to_string()));
    }
    if let Some(model) = resolved.model_name.as_deref().or(resolved.model.as_deref()) {
        object.insert("model".to_string(), serde_json::Value::String(model.to_string()));
    }
    serde_json::Value::Object(object)
}

fn snapshot_session_dir(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    slug: &str,
) -> PathBuf {
    workspace_root
        .join("runtime")
        .join("snapshot-sessions")
        .join(format!("{task_id}-{state_name}-{slug}-{}", snapshot_nonce()))
}

fn snapshot_nonce() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

fn newest_snapshot_session_file(dir: &Path, ext: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(OsStr::to_str) != Some(ext) {
            continue;
        }
        let modified = entry.metadata().and_then(|metadata| metadata.modified()).ok()?;
        if newest.as_ref().is_none_or(|(existing, _)| modified > *existing) {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path)
}

fn transcript_source_for_snapshot(
    session_dir: Option<&Path>,
    layout: &serde_json::Value,
) -> Option<(PathBuf, String, String)> {
    let ext = snapshot_layout_ext(layout)?;
    match snapshot_layout_kind(layout).as_deref() {
        Some("FlatById") => {
            let path = newest_snapshot_session_file(session_dir?, &ext)?;
            let session_id = path.file_stem().and_then(OsStr::to_str)?.to_string();
            Some((path, ext, session_id))
        }
        _ => None,
    }
}

fn snapshot_declared_provider(resolved: &ResolvedAgent) -> &str {
    resolved.model_provider.as_deref().unwrap_or_default()
}

fn snapshot_declared_model(resolved: &ResolvedAgent) -> &str {
    resolved.model_name.as_deref().or(resolved.model.as_deref()).unwrap_or_default()
}

fn pi_jsonl_observed_target(transcript_source: &Path) -> Option<(String, String)> {
    let file = fs::File::open(transcript_source).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    for _ in 0..8 {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
        return pi_header_target_from_value(&value);
    }
    None
}

fn pi_header_target_from_value(value: &serde_json::Value) -> Option<(String, String)> {
    fn string_at<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
        let mut cursor = value;
        for key in keys {
            cursor = cursor.get(*key)?;
        }
        cursor.as_str().filter(|text| !text.trim().is_empty())
    }

    let candidates = [
        (["provider"].as_slice(), ["model"].as_slice()),
        (["provider"].as_slice(), ["model_name"].as_slice()),
        (["model", "provider"].as_slice(), ["model", "name"].as_slice()),
        (["model", "provider"].as_slice(), ["model", "model"].as_slice()),
        (["target", "provider"].as_slice(), ["target", "model"].as_slice()),
        (["session", "provider"].as_slice(), ["session", "model"].as_slice()),
    ];
    candidates.iter().find_map(|(provider_path, model_path)| {
        let provider = string_at(value, provider_path)?;
        let model = string_at(value, model_path)?;
        Some((provider.to_string(), model.to_string()))
    })
}

fn observed_snapshot_target(
    resolved: &ResolvedAgent,
    transcript_source: &Path,
    transcript_ext: &str,
) -> (String, String) {
    let declared_provider = snapshot_declared_provider(resolved).to_string();
    let declared_model = snapshot_declared_model(resolved).to_string();
    if resolved.agent.id() != "pi" || transcript_ext != "jsonl" {
        return (declared_provider, declared_model);
    }
    if let Some((provider, model)) = pi_jsonl_observed_target(transcript_source) {
        return (provider, model);
    }
    eprintln!(
        "warning: pi snapshot transcript '{}' has no parseable provider/model header; falling back to declared target {}:{}",
        transcript_source.display(),
        declared_provider,
        declared_model
    );
    (declared_provider, declared_model)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(not(test))]
const SNAPSHOT_REDACTOR_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(test)]
const SNAPSHOT_REDACTOR_TIMEOUT: Duration = Duration::from_millis(500);
#[cfg(not(test))]
const SNAPSHOT_REDACTOR_TERMINATE_GRACE: Duration = Duration::from_secs(10);
#[cfg(test)]
const SNAPSHOT_REDACTOR_TERMINATE_GRACE: Duration = Duration::from_millis(50);

fn apply_snapshot_redactor(
    settings: &RheiSettings,
    workspace_root: &Path,
    transcript_bytes: Vec<u8>,
    log_path: Option<&Path>,
) -> MietteResult<Vec<u8>> {
    let Some(snapshot_settings) = settings.snapshots.as_ref() else {
        return Ok(transcript_bytes);
    };
    let Some(redactor) = snapshot_settings.redactor.as_ref() else {
        return Ok(transcript_bytes);
    };
    let redactor_path =
        if redactor.is_absolute() { redactor.clone() } else { workspace_root.join(redactor) };
    let redactor_label = redactor_path.display().to_string();
    let mut command = std::process::Command::new(&redactor_path);
    command
        .current_dir(workspace_root)
        .env_clear()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (key, value) in snapshot_redactor_default_env(workspace_root) {
        command.env(key, value);
    }
    for key in &snapshot_settings.redactor_env {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut child = command.spawn().map_err(|err| {
        file_io_report(&redactor_path, "failed to spawn snapshot redactor", err)
    })?;
    let mut stdin =
        child.stdin.take().ok_or_else(|| miette!("failed to open snapshot redactor stdin"))?;
    let mut stdout =
        child.stdout.take().ok_or_else(|| miette!("failed to open snapshot redactor stdout"))?;
    let mut stderr =
        child.stderr.take().ok_or_else(|| miette!("failed to open snapshot redactor stderr"))?;

    let writer = std::thread::spawn(move || stdin.write_all(&transcript_bytes));
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= SNAPSHOT_REDACTOR_TIMEOUT {
                    timed_out = true;
                    let pid = Pid::from_raw(child.id() as i32);
                    let _ = signal::kill(pid, Signal::SIGTERM);
                    std::thread::sleep(SNAPSHOT_REDACTOR_TERMINATE_GRACE);
                    match child.try_wait() {
                        Ok(Some(status)) => break status,
                        _ => {
                            let _ = child.kill();
                            break child.wait().map_err(|err| {
                                miette!("failed to wait for snapshot redactor after kill: {err}")
                            })?;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(err) => return Err(miette!("error waiting for snapshot redactor: {err}")),
        }
    };

    let writer_result = writer
        .join()
        .map_err(|_| miette!("snapshot redactor stdin writer panicked"))?;
    let stdout_bytes = stdout_reader
        .join()
        .map_err(|_| miette!("snapshot redactor stdout reader panicked"))?
        .map_err(|err| miette!("failed to read snapshot redactor stdout: {err}"))?;
    let stderr_bytes = stderr_reader
        .join()
        .map_err(|_| miette!("snapshot redactor stderr reader panicked"))?
        .map_err(|err| miette!("failed to read snapshot redactor stderr: {err}"))?;
    let (stderr_summary, stderr_truncated) = snapshot_redactor_stderr_summary(&stderr_bytes);
    append_snapshot_redactor_diagnostic(
        log_path,
        &redactor_label,
        &status,
        timed_out,
        stderr_truncated,
        &stderr_summary,
    )?;

    if timed_out {
        return Err(miette!(
            "snapshot redactor '{}' timed out after {}s; stderr: {}",
            redactor_label,
            SNAPSHOT_REDACTOR_TIMEOUT.as_secs_f64(),
            stderr_summary
        ));
    }
    if !status.success() {
        return Err(miette!(
            "snapshot redactor '{}' exited with status {}; stderr: {}",
            redactor_label,
            status,
            stderr_summary
        ));
    }
    writer_result.map_err(|err| miette!("failed to write snapshot redactor stdin: {err}"))?;
    Ok(stdout_bytes)
}

fn snapshot_redactor_default_env(workspace_root: &Path) -> Vec<(&'static str, PathBuf)> {
    let executable = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rhei"));
    let global_settings =
        home_dir().map(|home| home.join(".config/rhei/settings.json")).unwrap_or_default();
    vec![
        ("RHEI_EXECUTABLE_PATH", executable),
        ("RHEI_WORKSPACE_ROOT", workspace_root.to_path_buf()),
        ("RHEI_PROJECT_SETTINGS_PATH", workspace_root.join(".rhei/settings.json")),
        ("RHEI_GLOBAL_SETTINGS_PATH", global_settings),
    ]
}

fn append_snapshot_redactor_diagnostic(
    log_path: Option<&Path>,
    redactor_path: &str,
    status: &std::process::ExitStatus,
    timed_out: bool,
    stderr_truncated: bool,
    stderr_summary: &str,
) -> MietteResult<()> {
    let Some(log_path) = log_path else {
        return Ok(());
    };
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create snapshot log dir", err))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|err| file_io_report(log_path, "failed to append snapshot redactor diagnostic", err))?;
    let summary = stderr_summary.replace('\n', "\\n").replace('\r', "\\r");
    writeln!(
        file,
        "snapshot redactor: path={} status={} timeout={} stderr_truncated={} stderr={}",
        redactor_path, status, timed_out, stderr_truncated, summary
    )
    .map_err(|err| file_io_report(log_path, "failed to write snapshot redactor diagnostic", err))
}

fn snapshot_redactor_stderr_summary(bytes: &[u8]) -> (String, bool) {
    const LIMIT: usize = 1024;
    if bytes.is_empty() {
        return ("<empty>".to_string(), false);
    }
    let clipped = &bytes[..bytes.len().min(LIMIT)];
    let mut summary = String::from_utf8_lossy(clipped).trim().to_string();
    if summary.is_empty() {
        summary = "<empty>".to_string();
    }
    (summary, bytes.len() > LIMIT)
}

#[allow(clippy::too_many_arguments)]
fn write_snapshot_generation_atomic(
    cache_root: &Path,
    workspace_root: &Path,
    settings: &RheiSettings,
    task_id: &str,
    snapshot_name: &str,
    emitting_state: &str,
    visit: u64,
    target_slug: &str,
    target_selector: &str,
    resolved: &ResolvedAgent,
    session_layout: serde_json::Value,
    session_id: &str,
    transcript_source: &Path,
    transcript_ext: &str,
    observed_provider: &str,
    observed_model: &str,
    parent_ref: Option<&serde_json::Value>,
    completion: SnapshotCompletion,
    produced_by: SnapshotProducedBy,
    redactor_log_path: Option<&Path>,
) -> MietteResult<SnapshotRecord> {
    let identity_dir = cache_root
        .join(task_id)
        .join(snapshot_name)
        .join(emitting_state)
        .join(visit.to_string())
        .join(target_slug);
    fs::create_dir_all(&identity_dir).map_err(|err| {
        file_io_report(&identity_dir, "failed to create snapshot identity dir", err)
    })?;
    let lock_path = identity_dir.join(".lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|err| file_io_report(&lock_path, "failed to open snapshot identity lock", err))?;
    lock.lock_exclusive()
        .map_err(|err| file_io_report(&lock_path, "failed to lock snapshot identity", err))?;

    let transcript_bytes = fs::read(transcript_source).map_err(|err| {
        file_io_report(transcript_source, "failed to read snapshot transcript source", err)
    })?;
    let transcript_bytes =
        apply_snapshot_redactor(settings, workspace_root, transcript_bytes, redactor_log_path)?;
    let transcript_sha256 = sha256_hex(&transcript_bytes);
    let transcript_name = format!("transcript.{transcript_ext}");
    let mut generation = next_snapshot_generation(&identity_dir)?;

    loop {
        let nonce = snapshot_nonce();
        let tmp_dir = identity_dir.join(format!("g{generation}.tmp-{nonce}"));
        let generation_dir = identity_dir.join(format!("g{generation}"));
        fs::create_dir_all(&tmp_dir).map_err(|err| {
            file_io_report(&tmp_dir, "failed to create snapshot staging dir", err)
        })?;
        let transcript_path = tmp_dir.join(&transcript_name);
        fs::write(&transcript_path, &transcript_bytes).map_err(|err| {
            file_io_report(&transcript_path, "failed to write snapshot transcript", err)
        })?;
        let created_at = format_iso8601_utc(std::time::SystemTime::now());
        let manifest = serde_json::json!({
            "version": 1,
            "rhei_version": env!("CARGO_PKG_VERSION"),
            "snapshot_name": snapshot_name,
            "task_id": task_id,
            "emitting_state": emitting_state,
            "visit": visit,
            "generation": generation,
            "target": {
                "selector": target_selector,
                "slug": target_slug,
                "resolved": snapshot_resolved_target_json(resolved),
            },
            "declared_provider": snapshot_declared_provider(resolved),
            "declared_model": snapshot_declared_model(resolved),
            "observed_provider": observed_provider,
            "observed_model": observed_model,
            "session_id": session_id,
            "session_layout": session_layout,
            "transcript_path": transcript_name,
            "transcript_sha256": transcript_sha256,
            "transcript_bytes": transcript_bytes.len() as u64,
            "parent_ref": parent_ref.cloned().unwrap_or(serde_json::Value::Null),
            "created_at": created_at,
            "completion": completion.as_str(),
            "produced_by": produced_by.as_str(),
        });
        let manifest_text = serde_json::to_string_pretty(&manifest)
            .map_err(|err| miette!("failed to serialize snapshot manifest: {err}"))?;
        fs::write(tmp_dir.join("manifest.json"), manifest_text).map_err(|err| {
            file_io_report(&tmp_dir.join("manifest.json"), "failed to write snapshot manifest", err)
        })?;

        match fs::rename(&tmp_dir, &generation_dir) {
            Ok(()) => {
                if produced_by == SnapshotProducedBy::Orchestrator {
                    replace_current_symlink_with_nonce(
                        &identity_dir,
                        &format!("g{generation}"),
                        &nonce,
                    )?;
                }
                let _ = lock.unlock();
                return snapshot_record_from_manifest(
                    cache_root,
                    &generation_dir.join("manifest.json"),
                    manifest,
                )?
                .ok_or_else(|| {
                    miette!(
                        "failed to read back snapshot generation '{}'",
                        generation_dir.display()
                    )
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_dir_all(&tmp_dir);
                generation = next_snapshot_generation(&identity_dir)?;
            }
            Err(err) => {
                let _ = fs::remove_dir_all(&tmp_dir);
                let _ = lock.unlock();
                return Err(file_io_report(
                    &generation_dir,
                    "failed to finalize snapshot generation",
                    err,
                ));
            }
        }
    }
}

fn next_snapshot_generation(identity_dir: &Path) -> MietteResult<u64> {
    let mut generation = 1;
    if identity_dir.exists() {
        for entry in fs::read_dir(identity_dir).map_err(|err| {
            file_io_report(identity_dir, "failed to inspect snapshot identity dir", err)
        })? {
            let entry = entry.map_err(|err| {
                file_io_report(identity_dir, "failed to inspect snapshot identity entry", err)
            })?;
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if name.contains(".tmp-") {
                continue;
            }
            if let Some(value) = name.strip_prefix('g').and_then(|value| value.parse::<u64>().ok())
            {
                generation = generation.max(value.saturating_add(1));
            }
        }
    }
    Ok(generation)
}

#[cfg(unix)]
fn replace_current_symlink_with_nonce(
    identity_dir: &Path,
    target: &str,
    nonce: &str,
) -> MietteResult<()> {
    use std::os::unix::fs::symlink;
    let tmp = identity_dir.join(format!("current.tmp-{nonce}"));
    let current = identity_dir.join("current");
    if tmp.exists() || tmp.is_symlink() {
        fs::remove_file(&tmp).map_err(|err| {
            file_io_report(&tmp, "failed to remove stale current tmp pointer", err)
        })?;
    }
    symlink(target, &tmp)
        .map_err(|err| file_io_report(&tmp, "failed to write current tmp pointer", err))?;
    fs::rename(&tmp, &current)
        .map_err(|err| file_io_report(&current, "failed to update current pointer", err))
}

#[cfg(not(unix))]
fn replace_current_symlink_with_nonce(
    identity_dir: &Path,
    target: &str,
    _nonce: &str,
) -> MietteResult<()> {
    let current = identity_dir.join("current");
    fs::write(&current, target)
        .map_err(|err| file_io_report(&current, "failed to update current pointer", err))
}
