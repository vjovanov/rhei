// The run report's accounting strip: what this run spent, which of the two
// ways that number was arrived at, and the workspace's lifetime total on a row
// of its own.
//
// The strip answers one question — *what did this run spend* — and it must not
// be readable as any other. Until this existed the table was headed plain
// `Accounting` and carried the workspace's whole history, so a report whose own
// activity table read `agent invocations | 0` printed 105.9M directly above its
// own note that nothing ran. The model and both of its renderers live here
// together, because the heading, the `source` row and the lifetime row are one
// decision rendered twice.

// §AR-source-file-size.3 §FS-rhei-run-report.2.1

/// Which of the two ways the accounting strip can be produced produced this
/// one.
///
/// They are different quantities, and a table that does not say which is the
/// defect, not the number: in one workspace a run showing 2.6M sat between
/// neighbours showing 98.9M and 105.9M with nothing to tell them apart.
// §FS-rhei-run-report.2.1
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AccountingSource {
    /// The end-of-run rollup over the invocation records that name this run.
    Rollup,
    /// No rollup could be produced, so only the `UsageReported` events this run
    /// observed were summarized. This is the default because it is what the
    /// report can show before — or instead of — `RunFinished` publishing a
    /// rollup, which is exactly the interrupted run's case.
    // §FS-rhei-cost-accounting.7
    #[default]
    RunEvents,
}

impl AccountingSource {
    fn label(self) -> &'static str {
        match self {
            AccountingSource::Rollup => "rollup",
            AccountingSource::RunEvents => "run events",
        }
    }
}

/// The accounting strip's two numbers and the source that produced them.
// §FS-rhei-run-report.2.1
#[derive(Clone, Default)]
struct RunAccountingStrip {
    /// What this run spent.
    run: Option<rhei_tui::AccountingRunSummary>,
    /// What the workspace has spent in its lifetime. Absent under the events
    /// fallback, which has no rollup to read it from, rather than guessed.
    workspace: Option<rhei_tui::AccountingRunSummary>,
    source: AccountingSource,
}

impl RunAccountingStrip {
    /// The durable report's table. Empty when the run has no accounting at all,
    /// so a workspace that never spawned an accountable agent gains no table.
    // §FS-rhei-run-report.2.1
    fn render_markdown(&self) -> String {
        let Some(accounting) = &self.run else {
            return String::new();
        };
        let mut out = String::new();
        // The heading names the scope: `Accounting` alone is completed by
        // whatever the reader came looking for. §FS-rhei-run-report.2.1
        out.push_str("| Accounting (this run) | Value |\n| --- | ---: |\n");
        // Which of the strip's two quantities this is, before any number.
        out.push_str(&format!("| source | {} |\n", self.source.label()));
        out.push_str(&format!("| cost | {} |\n", format_summary_cost(accounting)));
        for (label, dimension) in [
            ("total tokens", &accounting.total),
            ("input tokens", &accounting.input_total),
            ("input cached", &accounting.input_cached_read),
            ("output tokens", &accounting.output_total),
            ("output cached", &accounting.output_cached_read),
        ] {
            out.push_str(&format!("| {label} | {} |\n", format_dimension_value(dimension)));
        }
        out.push_str(&format!("| coverage | {:?} |\n", accounting.coverage));
        // A row of its own, and only under a rollup: the events fallback has
        // nothing to read it from. §FS-rhei-run-report.2.1
        if let Some(workspace) = &self.workspace {
            out.push_str(&format!(
                "| workspace total tokens | {} |\n",
                format_dimension_value(&workspace.total)
            ));
        }
        out.push('\n');
        out
    }

    /// The end-of-run console's line, carrying the same quantity under the same
    /// scope label as the table above. §FS-rhei-run-report.2.1 §FS-rhei-cost-accounting.9
    fn render_console(&self) -> String {
        let Some(accounting) = &self.run else {
            return String::new();
        };
        let mut out = format!(
            "  This run  {} · Total {} · In {} · In cached {} · Out {} · Out cached {} · \
             Coverage {:?} · via {}\n",
            format_summary_cost(accounting),
            format_dimension_value(&accounting.total),
            format_dimension_value(&accounting.input_total),
            format_dimension_value(&accounting.input_cached_read),
            format_dimension_value(&accounting.output_total),
            format_dimension_value(&accounting.output_cached_read),
            accounting.coverage,
            self.source.label(),
        );
        if let Some(workspace) = &self.workspace {
            out.push_str(&format!(
                "  Workspace {} tokens over its lifetime\n",
                format_dimension_value(&workspace.total)
            ));
        }
        out
    }
}
