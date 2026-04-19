use rhei_core::ast::TaskId;
use rhei_core::{tokenize, Token};

#[test]
fn malformed_structure_near_misses_fall_back_to_text_tokens() {
    let input = r#"#Rhei: Missing Space
##Tasks
###Task 1: Missing Heading Space
### task 1: Wrong Case
#### Subtask 1: Missing Decimal Component
**State** pending
**Prior** Task 1
### Task -bad: Invalid Named Id Boundary
### Task _bad: Invalid Named Id Boundary
### Task 1bad: Invalid Numeric Boundary
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    assert_eq!(
        tokens,
        vec![
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
        ]
    );
}

#[test]
fn distinguishes_valid_named_task_ids_from_invalid_boundaries() {
    let input = r#"
### Task build-1_ok: Valid
### Task build--stage: Also Valid
### Task -build: Invalid Leading Hyphen
### Task 1build: Invalid Numeric Prefix
### Task build$: Invalid Trailing Character
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    assert_eq!(
        tokens,
        vec![
            Token::TaskHeader { id: TaskId::Named("build-1_ok".to_string()) },
            Token::TaskHeader { id: TaskId::Named("build--stage".to_string()) },
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
        ]
    );
}

#[test]
fn malformed_structure_inside_fenced_code_blocks_is_not_tokenized() {
    let input = r#"# Rhei: Example

## Tasks

### Task 1: Title
**State:** pending

```
# Rhei: Hidden Rhei
## Tasks
### Task 999: Should Not Tokenize
### task 2: Wrong Case
#### Subtask 1: Missing Decimal Component
**State** code-block near-miss
**Prior** Task 2
```

#Rhei: Missing Space Outside Fence
##Tasks
###Task 2: Missing Heading Space Outside Fence
**State** pending outside fence
**Prior** Task 3 outside fence
**Prior:** Task 1, Task 2
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::RheiHeader,
        Token::TasksSection,
        Token::TaskHeader { id: TaskId::Number(1) },
        Token::MetadataState { state: "pending".to_string() },
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::MetadataPrior { task_ids: vec![TaskId::Number(1), TaskId::Number(2)] },
    ];

    assert_eq!(tokens, expected);
}

#[test]
fn state_metadata_backtick_escaping() {
    let input = "\
**State:** `in progress`
**State:** `ready to ship`
";

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::MetadataState { state: "in progress".to_string() },
        Token::MetadataState { state: "ready to ship".to_string() },
    ];

    assert_eq!(tokens, expected);
}
