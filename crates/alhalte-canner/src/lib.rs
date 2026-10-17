//! # alhalte-canner
//!
//! Client-side deterministic asset re-encoding and encryption module for Alhalte.
//!
//! Provides deterministic image downscaling to WebP (lossy, quality 80),
//! audio resampling to 48kHz Opus (Ogg container, 128kbps), PADMÉ size-obfuscating
//! low-entropy encryption (`0010`), and convergent high-entropy encryption (`0001`) with
//! room-key wrapped convergent keys.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod encryption;
pub mod error;
pub mod image;
pub mod padding;
pub mod sound;

pub use encryption::{
    HighEntropyOutput, decrypt_high_entropy, decrypt_low_entropy, encrypt_high_entropy,
    encrypt_high_entropy_random, encrypt_low_entropy, encrypt_low_entropy_random,
};
pub use error::{CannerError, Result};
pub use image::{MAX_IMAGE_SIDE, WEBP_LOSSY_QUALITY, encode_image};
pub use padding::{pad_iso7816_4_padme, padme_padded_len, unpad_iso7816_4_padme};
pub use sound::{OPUS_FRAME_SIZE, TARGET_BITRATE_BPS, TARGET_SAMPLE_RATE, encode_sound};
