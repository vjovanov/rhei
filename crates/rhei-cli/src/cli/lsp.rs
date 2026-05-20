const LSP_TEXT_SYNC_FULL: u8 = 1;
const LSP_ERROR: u8 = 1;
const LSP_WARNING: u8 = 2;

struct LspServerState {
    state_machine_path: Option<PathBuf>,
    documents: HashMap<String, String>,
    should_exit: bool,
}

impl LspServerState {
    fn new(state_machine_path: Option<PathBuf>) -> Self {
        Self {
            state_machine_path,
            documents: HashMap::new(),
            should_exit: false,
        }
    }

    fn handle_message(&mut self, message: serde_json::Value) -> Vec<serde_json::Value> {
        if message.get("method").is_none() {
            return Vec::new();
        }

        let method = message.get("method").and_then(serde_json::Value::as_str).unwrap_or("");
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(serde_json::Value::Null);

        match (id, method) {
            (Some(id), "initialize") => vec![lsp_response(id, lsp_initialize_result())],
            (Some(id), "shutdown") => vec![lsp_response(id, serde_json::Value::Null)],
            (Some(id), "textDocument/documentSymbol") => {
                vec![lsp_response(id, self.document_symbol_result(&params))]
            }
            (Some(id), "textDocument/completion") => {
                vec![lsp_response(id, self.completion_result(&params))]
            }
            (Some(id), "textDocument/hover") => vec![lsp_response(id, self.hover_result(&params))],
            (Some(id), "textDocument/definition") => {
                vec![lsp_response(id, self.definition_result(&params))]
            }
            (Some(id), _) => vec![lsp_error_response(id, -32601, "method not found")],
            (None, "initialized") => Vec::new(),
            (None, "exit") => {
                self.should_exit = true;
                Vec::new()
            }
            (None, "textDocument/didOpen") => self.did_open(&params),
            (None, "textDocument/didChange") => self.did_change(&params),
            (None, "textDocument/didSave") => self.did_save(&params),
            (None, "textDocument/didClose") => self.did_close(&params),
            (None, "$/cancelRequest") => Vec::new(),
            (None, _) => Vec::new(),
        }
    }

    fn did_open(&mut self, params: &serde_json::Value) -> Vec<serde_json::Value> {
        let Some(uri) = params
            .pointer("/textDocument/uri")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return Vec::new();
        };
        let text = params
            .pointer("/textDocument/text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        self.documents.insert(uri.clone(), text);
        vec![self.publish_diagnostics(&uri)]
    }

    fn did_change(&mut self, params: &serde_json::Value) -> Vec<serde_json::Value> {
        let Some(uri) = params
            .pointer("/textDocument/uri")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return Vec::new();
        };
        let Some(text) = params
            .pointer("/contentChanges/0/text")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return Vec::new();
        };
        self.documents.insert(uri.clone(), text);
        vec![self.publish_diagnostics(&uri)]
    }

    fn did_save(&mut self, params: &serde_json::Value) -> Vec<serde_json::Value> {
        let Some(uri) = params
            .pointer("/textDocument/uri")
            .and_then(serde_json::Value::as_str)
        else {
            return Vec::new();
        };
        vec![self.publish_diagnostics(uri)]
    }

    fn did_close(&mut self, params: &serde_json::Value) -> Vec<serde_json::Value> {
        let Some(uri) = params
            .pointer("/textDocument/uri")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return Vec::new();
        };
        self.documents.remove(&uri);
        vec![lsp_publish_diagnostics(&uri, Vec::new())]
    }

    fn publish_diagnostics(&self, uri: &str) -> serde_json::Value {
        let text = self.documents.get(uri).map(String::as_str).unwrap_or("");
        let diagnostics = self.diagnostics_for_document(uri, text);
        lsp_publish_diagnostics(uri, diagnostics)
    }

    fn diagnostics_for_document(
        &self,
        uri: &str,
        text: &str,
    ) -> Vec<serde_json::Value> {
        let path = lsp_uri_to_path(uri);
        let (maybe_rhei, parse_errors) = rhei_core::parser::parse_collect(text);

        if !parse_errors.is_empty() {
            return parse_errors
                .iter()
                .map(|err| {
                    lsp_diagnostic(
                        text,
                        err.line,
                        LSP_ERROR,
                        err.message.as_str(),
                    )
                })
                .collect();
        }

        let Some(rhei) = maybe_rhei else {
            return vec![lsp_diagnostic(text, None, LSP_ERROR, "document is not a Rhei plan")];
        };

        // §FS-rhei-lsp.3: Diagnostics reuse parser output and semantic validation.
        let input_path = path.as_deref().unwrap_or_else(|| Path::new("."));
        let loaded = LoadedPlan { rhei, task_sources: HashMap::new() };
        let resolved = match resolve_state_machine_for_loaded_plan(
            input_path,
            &loaded,
            self.state_machine_path.as_deref(),
        ) {
            Ok(resolved) => resolved,
            Err(err) => {
                return vec![lsp_diagnostic(text, Some(1), LSP_ERROR, &err.to_string())];
            }
        };

        let base_path = path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."));
        let report =
            rhei_validator::validate_with_machine_and_base(&loaded.rhei, &resolved.machine, base_path);

        report
            .errors
            .iter()
            .map(|message| lsp_diagnostic(text, None, LSP_ERROR, message))
            .chain(
                report
                    .warnings
                    .iter()
                    .map(|message| lsp_diagnostic(text, None, LSP_WARNING, message)),
            )
            .collect()
    }

    fn document_symbol_result(&self, params: &serde_json::Value) -> serde_json::Value {
        let Some(uri) = lsp_param_uri(params) else {
            return serde_json::json!([]);
        };
        let Some(text) = self.documents.get(uri) else {
            return serde_json::json!([]);
        };
        serde_json::Value::Array(lsp_document_symbols(text))
    }

    fn completion_result(&self, params: &serde_json::Value) -> serde_json::Value {
        let Some(uri) = lsp_param_uri(params) else {
            return serde_json::json!({"isIncomplete": false, "items": []});
        };
        let Some(text) = self.documents.get(uri) else {
            return serde_json::json!({"isIncomplete": false, "items": []});
        };
        let position = lsp_param_position(params);
        let prefix = position
            .and_then(|(line, character)| lsp_line_prefix(text, line, character))
            .unwrap_or_default();
        let path = lsp_uri_to_path(uri);

        let items = if lsp_state_completion_context(&prefix) {
            // §FS-rhei-lsp.4: State completions come from the resolved state machine.
            match self.resolve_machine_for_text(text, path.as_deref()) {
                Ok(machine) => machine
                    .allowed_states()
                    .map(|state| {
                        serde_json::json!({
                            "label": state,
                            "kind": 14,
                            "detail": "Rhei state",
                        })
                    })
                    .collect(),
                Err(_) => Vec::new(),
            }
        } else if lsp_prior_completion_context(&prefix) {
            lsp_scan_tasks(text)
                .into_iter()
                .map(|task| {
                    serde_json::json!({
                        "label": task.id,
                        "kind": 18,
                        "detail": task.title,
                    })
                })
                .collect()
        } else if lsp_heading_completion_context(&prefix) {
            vec![serde_json::json!({
                "label": "Task",
                "kind": 14,
                "detail": "Rhei node kind",
            })]
        } else {
            Vec::new()
        };

        serde_json::json!({
            "isIncomplete": false,
            "items": items,
        })
    }

    fn hover_result(&self, params: &serde_json::Value) -> serde_json::Value {
        let Some(uri) = lsp_param_uri(params) else {
            return serde_json::Value::Null;
        };
        let Some(text) = self.documents.get(uri) else {
            return serde_json::Value::Null;
        };
        let Some((line, character)) = lsp_param_position(params) else {
            return serde_json::Value::Null;
        };
        let Some(word) = lsp_word_at_position(text, line, character) else {
            return serde_json::Value::Null;
        };
        let path = lsp_uri_to_path(uri);
        let Ok(machine) = self.resolve_machine_for_text(text, path.as_deref()) else {
            return serde_json::Value::Null;
        };
        let Some(state) = machine.states.get(&word.text) else {
            return serde_json::Value::Null;
        };

        let mut lines = vec![format!("**{}**", word.text)];
        if let Some(description) = &state.description {
            lines.push(String::new());
            lines.push(description.clone());
        }
        lines.push(String::new());
        lines.push(format!("Terminal: {}", if state.terminal { "yes" } else { "no" }));
        lines.push(format!("Gating: {}", if state.gating { "yes" } else { "no" }));

        serde_json::json!({
            "contents": {
                "kind": "markdown",
                "value": lines.join("\n"),
            },
            "range": lsp_range(line, word.start, line, word.end),
        })
    }

    fn definition_result(&self, params: &serde_json::Value) -> serde_json::Value {
        let Some(uri) = lsp_param_uri(params) else {
            return serde_json::Value::Null;
        };
        let Some(text) = self.documents.get(uri) else {
            return serde_json::Value::Null;
        };
        let Some((line, character)) = lsp_param_position(params) else {
            return serde_json::Value::Null;
        };
        let Some(word) = lsp_word_at_position(text, line, character) else {
            return serde_json::Value::Null;
        };
        let Some(task) = lsp_scan_tasks(text).into_iter().find(|task| task.id == word.text) else {
            return serde_json::Value::Null;
        };
        serde_json::json!({
            "uri": uri,
            "range": lsp_range(task.line, 0, task.line, task.line_end),
        })
    }

    fn resolve_machine_for_text(
        &self,
        text: &str,
        path: Option<&Path>,
    ) -> Result<rhei_validator::StateMachine, String> {
        let (maybe_rhei, parse_errors) = rhei_core::parser::parse_collect(text);
        let Some(rhei) = maybe_rhei else {
            if !parse_errors.is_empty() {
                return load_state_machine(self.state_machine_path.as_deref())
                    .map_err(|err| err.to_string());
            }
            return Err("document is not a Rhei plan".to_string());
        };
        let input_path = path.unwrap_or_else(|| Path::new("."));
        let loaded = LoadedPlan { rhei, task_sources: HashMap::new() };
        resolve_state_machine_for_loaded_plan(
            input_path,
            &loaded,
            self.state_machine_path.as_deref(),
        )
        .map(|resolved| resolved.machine)
        .map_err(|err| err.to_string())
    }
}

/// Start the stdio language server.
fn lsp_command(state_machine: Option<&Path>) -> MietteResult<()> {
    let mut server = LspServerState::new(state_machine.map(Path::to_path_buf));
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    while let Some(body) = lsp_read_message(&mut reader)? {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&body) else {
            continue;
        };
        for response in server.handle_message(message) {
            lsp_write_message(&mut writer, &response)?;
        }
        if server.should_exit {
            break;
        }
    }

    Ok(())
}

fn lsp_read_message<R: BufRead>(reader: &mut R) -> MietteResult<Option<String>> {
    let mut content_length = None;
    let mut saw_header = false;

    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|err| miette!("failed to read LSP header: {err}"))?;
        if read == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        saw_header = true;
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let Some(length) = content_length else {
        if saw_header {
            return Err(miette!("LSP message missing Content-Length header"));
        }
        return Ok(None);
    };

    let mut body = vec![0_u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|err| miette!("failed to read LSP body: {err}"))?;
    String::from_utf8(body)
        .map(Some)
        .map_err(|err| miette!("LSP body is not valid UTF-8: {err}"))
}

fn lsp_write_message<W: Write>(writer: &mut W, message: &serde_json::Value) -> MietteResult<()> {
    let body =
        serde_json::to_string(message).map_err(|err| miette!("failed to serialize LSP: {err}"))?;
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)
        .and_then(|_| writer.flush())
        .map_err(|err| miette!("failed to write LSP message: {err}"))
}

fn lsp_initialize_result() -> serde_json::Value {
    serde_json::json!({
        "capabilities": {
            "textDocumentSync": LSP_TEXT_SYNC_FULL,
            "completionProvider": {
                "triggerCharacters": [":", " ", "[", ","],
            },
            "documentSymbolProvider": true,
            "hoverProvider": true,
            "definitionProvider": true,
        },
        "serverInfo": {
            "name": "rhei-lsp",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

fn lsp_response(id: serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn lsp_error_response(id: serde_json::Value, code: i64, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        },
    })
}

fn lsp_publish_diagnostics(uri: &str, diagnostics: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": diagnostics,
        },
    })
}

fn lsp_diagnostic(
    text: &str,
    one_based_line: Option<usize>,
    severity: u8,
    message: &str,
) -> serde_json::Value {
    let line = one_based_line.unwrap_or(1).saturating_sub(1);
    serde_json::json!({
        "range": lsp_line_range(text, line),
        "severity": severity,
        "source": "rhei",
        "message": message,
    })
}

fn lsp_document_symbols(text: &str) -> Vec<serde_json::Value> {
    let mut symbols = Vec::new();
    for (line, raw) in text.lines().enumerate() {
        let trimmed = raw.trim_start();
        if let Some(title) = trimmed.strip_prefix("# Rhei:") {
            symbols.push(serde_json::json!({
                "name": title.trim(),
                "kind": 1,
                "range": lsp_line_range(text, line),
                "selectionRange": lsp_line_range(text, line),
            }));
            continue;
        }
        if let Some(task) = lsp_parse_task_heading(raw, line) {
            symbols.push(serde_json::json!({
                "name": format!("{} {}: {}", task.kind, task.id, task.title),
                "kind": 19,
                "range": lsp_range(task.line, 0, task.line, task.line_end),
                "selectionRange": lsp_range(task.line, task.selection_start, task.line, task.line_end),
            }));
        }
    }
    symbols
}

#[derive(Clone)]
struct LspTaskLocation {
    id: String,
    title: String,
    kind: String,
    line: usize,
    line_end: usize,
    selection_start: usize,
}

fn lsp_scan_tasks(text: &str) -> Vec<LspTaskLocation> {
    text.lines()
        .enumerate()
        .filter_map(|(line, raw)| lsp_parse_task_heading(raw, line))
        .collect()
}

fn lsp_parse_task_heading(raw: &str, line: usize) -> Option<LspTaskLocation> {
    let heading = Regex::new(r"^(#{3,6})\s+([A-Za-z][A-Za-z0-9_-]*)\s+([A-Za-z0-9_.-]+):\s*(.+)$")
        .ok()?;
    let caps = heading.captures(raw)?;
    let hashes = caps.get(1)?;
    let kind = caps.get(2)?.as_str().to_string();
    let id = caps.get(3)?.as_str().to_string();
    let title = caps.get(4)?.as_str().trim().to_string();
    Some(LspTaskLocation {
        id,
        title,
        kind,
        line,
        line_end: lsp_utf16_len(raw),
        selection_start: lsp_utf16_len(&raw[..hashes.end()]),
    })
}

fn lsp_state_completion_context(prefix: &str) -> bool {
    prefix.contains("**State:**")
}

fn lsp_prior_completion_context(prefix: &str) -> bool {
    prefix.contains("**Prior:**")
}

fn lsp_heading_completion_context(prefix: &str) -> bool {
    let trimmed = prefix.trim_start();
    trimmed.starts_with("###") && !trimmed.contains(':')
}

fn lsp_param_uri(params: &serde_json::Value) -> Option<&str> {
    params
        .pointer("/textDocument/uri")
        .and_then(serde_json::Value::as_str)
}

fn lsp_param_position(params: &serde_json::Value) -> Option<(usize, usize)> {
    let position = params.pointer("/position")?;
    let line = position.get("line")?.as_u64()? as usize;
    let character = position.get("character")?.as_u64()? as usize;
    Some((line, character))
}

fn lsp_line_prefix(text: &str, line: usize, character: usize) -> Option<String> {
    let raw = text.lines().nth(line)?;
    Some(lsp_prefix_utf16(raw, character))
}

#[derive(Debug, PartialEq, Eq)]
struct LspWord {
    text: String,
    start: usize,
    end: usize,
}

fn lsp_word_at_position(text: &str, line: usize, character: usize) -> Option<LspWord> {
    let raw = text.lines().nth(line)?;
    let byte_index = lsp_utf16_to_byte_index(raw, character);
    let bytes = raw.as_bytes();
    if byte_index > bytes.len() {
        return None;
    }
    let mut start = byte_index;
    while start > 0 && lsp_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = byte_index;
    while end < bytes.len() && lsp_word_byte(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some(LspWord {
        text: raw[start..end].to_string(),
        start: lsp_utf16_len(&raw[..start]),
        end: lsp_utf16_len(&raw[..end]),
    })
}

fn lsp_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

fn lsp_line_range(text: &str, line: usize) -> serde_json::Value {
    let end = text.lines().nth(line).map(lsp_utf16_len).unwrap_or(0);
    lsp_range(line, 0, line, end)
}

fn lsp_range(
    start_line: usize,
    start_character: usize,
    end_line: usize,
    end_character: usize,
) -> serde_json::Value {
    serde_json::json!({
        "start": { "line": start_line, "character": start_character },
        "end": { "line": end_line, "character": end_character },
    })
}

fn lsp_utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn lsp_prefix_utf16(raw: &str, character: usize) -> String {
    let byte_index = lsp_utf16_to_byte_index(raw, character);
    raw[..byte_index].to_string()
}

fn lsp_utf16_to_byte_index(raw: &str, character: usize) -> usize {
    let mut utf16 = 0;
    for (byte_index, ch) in raw.char_indices() {
        if utf16 >= character {
            return byte_index;
        }
        utf16 += ch.len_utf16();
    }
    raw.len()
}

fn lsp_uri_to_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://")?;
    Some(PathBuf::from(lsp_percent_decode(raw)))
}

fn lsp_percent_decode(raw: &str) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (lsp_hex(bytes[i + 1]), lsp_hex(bytes[i + 2])) {
                out.push(high * 16 + low);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into())
}

fn lsp_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
