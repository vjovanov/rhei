fn accounting_test_record() -> AccountingInvocationRecord {
    AccountingInvocationRecord {
        schema: ACCOUNTING_INVOCATION_SCHEMA.to_string(),
        invocation_id: "1::work::codex::visit-1".to_string(),
        task_id: "1".to_string(),
        state: "work".to_string(),
        visit: 1,
        target_slug: None,
        agent: "codex".to_string(),
        provider: Some("openai".to_string()),
        model: Some("gpt-test".to_string()),
        started_at: "2026-05-20T10:00:00Z".to_string(),
        ended_at: "2026-05-20T10:00:00Z".to_string(),
        extraction_status: "measured".to_string(),
        scope: "aggregate-agent-process".to_string(),
        tokens: AccountingTokens::default(),
        pricing: AccountingPricing {
            status: "unpriced".to_string(),
            currency: Some("USD".to_string()),
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: Some(PRICE_BOOK_ID.to_string()),
        },
    }
}

fn accounting_usage(
    coverage: rhei_tui::UsageCoverage,
    pricing_status: rhei_tui::PricingStatus,
    cost_micro: Option<u64>,
    priced_cost_micro: Option<u64>,
) -> rhei_tui::UsageSummary {
    let measured = rhei_tui::DimensionSummary {
        value: Some(1),
        status: rhei_tui::DimensionStatus::Measured,
        missing_count: 0,
        measured_count: 1,
    };
    rhei_tui::UsageSummary {
        invocation_id: format!("{pricing_status:?}-{coverage:?}"),
        state: "work".to_string(),
        agent: "codex".to_string(),
        provider: Some("openai".to_string()),
        model: Some("gpt-test".to_string()),
        total: measured.clone(),
        input_total: measured.clone(),
        input_cached_read: measured.clone(),
        input_cache_write: measured.clone(),
        output_total: measured.clone(),
        output_cached_read: measured.clone(),
        output_cache_write: measured,
        cost_micro,
        priced_cost_micro,
        currency: Some("USD".to_string()),
        coverage,
        status: rhei_tui::UsageStatus::Measured,
        pricing_status,
    }
}

#[test]
fn accounting_invocation_file_ids_are_unique_for_fast_reruns() {
    let record = accounting_test_record();

    assert_ne!(invocation_file_id(&record), invocation_file_id(&record));
}

#[test]
fn accounting_task_file_segments_do_not_collapse_valid_task_ids() {
    assert_eq!(safe_accounting_file_segment("build.api"), "build.api");
    assert_eq!(safe_accounting_file_segment("build_api"), "build_api");
    assert_ne!(
        safe_accounting_file_segment("build.api"),
        safe_accounting_file_segment("build_api")
    );
    assert_eq!(safe_accounting_file_segment("build/api"), "build%2Fapi");
}

#[test]
fn accounting_mixed_priced_and_unpriced_rollup_is_partial() {
    let priced = accounting_usage(
        rhei_tui::UsageCoverage::Complete,
        rhei_tui::PricingStatus::Priced,
        Some(100),
        Some(100),
    );
    let unpriced = accounting_usage(
        rhei_tui::UsageCoverage::Unpriced,
        rhei_tui::PricingStatus::Unpriced,
        None,
        None,
    );

    let summary = rhei_tui::summarize_usage_summaries([&priced, &unpriced]).expect("summary");

    assert_eq!(summary.coverage, rhei_tui::UsageCoverage::Partial);
    assert_eq!(summary.pricing_status, rhei_tui::PricingStatus::PartialPrice);
    assert_eq!(summary.cost_micro, None);
    assert_eq!(summary.priced_cost_micro, Some(100));
}

#[test]
fn accounting_capture_env_is_declared_before_spawn() {
    let path = std::path::PathBuf::from("/tmp/rhei-usage.jsonl");
    let mut command = std::process::Command::new("agent");

    configure_accounting_capture(&mut command, Some(&path));

    let env: std::collections::BTreeMap<String, String> = command
        .get_envs()
        .filter_map(|(key, value)| {
            value.map(|value| {
                (
                    key.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
        })
        .collect();
    assert_eq!(
        env.get("RHEI_ACCOUNTING_USAGE_PATH").map(String::as_str),
        Some("/tmp/rhei-usage.jsonl")
    );
    assert_eq!(
        env.get("RHEI_ACCOUNTING_USAGE_SCHEMA").map(String::as_str),
        Some(ACCOUNTING_USAGE_EVENT_SCHEMA)
    );
}

#[test]
fn accounting_extractor_ignores_arbitrary_json_without_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage.jsonl");
    std::fs::write(
        &path,
        r#"{"metrics":{"input_tokens":123,"output_tokens":456}}"#,
    )
    .expect("write capture");

    match extract_usage_from_capture(Some(&path)) {
        ExtractedUsageStatus::NoUsageEmitted => {}
        _ => panic!("arbitrary JSON must not be treated as usage"),
    }
}

#[test]
fn accounting_extractor_accepts_structured_usage_event() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage.jsonl");
    std::fs::write(
        &path,
        format!(
            r#"{{"schema":"{}","usage":{{"input_tokens":123,"output_tokens":456}}}}"#,
            ACCOUNTING_USAGE_EVENT_SCHEMA
        ),
    )
    .expect("write capture");

    match extract_usage_from_capture(Some(&path)) {
        ExtractedUsageStatus::Measured(usage) => {
            assert_eq!(usage.input_total, Some(123));
            assert_eq!(usage.output_total, Some(456));
        }
        _ => panic!("structured usage event should be measured"),
    }
}

fn claude_result_line(usage: &str) -> String {
    format!(r#"{{"type":"result","subtype":"success","is_error":false,"usage":{usage}}}"#)
}

fn claude_capture(path: &std::path::Path) -> AgentUsageCapture {
    AgentUsageCapture {
        extractor: AgentUsageExtractor::ClaudeStreamJson,
        path: path.to_path_buf(),
        invocation_id: "1::work::claude-code::visit-1".to_string(),
        task_id: "1".to_string(),
        state: "work".to_string(),
        agent: "claude-code".to_string(),
        provider: Some("anthropic".to_string()),
        model: Some("claude-opus-5".to_string()),
        slot: 0,
    }
}

#[test]
fn accounting_claude_stream_json_result_maps_every_token_dimension() {
    let usage = extract_claude_json_usage(&claude_result_line(
        r#"{"input_tokens":4,"cache_creation_input_tokens":7379,
            "cache_read_input_tokens":23148,"output_tokens":146}"#,
    ))
    .expect("claude result usage");

    assert_eq!(usage.input_total, Some(4));
    assert_eq!(usage.input_cached_read, Some(23148));
    assert_eq!(usage.input_cache_write, Some(7379));
    assert_eq!(usage.output_total, Some(146));

    let tokens = tokens_from_usage(usage);
    assert_eq!(tokens.input.total.value, Some(4));
    assert_eq!(tokens.input.cached_read.value, Some(23148));
    assert_eq!(tokens.input.cache_write.value, Some(7379));
    assert_eq!(tokens.output.total.value, Some(146));
    assert_eq!(tokens.total.value, Some(150));
}

#[test]
fn accounting_claude_stream_json_ignores_repeated_assistant_usage() {
    // Claude Code repeats per-message usage on `assistant` events and only the
    // terminal `result` event is cumulative; counting both would double count.
    let assistant = r#"{"type":"assistant","message":{"role":"assistant","content":[
        {"type":"text","text":"ok"}],
        "usage":{"input_tokens":2,"cache_creation_input_tokens":7154,
                 "cache_read_input_tokens":7997,"output_tokens":20}}}"#;
    assert!(extract_claude_json_usage(assistant).is_none());

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage.jsonl");
    let capture = claude_capture(&path);
    let sink: std::sync::Arc<dyn rhei_tui::EventSink> =
        std::sync::Arc::new(rhei_tui::NullSink);
    for line in [
        assistant,
        assistant,
        &claude_result_line(
            r#"{"input_tokens":4,"cache_creation_input_tokens":7379,
                "cache_read_input_tokens":23148,"output_tokens":146}"#,
        ),
    ] {
        capture_agent_output_usage(
            Some(&capture),
            rhei_tui::AgentStream::Stdout,
            line,
            &sink,
        );
    }

    match extract_usage_from_capture(Some(&path)) {
        ExtractedUsageStatus::Measured(usage) => {
            assert_eq!(usage.input_total, Some(4));
            assert_eq!(usage.input_cached_read, Some(23148));
            assert_eq!(usage.input_cache_write, Some(7379));
            assert_eq!(usage.output_total, Some(146));
        }
        _ => panic!("claude result usage should be measured exactly once"),
    }
}

#[test]
fn accounting_claude_stream_json_result_without_usage_fails_extraction() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage.jsonl");
    let capture = claude_capture(&path);
    let sink: std::sync::Arc<dyn rhei_tui::EventSink> =
        std::sync::Arc::new(rhei_tui::NullSink);

    capture_agent_output_usage(
        Some(&capture),
        rhei_tui::AgentStream::Stdout,
        r#"{"type":"result","subtype":"success","is_error":false,"tokens":"7"}"#,
        &sink,
    );

    match extract_usage_from_capture(Some(&path)) {
        ExtractedUsageStatus::ExtractorFailed => {}
        _ => panic!("a result event without usage is a format change"),
    }
    let captured = std::fs::read_to_string(&path).expect("capture file");
    assert!(captured.contains("carries no 'usage' object"), "{captured}");
}

#[test]
fn accounting_claude_stream_json_leaves_unrelated_lines_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage.jsonl");
    let capture = claude_capture(&path);
    let sink: std::sync::Arc<dyn rhei_tui::EventSink> =
        std::sync::Arc::new(rhei_tui::NullSink);

    capture_agent_output_usage(
        Some(&capture),
        rhei_tui::AgentStream::Stdout,
        "npm warn: not json at all",
        &sink,
    );

    assert!(!path.exists(), "non-JSON stdout must not create a capture file");
    assert!(matches!(
        display_output_line(AgentUsageExtractor::ClaudeStreamJson, "npm warn: not json at all"),
        AgentOutputLine::Passthrough
    ));
}

#[test]
fn accounting_claude_stream_json_renders_readable_log_lines() {
    match display_output_line(
        AgentUsageExtractor::ClaudeStreamJson,
        &claude_result_line(
            r#"{"input_tokens":4,"cache_creation_input_tokens":7379,
                "cache_read_input_tokens":23148,"output_tokens":146}"#,
        ),
    ) {
        AgentOutputLine::Replace(line) => assert_eq!(
            line,
            "claude run completed: total=150 input=4 cached_input=23148 \
             cache_write_input=7379 output=146"
        ),
        _ => panic!("result event should render a usage summary"),
    }
}

#[test]
fn accounting_args_request_structured_output_per_agent() {
    fn args(agent: &str, stream_json_already_configured: bool) -> Vec<String> {
        let resolved = ResolvedAgent {
            agent: AgentConfig::from(agent),
            profile: built_in_agents().remove(agent).expect("built-in agent"),
            mode: None,
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: None,
            autonomous_args: Vec::new(),
        };
        let mut command = std::process::Command::new("agent");
        configure_agent_accounting_args(&mut command, &resolved, stream_json_already_configured);
        command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    assert_eq!(args("codex", false), vec!["--json"]);
    assert_eq!(args("pi", false), vec!["--mode", "json"]);
    assert_eq!(
        args("claude-code", false),
        vec!["--output-format", "stream-json", "--verbose"]
    );
    assert!(
        args("claude-code", true).is_empty(),
        "stream-json flags must not be repeated for intervention spawns"
    );
}
