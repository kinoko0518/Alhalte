//! Opener definition and related enums for encryption and compression metadata.

mod compression;
mod encryption;

use crate::error::TinError;
use serde::{Deserialize, Serialize};

pub use compression::compress;
pub use encryption::{
    ConvergentKey, RoomKey, decrypt_aes_256_gcm_siv, derive_convergent_id_and_key,
    encrypt_aes_256_gcm_siv,
};

/// Encryption modes supported by the `tin` format (upper 4 bits of the Opener).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Encryption {
    /// `0000`: Plaintext (unencrypted payload).
    Plaintext = 0x0,
    /// `0001`: Convergent encryption with Zero Nonce (`[0u8; 12]`) and AES-256-GCM-SIV.
    Convergent = 0x1,
    /// `0010`: Room key encryption with random 96-bit Nonce and AES-256-GCM-SIV.
    RoomKey([u8; 12]) = 0x2,
}

impl Encryption {
    /// Returns the 4-bit representation of the encryption mode.
    pub const fn to_u4(self) -> u8 {
        match self {
            Self::Plaintext => 0,
            Self::Convergent => 1,
            Self::RoomKey(_) => 2,
        }
    }
}

/// Compression algorithms supported by the `tin` format (lower 4 bits of the Opener).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Compression {
    /// `0000`: No compression.
    None = 0x0,
    /// `0001`: Zstandard compression (level 19).
    Zstd = 0x1,
    /// `0010`: LZ4 block compression with prepended uncompressed size.
    Lz4 = 0x2,
    /// `0011`: Brotli compression (quality 11, lgwin 22).
    Brotli = 0x3,
}

impl Compression {
    /// Attempts to parse a compression algorithm from a 4-bit nibble.
    pub fn try_from_u4(nibble: u8) -> Result<Self, TinError> {
        match nibble {
            0x0 => Ok(Self::None),
            0x1 => Ok(Self::Zstd),
            0x2 => Ok(Self::Lz4),
            0x3 => Ok(Self::Brotli),
            other => Err(TinError::UnsupportedCompression(other)),
        }
    }

    /// Returns the 4-bit representation of the compression algorithm.
    pub const fn to_u4(self) -> u8 {
        self as u8
    }
}

/// 1-byte header describing encryption and compression algorithms of a tin.
///
/// The upper 4 bits specify the encryption mode, while the lower 4 bits
/// specify the compression algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Opener {
    /// Encryption mode.
    pub encryption: Encryption,
    /// Compression algorithm.
    pub compression: Compression,
}

impl Opener {
    /// Creates a new `Opener` with the specified encryption and compression settings.
    pub const fn new(encryption: Encryption, compression: Compression) -> Self {
        Self {
            encryption,
            compression,
        }
    }
}

impl From<Opener> for u8 {
    fn from(opener: Opener) -> Self {
        (opener.encryption.to_u4() << 4) | opener.compression.to_u4()
    }
}
