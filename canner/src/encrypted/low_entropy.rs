use aes_gcm_siv::{
    Aes256GcmSiv, Key, KeyInit, Nonce,
    aead::{Aead, Generate},
};

use crate::{CannerError, compressed::Compressed, encrypted::Encrypted};

pub struct LowEntropyCiphertext {
    pub nonce: Nonce,
    pub ciphertext: Vec<u8>,
}

pub fn low_entropy_encrypt(
    bytes: Vec<u8>,
    room_key: Key<Aes256GcmSiv>,
) -> Result<Encrypted, CannerError> {
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
        .map_err(|e| CannerError::AesGcmSivError(e))?;

    Ok(Encrypted::LowEntropy(LowEntropyCiphertext {
        ciphertext,
        nonce,
    }))
}

pub fn low_entropy_decrypt(
    ciphertext: LowEntropyCiphertext,
    room_key: Key<Aes256GcmSiv>,
) -> Result<Compressed, CannerError> {
    let cipher = Aes256GcmSiv::new(&room_key);
    let (nonce, ciphertext) = (ciphertext.nonce, ciphertext.ciphertext);

    let data = {
        let mut decrypted = cipher
            .decrypt(&nonce, &ciphertext[..])
            .map_err(|e| CannerError::AesGcmSivError(e))?;
        (decrypted.len() >= 8)
            .then(|| {
                let mut data = decrypted.split_off(8);
                let len: [u8; 8] = decrypted.try_into().unwrap();
                let original_len = u64::from_le_bytes(len) as usize;
                data.truncate(original_len);
                data
            })
            .ok_or(CannerError::LenNotFound)?
    };

    Compressed::try_from(data)
}
