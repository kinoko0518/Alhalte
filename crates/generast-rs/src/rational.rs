//! Arbitrary-precision rational number type.

use std::fmt;
use std::ops::{Add, Mul, Sub};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};

use crate::error::EvalError;

/// Arbitrary-precision rational number.
///
/// Wraps [`BigRational`], ensuring canonical normalized representation (irreducible fraction
/// with positive denominator). Division by zero returns [`EvalError::DivisionByZero`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rational(BigRational);

impl Rational {
    /// Creates a new rational number from a numerator and denominator.
    ///
    /// Returns [`EvalError::DivisionByZero`] if `denom == 0`.
    pub fn new(numer: impl Into<BigInt>, denom: impl Into<BigInt>) -> Result<Self, EvalError> {
        let denom_bi = denom.into();
        if denom_bi.is_zero() {
            return Err(EvalError::DivisionByZero);
        }
        Ok(Self(BigRational::new(numer.into(), denom_bi)))
    }

    /// Creates a rational from an unsigned integer (`u64`).
    pub fn from_integer(val: u64) -> Self {
        Self(BigRational::from_integer(BigInt::from(val)))
    }

    /// Creates a rational representation of zero (`0 / 1`).
    pub fn zero() -> Self {
        Self(BigRational::zero())
    }

    /// Creates a rational representation of one (`1 / 1`).
    pub fn one() -> Self {
        Self(BigRational::one())
    }

    /// Returns `true` if this rational is equal to zero.
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// Returns the underlying [`BigRational`].
    pub fn inner(&self) -> &BigRational {
        &self.0
    }

    /// Divides `self` by `other`, returning [`EvalError::DivisionByZero`] if `other == 0`.
    pub fn checked_div(&self, other: &Self) -> Result<Self, EvalError> {
        if other.is_zero() {
            return Err(EvalError::DivisionByZero);
        }
        Ok(Self(&self.0 / &other.0))
    }

    /// Computes the floor of this rational, projected to [`u64`].
    ///
    /// Returns [`EvalError::IntegerOutOfRange`] if the floored value is negative or exceeds [`u64::MAX`].
    pub fn floor(&self) -> Result<u64, EvalError> {
        let bi = self.0.floor().to_integer();
        bi.to_u64().ok_or(EvalError::IntegerOutOfRange)
    }

    /// Computes the ceiling of this rational, projected to [`u64`].
    ///
    /// Returns [`EvalError::IntegerOutOfRange`] if the ceiling value is negative or exceeds [`u64::MAX`].
    pub fn ceil(&self) -> Result<u64, EvalError> {
        let bi = self.0.ceil().to_integer();
        bi.to_u64().ok_or(EvalError::IntegerOutOfRange)
    }

    /// Computes the rounded value of this rational, defined as `Floor(Add(x, 1/2))`.
    ///
    /// Returns [`EvalError::IntegerOutOfRange`] if the rounded value is negative or exceeds [`u64::MAX`].
    pub fn round(&self) -> Result<u64, EvalError> {
        let half = Self(BigRational::new(BigInt::from(1), BigInt::from(2)));
        let sum = self + &half;
        sum.floor()
    }
}

impl From<u64> for Rational {
    fn from(val: u64) -> Self {
        Self::from_integer(val)
    }
}

impl From<u32> for Rational {
    fn from(val: u32) -> Self {
        Self::from_integer(val as u64)
    }
}

impl From<i64> for Rational {
    fn from(val: i64) -> Self {
        Self(BigRational::from_integer(BigInt::from(val)))
    }
}

impl From<i32> for Rational {
    fn from(val: i32) -> Self {
        Self(BigRational::from_integer(BigInt::from(val)))
    }
}

impl Add for &Rational {
    type Output = Rational;
    fn add(self, rhs: Self) -> Self::Output {
        Rational(&self.0 + &rhs.0)
    }
}

impl Add for Rational {
    type Output = Rational;
    fn add(self, rhs: Self) -> Self::Output {
        Rational(&self.0 + &rhs.0)
    }
}

impl Sub for &Rational {
    type Output = Rational;
    fn sub(self, rhs: Self) -> Self::Output {
        Rational(&self.0 - &rhs.0)
    }
}

impl Sub for Rational {
    type Output = Rational;
    fn sub(self, rhs: Self) -> Self::Output {
        Rational(&self.0 - &rhs.0)
    }
}

impl Mul for &Rational {
    type Output = Rational;
    fn mul(self, rhs: Self) -> Self::Output {
        Rational(&self.0 * &rhs.0)
    }
}

impl Mul for Rational {
    type Output = Rational;
    fn mul(self, rhs: Self) -> Self::Output {
        Rational(&self.0 * &rhs.0)
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_integer() {
            write!(f, "{}", self.0.numer())
        } else {
            write!(f, "{}/{}", self.0.numer(), self.0.denom())
        }
    }
}
