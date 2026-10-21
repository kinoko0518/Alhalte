//! Encryption and cryptographic primitives for `tin`.

use aes_gcm_siv::{
    Aes256GcmSiv, Key, Nonce,
    aead::{Aead, KeyInit},
};

use crate::error::TinError;

/// Zero nonce (`[0u8; 12]`) used for convergent encryption (`0001`).
pub const ZERO_NONCE: [u8; 12] = [0u8; 12];

/// Derives the 256-bit `TinId` and 256-bit AES-256-GCM-SIV convergent encryption key
/// from the BLAKE3 XOF 512-bit hash of `plaintext`.
///
/// The first 256 bits (32 bytes) form the `TinId`, and the remaining 256 bits (32 bytes)
/// form the AES-256-GCM-SIV encryption key.
pub fn derive_convergent_id_and_key(plaintext: &[u8]) -> (TinId, [u8; 32]) {
    let mut hasher = blake3::Hasher::new();
    hasher.update(plaintext);
    let mut xof = hasher.finalize_xof();
    let mut xof_out = [0u8; 64];
    xof.fill(&mut xof_out);

    let mut tin_id = [0u8; 32];
    let mut key = [0u8; 32];
    tin_id.copy_from_slice(&xof_out[0..32]);
    key.copy_from_slice(&xof_out[32..64]);

    (tin_id, key)
}

/// Verifies whether the convergent key (and optionally `TinId`) derived from `plaintext`
/// matches the provided `key` (and `tin_id`).
pub fn verify_convergent(
    plaintext: &[u8],
    key: &[u8; 32],
    tin_id: Option<&TinId>,
) -> Result<(), TinError> {
    let (derived_tin_id, derived_key) = derive_convergent_id_and_key(plaintext);
    if &derived_key != key {
        return Err(TinError::KeyMismatch);
    }
    if let Some(expected_id) = tin_id
        && &derived_tin_id != expected_id
    {
        return Err(TinError::TinIdMismatch);
    }
    Ok(())
}

/// Encrypts `plaintext` using AES-256-GCM-SIV with the given 256-bit key and 96-bit nonce.
pub fn encrypt_aes_256_gcm_siv(
    key: &[u8; 32],
    nonce: &[u8; 12],
    plaintext: &[u8],
) -> Result<Vec<u8>, TinError> {
    let cipher = Aes256GcmSiv::new(Key::<Aes256GcmSiv>::from_slice(key));
    let nonce_ref = Nonce::from_slice(nonce);
    cipher
        .encrypt(nonce_ref, plaintext)
        .map_err(|e| TinError::EncryptionFailed(e.to_string()))
}

/// Decrypts `ciphertext` using AES-256-GCM-SIV with the given 256-bit key and 96-bit nonce.
pub fn decrypt_aes_256_gcm_siv(
    key: &[u8; 32],
    nonce: &[u8; 12],
    ciphertext: &[u8],
) -> Result<Vec<u8>, TinError> {
    let cipher = Aes256GcmSiv::new(Key::<Aes256GcmSiv>::from_slice(key));
    let nonce_ref = Nonce::from_slice(nonce);
    cipher
        .decrypt(nonce_ref, ciphertext)
        .map_err(|e| TinError::DecryptionFailed(e.to_string()))
}
