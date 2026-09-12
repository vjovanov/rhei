// The two sites that emit `UsageReported`, and which report each of them names.
// These are what a line-oriented frontend tells apart, so the naming is pinned
// at the emitters rather than only end to end.
// §FS-rhei-cost-accounting.7.1

/// Every report the streaming emitter puts on the sink is `Streamed`: it fires
/// once per turn the extractor measures, and each one is re-summed from the
/// whole capture, so it is a running total rather than the invocation's cost.
// §FS-rhei-cost-accounting.7.1
#[test]
fn the_streaming_emitter_reports_streamed() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let recorder = Arc::new(RecordingSink::default());
    let sink: Arc<dyn rhei_tui::EventSink> = recorder.clone();
    let capture = AgentUsageCapture {
        extractor: AgentUsageExtractor::Codex,
        replace_usage_capture: false,
        path: dir.path().join("usage.jsonl"),
        invocation_id: "1::work::codex::visit-1".to_string(),
        task_id: "1".to_string(),
        state: "work".to_string(),
        agent: "codex".to_string(),
        provider: Some("openai".to_string()),
        model: Some("gpt-5.6-luna".to_string()),
        price_book: builtin_price_book(),
        slot: 0,
        cli_session: Arc::new(Mutex::new(None)),
    };

    for input_tokens in [1_250_000u64, 250_000] {
        let line = serde_json::json!({
            "type": "turn.completed",
            "usage": {
                "input_tokens": input_tokens,
                "cached_input_tokens": 0,
                "cache_creation_input_tokens": 0,
                "output_tokens": 1_000,
            },
        })
        .to_string();
        capture_agent_output_usage(Some(&capture), rhei_tui::AgentStream::Stdout, &line, &sink);
    }

    assert_eq!(recorded_reports(&recorder), [rhei_tui::UsageReport::Streamed; 2]);
}

/// The record-writing emitter fires once, after the durable record, and names
/// its one report `Final` — the only report a line-oriented frontend prints.
// §FS-rhei-cost-accounting.7.1
#[test]
fn the_record_writing_emitter_reports_final() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let capture_path = dir.path().join("usage.jsonl");
    append_usage_capture_event(
        &capture_path,
        ExtractedUsage {
            input_total: Some(1_500_000),
            output_total: Some(2_000),
            ..ExtractedUsage::default()
        },
        false,
    )
    .expect("write usage capture");

    let mut profile = built_in_agents().remove("codex").expect("codex profile");
    profile.command = vec!["true".to_string()];
    let resolved = ResolvedAgent {
        agent: AgentConfig::from("codex"),
        profile,
        mode: None,
        target: None,
        model: Some("luna".to_string()),
        model_provider: Some("openai".to_string()),
        model_name: Some("gpt-5.6-luna".to_string()),
        timeout_secs: Some(10),
        autonomous_args: Vec::new(),
    };
    let plan =
        rhei_core::parse("# Rhei: Final Report\n\n## Tasks\n\n### Task 1: Work\n**State:** work\n")
            .expect("parse plan");
    let recorder = Arc::new(RecordingSink::default());
    let sink_trait: Arc<dyn rhei_tui::EventSink> = recorder.clone();

    record_agent_accounting_invocation(AgentAccountingInvocation {
        workspace_root: dir.path(),
        task: &plan.tasks[0],
        state: "work",
        resolved: &resolved,
        visit: 1,
        started_at: std::time::SystemTime::now(),
        ended_at: std::time::SystemTime::now(),
        slot: Some(0),
        usage_capture_path: Some(&capture_path),
        cli_session: None,
        log_path: None,
        price_book: &builtin_price_book(),
        sink: &sink_trait,
    })
    .expect("record accounting")
    .expect("the invocation was measured");

    assert_eq!(recorded_reports(&recorder), [rhei_tui::UsageReport::Final]);
}

/// Which report each `UsageReported` the sink saw carries, in order.
fn recorded_reports(recorder: &Arc<RecordingSink>) -> Vec<rhei_tui::UsageReport> {
    recorder
        .events
        .lock()
        .expect("recording sink lock")
        .iter()
        .filter_map(|event| match event {
            rhei_tui::RunEvent::UsageReported { report, .. } => Some(*report),
            _ => None,
        })
        .collect()
}
