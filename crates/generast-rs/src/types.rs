//! Core data types and structured payload types.

use serde::{Deserialize, Serialize};

use crate::ast::Generast;
use crate::dice::DiceExpr;
use crate::entropy::Integer;
use crate::error::EvalError;
use crate::rational::Rational;

/// A die specification with a given number of faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dice {
    /// Number of faces.
    pub faces: Integer,
}

impl Dice {
    /// Creates a new `Dice` with `faces` faces.
    pub const fn new(faces: Integer) -> Self {
        Self { faces }
    }
}

/// Sort mode for D66 rolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SortMode {
    /// No sorting; tens digit is first roll, ones digit is second roll.
    None,
    /// Ascending sort; smaller roll becomes tens digit.
    Asc,
    /// Descending sort; larger roll becomes tens digit.
    Desc,
}

/// Binary operation for fold aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FoldOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
}

/// Record of a single die roll outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RollRecord {
    /// Number of faces of the rolled die.
    pub faces: Integer,
    /// The rolled outcome (`1..=faces`).
    pub roll: Integer,
}

/// Dynamic value resulting from evaluating a dice expression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiceValue {
    /// Unsigned integer value.
    Integer(Integer),
    /// Arbitrary-precision rational number.
    Rational(Rational),
    /// Boolean value.
    Bool(bool),
    /// UTF-8 string.
    String(String),
    /// Dice definition.
    Dice(Dice),
    /// Ordered list of values.
    List(Vec<DiceValue>),
}

impl DiceValue {
    /// Extracts the integer value or returns [`EvalError::TypeMismatch`].
    pub fn as_integer(&self) -> Result<Integer, EvalError> {
        match self {
            Self::Integer(i) => Ok(*i),
            _ => Err(EvalError::TypeMismatch {
                expected: "Integer",
                actual: self.type_name(),
            }),
        }
    }

    /// Extracts the rational value, converting integer to rational if necessary.
    pub fn as_rational(&self) -> Result<Rational, EvalError> {
        match self {
            Self::Rational(r) => Ok(r.clone()),
            Self::Integer(i) => Ok(Rational::from_integer(*i)),
            _ => Err(EvalError::TypeMismatch {
                expected: "Rational",
                actual: self.type_name(),
            }),
        }
    }

    /// Extracts the boolean value or returns [`EvalError::TypeMismatch`].
    pub fn as_bool(&self) -> Result<bool, EvalError> {
        match self {
            Self::Bool(b) => Ok(*b),
            _ => Err(EvalError::TypeMismatch {
                expected: "Bool",
                actual: self.type_name(),
            }),
        }
    }

    /// Extracts the string slice or returns [`EvalError::TypeMismatch`].
    pub fn as_str(&self) -> Result<&str, EvalError> {
        match self {
            Self::String(s) => Ok(s.as_str()),
            _ => Err(EvalError::TypeMismatch {
                expected: "String",
                actual: self.type_name(),
            }),
        }
    }

    /// Extracts the dice value or returns [`EvalError::TypeMismatch`].
    pub fn as_dice(&self) -> Result<Dice, EvalError> {
        match self {
            Self::Dice(d) => Ok(*d),
            _ => Err(EvalError::TypeMismatch {
                expected: "Dice",
                actual: self.type_name(),
            }),
        }
    }

    /// Extracts the list reference or returns [`EvalError::TypeMismatch`].
    pub fn as_list(&self) -> Result<&[DiceValue], EvalError> {
        match self {
            Self::List(l) => Ok(l.as_slice()),
            _ => Err(EvalError::TypeMismatch {
                expected: "List",
                actual: self.type_name(),
            }),
        }
    }

    /// Consumes this value and returns the inner list or returns [`EvalError::TypeMismatch`].
    pub fn into_list(self) -> Result<Vec<DiceValue>, EvalError> {
        match self {
            Self::List(l) => Ok(l),
            _ => Err(EvalError::TypeMismatch {
                expected: "List",
                actual: self.type_name(),
            }),
        }
    }

    /// Returns the static name of this value's type.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Integer(_) => "Integer",
            Self::Rational(_) => "Rational",
            Self::Bool(_) => "Bool",
            Self::String(_) => "String",
            Self::Dice(_) => "Dice",
            Self::List(_) => "List",
        }
    }
}

/// Structured payload for `alhalte/generast/diceroll`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DicerollData {
    /// 256-bit seed used to initialize entropy.
    pub seed: [u8; 32],
    /// Dice syntax tree.
    pub expr: DiceExpr,
}

/// Structured payload for `alhalte/generast/judgement`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JudgementData {
    /// Degree of success: negative for failure, 0 for neutral, positive for success.
    pub degree: i8,
    /// Human-readable game-system-specific judgment level name (e.g. "Critical", "Success").
    pub level_name: String,
    /// Subject of the judgment (e.g. skill name).
    pub subject: Option<String>,
    /// Target value for the judgment.
    pub target: Option<Rational>,
    /// Dice roll syntax tree or result.
    pub roll: Generast,
}

/// Structured payload for `alhalte/generast/fluct` (`(character_id, status, delta)`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FluctData {
    /// Target character identifier.
    pub character_id: u64,
    /// Name of status or parameter being modified.
    pub status: String,
    /// Delta applied to the status.
    pub delta: i32,
}

/// Structured payload for `alhalte/generast/assign` (`(character_id, status, value)`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssignData {
    /// Target character identifier.
    pub character_id: u64,
    /// Name of status or parameter being assigned.
    pub status: String,
    /// Absolute value assigned.
    pub value: i32,
}
