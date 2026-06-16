# FS-rhei-cost-accounting: Rhei Cost Accounting

Rhei records token usage for agent work, converts measured tokens into cost
with a reproducible price book, rolls the result up to every task node, and
shows those totals in the CLI, TUI, and browser dashboard. §GOAL-rhei-outcomes §FS-rhei-run

For agent spawning see [Agents Specification](rhei-agents.spec.md). For run
events and dashboard transport see [Run TUI Specification](rhei-run-tui.spec.md).
For visual dashboard behavior see [Flow Visualization](rhei-viz.spec.md).

## Goals

1. Every `claude-code`, `codex`, and `pi` invocation spawned by `rhei run`
   produces either a measured usage record or an explicit failure/status
   record. §FS-rhei-agents
2. Every task node has derived direct and subtree token/cost totals. §FS-rhei-plan-language
3. Token measurement and price calculation are separate so old runs stay
   explainable when provider prices change.
4. Unknown, omitted, unsupported, partial, and zero-valued token dimensions are
   distinct.
5. Monitoring views show spend, token totals, cache effect, and coverage while
   work is running. §FS-rhei-run-tui §FS-rhei-viz

## Non-Goals

- Guessing billing from transcript bytes, prompt text length, or local
  tokenizers when measured usage exists.
- Enforcing budgets or stopping a run based on spend.
- Writing cost rollups into task markdown.
- Failing a task just because accounting is unsupported or extraction failed.

## 1. Mental Model

Cost accounting has three layers:

| Layer | Meaning |
| --- | --- |
| Invocation record | One durable record for one spawned agent process. This is the source of truth. |
| Price book | The versioned table used to turn measured tokens into currency. |
| Rollups | Derived task, subtree, and run totals computed from invocation records. |

The important rule is: **measure first, price second, roll up last**.

## 2. Runtime Files

Rhei stores accounting under the workspace:

```text
runtime/accounting/
  invocations/<invocation_file_id>.json
  tasks/<task_file_id>.json
  summary.json
  prices.json
```

`invocations/` is authoritative. `tasks/` and `summary.json` are derived
indexes and may be regenerated from invocation records and the current plan
tree.

`invocation_id` is the logical identity inside the JSON record. It may contain
task ids, states, target slugs, and visit numbers. File names must use
`invocation_file_id`, which is path-safe: a UUID/ULID, encoded id, or hash.
Raw `invocation_id` text must not be used as the file name.

Task rollup JSON contains the raw `task_id`; `task_file_id` must be a path-safe,
collision-resistant encoding of that id so distinct valid task ids do not
overwrite the same derived rollup file.

## 3. Invocation Record

Each supported agent spawn writes one JSON object:

```json
{
  "schema": "rhei.accounting.invocation.v1",
  "invocation_id": "1::pending::claude-code-anthropic-sonnet::visit-1",
  "task_id": "1",
  "state": "pending",
  "visit": 1,
  "target_slug": "claude-code-anthropic-sonnet",
  "agent": "claude-code",
  "provider": "anthropic",
  "model": "claude-sonnet-4-6",
  "started_at": "2026-05-20T10:30:00Z",
  "ended_at": "2026-05-20T10:34:23Z",
  "extraction_status": "measured",
  "scope": "aggregate-agent-process",
  "tokens": {
    "input": {
      "total": { "value": 12345, "source": "agent-usage-capture" },
      "cached_read": { "value": 9000, "source": "agent-usage-capture" },
      "cache_write": { "value": 1200, "source": "agent-usage-capture" }
    },
    "output": {
      "total": { "value": 2300, "source": "agent-usage-capture" },
      "cached_read": { "status": "unsupported" },
      "cache_write": { "status": "unsupported" }
    }
  },
  "pricing": {
    "status": "priced",
    "currency": "USD",
    "amount_micro": 18342,
    "price_book_id": "builtin-2026-05-20"
  }
}
```

### 3.1. Token Dimensions

| Dimension | Meaning |
| --- | --- |
| `input.total` | Total input tokens reported by the agent/provider. |
| `input.cached_read` | Input tokens served from cache. |
| `input.cache_write` | Input tokens written into cache. |
| `output.total` | Total output tokens reported by the agent/provider. |
| `output.cached_read` | Output tokens served from cache, if reported. |
| `output.cache_write` | Output tokens written to cache, if reported. |

Each dimension is either measured:

```json
{ "value": 12345, "source": "agent-usage-capture" }
```

or unavailable:

```json
{ "status": "unsupported" }
```

Unavailable statuses are:

| Status | Meaning |
| --- | --- |
| `unsupported` | The agent/provider cannot report this dimension. |
| `omitted` | The agent/provider may support it, but this invocation omitted it. |
| `unknown` | Rhei tried to extract it but could not determine the value. |

Measured zero is `"value": 0`; it is not the same as unavailable.

### 3.2. Extraction Status

Every `claude-code`, `codex`, and `pi` invocation writes a record even when
tokens cannot be measured.

| `extraction_status` | Meaning |
| --- | --- |
| `measured` | At least input or output total tokens were extracted. |
| `unsupported-agent` | The agent has no accounting extractor. |
| `extractor-unavailable` | The configured extractor could not run. |
| `extractor-failed` | The extractor ran but could not parse usage data. |
| `no-usage-emitted` | The agent exited without producing supported usage data. |

Unsupported custom agents may omit records only when the resolved agent profile
has no accounting extractor. Built-in `claude-code`, `codex`, and `pi` must not
silently omit records.

### 3.3. Measurement Scope

`scope` says what one record covers:

| Scope | Meaning |
| --- | --- |
| `aggregate-agent-process` | Usage for the whole spawned agent process. |
| `provider-call` | Usage for one provider API call. |
| `child-invocation` | Usage for a nested agent invocation Rhei can identify. |

The v1 built-in extractors may use `aggregate-agent-process` when the agent CLI
does not expose finer-grained usage.

## 4. Extraction Flow

Accounting is separate from snapshots. Snapshot support may provide a useful
transcript source, but a missing snapshot `session` profile must not disable
accounting for `claude-code`, `codex`, or `pi`. §FS-rhei-snapshots

For each agent invocation:

1. Before spawn, the extractor declares any extra arguments, environment
   variables, or capture paths needed for structured usage. Rhei's built-in
   capture contract sets `RHEI_ACCOUNTING_USAGE_PATH` and
   `RHEI_ACCOUNTING_USAGE_SCHEMA=rhei.accounting.usage.v1`.
2. `rhei run` spawns the agent normally.
3. The agent exits and Rhei drains stdout/stderr.
4. Rhei evaluates completion and selects the outgoing transition.
5. Rhei extracts usage and writes the invocation record.
6. Rhei emits `UsageReported`.
7. Rhei applies normal snapshot side effects and task transition behavior.

Extraction failures affect accounting coverage only. They do not change the
agent exit code, completion condition, selected transition, or callbacks.

Built-in extractor requirements:

| Agent | Requirement |
| --- | --- |
| `claude-code` | Use the most structured usage output available from Claude Code. |
| `codex` | Use the most structured usage output from `codex exec` or its runtime transcript. Do not depend on Codex snapshot support. |
| `pi` | Parse Pi JSONL/session usage when available. Accounting-only session data belongs under `runtime/accounting/`, not snapshot cache paths. |

If an upstream CLI changes format, the extractor records `extractor-failed`
with a concise diagnostic. It must not guess from nearby human-readable text.
Rhei must not parse arbitrary agent stdout/stderr JSON as billing telemetry; it
only accepts structured capture events that identify the accounting schema.

## 5. Pricing

`runtime/accounting/prices.json` records the price book used for a run:

```json
{
  "schema": "rhei.accounting.prices.v1",
  "price_book_id": "builtin-2026-05-20",
  "currency": "USD",
  "entries": [
    {
      "provider": "anthropic",
      "model": "claude-sonnet-4-6",
      "effective_at": "2026-05-20T00:00:00Z",
      "unit": "1m_tokens",
      "input_total_micro": 3000000,
      "input_cached_read_micro": 300000,
      "input_cache_write_micro": 3750000,
      "output_total_micro": 15000000
    }
  ]
}
```

Rules:

- Prices are integer micro-units of the configured currency.
- Rhei must not use floating-point arithmetic for cost calculation.
- One price book has exactly one currency. Mixed-currency price books are
  rejected.
- Every priced invocation in one run uses the same price-book currency.

Cost formula:

```text
sum(measured_dimension_tokens * matching_dimension_price / unit_tokens)
```

Pricing status:

| `pricing.status` | Meaning |
| --- | --- |
| `priced` | Every measured billable dimension had a price. |
| `partial-price` | Some measured billable dimensions had prices and some did not. |
| `unpriced` | Tokens were measured, but none of the measured billable dimensions had prices. |
| `not-applicable` | No measured tokens were available to price. |

Missing prices must not be treated as zero-cost. For `partial-price`,
`priced_amount_micro` may be written as a lower-bound amount. `amount_micro` is
written only when status is `priced`.

## 6. Rollups

Rollups are derived from invocation records:

```text
direct(node)  = sum(invocations where invocation.task_id == node.id)
subtree(node) = direct(node) + sum(subtree(child) for every child node)
run_total     = sum(subtree(root) for every root node)
```

Coverage says how complete the rollup is:

| Coverage | Meaning |
| --- | --- |
| `complete` | Every invocation was measured and fully priced. |
| `partial` | At least one invocation or dimension is missing, unsupported, unknown, or only partly priced. |
| `unpriced` | Tokens exist, but no cost could be computed. |
| `none` | No measured usage exists. |

Missing invocation records for supported built-in agents count as `partial`,
because the absence itself is a coverage defect.

Dimension summaries expose partial rollups without forcing dashboards to fetch
every invocation:

```rust
pub enum DimensionStatus {
    Measured,
    Partial,
    Unsupported,
    Omitted,
    Unknown,
}

pub struct DimensionSummary {
    pub value: Option<u64>,
    pub status: DimensionStatus,
    pub measured_count: u64,
    pub missing_count: u64,
}
```

For one invocation, `Measured` means `value` is present. For a rollup,
`Measured` means every contributing invocation reported the dimension.
`Partial` means `value` is a subtotal from measured contributions and at least
one contributing invocation was missing, unsupported, omitted, or unknown.

## 7. Run Events

After the invocation record is written, Rhei emits:

```rust
pub struct UsageSummary {
    pub invocation_id: String,
    pub state: String,
    pub agent: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_total: DimensionSummary,
    pub input_cached_read: DimensionSummary,
    pub input_cache_write: DimensionSummary,
    pub output_total: DimensionSummary,
    pub output_cached_read: DimensionSummary,
    pub output_cache_write: DimensionSummary,
    pub cost_micro: Option<u64>,
    pub priced_cost_micro: Option<u64>,
    pub currency: Option<String>,
    pub coverage: UsageCoverage,
    pub status: UsageStatus,
    pub pricing_status: PricingStatus,
}

pub enum RunEvent {
    UsageReported {
        slot: Option<Slot>,
        task: String,
        invocation_id: String,
        usage: UsageSummary,
    }
}
```

`UsageReported` may arrive after `SlotReleased`; frontends must update task,
slot history, and run totals without assuming the slot is still active. §FS-rhei-run-tui

`RunSummary.accounting` contains an optional `AccountingRunSummary` with the
same dimension, cost, currency, coverage, and pricing-status shape as
`UsageSummary`. It is `None` when the run did not enter agent mode or no
accounting records were produced.

## 8. CLI Inspection

`rhei cost` reads accounting artifacts without changing the plan:

```bash
rhei cost <RHEI_PLAN_OR_WORKSPACE> [--task <ID>] [--json] [--by agent|model|state|node]
```

Default text output shows run totals, coverage, and highest-cost nodes by
subtree cost. `--task <ID>` shows that node's direct and subtree totals plus
the contributing invocation records. `--json` emits the same data with stable
field names matching the runtime artifact schema.

When no accounting artifacts exist, `rhei cost` exits 0 and prints:

```text
(no accounting records found)
```

## 9. Visualization

The TUI header shows a compact run-level strip when accounting is available:

```text
Cost: $1.23  in=2.4M  in_cached=1.5M  out=180k  out_cached=-  coverage=Partial
```

The header uses absolute token totals rather than a cache percentage so cached
input and cached output remain separate. Unavailable dimensions render as `-`.
When `UsageReported` arrives after slot release, the TUI updates the run-level
header and journal summary; it does not keep a completed-slot accounting history
in the terminal UI.

The end-of-run console summary and durable run report include the same run-level
accounting strip. Task rows may show a compact direct task cost only; the direct
task cost is the sum of usage reported for all agent states spawned for that
task in the run. §FS-rhei-run-report

The browser dashboard adds a **Cost** tab before **Journal**. Its live summary
shows:

- run totals and coverage;
- top-level task direct and subtree costs;
- top-level task subtree input, cached input, and output totals.

The dashboard serves per-invocation details from `/accounting/invocations` so a
future drill-down can show token dimensions and pricing status without bloating
the frequently polled `/snapshot` payload.

Task accounting rollups are carried in `task_runtime` so dashboard views can add
direct cost, subtree cost, input, output, cached input, cached output, and
coverage where that density fits. The current Cost tab exposes the compact
top-level task table; future Cube and Sankey modes may use subtree cost as
heatmap color or ribbon width. §FS-rhei-viz

## 10. Dashboard Data

The frequently polled `/snapshot` payload carries compact rollups:

```ts
type TaskAccounting = {
  direct?: AccountingRollup;
  subtree?: AccountingRollup;
};

type TaskRow = {
  // existing flattened task fields
  accounting?: TaskAccounting;
};

type Snapshot = {
  accounting?: AccountingRunSummary;
  tasks: TaskRow[];
};
```

Invocation details are served from a separate loopback endpoint such as
`/accounting/invocations` so `/snapshot` stays small. §FS-rhei-run-tui

## 11. Failure Modes

| Failure | Required behavior |
| --- | --- |
| Extractor failure | Write an invocation record with `extractor-failed`, emit `UsageReported`, and continue normal transition handling. |
| Missing price | Record measured tokens with `unpriced` or `partial-price`. |
| Accounting write failure | Warn in the run journal and mark run accounting coverage partial. Do not hide the agent log or transition outcome. |
| Malformed accounting artifact | `rhei cost` reports the bad path and continues reading other valid records. With `--json`, it returns a structured error. |
| Concurrent writes | Write to a unique staging path, then atomically rename to `<invocation_file_id>.json`. Rollup files may be regenerated after pass writes complete. |

## Related Specifications

- [Agents Specification](rhei-agents.spec.md) - agent configuration and spawn behavior
- [Run Specification](rhei-run.spec.md) - orchestrator execution loop
- [Run TUI Specification](rhei-run-tui.spec.md) - event surface and dashboard transport
- [Flow Visualization](rhei-viz.spec.md) - visual plan views
- [Snapshots Specification](rhei-snapshots.spec.md) - separate session snapshot feature
