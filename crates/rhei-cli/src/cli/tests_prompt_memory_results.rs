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
