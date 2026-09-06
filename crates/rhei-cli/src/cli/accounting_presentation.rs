// The shared presentation projection for normalized accounting totals. It
// owns the public order and wording so run-report and summary renderers cannot
// independently omit a priced cache dimension.

// §FS-rhei-run-report.2.1 §FS-rhei-run-report.2.2 §FS-rhei-summary.2.3

#[derive(Clone, Copy)]
#[repr(usize)]
enum PresentationTokenDimension {
    Total,
    InputTotal,
    InputCacheRead,
    InputCacheWrite,
    OutputTotal,
    OutputCacheRead,
    OutputCacheWrite,
}

const ACCOUNTING_TOKEN_DIMENSIONS: [PresentationTokenDimension; 7] = [
    PresentationTokenDimension::Total,
    PresentationTokenDimension::InputTotal,
    PresentationTokenDimension::InputCacheRead,
    PresentationTokenDimension::InputCacheWrite,
    PresentationTokenDimension::OutputTotal,
    PresentationTokenDimension::OutputCacheRead,
    PresentationTokenDimension::OutputCacheWrite,
];

impl PresentationTokenDimension {
    fn row_label(self) -> &'static str {
        match self {
            Self::Total => "total tokens",
            Self::InputTotal => "input tokens (incl. cache)",
            Self::InputCacheRead => "input cache read",
            Self::InputCacheWrite => "input cache write",
            Self::OutputTotal => "output tokens (incl. cache)",
            Self::OutputCacheRead => "output cache read",
            Self::OutputCacheWrite => "output cache write",
        }
    }

    fn column_heading(self) -> &'static str {
        match self {
            Self::Total => "Total",
            Self::InputTotal => "Input (incl. cache)",
            Self::InputCacheRead => "Input cache read",
            Self::InputCacheWrite => "Input cache write",
            Self::OutputTotal => "Output (incl. cache)",
            Self::OutputCacheRead => "Output cache read",
            Self::OutputCacheWrite => "Output cache write",
        }
    }

    fn summary(self, accounting: &rhei_tui::AccountingRunSummary) -> &rhei_tui::DimensionSummary {
        match self {
            Self::Total => &accounting.total,
            Self::InputTotal => &accounting.input_total,
            Self::InputCacheRead => &accounting.input_cached_read,
            Self::InputCacheWrite => &accounting.input_cache_write,
            Self::OutputTotal => &accounting.output_total,
            Self::OutputCacheRead => &accounting.output_cached_read,
            Self::OutputCacheWrite => &accounting.output_cache_write,
        }
    }
}

/// The seven formatted token dimensions, projected without arithmetic from
/// normalized inclusive totals and their cache parts.
/// §FS-rhei-cost-accounting.3.1 §FS-rhei-run-report.2.1 §FS-rhei-summary.2.3
struct AccountingTokenPresentation {
    values: [String; ACCOUNTING_TOKEN_DIMENSIONS.len()],
}

impl AccountingTokenPresentation {
    fn new(accounting: &rhei_tui::AccountingRunSummary) -> Self {
        Self {
            values: ACCOUNTING_TOKEN_DIMENSIONS
                .map(|dimension| format_dimension_value(dimension.summary(accounting))),
        }
    }

    fn rows(&self) -> impl Iterator<Item = (&'static str, &str)> {
        ACCOUNTING_TOKEN_DIMENSIONS
            .iter()
            .copied()
            .map(move |dimension| (dimension.row_label(), self.value(dimension)))
    }

    fn values(&self) -> impl Iterator<Item = &str> {
        self.values.iter().map(String::as_str)
    }

    fn value(&self, dimension: PresentationTokenDimension) -> &str {
        &self.values[dimension as usize]
    }
}
