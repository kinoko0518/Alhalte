pub mod encrypted;

use aes_gcm_siv::{Aes256GcmSiv, Key};

pub use crate::encrypted::Encrypted;
use crate::encrypted::{high_entropy_encrypt, low_entropy_encrypt};

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

type Label = [u8; 32];
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
}
