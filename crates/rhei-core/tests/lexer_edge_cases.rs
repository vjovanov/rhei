use rhei_core::{tokenize, Token};
use rhei_core::ast::TaskId;

#[test]
fn ignores_structure_inside_fenced_code_blocks_and_unescapes_state_outside() {
    let input = r#"# Saga: Example

## Tasks

### Task 1: Title
**State:** pending

```
### Task 999: Should Not Tokenize
**State:** in\ progress
```

**Prior:** Task 1, Task 2
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::SagaHeader,
        Token::TasksSection,
        Token::TaskHeader { id: TaskId::Number(1) },
        Token::MetadataState {
            state: "pending".to_string(),
        },
        // Fence start
        Token::TextContent,
        // Lines inside code block should not be tokenized structurally
        Token::TextContent,
        Token::TextContent,
        // Fence end
        Token::TextContent,
        // After code block, metadata should be recognized again
        Token::MetadataPrior {
            task_ids: vec![TaskId::Number(1), TaskId::Number(2)],
        },
    ];

    assert_eq!(tokens, expected);
}

#[test]
fn state_metadata_unescapes_backslash_sequences() {
    // Ensure \  -> space and \\ -> \ are handled
    let input = "\
**State:** in\\ progress
**State:** path\\\\to\\\\file
";

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::MetadataState {
            state: "in progress".to_string(),
        },
        Token::MetadataState {
            state: "path\\to\\file".to_string(),
        },
    ];

    assert_eq!(tokens, expected);
}
