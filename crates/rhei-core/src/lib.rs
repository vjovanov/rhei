//! Core parsing types and entry points for the Rhei markdown plan compiler.
//!
//! This crate owns the syntax-facing pieces of the compiler:
//! - token definitions and a tokenizer in [`crate::tokens`] and [`crate::lexer`]
//! - AST types in [`crate::ast`]
//! - the markdown plan parser in [`parse`](crate::parse)
//!
//! Most consumers only need [`parse`](crate::parse) plus the public AST types
//! from [`crate::ast`].

pub mod tokens;
pub mod lexer;
pub mod ast;
pub mod parser;

pub use tokens::Token;
pub use lexer::{Tokenizer, tokenize};
pub use parser::parse;

/// Returns the crate version reported by Cargo metadata.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Returns a short human-readable help string for compatibility surfaces.
///
/// The main command-line experience lives in the [`rhei-cli`](../../rhei-cli/src/main.rs)
/// binary crate.
pub fn help_text() -> String {
    "Rhei - Markdown plan compiler scaffold\n\nUsage:\n  rhei [OPTIONS]\n\nFor now, use --help and --version."
        .to_string()
}
