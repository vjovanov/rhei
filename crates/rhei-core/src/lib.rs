pub mod tokens;
pub mod lexer;
pub mod ast;
pub mod parser;

pub use tokens::Token;
pub use lexer::{Tokenizer, tokenize};
pub use parser::parse;

pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn help_text() -> String {
    "Rhei - Markdown plan compiler scaffold\n\nUsage:\n  rhei [OPTIONS]\n\nFor now, use --help and --version."
        .to_string()
}
