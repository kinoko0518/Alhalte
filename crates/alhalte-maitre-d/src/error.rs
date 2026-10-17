//! Error types for `alhalte-maitre-d`.

/// Result type alias for `alhalte-maitre-d` operations.
pub type Result<T> = std::result::Result<T, MaitreDError>;

/// Core error enumeration for `alhalte-maitre-d`.
#[derive(Debug, thiserror::Error)]
pub enum MaitreDError {
    /// SQLite database error.
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// I/O error during file or socket operation.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Google OAuth error.
    #[error("OAuth error: {0}")]
    OAuth(String),

    /// Cryptographic or encryption error.
    #[error("Cryptographic error: {0}")]
    Crypto(String),

    /// Unauthorized access or invalid session.
    #[error("Unauthorized: invalid account ID or session ID")]
    Unauthorized,

    /// Account not found.
    #[error("Account not found: {0}")]
    AccountNotFound(u64),

    /// Session not found.
    #[error("Session not found: {0}")]
    SessionNotFound(u64),

    /// Banquette room process communication error.
    #[error("Banquette communication error: {0}")]
    Banquette(String),

    /// Serialization or deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Invalid Tin container format or payload mismatch.
    #[error("Invalid tin format: {0}")]
    InvalidTin(String),

    /// Invalid request parameters.
    #[error("Bad request: {0}")]
    BadRequest(String),
}
