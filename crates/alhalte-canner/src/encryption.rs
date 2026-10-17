//! High-entropy and low-entropy encryption routines for `Tin` containers.

use tin::{
    Compression, Encryption, Opener, Tin, TinId, decode_raw_plaintext_bytes,
    decrypt_aes_256_gcm_siv, encode_raw_convergent, encode_raw_room_key, encrypt_aes_256_gcm_siv,
};

use crate::error::{CannerError, Result};
use crate::padding::{pad_iso7816_4_padme, unpad_iso7816_4_padme};

/// Output of high-entropy convergent encryption with wrapped key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighEntropyOutput {
    /// 256-bit Tin ID (first 256 bits of plaintext BLAKE3 XOF output).
    pub tin_id: TinId,
    /// Wrapped convergent key: `nonce(12B) || ciphertext(48B)` encrypted under `room_key`.
    pub wrapped_key: Vec<u8>,
    /// Wire-format tin bytes with convergent encryption (`0001`).
    pub tin_bytes: Vec<u8>,
}

/// Encrypts low-entropy data (such as chat text) with room key and explicit nonce.
///
/// Plaintext (`label(8B LE) || payload`) is padded using ISO/IEC 7816-4 up to PADMÉ length,
/// compressed with `compression`, and encrypted with `room_key` and `nonce` into encryption
/// format `0010` (`Encryption::RoomKey`).
///
/// // spec: alhalte-canner/encrypt-low-entropy
pub fn encrypt_low_entropy(
    tin: &Tin,
    compression: Compression,
    room_key: &[u8; 32],
    nonce: &[u8; 12],
) -> Result<(Vec<u8>, TinId)> {
    let plaintext = tin.to_plaintext();
    let padded = pad_iso7816_4_padme(&plaintext);
    let (wire_bytes, tin_id) = encode_raw_room_key(&padded, compression, room_key, nonce)?;
    Ok((wire_bytes, tin_id))
}

/// Encrypts low-entropy data using a cryptographically secure randomly generated 96-bit nonce.
///
/// // spec: alhalte-canner/encrypt-low-entropy
pub fn encrypt_low_entropy_random(
    tin: &Tin,
    compression: Compression,
    room_key: &[u8; 32],
) -> Result<(Vec<u8>, TinId)> {
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce)?;
    encrypt_low_entropy(tin, compression, room_key, &nonce)
}

/// Decrypts a low-entropy wire-format tin (`0010`) using `room_key` and removes PADMÉ padding.
///
/// // spec: alhalte-canner/encrypt-low-entropy
pub fn decrypt_low_entropy(wire_bytes: &[u8], room_key: &[u8; 32]) -> Result<(Tin, Opener, TinId)> {
    let (padded_plaintext, opener, tin_id) =
        decode_raw_plaintext_bytes(wire_bytes, Some(room_key))?;
    if opener.encryption != Encryption::RoomKey {
        return Err(CannerError::InvalidEncryption(opener.encryption));
    }
    let unpadded = unpad_iso7816_4_padme(&padded_plaintext)?;
    let tin = Tin::from_plaintext(unpadded)?;
    Ok((tin, opener, tin_id))
}

/// Encrypts high-entropy data (such as images and audio) using convergent encryption (`0001`)
/// and wraps the 256-bit convergent key with `room_key` and `wrap_nonce`.
///
/// Returns `HighEntropyOutput` containing `(tin_id, wrapped_key, tin_bytes)`.
///
/// // spec: alhalte-canner/encrypt-high-entropy
pub fn encrypt_high_entropy(
    tin: &Tin,
    compression: Compression,
    room_key: &[u8; 32],
    wrap_nonce: &[u8; 12],
) -> Result<HighEntropyOutput> {
    let plaintext = tin.to_plaintext();
    let (tin_bytes, tin_id, convergent_key) = encode_raw_convergent(&plaintext, compression)?;

    // Wrap the 32-byte convergent key: AES-256-GCM-SIV produces 32B ciphertext + 16B tag = 48B
    let ciphertext = encrypt_aes_256_gcm_siv(room_key, wrap_nonce, &convergent_key)?;
    let mut wrapped_key = Vec::with_capacity(12 + ciphertext.len());
    wrapped_key.extend_from_slice(wrap_nonce);
    wrapped_key.extend_from_slice(&ciphertext);

    Ok(HighEntropyOutput {
        tin_id,
        wrapped_key,
        tin_bytes,
    })
}

/// Encrypts high-entropy data using a randomly generated 96-bit nonce for key wrapping.
///
/// // spec: alhalte-canner/encrypt-high-entropy
pub fn encrypt_high_entropy_random(
    tin: &Tin,
    compression: Compression,
    room_key: &[u8; 32],
) -> Result<HighEntropyOutput> {
    let mut wrap_nonce = [0u8; 12];
    getrandom::getrandom(&mut wrap_nonce)?;
    encrypt_high_entropy(tin, compression, room_key, &wrap_nonce)
}

/// Decrypts a high-entropy wire-format tin (`0001`) by first unwrapping the convergent key
/// from `wrapped_key` using `room_key`, then decrypting the tin body.
///
/// // spec: alhalte-canner/encrypt-high-entropy
pub fn decrypt_high_entropy(
    wire_bytes: &[u8],
    wrapped_key: &[u8],
    room_key: &[u8; 32],
) -> Result<(Tin, Opener, TinId)> {
    if wrapped_key.len() < 28 {
        return Err(CannerError::InvalidWrappedKeyLength(wrapped_key.len()));
    }
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&wrapped_key[..12]);
    let ciphertext = &wrapped_key[12..];

    let decrypted_key = decrypt_aes_256_gcm_siv(room_key, &nonce, ciphertext)?;
    if decrypted_key.len() != 32 {
        return Err(CannerError::InvalidConvergentKeyLength(decrypted_key.len()));
    }
    let mut convergent_key = [0u8; 32];
    convergent_key.copy_from_slice(&decrypted_key);

    let (tin, opener, tin_id) = Tin::decode(wire_bytes, Some(&convergent_key))?;
    if opener.encryption != Encryption::Convergent {
        return Err(CannerError::InvalidEncryption(opener.encryption));
    }
    Ok((tin, opener, tin_id))
}
