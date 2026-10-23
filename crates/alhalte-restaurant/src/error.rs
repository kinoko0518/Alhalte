//! Error types for `alhalte-restaurant`.

use thiserror::Error;

/// Result type alias for `alhalte-restaurant`.
pub type Result<T> = std::result::Result<T, RestaurantError>;

/// Errors that can occur while reading, decrypting, or validating
/// `Restaurant.toml`.
#[derive(Debug, Error)]
pub enum RestaurantError {
    /// I/O error when reading or writing configuration, secret, or staged files.
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

    /// An age-armored value needs an identity, but none was configured.
    ///
    /// Pass `--age-key`, `ROPS_AGE`, or `ALHALTE_AGE_KEY`.
    #[error(
        "age identity is required to decrypt armored values in Restaurant.toml; \
         pass --age-key, ROPS_AGE, or ALHALTE_AGE_KEY"
    )]
    AgeKeyMissing,

    /// The configured age identity string is not a valid x25519 identity.
    #[error("invalid age identity: {0}")]
    AgeIdentity(String),

    /// Decryption of an age-armored value failed.
    #[error("age decryption error: {0}")]
    AgeDecrypt(String),

    /// Encryption with an age recipient failed.
    #[error("age encryption error: {0}")]
    AgeEncrypt(String),

    /// A secret value is still plaintext.
    ///
    /// Plaintext secrets are fail-closed: encrypt them with
    /// `alhalte-bistrot encrypt-value --recipient age1...` (Issue #194).
    /// `ALHALTE_ALLOW_PLAINTEXT_SECRETS=1` allows them for local development.
    #[error(
        "plaintext secret(s) in Restaurant.toml: {0}; encrypt them with \
         `alhalte-bistrot encrypt-value --recipient age1...` or set \
         ALHALTE_ALLOW_PLAINTEXT_SECRETS=1 for local development only"
    )]
    PlaintextSecret(String),

    /// A configured extension `.wasm` file does not exist.
    ///
    /// This is a `Restaurant.toml` configuration error (the path is wrong or the
    /// wasm was not built). Staging fails instead of publishing a
    /// `manifest.json` entry whose `url` would 404 in the browser.
    #[error("extension '{id}' wasm not found: {path}; build it or fix the path in Restaurant.toml")]
    ExtensionWasmMissing {
        /// Extension id from `[[extensions]]`.
        id: String,
        /// Path that was checked (configured path or `base_dir`-relative fallback).
        path: std::path::PathBuf,
    },
}
