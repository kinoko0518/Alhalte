//! Dice abstract syntax tree and deterministic evaluator.

use serde::{Deserialize, Serialize};

use crate::entropy::{Entropy, Integer};
use crate::error::EvalError;
use crate::pred::Pred;
use crate::rational::Rational;
use crate::types::{Dice, DiceValue, FoldOp, RollRecord, SortMode};

/// Default maximum number of dice rolls allowed during a single evaluation.
pub const DEFAULT_MAX_TAKES: usize = 4096;

/// Abstract syntax tree representation of dice operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiceExpr {
    // --- Literals ---
    /// Integer literal.
    Integer(Integer),
    /// Rational literal.
    Rational(Rational),
    /// Boolean literal.
    Bool(bool),
    /// String literal.
    String(String),
    /// Dice definition literal.
    Dice(Dice),
    /// List literal.
    List(Vec<DiceExpr>),

    // --- Primitive Operations ---
    /// Cast integer to rational (`Integer -> Rational`).
    ItoR(Box<DiceExpr>),
    /// Floor rational to integer (`Rational -> Integer`).
    Floor(Box<DiceExpr>),
    /// Rational addition (`Rational × Rational -> Rational`).
    Add(Box<DiceExpr>, Box<DiceExpr>),
    /// Rational subtraction (`Rational × Rational -> Rational`).
    Sub(Box<DiceExpr>, Box<DiceExpr>),
    /// Rational multiplication (`Rational × Rational -> Rational`).
    Mul(Box<DiceExpr>, Box<DiceExpr>),
    /// Rational division (`Rational × Rational -> Rational`).
    Div(Box<DiceExpr>, Box<DiceExpr>),
    /// Rational equality check (`Rational × Rational -> Bool`).
    Eq(Box<DiceExpr>, Box<DiceExpr>),
    /// Rational strictly-less-than check (`Rational × Rational -> Bool`).
    Lt(Box<DiceExpr>, Box<DiceExpr>),
    /// Boolean negation (`Bool -> Bool`).
    Not(Box<DiceExpr>),
    /// Boolean conjunction (`Bool × Bool -> Bool`).
    And(Box<DiceExpr>, Box<DiceExpr>),
    /// Conditional branch (`Bool × T × T -> T`).
    If {
        /// Condition expression.
        cond: Box<DiceExpr>,
        /// Evaluated if condition is true.
        then_branch: Box<DiceExpr>,
        /// Evaluated if condition is false.
        else_branch: Box<DiceExpr>,
    },
    /// Creates a die with the specified number of faces (`Integer -> Dice`).
    MakeDice(Box<DiceExpr>),
    /// Consumes entropy to roll a die once (`Dice -> Integer`).
    Take(Box<DiceExpr>),
    /// Repeatedly rolls die while predicate over roll history is true (`Dice × Pred -> List(Integer)`).
    DiceUntil {
        /// Target die.
        dice: Box<DiceExpr>,
        /// Continuation predicate.
        pred: Pred,
    },
    /// Filters list elements satisfying predicate (`List(T) × Pred(T) -> List(T)`).
    Filter {
        /// Target list.
        list: Box<DiceExpr>,
        /// Filter predicate.
        pred: Pred,
    },
    /// Left fold over list (`List(T) × T' × FoldOp -> T'`).
    Fold {
        /// Target list.
        list: Box<DiceExpr>,
        /// Initial accumulator value.
        init: Box<DiceExpr>,
        /// Binary folding operation.
        op: FoldOp,
    },
    /// Flattens a nested list by one level (`List(List(T)) -> List(T)`).
    Flatten(Box<DiceExpr>),
    /// Sorts rational numbers in ascending order (`List(Rational) -> List(Rational)`).
    SortAsc(Box<DiceExpr>),
    /// Extracts sub-slice of list (`List(T) × Integer × Integer -> List(T)`).
    Slice {
        /// Target list.
        list: Box<DiceExpr>,
        /// Start index (0-based).
        start: Box<DiceExpr>,
        /// Number of elements.
        length: Box<DiceExpr>,
    },
    /// Computes the number of elements in a list (`List(T) -> Integer`).
    Length(Box<DiceExpr>),

    // --- Semantic Operations ---
    /// Ceils rational to integer (`Rational -> Integer`).
    Ceil(Box<DiceExpr>),
    /// Rounds rational to nearest integer `Floor(Add(x, 1/2))` (`Rational -> Integer`).
    Round(Box<DiceExpr>),
    /// Rational not-equal check (`Rational × Rational -> Bool`).
    Ne(Box<DiceExpr>, Box<DiceExpr>),
    /// Rational less-than-or-equal check (`Rational × Rational -> Bool`).
    Le(Box<DiceExpr>, Box<DiceExpr>),
    /// Rational strictly-greater-than check (`Rational × Rational -> Bool`).
    Gt(Box<DiceExpr>, Box<DiceExpr>),
    /// Rational greater-than-or-equal check (`Rational × Rational -> Bool`).
    Ge(Box<DiceExpr>, Box<DiceExpr>),
    /// Sums a list of rationals (`List(Rational) -> Rational`).
    Sum(Box<DiceExpr>),
    /// Keeps the `k` highest values from a list (`List(Rational) × Integer -> List(Rational)`).
    KeepHighest {
        /// Target list.
        list: Box<DiceExpr>,
        /// Number of highest elements to keep.
        k: Box<DiceExpr>,
    },
    /// Keeps the `k` lowest values from a list (`List(Rational) × Integer -> List(Rational)`).
    KeepLowest {
        /// Target list.
        list: Box<DiceExpr>,
        /// Number of lowest elements to keep.
        k: Box<DiceExpr>,
    },
    /// Drops the `k` highest values from a list (`List(Rational) × Integer -> List(Rational)`).
    DropHighest {
        /// Target list.
        list: Box<DiceExpr>,
        /// Number of highest elements to drop.
        k: Box<DiceExpr>,
    },
    /// Drops the `k` lowest values from a list (`List(Rational) × Integer -> List(Rational)`).
    DropLowest {
        /// Target list.
        list: Box<DiceExpr>,
        /// Number of lowest elements to drop.
        k: Box<DiceExpr>,
    },
    /// Rolls die `n` times (`Dice × Integer -> List(Integer)`).
    RollN {
        /// Target die.
        dice: Box<DiceExpr>,
        /// Number of rolls.
        count: Box<DiceExpr>,
    },
    /// Counts elements satisfying predicate (`List(Rational) × Pred(Rational) -> Integer`).
    CountSuccess {
        /// Target list.
        list: Box<DiceExpr>,
        /// Success condition predicate.
        pred: Pred,
    },
    /// Exploding / reroll dice on matching condition (`List(Dice) × Pred(Integer) -> List(Integer)`).
    RerollDice {
        /// List of dice to roll.
        dice_list: Box<DiceExpr>,
        /// Condition to roll again.
        pred: Pred,
    },
    /// Upper infinite roll by groups (`Dice × Integer × Pred(Integer) -> List(List(Integer))`).
    UpperDice {
        /// Target die.
        dice: Box<DiceExpr>,
        /// Number of groups.
        groups: Box<DiceExpr>,
        /// Continuation condition for each die in the group.
        pred: Pred,
    },
    /// D66 roll with sort mode (`SortMode -> Integer`).
    D66(SortMode),
    /// Tallies count of each face (`Dice × Integer -> List(Integer × Integer)`).
    Tally {
        /// Target die.
        dice: Box<DiceExpr>,
        /// Number of dice to roll.
        count: Box<DiceExpr>,
        /// If true, excludes faces with count 0 (`TY`).
        exclude_zero: bool,
    },
    /// Choice without replacement from list of strings (`List(String) × Integer -> List(String)`).
    Choose {
        /// Options list.
        list: Box<DiceExpr>,
        /// Number of items to choose.
        count: Box<DiceExpr>,
    },
    /// Table lookup by key (`List(Integer × String) × Integer -> String`).
    Lookup {
        /// Table pairs list.
        table: Box<DiceExpr>,
        /// Search key.
        key: Box<DiceExpr>,
    },
    /// Repeats expression evaluation `n` times (`Integer × Expr -> List(T)`).
    Repeat {
        /// Repetition count.
        count: Box<DiceExpr>,
        /// Expression to evaluate repeatedly.
        expr: Box<DiceExpr>,
    },
}

impl DiceExpr {
    /// Smart constructor for `DiceUntil`, validating that the predicate is not unstoppable.
    pub fn dice_until(dice: Self, pred: Pred) -> Result<Self, EvalError> {
        if let Self::Dice(d) = &dice {
            validate_dice_until(d.faces, &pred)?;
        } else if let Self::MakeDice(inner) = &dice
            && let Self::Integer(faces) = **inner
        {
            validate_dice_until(faces, &pred)?;
        }
        Ok(Self::DiceUntil {
            dice: Box::new(dice),
            pred,
        })
    }

    /// Evaluates this dice expression deterministically using the provided entropy source.
    pub fn eval<E: Entropy>(&self, entropy: &mut E) -> Result<DiceValue, EvalError> {
        self.eval_with_limit(entropy, DEFAULT_MAX_TAKES)
    }

    /// Evaluates this expression with a custom maximum take limit.
    pub fn eval_with_limit<E: Entropy>(
        &self,
        entropy: &mut E,
        max_takes: usize,
    ) -> Result<DiceValue, EvalError> {
        let (val, _) = self.eval_with_trace_and_limit(entropy, max_takes)?;
        Ok(val)
    }

    /// Evaluates this dice expression, returning both the result and roll trace.
    pub fn eval_with_trace<E: Entropy>(
        &self,
        entropy: &mut E,
    ) -> Result<(DiceValue, Vec<RollRecord>), EvalError> {
        self.eval_with_trace_and_limit(entropy, DEFAULT_MAX_TAKES)
    }

    /// Evaluates this expression with a custom take limit, returning result and trace.
    pub fn eval_with_trace_and_limit<E: Entropy>(
        &self,
        entropy: &mut E,
        max_takes: usize,
    ) -> Result<(DiceValue, Vec<RollRecord>), EvalError> {
        let mut take_count = 0;
        let mut trace = Vec::new();
        let val = eval_internal(self, entropy, &mut take_count, max_takes, &mut trace)?;
        Ok((val, trace))
    }
}

/// Validates that a `DiceUntil` predicate will not run infinitely on a die with `faces` faces.
pub fn validate_dice_until(faces: Integer, pred: &Pred) -> Result<(), EvalError> {
    if faces == 0 {
        return Err(EvalError::Entropy(crate::error::EntropyError::ZeroFaces));
    }
    if pred.is_unstoppable_on_faces(faces) {
        return Err(EvalError::UnstoppableDiceUntil);
    }
    Ok(())
}

fn do_take<E: Entropy>(
    entropy: &mut E,
    faces: Integer,
    take_count: &mut usize,
    max_takes: usize,
    trace: &mut Vec<RollRecord>,
) -> Result<Integer, EvalError> {
    if faces == 0 {
        return Err(EvalError::Entropy(crate::error::EntropyError::ZeroFaces));
    }
    if *take_count >= max_takes {
        return Err(EvalError::TakeLimitExceeded { limit: max_takes });
    }
    *take_count += 1;
    let roll = entropy.take(faces)?;
    trace.push(RollRecord { faces, roll });
    Ok(roll)
}

fn eval_internal<E: Entropy>(
    expr: &DiceExpr,
    entropy: &mut E,
    take_count: &mut usize,
    max_takes: usize,
    trace: &mut Vec<RollRecord>,
) -> Result<DiceValue, EvalError> {
    match expr {
        // --- Literals ---
        DiceExpr::Integer(i) => Ok(DiceValue::Integer(*i)),
        DiceExpr::Rational(r) => Ok(DiceValue::Rational(r.clone())),
        DiceExpr::Bool(b) => Ok(DiceValue::Bool(*b)),
        DiceExpr::String(s) => Ok(DiceValue::String(s.clone())),
        DiceExpr::Dice(d) => Ok(DiceValue::Dice(*d)),
        DiceExpr::List(list) => {
            let mut res = Vec::with_capacity(list.len());
            for item in list {
                res.push(eval_internal(item, entropy, take_count, max_takes, trace)?);
            }
            Ok(DiceValue::List(res))
        }

        // --- Primitive Operations ---
        DiceExpr::ItoR(inner) => {
            let val = eval_internal(inner, entropy, take_count, max_takes, trace)?;
            let i = val.as_integer()?;
            Ok(DiceValue::Rational(Rational::from_integer(i)))
        }
        DiceExpr::Floor(inner) => {
            let val = eval_internal(inner, entropy, take_count, max_takes, trace)?;
            let r = val.as_rational()?;
            Ok(DiceValue::Integer(r.floor()?))
        }
        DiceExpr::Add(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Rational(&ra + &rb))
        }
        DiceExpr::Sub(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Rational(&ra - &rb))
        }
        DiceExpr::Mul(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Rational(&ra * &rb))
        }
        DiceExpr::Div(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Rational(ra.checked_div(&rb)?))
        }
        DiceExpr::Eq(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Bool(ra == rb))
        }
        DiceExpr::Lt(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Bool(ra < rb))
        }
        DiceExpr::Not(inner) => {
            let val = eval_internal(inner, entropy, take_count, max_takes, trace)?;
            let b = val.as_bool()?;
            Ok(DiceValue::Bool(!b))
        }
        DiceExpr::And(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let ba = va.as_bool()?;
            if !ba {
                return Ok(DiceValue::Bool(false));
            }
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let bb = vb.as_bool()?;
            Ok(DiceValue::Bool(bb))
        }
        DiceExpr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let c_val = eval_internal(cond, entropy, take_count, max_takes, trace)?;
            let c_bool = c_val.as_bool()?;
            if c_bool {
                eval_internal(then_branch, entropy, take_count, max_takes, trace)
            } else {
                eval_internal(else_branch, entropy, take_count, max_takes, trace)
            }
        }
        DiceExpr::MakeDice(faces_expr) => {
            let val = eval_internal(faces_expr, entropy, take_count, max_takes, trace)?;
            let faces = val.as_integer()?;
            if faces == 0 {
                return Err(EvalError::Entropy(crate::error::EntropyError::ZeroFaces));
            }
            Ok(DiceValue::Dice(Dice::new(faces)))
        }
        DiceExpr::Take(dice_expr) => {
            let val = eval_internal(dice_expr, entropy, take_count, max_takes, trace)?;
            let d = val.as_dice()?;
            let roll = do_take(entropy, d.faces, take_count, max_takes, trace)?;
            Ok(DiceValue::Integer(roll))
        }
        DiceExpr::DiceUntil { dice, pred } => {
            let val = eval_internal(dice, entropy, take_count, max_takes, trace)?;
            let d = val.as_dice()?;
            validate_dice_until(d.faces, pred)?;

            let mut acc = Vec::new();
            while pred.eval_history(&acc) {
                let roll = do_take(entropy, d.faces, take_count, max_takes, trace)?;
                acc.push(roll);
            }
            Ok(DiceValue::List(
                acc.into_iter().map(DiceValue::Integer).collect(),
            ))
        }
        DiceExpr::Filter { list, pred } => {
            let val = eval_internal(list, entropy, take_count, max_takes, trace)?;
            let items = val.into_list()?;
            let mut filtered = Vec::new();
            for item in items {
                let matched = match &item {
                    DiceValue::Rational(r) => pred.eval_rational(r),
                    DiceValue::Integer(i) => pred.eval_integer(*i),
                    _ => false,
                };
                if matched {
                    filtered.push(item);
                }
            }
            Ok(DiceValue::List(filtered))
        }
        DiceExpr::Fold { list, init, op } => {
            let val_list = eval_internal(list, entropy, take_count, max_takes, trace)?;
            let items = val_list.into_list()?;
            let init_val = eval_internal(init, entropy, take_count, max_takes, trace)?;
            let mut acc = init_val.as_rational()?;
            for item in items {
                let x = item.as_rational()?;
                acc = match op {
                    FoldOp::Add => &acc + &x,
                    FoldOp::Sub => &acc - &x,
                    FoldOp::Mul => &acc * &x,
                };
            }
            Ok(DiceValue::Rational(acc))
        }
        DiceExpr::Flatten(inner) => {
            let val = eval_internal(inner, entropy, take_count, max_takes, trace)?;
            let items = val.into_list()?;
            let mut flattened = Vec::new();
            for item in items {
                match item {
                    DiceValue::List(sub) => flattened.extend(sub),
                    other => flattened.push(other),
                }
            }
            Ok(DiceValue::List(flattened))
        }
        DiceExpr::SortAsc(inner) => {
            let val = eval_internal(inner, entropy, take_count, max_takes, trace)?;
            let items = val.into_list()?;
            let mut pairs = Vec::with_capacity(items.len());
            for item in items {
                let r = item.as_rational()?;
                pairs.push((r, item));
            }
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            let sorted = pairs.into_iter().map(|(_, item)| item).collect();
            Ok(DiceValue::List(sorted))
        }
        DiceExpr::Slice {
            list,
            start,
            length,
        } => {
            let val_list = eval_internal(list, entropy, take_count, max_takes, trace)?;
            let items = val_list.into_list()?;
            let val_start = eval_internal(start, entropy, take_count, max_takes, trace)?;
            let s = val_start.as_integer()? as usize;
            let val_len = eval_internal(length, entropy, take_count, max_takes, trace)?;
            let l = val_len.as_integer()? as usize;

            let sliced = if s >= items.len() {
                Vec::new()
            } else {
                let end = (s + l).min(items.len());
                items[s..end].to_vec()
            };
            Ok(DiceValue::List(sliced))
        }
        DiceExpr::Length(inner) => {
            let val = eval_internal(inner, entropy, take_count, max_takes, trace)?;
            let items = val.into_list()?;
            Ok(DiceValue::Integer(items.len() as u64))
        }

        // --- Semantic Operations ---
        DiceExpr::Ceil(inner) => {
            let val = eval_internal(inner, entropy, take_count, max_takes, trace)?;
            let r = val.as_rational()?;
            Ok(DiceValue::Integer(r.ceil()?))
        }
        DiceExpr::Round(inner) => {
            let val = eval_internal(inner, entropy, take_count, max_takes, trace)?;
            let r = val.as_rational()?;
            Ok(DiceValue::Integer(r.round()?))
        }
        DiceExpr::Ne(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Bool(ra != rb))
        }
        DiceExpr::Le(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Bool(ra <= rb))
        }
        DiceExpr::Gt(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Bool(ra > rb))
        }
        DiceExpr::Ge(a, b) => {
            let va = eval_internal(a, entropy, take_count, max_takes, trace)?;
            let vb = eval_internal(b, entropy, take_count, max_takes, trace)?;
            let ra = va.as_rational()?;
            let rb = vb.as_rational()?;
            Ok(DiceValue::Bool(ra >= rb))
        }
        DiceExpr::Sum(inner) => {
            let val = eval_internal(inner, entropy, take_count, max_takes, trace)?;
            let items = val.into_list()?;
            let mut sum = Rational::zero();
            for item in items {
                let r = item.as_rational()?;
                sum = &sum + &r;
            }
            Ok(DiceValue::Rational(sum))
        }
        DiceExpr::KeepHighest { list, k } => {
            let sorted_val = eval_internal(
                &DiceExpr::SortAsc(list.clone()),
                entropy,
                take_count,
                max_takes,
                trace,
            )?;
            let items = sorted_val.into_list()?;
            let k_val = eval_internal(k, entropy, take_count, max_takes, trace)?;
            let k_usize = k_val.as_integer()? as usize;
            let len = items.len();
            let start = len.saturating_sub(k_usize);
            Ok(DiceValue::List(items[start..].to_vec()))
        }
        DiceExpr::KeepLowest { list, k } => {
            let sorted_val = eval_internal(
                &DiceExpr::SortAsc(list.clone()),
                entropy,
                take_count,
                max_takes,
                trace,
            )?;
            let items = sorted_val.into_list()?;
            let k_val = eval_internal(k, entropy, take_count, max_takes, trace)?;
            let k_usize = (k_val.as_integer()? as usize).min(items.len());
            Ok(DiceValue::List(items[0..k_usize].to_vec()))
        }
        DiceExpr::DropHighest { list, k } => {
            let sorted_val = eval_internal(
                &DiceExpr::SortAsc(list.clone()),
                entropy,
                take_count,
                max_takes,
                trace,
            )?;
            let items = sorted_val.into_list()?;
            let k_val = eval_internal(k, entropy, take_count, max_takes, trace)?;
            let k_usize = k_val.as_integer()? as usize;
            let len = items.len();
            if k_usize >= len {
                Ok(DiceValue::List(Vec::new()))
            } else {
                Ok(DiceValue::List(items[0..(len - k_usize)].to_vec()))
            }
        }
        DiceExpr::DropLowest { list, k } => {
            let sorted_val = eval_internal(
                &DiceExpr::SortAsc(list.clone()),
                entropy,
                take_count,
                max_takes,
                trace,
            )?;
            let items = sorted_val.into_list()?;
            let k_val = eval_internal(k, entropy, take_count, max_takes, trace)?;
            let k_usize = k_val.as_integer()? as usize;
            let len = items.len();
            if k_usize >= len {
                Ok(DiceValue::List(Vec::new()))
            } else {
                Ok(DiceValue::List(items[k_usize..].to_vec()))
            }
        }
        DiceExpr::RollN { dice, count } => {
            let val_dice = eval_internal(dice, entropy, take_count, max_takes, trace)?;
            let d = val_dice.as_dice()?;
            let val_count = eval_internal(count, entropy, take_count, max_takes, trace)?;
            let n = val_count.as_integer()?;

            let mut rolls = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let roll = do_take(entropy, d.faces, take_count, max_takes, trace)?;
                rolls.push(DiceValue::Integer(roll));
            }
            Ok(DiceValue::List(rolls))
        }
        DiceExpr::CountSuccess { list, pred } => {
            let filtered_val = eval_internal(
                &DiceExpr::Filter {
                    list: list.clone(),
                    pred: pred.clone(),
                },
                entropy,
                take_count,
                max_takes,
                trace,
            )?;
            let items = filtered_val.into_list()?;
            Ok(DiceValue::Integer(items.len() as u64))
        }
        DiceExpr::RerollDice { dice_list, pred } => {
            let val_list = eval_internal(dice_list, entropy, take_count, max_takes, trace)?;
            let items = val_list.into_list()?;
            let mut all_rolls = Vec::new();
            for item in items {
                let d = item.as_dice()?;
                validate_dice_until(d.faces, pred)?;
                let mut die_rolls = Vec::new();
                while pred.eval_history(&die_rolls) {
                    let roll = do_take(entropy, d.faces, take_count, max_takes, trace)?;
                    die_rolls.push(roll);
                }
                all_rolls.extend(die_rolls.into_iter().map(DiceValue::Integer));
            }
            Ok(DiceValue::List(all_rolls))
        }
        DiceExpr::UpperDice { dice, groups, pred } => {
            let val_dice = eval_internal(dice, entropy, take_count, max_takes, trace)?;
            let d = val_dice.as_dice()?;
            validate_dice_until(d.faces, pred)?;

            let val_groups = eval_internal(groups, entropy, take_count, max_takes, trace)?;
            let n = val_groups.as_integer()?;

            let mut result_groups = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let mut group_rolls = Vec::new();
                while pred.eval_history(&group_rolls) {
                    let roll = do_take(entropy, d.faces, take_count, max_takes, trace)?;
                    group_rolls.push(roll);
                }
                result_groups.push(DiceValue::List(
                    group_rolls.into_iter().map(DiceValue::Integer).collect(),
                ));
            }
            Ok(DiceValue::List(result_groups))
        }
        DiceExpr::D66(mode) => {
            let d1 = do_take(entropy, 6, take_count, max_takes, trace)?;
            let d2 = do_take(entropy, 6, take_count, max_takes, trace)?;
            let (tens, ones) = match mode {
                SortMode::None => (d1, d2),
                SortMode::Asc => {
                    if d1 <= d2 {
                        (d1, d2)
                    } else {
                        (d2, d1)
                    }
                }
                SortMode::Desc => {
                    if d1 >= d2 {
                        (d1, d2)
                    } else {
                        (d2, d1)
                    }
                }
            };
            Ok(DiceValue::Integer(tens * 10 + ones))
        }
        DiceExpr::Tally {
            dice,
            count,
            exclude_zero,
        } => {
            let val_dice = eval_internal(dice, entropy, take_count, max_takes, trace)?;
            let d = val_dice.as_dice()?;
            let val_count = eval_internal(count, entropy, take_count, max_takes, trace)?;
            let n = val_count.as_integer()?;

            if d.faces == 0 {
                return Err(EvalError::Entropy(crate::error::EntropyError::ZeroFaces));
            }

            let mut counts = vec![0u64; (d.faces + 1) as usize];
            for _ in 0..n {
                let roll = do_take(entropy, d.faces, take_count, max_takes, trace)?;
                counts[roll as usize] += 1;
            }

            let mut result = Vec::new();
            for face in 1..=d.faces {
                let c = counts[face as usize];
                if !exclude_zero || c > 0 {
                    result.push(DiceValue::List(vec![
                        DiceValue::Integer(face),
                        DiceValue::Integer(c),
                    ]));
                }
            }
            Ok(DiceValue::List(result))
        }
        DiceExpr::Choose { list, count } => {
            let val_list = eval_internal(list, entropy, take_count, max_takes, trace)?;
            let mut items = val_list.into_list()?;
            let val_count = eval_internal(count, entropy, take_count, max_takes, trace)?;
            let k = val_count.as_integer()? as usize;

            if k > items.len() {
                return Err(EvalError::InvalidArguments(
                    "cannot choose more items than available in list".to_string(),
                ));
            }

            let mut chosen = Vec::with_capacity(k);
            for _ in 0..k {
                let remaining_len = items.len() as Integer;
                let roll = do_take(entropy, remaining_len, take_count, max_takes, trace)?;
                let item = items.remove((roll - 1) as usize);
                chosen.push(item);
            }
            Ok(DiceValue::List(chosen))
        }
        DiceExpr::Lookup { table, key } => {
            let val_table = eval_internal(table, entropy, take_count, max_takes, trace)?;
            let pairs = val_table.into_list()?;
            let val_key = eval_internal(key, entropy, take_count, max_takes, trace)?;
            let target_key = val_key.as_integer()?;

            for pair in pairs {
                let sub = pair.as_list()?;
                if sub.len() == 2 && sub[0].as_integer() == Ok(target_key) {
                    return Ok(sub[1].clone());
                }
            }
            Err(EvalError::LookupNotFound(target_key))
        }
        DiceExpr::Repeat { count, expr } => {
            let val_count = eval_internal(count, entropy, take_count, max_takes, trace)?;
            let n = val_count.as_integer()?;
            let mut results = Vec::with_capacity(n as usize);
            for _ in 0..n {
                results.push(eval_internal(expr, entropy, take_count, max_takes, trace)?);
            }
            Ok(DiceValue::List(results))
        }
    }
}
