// Native agent session identity and invocation timing captured beside usage.

fn accounting_duration_ms(
    started_at: std::time::SystemTime,
    ended_at: std::time::SystemTime,
) -> u64 {
    u64::try_from(ended_at.duration_since(started_at).unwrap_or_default().as_millis())
        .unwrap_or(u64::MAX)
}

fn capture_cli_session_from_output(capture: &AgentUsageCapture, line: &str) {
    // This state is independent of the usage JSONL because cumulative Claude
    // stream results replace that file. §FS-rhei-cost-accounting.3.4
    if let Some(session) = extract_cli_session_from_output_line(capture.extractor, line) {
        if let Ok(mut observed) = capture.cli_session.lock() {
            *observed = Some(session);
        }
    }
}

fn extract_cli_session_from_output_line(
    extractor: AgentUsageExtractor,
    line: &str,
) -> Option<AccountingCliSession> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let object = value.as_object()?;
    let id = match extractor {
        AgentUsageExtractor::Claude
            if object.get("type").and_then(serde_json::Value::as_str) == Some("result") =>
        {
            object.get("session_id").and_then(serde_json::Value::as_str)
        }
        AgentUsageExtractor::Codex
            if object.get("type").and_then(serde_json::Value::as_str)
                == Some("thread.started") =>
        {
            object.get("thread_id").and_then(serde_json::Value::as_str)
        }
        AgentUsageExtractor::Pi
            if object.get("type").and_then(serde_json::Value::as_str) == Some("session") =>
        {
            object.get("id").and_then(serde_json::Value::as_str)
        }
        _ => None,
    }?;
    if id.is_empty() {
        return None;
    }
    Some(AccountingCliSession { id: id.to_string(), store_path: None })
}
