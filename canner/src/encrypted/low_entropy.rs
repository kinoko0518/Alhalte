use aes_gcm_siv::{
    Aes256GcmSiv, KeyInit, Nonce,
    aead::{Aead, Generate},
};

use crate::{
    CannerError, RoomKey, WrappedKey,
    encrypted::{ConvergentKey, get_convergent_key},
};

pub struct LowEntropyCiphertext {
    pub nonce: Nonce,
    pub ciphertext: Vec<u8>,
}

pub fn low_entropy_encrypt(
    bytes: Vec<u8>,
    room_key: impl Into<RoomKey>,
) -> Result<(LowEntropyCiphertext, WrappedKey), CannerError> {
    let conv_key = get_convergent_key(&bytes);

    let padded = {
        let mut padded = (bytes.len() as u64).to_le_bytes().to_vec();
        padded.extend(bytes);
        let target_len = padme_padding::Padme::padding_len(padded.len());
        padded.extend(vec![0; target_len]);
        padded
    };

    let cipher = Aes256GcmSiv::new(&conv_key.into());
    let nonce = Nonce::generate();

    let ciphertext = cipher
        .encrypt(&nonce, &padded[..])
        .map_err(|e| CannerError::AesGcmSivError(e))?;

    Ok((
        LowEntropyCiphertext { ciphertext, nonce },
        WrappedKey::wrap(conv_key, room_key.into()).map_err(|e| CannerError::AesGcmSivError(e))?,
    ))
}

pub fn low_entropy_decrypt(
    ciphertext: LowEntropyCiphertext,
    conv_key: ConvergentKey,
) -> Result<Vec<u8>, CannerError> {
    let cipher = Aes256GcmSiv::new(&conv_key.into());
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

    Ok(data)
}
