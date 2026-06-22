const ACCOUNTING_INVOCATION_SCHEMA: &str = "rhei.accounting.invocation.v1";
const ACCOUNTING_PRICES_SCHEMA: &str = "rhei.accounting.prices.v1";
const ACCOUNTING_USAGE_EVENT_SCHEMA: &str = "rhei.accounting.usage.v1";
const PRICE_BOOK_ID: &str = "builtin-2026-05-20";
const PRICE_UNIT_TOKENS: u64 = 1_000_000;
static ACCOUNTING_INVOCATION_FILE_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingInvocationRecord {
    schema: String,
    invocation_id: String,
    task_id: String,
    state: String,
    visit: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_slug: Option<String>,
    agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    started_at: String,
    ended_at: String,
    extraction_status: String,
    scope: String,
    tokens: AccountingTokens,
    pricing: AccountingPricing,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingTokens {
    #[serde(default = "unknown_token_dimension")]
    total: AccountingTokenDimension,
    input: AccountingTokenSide,
    output: AccountingTokenSide,
}

impl Default for AccountingTokens {
    fn default() -> Self {
        Self {
            total: AccountingTokenDimension::unavailable("unknown"),
            input: AccountingTokenSide::default(),
            output: AccountingTokenSide::default(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingTokenSide {
    total: AccountingTokenDimension,
    cached_read: AccountingTokenDimension,
    cache_write: AccountingTokenDimension,
}

impl Default for AccountingTokenSide {
    fn default() -> Self {
        Self {
            total: AccountingTokenDimension::unavailable("unknown"),
            cached_read: AccountingTokenDimension::unavailable("unsupported"),
            cache_write: AccountingTokenDimension::unavailable("unsupported"),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingTokenDimension {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

impl AccountingTokenDimension {
    fn measured(value: u64) -> Self {
        Self::measured_from(value, "agent-usage-capture")
    }

    fn measured_from(value: u64, source: &str) -> Self {
        Self {
            value: Some(value),
            source: Some(source.to_string()),
            status: None,
        }
    }

    fn unavailable(status: &str) -> Self {
        Self { value: None, source: None, status: Some(status.to_string()) }
    }
}

fn unknown_token_dimension() -> AccountingTokenDimension {
    AccountingTokenDimension::unavailable("unknown")
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingPricing {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount_micro: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priced_amount_micro: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price_book_id: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
struct PriceBook {
    schema: &'static str,
    price_book_id: &'static str,
    currency: &'static str,
    entries: Vec<PriceBookEntry>,
}

#[derive(Clone, Debug, serde::Serialize)]
struct PriceBookEntry {
    provider: &'static str,
    model: &'static str,
    effective_at: &'static str,
    unit: &'static str,
    input_total_micro: u64,
    input_cached_read_micro: u64,
    input_cache_write_micro: u64,
    output_total_micro: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct ExtractedUsage {
    total: Option<u64>,
    total_source: Option<&'static str>,
    input_total: Option<u64>,
    input_cached_read: Option<u64>,
    input_cache_write: Option<u64>,
    output_total: Option<u64>,
    output_cached_read: Option<u64>,
    output_cache_write: Option<u64>,
}

impl ExtractedUsage {
    fn merge(&mut self, other: ExtractedUsage) {
        merge_usage_value(&mut self.total, other.total);
        merge_usage_value(&mut self.input_total, other.input_total);
        merge_usage_value(&mut self.input_cached_read, other.input_cached_read);
        merge_usage_value(&mut self.input_cache_write, other.input_cache_write);
        merge_usage_value(&mut self.output_total, other.output_total);
        merge_usage_value(&mut self.output_cached_read, other.output_cached_read);
        merge_usage_value(&mut self.output_cache_write, other.output_cache_write);
    }

    fn has_total(&self) -> bool {
        self.total.is_some() || self.input_total.is_some() || self.output_total.is_some()
    }
}

fn merge_usage_value(target: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *target = Some(target.unwrap_or(0).saturating_add(value));
    }
}

enum ExtractedUsageStatus {
    Measured(ExtractedUsage),
    NoUsageEmitted,
    ExtractorUnavailable,
    ExtractorFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentUsageExtractor {
    StructuredCapture,
    CodexJson,
}

#[derive(Clone, Debug)]
struct AgentUsageCapture {
    extractor: AgentUsageExtractor,
    path: PathBuf,
    invocation_id: String,
    task_id: String,
    state: String,
    agent: String,
    provider: Option<String>,
    model: Option<String>,
    slot: rhei_tui::Slot,
}

#[derive(Clone, Debug)]
struct CostInspection {
    summary: Option<rhei_tui::AccountingRunSummary>,
    invocations: Vec<(PathBuf, AccountingInvocationRecord)>,
    errors: Vec<String>,
}

struct AgentAccountingInvocation<'a> {
    workspace_root: &'a Path,
    task: &'a rhei_core::ast::Task,
    state: &'a str,
    resolved: &'a ResolvedAgent,
    visit: u64,
    started_at: std::time::SystemTime,
    ended_at: std::time::SystemTime,
    slot: Option<rhei_tui::Slot>,
    usage_capture_path: Option<&'a Path>,
    log_path: Option<&'a Path>,
    sink: &'a Arc<dyn rhei_tui::EventSink>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CostGroup {
    Agent,
    Model,
    State,
    Node,
}

fn record_agent_accounting_invocation(
    invocation: AgentAccountingInvocation<'_>,
) -> MietteResult<Option<rhei_tui::UsageSummary>> {
    // §FS-rhei-cost-accounting.3.2: Built-ins must not silently omit records.
    if !agent_has_accounting_extractor(invocation.resolved.agent.id()) {
        return Ok(None);
    }

    // §FS-rhei-cost-accounting.2: Accounting files live under runtime/accounting/.
    let accounting_root = invocation.workspace_root.join("runtime/accounting");
    write_price_book(&accounting_root)?;

    // §FS-rhei-cost-accounting.11: Extraction failures affect coverage only.
    let (tokens, extraction_status) =
        match extract_usage(invocation.usage_capture_path, invocation.log_path) {
            ExtractedUsageStatus::Measured(usage) => (tokens_from_usage(usage), "measured"),
            ExtractedUsageStatus::NoUsageEmitted => {
                (AccountingTokens::default(), "no-usage-emitted")
            }
            ExtractedUsageStatus::ExtractorUnavailable => {
                (AccountingTokens::default(), "extractor-unavailable")
            }
            ExtractedUsageStatus::ExtractorFailed => {
                (AccountingTokens::default(), "extractor-failed")
            }
        };
    let provider = invocation.resolved.model_provider.clone();
    let model =
        invocation.resolved.model_name.clone().or_else(|| invocation.resolved.model.clone());
    let pricing = price_tokens(provider.as_deref(), model.as_deref(), &tokens);
    let target_slug = resolved_agent_target_slug(invocation.resolved);
    let invocation_id = accounting_invocation_id(
        &invocation.task.id.to_string(),
        invocation.state,
        invocation.resolved,
        invocation.visit,
    );
    let record = AccountingInvocationRecord {
        schema: ACCOUNTING_INVOCATION_SCHEMA.to_string(),
        invocation_id: invocation_id.clone(),
        task_id: invocation.task.id.to_string(),
        state: invocation.state.to_string(),
        visit: invocation.visit,
        target_slug,
        agent: invocation.resolved.agent.id().to_string(),
        provider,
        model,
        started_at: format_iso8601_utc(invocation.started_at),
        ended_at: format_iso8601_utc(invocation.ended_at),
        extraction_status: extraction_status.to_string(),
        scope: "aggregate-agent-process".to_string(),
        tokens,
        pricing,
    };

    write_invocation_record(&accounting_root, &record)?;
    let usage = usage_summary_from_record(&record);
    // §FS-rhei-cost-accounting.7: Emit UsageReported after durable write.
    invocation.sink.emit(rhei_tui::RunEvent::UsageReported {
        slot: invocation.slot,
        task: invocation.task.id.to_string(),
        invocation_id,
        usage: usage.clone(),
    });
    Ok(Some(usage))
}

fn regenerate_accounting_indexes(
    workspace_root: &Path,
    rhei: &rhei_core::ast::Rhei,
) -> MietteResult<Option<rhei_tui::AccountingRunSummary>> {
    // §FS-rhei-cost-accounting.6: Task and run rollups are derived indexes.
    let accounting_root = workspace_root.join("runtime/accounting");
    let inspection = read_cost_inspection(&accounting_root);
    if inspection.invocations.is_empty() {
        return Ok(None);
    }
    let task_dir = accounting_root.join("tasks");
    fs::create_dir_all(&task_dir).map_err(|err| {
        file_io_report(&task_dir, "failed to create accounting task index directory", err)
    })?;

    let tasks = flatten_tasks(rhei);
    for task in tasks {
        let task_id = task.id.to_string();
        let direct = summarize_records(
            inspection
                .invocations
                .iter()
                .filter(|(_, record)| record.task_id == task_id)
                .map(|(_, record)| record),
        );
        let subtree = summarize_records(
            inspection
                .invocations
                .iter()
                .filter(|(_, record)| record.task_id == task_id || is_descendant_id(&record.task_id, &task_id))
                .map(|(_, record)| record),
        );
        if direct.is_some() || subtree.is_some() {
            let payload = serde_json::json!({
                "schema": "rhei.accounting.task.v1",
                "task_id": task_id,
                "direct": direct,
                "subtree": subtree,
            });
            let path = task_dir.join(format!("{}.json", safe_accounting_file_segment(&task.id.to_string())));
            write_json_atomic(&path, &payload)?;
        }
    }

    if let Some(summary) = inspection.summary.as_ref() {
        let payload = serde_json::json!({
            "schema": "rhei.accounting.summary.v1",
            "summary": summary,
        });
        write_json_atomic(&accounting_root.join("summary.json"), &payload)?;
    }
    Ok(inspection.summary)
}

fn cost_command(
    input: &Path,
    task: Option<&str>,
    json: bool,
    by: CostGroup,
) -> MietteResult<()> {
    // §FS-rhei-cost-accounting.8: `rhei cost` inspects without changing plan.
    let input_buf = normalize_workspace_input(input);
    let loaded = load_plan(&input_buf)?;
    let workspace_root = execution_workspace_root(&input_buf);
    let accounting_root = workspace_root.join("runtime/accounting");
    let inspection = read_cost_inspection(&accounting_root);

    if json {
        let payload = cost_json_payload(&loaded.rhei, &inspection, task, by);
        println!("{}", serde_json::to_string_pretty(&payload).expect("cost json serializes"));
        return Ok(());
    }

    for error in &inspection.errors {
        eprintln!("warning: {error}");
    }
    if inspection.invocations.is_empty() {
        // §FS-rhei-cost-accounting.8: Empty accounting exits 0 with this text.
        println!("(no accounting records found)");
        return Ok(());
    }

    if let Some(task_id) = task {
        print_task_cost(&loaded.rhei, &inspection, task_id);
    } else {
        print_run_cost(&loaded.rhei, &inspection, by);
    }
    Ok(())
}

fn read_cost_inspection(accounting_root: &Path) -> CostInspection {
    let mut invocations = Vec::new();
    let mut errors = Vec::new();
    let dir = accounting_root.join("invocations");
    // §FS-rhei-cost-accounting.2: Invocation records are authoritative.
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return CostInspection { summary: None, invocations, errors };
        }
        Err(err) => {
            errors.push(format!("{}: {err}", dir.display()));
            return CostInspection { summary: None, invocations, errors };
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) != Some("json") {
            continue;
        }
        match fs::read_to_string(&path)
            .map_err(|err| err.to_string())
            .and_then(|text| serde_json::from_str::<AccountingInvocationRecord>(&text).map_err(|err| err.to_string()))
        {
            Ok(record) => invocations.push((path, record)),
            Err(err) => errors.push(format!("{}: {err}", path.display())),
        }
    }
    invocations.sort_by(|(_, a), (_, b)| a.started_at.cmp(&b.started_at).then_with(|| a.invocation_id.cmp(&b.invocation_id)));
    let summary = summarize_records(invocations.iter().map(|(_, record)| record));
    CostInspection { summary, invocations, errors }
}

fn cost_json_payload(
    rhei: &rhei_core::ast::Rhei,
    inspection: &CostInspection,
    task: Option<&str>,
    by: CostGroup,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "rhei.accounting.cost.v1",
        "summary": inspection.summary,
        "task": task.map(|task_id| task_cost_json(rhei, inspection, task_id)),
        "groups": grouped_cost_json(inspection, by),
        "errors": inspection.errors,
    })
}

fn task_cost_json(
    rhei: &rhei_core::ast::Rhei,
    inspection: &CostInspection,
    task_id: &str,
) -> serde_json::Value {
    let title = flatten_tasks(rhei)
        .into_iter()
        .find(|task| task.id.to_string() == task_id)
        .map(|task| task.title.clone());
    serde_json::json!({
        // §FS-rhei-cost-accounting.8: JSON uses stable runtime schema names.
        "task_id": task_id,
        "title": title,
        "direct": summarize_records(inspection.invocations.iter().filter(|(_, record)| record.task_id == task_id).map(|(_, record)| record)),
        "subtree": summarize_records(inspection.invocations.iter().filter(|(_, record)| record.task_id == task_id || is_descendant_id(&record.task_id, task_id)).map(|(_, record)| record)),
        "invocations": inspection.invocations.iter().filter(|(_, record)| record.task_id == task_id || is_descendant_id(&record.task_id, task_id)).map(|(_, record)| record).collect::<Vec<_>>(),
    })
}

fn grouped_cost_json(inspection: &CostInspection, by: CostGroup) -> Vec<serde_json::Value> {
    grouped_records(inspection, by)
        .into_iter()
        .map(|(key, records)| {
            serde_json::json!({
                "key": key,
                "summary": summarize_records(records.into_iter()),
            })
        })
        .collect()
}

fn print_run_cost(rhei: &rhei_core::ast::Rhei, inspection: &CostInspection, by: CostGroup) {
    if let Some(summary) = inspection.summary.as_ref() {
        println!(
            "Cost {} | Total {} | In {} | Out {} | Coverage {:?} | Invocations {}",
            format_summary_cost(summary),
            format_dimension_value(&summary.total),
            format_dimension_value(&summary.input_total),
            format_dimension_value(&summary.output_total),
            summary.coverage,
            summary.invocation_count
        );
    }
    println!("\nBy {:?}:", by);
    for (key, records) in grouped_records(inspection, by) {
        if let Some(summary) = summarize_records(records.into_iter()) {
            println!(
                "  {key}: {} total={} in={} out={} coverage={:?}",
                format_summary_cost(&summary),
                format_dimension_value(&summary.total),
                format_dimension_value(&summary.input_total),
                format_dimension_value(&summary.output_total),
                summary.coverage
            );
        }
    }
    println!("\nHighest subtree nodes:");
    for (task_id, title, summary) in highest_subtree_nodes(rhei, inspection).into_iter().take(8) {
        println!("  {task_id} {title}: {}", format_summary_cost(&summary));
    }
}

fn print_task_cost(rhei: &rhei_core::ast::Rhei, inspection: &CostInspection, task_id: &str) {
    let title = flatten_tasks(rhei)
        .into_iter()
        .find(|task| task.id.to_string() == task_id)
        .map(|task| task.title.clone())
        .unwrap_or_else(|| "(unknown task)".to_string());
    println!("Task {task_id}: {title}");
    let direct = summarize_records(
        inspection
            .invocations
            .iter()
            .filter(|(_, record)| record.task_id == task_id)
            .map(|(_, record)| record),
    );
    let subtree = summarize_records(
        inspection
            .invocations
            .iter()
            .filter(|(_, record)| record.task_id == task_id || is_descendant_id(&record.task_id, task_id))
            .map(|(_, record)| record),
    );
    println!("  Direct: {}", direct.as_ref().map(format_summary_cost).unwrap_or_else(|| "none".to_string()));
    println!("  Subtree: {}", subtree.as_ref().map(format_summary_cost).unwrap_or_else(|| "none".to_string()));
    println!("  Invocations:");
    for (_, record) in inspection
        .invocations
        .iter()
        .filter(|(_, record)| record.task_id == task_id || is_descendant_id(&record.task_id, task_id))
    {
        let usage = usage_summary_from_record(record);
        println!(
            "    {} {} {} {}",
            record.invocation_id,
            record.agent,
            record.model.as_deref().unwrap_or("-"),
            format_usage_cost(&usage)
        );
    }
}

fn grouped_records(
    inspection: &CostInspection,
    by: CostGroup,
) -> Vec<(String, Vec<&AccountingInvocationRecord>)> {
    let mut groups: BTreeMap<String, Vec<&AccountingInvocationRecord>> = BTreeMap::new();
    for (_, record) in &inspection.invocations {
        let key = match by {
            CostGroup::Agent => record.agent.clone(),
            CostGroup::Model => record.model.clone().unwrap_or_else(|| "(unknown)".to_string()),
            CostGroup::State => record.state.clone(),
            CostGroup::Node => record.task_id.clone(),
        };
        groups.entry(key).or_default().push(record);
    }
    groups.into_iter().collect()
}

fn highest_subtree_nodes(
    rhei: &rhei_core::ast::Rhei,
    inspection: &CostInspection,
) -> Vec<(String, String, rhei_tui::AccountingRunSummary)> {
    let mut rows = Vec::new();
    for task in flatten_tasks(rhei) {
        let task_id = task.id.to_string();
        // §FS-rhei-cost-accounting.6: subtree(node)=direct+descendants.
        if let Some(summary) = summarize_records(
            inspection
                .invocations
                .iter()
                .filter(|(_, record)| record.task_id == task_id || is_descendant_id(&record.task_id, &task_id))
                .map(|(_, record)| record),
        ) {
            rows.push((task_id, task.title.clone(), summary));
        }
    }
    rows.sort_by(|a, b| summary_sort_cost(&b.2).cmp(&summary_sort_cost(&a.2)));
    rows
}

fn summarize_records<'a>(
    records: impl IntoIterator<Item = &'a AccountingInvocationRecord>,
) -> Option<rhei_tui::AccountingRunSummary> {
    // §FS-rhei-cost-accounting.6: Rollups summarize invocation records.
    let usages: Vec<rhei_tui::UsageSummary> = records.into_iter().map(usage_summary_from_record).collect();
    rhei_tui::summarize_usage_summaries(usages.iter())
}

fn usage_summary_from_record(record: &AccountingInvocationRecord) -> rhei_tui::UsageSummary {
    // §FS-rhei-cost-accounting.7: UsageSummary mirrors invocation data.
    let status = match record.extraction_status.as_str() {
        "measured" => rhei_tui::UsageStatus::Measured,
        "unsupported-agent" => rhei_tui::UsageStatus::UnsupportedAgent,
        "extractor-unavailable" => rhei_tui::UsageStatus::ExtractorUnavailable,
        "extractor-failed" => rhei_tui::UsageStatus::ExtractorFailed,
        _ => rhei_tui::UsageStatus::NoUsageEmitted,
    };
    let pricing_status = match record.pricing.status.as_str() {
        "priced" => rhei_tui::PricingStatus::Priced,
        "partial-price" => rhei_tui::PricingStatus::PartialPrice,
        "unpriced" => rhei_tui::PricingStatus::Unpriced,
        _ => rhei_tui::PricingStatus::NotApplicable,
    };
    let coverage = usage_coverage(status, pricing_status);
    rhei_tui::UsageSummary {
        invocation_id: record.invocation_id.clone(),
        state: record.state.clone(),
        agent: record.agent.clone(),
        provider: record.provider.clone(),
        model: record.model.clone(),
        total: dimension_summary(&record.tokens.total),
        input_total: dimension_summary(&record.tokens.input.total),
        input_cached_read: dimension_summary(&record.tokens.input.cached_read),
        input_cache_write: dimension_summary(&record.tokens.input.cache_write),
        output_total: dimension_summary(&record.tokens.output.total),
        output_cached_read: dimension_summary(&record.tokens.output.cached_read),
        output_cache_write: dimension_summary(&record.tokens.output.cache_write),
        cost_micro: record.pricing.amount_micro,
        priced_cost_micro: record.pricing.priced_amount_micro.or(record.pricing.amount_micro),
        currency: record.pricing.currency.clone(),
        coverage,
        status,
        pricing_status,
    }
}

fn usage_summary_from_extracted_usage(
    invocation_id: &str,
    state: &str,
    agent: &str,
    provider: Option<String>,
    model: Option<String>,
    usage: ExtractedUsage,
) -> rhei_tui::UsageSummary {
    let tokens = tokens_from_usage(usage);
    let pricing = price_tokens(provider.as_deref(), model.as_deref(), &tokens);
    let pricing_status = match pricing.status.as_str() {
        "priced" => rhei_tui::PricingStatus::Priced,
        "partial-price" => rhei_tui::PricingStatus::PartialPrice,
        "unpriced" => rhei_tui::PricingStatus::Unpriced,
        _ => rhei_tui::PricingStatus::NotApplicable,
    };
    let status = rhei_tui::UsageStatus::Measured;
    rhei_tui::UsageSummary {
        invocation_id: invocation_id.to_string(),
        state: state.to_string(),
        agent: agent.to_string(),
        provider,
        model,
        total: dimension_summary(&tokens.total),
        input_total: dimension_summary(&tokens.input.total),
        input_cached_read: dimension_summary(&tokens.input.cached_read),
        input_cache_write: dimension_summary(&tokens.input.cache_write),
        output_total: dimension_summary(&tokens.output.total),
        output_cached_read: dimension_summary(&tokens.output.cached_read),
        output_cache_write: dimension_summary(&tokens.output.cache_write),
        cost_micro: pricing.amount_micro,
        priced_cost_micro: pricing.priced_amount_micro.or(pricing.amount_micro),
        currency: pricing.currency,
        coverage: usage_coverage(status, pricing_status),
        status,
        pricing_status,
    }
}

fn usage_coverage(
    status: rhei_tui::UsageStatus,
    pricing_status: rhei_tui::PricingStatus,
) -> rhei_tui::UsageCoverage {
    if status != rhei_tui::UsageStatus::Measured {
        return rhei_tui::UsageCoverage::None;
    }
    match pricing_status {
        rhei_tui::PricingStatus::Priced => rhei_tui::UsageCoverage::Complete,
        rhei_tui::PricingStatus::PartialPrice => rhei_tui::UsageCoverage::Partial,
        rhei_tui::PricingStatus::Unpriced => rhei_tui::UsageCoverage::Unpriced,
        rhei_tui::PricingStatus::NotApplicable => rhei_tui::UsageCoverage::None,
    }
}

fn dimension_summary(dimension: &AccountingTokenDimension) -> rhei_tui::DimensionSummary {
    // §FS-rhei-cost-accounting.3.1: Dimension status distinguishes absence.
    if let Some(value) = dimension.value {
        return rhei_tui::DimensionSummary {
            value: Some(value),
            status: rhei_tui::DimensionStatus::Measured,
            measured_count: 1,
            missing_count: 0,
        };
    }
    let status = match dimension.status.as_deref() {
        Some("unsupported") => rhei_tui::DimensionStatus::Unsupported,
        Some("omitted") => rhei_tui::DimensionStatus::Omitted,
        _ => rhei_tui::DimensionStatus::Unknown,
    };
    rhei_tui::DimensionSummary { value: None, status, measured_count: 0, missing_count: 1 }
}

fn agent_has_accounting_extractor(agent: &str) -> bool {
    // §FS-rhei-cost-accounting.4: v1 supports claude-code, codex, and pi.
    agent_usage_extractor(agent).is_some()
}

fn agent_usage_extractor(agent: &str) -> Option<AgentUsageExtractor> {
    match agent {
        "codex" => Some(AgentUsageExtractor::CodexJson),
        "claude-code" | "pi" => Some(AgentUsageExtractor::StructuredCapture),
        _ => None,
    }
}

fn accounting_capture_path_for_spawn(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    resolved: &ResolvedAgent,
) -> Option<PathBuf> {
    if !agent_has_accounting_extractor(resolved.agent.id()) {
        return None;
    }
    let target = resolved_agent_target_slug(resolved).unwrap_or_else(|| resolved.agent.id().to_string());
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = ACCOUNTING_INVOCATION_FILE_SEQUENCE
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Some(runtime_dir.join("accounting/captures").join(format!(
        "{}-{}-{}-{}-{}.jsonl",
        safe_accounting_file_segment(task_id),
        safe_accounting_file_segment(state_name),
        safe_accounting_file_segment(&target),
        millis,
        sequence
    )))
}

fn accounting_invocation_id(
    task_id: &str,
    state: &str,
    resolved: &ResolvedAgent,
    visit: u64,
) -> String {
    let target_slug = resolved_agent_target_slug(resolved);
    format!(
        "{}::{}::{}::visit-{}",
        task_id,
        state,
        target_slug.as_deref().unwrap_or(resolved.agent.id()),
        visit
    )
}

fn usage_capture_for_spawn(
    resolved: &ResolvedAgent,
    capture_path: Option<&Path>,
    task_id: &str,
    state: &str,
    visit: u64,
    slot: rhei_tui::Slot,
) -> Option<AgentUsageCapture> {
    let extractor = agent_usage_extractor(resolved.agent.id())?;
    Some(AgentUsageCapture {
        extractor,
        path: capture_path?.to_path_buf(),
        invocation_id: accounting_invocation_id(task_id, state, resolved, visit),
        task_id: task_id.to_string(),
        state: state.to_string(),
        agent: resolved.agent.id().to_string(),
        provider: resolved.model_provider.clone(),
        model: resolved.model_name.clone().or_else(|| resolved.model.clone()),
        slot,
    })
}

fn configure_agent_accounting_args(cmd: &mut std::process::Command, resolved: &ResolvedAgent) {
    if agent_usage_extractor(resolved.agent.id()) == Some(AgentUsageExtractor::CodexJson) {
        // §FS-rhei-cost-accounting.4: Codex usage is extracted from JSONL turn events.
        cmd.arg("--json");
    }
}

fn configure_accounting_capture(cmd: &mut std::process::Command, capture_path: Option<&Path>) {
    if let Some(path) = capture_path {
        // §FS-rhei-cost-accounting.4: Declare the structured usage capture path before spawn.
        cmd.env("RHEI_ACCOUNTING_USAGE_PATH", path);
        cmd.env("RHEI_ACCOUNTING_USAGE_SCHEMA", ACCOUNTING_USAGE_EVENT_SCHEMA);
    }
}

fn capture_agent_output_usage(
    capture: Option<&AgentUsageCapture>,
    stream: rhei_tui::AgentStream,
    line: &str,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let Some(capture) = capture else { return };
    if stream != rhei_tui::AgentStream::Stdout {
        return;
    }
    let Some(usage) = extract_usage_from_output_line(capture.extractor, line) else {
        return;
    };
    if append_usage_capture_event(&capture.path, usage).is_err() {
        return;
    }
    if let ExtractedUsageStatus::Measured(aggregate) = extract_usage_from_capture(Some(&capture.path))
    {
        let usage = usage_summary_from_extracted_usage(
            &capture.invocation_id,
            &capture.state,
            &capture.agent,
            capture.provider.clone(),
            capture.model.clone(),
            aggregate,
        );
        sink.emit(rhei_tui::RunEvent::UsageReported {
            slot: Some(capture.slot),
            task: capture.task_id.clone(),
            invocation_id: capture.invocation_id.clone(),
            usage,
        });
    }
}

fn display_agent_output_line(
    capture: Option<&AgentUsageCapture>,
    stream: rhei_tui::AgentStream,
    line: &str,
) -> String {
    if stream == rhei_tui::AgentStream::Stdout {
        if let Some(capture) = capture {
            if let Some(display) = display_output_line(capture.extractor, line) {
                return display;
            }
        }
    }
    line.to_string()
}

fn extract_usage_from_output_line(
    extractor: AgentUsageExtractor,
    line: &str,
) -> Option<ExtractedUsage> {
    match extractor {
        AgentUsageExtractor::CodexJson => extract_codex_json_usage(line),
        AgentUsageExtractor::StructuredCapture => None,
    }
}

fn display_output_line(extractor: AgentUsageExtractor, line: &str) -> Option<String> {
    match extractor {
        AgentUsageExtractor::CodexJson => display_codex_json_line(line),
        AgentUsageExtractor::StructuredCapture => None,
    }
}

fn extract_codex_json_usage(line: &str) -> Option<ExtractedUsage> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let object = value.as_object()?;
    if object.get("type").and_then(serde_json::Value::as_str) != Some("turn.completed") {
        return None;
    }
    object.get("usage").and_then(usage_from_json_payload)
}

fn display_codex_json_line(line: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let object = value.as_object()?;
    match object.get("type").and_then(serde_json::Value::as_str)? {
        "thread.started" => object
            .get("thread_id")
            .and_then(serde_json::Value::as_str)
            .map(|id| format!("codex thread started: {id}")),
        "turn.started" => Some("codex turn started".to_string()),
        "turn.completed" => extract_codex_json_usage(line).map(|usage| {
            format!(
                "codex turn completed: total={} input={} cached_input={} output={}",
                usage
                    .total
                    .or_else(|| sum_optional_pair(usage.input_total, usage.output_total))
                    .map(format_plain_u64)
                    .unwrap_or_else(|| "-".to_string()),
                usage.input_total.map(format_plain_u64).unwrap_or_else(|| "-".to_string()),
                usage.input_cached_read
                    .map(format_plain_u64)
                    .unwrap_or_else(|| "-".to_string()),
                usage.output_total.map(format_plain_u64).unwrap_or_else(|| "-".to_string()),
            )
        }),
        "item.completed" => object
            .get("item")
            .and_then(serde_json::Value::as_object)
            .and_then(|item| {
                if item.get("type").and_then(serde_json::Value::as_str) == Some("agent_message") {
                    item.get("text").and_then(serde_json::Value::as_str).map(str::to_string)
                } else {
                    None
                }
            }),
        "error" => object
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(|message| format!("codex error: {message}")),
        _ => None,
    }
}

fn format_plain_u64(value: u64) -> String {
    value.to_string()
}

fn append_usage_capture_event(path: &Path, usage: ExtractedUsage) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut usage_object = serde_json::Map::new();
    if let Some(value) = usage.total {
        usage_object.insert("total_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.input_total {
        usage_object.insert("input_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.input_cached_read {
        usage_object.insert("cached_input_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.input_cache_write {
        usage_object.insert("cache_write_input_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.output_total {
        usage_object.insert("output_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.output_cached_read {
        usage_object.insert("cached_output_tokens".to_string(), serde_json::Value::from(value));
    }
    if let Some(value) = usage.output_cache_write {
        usage_object.insert("output_cache_write".to_string(), serde_json::Value::from(value));
    }
    let event = serde_json::json!({
        "schema": ACCOUNTING_USAGE_EVENT_SCHEMA,
        "usage": serde_json::Value::Object(usage_object),
    });
    let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", event)
}

fn extract_usage(capture_path: Option<&Path>, log_path: Option<&Path>) -> ExtractedUsageStatus {
    match extract_usage_from_capture(capture_path) {
        ExtractedUsageStatus::NoUsageEmitted => {
            extract_usage_from_agent_log(log_path).unwrap_or(ExtractedUsageStatus::NoUsageEmitted)
        }
        other => other,
    }
}

fn extract_usage_from_capture(capture_path: Option<&Path>) -> ExtractedUsageStatus {
    // §FS-rhei-cost-accounting.4: Only Rhei-declared structured usage events are accepted.
    let Some(capture_path) = capture_path else {
        return ExtractedUsageStatus::ExtractorUnavailable;
    };
    if !capture_path.is_file() {
        return ExtractedUsageStatus::NoUsageEmitted;
    }
    let Ok(text) = fs::read_to_string(capture_path) else {
        return ExtractedUsageStatus::ExtractorUnavailable;
    };
    let mut aggregate = ExtractedUsage::default();
    let mut saw = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        let value = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) => value,
            Err(_) => return ExtractedUsageStatus::ExtractorFailed,
        };
        if let Some(usage) = usage_from_structured_event_value(&value) {
            aggregate.merge(usage);
            saw = true;
        }
    }
    if saw && aggregate.has_total() {
        ExtractedUsageStatus::Measured(aggregate)
    } else {
        ExtractedUsageStatus::NoUsageEmitted
    }
}

fn extract_usage_from_agent_log(log_path: Option<&Path>) -> Option<ExtractedUsageStatus> {
    let log_path = log_path?;
    let text = fs::read_to_string(log_path).ok()?;
    parse_codex_total_tokens_from_log(&text).map(|total| {
        ExtractedUsageStatus::Measured(ExtractedUsage {
            total: Some(total),
            total_source: Some("agent-log-total"),
            ..ExtractedUsage::default()
        })
    })
}

fn parse_codex_total_tokens_from_log(text: &str) -> Option<u64> {
    let mut lines = text.lines();
    let mut last_total = None;
    while let Some(line) = lines.next() {
        if line.trim().eq_ignore_ascii_case("tokens used") {
            if let Some(value_line) = lines.next() {
                if let Some(value) = parse_token_count(value_line.trim()) {
                    last_total = Some(value);
                }
            }
        }
    }
    last_total
}

fn parse_token_count(text: &str) -> Option<u64> {
    let compact: String = text.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if compact.is_empty() {
        None
    } else {
        compact.parse().ok()
    }
}

fn usage_from_structured_event_value(value: &serde_json::Value) -> Option<ExtractedUsage> {
    let object = value.as_object()?;
    let schema = object.get("schema").and_then(serde_json::Value::as_str)?;
    if schema != ACCOUNTING_USAGE_EVENT_SCHEMA {
        return None;
    }
    object
        .get("usage")
        .and_then(usage_from_json_payload)
        .or_else(|| usage_from_json_payload(value))
}

fn usage_from_json_payload(value: &serde_json::Value) -> Option<ExtractedUsage> {
    let object = value.as_object()?;
    for key in ["usage", "token_usage", "tokens", "metrics"] {
        if let Some(nested) = object.get(key).and_then(usage_from_json_payload) {
            return Some(nested);
        }
    }

    let mut usage = ExtractedUsage {
        total: first_u64(object, &["total_tokens", "tokens_used", "total"]),
        total_source: None,
        input_total: first_u64(
            object,
            &[
                "input_tokens",
                "prompt_tokens",
                "input_total",
                "total_input_tokens",
            ],
        ),
        output_total: first_u64(
            object,
            &[
                "output_tokens",
                "completion_tokens",
                "output_total",
                "total_output_tokens",
            ],
        ),
        input_cached_read: first_u64(
            object,
            &[
                "cache_read_input_tokens",
                "cached_input_tokens",
                "input_cached_read",
            ],
        ),
        input_cache_write: first_u64(
            object,
            &[
                "cache_creation_input_tokens",
                "cache_write_input_tokens",
                "input_cache_write",
            ],
        ),
        output_cached_read: first_u64(
            object,
            &["output_cached_read", "cached_output_tokens"],
        ),
        output_cache_write: first_u64(object, &["output_cache_write"]),
    };

    if let Some(input) = object.get("input").and_then(serde_json::Value::as_object) {
        usage.input_total = usage.input_total.or_else(|| dimension_u64(input.get("total")));
        usage.input_cached_read =
            usage.input_cached_read.or_else(|| dimension_u64(input.get("cached_read")));
        usage.input_cache_write =
            usage.input_cache_write.or_else(|| dimension_u64(input.get("cache_write")));
    }
    if let Some(output) = object.get("output").and_then(serde_json::Value::as_object) {
        usage.output_total = usage.output_total.or_else(|| dimension_u64(output.get("total")));
        usage.output_cached_read =
            usage.output_cached_read.or_else(|| dimension_u64(output.get("cached_read")));
        usage.output_cache_write =
            usage.output_cache_write.or_else(|| dimension_u64(output.get("cache_write")));
    }

    usage.has_total().then_some(usage)
}

fn first_u64(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<u64> {
    keys.iter().find_map(|key| dimension_u64(object.get(*key)))
}

fn dimension_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    match value? {
        serde_json::Value::Number(number) => number.as_u64(),
        serde_json::Value::String(text) => text.parse::<u64>().ok(),
        serde_json::Value::Object(object) => first_u64(object, &["value", "tokens"]),
        _ => None,
    }
}

fn tokens_from_usage(usage: ExtractedUsage) -> AccountingTokens {
    // §FS-rhei-cost-accounting.3.1: Missing dimensions remain unavailable.
    let total = usage.total.or_else(|| sum_optional_pair(usage.input_total, usage.output_total));
    let total_source = usage.total_source.unwrap_or("agent-usage-capture");
    AccountingTokens {
        total: total
            .map(|value| AccountingTokenDimension::measured_from(value, total_source))
            .unwrap_or_else(|| AccountingTokenDimension::unavailable("unknown")),
        input: AccountingTokenSide {
            total: usage
                .input_total
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unknown")),
            cached_read: usage
                .input_cached_read
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unsupported")),
            cache_write: usage
                .input_cache_write
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unsupported")),
        },
        output: AccountingTokenSide {
            total: usage
                .output_total
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unknown")),
            cached_read: usage
                .output_cached_read
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unsupported")),
            cache_write: usage
                .output_cache_write
                .map(AccountingTokenDimension::measured)
                .unwrap_or_else(|| AccountingTokenDimension::unavailable("unsupported")),
        },
    }
}

fn sum_optional_pair(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn price_tokens(
    provider: Option<&str>,
    model: Option<&str>,
    tokens: &AccountingTokens,
) -> AccountingPricing {
    // §FS-rhei-cost-accounting.5: Pricing is separate from measurement.
    let priceable_measured = [
        tokens.input.total.value,
        tokens.input.cached_read.value,
        tokens.input.cache_write.value,
        tokens.output.total.value,
        tokens.output.cached_read.value,
        tokens.output.cache_write.value,
    ]
    .into_iter()
    .flatten()
    .count();
    if priceable_measured == 0 && tokens.total.value.is_none() {
        return AccountingPricing {
            status: "not-applicable".to_string(),
            currency: None,
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: None,
        };
    }
    if priceable_measured == 0 {
        return AccountingPricing {
            status: "unpriced".to_string(),
            currency: Some("USD".to_string()),
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: Some(PRICE_BOOK_ID.to_string()),
        };
    }

    let Some(entry) = price_entry(provider, model) else {
        return AccountingPricing {
            status: "unpriced".to_string(),
            currency: Some("USD".to_string()),
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: Some(PRICE_BOOK_ID.to_string()),
        };
    };
    let mut amount = 0u64;
    amount = amount.saturating_add(price_dimension(tokens.input.total.value, entry.input_total_micro));
    amount = amount.saturating_add(price_dimension(
        tokens.input.cached_read.value,
        entry.input_cached_read_micro,
    ));
    amount = amount.saturating_add(price_dimension(
        tokens.input.cache_write.value,
        entry.input_cache_write_micro,
    ));
    amount = amount.saturating_add(price_dimension(tokens.output.total.value, entry.output_total_micro));
    AccountingPricing {
        status: "priced".to_string(),
        currency: Some("USD".to_string()),
        amount_micro: Some(amount),
        priced_amount_micro: Some(amount),
        price_book_id: Some(PRICE_BOOK_ID.to_string()),
    }
}

fn price_dimension(tokens: Option<u64>, price_micro: u64) -> u64 {
    let Some(tokens) = tokens else { return 0 };
    // §FS-rhei-cost-accounting.5: Cost uses integer micro-unit arithmetic.
    ((tokens as u128 * price_micro as u128) / PRICE_UNIT_TOKENS as u128) as u64
}

fn price_entry(provider: Option<&str>, model: Option<&str>) -> Option<PriceBookEntry> {
    let provider = provider?;
    let model = model?;
    builtin_price_entries()
        .into_iter()
        .find(|entry| entry.provider == provider && entry.model == model)
}

fn builtin_price_entries() -> Vec<PriceBookEntry> {
    vec![PriceBookEntry {
        provider: "anthropic",
        model: "claude-sonnet-4-6",
        effective_at: "2026-05-20T00:00:00Z",
        unit: "1m_tokens",
        input_total_micro: 3_000_000,
        input_cached_read_micro: 300_000,
        input_cache_write_micro: 3_750_000,
        output_total_micro: 15_000_000,
    }]
}

fn write_price_book(accounting_root: &Path) -> MietteResult<()> {
    let path = accounting_root.join("prices.json");
    let price_book = PriceBook {
        schema: ACCOUNTING_PRICES_SCHEMA,
        price_book_id: PRICE_BOOK_ID,
        currency: "USD",
        entries: builtin_price_entries(),
    };
    write_json_atomic(&path, &price_book)
}

fn write_invocation_record(
    accounting_root: &Path,
    record: &AccountingInvocationRecord,
) -> MietteResult<PathBuf> {
    // §FS-rhei-cost-accounting.2: File names use path-safe file ids.
    let dir = accounting_root.join("invocations");
    fs::create_dir_all(&dir)
        .map_err(|err| file_io_report(&dir, "failed to create accounting invocation directory", err))?;
    let file_id = invocation_file_id(record);
    let path = dir.join(format!("{file_id}.json"));
    write_json_atomic(&path, record)?;
    Ok(path)
}

fn write_json_atomic(path: &Path, value: &impl serde::Serialize) -> MietteResult<()> {
    // §FS-rhei-cost-accounting.11: Publish accounting artifacts atomically.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create accounting directory", err))?;
    }
    let staging = unique_staging_path(path);
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| miette!("failed to serialize accounting artifact '{}': {err}", path.display()))?;
    fs::write(&staging, text)
        .map_err(|err| file_io_report(&staging, "failed to write accounting staging file", err))?;
    fs::rename(&staging, path)
        .map_err(|err| file_io_report(path, "failed to publish accounting artifact", err))
}

fn unique_staging_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or("artifact.json");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nanos))
}

fn invocation_file_id(record: &AccountingInvocationRecord) -> String {
    let mut hasher = Sha256::new();
    hasher.update(record.invocation_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(record.started_at.as_bytes());
    hasher.update(b"\0");
    hasher.update(record.ended_at.as_bytes());
    hasher.update(b"\0");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = ACCOUNTING_INVOCATION_FILE_SEQUENCE
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(nanos.to_le_bytes());
    hasher.update(sequence.to_le_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(32);
    for byte in &digest[..16] {
        std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"))
            .expect("writing to String cannot fail");
    }
    out
}

fn safe_accounting_file_segment(value: &str) -> String {
    // §FS-rhei-cost-accounting.2: Task index file ids preserve distinct task ids.
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                encoded.push(*byte as char);
            }
            other => {
                std::fmt::Write::write_fmt(&mut encoded, format_args!("%{other:02X}"))
                    .expect("writing to String cannot fail");
            }
        }
    }
    if encoded.is_empty() {
        "task".to_string()
    } else {
        encoded
    }
}

fn is_descendant_id(candidate: &str, ancestor: &str) -> bool {
    candidate.starts_with(ancestor) && candidate.as_bytes().get(ancestor.len()) == Some(&b'.')
}

fn format_summary_cost(summary: &rhei_tui::AccountingRunSummary) -> String {
    match summary.cost_micro.or(summary.priced_cost_micro) {
        Some(value) => format_cost_micro(value, summary.currency.as_deref()),
        None => "unpriced".to_string(),
    }
}

fn format_usage_cost(usage: &rhei_tui::UsageSummary) -> String {
    match usage.cost_micro.or(usage.priced_cost_micro) {
        Some(value) => format_cost_micro(value, usage.currency.as_deref()),
        None => "unpriced".to_string(),
    }
}

fn format_cost_micro(value: u64, currency: Option<&str>) -> String {
    let units = value / 1_000_000;
    let cents = (value % 1_000_000) / 10_000;
    match currency {
        Some("USD") | None => format!("${units}.{cents:02}"),
        Some(currency) => format!("{units}.{cents:02} {currency}"),
    }
}

fn format_dimension_value(summary: &rhei_tui::DimensionSummary) -> String {
    let Some(value) = summary.value else {
        return "-".to_string();
    };
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn summary_sort_cost(summary: &rhei_tui::AccountingRunSummary) -> u64 {
    summary.cost_micro.or(summary.priced_cost_micro).unwrap_or(0)
}
