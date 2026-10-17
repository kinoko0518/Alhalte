/**
 * Wire format encoding and decoding for Tins.
 *
 * References:
 * - ARCHITECTURE.md: `## tin-rs -> ### バイト列表現, ### tin id, ### opener`
 * - ARCHITECTURE.md: `## alhalte-canner -> encrypt-high-entropy, encrypt-low-entropy`
 */

import { decompress } from './compression.js';
import {
  ZERO_NONCE,
  bytesToHex,
  computeBlake3Hash,
  decryptAes256GcmSiv,
  deriveConvergentIdAndKey,
  hasIso7816Padding,
  normalizeTinId,
  parseRoomKey,
  removeIso7816Padding,
} from './crypto.js';
import {
  DecryptionError,
  IntegrityError,
  MissingKeyError,
  MissingWrappedKeyError,
  WireFormatError,
} from './errors.js';
import { hasWireNonce, parseOpener } from './opener.js';
import { Compression, type DecryptedTin, Encryption } from './types.js';

export interface DecodeTinOptions {
  /**
   * 256-bit room key (required for RoomKey and Convergent encryption).
   */
  roomKey?: string | Uint8Array | null;

  /**
   * 60-byte wrapped key from cellar (`12B nonce || 48B ciphertext`).
   * Required for Convergent encryption.
   */
  wrappedKey?: Uint8Array | null;

  /**
   * Expected Tin ID to verify against the decoded wire bytes.
   */
  expectedTinId?: string | Uint8Array | null;

  /**
   * Whether to strip ISO/IEC 7816-4 (PADMÉ) padding for RoomKey encrypted tins.
   * Defaults to `true`.
   */
  stripPadme?: boolean;
}

/**
 * Decodes raw wire bytes into a `DecryptedTin`.
 *
 * Handles:
 * - Plaintext (`0000`): direct decompression.
 * - RoomKey (`0010`): AES-256-GCM-SIV decryption with room key and wire nonce,
 *   optional PADMÉ unpadding, and decompression.
 * - Convergent (`0001`): AES-256-GCM-SIV decryption of wrapped key with room key,
 *   AES-256-GCM-SIV decryption of tin body with zero nonce and derived key,
 *   decompression, and BLAKE3 XOF (512-bit) integrity verification.
 */
export function decodeTin(wireBytes: Uint8Array, options?: DecodeTinOptions): DecryptedTin {
  if (wireBytes.length === 0) {
    throw new WireFormatError('Wire bytes cannot be empty');
  }

  const opener = parseOpener(wireBytes[0]);
  const stripPadme = options?.stripPadme !== false;

  let plaintext: Uint8Array;
  let resolvedTinId: Uint8Array;

  // Resolve room key if provided
  let roomKeyBytes: Uint8Array | null = null;
  if (options?.roomKey) {
    roomKeyBytes = parseRoomKey(options.roomKey);
  }

  // Normalize expected Tin ID if provided
  let expectedIdBytes: Uint8Array | null = null;
  if (options?.expectedTinId) {
    expectedIdBytes = normalizeTinId(options.expectedTinId).bytes;
  }

  switch (opener.encryption) {
    case Encryption.Plaintext: {
      const body = wireBytes.subarray(1);
      plaintext = decompress(body, opener.compression);
      resolvedTinId = computeBlake3Hash(wireBytes);
      break;
    }

    case Encryption.RoomKey: {
      if (!roomKeyBytes) {
        throw new MissingKeyError('Room key is required to decrypt 0010 (RoomKey) tin');
      }
      if (wireBytes.length < 13) {
        throw new WireFormatError(
          `RoomKey wire bytes too short: expected at least 13 bytes (opener + 12B nonce), received ${wireBytes.length}`,
        );
      }

      const nonce = wireBytes.subarray(1, 13);
      const ciphertext = wireBytes.subarray(13);

      resolvedTinId = computeBlake3Hash(wireBytes);

      // Decrypt ciphertext using room key and wire nonce
      let decrypted = decryptAes256GcmSiv(roomKeyBytes, nonce, ciphertext);

      // Handle ISO/IEC 7816-4 (PADMÉ) padding and decompression
      if (opener.compression === Compression.None) {
        if (stripPadme && hasIso7816Padding(decrypted)) {
          plaintext = removeIso7816Padding(decrypted);
        } else {
          plaintext = decrypted;
        }
      } else {
        // If compressed, padding may have been applied to compressed bytes
        if (stripPadme && hasIso7816Padding(decrypted)) {
          try {
            plaintext = decompress(removeIso7816Padding(decrypted), opener.compression);
          } catch {
            // Fall back to decompressing without unpadding first
            plaintext = decompress(decrypted, opener.compression);
          }
        } else {
          plaintext = decompress(decrypted, opener.compression);
        }
      }
      break;
    }

    case Encryption.Convergent: {
      if (!roomKeyBytes) {
        throw new MissingKeyError('Room key is required to unwrap convergent key for 0001 tin');
      }
      if (!options?.wrappedKey || options.wrappedKey.length === 0) {
        throw new MissingWrappedKeyError(
          'Wrapped key (nonce || ciphertext) is required to decrypt 0001 (Convergent) tin',
        );
      }
      if (options.wrappedKey.length < 13) {
        throw new WireFormatError(
          `Wrapped key too short: expected 60 bytes (12B nonce + 48B ciphertext), received ${options.wrappedKey.length}`,
        );
      }

      const wkNonce = options.wrappedKey.subarray(0, 12);
      const wkCiphertext = options.wrappedKey.subarray(12);

      // 1. Decrypt wrapped key using room key -> 32-byte convergent key
      const convergentKey = decryptAes256GcmSiv(roomKeyBytes, wkNonce, wkCiphertext);
      if (convergentKey.length !== 32) {
        throw new DecryptionError(
          `Decrypted convergent key length must be 32 bytes; received ${convergentKey.length}`,
        );
      }

      // 2. Decrypt tin body using convergent key and ZERO_NONCE
      const ciphertext = wireBytes.subarray(1);
      const compressed = decryptAes256GcmSiv(convergentKey, ZERO_NONCE, ciphertext);

      // 3. Decompress body
      plaintext = decompress(compressed, opener.compression);

      // 4. Verify BLAKE3 XOF (512-bit) integrity: first 256b is TinId, next 256b is convergent key
      const { tinId: derivedId, convergentKey: derivedKey } =
        deriveConvergentIdAndKey(plaintext);

      // Verify convergent key match
      if (!arraysEqual(derivedKey, convergentKey)) {
        throw new IntegrityError('Convergent key derived from plaintext does not match unwrapped key');
      }

      resolvedTinId = derivedId;
      break;
    }

    default:
      throw new WireFormatError(`Unsupported encryption: ${opener.encryption}`);
  }

  // Verify Tin ID if an expected ID was provided
  if (expectedIdBytes && !arraysEqual(resolvedTinId, expectedIdBytes)) {
    throw new IntegrityError(
      `Tin ID mismatch: expected ${bytesToHex(expectedIdBytes)}, got ${bytesToHex(resolvedTinId)}`,
    );
  }

  // Extract label (first 8 bytes u64 LE) and payload
  if (plaintext.length < 8) {
    throw new WireFormatError(
      `Plaintext too short for label: expected at least 8 bytes, received ${plaintext.length}`,
    );
  }

  const view = new DataView(plaintext.buffer, plaintext.byteOffset, plaintext.byteLength);
  const label = view.getBigUint64(0, true);
  const payload = plaintext.subarray(8);

  return {
    tinId: bytesToHex(resolvedTinId).toLowerCase(),
    label,
    payload,
    opener,
  };
}

/**
 * Compares two byte arrays for equality.
 */
function arraysEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}
