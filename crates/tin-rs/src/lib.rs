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
#![warn(missing_docs)]

pub mod codec;
pub mod compression;
pub mod encryption;
pub mod error;
pub mod label;
pub mod opener;
pub mod tin;

pub use codec::{
    EncodedRaw, EncryptionParam, decode, decode_raw_plaintext_bytes, encode_raw_convergent,
    encode_raw_plaintext, encode_raw_plaintext_bytes, encode_raw_room_key,
};
pub use compression::{compress, decompress};
pub use encryption::{
    TinId, ZERO_NONCE, decrypt_aes_256_gcm_siv, derive_convergent_id_and_key,
    encrypt_aes_256_gcm_siv, verify_convergent,
};
pub use error::TinError;
pub use label::{
    IDENT_ALHALTE_TIN_ID, IDENT_ALHALTE_UNIX_TIME, IDENT_PRIMITIVE_BOOL, IDENT_PRIMITIVE_F16,
    IDENT_PRIMITIVE_F32, IDENT_PRIMITIVE_F64, IDENT_PRIMITIVE_I8, IDENT_PRIMITIVE_I16,
    IDENT_PRIMITIVE_I32, IDENT_PRIMITIVE_I64, IDENT_PRIMITIVE_I128, IDENT_PRIMITIVE_STRING,
    IDENT_PRIMITIVE_U8, IDENT_PRIMITIVE_U16, IDENT_PRIMITIVE_U32, IDENT_PRIMITIVE_U64,
    IDENT_PRIMITIVE_U128, LABEL_ALHALTE_TIN_ID, LABEL_ALHALTE_UNIX_TIME, LABEL_NEED_NOT_KNOW,
    LABEL_PRIMITIVE_BOOL, LABEL_PRIMITIVE_F16, LABEL_PRIMITIVE_F32, LABEL_PRIMITIVE_F64,
    LABEL_PRIMITIVE_I8, LABEL_PRIMITIVE_I16, LABEL_PRIMITIVE_I32, LABEL_PRIMITIVE_I64,
    LABEL_PRIMITIVE_I128, LABEL_PRIMITIVE_STRING, LABEL_PRIMITIVE_U8, LABEL_PRIMITIVE_U16,
    LABEL_PRIMITIVE_U32, LABEL_PRIMITIVE_U64, LABEL_PRIMITIVE_U128, Label, label_hash,
};
pub use opener::{Compression, Encryption, Opener};
pub use tin::Tin;
