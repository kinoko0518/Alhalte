//! # tin
//!
//! `tin` is a library crate for handling self-typed binary containers in the Alhalte ecosystem.
//!
//! A tin consists of:
//! - A 1-byte **opener** encoding encryption (upper 4 bits) and compression (lower 4 bits).
//! - An optional 12-byte **nonce** (if encryption is `0010` / RoomKey).
//! - An encrypted, compressed **body** wrapping the plaintext.
//!
//! The plaintext format is:
//! ```text
//! plaintext = label(8B, u64 LE) || payload
//! ```
//! where `label` is the XXH64 hash of a format identifier string (seed 0).
//!
//! # Example
//!
//! ```
//! use tin::{Compression, Encryption, Tin};
//!
//! let original = Tin::from_string("Hello, Alhalte!");
//! let (wire_bytes, tin_id) = original.encode_plaintext(Compression::Zstd).unwrap();
//!
//! let (decoded, opener, decoded_id) = Tin::decode(&wire_bytes, None).unwrap();
//! assert_eq!(decoded.as_string().unwrap(), "Hello, Alhalte!");
//! assert_eq!(opener.encryption, Encryption::Plaintext);
//! assert_eq!(opener.compression, Compression::Zstd);
//! assert_eq!(tin_id, decoded_id);
//! ```

#![forbid(unsafe_code)]

pub mod codec;
pub mod error;
pub mod tin;

pub use codec::{
    EncodedRaw, EncryptionParam, decode, decode_raw_plaintext_bytes, encode_raw_convergent,
    encode_raw_plaintext, encode_raw_plaintext_bytes, encode_raw_room_key,
};
pub use error::TinError;
pub use tin::Tin;
pub use tin::label::*;

/// 256-bit identifier uniquely identifying a tin.
pub type TinId = [u8; 32];
