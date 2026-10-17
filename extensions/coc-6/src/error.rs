//! Error types for CoC 6th edition extension.

use thiserror::Error;

/// Errors that can occur during CoC 6 judgment parsing or processing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Coc6Error {
    /// Parsing error.
    #[error("coc-6 parse error: {0}")]
    Parse(String),
    /// Generast or Tin serialization error.
    #[error("generast error: {0}")]
    Generast(String),
}

impl From<generast::GenerastError> for Coc6Error {
    fn from(err: generast::GenerastError) -> Self {
        Self::Generast(err.to_string())
    }
}
