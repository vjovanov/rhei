// vjovanov/rhei#125: fixed-location session emit (`dir_template`, with and
// without `assign_id_flag`) — split from tests_snapshot_runtime.rs to stay
// under the file-size budget. §AR-source-file-size.3

// vjovanov/rhei#125: a fixed-location session with `assign_id_flag` gets
// the flag and a rhei-chosen id at spawn, and emit reads the exact
// `<dir>/<id>.<ext>` path afterward. §FS-rhei-snapshots.9.1 §FS-rhei-snapshots.10.2
#[test]
fn snapshot_fixed_location_assign_id_flag_reads_exact_path_and_stamps_manifest() {
    let dir = snapshot_workspace();
    write_fixed_location_emit_machine(dir.path());
    let fixed_dir = dir.path().join("fixed-sessions");
    let settings =
        fixed_location_settings(fixed_dir.to_str().expect("utf8 path"), Some("--session-id"));
    let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
    let task = loaded.rhei.tasks.first().expect("task");

    let preload = preload_snapshot_inherit_before_spawn(
        dir.path(),
        dir.path(),
        &machine,
        task,
        "pending",
        &resolved,
        &settings,
        1,
        None,
        &default_run_options(),
    )
    .expect("fixed-location profile with assign_id_flag preloads");
    assert_eq!(preload.fixed_session_dir.as_deref(), Some(fixed_dir.as_path()));
    assert!(preload.session_dir.is_none(), "assign_id_flag path never redirects");
    let assigned_id = preload.fixed_session_id.clone().expect("assigned id");
    assert_eq!(preload.extra_args, vec!["--session-id".to_string(), assigned_id.clone()]);

    fs::create_dir_all(&fixed_dir).expect("fixed dir");
    fs::write(fixed_dir.join(format!("{assigned_id}.jsonl")), b"assigned transcript\n")
        .expect("write transcript");

    let log_path = dir.path().join("runtime/logs/task-1-pending.log");
    fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
    fs::write(&log_path, "log\n").expect("log");

    emit_snapshots_after_agent_exit(
        dir.path(),
        &machine,
        &settings,
        task,
        "pending",
        Some("done"),
        &resolved,
        &log_path,
        1,
        SnapshotCompletion::Success,
        &preload,
    )
    .expect("emit reads the exact assigned-id path");

    let records =
        read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
    let named = records
        .iter()
        .find(|record| record.snapshot_name == "impl")
        .expect("named snapshot written");
    assert_eq!(
        named.manifest.get("session_id").and_then(serde_json::Value::as_str),
        Some(assigned_id.as_str())
    );
    assert_eq!(fs::read(named.transcript_path()).expect("transcript"), b"assigned transcript\n");
}

// Without `assign_id_flag`, emit scans for the newest file no earlier than
// the spawn — a pre-existing older transcript is not captured.
// §FS-rhei-snapshots.9.1 §FS-rhei-snapshots.10.2
#[test]
fn snapshot_fixed_location_without_assign_id_flag_ignores_transcripts_older_than_spawn() {
    let dir = snapshot_workspace();
    write_fixed_location_emit_machine(dir.path());
    let fixed_dir = dir.path().join("fixed-sessions");
    fs::create_dir_all(&fixed_dir).expect("fixed dir");
    let old_path = fixed_dir.join("old-session.jsonl");
    fs::write(&old_path, b"stale transcript\n").expect("old transcript");
    set_file_mtime(&old_path, std::time::SystemTime::now() - std::time::Duration::from_secs(3600));

    let settings = fixed_location_settings(fixed_dir.to_str().expect("utf8 path"), None);
    let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
    let task = loaded.rhei.tasks.first().expect("task");

    let preload = preload_snapshot_inherit_before_spawn(
        dir.path(),
        dir.path(),
        &machine,
        task,
        "pending",
        &resolved,
        &settings,
        1,
        None,
        &default_run_options(),
    )
    .expect("fixed-location profile without assign_id_flag preloads");
    assert!(preload.fixed_session_id.is_none());
    assert!(preload.extra_args.is_empty());
    let floor = preload.fixed_session_scan_floor.expect("scan floor recorded");

    let new_path = fixed_dir.join("new-session.jsonl");
    fs::write(&new_path, b"live transcript\n").expect("new transcript");
    set_file_mtime(&new_path, floor + std::time::Duration::from_secs(10));

    let log_path = dir.path().join("runtime/logs/task-1-pending.log");
    fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
    fs::write(&log_path, "log\n").expect("log");

    emit_snapshots_after_agent_exit(
        dir.path(),
        &machine,
        &settings,
        task,
        "pending",
        Some("done"),
        &resolved,
        &log_path,
        1,
        SnapshotCompletion::Success,
        &preload,
    )
    .expect("emit scans for the newest transcript since spawn");

    let records =
        read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
    let named = records
        .iter()
        .find(|record| record.snapshot_name == "impl")
        .expect("named snapshot written");
    assert_eq!(
        named.manifest.get("session_id").and_then(serde_json::Value::as_str),
        Some("new-session")
    );
    assert_eq!(fs::read(named.transcript_path()).expect("transcript"), b"live transcript\n");
}

fn write_fixed_location_emit_machine(dir: &Path) {
    fs::write(
        dir.join("states.yaml"),
        r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    target: fixed:openai:model
    snapshot:
      emit:
        name: impl
        on: always
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
    )
    .expect("write states");
}

/// `dir_template` is an absolute path into the test's own tempdir rather
/// than a `~/`-prefixed one, which pins tilde-free templates working
/// alongside the home-relative case covered by the ticket's own repro.
// §FS-rhei-snapshots.9.1
fn fixed_location_settings(dir_template: &str, assign_id_flag: Option<&str>) -> RheiSettings {
    let mut session = serde_json::json!({
        "layout": {
            "kind": "FlatById",
            "dir_template": dir_template,
            "ext": "jsonl"
        }
    });
    if let Some(flag) = assign_id_flag {
        session["assign_id_flag"] = serde_json::Value::String(flag.to_string());
    }
    let mut agents = BTreeMap::new();
    agents.insert(
        "fixed".to_string(),
        CustomAgentProfile {
            command: vec!["fixed".to_string()],
            prompt_flag: Some("-p".to_string()),
            session: Some(session),
            ..Default::default()
        },
    );
    RheiSettings { agents, ..Default::default() }
}

fn set_file_mtime(path: &Path, time: std::time::SystemTime) {
    let file = fs::OpenOptions::new().write(true).open(path).expect("open for mtime");
    file.set_modified(time).expect("set mtime");
}
