    #[test]
    fn snapshot_targetless_auto_state_without_authored_snapshot_runs_cold() {
        let dir = snapshot_workspace();
        write_targetless_snapshot_machine(dir.path(), "");
        let settings = targetless_snapshot_settings();
        let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "transcript\n").expect("log");

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
        .expect("targetless state without inherit runs cold");
        assert!(preload.session_dir.is_none());

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
        .expect("targetless auto snapshot is skipped");
        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        assert!(records.is_empty());
    }

    #[test]
    fn snapshot_targetless_explicit_emit_and_inherit_require_target() {
        let emit_dir = snapshot_workspace();
        write_targetless_snapshot_machine(
            emit_dir.path(),
            "    snapshot:\n      emit:\n        name: impl\n",
        );
        let settings = targetless_snapshot_settings();
        let (loaded, machine, resolved) = snapshot_preload_parts(emit_dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = emit_dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "transcript\n").expect("log");

        let err = emit_snapshots_after_agent_exit(
            emit_dir.path(),
            &machine,
            &settings,
            task,
            "pending",
            Some("done"),
            &resolved,
            &log_path,
            1,
            SnapshotCompletion::Success,
            &SnapshotPreload::default(),
        )
        .expect_err("explicit emit requires target");
        assert!(err.to_string().contains("snapshot-requires-target"));

        let inherit_dir = snapshot_workspace();
        fs::write(
            inherit_dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  source:
    description: source
    agent: fake
    snapshot:
      emit:
        name: impl
  pending:
    description: pending
    initial: true
    agent: fake
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
        let (loaded, machine, resolved) = snapshot_preload_parts(inherit_dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let err = preload_snapshot_inherit_before_spawn(
            inherit_dir.path(),
            inherit_dir.path(),
            &machine,
            task,
            "pending",
            &resolved,
            &settings,
            1,
            None,
            &default_run_options(),
        )
        .expect_err("explicit inherit requires target");
        assert!(err.to_string().contains("snapshot-requires-target"));
    }

    #[test]
    fn snapshot_emit_missing_native_transcript_skips_auto_but_fails_named_emit() {
        let auto_dir = snapshot_workspace();
        fs::write(
            auto_dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    target: pi:openai:model
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
        )
        .expect("write states");
        let settings = RheiSettings { agents: built_in_agents(), ..Default::default() };
        let (loaded, machine, resolved) = snapshot_preload_parts(auto_dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = auto_dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "rhei log is not a native transcript\n").expect("log");

        emit_snapshots_after_agent_exit(
            auto_dir.path(),
            &machine,
            &settings,
            task,
            "pending",
            Some("done"),
            &resolved,
            &log_path,
            1,
            SnapshotCompletion::Success,
            &SnapshotPreload::default(),
        )
        .expect("auto emit skips missing native transcript");
        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, auto_dir.path())).expect("records");
        assert!(records.is_empty());

        let named_dir = snapshot_workspace();
        write_snapshot_emit_machine(named_dir.path());
        let (loaded, machine, resolved) = snapshot_preload_parts(named_dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = named_dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "rhei log is not a native transcript\n").expect("log");

        let err = emit_snapshots_after_agent_exit(
            named_dir.path(),
            &machine,
            &settings,
            task,
            "pending",
            Some("done"),
            &resolved,
            &log_path,
            1,
            SnapshotCompletion::Success,
            &SnapshotPreload::default(),
        )
        .expect_err("named emit fails missing native transcript");
        assert!(err.to_string().contains("unsupported-snapshot-session"));
        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, named_dir.path())).expect("records");
        assert!(records.is_empty());
    }

    #[test]
    fn snapshot_emit_skips_poll_self_loop_until_terminal_exit() {
        let dir = snapshot_workspace();
        fs::write(
            dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    target: pi:openai:model
    poll:
      interval: 1s
      max_attempts: 2
    snapshot:
      emit:
        name: impl
        on: always
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: pending
  - from: pending
    to: done
"#,
        )
        .expect("write states");
        let settings = RheiSettings { agents: built_in_agents(), ..Default::default() };
        let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "log\n").expect("log");
        let preload =
            snapshot_preload_with_native_session(dir.path(), "poll-session", b"poll transcript\n");

        emit_snapshots_after_agent_exit(
            dir.path(),
            &machine,
            &settings,
            task,
            "pending",
            Some("pending"),
            &resolved,
            &log_path,
            1,
            SnapshotCompletion::Success,
            &preload,
        )
        .expect("poll self-loop skips emit");
        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        assert!(records.is_empty());

        emit_snapshots_after_agent_exit(
            dir.path(),
            &machine,
            &settings,
            task,
            "pending",
            Some("pending"),
            &resolved,
            &log_path,
            1,
            SnapshotCompletion::Failure,
            &preload,
        )
        .expect("poll failure self-loop skips emit");
        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        assert!(records.is_empty());

        emit_snapshots_after_agent_exit(
            dir.path(),
            &machine,
            &settings,
            task,
            "pending",
            Some("pending"),
            &resolved,
            &log_path,
            1,
            SnapshotCompletion::Timeout,
            &preload,
        )
        .expect("poll timeout self-loop skips emit");
        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        assert!(records.is_empty());

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
        .expect("terminal poll exit emits");
        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        assert_eq!(records.iter().filter(|record| record.snapshot_name == "_state").count(), 1);
        assert_eq!(records.iter().filter(|record| record.snapshot_name == "impl").count(), 1);
    }

    #[test]
    fn snapshot_inherit_ancestor_applies_selected_state_while_walking() {
        let dir = snapshot_workspace();
        let settings = snapshot_preload_settings();
        let cache_root = snapshot_cache_dir(&settings, dir.path());
        write_snapshot_generation(
            &cache_root,
            "1.1",
            "impl",
            "review",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &cache_root,
            "1",
            "impl",
            "source",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        refresh_current_links(
            &cache_root,
            [
                SnapshotIdentity {
                    task_id: "1.1".to_string(),
                    snapshot_name: "impl".to_string(),
                    emitting_state: "review".to_string(),
                    visit: 1,
                    target_slug: "claude-code-anthropic-model".to_string(),
                },
                SnapshotIdentity {
                    task_id: "1".to_string(),
                    snapshot_name: "impl".to_string(),
                    emitting_state: "source".to_string(),
                    visit: 1,
                    target_slug: "claude-code-anthropic-model".to_string(),
                },
            ]
            .into_iter()
            .collect(),
        )
        .expect("current");
        let task = rhei_core::ast::Task {
            id: parse_task_id("1.1.1"),
            kind: "task".to_string(),
            title: "child".to_string(),
            state: "pending".to_string(),
            prior: Vec::new(),
            assignee: None,
            content: String::new(),
            children: Vec::new(),
        };
        let inherit = rhei_validator::SnapshotInheritConfig {
            name: "impl".to_string(),
            from_axis: Some("ancestor".to_string()),
            compat: None,
            required: None,
            select: Some(rhei_validator::SnapshotInheritSelectConfig {
                state: Some("source".to_string()),
                target: Some("same".to_string()),
                visit: None,
                generation: None,
            }),
        };

        let record = resolve_inherit_snapshot_source(
            &cache_root,
            &task,
            "pending",
            &inherit,
            "claude-code-anthropic-model",
            1,
        )
        .expect("resolve")
        .expect("selected ancestor");
        assert_eq!(record.task_id, "1");
        assert_eq!(record.emitting_state, "source");
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
            None,
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

    #[test]
    fn snapshot_preload_from_snapshot_rejects_inherit_contract_mismatch() {
        let cases = [
            (
                "name mismatch",
                "        name: impl\n        required: true\n        select:\n          state: source\n          target: same\n",
                "1:other:source@1:claude-code-anthropic-model/g1",
                "other",
                "source",
                "claude-code-anthropic-model",
                1,
                "snapshot name",
            ),
            (
                "state mismatch",
                "        name: impl\n        required: true\n        select:\n          state: source\n          target: same\n",
                "1:impl:review@1:claude-code-anthropic-model/g1",
                "impl",
                "review",
                "claude-code-anthropic-model",
                1,
                "select.state",
            ),
            (
                "target mismatch",
                "        name: impl\n        required: true\n        select:\n          state: source\n          target: same\n",
                "1:impl:source@1:other-target/g1",
                "impl",
                "source",
                "other-target",
                1,
                "select.target",
            ),
            (
                "generation mismatch",
                "        name: impl\n        required: true\n        select:\n          state: source\n          target: same\n          generation: 2\n",
                "1:impl:source@1:claude-code-anthropic-model/g1",
                "impl",
                "source",
                "claude-code-anthropic-model",
                1,
                "select.generation",
            ),
        ];

        for (
            label,
            inherit_yaml,
            reference,
            snapshot_name,
            emitting_state,
            target_slug,
            generation,
            expected,
        ) in cases
        {
            let dir = snapshot_workspace();
            write_snapshot_inherit_machine(dir.path(), inherit_yaml);
            let settings = snapshot_preload_settings();
            write_snapshot_generation(
                &snapshot_cache_dir(&settings, dir.path()),
                "1",
                snapshot_name,
                emitting_state,
                1,
                target_slug,
                generation,
                "orchestrator",
            );
            refresh_current_links(
                &snapshot_cache_dir(&settings, dir.path()),
                [SnapshotIdentity {
                    task_id: "1".to_string(),
                    snapshot_name: snapshot_name.to_string(),
                    emitting_state: emitting_state.to_string(),
                    visit: 1,
                    target_slug: target_slug.to_string(),
                }]
                .into_iter()
                .collect(),
            )
            .expect("current");
            let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
            let task = loaded.rhei.tasks.first().expect("task");
            let opts = snapshot_override_options(reference, false);

            match preload_snapshot_inherit_before_spawn(
                dir.path(),
                dir.path(),
                &machine,
                task,
                "pending",
                &resolved,
                &settings,
                1,
                None,
                &opts,
            ) {
                Ok(preload) => {
                    panic!("expected {label} mismatch to fail, got {:?}", preload.parent_ref)
                }
                Err(err) => assert!(
                    err.to_string().contains(expected),
                    "expected {label} error to contain '{expected}', got: {err}"
                ),
            }
        }
    }

    #[test]
    fn snapshot_preload_from_snapshot_rejects_compat_none_without_override() {
        let dir = snapshot_workspace();
        write_snapshot_inherit_machine(
            dir.path(),
            "        name: impl\n        compat: none\n        select:\n          state: source\n",
        );
        let settings = snapshot_preload_settings();
        write_snapshot_generation(
            &snapshot_cache_dir(&settings, dir.path()),
            "1",
            "impl",
            "source",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        refresh_current_links(
            &snapshot_cache_dir(&settings, dir.path()),
            [SnapshotIdentity {
                task_id: "1".to_string(),
                snapshot_name: "impl".to_string(),
                emitting_state: "source".to_string(),
                visit: 1,
                target_slug: "claude-code-anthropic-model".to_string(),
            }]
            .into_iter()
            .collect(),
        )
        .expect("current");
        let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let opts =
            snapshot_override_options("1:impl:source@1:claude-code-anthropic-model/g1", false);

        let err = preload_snapshot_inherit_before_spawn(
            dir.path(),
            dir.path(),
            &machine,
            task,
            "pending",
            &resolved,
            &settings,
            1,
            None,
            &opts,
        )
        .expect_err("compat none should reject override without bypass");
        assert!(err.to_string().contains("compat: none"));
    }

    #[test]
    fn snapshot_preload_from_snapshot_override_inherit_bypasses_contract_checks() {
        let dir = snapshot_workspace();
        write_snapshot_inherit_machine(
            dir.path(),
            "        name: impl\n        compat: none\n        select:\n          state: source\n",
        );
        let settings = snapshot_preload_settings();
        write_snapshot_generation(
            &snapshot_cache_dir(&settings, dir.path()),
            "1",
            "impl",
            "review",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        refresh_current_links(
            &snapshot_cache_dir(&settings, dir.path()),
            [SnapshotIdentity {
                task_id: "1".to_string(),
                snapshot_name: "impl".to_string(),
                emitting_state: "review".to_string(),
                visit: 1,
                target_slug: "claude-code-anthropic-model".to_string(),
            }]
            .into_iter()
            .collect(),
        )
        .expect("current");
        let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let opts =
            snapshot_override_options("1:impl:review@1:claude-code-anthropic-model/g1", true);

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
            &opts,
        )
        .expect("override-inherit bypasses authored source checks");
        assert_eq!(
            preload
                .parent_ref
                .as_ref()
                .and_then(|value| value.get("emitting_state"))
                .and_then(serde_json::Value::as_str),
            Some("review")
        );
    }

    #[test]
    fn snapshot_reader_ignores_stale_staging_manifests() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        write_snapshot_inherit_machine(
            dir.path(),
            "        name: impl\n        required: true\n        select:\n          state: source\n          target: same\n",
        );
        let settings = snapshot_preload_settings();
        let cache_root = snapshot_cache_dir(&settings, dir.path());
        write_snapshot_generation(
            &cache_root,
            "1",
            "impl",
            "source",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_staging_generation(
            &cache_root,
            "1",
            "impl",
            "source",
            1,
            "claude-code-anthropic-model",
            2,
            "stale",
        );
        refresh_current_links(
            &cache_root,
            [SnapshotIdentity {
                task_id: "1".to_string(),
                snapshot_name: "impl".to_string(),
                emitting_state: "source".to_string(),
                visit: 1,
                target_slug: "claude-code-anthropic-model".to_string(),
            }]
            .into_iter()
            .collect(),
        )
        .expect("current");

        let records = read_snapshot_records(&cache_root).expect("records ignore stale staging");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].generation, 1);

        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        let shown = resolve_snapshot_ref(&ctx, "1:impl:source", None, None).expect("show ref");
        assert_eq!(shown.generation, 1);

        let (loaded, machine, _resolved) = snapshot_preload_parts(dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let inherit = machine
            .states
            .get("pending")
            .and_then(|state| state.snapshot.as_ref())
            .and_then(|snapshot| snapshot.inherit.as_ref())
            .expect("inherit");
        let preloaded = resolve_inherit_snapshot_source(
            &cache_root,
            task,
            "pending",
            inherit,
            "claude-code-anthropic-model",
            1,
        )
        .expect("preload-visible records ignore staging")
        .expect("source snapshot");
        assert_eq!(preloaded.generation, 1);
    }

    #[test]
    fn snapshot_from_snapshot_requires_unique_run_invocation() {
        let dir = snapshot_workspace();
        fs::write(
            dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  source:
    description: source
    target: claude-code:anthropic:model-a
    snapshot:
      emit:
        name: impl
  pending:
    description: pending
    initial: true
    all_targets:
      - claude-code:anthropic:model-a
      - claude-code:anthropic:model-b
    snapshot:
      inherit:
        name: impl
        required: true
        select:
          state: source
          target: same
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
        )
        .expect("write states");
        let settings = snapshot_preload_settings();
        let loaded = load_plan(dir.path()).expect("load plan");
        let machine = rhei_validator::StateMachine::from_yaml_file(dir.path().join("states.yaml"))
            .expect("state machine");
        let task = loaded.rhei.tasks.first().expect("task");
        let resolved =
            resolve_agent_invocations(&machine, "pending", &settings, &default_run_options())
                .expect("resolve");
        assert_eq!(resolved.len(), 2);
        let slug_a = resolved_agent_target_slug(&resolved[0]).expect("slug a");
        let slug_b = resolved_agent_target_slug(&resolved[1]).expect("slug b");
        let cache_root = snapshot_cache_dir(&settings, dir.path());
        for slug in [&slug_a, &slug_b] {
            write_snapshot_generation(
                &cache_root,
                "1",
                "impl",
                "source",
                1,
                slug,
                1,
                "orchestrator",
            );
        }
        refresh_current_links(
            &cache_root,
            [slug_a.clone(), slug_b.clone()]
                .into_iter()
                .map(|target_slug| SnapshotIdentity {
                    task_id: "1".to_string(),
                    snapshot_name: "impl".to_string(),
                    emitting_state: "source".to_string(),
                    visit: 1,
                    target_slug,
                })
                .collect(),
        )
        .expect("current");
        let invocations = resolved
            .iter()
            .cloned()
            .map(|resolved| {
                (
                    "1".to_string(),
                    "pending".to_string(),
                    "pending".to_string(),
                    resolved,
                )
            })
            .collect::<Vec<_>>();
        let mut opts = snapshot_override_options(
            &format!("1:impl:source@1:{slug_a}/g1"),
            false,
        );
        let err = select_snapshot_override_run_invocation(&machine, &opts, &invocations)
            .expect_err("ambiguous run invocation is rejected");
        let msg = err.to_string();
        assert!(msg.contains("ambiguous"));
        assert!(msg.contains(&format!("task=1 target={slug_a}")));
        assert!(msg.contains(&format!("task=1 target={slug_b}")));

        opts.snapshot.snapshot_target = Some(slug_a.clone());
        let selection = select_snapshot_override_run_invocation(&machine, &opts, &invocations)
            .expect("selected")
            .expect("selection");
        assert_eq!(selection.task_id, "1");
        assert_eq!(selection.target_slug, slug_a);

        let preload_a = preload_snapshot_inherit_before_spawn(
            dir.path(),
            dir.path(),
            &machine,
            task,
            "pending",
            &resolved[0],
            &settings,
            1,
            Some(&selection),
            &opts,
        )
        .expect("selected target uses override");
        assert_eq!(
            preload_a
                .parent_ref
                .as_ref()
                .and_then(|value| value.get("target_slug"))
                .and_then(serde_json::Value::as_str),
            Some(selection.target_slug.as_str())
        );

        let preload_b = preload_snapshot_inherit_before_spawn(
            dir.path(),
            dir.path(),
            &machine,
            task,
            "pending",
            &resolved[1],
            &settings,
            1,
            Some(&selection),
            &opts,
        )
        .expect("non-selected target keeps authored inheritance");
        assert_eq!(
            preload_b
                .parent_ref
                .as_ref()
                .and_then(|value| value.get("target_slug"))
                .and_then(serde_json::Value::as_str),
            Some(slug_b.as_str())
        );
    }

    #[test]
    fn snapshot_redactor_replaces_transcript_before_hashing() {
        let dir = snapshot_workspace();
        write_snapshot_emit_machine(dir.path());
        let redactor = write_executable_redactor(dir.path(), "redact.sh", "sed 's/secret/redacted/g'\n");
        let mut settings = RheiSettings { agents: built_in_agents(), ..Default::default() };
        settings.snapshots = Some(SnapshotSettings {
            redactor: Some(redactor),
            ..Default::default()
        });
        let loaded = load_plan(dir.path()).expect("load plan");
        let machine = rhei_validator::StateMachine::from_yaml_file(dir.path().join("states.yaml"))
            .expect("state machine");
        let resolved =
            resolve_agent_invocations(&machine, "pending", &settings, &default_run_options())
                .expect("resolve")
                .remove(0);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "secret\n").expect("log");
        let snapshot_preload =
            snapshot_preload_with_native_session(dir.path(), "redactor-session", b"secret\n");

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
            &snapshot_preload,
        )
        .expect("emit snapshots");

        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        let record = records
            .iter()
            .find(|record| record.snapshot_name == "_state")
            .expect("auto state snapshot");
        let transcript = fs::read(record.transcript_path()).expect("transcript");
        assert_eq!(transcript, b"redacted\n");
        let expected_sha = sha256_hex(b"redacted\n");
        assert_eq!(
            record.manifest.get("transcript_sha256").and_then(serde_json::Value::as_str),
            Some(expected_sha.as_str())
        );
        assert_eq!(
            record.manifest.get("session_id").and_then(serde_json::Value::as_str),
            Some("redactor-session")
        );
        assert!(record.manifest.get("redactor").is_none());
    }

    #[test]
    fn snapshot_redactor_failure_aborts_without_generation() {
        let dir = snapshot_workspace();
        write_snapshot_emit_machine(dir.path());
        let redactor = write_executable_redactor(
            dir.path(),
            "fail-redact.sh",
            "printf 'redactor failed\\n' >&2\nexit 7\n",
        );
        let mut settings = RheiSettings { agents: built_in_agents(), ..Default::default() };
        settings.snapshots = Some(SnapshotSettings {
            redactor: Some(redactor),
            ..Default::default()
        });
        let loaded = load_plan(dir.path()).expect("load plan");
        let machine = rhei_validator::StateMachine::from_yaml_file(dir.path().join("states.yaml"))
            .expect("state machine");
        let resolved =
            resolve_agent_invocations(&machine, "pending", &settings, &default_run_options())
                .expect("resolve")
                .remove(0);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "secret\n").expect("log");
        let snapshot_preload =
            snapshot_preload_with_native_session(dir.path(), "redactor-fail-session", b"secret\n");

        let err = emit_snapshots_after_agent_exit(
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
            &snapshot_preload,
        )
        .expect_err("failing redactor aborts snapshot write");
        assert!(err.to_string().contains("redactor failed"));
        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        assert!(records.is_empty());
    }

    #[test]
    fn snapshot_redactor_receives_minimal_default_env_and_logs_diagnostics() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        write_snapshot_emit_machine(dir.path());
        let env_capture = dir.path().join("redactor-env.txt");
        std::env::set_var("RHEI_REDACTOR_ALLOWED", "allowed-value");
        std::env::set_var("RHEI_REDACTOR_BLOCKED", "blocked-value");
        let redactor = write_executable_redactor(
            dir.path(),
            "env-redact.sh",
            &format!(
                "capture='{}'\n\
printf 'RHEI_EXECUTABLE_PATH=%s\\n' \"$RHEI_EXECUTABLE_PATH\" > \"$capture\"\n\
printf 'RHEI_WORKSPACE_ROOT=%s\\n' \"$RHEI_WORKSPACE_ROOT\" >> \"$capture\"\n\
printf 'RHEI_PROJECT_SETTINGS_PATH=%s\\n' \"$RHEI_PROJECT_SETTINGS_PATH\" >> \"$capture\"\n\
printf 'RHEI_GLOBAL_SETTINGS_PATH=%s\\n' \"$RHEI_GLOBAL_SETTINGS_PATH\" >> \"$capture\"\n\
printf 'RHEI_REDACTOR_ALLOWED=%s\\n' \"$RHEI_REDACTOR_ALLOWED\" >> \"$capture\"\n\
printf 'RHEI_REDACTOR_BLOCKED=%s\\n' \"${{RHEI_REDACTOR_BLOCKED-unset}}\" >> \"$capture\"\n\
printf 'redactor diagnostic\\n' >&2\n\
while IFS= read -r line; do printf '%s\\n' \"$line\"; done\n",
                env_capture.display()
            ),
        );
        let mut settings = RheiSettings { agents: built_in_agents(), ..Default::default() };
        settings.snapshots = Some(SnapshotSettings {
            redactor: Some(redactor),
            redactor_env: vec!["RHEI_REDACTOR_ALLOWED".to_string()],
            ..Default::default()
        });
        let loaded = load_plan(dir.path()).expect("load plan");
        let machine = rhei_validator::StateMachine::from_yaml_file(dir.path().join("states.yaml"))
            .expect("state machine");
        let resolved =
            resolve_agent_invocations(&machine, "pending", &settings, &default_run_options())
                .expect("resolve")
                .remove(0);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "agent log\n").expect("log");
        let snapshot_preload =
            snapshot_preload_with_native_session(dir.path(), "redactor-env-session", b"secret\n");

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
            &snapshot_preload,
        )
        .expect("emit snapshots");

        let captured = fs::read_to_string(&env_capture).expect("captured env");
        assert!(captured.contains("RHEI_EXECUTABLE_PATH="));
        assert!(captured.contains(&format!("RHEI_WORKSPACE_ROOT={}", dir.path().display())));
        assert!(captured.contains(&format!(
            "RHEI_PROJECT_SETTINGS_PATH={}",
            dir.path().join(".rhei/settings.json").display()
        )));
        assert!(captured.contains("RHEI_GLOBAL_SETTINGS_PATH="));
        assert!(captured.contains("RHEI_REDACTOR_ALLOWED=allowed-value"));
        assert!(captured.contains("RHEI_REDACTOR_BLOCKED=unset"));

        let log = fs::read_to_string(&log_path).expect("log");
        assert!(log.contains("snapshot redactor: path="));
        assert!(log.contains("status=exit status: 0"));
        assert!(log.contains("timeout=false"));
        assert!(log.contains("stderr_truncated=false"));
        assert!(log.contains("stderr=redactor diagnostic"));
        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        assert!(records.iter().all(|record| record.manifest.get("redactor").is_none()));
        std::env::remove_var("RHEI_REDACTOR_ALLOWED");
        std::env::remove_var("RHEI_REDACTOR_BLOCKED");
    }

    #[test]
    fn snapshot_pi_emit_records_observed_provider_model_from_header() {
        let dir = snapshot_workspace();
        write_snapshot_emit_machine(dir.path());
        let settings = default_settings();
        let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "log\n").expect("log");
        let preload = snapshot_preload_with_native_session(
            dir.path(),
            "pi-observed",
            br#"{"provider":"anthropic","model":"claude-sonnet-4-6"}
{"role":"assistant","content":"done"}
"#,
        );

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
        .expect("emit pi snapshot");

        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        let state_record =
            records.iter().find(|record| record.snapshot_name == "_state").expect("state record");
        assert_eq!(
            state_record.manifest.get("declared_provider").and_then(serde_json::Value::as_str),
            Some("openai")
        );
        assert_eq!(
            state_record.manifest.get("observed_provider").and_then(serde_json::Value::as_str),
            Some("anthropic")
        );
        assert_eq!(
            state_record.manifest.get("observed_model").and_then(serde_json::Value::as_str),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            snapshot_cache_benefit_reason(state_record, &resolved).as_deref(),
            Some("provider mismatch")
        );
        assert!(records.iter().any(|record| record.snapshot_name == "impl"));
    }

    #[test]
    fn snapshot_pi_emit_falls_back_to_declared_target_when_header_unparsable() {
        let dir = snapshot_workspace();
        write_snapshot_emit_machine(dir.path());
        let settings = default_settings();
        let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
        let task = loaded.rhei.tasks.first().expect("task");
        let log_path = dir.path().join("runtime/logs/task-1-pending.log");
        fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
        fs::write(&log_path, "log\n").expect("log");
        let preload =
            snapshot_preload_with_native_session(dir.path(), "pi-fallback", b"not json\n");

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
        .expect("emit pi snapshot with fallback target");

        let records =
            read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
        let state_record =
            records.iter().find(|record| record.snapshot_name == "_state").expect("state record");
        assert_eq!(
            state_record.manifest.get("observed_provider").and_then(serde_json::Value::as_str),
            Some("openai")
        );
        assert_eq!(
            state_record.manifest.get("observed_model").and_then(serde_json::Value::as_str),
            Some("model")
        );
        assert!(snapshot_cache_benefit_reason(state_record, &resolved).is_none());
    }

    fn snapshot_preload_settings() -> RheiSettings {
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
        RheiSettings { agents, ..Default::default() }
    }

    fn targetless_snapshot_settings() -> RheiSettings {
        let mut agents = BTreeMap::new();
        agents.insert(
            "fake".to_string(),
            CustomAgentProfile {
                command: vec!["fake".to_string()],
                prompt_flag: Some("-p".to_string()),
                session: Some(serde_json::json!({
                    "resume": {"flag": "--resume"},
                    "session_dir_flag": "--session-dir",
                    "layout": {"kind": "FlatById", "ext": "jsonl"}
                })),
                ..Default::default()
            },
        );
        RheiSettings { agents, ..Default::default() }
    }

    fn snapshot_preload_with_native_session(
        workspace: &Path,
        session_id: &str,
        bytes: &[u8],
    ) -> SnapshotPreload {
        let session_dir = workspace.join("runtime/native-sessions").join(session_id);
        fs::create_dir_all(&session_dir).expect("session dir");
        fs::write(session_dir.join(format!("{session_id}.jsonl")), bytes).expect("session file");
        SnapshotPreload { session_dir: Some(session_dir), ..Default::default() }
    }

    fn write_snapshot_inherit_machine(dir: &Path, inherit_yaml: &str) {
        fs::write(
            dir.join("states.yaml"),
            format!(
                r#"name: snapshot-test
version: 1
states:
  source:
    description: source
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: impl
  review:
    description: review
    target: claude-code:anthropic:model
  pending:
    description: pending
    initial: true
    target: claude-code:anthropic:model
    snapshot:
      inherit:
{inherit_yaml}  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#
            ),
        )
        .expect("write states");
    }

    fn write_targetless_snapshot_machine(dir: &Path, snapshot_yaml: &str) {
        fs::write(
            dir.join("states.yaml"),
            format!(
                r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    agent: fake
{snapshot_yaml}  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#
            ),
        )
        .expect("write states");
    }

    fn write_snapshot_emit_machine(dir: &Path) {
        fs::write(
            dir.join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    target: pi:openai:model
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

    fn snapshot_preload_parts(
        dir: &Path,
        settings: &RheiSettings,
    ) -> (LoadedPlan, rhei_validator::StateMachine, ResolvedAgent) {
        let loaded = load_plan(dir).expect("load plan");
        let machine = rhei_validator::StateMachine::from_yaml_file(dir.join("states.yaml"))
            .expect("state machine");
        let resolved =
            resolve_agent_invocations(&machine, "pending", settings, &default_run_options())
                .expect("resolve")
                .remove(0);
        (loaded, machine, resolved)
    }

    fn snapshot_override_options(reference: &str, override_inherit: bool) -> RunOptions {
        let mut opts = default_run_options();
        opts.snapshot.from_snapshot = Some(reference.to_string());
        opts.snapshot.override_inherit = override_inherit;
        opts
    }

    fn write_executable_redactor(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).expect("write redactor");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&path).expect("redactor metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).expect("chmod redactor");
        }
        path
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
    fn write_snapshot_staging_generation(
        cache_root: &Path,
        task_id: &str,
        name: &str,
        state: &str,
        visit: u64,
        target_slug: &str,
        generation: u64,
        suffix: &str,
    ) {
        write_snapshot_generation_at_dir(
            cache_root,
            task_id,
            name,
            state,
            visit,
            target_slug,
            generation,
            "orchestrator",
            "2026-05-18T08:14:22Z",
            &format!("g{generation}.tmp-{suffix}"),
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
        write_snapshot_generation_at_dir(
            cache_root,
            task_id,
            name,
            state,
            visit,
            target_slug,
            generation,
            produced_by,
            created_at,
            &format!("g{generation}"),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn write_snapshot_generation_at_dir(
        cache_root: &Path,
        task_id: &str,
        name: &str,
        state: &str,
        visit: u64,
        target_slug: &str,
        generation: u64,
        produced_by: &str,
        created_at: &str,
        generation_dir_name: &str,
    ) {
        let dir = cache_root
            .join(task_id)
            .join(name)
            .join(state)
            .join(visit.to_string())
            .join(target_slug)
            .join(generation_dir_name);
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
