use rhei_core::{tokenize, Token};
use rhei_core::ast::TaskId;

#[test]
fn tokenizes_basic_structure() {
    let input = r#"# Saga: User Authentication System

## Tasks

### Task 1: Database Schema Design
**State:** completed

#### Subtask 1.1: Define User Table
Some description line

**Prior:** Task 1, Task 2
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::SagaHeader,
        Token::TasksSection,
        Token::TaskHeader { id: TaskId::Number(1) },
        Token::MetadataState {
            state: "completed".to_string(),
        },
        Token::SubtaskHeader {
            task_number: 1,
            subtask_number: 1,
        },
        Token::TextContent,
        Token::MetadataPrior {
            task_ids: vec![TaskId::Number(1), TaskId::Number(2)],
        },
    ];

    assert_eq!(tokens, expected);
}
