        let machine = rhei_validator::StateMachine::from_yaml_file(dir.path().join("states.yaml"))
            .expect("state machine");
        let settings = RheiSettings { agents: built_in_agents(), ..Default::default() };
        let resolved =
            resolve_agent_invocations(&machine, "pending", &settings, &default_run_options())
                .expect("resolve")
                .remove(0);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "transcript\n").expect("log");

        emit_snapshots_after_agent_exit(
            dir.path(),
            &machine,
            &settings,
            task,
            "pending",
            &resolved,
            &log_path,
            1,
            SnapshotCompletion::Failure,
            &SnapshotPreload::default(),
        )
        .expect("emit snapshots");

        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        assert!(records.iter().any(|record| record.snapshot_name == "_state"));
        assert!(records.iter().any(|record| record.snapshot_name == "impl"));
        assert!(records.iter().all(|record| record.generation == 1 && record.is_current));
    }

    #[test]
    fn snapshot_preload_resolves_self_current_generation() {
        let dir = snapshot_workspace();
        fs::write(
            dir.path().join("states.yaml"),
            r#"name: snapshot-test
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
        let mut agents = BTreeMap::new();
        agents.insert(
            "claude-code".to_string(),
            CustomAgentProfile {
                command: vec!["fake".to_string()],
                prompt_flag: Some("-p".to_string()),
                model_flag: Some("--model".to_string()),
                session: Some(serde_json::json!({
                    "resume": {"flag": "--resume"},
                    "layout": {"kind": "FlatById", "ext": "jsonl"}
                })),
                ..Default::default()
            },
        );
        let settings = RheiSettings { agents, ..Default::default() };
        let identity = SnapshotIdentity {
            task_id: "1".to_string(),
            snapshot_name: "impl".to_string(),
            emitting_state: "source".to_string(),
            visit: 1,
            target_slug: "claude-code-anthropic-model".to_string(),
        };
        write_snapshot_generation(
            &snapshot_cache_dir(&settings, dir.path()),
            &identity.task_id,
            &identity.snapshot_name,
            &identity.emitting_state,
            identity.visit,
            &identity.target_slug,
            1,
            "orchestrator",
        );
        refresh_current_links(
            &snapshot_cache_dir(&settings, dir.path()),
            [identity].into_iter().collect(),
        )
        .expect("current");
        let loaded = load_plan(dir.path()).expect("load plan");
        let machine = rhei_validator::StateMachine::from_yaml_file(dir.path().join("states.yaml"))
            .expect("state machine");
        let task = loaded.rhei.tasks.first().expect("task");
        let resolved =
            resolve_agent_invocations(&machine, "pending", &settings, &default_run_options())
                .expect("resolve")
                .remove(0);

        let preload = preload_snapshot_inherit_before_spawn(
            dir.path(),
            dir.path(),
            &machine,
            task,
            "pending",
            &resolved,
            &settings,
            1,
            &default_run_options(),
        )
        .expect("preload");

        assert_eq!(
            preload
                .parent_ref
                .as_ref()
                .and_then(|value| value.get("snapshot_name"))
                .and_then(serde_json::Value::as_str),
            Some("impl")
        );
        assert!(preload.extra_args.windows(2).any(|pair| pair[0] == "--resume"));
    }

    fn snapshot_workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmpdir");
        fs::write(
            dir.path().join("index.rhei.md"),
            "# Rhei: Snapshot Test\n**States:** snapshot-test\n\n## Notes\n",
        )
        .expect("write index");
        fs::create_dir_all(dir.path().join("tasks")).expect("tasks dir");
        fs::write(
            dir.path().join("tasks/01.md"),
            "### Task 1: Implement\n**State:** pending\n\nDo work.\n",
        )
        .expect("write task");
        fs::write(
            dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    target: claude-code:anthropic:model
  review:
    description: review
    target: claude-code:anthropic:model
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: review
  - from: review
    to: done
"#,
        )
        .expect("write states");
        dir
    }

    fn write_snapshot_workspace_task(dir: &Path, task_body: &str) {
        fs::write(dir.join("tasks/01.md"), task_body).expect("write task");
    }

    #[allow(clippy::too_many_arguments)]
    fn write_snapshot_generation(
        cache_root: &Path,
        task_id: &str,
        name: &str,
        state: &str,
        visit: u64,
        target_slug: &str,
        generation: u64,
        produced_by: &str,
    ) {
        write_snapshot_generation_with_created_at(
            cache_root,
            task_id,
            name,
            state,
            visit,
            target_slug,
            generation,
            produced_by,
            "2026-05-18T08:14:22Z",
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn write_snapshot_generation_with_created_at(
        cache_root: &Path,
        task_id: &str,
        name: &str,
        state: &str,
        visit: u64,
        target_slug: &str,
        generation: u64,
        produced_by: &str,
        created_at: &str,
    ) {
        let dir = cache_root
            .join(task_id)
            .join(name)
            .join(state)
            .join(visit.to_string())
            .join(target_slug)
            .join(format!("g{generation}"));
        fs::create_dir_all(&dir).expect("snapshot dir");
        fs::write(dir.join("transcript.jsonl"), format!("generation {generation}\n"))
            .expect("transcript");
        let manifest = serde_json::json!({
            "version": 1,
            "rhei_version": "test",
            "snapshot_name": name,
            "task_id": task_id,
            "emitting_state": state,
            "visit": visit,
            "generation": generation,
            "target": {
                "selector": "claude-code:anthropic:model",
                "slug": target_slug,
                "resolved": {
                    "agent": "claude-code",
                    "provider": "anthropic",
                    "model": "model"
                }
            },
            "declared_provider": "anthropic",
            "declared_model": "model",
            "observed_provider": "anthropic",
            "observed_model": "model",
            "session_id": format!("session-{generation}"),
            "session_layout": {"kind": "FlatById", "ext": "jsonl"},
            "transcript_path": "transcript.jsonl",
            "transcript_sha256": "test",
            "transcript_bytes": 13,
            "parent_ref": null,
            "created_at": created_at,
            "completion": "success",
            "produced_by": produced_by
        });
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("manifest json"),
        )
        .expect("manifest");
    }

    /// RAII helper that temporarily redirects `HOME` to a sandboxed
    /// directory so tests that interrogate `~/.config/rhei` do not
    /// touch the real user's home.
    static TEST_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct TempHome {
        dir: tempfile::TempDir,
        previous: Option<std::ffi::OsString>,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl TempHome {
        fn new() -> Self {
            let guard = TEST_HOME_LOCK.lock().expect("home lock");
            let dir = tempfile::tempdir().expect("tmphome");
            let previous = std::env::var_os("HOME");
            std::env::set_var("HOME", dir.path());
            TempHome { dir, previous, _guard: guard }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(prev) => std::env::set_var("HOME", prev),
                None => std::env::remove_var("HOME"),
            }
            // dir cleans up by drop
            let _ = &self.dir;
        }
    }
