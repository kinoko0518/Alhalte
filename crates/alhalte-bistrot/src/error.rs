//! Error types for `alhalte-bistrot`.

use thiserror::Error;

/// Result type alias for `alhalte-bistrot`.
pub type Result<T> = std::result::Result<T, BistrotError>;

/// Errors that can occur during `alhalte-bistrot` configuration parsing, decryption, or generation.
#[derive(Debug, Error)]
pub enum BistrotError {
    /// I/O error when reading or writing configuration or systemd files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Error during TOML deserialization.
    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    /// Error during TOML serialization.
    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    /// Error during JSON serialization or deserialization.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Error during rops decryption.
    #[error("ROPS decryption error: {0}")]
    RopsDecrypt(String),

    /// Error during rops encryption.
    #[error("ROPS encryption error: {0}")]
    RopsEncrypt(String),

    /// Configuration validation error.
    #[error("Configuration validation error: {0}")]
    ConfigValidation(String),

    /// Unknown module name for argument generation.
    #[error("Unknown module name '{0}'; expected 'maitre-d', 'cellar', 'banquette', or 'all'")]
    UnknownModule(String),
}
