//! Wire format encoding and decoding for raw bytes and `Tin` containers.

use crate::error::TinError;
use crate::tin::Tin;

/// Parameter specifying encryption configuration for encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionParam<'a> {
    /// Plaintext (no encryption).
    Plaintext,
    /// Convergent encryption (key and TinId derived from plaintext BLAKE3 XOF 512, zero nonce).
    Convergent,
    /// Room key encryption with external 256-bit key and 96-bit nonce.
    RoomKey {
        /// 256-bit room key.
        room_key: &'a [u8; 32],
        /// 96-bit nonce.
        nonce: &'a [u8; 12],
    },
}

/// Result of encoding raw plaintext bytes into the wire format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedRaw {
    /// Full wire format bytes (`opener || [nonce] || body`).
    pub wire_bytes: Vec<u8>,
    /// 256-bit Tin ID.
    pub tin_id: TinId,
    /// Convergent encryption key if convergent encryption was selected.
    pub convergent_key: Option<[u8; 32]>,
}

/// Encodes raw plaintext bytes using the specified compression and encryption mode.
///
/// This function can be used when encoding arbitrary raw plaintext bytes,
/// such as PADMÉ-padded data before containerization.
pub fn encode_raw_plaintext_bytes(
    plaintext: &[u8],
    compression: Compression,
    encryption: EncryptionParam,
) -> Result<EncodedRaw, TinError> {
    match encryption {
        EncryptionParam::Plaintext => {
            let (wire_bytes, tin_id) = encode_raw_plaintext(plaintext, compression)?;
            Ok(EncodedRaw {
                wire_bytes,
                tin_id,
                convergent_key: None,
            })
        }
        EncryptionParam::Convergent => {
            let (wire_bytes, tin_id, convergent_key) =
                encode_raw_convergent(plaintext, compression)?;
            Ok(EncodedRaw {
                wire_bytes,
                tin_id,
                convergent_key: Some(convergent_key),
            })
        }
        EncryptionParam::RoomKey { room_key, nonce } => {
            let (wire_bytes, tin_id) =
                encode_raw_room_key(plaintext, compression, room_key, nonce)?;
            Ok(EncodedRaw {
                wire_bytes,
                tin_id,
                convergent_key: None,
            })
        }
    }
}

/// Encodes raw plaintext bytes as an unencrypted tin (`0000`).
pub fn encode_raw_plaintext(
    plaintext: &[u8],
    compression: Compression,
) -> Result<(Vec<u8>, TinId), TinError> {
    let opener = Opener::new(Encryption::Plaintext, compression);
    let compressed = compress(plaintext, compression)?;
    let mut wire_bytes = Vec::with_capacity(1 + compressed.len());
    wire_bytes.push(opener.to_byte());
    wire_bytes.extend_from_slice(&compressed);

    let tin_id = *blake3::hash(&wire_bytes).as_bytes();
    Ok((wire_bytes, tin_id))
}

/// Encodes raw plaintext bytes with convergent encryption (`0001`).
///
/// Returns a tuple of `(wire_bytes, tin_id, convergent_key)`.
pub fn encode_raw_convergent(
    plaintext: &[u8],
    compression: Compression,
) -> Result<(Vec<u8>, TinId, [u8; 32]), TinError> {
    let opener = Opener::new(Encryption::Convergent, compression);
    let (tin_id, convergent_key) = derive_convergent_id_and_key(plaintext);
    let compressed = compress(plaintext, compression)?;
    let encrypted = encrypt_aes_256_gcm_siv(&convergent_key, &ZERO_NONCE, &compressed)?;

    let mut wire_bytes = Vec::with_capacity(1 + encrypted.len());
    wire_bytes.push(opener.to_byte());
    wire_bytes.extend_from_slice(&encrypted);

    Ok((wire_bytes, tin_id, convergent_key))
}

/// Encodes raw plaintext bytes with room key encryption (`0010`).
pub fn encode_raw_room_key(
    plaintext: &[u8],
    compression: Compression,
    room_key: &[u8; 32],
    nonce: &[u8; 12],
) -> Result<(Vec<u8>, TinId), TinError> {
    let opener = Opener::new(Encryption::RoomKey, compression);
    let compressed = compress(plaintext, compression)?;
    let encrypted = encrypt_aes_256_gcm_siv(room_key, nonce, &compressed)?;

    let mut wire_bytes = Vec::with_capacity(1 + 12 + encrypted.len());
    wire_bytes.push(opener.to_byte());
    wire_bytes.extend_from_slice(nonce);
    wire_bytes.extend_from_slice(&encrypted);

    let tin_id = *blake3::hash(&wire_bytes).as_bytes();
    Ok((wire_bytes, tin_id))
}

/// Decodes raw plaintext bytes from wire format.
///
/// Returns the decrypted and decompressed plaintext bytes, the `Opener`, and the verified `TinId`.
pub fn decode_raw_plaintext_bytes(
    wire_bytes: &[u8],
    key: Option<&[u8; 32]>,
) -> Result<(Vec<u8>, Opener, TinId), TinError> {
    if wire_bytes.is_empty() {
        return Err(TinError::WireTooShort {
            expected: 1,
            actual: 0,
        });
    }

    let opener = Opener::from_byte(wire_bytes[0])?;

    match opener.encryption {
        Encryption::Plaintext => {
            let body = &wire_bytes[1..];
            let decompressed = decompress(body, opener.compression)?;
            let tin_id = *blake3::hash(wire_bytes).as_bytes();
            Ok((decompressed, opener, tin_id))
        }
        Encryption::Convergent => {
            let key = key.ok_or(TinError::MissingKey)?;
            let body = &wire_bytes[1..];
            let decrypted = decrypt_aes_256_gcm_siv(key, &ZERO_NONCE, body)?;
            let decompressed = decompress(&decrypted, opener.compression)?;

            let (expected_tin_id, expected_key) = derive_convergent_id_and_key(&decompressed);
            if &expected_key != key {
                return Err(TinError::KeyMismatch);
            }
            Ok((decompressed, opener, expected_tin_id))
        }
        Encryption::RoomKey => {
            let key = key.ok_or(TinError::MissingKey)?;
            if wire_bytes.len() < 13 {
                return Err(TinError::WireTooShort {
                    expected: 13,
                    actual: wire_bytes.len(),
                });
            }
            let mut nonce = [0u8; 12];
            nonce.copy_from_slice(&wire_bytes[1..13]);
            let body = &wire_bytes[13..];

            let decrypted = decrypt_aes_256_gcm_siv(key, &nonce, body)?;
            let decompressed = decompress(&decrypted, opener.compression)?;
            let tin_id = *blake3::hash(wire_bytes).as_bytes();
            Ok((decompressed, opener, tin_id))
        }
    }
}

/// Decodes a `Tin` container from wire format.
pub fn decode(wire_bytes: &[u8], key: Option<&[u8; 32]>) -> Result<(Tin, Opener, TinId), TinError> {
    Tin::decode(wire_bytes, key)
}
