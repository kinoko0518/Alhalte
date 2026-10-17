//! Parsing logic for Alhalte DSL using Winnow.

use generast::types::{Dice, SortMode};
use generast::{DiceExpr, Generast, Pred, Rational, assign_tin, diceroll_tin, fluct_tin};
use winnow::ascii::digit1;
use winnow::combinator::{alt, delimited, opt, preceded, terminated};
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::{literal, take_while};

use crate::error::DslError;

/// Derives a deterministic 256-bit seed from the speaker ID and input string.
pub fn derive_seed(speaker_id: u64, text: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&speaker_id.to_le_bytes());
    hasher.update(text.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Parses a DSL message and transforms it into a [`Generast`] AST node.
pub fn parse_dsl(input: &str, speaker_id: u64) -> Result<Generast, DslError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Generast::string(input));
    }

    // 1. Character status fluctuation: {CharacterID}.{N}+{M} or {CharacterID}.{N}-{M}
    if let Ok(fluct) = parse_fluct.parse(trimmed) {
        return Ok(Generast::element(fluct_tin(
            fluct.character_id,
            fluct.status,
            fluct.delta,
        )?));
    }

    // 2. Character status assignment: {CharacterID}.{N}={M}
    if let Ok(assign) = parse_assign.parse(trimmed) {
        return Ok(Generast::element(assign_tin(
            assign.character_id,
            assign.status,
            assign.value,
        )?));
    }

    // 3. Standalone character mention: @123 or {123}
    if let Ok(id) = parse_mention_exact.parse(trimmed) {
        return Ok(Generast::character_id(id));
    }

    // 4. Dice expression or syntax error
    if is_potential_dice_expr(trimmed) {
        let mut text_cursor = trimmed;
        match parse_dice_command.parse_next(&mut text_cursor) {
            Ok(expr) if text_cursor.trim().is_empty() => {
                let seed = derive_seed(speaker_id, trimmed);
                return Ok(Generast::element(diceroll_tin(seed, expr)?));
            }
            Ok(_) | Err(_) => {
                if should_be_syntax_error(trimmed) {
                    return Ok(Generast::syntax_error(format!(
                        "syntax error in dice expression: {trimmed}"
                    )));
                }
            }
        }
    }

    // 5. Embedded mentions (e.g. "Hello @123 world")
    if (trimmed.contains('@') || (trimmed.contains('{') && trimmed.contains('}')))
        && let Ok(nodes) = parse_mixed_text(input)
        && (nodes.len() > 1 || (nodes.len() == 1 && !matches!(nodes[0], Generast::String(_))))
    {
        if nodes.len() == 1 {
            return Ok(nodes.into_iter().next().unwrap_or(Generast::string(input)));
        }
        return Ok(Generast::Nested(generast::Nested::new(
            tin::Tin::need_not_know(Vec::new()),
            nodes,
        )));
    }

    // Fallback: plain text
    Ok(Generast::string(input))
}

fn is_potential_dice_expr(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("choice[") {
        return true;
    }
    if lower.starts_with('x')
        && lower[1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    {
        return true;
    }
    if lower.contains('d') {
        // e.g. 2d6, d6, d66, (1+3)d4
        let has_d_digit = s
            .chars()
            .any(|c| c == 'd' || c == 'D' || c.is_ascii_digit());
        if has_d_digit {
            return true;
        }
    }
    if lower.contains("ty") || lower.contains("tally") {
        return true;
    }
    if (lower.contains('r') || lower.contains('u'))
        && (lower.contains(">=")
            || lower.contains("<=")
            || lower.contains('>')
            || lower.contains('<')
            || lower.contains('='))
    {
        return true;
    }
    false
}

fn should_be_syntax_error(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("choice[") {
        return true;
    }
    // Unbalanced parentheses
    let open_count = s.chars().filter(|&c| c == '(').count();
    let close_count = s.chars().filter(|&c| c == ')').count();
    if open_count != close_count {
        return true;
    }
    // Trailing operators: 2d6+, 1d100*, etc.
    let trimmed = s.trim();
    if trimmed.ends_with('+')
        || trimmed.ends_with('-')
        || trimmed.ends_with('*')
        || trimmed.ends_with('/')
        || trimmed.ends_with('d')
        || trimmed.ends_with('D')
        || trimmed.ends_with(">=")
        || trimmed.ends_with("<=")
        || trimmed.ends_with("==")
        || trimmed.ends_with('>')
        || trimmed.ends_with('<')
        || trimmed.ends_with('=')
    {
        return true;
    }
    // Check if it clearly was meant to be a dice roll like 2D or D
    if lower == "d" || lower.starts_with("d+") || lower.ends_with("d") {
        return true;
    }
    false
}

#[derive(Debug)]
struct FluctParsed {
    character_id: u64,
    status: String,
    delta: i32,
}

fn parse_fluct(input: &mut &str) -> ModalResult<FluctParsed> {
    let _ = literal('{').parse_next(input)?;
    let id_str = digit1.parse_next(input)?;
    let _ = literal('}').parse_next(input)?;
    let _ = literal('.').parse_next(input)?;
    let has_brace = opt(literal('{')).parse_next(input)?;
    let status = take_while(1.., |c: char| {
        c != '+' && c != '-' && c != '=' && c != '}' && !c.is_whitespace()
    })
    .parse_next(input)?;
    if has_brace.is_some() {
        let _ = literal('}').parse_next(input)?;
    }
    let sign = alt(('+', '-')).parse_next(input)?;
    let num_str = digit1.parse_next(input)?;

    let character_id: u64 = id_str
        .parse()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
    let val: i32 = num_str
        .parse()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
    let delta = if sign == '-' { -val } else { val };

    Ok(FluctParsed {
        character_id,
        status: status.to_string(),
        delta,
    })
}

#[derive(Debug)]
struct AssignParsed {
    character_id: u64,
    status: String,
    value: i32,
}

fn parse_assign(input: &mut &str) -> ModalResult<AssignParsed> {
    let _ = literal('{').parse_next(input)?;
    let id_str = digit1.parse_next(input)?;
    let _ = literal('}').parse_next(input)?;
    let _ = literal('.').parse_next(input)?;
    let has_brace = opt(literal('{')).parse_next(input)?;
    let status =
        take_while(1.., |c: char| c != '=' && c != '}' && !c.is_whitespace()).parse_next(input)?;
    if has_brace.is_some() {
        let _ = literal('}').parse_next(input)?;
    }
    let _ = literal('=').parse_next(input)?;
    let sign = opt(alt(('+', '-'))).parse_next(input)?;
    let num_str = digit1.parse_next(input)?;

    let character_id: u64 = id_str
        .parse()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
    let mut val: i32 = num_str
        .parse()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
    if sign == Some('-') {
        val = -val;
    }

    Ok(AssignParsed {
        character_id,
        status: status.to_string(),
        value: val,
    })
}

fn parse_mention_exact(input: &mut &str) -> ModalResult<u64> {
    let id_str = alt((preceded('@', digit1), delimited('{', digit1, '}'))).parse_next(input)?;

    id_str
        .parse::<u64>()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))
}

fn parse_mixed_text(input: &str) -> Result<Vec<Generast>, DslError> {
    let mut nodes = Vec::new();
    let mut cursor = input;

    while !cursor.is_empty() {
        if cursor.starts_with('@') {
            let mut test_c = cursor;
            if let Ok(id) = parse_mention_at.parse_next(&mut test_c) {
                nodes.push(Generast::character_id(id));
                cursor = test_c;
                continue;
            }
        } else if cursor.starts_with('{') {
            let mut test_c = cursor;
            if let Ok(id) = parse_mention_braced.parse_next(&mut test_c) {
                nodes.push(Generast::character_id(id));
                cursor = test_c;
                continue;
            }
        }

        // Take regular text until next mention or end
        let end_idx = cursor[1..]
            .find(['@', '{'])
            .map(|i| i + 1)
            .unwrap_or(cursor.len());
        let text_part = &cursor[..end_idx];
        if !text_part.is_empty() {
            nodes.push(Generast::string(text_part));
        }
        cursor = &cursor[end_idx..];
    }

    Ok(nodes)
}

fn parse_mention_at(input: &mut &str) -> ModalResult<u64> {
    let _ = literal('@').parse_next(input)?;
    let id_str = digit1.parse_next(input)?;
    id_str
        .parse::<u64>()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))
}

fn parse_mention_braced(input: &mut &str) -> ModalResult<u64> {
    let _ = literal('{').parse_next(input)?;
    let id_str = digit1.parse_next(input)?;
    let _ = literal('}').parse_next(input)?;
    id_str
        .parse::<u64>()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))
}

// --- Dice Command Parsers ---

/// Parses a complete dice command, including optional repetition prefixes or suffixes.
pub fn parse_dice_command(input: &mut &str) -> ModalResult<DiceExpr> {
    // Check for repeat prefix: xN <expr>
    let prefix_repeat = opt(terminated(
        preceded(alt(('x', 'X')), digit1),
        take_while(0.., |c: char| c.is_whitespace()),
    ))
    .parse_next(input)?;

    let expr = if input.to_ascii_lowercase().starts_with("choice[") {
        parse_choice.parse_next(input)?
    } else {
        parse_comparison.parse_next(input)?
    };

    // Check for repeat suffix: <expr> xN
    let suffix_repeat = opt(preceded(
        take_while(0.., |c: char| c.is_whitespace()),
        preceded(alt(('x', 'X')), digit1),
    ))
    .parse_next(input)?;

    let rep_count_str = prefix_repeat.or(suffix_repeat);
    if let Some(count_str) = rep_count_str {
        let count: u64 = count_str
            .parse()
            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
        Ok(DiceExpr::Repeat {
            count: Box::new(DiceExpr::Integer(count)),
            expr: Box::new(expr),
        })
    } else {
        Ok(expr)
    }
}

fn parse_choice(input: &mut &str) -> ModalResult<DiceExpr> {
    let _ = alt(("choice[", "CHOICE[", "Choice[")).parse_next(input)?;
    let content = take_while(0.., |c: char| c != ']').parse_next(input)?;
    let _ = literal(']').parse_next(input)?;

    let items: Vec<DiceExpr> = content
        .split(',')
        .map(|s| DiceExpr::String(s.trim().to_string()))
        .collect();

    Ok(DiceExpr::Choose {
        list: Box::new(DiceExpr::List(items)),
        count: Box::new(DiceExpr::Integer(1)),
    })
}

fn parse_comparison(input: &mut &str) -> ModalResult<DiceExpr> {
    let left = parse_additive.parse_next(input)?;

    let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
    let op = opt(alt(("==", "!=", "<=", ">=", "<", ">", "="))).parse_next(input)?;

    if let Some(op_str) = op {
        let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
        let right = parse_additive.parse_next(input)?;

        let left_boxed = Box::new(wrap_for_arithmetic(left));
        let right_boxed = Box::new(wrap_for_arithmetic(right));

        match op_str {
            "==" | "=" => Ok(DiceExpr::Eq(left_boxed, right_boxed)),
            "!=" => Ok(DiceExpr::Ne(left_boxed, right_boxed)),
            "<" => Ok(DiceExpr::Lt(left_boxed, right_boxed)),
            "<=" => Ok(DiceExpr::Le(left_boxed, right_boxed)),
            ">" => Ok(DiceExpr::Gt(left_boxed, right_boxed)),
            ">=" => Ok(DiceExpr::Ge(left_boxed, right_boxed)),
            _ => Err(ErrMode::Backtrack(ContextError::new())),
        }
    } else {
        Ok(left)
    }
}

fn parse_additive(input: &mut &str) -> ModalResult<DiceExpr> {
    let mut left = parse_multiplicative.parse_next(input)?;

    loop {
        let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
        let op = opt(alt(('+', '-'))).parse_next(input)?;
        let Some(op_char) = op else { break };

        let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
        let right = parse_multiplicative.parse_next(input)?;

        let l_boxed = Box::new(wrap_for_arithmetic(left));
        let r_boxed = Box::new(wrap_for_arithmetic(right));

        left = match op_char {
            '+' => DiceExpr::Add(l_boxed, r_boxed),
            '-' => DiceExpr::Sub(l_boxed, r_boxed),
            _ => unreachable!(),
        };
    }

    Ok(left)
}

fn parse_multiplicative(input: &mut &str) -> ModalResult<DiceExpr> {
    let mut left = parse_unary.parse_next(input)?;

    loop {
        let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
        let op = opt(alt(('*', '/'))).parse_next(input)?;
        let Some(op_char) = op else { break };

        let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
        let right = parse_unary.parse_next(input)?;

        let l_boxed = Box::new(wrap_for_arithmetic(left));
        let r_boxed = Box::new(wrap_for_arithmetic(right));

        left = match op_char {
            '*' => DiceExpr::Mul(l_boxed, r_boxed),
            '/' => DiceExpr::Div(l_boxed, r_boxed),
            _ => unreachable!(),
        };
    }

    Ok(left)
}

fn parse_unary(input: &mut &str) -> ModalResult<DiceExpr> {
    let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
    let sign = opt(alt(('+', '-'))).parse_next(input)?;
    let primary = parse_primary.parse_next(input)?;

    if sign == Some('-') {
        Ok(DiceExpr::Sub(
            Box::new(DiceExpr::Rational(Rational::zero())),
            Box::new(wrap_for_arithmetic(primary)),
        ))
    } else {
        Ok(primary)
    }
}

fn parse_primary(input: &mut &str) -> ModalResult<DiceExpr> {
    let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;

    // 1. Parenthesized: ( expr )
    if input.starts_with('(') {
        let _ = literal('(').parse_next(input)?;
        let inner = parse_comparison.parse_next(input)?;
        let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
        let _ = literal(')').parse_next(input)?;

        // Check if followed by D (e.g. (1+3)D(2+2))
        let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
        if input.starts_with('d') || input.starts_with('D') {
            let _ = alt(('d', 'D')).parse_next(input)?;
            let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
            let faces_expr = parse_primary.parse_next(input)?;
            let faces_boxed = Box::new(DiceExpr::MakeDice(Box::new(DiceExpr::Floor(Box::new(
                wrap_for_arithmetic(faces_expr),
            )))));
            let count_boxed = Box::new(DiceExpr::Floor(Box::new(wrap_for_arithmetic(inner))));
            let roll = DiceExpr::RollN {
                dice: faces_boxed,
                count: count_boxed,
            };
            return parse_dice_keep_drop(input, roll);
        }
        return Ok(inner);
    }

    // 2. D66
    if let Ok(d66_expr) = parse_d66(input) {
        return Ok(d66_expr);
    }

    // 3. Choice, Tally, Reroll, Upper, or standard Dice / Integer
    // Try count + special op (TY, R, U) or D
    // Or leading D (e.g. D6)
    if input.starts_with('d') || input.starts_with('D') {
        let _ = alt(('d', 'D')).parse_next(input)?;
        let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
        let faces_str = digit1.parse_next(input)?;
        let faces: u64 = faces_str
            .parse()
            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
        let roll = DiceExpr::RollN {
            dice: Box::new(DiceExpr::Dice(Dice::new(faces))),
            count: Box::new(DiceExpr::Integer(1)),
        };
        return parse_dice_keep_drop(input, roll);
    }

    // Must be a number followed by either D, R, U, TY, Tally, or just an integer
    let num_str = digit1.parse_next(input)?;
    let count: u64 = num_str
        .parse()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;

    // Check what follows
    if input.starts_with('d') || input.starts_with('D') {
        let _ = alt(('d', 'D')).parse_next(input)?;
        let _ = take_while(0.., |c: char| c.is_whitespace()).parse_next(input)?;
        let faces_expr = parse_primary.parse_next(input)?;
        let faces_boxed = Box::new(DiceExpr::MakeDice(Box::new(DiceExpr::Floor(Box::new(
            wrap_for_arithmetic(faces_expr),
        )))));
        let roll = DiceExpr::RollN {
            dice: faces_boxed,
            count: Box::new(DiceExpr::Integer(count)),
        };
        return parse_dice_keep_drop(input, roll);
    }

    // Reroll (nRm>=t)
    if input.starts_with('r') || input.starts_with('R') {
        let _ = alt(('r', 'R')).parse_next(input)?;
        let faces_str = digit1.parse_next(input)?;
        let faces: u64 = faces_str
            .parse()
            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
        let pred = parse_predicate.parse_next(input)?;
        let dice_list = (0..count)
            .map(|_| DiceExpr::Dice(Dice::new(faces)))
            .collect();
        return Ok(DiceExpr::RerollDice {
            dice_list: Box::new(DiceExpr::List(dice_list)),
            pred,
        });
    }

    // Upper infinite (nUm>=t)
    if input.starts_with('u') || input.starts_with('U') {
        let _ = alt(('u', 'U')).parse_next(input)?;
        let faces_str = digit1.parse_next(input)?;
        let faces: u64 = faces_str
            .parse()
            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
        let pred = parse_predicate.parse_next(input)?;
        return Ok(DiceExpr::UpperDice {
            dice: Box::new(DiceExpr::Dice(Dice::new(faces))),
            groups: Box::new(DiceExpr::Integer(count)),
            pred,
        });
    }

    // Tally (nTYm or nTallym)
    if input.to_ascii_lowercase().starts_with("ty")
        || input.to_ascii_lowercase().starts_with("tally")
    {
        let is_ty = input.to_ascii_lowercase().starts_with("ty");
        if is_ty {
            let _ = alt(("TY", "ty", "Ty")).parse_next(input)?;
        } else {
            let _ = alt(("TALLY", "tally", "Tally")).parse_next(input)?;
        }
        let faces_str = digit1.parse_next(input)?;
        let faces: u64 = faces_str
            .parse()
            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
        return Ok(DiceExpr::Tally {
            dice: Box::new(DiceExpr::Dice(Dice::new(faces))),
            count: Box::new(DiceExpr::Integer(count)),
            exclude_zero: is_ty,
        });
    }

    // Plain integer
    Ok(DiceExpr::Integer(count))
}

fn parse_dice_keep_drop(input: &mut &str, roll: DiceExpr) -> ModalResult<DiceExpr> {
    let kd = opt(alt(("KH", "kh", "KL", "kl", "DH", "dh", "DL", "dl"))).parse_next(input)?;
    if let Some(kd_str) = kd {
        let k_str = digit1.parse_next(input)?;
        let k: u64 = k_str
            .parse()
            .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
        let kd_upper = kd_str.to_ascii_uppercase();
        let kept = match kd_upper.as_str() {
            "KH" => DiceExpr::KeepHighest {
                list: Box::new(roll),
                k: Box::new(DiceExpr::Integer(k)),
            },
            "KL" => DiceExpr::KeepLowest {
                list: Box::new(roll),
                k: Box::new(DiceExpr::Integer(k)),
            },
            "DH" => DiceExpr::DropHighest {
                list: Box::new(roll),
                k: Box::new(DiceExpr::Integer(k)),
            },
            "DL" => DiceExpr::DropLowest {
                list: Box::new(roll),
                k: Box::new(DiceExpr::Integer(k)),
            },
            _ => unreachable!(),
        };
        Ok(DiceExpr::Sum(Box::new(kept)))
    } else {
        Ok(DiceExpr::Sum(Box::new(roll)))
    }
}

fn parse_d66(input: &mut &str) -> ModalResult<DiceExpr> {
    let _ = alt(("D66", "d66")).parse_next(input)?;
    let mode_char = opt(alt(('S', 's', 'A', 'a', 'D', 'd', 'N', 'n'))).parse_next(input)?;
    let mode = match mode_char {
        Some('S' | 's' | 'A' | 'a') => SortMode::Asc,
        Some('D' | 'd') => SortMode::Desc,
        _ => SortMode::None,
    };
    Ok(DiceExpr::D66(mode))
}

fn parse_predicate(input: &mut &str) -> ModalResult<Pred> {
    let op = alt(("==", "!=", "<=", ">=", "<", ">", "=")).parse_next(input)?;
    let val_str = digit1.parse_next(input)?;
    let val: u64 = val_str
        .parse()
        .map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
    let r = Rational::from_integer(val);

    let pred = match op {
        "==" | "=" => Pred::Eq(r),
        "!=" => Pred::Ne(r),
        "<" => Pred::Lt(r),
        "<=" => Pred::Le(r),
        ">" => Pred::Gt(r),
        ">=" => Pred::Ge(r),
        _ => unreachable!(),
    };
    Ok(pred)
}

fn wrap_for_arithmetic(expr: DiceExpr) -> DiceExpr {
    match expr {
        DiceExpr::Integer(i) => DiceExpr::Rational(Rational::from_integer(i)),
        DiceExpr::RollN { .. } => DiceExpr::Sum(Box::new(expr)),
        other => other,
    }
}
