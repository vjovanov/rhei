    // The role-specific edge of Position: a state declaration, rather than
    // tree shape, decides whether standing context is delivered or linked.

    /// §FS-rhei-memory.3.1 §FS-rhei-memory.4.2: tasks at the same depth differ
    /// only by whether their current state declares `execute_on`. A supervisor
    /// keeps the position and navigation map without receiving either broad
    /// context body; an ordinary worker still receives both bodies verbatim.
    #[test]
    fn supervisor_prompt_omits_repository_context_while_worker_keeps_it() {
        let dir = memory_dir(&[
            (
                "index.panta.md",
                "# Panta: Role Test\n\n## Project Rules\n\nPROJECT-CONTEXT-EVIDENCE\n",
            ),
            (
                "flow.rhei.md",
                "# Rhei: Role Test\n\n## Rhei Rules\n\nRHEI-CONTEXT-EVIDENCE\n\n## Tasks\n\n\
                 ### Task 1: Coordinate\n**State:** guiding\n\nKeep the selected work aligned.\n\n\
                 ### Task 2: Implement\n**State:** working\n\nApply the selected change.\n",
            ),
        ]);
        let project = dir.path().to_path_buf();
        let loaded = load_plan(&project).expect("project loads");
        let memory = prompt_memory(&loaded, &project, &project.join("runtime"), BTreeSet::new());
        let machine = rhei_validator::StateMachine::from_yaml_str(
            r#"
name: role-test
version: 1
states:
  guiding:
    initial: true
    description: Coordinate selected work
    execute_on: child-terminal
    agent: mock
    instructions: Coordinate the selected work.
  working:
    description: Implement selected work
    instructions: Implement the selected work.
  completed:
    description: Done
    final: true
transitions:
  - { from: guiding, to: guiding }
  - { from: guiding, to: completed }
  - { from: working, to: completed }
"#,
        )
        .expect("machine parses");

        let worker = find_task_by_id_str(&loaded.rhei.tasks, "flow.2").expect("worker task");
        let worker_context =
            memory_context(&project, &project, &loaded, &memory, &machine, worker, "working");
        let worker_position = render_position(&worker_context);
        assert!(worker_position.contains("### Rhei Context"), "got:\n{worker_position}");
        assert!(worker_position.contains("RHEI-CONTEXT-EVIDENCE"), "got:\n{worker_position}");
        assert!(worker_position.contains("### Project Context"), "got:\n{worker_position}");
        assert!(worker_position.contains("PROJECT-CONTEXT-EVIDENCE"), "got:\n{worker_position}");

        let supervisor =
            find_task_by_id_str(&loaded.rhei.tasks, "flow.1").expect("supervisor task");
        let supervisor_context =
            memory_context(&project, &project, &loaded, &memory, &machine, supervisor, "guiding");
        let navigation = render_rhei_navigation(&supervisor_context);
        assert!(navigation.contains("plan `flow.rhei.md`"), "got:\n{navigation}");
        assert!(navigation.contains("rhei render <plan> --format json --pretty"), "got:\n{navigation}");

        let position = render_position(&supervisor_context);
        assert!(position.contains("**Task flow.1: Coordinate [guiding]**"), "got:\n{position}");
        let embedded: Vec<&str> = ["RHEI-CONTEXT-EVIDENCE", "PROJECT-CONTEXT-EVIDENCE"]
            .into_iter()
            .filter(|marker| position.contains(marker))
            .collect();
        assert!(
            embedded.is_empty()
                && !position.contains("### Rhei Context")
                && !position.contains("### Project Context"),
            "supervisor Position embedded {} broad context marker(s) {:?}; expected only direct navigation:\n{}",
            embedded.len(),
            embedded,
            position
        );
    }
