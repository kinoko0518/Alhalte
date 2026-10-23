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

/// Returns self-introduction metadata `(id, name, icon_svg, description)` for Alhalte DSL.
///
/// The id is the crate's package name, which must match the `Restaurant.toml`
/// extension id (ABI 2, Issue #274).
pub fn self_introduction_info() -> (&'static str, &'static str, &'static str, &'static str) {
    (
        env!("CARGO_PKG_NAME"),
        "Alhalte DSL",
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM7.5 18c-.83 0-1.5-.67-1.5-1.5S6.67 15 7.5 15s1.5.67 1.5 1.5S8.33 18 7.5 18zm0-9C6.67 9 6 8.33 6 7.5S6.67 6 7.5 6 9 6.67 9 7.5 8.33 9 7.5 9zm4.5 4.5c-.83 0-1.5-.67-1.5-1.5s.67-1.5 1.5-1.5 1.5.67 1.5 1.5-.67 1.5-1.5 1.5zm4.5 4.5c-.83 0-1.5-.67-1.5-1.5s.67-1.5 1.5-1.5 1.5.67 1.5 1.5-.67 1.5-1.5 1.5zm0-9c-.83 0-1.5-.67-1.5-1.5S15.67 6 16.5 6s1.5.67 1.5 1.5-.67 1.5-1.5 1.5z"/></svg>"#,
        "ダイスの記法などを処理します。ほとんどのルール拡張の動作のために必要です。",
    )
}
