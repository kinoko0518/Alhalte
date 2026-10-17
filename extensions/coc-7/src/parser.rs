//! Parsing and judgment logic for Call of Cthulhu 7th edition.

use generast::entropy::Entropy;
use generast::types::{Dice, JudgementData};
use generast::{
    DiceExpr, Generast, Rational, SeededEntropy, diceroll_tin, extract_diceroll, judgement_tin,
};
use winnow::ascii::digit1;
use winnow::combinator::{alt, delimited, opt, preceded};
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::take_while;

use crate::error::Coc7Error;

/// Derives a deterministic seed from speaker ID and text.
pub fn derive_seed(speaker_id: u64, text: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&speaker_id.to_le_bytes());
    hasher.update(text.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Evaluates CoC 7th edition judgment degree and level name.
///
/// - 出目 1: クリティカル `degree: 4`
/// - 出目 100、または目標値 < 50 の場合 出目 96..=100: ファンブル `degree: -2`
/// - 出目 <= floor(target / 5): イクストリーム成功 `degree: 3`
/// - 出目 <= floor(target / 2): ハード成功 `degree: 2`
/// - 出目 <= target: レギュラー成功 `degree: 1`
/// - 出目 > target: 失敗 `degree: -1`
pub fn judge_coc7(roll: u64, target: u64) -> (i8, String) {
    if roll == 1 {
        (4, "クリティカル".to_string())
    } else if roll == 100 || (target < 50 && (96..=100).contains(&roll)) {
        (-2, "ファンブル".to_string())
    } else if roll <= target / 5 {
        (3, "イクストリーム成功".to_string())
    } else if roll <= target / 2 {
        (2, "ハード成功".to_string())
    } else if roll <= target {
        (1, "レギュラー成功".to_string())
    } else {
        (-1, "失敗".to_string())
    }
}

/// Rolls dice for CoC 7th edition according to bonus/penalty count.
pub fn roll_coc7_dice<E: Entropy>(bonus: i8, mut entropy: E) -> Result<u64, Coc7Error> {
    if bonus == 0 {
        let val = entropy
            .take(100)
            .map_err(|e| Coc7Error::Parse(e.to_string()))?;
        return Ok(val);
    }

    // Units die: 0..=9 (where 10 is 0)
    let u_raw = entropy
        .take(10)
        .map_err(|e| Coc7Error::Parse(e.to_string()))?;
    let u = if u_raw == 10 { 0 } else { u_raw };

    let tens_count = 1 + bonus.unsigned_abs() as usize;
    let mut candidate_rolls = Vec::with_capacity(tens_count);

    for _ in 0..tens_count {
        let t_raw = entropy
            .take(10)
            .map_err(|e| Coc7Error::Parse(e.to_string()))?;
        let t = if t_raw == 10 { 0 } else { t_raw };
        let roll = if t == 0 && u == 0 { 100 } else { t * 10 + u };
        candidate_rolls.push(roll);
    }

    if bonus > 0 {
        Ok(candidate_rolls.into_iter().min().unwrap_or(1))
    } else {
        Ok(candidate_rolls.into_iter().max().unwrap_or(100))
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

/// Parses CoC 7 command prefix with bonus/penalty indication.
pub fn parse_prefix(input: &mut &str) -> ModalResult<Option<i8>> {
    let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;

    // Longest matches first
    opt(alt((
        alt(("CCB2", "ccb2", "CC+2", "cc+2")).value(2i8),
        alt(("CCB1", "ccb1", "CCB", "ccb", "CC+1", "cc+1")).value(1i8),
        alt(("CCP2", "ccp2", "CC-2", "cc-2")).value(-2i8),
        alt(("CCP1", "ccp1", "CCP", "ccp", "CC-1", "cc-1")).value(-1i8),
        alt(("1D100", "1d100", "D100", "d100", "CC", "cc")).value(0i8),
    )))
    .parse_next(input)
}

/// Parses an optional target and skill subject suffix, e.g. `<=70【目星】`.
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

/// Parses a complete CoC 7 text command, e.g. `CC<=70【目星】` or `CCB<=70【目星】`.
pub fn parse_coc7_string(input: &str, speaker_id: u64) -> Result<Generast, Coc7Error> {
    let trimmed = input.trim();

    let mut cursor = trimmed;
    let prefix_bonus = parse_prefix
        .parse_next(&mut cursor)
        .map_err(|e| Coc7Error::Parse(format!("prefix parse error: {e}")))?;

    if let Some(bonus) = prefix_bonus
        && let Ok((target_opt, subject_opt)) = parse_suffix.parse_next(&mut cursor)
        && let Some(target) = target_opt
    {
        let seed = derive_seed(speaker_id, trimmed);
        let entropy = SeededEntropy::from_seed(seed);
        let roll = roll_coc7_dice(bonus, entropy)?;

        let (degree, level_name) = judge_coc7(roll, target);

        let full_expr = if bonus == 0 {
            let dice_expr = DiceExpr::Sum(Box::new(DiceExpr::RollN {
                dice: Box::new(DiceExpr::Dice(Dice::new(100))),
                count: Box::new(DiceExpr::Integer(1)),
            }));
            DiceExpr::Le(Box::new(dice_expr), Box::new(DiceExpr::Integer(target)))
        } else {
            DiceExpr::Le(
                Box::new(DiceExpr::Integer(roll)),
                Box::new(DiceExpr::Integer(target)),
            )
        };

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

    if target_opt.is_none() && subject_opt.is_none() {
        return None;
    }

    let target = if let Some(t) = target_opt {
        t
    } else if let DiceExpr::Le(_, target_expr) = &dice_data.expr {
        let mut temp_e = SeededEntropy::from_seed([0u8; 32]);
        let val = target_expr.eval(&mut temp_e).ok()?;
        val.as_rational().ok()?.floor().ok()?
    } else {
        return None;
    };

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

    let (degree, level_name) = judge_coc7(roll, target);
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
