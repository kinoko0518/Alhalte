//! `alhalte-dsl` extension for parsing dice and character DSL expressions.
//!
//! Handles BCDice-compatible flexible dice rolling expressions, status modifications,
//! status assignments, character mentions, and syntax error detection.

#![warn(missing_docs)]

pub mod abi;
pub mod error;
pub mod parser;

pub use error::DslError;
pub use parser::{derive_seed, parse_dsl};

use generast::Generast;

/// Processes an incoming [`Generast`] syntax tree, transforming DSL notations.
pub fn process_ast(ast: Generast, speaker_id: u64) -> Generast {
    match ast {
        Generast::String(text) => match parse_dsl(&text, speaker_id) {
            Ok(parsed) => parsed,
            Err(e) => Generast::syntax_error(e.to_string()),
        },
        Generast::Nested(mut nested) => {
            nested.contents = nested
                .contents
                .into_iter()
                .map(|child| process_ast(child, speaker_id))
                .collect();
            Generast::Nested(nested)
        }
        Generast::Element(e) => Generast::Element(e),
    }
}
