use aes_gcm_siv::{
    Aes256GcmSiv, Key, KeyInit, Nonce,
    aead::{Aead, Generate},
};

use crate::{CellarError, compressed::Compressed, tin::Tin};

pub struct LowEntropyCiphertext {
    nonce: Nonce,
    ciphertext: Vec<u8>,
}

pub fn low_entropy_encrypt(
    bytes: Vec<u8>,
    room_key: Key<Aes256GcmSiv>,
) -> Result<Tin, CellarError> {
    let padded = {
        let mut padded = (bytes.len() as u64).to_le_bytes().to_vec();
        padded.extend(bytes);
        let target_len = padme_padding::Padme::padding_len(padded.len());
        padded.extend(vec![0; target_len]);
        padded
    };

    let cipher = Aes256GcmSiv::new(&room_key);
    let nonce = Nonce::generate();

    let ciphertext = cipher
        .encrypt(&nonce, &padded[..])
        .map_err(|e| CellarError::AesGcmSivError(e))?;

    Ok(Tin::LowEntropy(LowEntropyCiphertext { ciphertext, nonce }))
}

pub fn low_entropy_decrypt(
    ciphertext: LowEntropyCiphertext,
    room_key: Key<Aes256GcmSiv>,
) -> Result<Compressed, CellarError> {
    let cipher = Aes256GcmSiv::new(&room_key);
    let (nonce, ciphertext) = (ciphertext.nonce, ciphertext.ciphertext);

    let data = {
        let mut decrypted = cipher
            .decrypt(&nonce, &ciphertext[..])
            .map_err(|e| CellarError::AesGcmSivError(e))?;
        if decrypted.len() >= 4 {
            let mut data = decrypted.split_off(4);
            let len = ((0..4).fold(0_u64, |acc, i| {
                acc | (decrypted.get(i).cloned().unwrap() as u64) << i * 8
            })) as usize;
            data.split_off(len)
        } else {
            return Err(CellarError::LenNotFound);
        }
    };

    Compressed::try_from(data)
}
