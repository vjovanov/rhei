use rhei_core::ast::TaskId;
use rhei_core::{Token, tokenize};

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
        Token::TaskHeader {
            id: TaskId::Number(1),
        },
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

#[test]
fn tokenizes_named_task_ids_and_prior_references() {
    let input = r#"# Saga: Named Tasks

## Tasks

### Task bootstrap_env: Bootstrap environments
**State:** in-progress
**Prior:** Task seed_data, Task 2

### Task seed_data: Seed database
**State:** pending
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::SagaHeader,
        Token::TasksSection,
        Token::TaskHeader {
            id: TaskId::Named("bootstrap_env".to_string()),
        },
        Token::MetadataState {
            state: "in-progress".to_string(),
        },
        Token::MetadataPrior {
            task_ids: vec![
                TaskId::Named("seed_data".to_string()),
                TaskId::Number(2),
            ],
        },
        Token::TaskHeader {
            id: TaskId::Named("seed_data".to_string()),
        },
        Token::MetadataState {
            state: "pending".to_string(),
        },
    ];

    assert_eq!(tokens, expected);
}
