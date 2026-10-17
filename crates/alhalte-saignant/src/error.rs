//! Error types for the `alhalte-saignant` crate.

use thiserror::Error;

/// Result type alias for operations in `alhalte-saignant`.
pub type SaignantResult<T> = Result<T, SaignantError>;

/// Errors that can occur during log interpretation, AST rendering, or style processing.
#[derive(Debug, Error)]
pub enum SaignantError {
    /// Error originating from the `tin` crate.
    #[error("tin container error: {0}")]
    Tin(#[from] tin::TinError),

    /// Error originating from the `generast` crate.
    #[error("generast AST error: {0}")]
    Generast(#[from] generast::GenerastError),

    /// Error occurred during postcard deserialization.
    #[error("postcard deserialization error: {0}")]
    Postcard(#[from] postcard::Error),

    /// Error occurred during dice expression evaluation.
    #[error("dice evaluation error: {0}")]
    Eval(#[from] generast::EvalError),

    /// Error occurred during entropy sampling.
    #[error("entropy error: {0}")]
    Entropy(#[from] generast::EntropyError),

    /// Payload contains invalid UTF-8 string data.
    #[error("invalid UTF-8 data: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    /// Invalid or malformed structured data payload.
    #[error("invalid data: {0}")]
    InvalidData(String),
}
