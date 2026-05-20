    #[test]
    fn parses_lsp_command() {
        let cli = Cli::try_parse_from(["rhei", "lsp"]).expect("cli should parse");
        assert!(cli.state_machine.is_none());
        assert!(matches!(cli.command, Commands::Lsp));
    }

    #[test]
    fn lsp_initialize_advertises_editor_features() {
        let result = lsp_initialize_result();
        assert_eq!(result.pointer("/capabilities/textDocumentSync").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(result.pointer("/capabilities/documentSymbolProvider"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(result.pointer("/capabilities/hoverProvider"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(result.pointer("/capabilities/definitionProvider"), Some(&serde_json::Value::Bool(true)));
    }

    #[test]
    fn lsp_publishes_parse_diagnostics_for_open_document() {
        let mut server = LspServerState::new(None);
        let uri = "file:///tmp/broken.rhei.md";
        let open = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "text": "# Rhei: Broken\n\n### Nope\n",
                }
            }
        });

        let messages = server.handle_message(open);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].pointer("/method").and_then(|v| v.as_str()), Some("textDocument/publishDiagnostics"));
        let diagnostics = messages[0].pointer("/params/diagnostics").and_then(|v| v.as_array()).unwrap();
        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics[0].pointer("/severity").and_then(|v| v.as_u64()), Some(1));
    }

    #[test]
    fn lsp_reads_and_writes_content_length_messages() {
        let body = r#"{"jsonrpc":"2.0","method":"exit"}"#;
        let input = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let mut reader = BufReader::new(input.as_bytes());
        assert_eq!(lsp_read_message(&mut reader).unwrap(), Some(body.to_string()));

        let mut output = Vec::new();
        lsp_write_message(
            &mut output,
            &serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": null}),
        )
        .unwrap();
        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.starts_with("Content-Length: "));
        assert!(rendered.contains("\r\n\r\n{\"id\":1,\"jsonrpc\":\"2.0\",\"result\":null}"));
    }

    #[test]
    fn lsp_completes_states_and_prior_task_ids() {
        let mut server = LspServerState::new(None);
        let uri = "file:///tmp/plan.rhei.md";
        let text = "# Rhei: Demo\n\n**States:** rhei\n\n## Tasks\n\n### Task 1: First\n**State:** pending\n\n### Task 2: Second\n**State:** \n**Prior:** ";
        server.documents.insert(uri.to_string(), text.to_string());

        let state_completion = server.completion_result(&serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": 10, "character": 11 }
        }));
        let state_items = state_completion.pointer("/items").and_then(|v| v.as_array()).unwrap();
        assert!(state_items.iter().any(|item| item.pointer("/label").and_then(|v| v.as_str()) == Some("pending")));

        let prior_completion = server.completion_result(&serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": 11, "character": 11 }
        }));
        let prior_items = prior_completion.pointer("/items").and_then(|v| v.as_array()).unwrap();
        assert!(prior_items.iter().any(|item| item.pointer("/label").and_then(|v| v.as_str()) == Some("1")));
    }

    #[test]
    fn lsp_defines_task_ids_from_current_document() {
        let mut server = LspServerState::new(None);
        let uri = "file:///tmp/plan.rhei.md";
        let text = "# Rhei: Demo\n\n**States:** rhei\n\n## Tasks\n\n### Task 1: First\n**State:** pending\n\n### Task 2: Second\n**State:** pending\n**Prior:** 1\n";
        server.documents.insert(uri.to_string(), text.to_string());

        let definition = server.definition_result(&serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": 11, "character": 11 }
        }));
        assert_eq!(definition.pointer("/uri").and_then(|v| v.as_str()), Some(uri));
        assert_eq!(definition.pointer("/range/start/line").and_then(|v| v.as_u64()), Some(6));
    }
