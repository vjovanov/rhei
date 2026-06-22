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
