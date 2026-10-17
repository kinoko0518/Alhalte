//! Error types for Emoklore extension.

use thiserror::Error;

/// Errors that can occur during Emoklore processing.
#[derive(Debug, Error)]
pub enum EmokloreError {
    /// Parsing failed.
    #[error("Emoklore parse error: {0}")]
    Parse(String),

    /// Generast operation failed.
    #[error("generast error: {0}")]
    Generast(#[from] generast::GenerastError),
}
