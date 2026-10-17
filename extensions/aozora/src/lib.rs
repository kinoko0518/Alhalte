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

/// Returns self-introduction metadata `(name, icon_svg, description)` for Aozora Bunko.
pub fn self_introduction_info() -> (&'static str, &'static str, &'static str) {
    (
        "青空文庫ルビ・傍点",
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M18 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zM6 4h5v8l-2.5-1.5L6 12V4z"/></svg>"#,
        "青空文庫形式のルビ（｜親文字《ルビ》）と傍点（［＃「…」に傍点］）を変換する公式拡張機能",
    )
}
