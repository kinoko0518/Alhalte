//! Error types for Aozora extension.

use thiserror::Error;

/// Errors that can occur during Aozora parsing or processing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AozoraError {
    /// Tokenizer or parser error.
    #[error("aozora parser error: {0}")]
    Parse(String),
    /// Generast or Tin serialization error.
    #[error("generast error: {0}")]
    Generast(String),
}

impl From<generast::GenerastError> for AozoraError {
    fn from(err: generast::GenerastError) -> Self {
        Self::Generast(err.to_string())
    }
}
