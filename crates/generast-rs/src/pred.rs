//! Predicate expressions for conditional evaluation and filtering.

use serde::{Deserialize, Serialize};

use crate::entropy::Integer;
use crate::rational::Rational;

/// Predicate expression representing comparison and termination conditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pred {
    /// Tests if value equals target.
    Eq(Rational),
    /// Tests if value is not equal to target.
    Ne(Rational),
    /// Tests if value is strictly less than target.
    Lt(Rational),
    /// Tests if value is less than or equal to target.
    Le(Rational),
    /// Tests if value is strictly greater than target.
    Gt(Rational),
    /// Tests if value is greater than or equal to target.
    Ge(Rational),
    /// Logical AND of two predicates.
    And(Box<Pred>, Box<Pred>),
    /// Logical OR of two predicates.
    Or(Box<Pred>, Box<Pred>),
    /// Logical NOT of a predicate.
    Not(Box<Pred>),
    /// Constantly true predicate.
    Always,
    /// Constantly false predicate.
    Never,
    /// Roll history predicate: true while history length is strictly less than `limit`.
    LengthLessThan(Integer),
    /// Roll history predicate: applies predicate to the last roll in history (true if empty).
    LastRoll(Box<Pred>),
}

impl Pred {
    /// Creates an equality predicate (`== val`).
    pub fn eq(val: impl Into<Rational>) -> Self {
        Self::Eq(val.into())
    }

    /// Creates a non-equality predicate (`!= val`).
    pub fn ne(val: impl Into<Rational>) -> Self {
        Self::Ne(val.into())
    }

    /// Creates a strictly less-than predicate (`< val`).
    pub fn lt(val: impl Into<Rational>) -> Self {
        Self::Lt(val.into())
    }

    /// Creates a less-than-or-equal predicate (`<= val`).
    pub fn le(val: impl Into<Rational>) -> Self {
        Self::Le(val.into())
    }

    /// Creates a strictly greater-than predicate (`> val`).
    pub fn gt(val: impl Into<Rational>) -> Self {
        Self::Gt(val.into())
    }

    /// Creates a greater-than-or-equal predicate (`>= val`).
    pub fn ge(val: impl Into<Rational>) -> Self {
        Self::Ge(val.into())
    }

    /// Creates an `Always` predicate.
    pub fn always() -> Self {
        Self::Always
    }

    /// Creates a `Never` predicate.
    pub fn never() -> Self {
        Self::Never
    }

    /// Creates a `LengthLessThan` predicate.
    pub fn length_less_than(n: Integer) -> Self {
        Self::LengthLessThan(n)
    }

    /// Creates a `LastRoll` predicate testing the most recent roll.
    pub fn last_roll(inner: Self) -> Self {
        Self::LastRoll(Box::new(inner))
    }

    /// Combines `self` and `other` with logical AND.
    pub fn and(self, other: Self) -> Self {
        Self::And(Box::new(self), Box::new(other))
    }

    /// Combines `self` and `other` with logical OR.
    pub fn or(self, other: Self) -> Self {
        Self::Or(Box::new(self), Box::new(other))
    }

    /// Inverts `self` with logical NOT.
    pub fn negate(self) -> Self {
        !self
    }

    /// Inverts `self` with logical NOT (alias for [`negate`](Self::negate)).
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Self {
        !self
    }

    /// Evaluates this predicate against a single rational value.
    pub fn eval_rational(&self, val: &Rational) -> bool {
        match self {
            Self::Eq(target) => val == target,
            Self::Ne(target) => val != target,
            Self::Lt(target) => val < target,
            Self::Le(target) => val <= target,
            Self::Gt(target) => val > target,
            Self::Ge(target) => val >= target,
            Self::And(a, b) => a.eval_rational(val) && b.eval_rational(val),
            Self::Or(a, b) => a.eval_rational(val) || b.eval_rational(val),
            Self::Not(p) => !p.eval_rational(val),
            Self::Always => true,
            Self::Never => false,
            Self::LengthLessThan(_) => false,
            Self::LastRoll(p) => p.eval_rational(val),
        }
    }

    /// Evaluates this predicate against an integer value.
    pub fn eval_integer(&self, val: Integer) -> bool {
        self.eval_rational(&Rational::from_integer(val))
    }

    /// Evaluates this predicate against an accumulated list of dice rolls.
    pub fn eval_history(&self, history: &[Integer]) -> bool {
        match self {
            Self::LengthLessThan(limit) => (history.len() as u64) < *limit,
            Self::LastRoll(p) => match history.last() {
                Some(&last) => p.eval_integer(last),
                None => true,
            },
            Self::And(a, b) => a.eval_history(history) && b.eval_history(history),
            Self::Or(a, b) => a.eval_history(history) || b.eval_history(history),
            Self::Not(p) => !p.eval_history(history),
            Self::Always => true,
            Self::Never => false,
            Self::Eq(_) | Self::Ne(_) | Self::Lt(_) | Self::Le(_) | Self::Gt(_) | Self::Ge(_) => {
                match history.last() {
                    Some(&last) => self.eval_integer(last),
                    None => true,
                }
            }
        }
    }

    /// Checks if this predicate is unstoppable on a die with the given number of faces.
    ///
    /// An unstoppable predicate causes `DiceUntil` to loop infinitely because every possible
    /// face `1..=faces` satisfies the continuation condition and there is no terminating length limit.
    pub fn is_unstoppable_on_faces(&self, faces: Integer) -> bool {
        if faces == 0 {
            return false;
        }
        match self {
            Self::LengthLessThan(_) => false,
            Self::Never => false,
            Self::Always => true,
            Self::LastRoll(inner) => {
                if inner.is_unstoppable_on_faces(faces) {
                    true
                } else {
                    (1..=faces).all(|x| inner.eval_integer(x))
                }
            }
            Self::And(a, b) => a.is_unstoppable_on_faces(faces) && b.is_unstoppable_on_faces(faces),
            Self::Or(a, b) => {
                a.is_unstoppable_on_faces(faces)
                    || b.is_unstoppable_on_faces(faces)
                    || (1..=faces).all(|x| self.eval_integer(x))
            }
            Self::Not(p) => match &**p {
                Self::Never => true,
                Self::Always => false,
                _ => (1..=faces).all(|x| self.eval_integer(x)),
            },
            Self::Eq(_) | Self::Ne(_) | Self::Lt(_) | Self::Le(_) | Self::Gt(_) | Self::Ge(_) => {
                (1..=faces).all(|x| self.eval_integer(x))
            }
        }
    }
}

impl std::ops::Not for Pred {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self::Not(Box::new(self))
    }
}
