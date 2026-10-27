use aes_gcm_siv::{Aes256GcmSiv, KeyInit, aead::Aead};

use crate::{
    CannerError,
    encrypted::{ConvergentKey, get_convergent_key},
};

pub const NONCE_SIZE: usize = 12;
pub const ZERO_NONCE: [u8; 12] = [0; NONCE_SIZE];

pub struct HighEntropyCiphertext(pub Vec<u8>);

pub fn high_entropy_encrypt(plaintext: Vec<u8>) -> Result<HighEntropyCiphertext, CannerError> {
    let conv_key = get_convergent_key(&plaintext).into();
    let cipher = Aes256GcmSiv::new(&conv_key);
    let encrypted = cipher
        .encrypt(&ZERO_NONCE.into(), &plaintext[..])
        .map_err(|e| CannerError::AesGcmSivError(e))?;

    Ok(HighEntropyCiphertext(encrypted))
}

pub fn high_entropy_decrypt(
    ciphertext: HighEntropyCiphertext,
    conv_key: ConvergentKey,
) -> Result<Vec<u8>, CannerError> {
    let cipher = Aes256GcmSiv::new(&conv_key.into());
    let decrypted = cipher
        .decrypt(&ZERO_NONCE.into(), &ciphertext.0[..])
        .map_err(|e| CannerError::AesGcmSivError(e))?;

    Ok(decrypted)
}
