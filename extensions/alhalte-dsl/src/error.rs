//! Error types for Alhalte DSL.

use thiserror::Error;

/// Errors that can occur during DSL parsing or transformation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DslError {
    /// A syntax error in dice expression or DSL syntax.
    #[error("syntax error: {0}")]
    SyntaxError(String),
    /// Generast or Tin serialization error.
    #[error("generast error: {0}")]
    Generast(String),
}

impl From<generast::GenerastError> for DslError {
    fn from(err: generast::GenerastError) -> Self {
        Self::Generast(err.to_string())
    }
}
