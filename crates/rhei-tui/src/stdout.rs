use crate::event::{EventSink, MessageLevel, RunEvent};

/// Frontend sink for non-TTY output.
///
/// The engine still drives all human-readable stdout via direct `println!`
/// calls (so that byte-for-byte output is preserved). This sink therefore
/// only reacts to `Message` events — which never originate from the current
/// engine — and to terminal errors re-emitted through the channel. In
/// practice that means it is a near no-op today and exists so that future
/// engine refactors can route prints through the sink surface.
pub struct StdoutSink;

impl StdoutSink {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StdoutSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink for StdoutSink {
    fn emit(&self, event: RunEvent) {
        if let RunEvent::Message { level, text } = event {
            match level {
                MessageLevel::Info => println!("{text}"),
                MessageLevel::Warn | MessageLevel::Error => eprintln!("{text}"),
            }
        } else if let RunEvent::RunLink { label, url } = event {
            println!("{label}: {url}");
        } else if let RunEvent::UsageReported { task, state, usage, .. } = event {
            if let Some(cost) = usage.cost_micro.or(usage.priced_cost_micro) {
                println!(
                    "accounting: task {task} state {state} {} {}",
                    usage.agent,
                    format_cost_micro(cost, usage.currency.as_deref())
                );
            }
        }
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
