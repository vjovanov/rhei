        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan.rhei.md");
        let states = dir.path().join("states.yaml");
        let rhei_dir = dir.path().join(".rhei");
        fs::create_dir_all(&rhei_dir).expect("mkdir");
        let spawned = dir.path().join("spawned");
        let script = dir.path().join("fake-agent.sh");
        fs::write(&script, format!("#!/bin/sh\ntouch '{}'\n", spawned.display())).expect("script");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).expect("chmod");
        }
        fs::write(
            &plan,
            r#"# Rhei: Missing Skill

## Tasks

### Task 1: Work
**State:** pending
"#,
        )
        .expect("plan");
        fs::write(
            &states,
            r#"name: missing-skill
version: 1
states:
  pending:
    description: pending
    agent: fake
    agent_timeout: 1s
    skills:
      - missing
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
        )
        .expect("states");
        fs::write(
            rhei_dir.join("settings.json"),
            format!(
                r#"{{
                  "agents": {{
                    "fake": {{
                      "command": [{}],
                      "prompt_flag": "--prompt"
                    }}
                  }},
                  "skills": {{
                    "missing": {{ "path": "{}" }}
                  }}
                }}"#,
                serde_json::to_string(script.to_string_lossy().as_ref()).expect("json"),
                dir.path().join("does-not-exist").display()
            ),
        )
        .expect("settings");

        let mut opts = default_run_options();
        opts.standalone.continue_on_error = true;
        opts.standalone.no_tui = true;
        run_command(&plan, Some(&states), opts).expect("run skips unavailable tooling");

        assert!(!spawned.exists(), "required missing skill must block agent spawn");
    }

    #[cfg(unix)]
    #[test]
    fn agent_spawn_outcome_carries_resolved_timeout() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let script = write_sleeping_fake_agent(dir.path());
        let log_path = dir.path().join("agent.log");
        let recorder = Arc::new(RecordingSink::default());
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("codex"),
            profile: CustomAgentProfile {
                command: vec![script.display().to_string()],
                ..CustomAgentProfile::default()
            },
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(1),
            autonomous_args: Vec::new(),
        };
        let tooling = ResolvedTooling::default();

        let status = spawn_and_wait_agent(
            &resolved,
            "prompt",
            dir.path(),
            dir.path(),
            None,
            "task-timeout",
            "pending",
            &tooling,
            &log_path,
            dir.path(),
            None,
            0,
            recorder,
        )
        .expect("timeout returns process status");

        assert!(status.timed_out);
        assert_eq!(status.timeout_secs, Some(1));
    }

    #[test]
    fn parses_snapshot_list_command() {
        let cli = Cli::try_parse_from([
            "rhei",
            "snapshot",
            "list",
            "--plan",
            "plan",
            "--task",
            "1",
            "--name",
            "_state",
            "--state",
            "pending",
            "--produced-by",
            "all",
            "--format",
            "json",
        ])
        .expect("cli should parse");

        match cli.command {
            Commands::Snapshot {
                command: SnapshotCommand::List { plan, task, name, state, produced_by, format, .. },
            } => {
                assert_eq!(plan, PathBuf::from("plan"));
                assert_eq!(task.as_deref(), Some("1"));
                assert_eq!(name.as_deref(), Some("_state"));
                assert_eq!(state.as_deref(), Some("pending"));
                assert_eq!(produced_by, SnapshotProducedByFilter::All);
                assert_eq!(format, SnapshotListFormat::Json);
            }
            other => panic!("expected snapshot list command, got {other:?}"),
        }
    }

    #[test]
    fn snapshot_ref_parser_prefers_named_snapshot_over_auto_shorthand() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "1",
            "pending",
            "review",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "1",
            "_state",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );

        let resolved = resolve_snapshot_ref(&ctx, "1:pending", None, None).expect("resolve ref");
        assert_eq!(resolved.snapshot_name, "pending");
        assert_eq!(resolved.emitting_state, "review");
    }

    #[test]
    fn snapshot_ref_parser_reports_ambiguous_broad_match() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "1",
            "impl",
            "review",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );

        let err = resolve_snapshot_ref(&ctx, "1:impl", None, None).expect_err("ambiguous ref");
        let msg = err.to_string();
        assert!(msg.contains("ambiguous"));
        assert!(msg.contains("1:impl:pending@1:claude-code-anthropic-model/g1"));
        assert!(msg.contains("1:impl:review@1:claude-code-anthropic-model/g1"));
    }

    #[test]
    fn snapshot_gc_keep_generations_ignores_operator_by_default() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            2,
            "operator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            3,
            "orchestrator",
        );

        snapshot_gc_command(&ctx, None, Some("impl"), None, Some(1), false, false, false, false)
            .expect("gc succeeds");

        let identity = ctx
            .cache_root
            .join("1")
            .join("impl")
            .join("pending")
            .join("1")
            .join("claude-code-anthropic-model");
        assert!(!identity.join("g1").exists(), "old orchestrator generation deleted");
        assert!(identity.join("g2").exists(), "operator generation ignored by default");
        assert!(identity.join("g3").exists(), "newest orchestrator generation retained");
    }

    #[test]
    fn snapshot_orphan_detection_recurses_into_child_tasks() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        write_snapshot_workspace_task(
            dir.path(),
            "### Task 1: Implement\n**State:** pending\n\nDo work.\n\n#### Task 1.1: Child\n**State:** pending\n\nDo child work.\n",
        );
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "1.1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        let record = resolve_snapshot_ref(&ctx, "1.1:impl", None, None).expect("resolve ref");

        assert!(!is_snapshot_orphaned(&record, &ctx), "child task snapshot must not be orphaned");
    }

    #[test]
    fn snapshot_gc_keep_generations_counts_newer_records_before_older_than() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation_with_created_at(
            &ctx.cache_root,
            "1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
            "1970-01-01T00:00:00Z",
        );
        write_snapshot_generation_with_created_at(
            &ctx.cache_root,
            "1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            2,
            "orchestrator",
            "1970-01-01T00:00:00Z",
        );
        write_snapshot_generation_with_created_at(
            &ctx.cache_root,
            "1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            3,
            "orchestrator",
            "2999-01-01T00:00:00Z",
        );

        snapshot_gc_command(
            &ctx,
            None,
            Some("impl"),
            Some("1s"),
            Some(2),
            false,
            false,
            false,
            false,
        )
        .expect("gc succeeds");

        let identity = ctx
            .cache_root
            .join("1")
            .join("impl")
            .join("pending")
            .join("1")
            .join("claude-code-anthropic-model");
        assert!(!identity.join("g1").exists(), "old generation outside retention deleted");
        assert!(identity.join("g2").exists(), "old generation inside retention retained");
        assert!(identity.join("g3").exists(), "new generation counted for retention");
    }

    #[test]
    fn snapshot_active_inherit_protection_recurses_into_child_tasks() {
        let _home = TempHome::new();
        let dir = snapshot_workspace();
        write_snapshot_workspace_task(
            dir.path(),
            "### Task 1: Implement\n**State:** completed\n\nDo work.\n\n#### Task 1.1: Child\n**State:** review\n\nDo child work.\n",
        );
        fs::write(
            dir.path().join("states.yaml"),
            r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    target: claude-code:anthropic:model
    snapshot:
      emit:
        name: impl
  review:
    description: review
    target: claude-code:anthropic:model
    snapshot:
      inherit:
        name: impl
        select:
          state: pending
          generation: latest
  completed:
    description: completed
    final: true
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
"#,
        )
        .expect("write states");
        let ctx = load_snapshot_context(dir.path(), None).expect("snapshot context");
        write_snapshot_generation(
            &ctx.cache_root,
            "1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            1,
            "orchestrator",
        );
        write_snapshot_generation(
            &ctx.cache_root,
            "1",
            "impl",
            "pending",
            1,
            "claude-code-anthropic-model",
            2,
            "orchestrator",
        );
        let latest = resolve_snapshot_ref(&ctx, "1:impl/g2", None, None).expect("resolve ref");

        assert!(
            snapshot_generation_protected_by_active_inherit(&latest, &ctx),
            "child task active inherit must protect selected generation"
        );
    }

    #[test]
    fn snapshot_emit_writes_auto_and_named_generations() {
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
        let loaded = load_plan(dir.path()).expect("load plan");
