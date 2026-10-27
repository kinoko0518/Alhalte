use aes_gcm_siv::{Aes256GcmSiv, Key, KeyInit, aead::Aead};

use crate::{CannerError, RoomKey};

pub const NONCE_SIZE: usize = 12;
pub const ZERO_NONCE: [u8; 12] = [0; NONCE_SIZE];

pub struct HighEntropyCiphertext(pub Vec<u8>);

pub fn high_entropy_encrypt(
    plaintext: Vec<u8>,
    room_key: RoomKey,
) -> Result<HighEntropyCiphertext, CannerError> {
    let cipher = Aes256GcmSiv::new(&room_key);
    let encrypted = cipher
        .encrypt(&ZERO_NONCE.into(), &plaintext[..])
        .map_err(|e| CannerError::AesGcmSivError(e))?;

    Ok(HighEntropyCiphertext(encrypted))
}

pub fn high_entropy_decrypt(
    ciphertext: HighEntropyCiphertext,
    room_key: RoomKey,
) -> Result<Vec<u8>, CannerError> {
    let cipher = Aes256GcmSiv::new(&room_key);
    let decrypted = cipher
        .decrypt(&ZERO_NONCE.into(), &ciphertext.0[..])
        .map_err(|e| CannerError::AesGcmSivError(e))?;

    Ok(decrypted)
}
