// vjovanov/rhei#146 R1-01: which of `resume` and `fork` a preload emits, and
// what it hands the chosen one — split from tests_snapshot_runtime.rs, which
// is at its file-size budget. §AR-source-file-size.3

/// A profile declaring both strategies is forked rather than resumed, and the
/// fork flag is handed the staged transcript's *path* — never a session id.
/// That is what makes `ForkStrategy::Native` unusable for an agent whose fork
/// takes an id, and it is why the built-in codex profile declares no `fork`
/// (§FS-rhei-snapshots.9.3.4). Nothing pinned the pair before this test, so
/// a profile could declare both and look correct while never resuming.
// §FS-rhei-snapshots.10.1 §FS-rhei-snapshots.9.2
#[test]
fn snapshot_preload_prefers_fork_and_hands_it_the_staged_transcript_path() {
    let dir = snapshot_workspace();
    write_preload_strategy_machine(dir.path());
    let settings = preload_strategy_settings(serde_json::json!({
        "resume": {"flag": "--resume"},
        "fork": {"flag": "--fork"},
        "layout": {"kind": "FlatById", "ext": "jsonl"}
    }));
    let cache_root = snapshot_cache_dir(&settings, dir.path());
    let identity = SnapshotIdentity {
        task_id: "plan.1".to_string(),
        snapshot_name: "impl".to_string(),
        emitting_state: "source".to_string(),
        visit: 1,
        target_slug: "claude-code-anthropic-model".to_string(),
    };
    write_snapshot_generation(
        &cache_root,
        &identity.task_id,
        &identity.snapshot_name,
        &identity.emitting_state,
        identity.visit,
        &identity.target_slug,
        1,
        "orchestrator",
    );
    refresh_current_links(&cache_root, [identity].into_iter().collect()).expect("current");

    let preload = preload_strategy_extra_args(dir.path(), &settings);

    let transcript = cache_root
        .join("plan.1")
        .join("impl")
        .join("source")
        .join("1")
        .join("claude-code-anthropic-model")
        .join("g1")
        .join("transcript.jsonl");
    assert_eq!(
        preload,
        vec!["--fork".to_string(), transcript.display().to_string()],
        "fork wins over resume, and its argument is the staged transcript path"
    );
    assert!(
        !preload.iter().any(|arg| arg == "--resume"),
        "the resume branch must not also run"
    );
}

/// The same profile with `fork` dropped resumes instead, on the manifest's
/// `session_id`. Read beside the test above, the pair is the whole rule: a
/// profile that wants resume emitted has to leave `fork` undeclared.
// §FS-rhei-snapshots.10.1 §FS-rhei-snapshots.9.3.4
#[test]
fn snapshot_preload_without_fork_resumes_on_the_manifest_session_id() {
    let dir = snapshot_workspace();
    write_preload_strategy_machine(dir.path());
    let settings = preload_strategy_settings(serde_json::json!({
        "resume": {"flag": "--resume"},
        "layout": {"kind": "FlatById", "ext": "jsonl"}
    }));
    let cache_root = snapshot_cache_dir(&settings, dir.path());
    let identity = SnapshotIdentity {
        task_id: "plan.1".to_string(),
        snapshot_name: "impl".to_string(),
        emitting_state: "source".to_string(),
        visit: 1,
        target_slug: "claude-code-anthropic-model".to_string(),
    };
    write_snapshot_generation(
        &cache_root,
        &identity.task_id,
        &identity.snapshot_name,
        &identity.emitting_state,
        identity.visit,
        &identity.target_slug,
        1,
        "orchestrator",
    );
    refresh_current_links(&cache_root, [identity].into_iter().collect()).expect("current");

    assert_eq!(
        preload_strategy_extra_args(dir.path(), &settings),
        vec!["--resume".to_string(), "session-1".to_string()]
    );
}

fn preload_strategy_extra_args(dir: &Path, settings: &RheiSettings) -> Vec<String> {
    let loaded = load_plan(dir).expect("load plan");
    let machine = rhei_validator::StateMachine::from_yaml_file(dir.join("states.yaml"))
        .expect("state machine");
    let task = loaded.rhei.tasks.first().expect("task");
    let resolved = resolve_agent_invocations(&machine, "pending", settings, &default_run_options())
        .expect("resolve")
        .remove(0);
    preload_snapshot_inherit_before_spawn(
        dir,
        single_root_preload(dir),
        dir,
        &machine,
        task,
        "pending",
        &resolved,
        settings,
        1,
        None,
        &default_run_options(),
    )
    .expect("preload")
    .extra_args
}

fn preload_strategy_settings(session: serde_json::Value) -> RheiSettings {
    let mut agents = BTreeMap::new();
    agents.insert(
        "claude-code".to_string(),
        CustomAgentProfile {
            command: vec!["fake".to_string()],
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            session: Some(session),
            ..Default::default()
        },
    );
    RheiSettings { agents, ..Default::default() }
}

fn write_preload_strategy_machine(dir: &Path) {
    fs::write(
        dir.join("states.yaml"),
        r#"name: snapshot-preload-strategy
version: 1
states:
  source:
    description: source
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: impl
  pending:
    description: pending
    initial: true
    target: claude-code:anthropic:model
    snapshot:
      inherit:
        name: impl
        required: true
        select:
          state: source
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
