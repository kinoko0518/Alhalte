//! Error types for `alhalte-cellar`.

use thiserror::Error;

/// Result alias for operations in `alhalte-cellar`.
pub type Result<T, E = CellarError> = std::result::Result<T, E>;

/// Errors that can occur within the cellar service.
#[derive(Debug, Error)]
pub enum CellarError {
    /// IO errors when reading or writing tin files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// SQLite database errors.
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Errors originating from the `tin` crate.
    #[error("Tin error: {0}")]
    Tin(#[from] tin::TinError),

    /// Session authentication or authorization failure.
    #[error("Invalid session for owner {owner} (session_id: {session_id})")]
    Unauthorized {
        /// Owner account ID.
        owner: u64,
        /// Session ID.
        session_id: u64,
    },

    /// Session verifier connection or query failure.
    #[error("Session verifier error: {0}")]
    Verifier(String),

    /// Invalid or incomplete service configuration detected at startup.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Requested tin was not found in storage or database.
    #[error("Tin with ID {0} was not found")]
    NotFound(String),

    /// Tin is already owned by the specified owner.
    #[error("Tin with ID {tin_id} is already owned by owner {owner}")]
    AlreadyOwned {
        /// Hex-encoded tin ID.
        tin_id: String,
        /// Owner account ID.
        owner: u64,
    },

    /// Hex decoding error.
    #[error("Hex decode error: {0}")]
    Hex(#[from] hex::FromHexError),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The request body did not satisfy the cellar contract.
    ///
    /// Raised for a JSON body that carries an unknown field — including the
    /// legacy `owner` / `session_id` claims removed in Issue #191 — so the
    /// answer is a plain 400 instead of Axum's default 422.
    #[error("Invalid request body: {0}")]
    InvalidBody(String),

    /// The uploaded data size did not match the declared size.
    #[error("Size mismatch: declared {expected} bytes, received {actual} bytes")]
    SizeMismatch {
        /// Expected size.
        expected: u64,
        /// Actual size received.
        actual: u64,
    },

    /// General internal server error.
    #[error("Internal cellar error: {0}")]
    Internal(String),
}
