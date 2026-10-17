//! Error types for `alhalte-canner`.

use thiserror::Error;
use tin::{Encryption, TinError};

/// Result alias for operations in `alhalte-canner`.
pub type Result<T, E = CannerError> = std::result::Result<T, E>;

/// Error type encompassing all failures that can occur in `alhalte-canner`.
#[derive(Debug, Error)]
pub enum CannerError {
    /// Failure during image decoding.
    #[error("image decode error: {0}")]
    ImageDecode(#[from] image::ImageError),

    /// Failure during WebP image encoding.
    #[error("webp encode error: {0}")]
    WebpEncode(#[from] oxideav_webp::WebpError),

    /// Input image has invalid dimensions (e.g. zero width or height).
    #[error("invalid image dimensions: {width}x{height}")]
    InvalidImageDimensions {
        /// Width of the invalid image.
        width: u32,
        /// Height of the invalid image.
        height: u32,
    },

    /// Failure during audio decoding.
    #[error("audio decode error: {0}")]
    AudioDecode(String),

    /// No audio track was found in the media stream.
    #[error("no audio track found in media")]
    NoAudioTrack,

    /// Failure during Opus or Ogg audio encoding.
    #[error("audio encode error: {0}")]
    AudioEncode(String),

    /// Invalid ISO/IEC 7816-4 padding or unpadding failure.
    #[error("invalid ISO/IEC 7816-4 padding")]
    InvalidPadding,

    /// Unexpected encryption mode encountered.
    #[error("unexpected encryption mode: {0:?}")]
    InvalidEncryption(Encryption),

    /// Invalid wrapped key length.
    #[error("invalid wrapped key length: expected at least 28 bytes, got {0}")]
    InvalidWrappedKeyLength(usize),

    /// Invalid convergent key length.
    #[error("invalid convergent key length: expected 32 bytes, got {0}")]
    InvalidConvergentKeyLength(usize),

    /// Underlying tin container error.
    #[error("tin container error: {0}")]
    Tin(#[from] TinError),

    /// Random number generation failure.
    #[error("random number generation error: {0}")]
    Rng(String),
}

impl From<getrandom::Error> for CannerError {
    fn from(e: getrandom::Error) -> Self {
        CannerError::Rng(e.to_string())
    }
}
