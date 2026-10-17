//! Error types for Call of Cthulhu 7th edition extension.

use thiserror::Error;

/// Errors that can occur during CoC 7th edition processing.
#[derive(Debug, Error)]
pub enum Coc7Error {
    /// Parsing failed.
    #[error("CoC 7th edition parse error: {0}")]
    Parse(String),

    /// Generast operation failed.
    #[error("generast error: {0}")]
    Generast(#[from] generast::GenerastError),
}
