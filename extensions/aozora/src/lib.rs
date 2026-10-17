//! `aozora` extension for parsing Aozora Bunko format into Generast AST.
//!
//! Converts ruby annotations, emphasis dots, headings, indents, hanging indents,
//! grounded lines, kerning, horizontal-in-vertical, bold, and italic into typed Generast structures.

#![warn(missing_docs)]

pub mod abi;
pub mod error;
pub mod parser;

pub use error::AozoraError;
pub use parser::parse_aozora;

use generast::Generast;

/// Processes an incoming [`Generast`] syntax tree, transforming Aozora Bunko notations.
pub fn process_ast(ast: Generast, _speaker_id: u64) -> Generast {
    match ast {
        Generast::String(text) => match parse_aozora(&text) {
            Ok(parsed) => parsed,
            Err(e) => Generast::syntax_error(e.to_string()),
        },
        Generast::Nested(mut nested) => {
            nested.contents = nested
                .contents
                .into_iter()
                .map(|child| process_ast(child, _speaker_id))
                .collect();
            Generast::Nested(nested)
        }
        Generast::Element(e) => Generast::Element(e),
    }
}
