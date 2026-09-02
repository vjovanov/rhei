#[test]
fn accounting_session_ids_are_extracted_only_from_typed_agent_events() {
    // §FS-rhei-cost-accounting.3.4: built-in structured transports expose the
    // native session identity without accepting nearby arbitrary JSON.
    for (extractor, line, expected) in [
        (
            AgentUsageExtractor::Claude,
            r#"{"type":"result","session_id":"claude-session"}"#,
            "claude-session",
        ),
        (
            AgentUsageExtractor::Codex,
            r#"{"type":"thread.started","thread_id":"codex-thread"}"#,
            "codex-thread",
        ),
        (
            AgentUsageExtractor::Pi,
            r#"{"type":"session","id":"pi-session"}"#,
            "pi-session",
        ),
    ] {
        let session =
            extract_cli_session_from_output_line(extractor, line).expect("typed session event");
        assert_eq!(session.id, expected);
        assert_eq!(session.store_path, None);
    }

    assert!(extract_cli_session_from_output_line(
        AgentUsageExtractor::Codex,
        r#"{"type":"item.completed","thread_id":"not-a-session-event"}"#,
    )
    .is_none());
}

#[test]
fn claude_session_capture_survives_cumulative_usage_replacement() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = AgentUsageCapture {
        extractor: AgentUsageExtractor::Claude,
        replace_usage_capture: true,
        path: dir.path().join("usage.jsonl"),
        invocation_id: "1::work::claude-code::visit-1".to_string(),
        task_id: "1".to_string(),
        state: "work".to_string(),
        agent: "claude-code".to_string(),
        provider: Some("anthropic".to_string()),
        model: Some("claude-sonnet-4-6".to_string()),
        price_book: builtin_price_book(),
        slot: 0,
        cli_session: Arc::new(Mutex::new(None)),
    };
    let sink: Arc<dyn rhei_tui::EventSink> = Arc::new(RecordingSink::default());

    for (session, input) in [("first-session", 10), ("stable-session", 20)] {
        capture_agent_output_usage(
            Some(&capture),
            rhei_tui::AgentStream::Stdout,
            &format!(
                r#"{{"type":"result","session_id":"{session}","result":"ok","usage":{{"input_tokens":{input},"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"output_tokens":5}}}}"#
            ),
            &sink,
        );
    }

    let observed = capture.cli_session.lock().expect("session capture").clone();
    assert_eq!(observed.expect("session").id, "stable-session");
    match extract_usage_from_capture(Some(&capture.path)) {
        ExtractedUsageStatus::Measured(usage) => assert_eq!(usage.input_total, Some(20)),
        _ => panic!("latest cumulative usage should remain measurable"),
    }
}

#[test]
fn old_invocation_records_remain_readable_without_new_optional_fields() {
    // §FS-rhei-cost-accounting.3.4 §FS-rhei-cost-accounting.8.1: v1 readers
    // remain compatible with records written before session and duration.
    let dir = tempfile::tempdir().expect("tempdir");
    let invocation_dir = dir.path().join("invocations");
    fs::create_dir_all(&invocation_dir).expect("invocation directory");
    let value = serde_json::to_value(accounting_test_record()).expect("old record value");
    assert!(value.get("duration_ms").is_none());
    assert!(value.get("cli_session").is_none());
    fs::write(
        invocation_dir.join("old.json"),
        serde_json::to_vec_pretty(&value).expect("serialize old record"),
    )
    .expect("write old record");

    let inspection = read_cost_inspection(dir.path());
    assert!(inspection.errors.is_empty(), "{:?}", inspection.errors);
    assert_eq!(inspection.invocations.len(), 1);
    let record = &inspection.invocations[0].1;
    assert_eq!(record.duration_ms, None);
    assert_eq!(record.cli_session, None);
    assert_eq!(record.pricing.status, "unpriced");
}

#[test]
fn accounting_duration_uses_the_record_wall_clocks() {
    let started = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_000);
    let ended = started + std::time::Duration::from_millis(2_345);
    assert_eq!(accounting_duration_ms(started, ended), 2_345);
}
