//! Error definitions for the `tin` crate.

use thiserror::Error;

/// Error type for operations in the `tin` crate.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TinError {
    /// Opener contains an unsupported encryption code.
    #[error("unsupported encryption: {0:#04x}")]
    UnsupportedEncryption(u8),

    /// Opener contains an unsupported compression code.
    #[error("unsupported compression: {0:#04x}")]
    UnsupportedCompression(u8),

    /// Wire format byte buffer is too short.
    #[error("wire format too short: expected at least {expected} bytes, got {actual}")]
    WireTooShort {
        /// Expected minimum length.
        expected: usize,
        /// Actual length received.
        actual: usize,
    },

    /// Plaintext byte buffer is too short (needs at least 8 bytes for the label).
    #[error("plaintext too short: expected at least 8 bytes, got {0}")]
    PlaintextTooShort(usize),

    /// Required decryption key was not provided.
    #[error("missing decryption key")]
    MissingKey,

    /// Decryption or authentication failed.
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    /// Encryption operation failed.
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    /// Convergent encryption key derived from plaintext does not match provided key.
    #[error("convergent key mismatch")]
    KeyMismatch,

    /// Tin ID does not match expected value.
    #[error("tin ID mismatch")]
    TinIdMismatch,

    /// Compression failed.
    #[error("compression error: {0}")]
    Compression(String),

    /// Decompression failed.
    #[error("decompression error: {0}")]
    Decompression(String),

    /// Payload is not valid UTF-8.
    #[error("invalid utf-8 string: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    /// Payload length does not match the expected length for this type.
    #[error("invalid payload length: expected {expected} bytes, got {actual}")]
    InvalidPayloadLength {
        /// Expected payload length.
        expected: usize,
        /// Actual payload length.
        actual: usize,
    },

    /// Label does not match the expected label for this conversion.
    #[error("label mismatch: expected {expected:#018x}, got {actual:#018x}")]
    LabelMismatch {
        /// Expected label hash.
        expected: u64,
        /// Actual label hash.
        actual: u64,
    },

    /// Byte representation of boolean is neither 0 nor 1.
    #[error("invalid bool byte: {0:#04x}")]
    InvalidBool(u8),

    /// Postcard serialization or deserialization failed.
    #[error("postcard error: {0}")]
    Postcard(#[from] postcard::Error),
}
