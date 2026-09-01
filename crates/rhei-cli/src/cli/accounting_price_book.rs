// Owned price books: the built-in default, caller input validation, and
// durable copying. Invocation pricing remains beside token accounting.

// §AR-source-file-size.3 §FS-rhei-cost-accounting.5.1

const ACCOUNTING_PRICES_SCHEMA: &str = "rhei.accounting.prices.v1";
const PRICE_BOOK_ID: &str = "builtin-2026-05-20";
const PRICE_UNIT_TOKENS: u64 = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct PriceBook {
    schema: String,
    price_book_id: String,
    currency: String,
    entries: Vec<PriceBookEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct PriceBookEntry {
    provider: String,
    model: String,
    effective_at: String,
    unit: String,
    input_total_micro: u64,
    input_cached_read_micro: u64,
    input_cache_write_micro: u64,
    output_total_micro: u64,
}

fn builtin_price_entries() -> Vec<PriceBookEntry> {
    vec![PriceBookEntry {
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        effective_at: "2026-05-20T00:00:00Z".to_string(),
        unit: "1m_tokens".to_string(),
        input_total_micro: 3_000_000,
        input_cached_read_micro: 300_000,
        input_cache_write_micro: 3_750_000,
        output_total_micro: 15_000_000,
    }]
}

fn builtin_price_book() -> PriceBook {
    PriceBook {
        schema: ACCOUNTING_PRICES_SCHEMA.to_string(),
        price_book_id: PRICE_BOOK_ID.to_string(),
        currency: "USD".to_string(),
        entries: builtin_price_entries(),
    }
}

fn load_price_book(path: &Path) -> MietteResult<PriceBook> {
    // Read exactly once before agent execution. §FS-rhei-cost-accounting.5.1
    let text = fs::read_to_string(path).map_err(|err| {
        miette!(
            help = "pass a readable local JSON file using rhei.accounting.prices.v1",
            "failed to read price book '{}': {err}", path.display()
        )
    })?;
    let price_book: PriceBook = serde_json::from_str(&text).map_err(|err| {
        miette!(
            help = "provide schema, price_book_id, currency, and entries in the rhei.accounting.prices.v1 JSON shape",
            "failed to parse price book '{}': {err}", path.display()
        )
    })?;
    validate_price_book(path, &price_book)?;
    Ok(price_book)
}

fn validate_price_book(path: &Path, price_book: &PriceBook) -> MietteResult<()> {
    let invalid = |problem: String| {
        miette!(
            help = "use a non-empty id and currency, unique provider/model entries, and unit `1m_tokens`",
            "invalid price book '{}': {problem}", path.display()
        )
    };
    if price_book.schema != ACCOUNTING_PRICES_SCHEMA {
        return Err(invalid(format!(
            "unsupported schema '{}'; expected '{}'",
            price_book.schema, ACCOUNTING_PRICES_SCHEMA
        )));
    }
    if price_book.price_book_id.trim().is_empty() {
        return Err(invalid("price_book_id must not be empty".to_string()));
    }
    if price_book.currency.trim().is_empty() {
        return Err(invalid("currency must not be empty".to_string()));
    }
    let mut matches = BTreeSet::new();
    for entry in &price_book.entries {
        if entry.provider.trim().is_empty() || entry.model.trim().is_empty() {
            return Err(invalid("entry provider and model must not be empty".to_string()));
        }
        if entry.effective_at.trim().is_empty() {
            return Err(invalid(format!(
                "effective_at must not be empty for {}/{}",
                entry.provider, entry.model
            )));
        }
        if entry.unit != "1m_tokens" {
            return Err(invalid(format!(
                "unsupported unit '{}' for {}/{}",
                entry.unit, entry.provider, entry.model
            )));
        }
        if !matches.insert((&entry.provider, &entry.model)) {
            return Err(invalid(format!(
                "duplicate provider/model entry for {}/{}",
                entry.provider, entry.model
            )));
        }
    }
    Ok(())
}

fn write_price_book(accounting_root: &Path, price_book: &PriceBook) -> MietteResult<()> {
    let path = accounting_root.join("prices.json");
    write_json_atomic(&path, price_book)
}
