//! Error types for Markdown extension.

use thiserror::Error;

/// Errors that can occur during Markdown parsing or AST processing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MarkdownError {
    /// Parsing error.
    #[error("markdown parse error: {0}")]
    Parse(String),
    /// Generast error.
    #[error("generast error: {0}")]
    Generast(String),
}

impl From<generast::GenerastError> for MarkdownError {
    fn from(err: generast::GenerastError) -> Self {
        Self::Generast(err.to_string())
    }
}
