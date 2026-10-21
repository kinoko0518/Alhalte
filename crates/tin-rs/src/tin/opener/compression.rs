//! Compression and decompression routines.

use crate::error::TinError;
use crate::opener::Compression;
use std::io::{Read, Write};

/// Compresses `data` using the specified compression algorithm.
pub fn compress(data: &[u8], compression: Compression) -> Result<Vec<u8>, TinError> {
    match compression {
        Compression::None => Ok(data.to_vec()),
        Compression::Zstd => {
            zstd::encode_all(data, 19).map_err(|e| TinError::Compression(e.to_string()))
        }
        Compression::Lz4 => Ok(lz4_flex::block::compress_prepend_size(data)),
        Compression::Brotli => {
            let mut writer = brotli::CompressorWriter::new(Vec::new(), 4096, 11, 22);
            writer
                .write_all(data)
                .and_then(|()| writer.flush())
                .map_err(|e| TinError::Compression(e.to_string()))?;
            Ok(writer.into_inner())
        }
    }
}

/// Decompresses `data` using the specified compression algorithm.
pub fn decompress(data: &[u8], compression: Compression) -> Result<Vec<u8>, TinError> {
    match compression {
        Compression::None => Ok(data.to_vec()),
        Compression::Zstd => {
            zstd::decode_all(data).map_err(|e| TinError::Decompression(e.to_string()))
        }
        Compression::Lz4 => lz4_flex::block::decompress_size_prepended(data)
            .map_err(|e| TinError::Decompression(e.to_string())),
        Compression::Brotli => {
            let mut decompressor = brotli::Decompressor::new(data, 4096);
            let mut decompressed = Vec::new();
            decompressor
                .read_to_end(&mut decompressed)
                .map_err(|e| TinError::Decompression(e.to_string()))?;
            Ok(decompressed)
        }
    }
}
