//! Error type for `alhalte-cafe`.

use alhalte_restaurant::RestaurantError;
use thiserror::Error;

/// Result type alias for `alhalte-cafe`.
pub type Result<T> = std::result::Result<T, CafeError>;

/// Errors that can occur while preparing or running a local Alhalte instance.
#[derive(Debug, Error)]
pub enum CafeError {
    /// Configuration, secret, or argument-generation error.
    #[error(transparent)]
    Restaurant(#[from] RestaurantError),

    /// I/O error while reading the configuration or preparing directories.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The one-shot launcher could not prepare or run the instance.
    #[error("cafe error: {0}")]
    Cafe(String),
}
