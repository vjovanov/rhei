fn custom_luna_price_book() -> PriceBook {
    PriceBook {
        schema: ACCOUNTING_PRICES_SCHEMA.to_string(),
        price_book_id: "fixture-luna-2026-09-01".to_string(),
        currency: "CHF".to_string(),
        entries: vec![PriceBookEntry {
            provider: "openai".to_string(),
            model: "gpt-5.6-luna".to_string(),
            effective_at: "2026-09-01T00:00:00Z".to_string(),
            unit: "1m_tokens".to_string(),
            input_total_micro: 2_000_000,
            input_cached_read_micro: 250_000,
            input_cache_write_micro: 4_000_000,
            output_total_micro: 10_000_000,
        }],
    }
}

#[test]
fn custom_price_book_prices_luna_usage_and_complete_rollup() {
    // §FS-rhei-cost-accounting.5.1: the selected book's exact match, id,
    // currency, integer rates, and durable semantics govern the invocation.
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("luna-prices.json");
    let expected_book = custom_luna_price_book();
    std::fs::write(
        &source,
        serde_json::to_string_pretty(&expected_book).expect("serialize fixture book"),
    )
    .expect("write fixture book");
    let price_book = load_price_book(&source).expect("load fixture book");
    assert_eq!(price_book, expected_book);

    let tokens = tokens_from_usage(ExtractedUsage {
        input_total: Some(1_250_000),
        input_cached_read: Some(500_000),
        input_cache_write: Some(250_000),
        output_total: Some(750_000),
        ..ExtractedUsage::default()
    });
    let pricing = price_tokens(
        &price_book,
        Some("openai"),
        Some("gpt-5.6-luna"),
        &tokens,
    );
    assert_eq!(pricing.status, "priced");
    assert_eq!(pricing.currency.as_deref(), Some("CHF"));
    assert_eq!(pricing.price_book_id.as_deref(), Some("fixture-luna-2026-09-01"));
    // The full input rate applies to what is left after the cache dimensions
    // are taken out of `input.total`. §FS-rhei-cost-accounting.5
    assert_eq!(pricing.amount_micro, Some(9_625_000));
    assert_eq!(pricing.priced_amount_micro, Some(9_625_000));

    let accounting_root = dir.path().join("runtime/accounting");
    write_price_book(&accounting_root, &price_book).expect("copy selected price book");
    let mut record = accounting_test_record();
    record.tokens = tokens;
    record.pricing = pricing;
    record.model = Some("gpt-5.6-luna".to_string());
    write_invocation_record(&accounting_root, &record).expect("write invocation");
    let plan = rhei_core::parse(
        "# Rhei: Custom Pricing\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n",
    )
    .expect("parse plan");
    // The workspace lifetime total, which is what this rollup has always been;
    // the run's own share travels beside it. §FS-rhei-cost-accounting.6
    let summary = regenerate_accounting_indexes(dir.path(), &plan)
        .expect("regenerate rollups")
        .expect("run summary")
        .workspace
        .expect("workspace rollup");
    assert_eq!(summary.cost_micro, Some(9_625_000));
    assert_eq!(summary.priced_cost_micro, Some(9_625_000));
    assert_eq!(summary.currency.as_deref(), Some("CHF"));
    assert_eq!(summary.coverage, rhei_tui::UsageCoverage::Complete);
    assert_eq!(summary.pricing_status, rhei_tui::PricingStatus::Priced);

    let copied: PriceBook = serde_json::from_str(
        &std::fs::read_to_string(accounting_root.join("prices.json"))
            .expect("read copied price book"),
    )
    .expect("parse copied price book");
    assert_eq!(copied, expected_book);
}

#[test]
fn custom_price_book_keeps_nonmatching_model_unpriced() {
    // §FS-rhei-cost-accounting.5.1: missing exact matches neither fall back
    // to the built-in book nor become zero-cost.
    let price_book = custom_luna_price_book();
    let tokens = tokens_from_usage(ExtractedUsage {
        input_total: Some(1_000_000),
        output_total: Some(1_000_000),
        ..ExtractedUsage::default()
    });

    let pricing = price_tokens(&price_book, Some("openai"), Some("gpt-other"), &tokens);

    assert_eq!(pricing.status, "unpriced");
    assert_eq!(pricing.currency.as_deref(), Some("CHF"));
    assert_eq!(pricing.price_book_id.as_deref(), Some("fixture-luna-2026-09-01"));
    assert_eq!(pricing.amount_micro, None);
    assert_eq!(pricing.priced_amount_micro, None);
}

#[test]
fn custom_price_book_rejects_wrong_schema_with_its_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("unsupported.json");
    let mut book = custom_luna_price_book();
    book.schema = "rhei.accounting.prices.v2".to_string();
    std::fs::write(&path, serde_json::to_string(&book).expect("serialize book"))
        .expect("write book");

    let error = load_price_book(&path).expect_err("wrong schema must fail").to_string();

    assert!(error.contains(path.to_string_lossy().as_ref()), "got: {error}");
    assert!(error.contains("unsupported schema"), "got: {error}");
}

#[test]
fn missing_custom_price_book_error_names_its_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("missing-prices.json");

    let error = load_price_book(&path).expect_err("missing book must fail").to_string();

    assert!(error.contains(path.to_string_lossy().as_ref()), "got: {error}");
    assert!(error.contains("failed to read price book"), "got: {error}");
}

#[test]
fn selected_book_does_not_reprice_an_old_unpriced_invocation() {
    // §FS-rhei-cost-accounting.5.1: `rhei cost` derives from each durable
    // invocation result; a later prices.json copy does not mutate old pricing.
    let dir = tempfile::tempdir().expect("tempdir");
    let accounting_root = dir.path().join("runtime/accounting");
    let mut old = accounting_test_record();
    old.model = Some("gpt-5.6-luna".to_string());
    old.tokens = tokens_from_usage(ExtractedUsage {
        input_total: Some(1_000_000),
        output_total: Some(1_000_000),
        ..ExtractedUsage::default()
    });
    write_invocation_record(&accounting_root, &old).expect("write old invocation");
    write_price_book(&accounting_root, &custom_luna_price_book())
        .expect("copy later matching book");

    let inspection = read_cost_inspection(&accounting_root);
    let summary = inspection.summary.expect("old invocation summary");

    assert_eq!(summary.pricing_status, rhei_tui::PricingStatus::Unpriced);
    assert_eq!(summary.coverage, rhei_tui::UsageCoverage::Unpriced);
    assert_eq!(summary.cost_micro, None);
    assert_eq!(summary.priced_cost_micro, None);
}

#[test]
fn built_in_book_rejects_an_old_unpriced_record_in_another_currency() {
    // §FS-rhei-cost-accounting.5.1: even an unpriced record supplies the
    // durable currency that a later scalar rollup would otherwise mislabel.
    let dir = tempfile::tempdir().expect("tempdir");
    let accounting_root = dir.path().join("runtime/accounting");
    let mut old = accounting_test_record();
    old.pricing = AccountingPricing {
        status: "unpriced".to_string(),
        currency: Some("CHF".to_string()),
        amount_micro: None,
        priced_amount_micro: None,
        price_book_id: Some("old-chf-book".to_string()),
    };
    write_invocation_record(&accounting_root, &old).expect("write old invocation");

    let error = validate_price_book_currency(&accounting_root, &builtin_price_book())
        .expect_err("USD selection must reject the CHF record")
        .to_string();

    assert!(error.contains(accounting_root.to_string_lossy().as_ref()), "got: {error}");
    assert!(error.contains("USD"), "got: {error}");
    assert!(error.contains("CHF"), "got: {error}");
}
