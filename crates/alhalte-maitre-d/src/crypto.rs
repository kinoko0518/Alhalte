//! Salting and hashing of Google account `sub` identifiers.
//!
//! A Google `sub` is a stable identifier for a human being. `alhalte-maitre-d`
//! never stores it: only a 128-bit BLAKE3 keyed hash of it, keyed by the
//! instance salt, is persisted (Issue #190). The raw identifier is zeroized
//! right after hashing, and the hash itself is redacted from `Debug` output and
//! never serialized into an HTTP response.

use std::fmt;

use zeroize::Zeroize;

use crate::error::{MaitreDError, Result};

/// Length of a [`SubHash`] in bytes (128 bits).
pub const SUB_HASH_BYTES: usize = 16;

/// Length of the lowercase hex form of a [`SubHash`] as stored in SQLite.
pub const SUB_HASH_HEX_LEN: usize = SUB_HASH_BYTES * 2;

/// Domain separator for the instance salt → hashing key derivation.
const SUB_KEY_CONTEXT: &[u8] = b"alhalte/maitre-d/sub-key-derivation";

/// A 128-bit, salt-keyed BLAKE3 hash of a Google `sub`.
///
/// This is the only form of a Google identity that `maitre-d` persists. Because
/// the hash is keyed by the instance salt, the same `sub` maps to the same
/// value on one restaurant and to unrelated values on another. There is
/// deliberately no inverse operation: the stored value cannot be turned back
/// into the original `sub`.
///
/// The hash is still a stable account correlator, so it is redacted from
/// [`Debug`](std::fmt::Debug) output and is not serializable: it must not reach
/// a client, a log line, or an error message.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubHash([u8; SUB_HASH_BYTES]);

impl SubHash {
    /// Returns the raw 128 bits.
    pub fn as_bytes(&self) -> &[u8; SUB_HASH_BYTES] {
        &self.0
    }

    /// Returns the 32-character lowercase hex form stored in SQLite.
    pub fn as_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parses the 32-character hex form stored in SQLite.
    ///
    /// # Errors
    ///
    /// Returns [`MaitreDError::InvalidSubHash`] when `value` is not exactly 32
    /// hex characters. The error never echoes the offending value.
    pub fn from_hex(value: &str) -> Result<Self> {
        let bytes = hex::decode(value).map_err(|_| MaitreDError::InvalidSubHash)?;
        let bytes: [u8; SUB_HASH_BYTES] =
            bytes.try_into().map_err(|_| MaitreDError::InvalidSubHash)?;
        Ok(Self(bytes))
    }
}

impl fmt::Debug for SubHash {
    /// Renders the hash as redacted: `sub_hash` must never appear in logs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SubHash(<redacted>)")
    }
}

/// Derives the 256-bit BLAKE3 keying key from the instance salt.
fn derive_sub_key(salt: &str) -> [u8; 32] {
    let context = blake3::hash(SUB_KEY_CONTEXT);
    let mut hasher = blake3::Hasher::new_keyed(context.as_bytes());
    hasher.update(salt.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Hashes a Google `sub` with the instance salt and zeroizes the input.
///
/// The first 128 bits of the BLAKE3 keyed hash are kept; 128 bits is far beyond
/// any collision or preimage concern for a per-instance account correlator, and
/// it keeps the stored value short. `sub` is overwritten with zeros before this
/// function returns, so the caller must not use it afterwards.
///
/// spec: alhalte-maitre-d/crypto 0001 sub is stored as a salted 128-bit hash
pub fn hash_sub(salt: &str, sub: &mut String) -> SubHash {
    let mut key = derive_sub_key(salt);
    let mut hasher = blake3::Hasher::new_keyed(&key);
    hasher.update(sub.as_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0u8; SUB_HASH_BYTES];
    bytes.copy_from_slice(&digest.as_bytes()[..SUB_HASH_BYTES]);

    key.zeroize();
    sub.zeroize();

    SubHash(bytes)
}
