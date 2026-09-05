// What the run report's accounting strip and its task cost rows must count.
// `UsageReported` arrives repeatedly for one invocation as a streaming
// extractor observes more turns, and arrives after the slot is released, so
// both levels upsert on invocation id rather than appending. Included into
// `mod run_summary_tests` in `run_summary.rs`, so the indentation is the
// module's and `use super::*` is already in scope from `tests_run_summary.rs`.

// §AR-source-file-size.3 §FS-rhei-cost-accounting.9

    /// The ticket's own invocation id shape: `<task>::<state>::<agent>::<visit>`.
    const INVOCATION: &str = "t1::work::codex::visit-1";

    /// A dimension an extractor measured, as one invocation reports it.
    fn measured(value: u64) -> rhei_tui::DimensionSummary {
        rhei_tui::DimensionSummary {
            value: Some(value),
            status: rhei_tui::DimensionStatus::Measured,
            missing_count: 0,
            measured_count: 1,
        }
    }

    /// One invocation's usage. `total` and `output_total` carry separate numbers
    /// so a rollup that only gets `invocation_count` right cannot pass.
    fn usage(
        invocation_id: &str,
        total: u64,
        output_total: u64,
        cost_micro: u64,
    ) -> rhei_tui::UsageSummary {
        rhei_tui::UsageSummary {
            invocation_id: invocation_id.to_string(),
            state: "work".to_string(),
            agent: "codex".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-test".to_string()),
            total: measured(total),
            input_total: measured(total - output_total),
            input_cached_read: measured(0),
            input_cache_write: measured(0),
            output_total: measured(output_total),
            output_cached_read: measured(0),
            output_cache_write: measured(0),
            cost_micro: Some(cost_micro),
            priced_cost_micro: Some(cost_micro),
            currency: Some("USD".to_string()),
            coverage: rhei_tui::UsageCoverage::Complete,
            status: rhei_tui::UsageStatus::Measured,
            pricing_status: rhei_tui::PricingStatus::Priced,
        }
    }

    fn report_usage(
        sink: &SummarySink,
        task: &str,
        slot: Option<u16>,
        usage: rhei_tui::UsageSummary,
    ) {
        use rhei_tui::EventSink;
        sink.emit(rhei_tui::RunEvent::UsageReported {
            slot,
            task: task.to_string(),
            invocation_id: usage.invocation_id.clone(),
            usage,
        });
    }

    fn assign_slot(sink: &SummarySink, task: &str) {
        use rhei_tui::EventSink;
        sink.emit(rhei_tui::RunEvent::SlotAssigned {
            slot: 0,
            task: task.to_string(),
            from: "work".to_string(),
            to: "completed".to_string(),
            agent: Some("codex".to_string()),
            template_context: None,
            log_path: std::path::PathBuf::from("runtime/logs/x.log"),
            started_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
        });
    }

    fn release_slot(sink: &SummarySink, task: &str) {
        use rhei_tui::EventSink;
        sink.emit(rhei_tui::RunEvent::SlotReleased {
            slot: 0,
            task: task.to_string(),
            from: "work".to_string(),
            to: "completed".to_string(),
            log_path: std::path::PathBuf::from("runtime/logs/x.log"),
            outcome: rhei_tui::TaskOutcome::Completed,
            finished_at: std::time::Instant::now(),
            wall_clock: std::time::SystemTime::now(),
            exit_code: Some(0),
            duration_ms: 1_200,
        });
    }

    /// The task's own accounting, as the console task row and the report's task
    /// table read it out of the snapshot.
    fn task_accounting(sink: &SummarySink, task: &str) -> rhei_tui::AccountingRunSummary {
        sink.snapshot()
            .get(task)
            .and_then(|activity| activity.accounting.clone())
            .expect("the task carries direct accounting")
    }

    /// The ticket's case. One invocation reported twice, with the slot released
    /// between the reports, is one invocation on both surfaces — not two, and
    /// not twice the tokens. Appending the second report made an aborted run's
    /// cost strip and every run's task cost row read double.
    // §FS-rhei-cost-accounting.9
    #[test]
    fn repeated_usage_report_for_one_invocation_counts_once() {
        let sink = SummarySink::new();
        assign_slot(&sink, "t1");
        report_usage(&sink, "t1", Some(0), usage(INVOCATION, 1_280_000, 96_000, 21_000));
        release_slot(&sink, "t1");
        // The same invocation reports again after its slot is gone.
        report_usage(&sink, "t1", None, usage(INVOCATION, 1_280_000, 96_000, 21_000));

        let run = sink.accounting().run.expect("the run carries fallback accounting");
        assert_eq!(run.invocation_count, 1, "run rollup: {run:?}");
        assert_eq!(run.total.value, Some(1_280_000), "run rollup: {run:?}");
        assert_eq!(run.output_total.value, Some(96_000), "run rollup: {run:?}");
        assert_eq!(run.cost_micro, Some(21_000), "run rollup: {run:?}");

        let task = task_accounting(&sink, "t1");
        assert_eq!(task.invocation_count, 1, "task rollup: {task:?}");
        assert_eq!(task.total.value, Some(1_280_000), "task rollup: {task:?}");
        assert_eq!(task.output_total.value, Some(96_000), "task rollup: {task:?}");
        assert_eq!(task.cost_micro, Some(21_000), "task rollup: {task:?}");
    }

    /// A streaming extractor's later report is the truth: it replaces what the
    /// same invocation reported before, rather than being added to it or
    /// dropped in favour of it. Two identical reports cannot tell those apart,
    /// so the second one here is larger than the first.
    // §FS-rhei-cost-accounting.9
    #[test]
    fn a_later_usage_report_replaces_the_earlier_one_for_the_same_invocation() {
        let sink = SummarySink::new();
        assign_slot(&sink, "t1");
        report_usage(&sink, "t1", Some(0), usage(INVOCATION, 400_000, 30_000, 7_000));
        release_slot(&sink, "t1");
        report_usage(&sink, "t1", None, usage(INVOCATION, 1_280_000, 96_000, 21_000));

        for (level, rollup) in [
            ("run", sink.accounting().run.expect("the run carries fallback accounting")),
            ("task", task_accounting(&sink, "t1")),
        ] {
            assert_eq!(rollup.invocation_count, 1, "{level} rollup: {rollup:?}");
            // The later report's numbers, not the earlier ones (400_000) and not
            // their sum (1_680_000).
            assert_eq!(rollup.total.value, Some(1_280_000), "{level} rollup: {rollup:?}");
            assert_eq!(rollup.output_total.value, Some(96_000), "{level} rollup: {rollup:?}");
            assert_eq!(rollup.cost_micro, Some(21_000), "{level} rollup: {rollup:?}");
        }
    }

    /// The guard on the upsert: two genuinely different invocations still count
    /// as two and still add up. Keying on invocation id must not collapse the
    /// case that was always right.
    // §FS-rhei-cost-accounting.9
    #[test]
    fn distinct_invocation_ids_still_roll_up_to_their_sum() {
        let sink = SummarySink::new();
        assign_slot(&sink, "t1");
        report_usage(&sink, "t1", Some(0), usage(INVOCATION, 400_000, 30_000, 7_000));
        release_slot(&sink, "t1");
        report_usage(
            &sink,
            "t1",
            Some(0),
            usage("t1::review::codex::visit-1", 1_280_000, 96_000, 21_000),
        );

        for (level, rollup) in [
            ("run", sink.accounting().run.expect("the run carries fallback accounting")),
            ("task", task_accounting(&sink, "t1")),
        ] {
            assert_eq!(rollup.invocation_count, 2, "{level} rollup: {rollup:?}");
            assert_eq!(rollup.total.value, Some(1_680_000), "{level} rollup: {rollup:?}");
            assert_eq!(rollup.output_total.value, Some(126_000), "{level} rollup: {rollup:?}");
            assert_eq!(rollup.cost_micro, Some(28_000), "{level} rollup: {rollup:?}");
        }
    }
