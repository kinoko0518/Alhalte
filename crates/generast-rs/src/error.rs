//! Error types for `generast`.

use thiserror::Error;

/// Top-level error type for generast operations.
#[derive(Debug, Error)]
pub enum GenerastError {
    /// Postcard serialization or deserialization error.
    #[error("postcard error: {0}")]
    Postcard(#[from] postcard::Error),

    /// Tin error.
    #[error("tin error: {0}")]
    Tin(#[from] tin::TinError),

    /// Label mismatch when decoding a tin.
    #[error("label mismatch: expected {expected:#x}, got {actual:#x}")]
    LabelMismatch {
        /// Expected label hash.
        expected: u64,
        /// Actual label hash found.
        actual: u64,
    },

    /// Invalid payload length for a fixed-size payload.
    #[error("invalid payload length: expected {expected}, got {actual}")]
    InvalidPayloadLength {
        /// Expected byte length.
        expected: usize,
        /// Actual byte length.
        actual: usize,
    },

    /// Invalid UTF-8 encoding in string payload.
    #[error("invalid utf-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    /// Evaluation error.
    #[error("dice evaluation error: {0}")]
    Eval(#[from] EvalError),

    /// Other invalid data.
    #[error("invalid data: {0}")]
    InvalidData(String),
}

/// Errors originating from entropy sources.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EntropyError {
    /// Attempted to roll a die with 0 faces.
    #[error("cannot roll a die with 0 faces")]
    ZeroFaces,

    /// Custom or external entropy error.
    #[error("entropy error: {0}")]
    Custom(String),
}

/// Errors occurring during dice expression evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EvalError {
    /// Division by zero on rational numbers.
    #[error("division by zero")]
    DivisionByZero,

    /// Number out of representable integer range (`u64`).
    #[error("integer out of range")]
    IntegerOutOfRange,

    /// Error produced by entropy source.
    #[error("entropy error: {0}")]
    Entropy(#[from] EntropyError),

    /// `DiceUntil` predicate would run forever on all faces of the die.
    #[error(
        "unstoppable DiceUntil predicate detected: all faces satisfy the continuation condition"
    )]
    UnstoppableDiceUntil,

    /// Total number of `Take` operations exceeded the limit.
    #[error("take limit exceeded: maximum allowed takes is {limit}")]
    TakeLimitExceeded {
        /// Maximum allowed take count.
        limit: usize,
    },

    /// Type mismatch in expression evaluation.
    #[error("type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        /// Expected type name.
        expected: &'static str,
        /// Actual type name.
        actual: &'static str,
    },

    /// Key not found during table lookup.
    #[error("lookup key {0} not found in table")]
    LookupNotFound(u64),

    /// Invalid arguments provided to an operation.
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),

    /// Operation requested a non-empty list but received an empty list.
    #[error("unexpected empty list")]
    EmptyList,

    /// List index out of bounds.
    #[error("index {index} out of bounds for length {length}")]
    IndexOutOfBounds {
        /// Requested index.
        index: u64,
        /// Length of list.
        length: u64,
    },
}
