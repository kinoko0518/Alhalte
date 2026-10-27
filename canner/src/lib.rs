pub mod blob;
pub mod compressed;
pub mod tin;

pub use blob::{Blob, BlobID};

pub enum CellarError {
    IoError(std::io::Error),
    AesGcmSivError(aes_gcm_siv::Error),
    CompressedByteNotFound,
    OpenerByteNotFound,
    UnknownCompression(u8),
    UnknownEncryption(u8),
    LenNotFound,
    NonceNotFound,
}
