/**
 * Tin wire-format encoding helper for test fixture generation and client-side canner support.
 */

import { init as initZstd, compress as zstdCompress } from '@bokuweb/zstd-wasm';
import { compressLz4BlockSizePrefixed } from './compression.js';
import {
  ZERO_NONCE,
  applyIso7816Padding,
  computeBlake3Hash,
  deriveConvergentIdAndKey,
  encryptAes256GcmSiv,
  parseRoomKey,
} from './crypto.js';
import { PantryError } from './errors.js';
import { serializeOpener } from './opener.js';
import { Compression, Encryption } from './types.js';

let zstdInitialized = false;
async function ensureZstd(): Promise<void> {
  if (!zstdInitialized) {
    await initZstd();
    zstdInitialized = true;
  }
}

export interface EncodeTinOptions {
  /**
   * Room key for RoomKey or Convergent wrapped key encryption.
   */
  roomKey?: string | Uint8Array;

  /**
   * 12-byte nonce for RoomKey encryption (defaults to random bytes).
   */
  nonce?: Uint8Array;

  /**
   * 12-byte nonce for wrapped key encryption in Convergent mode (defaults to random bytes).
   */
  wrappedKeyNonce?: Uint8Array;

  /**
   * Optional target length for ISO/IEC 7816-4 (PADMÉ) padding before RoomKey encryption.
   */
  padmeTargetLength?: number;
}

export interface EncodedTinResult {
  wireBytes: Uint8Array;
  tinId: Uint8Array;
  tinIdHex: string;
  convergentKey?: Uint8Array;
  wrappedKey?: Uint8Array;
}

/**
 * Encodes a plaintext container (`label || payload`) into the wire format.
 */
export async function encodeTin(
  tin: { label: bigint; payload: Uint8Array },
  encryption: Encryption,
  compression: Compression,
  options?: EncodeTinOptions,
): Promise<EncodedTinResult> {
  // 1. Plaintext = label(8B, u64 LE) || payload
  const plaintext = new Uint8Array(8 + tin.payload.length);
  const view = new DataView(plaintext.buffer);
  view.setBigUint64(0, tin.label, true);
  plaintext.set(tin.payload, 8);

  // 2. Compress
  let compressed: Uint8Array;
  switch (compression) {
    case Compression.None:
      compressed = plaintext;
      break;

    case Compression.Zstd:
      await ensureZstd();
      compressed = zstdCompress(plaintext, 19);
      break;

    case Compression.Lz4:
      compressed = compressLz4BlockSizePrefixed(plaintext);
      break;

    case Compression.Brotli: {
      // Dynamic import of node:zlib if available
      try {
        const zlib = await import('node:zlib');
        compressed = new Uint8Array(
          zlib.brotliCompressSync(Buffer.from(plaintext), {
            params: {
              [zlib.constants.BROTLI_PARAM_QUALITY]: 11,
              [zlib.constants.BROTLI_PARAM_LGWIN]: 22,
            },
          }),
        );
      } catch (err) {
        throw new PantryError(`Brotli compression requires node:zlib: ${String(err)}`);
      }
      break;
    }

    default:
      throw new PantryError(`Unsupported compression: ${compression}`);
  }

  const openerByte = serializeOpener({ encryption, compression });

  // 3. Encrypt & assemble wire format
  switch (encryption) {
    case Encryption.Plaintext: {
      const wireBytes = new Uint8Array(1 + compressed.length);
      wireBytes[0] = openerByte;
      wireBytes.set(compressed, 1);
      const tinId = computeBlake3Hash(wireBytes);
      const tinIdHex = Array.from(tinId)
        .map((b) => b.toString(16).padStart(2, '0'))
        .join('');
      return { wireBytes, tinId, tinIdHex };
    }

    case Encryption.RoomKey: {
      if (!options?.roomKey) {
        throw new PantryError('Room key is required for RoomKey encryption');
      }
      const roomKey = parseRoomKey(options.roomKey);
      let toEncrypt = compressed;
      if (options.padmeTargetLength) {
        toEncrypt = applyIso7816Padding(toEncrypt, options.padmeTargetLength);
      }

      const nonce = options.nonce ?? crypto.getRandomValues(new Uint8Array(12));
      const ciphertext = encryptAes256GcmSiv(roomKey, nonce, toEncrypt);

      // Wire: opener(1B) || nonce(12B) || ciphertext
      const wireBytes = new Uint8Array(1 + 12 + ciphertext.length);
      wireBytes[0] = openerByte;
      wireBytes.set(nonce, 1);
      wireBytes.set(ciphertext, 13);

      const tinId = computeBlake3Hash(wireBytes);
      const tinIdHex = Array.from(tinId)
        .map((b) => b.toString(16).padStart(2, '0'))
        .join('');
      return { wireBytes, tinId, tinIdHex };
    }

    case Encryption.Convergent: {
      // Tin ID and convergent key are derived from plaintext BLAKE3 XOF 512
      const { tinId, convergentKey } = deriveConvergentIdAndKey(plaintext);
      const ciphertext = encryptAes256GcmSiv(convergentKey, ZERO_NONCE, compressed);

      // Wire: opener(1B) || ciphertext
      const wireBytes = new Uint8Array(1 + ciphertext.length);
      wireBytes[0] = openerByte;
      wireBytes.set(ciphertext, 1);

      let wrappedKey: Uint8Array | undefined;
      if (options?.roomKey) {
        const roomKey = parseRoomKey(options.roomKey);
        const wkNonce = options.wrappedKeyNonce ?? crypto.getRandomValues(new Uint8Array(12));
        const wkCiphertext = encryptAes256GcmSiv(roomKey, wkNonce, convergentKey);
        wrappedKey = new Uint8Array(12 + wkCiphertext.length);
        wrappedKey.set(wkNonce, 0);
        wrappedKey.set(wkCiphertext, 12);
      }

      const tinIdHex = Array.from(tinId)
        .map((b) => b.toString(16).padStart(2, '0'))
        .join('');
      return { wireBytes, tinId, tinIdHex, convergentKey, wrappedKey };
    }

    default:
      throw new PantryError(`Unsupported encryption: ${encryption}`);
  }
}
