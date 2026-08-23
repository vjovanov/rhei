    // Which line of a result file opens the entry that stands, and which file
    // a summary is read from at all: the two questions every memory section
    // asks of a result. Fixtures are the ones in `tests_prompt_memory.rs`.

    // §FS-rhei-memory.4.3

    /// §FS-rhei-memory.4.3: a result file quotes as often as it reports, and
    /// a fenced example read as an entry becomes the standing verdict — the
    /// opposite of what the file says.
    #[test]
    fn a_result_heading_inside_a_fence_is_not_an_entry() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.1 review@completed\n"),
            (
                "runtime/results/plan.1.1.md",
                "## Result\n\nAPPROVED: the change is safe to ship.\n\n\
                 The prompt the agent received looked like this:\n\n\
                 ```markdown\n## Result \u{2014} codex\n\n\
                 BLOCKED, do not ship (this is a quotation, not a verdict).\n```\n\n\
                 That quotation is only an illustration.\n",
            ),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.4").expect("task 1.4");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                "- Task plan.1.1: Implement \u{2014} completed \u{2014} APPROVED: the change \
                 is safe to ship.\n"
            ),
            "got:\n{history}"
        );
        assert!(!history.contains("BLOCKED"), "the quotation became the verdict:\n{history}");
    }

    /// §FS-rhei-memory.4.3: an unclosed fence runs to the end of the file, the
    /// way a renderer reads it, so nothing after it opens an entry either.
    #[test]
    fn an_unclosed_fence_swallows_the_headings_below_it() {
        let dir = memory_plan_dir(&[
            ("runtime/state-transitions.log", "plan.1.1 review@completed\n"),
            (
                "runtime/results/plan.1.1.md",
                "## Result\n\nAPPROVED: the change is safe to ship.\n\n\
                 Everything below is the transcript I was handed:\n\n\
                 ~~~\n## Result \u{2014} codex\n\n\
                 BLOCKED, do not ship (this is a quotation, not a verdict).\n",
            ),
        ]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let memory =
            prompt_memory(&loaded, &plan_path, &dir.path().join("runtime"), BTreeSet::new());
        let machine = memory_machine();
        let task = find_task_by_id_str(&loaded.rhei.tasks, "plan.1.4").expect("task 1.4");
        let context =
            memory_context(dir.path(), &plan_path, &loaded, &memory, &machine, task, "pending");

        let history = render_plan_history(&context).expect("history");
        assert!(
            history.contains(
                "- Task plan.1.1: Implement \u{2014} completed \u{2014} APPROVED: the change \
                 is safe to ship.\n"
            ),
            "got:\n{history}"
        );
        assert!(!history.contains("BLOCKED"), "the quotation became the verdict:\n{history}");
    }

    /// §FS-rhei-plan-language.3.8: the block names an artifact of the owning
    /// rhei and nothing else — a target that leaves the execution root would
    /// point the prompt composer at any file on the machine.
    #[test]
    fn a_legacy_result_link_resolves_only_inside_the_owning_root() {
        let dir = memory_dir(&[(
            "plan.rhei.md",
            "# Rhei: Legacy\n\n## Tasks\n\n\
             ### Task 1: Absolute target\n**State:** completed\n\n\
             > **Result:** [1](/etc/passwd)\n\n\
             ### Task 2: Climbing target\n**State:** completed\n\n\
             > **Result:** [2](../elsewhere/2.md)\n\n\
             ### Task 3: Relative target\n**State:** completed\n\n\
             > **Result:** [3](runtime/results/3.md)\n",
        )]);
        let plan_path = dir.path().join("plan.rhei.md");
        let loaded = load_plan(&plan_path).expect("plan loads");
        let root = dir.path();
        let path_for = |id: &str| {
            let task = find_task_by_id_str(&loaded.rhei.tasks, id).expect("task");
            legacy_result_path(root, task)
        };

        assert_eq!(path_for("plan.1"), None, "an absolute target is not this rhei's artifact");
        assert_eq!(path_for("plan.2"), None, "a climbing target leaves the execution root");
        assert_eq!(
            path_for("plan.3"),
            Some(root.join("runtime/results/3.md")),
            "a relative target resolves against the owning rhei's root"
        );
    }

    /// §FS-rhei-memory.1.2: a root reached through a symlink and a directory
    /// the run resolved canonically are one place, and the prompt spells both
    /// the canonical way — including a directory that does not exist yet.
    #[cfg(unix)]
    #[test]
    fn a_path_is_spelled_canonically_even_before_it_exists() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join("alpha")).expect("real root");
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        let root = real.canonicalize().expect("canonical root");

        assert_eq!(canonical_spelling(&link.join("alpha")), Some(root.join("alpha")));
        assert_eq!(canonical_spelling(&link.join("runtime/logs")), Some(root.join("runtime/logs")));
        assert_eq!(canonical_spelling(&root.join("alpha")), Some(root.join("alpha")));
        assert_eq!(canonical_spelling(Path::new("no-such-root/anywhere")), None);
    }
