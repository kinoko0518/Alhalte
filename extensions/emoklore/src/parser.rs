//! Parsing and judgment logic for Emoklore.

use generast::types::{Dice, JudgementData};
use generast::{
    DiceExpr, Generast, Rational, SeededEntropy, diceroll_tin, extract_diceroll, judgement_tin,
};
use winnow::ascii::digit1;
use winnow::combinator::{alt, delimited, opt, preceded};
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::take_while;

use crate::error::EmokloreError;

/// Derives a deterministic seed from speaker ID and text.
pub fn derive_seed(speaker_id: u64, text: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&speaker_id.to_le_bytes());
    hasher.update(text.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Evaluates Emoklore judgment degree and level name from a list of 10-sided dice rolls and a target.
///
/// Rules:
/// - 出目 1: +2（クリティカル）
/// - 出目 2..=target: +1（成功）
/// - 出目 10: -1（ファンブル）
/// - その他: 0
///
/// 結果分類:
/// - 成功度 <= -1: ファンブル
/// - 成功度 0: 失敗
/// - 成功度 1: シングル
/// - 成功度 2: ダブル
/// - 成功度 3: トリプル
/// - 成功度 4..=9: ミラクル
/// - 成功度 >= 10: カタストロフ
pub fn judge_emoklore(rolls: &[u64], target: u64) -> (i8, String) {
    let mut degree: i32 = 0;
    for &r in rolls {
        if r == 1 {
            degree += 2;
        } else if (2..=target).contains(&r) {
            degree += 1;
        } else if r == 10 {
            degree -= 1;
        }
    }

    let degree_i8 = degree as i8;
    let level_name = match degree {
        ..=-1 => "ファンブル",
        0 => "失敗",
        1 => "シングル",
        2 => "ダブル",
        3 => "トリプル",
        4..=9 => "ミラクル",
        10.. => "カタストロフ",
    };

    (degree_i8, level_name.to_string())
}

/// Parses a skill subject from brackets `〈……〉`, `《……》`, `【……】`, etc.
pub fn parse_angle_bracket_subject<'s>(input: &mut &'s str) -> ModalResult<&'s str> {
    let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
    alt((
        delimited('〈', take_while(1.., |c: char| c != '〉'), '〉'),
        delimited('《', take_while(1.., |c: char| c != '》'), '》'),
        delimited('<', take_while(1.., |c: char| c != '>'), '>'),
        delimited('【', take_while(1.., |c: char| c != '】'), '】'),
        delimited('[', take_while(1.., |c: char| c != ']'), ']'),
    ))
    .parse_next(input)
}

/// Parses an optional target and skill subject suffix, e.g. `<=5〈調査〉` or `〈調査〉`.
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

    let subject = opt(parse_angle_bracket_subject).parse_next(input)?;

    Ok((target_val, subject))
}

/// Parses `{X}DM` or `XDM` or `DM` prefix.
pub fn parse_emoklore_prefix(input: &mut &str) -> ModalResult<Option<u64>> {
    let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;

    let count = opt(alt((delimited('{', digit1, '}'), digit1))).parse_next(input)?;

    let has_dm = opt(alt(("DM", "dm", "Dm", "dM"))).parse_next(input)?;

    match has_dm {
        Some(_) => {
            let count_val = if let Some(c_str) = count {
                c_str
                    .parse::<u64>()
                    .map_err(|_| ErrMode::Backtrack(ContextError::new()))?
            } else {
                1
            };
            Ok(Some(count_val))
        }
        None => Ok(None),
    }
}

/// Parses a complete Emoklore text command, e.g. `{2}DM<=5〈調査〉` or `2DM<=5〈調査〉`.
pub fn parse_emoklore_string(input: &str, speaker_id: u64) -> Result<Generast, EmokloreError> {
    let trimmed = input.trim();

    let mut cursor = trimmed;
    let prefix_res = parse_emoklore_prefix
        .parse_next(&mut cursor)
        .map_err(|e| EmokloreError::Parse(format!("prefix parse error: {e}")))?;

    if let Some(count) = prefix_res
        && let Ok((target_opt, subject_opt)) = parse_suffix.parse_next(&mut cursor)
        && let Some(target) = target_opt
    {
        let seed = derive_seed(speaker_id, trimmed);
        let dice_expr = DiceExpr::RollN {
            dice: Box::new(DiceExpr::Dice(Dice::new(10))),
            count: Box::new(DiceExpr::Integer(count)),
        };

        let mut entropy = SeededEntropy::from_seed(seed);
        let rolls_val = dice_expr
            .eval(&mut entropy)
            .map_err(|e| EmokloreError::Parse(e.to_string()))?;
        let list = rolls_val
            .into_list()
            .map_err(|e| EmokloreError::Parse(e.to_string()))?;
        let mut rolls = Vec::with_capacity(list.len());
        for item in list {
            rolls.push(
                item.as_integer()
                    .map_err(|e| EmokloreError::Parse(e.to_string()))?,
            );
        }

        let (degree, level_name) = judge_emoklore(&rolls, target);
        let roll_tin = diceroll_tin(seed, dice_expr)?;

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
    let rolls = match &dice_data.expr {
        DiceExpr::RollN { .. } => {
            let val = dice_data.expr.eval(&mut entropy).ok()?;
            let list = val.into_list().ok()?;
            let mut nums = Vec::new();
            for item in list {
                nums.push(item.as_integer().ok()?);
            }
            nums
        }
        DiceExpr::Le(roll_part, _) => {
            let val = roll_part.eval(&mut entropy).ok()?;
            if let Ok(list) = val.as_list() {
                let mut nums = Vec::new();
                for item in list {
                    nums.push(item.as_integer().ok()?);
                }
                nums
            } else {
                vec![val.as_rational().ok()?.floor().ok()?]
            }
        }
        other => {
            let val = other.eval(&mut entropy).ok()?;
            if let Ok(list) = val.as_list() {
                let mut nums = Vec::new();
                for item in list {
                    nums.push(item.as_integer().ok()?);
                }
                nums
            } else {
                vec![val.as_rational().ok()?.floor().ok()?]
            }
        }
    };

    let (degree, level_name) = judge_emoklore(&rolls, target);
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
