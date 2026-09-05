// Durable accounting record types shared by writers, readers, and inspection.

const ACCOUNTING_INVOCATION_SCHEMA: &str = "rhei.accounting.invocation.v1";
const ACCOUNTING_USAGE_EVENT_SCHEMA: &str = "rhei.accounting.usage.v1";
static ACCOUNTING_INVOCATION_FILE_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingInvocationRecord {
    schema: String,
    invocation_id: String,
    /// The one invocation of `rhei run` that spawned this process.
    ///
    /// Optional, and the schema string does not move with it: a record written
    /// before the field existed still parses, and is *unattributed* — never
    /// dropped, never folded into a named run.
    // §FS-rhei-cost-accounting.3.5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run_id: Option<String>,
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
    // Optional on read for records written before this field was published.
    // §FS-rhei-cost-accounting.3.4
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cli_session: Option<AccountingCliSession>,
    extraction_status: String,
    scope: String,
    tokens: AccountingTokens,
    pricing: AccountingPricing,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct AccountingCliSession {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    store_path: Option<String>,
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
