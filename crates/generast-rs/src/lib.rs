//! # generast
//!
//! `generast` is a library crate for handling generic abstract syntax trees (ASTs),
//! rich text styling, and deterministic dice evaluation in the Alhalte ecosystem.
//!
//! # Architecture
//!
//! - **AST Data Types**: [`Generast`], [`Nested`]
//! - **Label Definitions**: 23 standard label types and typed helpers
//! - **Dice AST & Evaluator**: [`DiceExpr`], [`DiceValue`], [`Entropy`], [`SeededEntropy`]
//! - **Judgement Trees**: [`JudgementData`]
//!
//! # Example
//!
//! ```
//! use generast::{Dice, DiceExpr, SeededEntropy};
//!
//! let expr = DiceExpr::Take(Box::new(DiceExpr::Dice(Dice::new(6))));
//! let mut entropy = SeededEntropy::from_seed([42u8; 32]);
//! let (result, trace) = expr.eval_with_trace(&mut entropy).unwrap();
//!
//! assert!(result.as_integer().unwrap() >= 1 && result.as_integer().unwrap() <= 6);
//! assert_eq!(trace.len(), 1);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod ast;
pub mod dice;
pub mod entropy;
pub mod error;
pub mod label;
pub mod pred;
pub mod rational;
pub mod types;

pub use ast::{Generast, Nested, decode_ext_io, decode_generast, encode_ext_io, encode_generast};
pub use dice::{DEFAULT_MAX_TAKES, DiceExpr, validate_dice_until};
pub use entropy::{Entropy, Integer, SeededEntropy};
pub use error::{EntropyError, EvalError, GenerastError};
pub use label::{
    IDENT_ALHALTE_GENERAST, IDENT_ALHALTE_GENERAST_ASSIGN, IDENT_ALHALTE_GENERAST_CHARACTER_ID,
    IDENT_ALHALTE_GENERAST_DICEROLL, IDENT_ALHALTE_GENERAST_FLUCT,
    IDENT_ALHALTE_GENERAST_STYLE_BOLD, IDENT_ALHALTE_GENERAST_STYLE_BOTEN,
    IDENT_ALHALTE_GENERAST_STYLE_GROUNDED, IDENT_ALHALTE_GENERAST_STYLE_H_IN_V,
    IDENT_ALHALTE_GENERAST_STYLE_H1, IDENT_ALHALTE_GENERAST_STYLE_H2,
    IDENT_ALHALTE_GENERAST_STYLE_H3, IDENT_ALHALTE_GENERAST_STYLE_H4,
    IDENT_ALHALTE_GENERAST_STYLE_HANGING, IDENT_ALHALTE_GENERAST_STYLE_HIDDEN,
    IDENT_ALHALTE_GENERAST_STYLE_INDENT, IDENT_ALHALTE_GENERAST_STYLE_ITALIC,
    IDENT_ALHALTE_GENERAST_STYLE_KERNING, IDENT_ALHALTE_GENERAST_STYLE_PILL,
    IDENT_ALHALTE_GENERAST_STYLE_RUBY, IDENT_ALHALTE_GENERAST_STYLE_TEXT_COLOR,
    IDENT_ALHALTE_GENERAST_SYNTAX_ERROR, IDENT_ALHALTE_JUDGEMENT, LABEL_ALHALTE_GENERAST,
    LABEL_ALHALTE_GENERAST_ASSIGN, LABEL_ALHALTE_GENERAST_CHARACTER_ID,
    LABEL_ALHALTE_GENERAST_DICEROLL, LABEL_ALHALTE_GENERAST_FLUCT,
    LABEL_ALHALTE_GENERAST_STYLE_BOLD, LABEL_ALHALTE_GENERAST_STYLE_BOTEN,
    LABEL_ALHALTE_GENERAST_STYLE_GROUNDED, LABEL_ALHALTE_GENERAST_STYLE_H_IN_V,
    LABEL_ALHALTE_GENERAST_STYLE_H1, LABEL_ALHALTE_GENERAST_STYLE_H2,
    LABEL_ALHALTE_GENERAST_STYLE_H3, LABEL_ALHALTE_GENERAST_STYLE_H4,
    LABEL_ALHALTE_GENERAST_STYLE_HANGING, LABEL_ALHALTE_GENERAST_STYLE_HIDDEN,
    LABEL_ALHALTE_GENERAST_STYLE_INDENT, LABEL_ALHALTE_GENERAST_STYLE_ITALIC,
    LABEL_ALHALTE_GENERAST_STYLE_KERNING, LABEL_ALHALTE_GENERAST_STYLE_PILL,
    LABEL_ALHALTE_GENERAST_STYLE_RUBY, LABEL_ALHALTE_GENERAST_STYLE_TEXT_COLOR,
    LABEL_ALHALTE_GENERAST_SYNTAX_ERROR, LABEL_ALHALTE_JUDGEMENT, assign_tin, bold_meta,
    boten_meta, character_id_tin, diceroll_tin, extract_assign, extract_character_id,
    extract_diceroll, extract_fluct, extract_hanging, extract_indent, extract_judgement,
    extract_kerning, extract_pill, extract_ruby, extract_syntax_error, extract_text_color,
    fluct_tin, grounded_meta, h_in_v_meta, h1_meta, h2_meta, h3_meta, h4_meta, hanging_meta,
    hidden_meta, indent_meta, is_bold_meta, is_boten_meta, is_grounded_meta, is_h_in_v_meta,
    is_h1_meta, is_h2_meta, is_h3_meta, is_h4_meta, is_hidden_meta, is_italic_meta, italic_meta,
    judgement_tin, kerning_meta, pill_meta, ruby_meta, syntax_error_tin, text_color_meta,
};
pub use pred::Pred;
pub use rational::Rational;
pub use types::{
    AssignData, Dice, DiceValue, DicerollData, FluctData, FoldOp, JudgementData, RollRecord,
    SortMode,
};
