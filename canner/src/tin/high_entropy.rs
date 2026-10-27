use aes_gcm_siv::{Aes256GcmSiv, Key, KeyInit, aead::Aead};

use crate::{CellarError, compressed::Compressed, tin::Tin};

pub const NONCE_SIZE: usize = 12;
pub const ZERO_NONCE: [u8; 12] = [0; NONCE_SIZE];

pub struct HighEntropyCiphertext(pub Vec<u8>);

pub fn high_entropy_encrypt(
    compressed: Compressed,
    room_key: Key<Aes256GcmSiv>,
) -> Result<Tin, CellarError> {
    let cipher = Aes256GcmSiv::new(&room_key);
    let encrypted = cipher
        .encrypt(
            &ZERO_NONCE.into(),
            &{
                let bytes: Vec<u8> = compressed.into();
                bytes
            }[..],
        )
        .map_err(|e| CellarError::AesGcmSivError(e))?;

    Ok(Tin::HighEntropy(HighEntropyCiphertext(encrypted)))
}

pub fn high_entropy_decrypt(
    ciphertext: HighEntropyCiphertext,
    room_key: Key<Aes256GcmSiv>,
) -> Result<Compressed, CellarError> {
    let cipher = Aes256GcmSiv::new(&room_key);
    let decrypted = cipher
        .decrypt(&ZERO_NONCE.into(), &ciphertext.0[..])
        .map_err(|e| CellarError::AesGcmSivError(e))?;
    let compressed: Compressed = decrypted.try_into()?;

    Ok(compressed)
}
