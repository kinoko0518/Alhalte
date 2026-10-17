//! `markdown` extension for parsing Markdown notations into Generast AST.
//!
//! Handles headings (`#` to `####`), bold (`**...**`), italic (`*...*`),
//! spoilers (`||...||`), text color (`[text]{color:#hex}`), and pill background colors (`[text]{pill:#hex}`).

#![warn(missing_docs)]

pub mod abi;
pub mod error;
pub mod parser;

pub use error::MarkdownError;
pub use parser::{parse_inlines, parse_markdown};

use generast::Generast;

/// Processes an incoming [`Generast`] syntax tree, interpreting Markdown formatting.
pub fn process_ast(ast: Generast, _speaker_id: u64) -> Generast {
    match ast {
        Generast::String(text) => match parse_markdown(&text) {
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
