//! Parsing and judgment logic for Call of Cthulhu 6th edition.

use generast::types::{Dice, JudgementData};
use generast::{
    DiceExpr, Generast, Rational, SeededEntropy, diceroll_tin, extract_diceroll, judgement_tin,
};
use winnow::ascii::digit1;
use winnow::combinator::{alt, delimited, opt, preceded};
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::take_while;

use crate::error::Coc6Error;

/// Derives a deterministic seed from speaker ID and text.
pub fn derive_seed(speaker_id: u64, text: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&speaker_id.to_le_bytes());
    hasher.update(text.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Evaluates CoC 6th edition judgment degree and level name.
///
/// - 出目 1..=5 かつ成功: 決定的成功 `degree: 2`
/// - 出目 <= floor(target/5): スペシャル `degree: 2`
/// - 出目 <= target: 成功 `degree: 1`
/// - 出目 96..=100 かつ失敗: 致命的失敗 `degree: -2`
/// - 出目 > target: 失敗 `degree: -1`
pub fn judge_coc6(roll: u64, target: u64) -> (i8, String) {
    if roll <= target {
        if roll <= 5 {
            (2, "決定的成功".to_string())
        } else if roll <= target / 5 {
            (2, "スペシャル".to_string())
        } else {
            (1, "成功".to_string())
        }
    } else if (96..=100).contains(&roll) {
        (-2, "致命的失敗".to_string())
    } else {
        (-1, "失敗".to_string())
    }
}

/// Parses a skill subject from brackets `【……】` or `[...]`.
pub fn parse_bracket_subject<'s>(input: &mut &'s str) -> ModalResult<&'s str> {
    let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
    alt((
        delimited('【', take_while(1.., |c: char| c != '】'), '】'),
        delimited('[', take_while(1.., |c: char| c != ']'), ']'),
    ))
    .parse_next(input)
}

/// Parses an optional target and skill subject suffix, e.g. `<=70【目星】` or `【目星】`.
pub fn parse_suffix<'s>(input: &mut &'s str) -> ModalResult<(Option<u64>, Option<&'s str>)> {
    let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;

    let target = opt(preceded(alt(("<=", "<", "==", "=")), digit1)).parse_next(input)?;

    let target_val = if let Some(t_str) = target {
        let v: u64 = t_str
            .parse()
            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
        Some(v)
    } else {
        None
    };

    let subject = opt(parse_bracket_subject).parse_next(input)?;

    Ok((target_val, subject))
}

fn parse_prefix<'s>(input: &mut &'s str) -> ModalResult<Option<&'s str>> {
    opt(alt(("1D100", "1d100", "D100", "d100", "CC", "cc"))).parse_next(input)
}

/// Parses a complete CoC 6 text command, e.g. `1D100<=70【目星】` or `CC<=70【目星】`.
pub fn parse_coc6_string(input: &str, speaker_id: u64) -> Result<Generast, Coc6Error> {
    let trimmed = input.trim();

    let mut cursor = trimmed;
    let has_prefix = parse_prefix
        .parse_next(&mut cursor)
        .map_err(|e| Coc6Error::Parse(format!("prefix parse error: {e}")))?;

    if has_prefix.is_some()
        && let Ok((target_opt, subject_opt)) = parse_suffix.parse_next(&mut cursor)
        && let Some(target) = target_opt
    {
        let seed = derive_seed(speaker_id, trimmed);
        let dice_expr = DiceExpr::Sum(Box::new(DiceExpr::RollN {
            dice: Box::new(DiceExpr::Dice(Dice::new(100))),
            count: Box::new(DiceExpr::Integer(1)),
        }));
        let full_expr = DiceExpr::Le(
            Box::new(dice_expr.clone()),
            Box::new(DiceExpr::Integer(target)),
        );

        let mut entropy = SeededEntropy::from_seed(seed);
        let roll_val = dice_expr
            .eval(&mut entropy)
            .map_err(|e| Coc6Error::Parse(e.to_string()))?;
        let roll = roll_val
            .as_rational()
            .map_err(|e| Coc6Error::Parse(e.to_string()))?
            .floor()
            .map_err(|e| Coc6Error::Parse(e.to_string()))?;

        let (degree, level_name) = judge_coc6(roll, target);
        let roll_tin = diceroll_tin(seed, full_expr)?;

        let judgement = JudgementData {
            degree,
            level_name,
            subject: subject_opt.map(|s| s.to_string()),
            target: Some(Rational::from_integer(target)),
            roll: Generast::element(roll_tin),
        };

        let elem = Generast::element(judgement_tin(&judgement)?);
        if !cursor.is_empty() {
            return Ok(Generast::nested(
                tin::Tin::need_not_know(Vec::new()),
                vec![elem, Generast::string(cursor)],
            ));
        }
        return Ok(elem);
    }

    Ok(Generast::string(input))
}

/// Attempts to parse a judgement node from an existing diceroll element and following string.
pub fn try_combine_diceroll_and_suffix(
    roll_ast: &Generast,
    suffix_str: &str,
) -> Option<(Generast, String)> {
    let tin = match roll_ast {
        Generast::Element(t) => t,
        _ => return None,
    };
    let dice_data = extract_diceroll(tin).ok()?;

    let mut cursor = suffix_str.trim_start();
    let (target_opt, subject_opt) = parse_suffix.parse_next(&mut cursor).ok()?;

    // Must have either target or subject or both
    if target_opt.is_none() && subject_opt.is_none() {
        return None;
    }

    // Determine target from suffix or from expr
    let target = if let Some(t) = target_opt {
        t
    } else if let DiceExpr::Le(_, target_expr) = &dice_data.expr {
        let mut temp_e = SeededEntropy::from_seed([0u8; 32]);
        let val = target_expr.eval(&mut temp_e).ok()?;
        val.as_rational().ok()?.floor().ok()?
    } else {
        return None;
    };

    // Determine roll from expr
    let mut entropy = SeededEntropy::from_seed(dice_data.seed);
    let roll = match &dice_data.expr {
        DiceExpr::Le(roll_part, _) => {
            let val = roll_part.eval(&mut entropy).ok()?;
            val.as_rational().ok()?.floor().ok()?
        }
        other => {
            let val = other.eval(&mut entropy).ok()?;
            val.as_rational().ok()?.floor().ok()?
        }
    };

    let (degree, level_name) = judge_coc6(roll, target);
    let judgement = JudgementData {
        degree,
        level_name,
        subject: subject_opt.map(|s| s.to_string()),
        target: Some(Rational::from_integer(target)),
        roll: roll_ast.clone(),
    };

    let tin = judgement_tin(&judgement).ok()?;
    let remaining = cursor.to_string();
    Some((Generast::element(tin), remaining))
}
