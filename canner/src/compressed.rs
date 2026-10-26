use std::io::Read;

use crate::{Blob, CellarError};

#[repr(u8)]
pub enum Compression {
    None = 0,
    Zstd = 1,
    Brotil = 2,
}

impl TryFrom<u8> for Compression {
    type Error = CellarError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Zstd),
            2 => Ok(Self::Brotil),
            _ => Err(CellarError::UnknownCompression(value)),
        }
    }
}

pub struct Compressed {
    pub compression: Compression,
    pub bytes: Vec<u8>,
}

impl Blob {
    pub fn compress(self, compression: Compression) -> Result<Compressed, CellarError> {
        let original = self.into_bytes();
        let compressed = match compression {
            Compression::Brotil => {
                let mut compressed = Vec::new();
                {
                    let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 6, 22);
                    std::io::copy(&mut &original[..], &mut writer)
                        .map_err(|e| CellarError::IoError(e))?;
                }
                compressed
            }
            Compression::Zstd => {
                zstd::encode_all(&original[..], 3).map_err(|e| CellarError::IoError(e))?
            }
            Compression::None => original,
        };
        Ok(Compressed {
            compression,
            bytes: compressed,
        })
    }
}

impl Compressed {
    pub fn decompress(self, compression: Compression) -> Result<Blob, CellarError> {
        let decompressed = match compression {
            Compression::Brotil => {
                let mut decompressed = Vec::new();
                brotli::Decompressor::new(&mut &self.bytes[..], 4096)
                    .read_to_end(&mut decompressed)
                    .map_err(|e| CellarError::IoError(e))?;
                decompressed
            }
            Compression::Zstd => {
                zstd::decode_all(&self.bytes[..]).map_err(|e| CellarError::IoError(e))?
            }
            Compression::None => self.bytes,
        };
        Ok(Blob::new(decompressed))
    }
}

impl Into<Vec<u8>> for Compressed {
    fn into(self) -> Vec<u8> {
        let mut data = vec![self.compression as u8];
        data.extend(self.bytes);
        data
    }
}

impl TryFrom<Vec<u8>> for Compressed {
    type Error = CellarError;

    fn try_from(mut value: Vec<u8>) -> Result<Self, Self::Error> {
        let cbyte: Compression = (!value.is_empty())
            .then(|| value.remove(0))
            .ok_or(CellarError::CompressedByteNotFound)?
            .try_into()?;
        Ok(Self {
            compression: cbyte,
            bytes: value,
        })
    }
}
