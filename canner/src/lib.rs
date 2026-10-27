pub mod blob;
pub mod compressed;
pub mod encrypted;

pub use blob::{Blob, BlobID};

use crate::encrypted::Encrypted;

pub enum CannerError {
    IoError(std::io::Error),
    AesGcmSivError(aes_gcm_siv::Error),
    CompressedByteNotFound,
    OpenerByteNotFound,
    UnknownCompression(u8),
    UnknownEncryption(u8),
    LenNotFound,
    NonceNotFound,
    LabelNotFound,
}

type Label = u64;

pub struct Tin {
    label: Label,
    encrypted: Encrypted,
}

impl Tin {
    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = self.label.to_le_bytes().to_vec();
        bytes.extend(self.encrypted.to_bytes());
        bytes
    }

    pub fn try_from_bytes(mut bytes: Vec<u8>) -> Result<Self, CannerError> {
        let (label, encrypted) = (bytes.len() >= 4)
            .then(|| {
                let encrypted = Encrypted::try_from_bytes(bytes.split_off(4));
                let label = u64::from_le_bytes(bytes.try_into().unwrap());
                (label, encrypted)
            })
            .ok_or(CannerError::LabelNotFound)?;
        let encrypted = encrypted?;

        Ok(Self { label, encrypted })
    }
}
