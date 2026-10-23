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

/// Returns self-introduction metadata `(id, name, icon_svg, description)` for Markdown.
///
/// The id is the crate's package name, which must match the `Restaurant.toml`
/// extension id (ABI 2, Issue #274).
pub fn self_introduction_info() -> (&'static str, &'static str, &'static str, &'static str) {
    (
        env!("CARGO_PKG_NAME"),
        "Markdown",
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M2.5 4v16h19V4H2.5zm2 2h15v12h-15V6zm2 2v8h2.5V11l2 2.5 2-2.5V16h2.5V8h-2.5l-2 2.5-2-2.5H6.5zm11 0l-2.5 3h1.5v5h2v-5h1.5l-2.5-3z"/></svg>"#,
        "太字（**）・斜体（*）・打ち消し線（~~）などのMarkdown記法を処理します。",
    )
}
