// What the run report knows about spend: one record per invocation id, upserted
// as reports arrive, from which both the run-level accounting strip and every
// task cost row are derived.
//
// Its own part because the strip and the rows next door are renderers over this
// list, and one list is what keeps the two levels from disagreeing.

// §AR-source-file-size.3 §FS-rhei-cost-accounting.9

/// The most recent usage one invocation reported, with the task it was reported
/// against. The task is part of the record rather than a key beside it, so an
/// invocation that moves carries its usage with it. §FS-rhei-cost-accounting.9
struct UsageRecord {
    task: String,
    usage: rhei_tui::UsageSummary,
}

/// Reported usage, keyed by invocation id. `UsageReported` arrives repeatedly
/// for one invocation as a streaming extractor observes further turns, and
/// arrives after `SlotReleased`, so a report replaces its predecessor rather
/// than adding to it. §FS-rhei-cost-accounting.9
#[derive(Default)]
struct UsageLedger {
    records: Vec<UsageRecord>,
}

impl UsageLedger {
    /// Record what `invocation_id` last reported, replacing any earlier report
    /// for it. Returns the task the invocation left, when the report moved it to
    /// another one, so that task's row can be rebuilt without the usage it no
    /// longer owns. §FS-rhei-cost-accounting.9
    fn report(
        &mut self,
        task: &str,
        invocation_id: &str,
        usage: rhei_tui::UsageSummary,
    ) -> Option<String> {
        let found =
            self.records.iter().position(|record| record.usage.invocation_id == invocation_id);
        let Some(index) = found else {
            self.records.push(UsageRecord { task: task.to_string(), usage });
            return None;
        };
        let record = &mut self.records[index];
        record.usage = usage;
        (record.task != task).then(|| std::mem::replace(&mut record.task, task.to_string()))
    }

    /// The run-level rollup, used before `RunFinished` publishes the
    /// authoritative one and as the fallback when it carries none.
    /// §FS-rhei-cost-accounting.9
    fn run_rollup(&self) -> Option<rhei_tui::AccountingRunSummary> {
        rhei_tui::summarize_usage_summaries(self.records.iter().map(|record| &record.usage))
    }

    /// One task's direct rollup, over the same records: `None` once no
    /// invocation is recorded against it. §FS-rhei-cost-accounting.9
    fn task_rollup(&self, task: &str) -> Option<rhei_tui::AccountingRunSummary> {
        rhei_tui::summarize_usage_summaries(
            self.records.iter().filter(|record| record.task == task).map(|record| &record.usage),
        )
    }
}
