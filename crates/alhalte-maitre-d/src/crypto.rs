//! Cryptographic primitives and deterministic AEAD encryption for Google account `sub` identifiers.

use aes_gcm_siv::aead::{Aead, KeyInit};
use aes_gcm_siv::{Aes256GcmSiv, Key, Nonce};

use crate::error::{MaitreDError, Result};

/// Special ciphertext identifier used for debug accounts in debug builds.
pub const DEBUG_SUB_CIPHERTEXT: &str = "0";

/// Zero nonce (`[0u8; 12]`) used for deterministic AEAD encryption of `sub`.
const ZERO_NONCE: [u8; 12] = [0u8; 12];

/// Derives a 256-bit AES-GCM-SIV encryption key from an instance salt.
fn derive_sub_key(salt: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(
        &blake3::hash(b"alhalte/maitre-d/sub-key-derivation").as_bytes()[0..32]
            .try_into()
            .unwrap_or([0u8; 32]),
    );
    hasher.update(salt.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Deterministically encrypts a Google `sub` identifier using AES-256-GCM-SIV and the instance salt.
///
/// Returns the ciphertext with authentication tag encoded as a lowercase hexadecimal string.
///
/// # Errors
///
/// Returns [`MaitreDError::Crypto`] if encryption fails.
pub fn encrypt_sub(salt: &str, sub: &str) -> Result<String> {
    let key_bytes = derive_sub_key(salt);
    let key = Key::<Aes256GcmSiv>::from_slice(&key_bytes);
    let cipher = Aes256GcmSiv::new(key);
    let nonce = Nonce::from_slice(&ZERO_NONCE);

    let ciphertext = cipher
        .encrypt(nonce, sub.as_bytes())
        .map_err(|e| MaitreDError::Crypto(format!("Failed to encrypt Google sub: {e:?}")))?;

    Ok(hex::encode(ciphertext))
}

/// Decrypts a previously encrypted Google `sub` identifier.
///
/// If `encrypted_sub_hex` is `"0"` (debug account), it returns `"0"`.
///
/// # Errors
///
/// Returns [`MaitreDError::Crypto`] if hex decoding or AEAD authentication fails.
pub fn decrypt_sub(salt: &str, encrypted_sub_hex: &str) -> Result<String> {
    if encrypted_sub_hex == DEBUG_SUB_CIPHERTEXT {
        return Ok(DEBUG_SUB_CIPHERTEXT.to_string());
    }

    let raw_bytes = hex::decode(encrypted_sub_hex)
        .map_err(|e| MaitreDError::Crypto(format!("Invalid hex for encrypted sub: {e:?}")))?;

    let key_bytes = derive_sub_key(salt);
    let key = Key::<Aes256GcmSiv>::from_slice(&key_bytes);
    let cipher = Aes256GcmSiv::new(key);
    let nonce = Nonce::from_slice(&ZERO_NONCE);

    let plaintext = cipher
        .decrypt(nonce, raw_bytes.as_ref())
        .map_err(|e| MaitreDError::Crypto(format!("Failed to decrypt Google sub: {e:?}")))?;

    String::from_utf8(plaintext)
        .map_err(|e| MaitreDError::Crypto(format!("Invalid UTF-8 in decrypted sub: {e:?}")))
}
