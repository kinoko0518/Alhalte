import { init as initZstd, compress as zstdCompress } from '@bokuweb/zstd-wasm';
import * as zlib from 'node:zlib';
import { beforeAll, describe, expect, it } from 'vitest';
import {
  Compression,
  compressLz4BlockSizePrefixed,
  decompress,
  decompressLz4BlockSizePrefixed,
} from '../src/index.js';

describe('Compression', () => {
  beforeAll(async () => {
    await initZstd();
  });

  const testPayload = new TextEncoder().encode(
    'Alhalte pantry decompression test string with repeated pattern: ' +
      'abcdefghijklmnopqrstuvwxyz0123456789. '.repeat(10),
  );

  // spec: alhalte-pantry/compression 0000 none
  it('handles 0000 None compression correctly', () => {
    const out = decompress(testPayload, Compression.None);
    expect(out).toEqual(testPayload);
  });

  // spec: alhalte-pantry/compression 0001 zstd
  it('decompresses 0001 Zstandard (level 19) streams', () => {
    const compressed = zstdCompress(testPayload, 19);
    const decompressed = decompress(compressed, Compression.Zstd);
    expect(decompressed).toEqual(testPayload);
  });

  // spec: alhalte-pantry/compression 0010 lz4 size-prefixed
  it('decompresses 0010 LZ4 block size-prefixed streams', () => {
    const compressed = compressLz4BlockSizePrefixed(testPayload);
    const decompressed = decompress(compressed, Compression.Lz4);
    expect(decompressed).toEqual(testPayload);

    // Also test decompressLz4BlockSizePrefixed directly
    const direct = decompressLz4BlockSizePrefixed(compressed);
    expect(direct).toEqual(testPayload);
  });

  // spec: alhalte-pantry/compression 0010 lz4 empty payload
  it('decompresses empty LZ4 size-prefixed data', () => {
    const empty = new Uint8Array(0);
    const compressed = compressLz4BlockSizePrefixed(empty);
    const decompressed = decompress(compressed, Compression.Lz4);
    expect(decompressed.length).toBe(0);
  });

  // spec: alhalte-pantry/compression 0011 brotli
  it('decompresses 0011 Brotli (quality 11, lgwin 22) streams', () => {
    const compressed = new Uint8Array(
      zlib.brotliCompressSync(Buffer.from(testPayload), {
        params: {
          [zlib.constants.BROTLI_PARAM_QUALITY]: 11,
          [zlib.constants.BROTLI_PARAM_LGWIN]: 22,
        },
      }),
    );
    const decompressed = decompress(compressed, Compression.Brotli);
    expect(decompressed).toEqual(testPayload);
  });
});
