//! Compression and decompression routines.

use crate::{Tin, encode_raw_convergent, error::TinError, tin::opener::{Compression, Encryption, derive_convergent_id_and_key}};
use std::io::{Read, Write};

pub struct CompressedTinContent {
    pub compression: Compression,
    pub bin: Vec<u8>,
}

/// Compresses `data` using the specified compression algorithm.
pub fn compress(data: Vec<u8>, compression: Compression) -> Result<CompressedTinContent, TinError> {
    let bin = match compression {
        Compression::None => Ok(data),
        Compression::Zstd => {
            zstd::encode_all(data.as_slice(), 19).map_err(|e| TinError::Compression(e.to_string()))
        }
        Compression::Lz4 => Ok(lz4_flex::block::compress_prepend_size(data.as_ref())),
        Compression::Brotli => {
            let mut writer = brotli::CompressorWriter::new(Vec::new(), 4096, 11, 22);
            writer
                .write_all(data.as_ref())
                .and_then(|()| writer.flush())
                .map_err(|e| TinError::Compression(e.to_string()))?;
            Ok(writer.into_inner())
        }
    };
    bin.map(|bin| CompressedTinContent { bin, compression })
}

impl CompressedTinContent {
    /// Decompresses `data` using the specified compression algorithm.
    pub fn decompress(self) -> Result<Vec<u8>, TinError> {
        match self.compression {
            Compression::None => Ok(self.bin),
            Compression::Zstd => zstd::decode_all(self.bin.as_slice())
                .map_err(|e| TinError::Decompression(e.to_string())),
            Compression::Lz4 => lz4_flex::block::decompress_size_prepended(&self.bin)
                .map_err(|e| TinError::Decompression(e.to_string())),
            Compression::Brotli => {
                let mut decompressor = brotli::Decompressor::new(self.bin.as_slice(), 4096);
                let mut decompressed = Vec::new();
                decompressor
                    .read_to_end(&mut decompressed)
                    .map_err(|e| TinError::Decompression(e.to_string()))?;
                Ok(decompressed)
            }
        }
    }

    pub fn encrypt_convergently(self) -> Result<Tin, TinError> {
        let (wire_bytes, tin_id, convergent_key) = encode_raw_convergent(&self.bin, self.compression)?;

        match  {

        }
    }
}
