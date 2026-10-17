/**
 * Compression and decompression routines for `@alhalte/pantry`.
 *
 * Supports:
 * - 0000: None (uncompressed)
 * - 0001: Zstandard (level 19)
 * - 0010: LZ4 block format with prepended uncompressed size (u32 LE)
 * - 0011: Brotli
 *
 * References:
 * - ARCHITECTURE.md: `## tin-rs -> ### バイト列表現, ### opener -> 圧縮表現`
 */

import { decompress as brotliDecompress } from 'brotli';
import { decompress as zstdDecompress } from 'fzstd';
import lz4js from 'lz4js';
import { DecompressionError, UnsupportedCompressionError } from './errors.js';
import { Compression } from './types.js';

/**
 * Decompresses `data` using the specified algorithm.
 */
export function decompress(data: Uint8Array, algorithm: Compression): Uint8Array {
  try {
    switch (algorithm) {
      case Compression.None:
        return data;

      case Compression.Zstd:
        return zstdDecompress(data);

      case Compression.Lz4:
        return decompressLz4BlockSizePrefixed(data);

      case Compression.Brotli:
        return decompressBrotli(data);

      default:
        throw new UnsupportedCompressionError(algorithm);
    }
  } catch (err) {
    if (err instanceof DecompressionError || err instanceof UnsupportedCompressionError) {
      throw err;
    }
    throw new DecompressionError(
      `Failed to decompress data using algorithm 0x${algorithm.toString(16)}: ${String(err)}`,
      err,
    );
  }
}

/**
 * Decompresses an LZ4 block prefixed with a 4-byte little-endian uncompressed size.
 * Wire format matches `lz4_flex::block::compress_prepend_size` in `tin-rs`.
 */
export function decompressLz4BlockSizePrefixed(data: Uint8Array): Uint8Array {
  if (data.length < 4) {
    throw new DecompressionError(
      `LZ4 size-prefixed data too short: expected at least 4 bytes, received ${data.length}`,
    );
  }

  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const uncompressedSize = view.getUint32(0, true);

  if (uncompressedSize === 0) {
    return new Uint8Array(0);
  }

  const dest = new Uint8Array(uncompressedSize);
  const blockBytes = data.subarray(4);

  // If compressed block is empty but uncompressed size > 0, invalid
  if (blockBytes.length === 0 && uncompressedSize > 0) {
    throw new DecompressionError(
      `LZ4 block payload empty with non-zero uncompressed size ${uncompressedSize}`,
    );
  }

  const bytesWritten = lz4js.decompressBlock(blockBytes, dest, 0, blockBytes.length, 0);
  if (bytesWritten !== uncompressedSize) {
    throw new DecompressionError(
      `LZ4 decompressed size mismatch: expected ${uncompressedSize}, decoded ${bytesWritten}`,
    );
  }

  return dest;
}

/**
 * Decompresses Brotli compressed data.
 */
function decompressBrotli(data: Uint8Array): Uint8Array {
  const result = brotliDecompress(data);
  if (!result) {
    throw new DecompressionError('Brotli decompression returned null/failure');
  }
  return result instanceof Uint8Array ? result : new Uint8Array(result);
}

/**
 * Compresses an LZ4 block with a 4-byte little-endian uncompressed size prefix.
 * Compatible with `tin-rs`'s `lz4_flex::block::compress_prepend_size`.
 */
export function compressLz4BlockSizePrefixed(data: Uint8Array): Uint8Array {
  const uncompressedSize = data.length;
  if (uncompressedSize === 0) {
    return new Uint8Array(4); // 4 zero bytes: uncompressed size = 0
  }

  const maxOutput = lz4js.compressBound(uncompressedSize);
  const temp = new Uint8Array(maxOutput);
  const hashTable = new Uint32Array(65536);
  const compSize = lz4js.compressBlock(data, temp, 0, data.length, hashTable);

  // If data couldn't be compressed as an LZ4 match sequence (e.g. tiny input),
  // lz4js.compressBlock still writes literals if >= minLength, or returns 0.
  // When compSize is 0, write uncompressed literals token.
  let blockBytes: Uint8Array;
  if (compSize === 0) {
    // Uncompressed fallback block: write token + literals
    // Token with high nibble = literal count
    const literalCount = uncompressedSize;
    const header: number[] = [];
    if (literalCount >= 15) {
      header.push(15 << 4);
      let rem = literalCount - 15;
      while (rem >= 255) {
        header.push(255);
        rem -= 255;
      }
      header.push(rem);
    } else {
      header.push(literalCount << 4);
    }
    blockBytes = new Uint8Array(header.length + uncompressedSize);
    blockBytes.set(header, 0);
    blockBytes.set(data, header.length);
  } else {
    blockBytes = temp.subarray(0, compSize);
  }

  const result = new Uint8Array(4 + blockBytes.length);
  const view = new DataView(result.buffer);
  view.setUint32(0, uncompressedSize, true);
  result.set(blockBytes, 4);

  return result;
}
