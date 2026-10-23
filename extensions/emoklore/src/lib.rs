//! `emoklore` extension for parsing Emoklore judgments.
//!
//! Generates [`JudgementData`] from `{X}DM<=Y` notations or dice roll elements
//! followed by skill notations `〈……〉`.

#![warn(missing_docs)]

pub mod abi;
pub mod error;
pub mod parser;

pub use error::EmokloreError;
pub use parser::{
    derive_seed, judge_emoklore, parse_angle_bracket_subject, parse_emoklore_string,
    try_combine_diceroll_and_suffix,
};

use generast::Generast;

/// Processes an incoming [`Generast`] syntax tree, resolving Emoklore judgments.
pub fn process_ast(ast: Generast, speaker_id: u64) -> Generast {
    match ast {
        Generast::String(text) => match parse_emoklore_string(&text, speaker_id) {
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

/// Returns self-introduction metadata `(id, name, icon_svg, description)` for Emoklore TRPG.
///
/// The id is the crate's package name, which must match the `Restaurant.toml`
/// extension id (ABI 2, Issue #274).
pub fn self_introduction_info() -> (&'static str, &'static str, &'static str, &'static str) {
    (
        env!("CARGO_PKG_NAME"),
        "エモクロアTRPG",
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/></svg>"#,
        "エモクロアTRPGの記法や処理のための公式拡張機能です。",
    )
}
