//! Opener definition and related enums for encryption and compression metadata.

use crate::error::TinError;
use serde::{Deserialize, Serialize};

/// Encryption modes supported by the `tin` format (upper 4 bits of the Opener).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Encryption {
    /// `0000`: Plaintext (unencrypted payload).
    Plaintext = 0x0,
    /// `0001`: Convergent encryption with Zero Nonce (`[0u8; 12]`) and AES-256-GCM-SIV.
    Convergent = 0x1,
    /// `0010`: Room key encryption with random 96-bit Nonce and AES-256-GCM-SIV.
    RoomKey = 0x2,
}

impl Encryption {
    /// Attempts to parse an encryption mode from a 4-bit nibble.
    pub fn try_from_u4(nibble: u8) -> Result<Self, TinError> {
        match nibble {
            0x0 => Ok(Self::Plaintext),
            0x1 => Ok(Self::Convergent),
            0x2 => Ok(Self::RoomKey),
            other => Err(TinError::UnsupportedEncryption(other)),
        }
    }

    /// Returns the 4-bit representation of the encryption mode.
    pub const fn to_u4(self) -> u8 {
        self as u8
    }

    /// Returns whether this encryption mode includes a 12-byte Nonce in the wire format.
    ///
    /// Only `RoomKey` (`0010`) includes a nonce directly in the wire format.
    pub const fn has_wire_nonce(self) -> bool {
        matches!(self, Self::RoomKey)
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

    /// Parses an `Opener` from a single byte.
    pub fn from_byte(b: u8) -> Result<Self, TinError> {
        let enc_nibble = (b >> 4) & 0x0F;
        let comp_nibble = b & 0x0F;

        let encryption = Encryption::try_from_u4(enc_nibble)?;
        let compression = Compression::try_from_u4(comp_nibble)?;

        Ok(Self {
            encryption,
            compression,
        })
    }

    /// Serializes this `Opener` into a single byte.
    pub const fn to_byte(self) -> u8 {
        (self.encryption.to_u4() << 4) | self.compression.to_u4()
    }
}

impl TryFrom<u8> for Opener {
    type Error = TinError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        Self::from_byte(b)
    }
}

impl From<Opener> for u8 {
    fn from(opener: Opener) -> Self {
        opener.to_byte()
    }
}
