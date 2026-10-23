//! `coc-7` extension for parsing Call of Cthulhu 7th edition judgments.
//!
//! Generates [`JudgementData`] from dice roll elements followed by skill notations `【……】`,
//! with support for bonus and penalty dice.

#![warn(missing_docs)]

pub mod abi;
pub mod error;
pub mod parser;

pub use error::Coc7Error;
pub use parser::{
    derive_seed, judge_coc7, parse_bracket_subject, parse_coc7_string,
    try_combine_diceroll_and_suffix,
};

use generast::Generast;

/// Processes an incoming [`Generast`] syntax tree, resolving CoC 7th edition judgments.
pub fn process_ast(ast: Generast, speaker_id: u64) -> Generast {
    match ast {
        Generast::String(text) => match parse_coc7_string(&text, speaker_id) {
            Ok(parsed) => parsed,
            Err(e) => Generast::syntax_error(e.to_string()),
        },
        Generast::Nested(mut nested) => {
            let mut new_contents = Vec::new();
            let mut i = 0;
            while i < nested.contents.len() {
                if i + 1 < nested.contents.len() {
                    let current = &nested.contents[i];
                    let next = &nested.contents[i + 1];
                    if let (Generast::Element(_), Generast::String(suffix)) = (current, next)
                        && let Some((judge_node, remaining)) =
                            try_combine_diceroll_and_suffix(current, suffix)
                    {
                        new_contents.push(judge_node);
                        if !remaining.is_empty() {
                            new_contents.push(Generast::string(remaining));
                        }
                        i += 2;
                        continue;
                    }
                }
                new_contents.push(process_ast(nested.contents[i].clone(), speaker_id));
                i += 1;
            }
            nested.contents = new_contents;
            Generast::Nested(nested)
        }
        Generast::Element(e) => Generast::Element(e),
    }
}

/// Returns self-introduction metadata `(id, name, icon_svg, description)` for CoC 7th edition.
///
/// The id is the crate's package name, which must match the `Restaurant.toml`
/// extension id (ABI 2, Issue #274).
pub fn self_introduction_info() -> (&'static str, &'static str, &'static str, &'static str) {
    (
        env!("CARGO_PKG_NAME"),
        "新クトゥルフ神話TRPG (第7版)",
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M12 2L4.5 6.5v11L12 22l7.5-4.5v-11L12 2zm0 2.3l5.5 3.3v8.8L12 19.7l-5.5-3.3V7.6L12 4.3zM9 9v2h3.5L9 16h2.2l3.5-5V9H9z"/></svg>"#,
        "新クトゥルフ神話TRPG第7版の記法や処理のための公式拡張機能です。",
    )
}
