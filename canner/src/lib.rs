pub mod encrypted;

use aes_gcm_siv::{Aes256GcmSiv, Key};

pub use crate::encrypted::Encrypted;
use crate::encrypted::{high_entropy_encrypt, low_entropy_encrypt};

pub enum CannerError {
    IoError(std::io::Error),
    AesGcmSivError(aes_gcm_siv::Error),
    OpenerByteNotFound,
    UnknownCompression(u8),
    UnknownEncryption(u8),
    LenNotFound,
    NonceNotFound,
    LabelNotFound,
}

const LABEL_BYTELEN: usize = 32;

type Label = [u8; LABEL_BYTELEN];
type RoomKey = Key<Aes256GcmSiv>;

pub struct Tin {
    label: Label,
    encrypted: Encrypted,
}

impl Tin {
    pub fn plaintext(label: Label, bytes: Vec<u8>) -> Self {
        Self {
            label,
            encrypted: Encrypted::Plaintext(bytes),
        }
    }

    pub fn high_entropy(
        label: Label,
        bytes: Vec<u8>,
        room_key: impl Into<RoomKey>,
    ) -> Result<Self, CannerError> {
        let encrypted = Encrypted::HighEntropy(high_entropy_encrypt(bytes, room_key.into())?);
        Ok(Self { label, encrypted })
    }

    pub fn low_entropy(
        label: Label,
        bytes: Vec<u8>,
        room_key: impl Into<RoomKey>,
    ) -> Result<Self, CannerError> {
        let encrypted = Encrypted::LowEntropy(low_entropy_encrypt(bytes, room_key.into())?);
        Ok(Self { label, encrypted })
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = vec![self.encrypted.opener()];
        bytes.extend(self.label);
        bytes.extend(self.encrypted.into_bytes());
        bytes
    }

    pub fn from_bytes(mut bytes: Vec<u8>) -> Result<Self, CannerError> {
        let label: [u8; LABEL_BYTELEN] = (bytes.len() >= LABEL_BYTELEN)
            .then(|| {
                let mut label = bytes.drain(..LABEL_BYTELEN);
                let label = std::array::from_fn(|_| label.next().unwrap());
                label
            })
            .ok_or(CannerError::LabelNotFound)?;
        let encrypted = Encrypted::try_from_bytes(bytes)?;

        Ok(Self { label, encrypted })
    }
}
