/**
 * Cryptographic operations and helpers for `@alhalte/pantry`.
 *
 * Implements:
 * - AES-256-GCM-SIV encryption/decryption (RFC 8452)
 * - BLAKE3 XOF (512-bit) derivation for convergent encryption
 * - BLAKE3-256 hashing for tin IDs
 * - ISO/IEC 7816-4 (PADMÉ) padding detection and removal
 * - Key and Tin ID parsing/normalization
 *
 * References:
 * - ARCHITECTURE.md: `## tin-rs -> ### tin id, opener`, `## alhalte-canner`
 */

import { gcmsiv } from '@noble/ciphers/aes.js';
import { blake3 } from '@noble/hashes/blake3.js';
import { bytesToHex, hexToBytes } from '@noble/hashes/utils.js';
import { DecryptionError, PantryError } from './errors.js';

/**
 * 12-byte zero nonce used for convergent encryption (`0001`).
 */
export const ZERO_NONCE: Readonly<Uint8Array> = new Uint8Array(12);

/**
 * Encrypts `plaintext` using AES-256-GCM-SIV with a 256-bit key and 96-bit nonce.
 */
export function encryptAes256GcmSiv(
  key: Uint8Array,
  nonce: Uint8Array,
  plaintext: Uint8Array,
): Uint8Array {
  if (key.length !== 32) {
    throw new PantryError(`AES-256-GCM-SIV key must be 32 bytes; received ${key.length}`);
  }
  if (nonce.length !== 12) {
    throw new PantryError(`AES-256-GCM-SIV nonce must be 12 bytes; received ${nonce.length}`);
  }
  try {
    const cipher = gcmsiv(key, nonce);
    return cipher.encrypt(plaintext);
  } catch (err) {
    throw new PantryError(`AES-256-GCM-SIV encryption failed: ${String(err)}`);
  }
}

/**
 * Decrypts `ciphertext` using AES-256-GCM-SIV with a 256-bit key and 96-bit nonce.
 */
export function decryptAes256GcmSiv(
  key: Uint8Array,
  nonce: Uint8Array,
  ciphertext: Uint8Array,
): Uint8Array {
  if (key.length !== 32) {
    throw new DecryptionError(`AES-256-GCM-SIV key must be 32 bytes; received ${key.length}`);
  }
  if (nonce.length !== 12) {
    throw new DecryptionError(`AES-256-GCM-SIV nonce must be 12 bytes; received ${nonce.length}`);
  }
  try {
    const cipher = gcmsiv(key, nonce);
    return cipher.decrypt(ciphertext);
  } catch (err) {
    throw new DecryptionError(`AES-256-GCM-SIV decryption failed: ${String(err)}`, err);
  }
}

/**
 * Derives the 256-bit TinId and 256-bit convergent encryption key from `plaintext`
 * using BLAKE3 XOF 512-bit output.
 *
 * First 32 bytes (256 bits) form the `tinId`.
 * Remaining 32 bytes (256 bits) form the AES-256-GCM-SIV `convergentKey`.
 */
export function deriveConvergentIdAndKey(plaintext: Uint8Array): {
  tinId: Uint8Array;
  convergentKey: Uint8Array;
} {
  const xof = blake3(plaintext, { dkLen: 64 });
  const tinId = xof.slice(0, 32);
  const convergentKey = xof.slice(32, 64);
  return { tinId, convergentKey };
}

/**
 * Computes standard BLAKE3-256 hash (32 bytes) of `data`.
 */
export function computeBlake3Hash(data: Uint8Array): Uint8Array {
  return blake3(data, { dkLen: 32 });
}

/**
 * Checks whether `data` ends with ISO/IEC 7816-4 padding (`0x80` followed by `0x00*`).
 */
export function hasIso7816Padding(data: Uint8Array): boolean {
  if (data.length === 0) return false;
  let i = data.length - 1;
  while (i >= 0 && data[i] === 0x00) {
    i--;
  }
  return i >= 0 && data[i] === 0x80;
}

/**
 * Removes ISO/IEC 7816-4 padding (末尾の `0x00*` と `0x80`) from `data`.
 * If padding is not present, returns `data` unchanged.
 */
export function removeIso7816Padding(data: Uint8Array): Uint8Array {
  if (data.length === 0) return data;
  let i = data.length - 1;
  while (i >= 0 && data[i] === 0x00) {
    i--;
  }
  if (i >= 0 && data[i] === 0x80) {
    return data.subarray(0, i);
  }
  return data;
}

/**
 * Applies ISO/IEC 7816-4 padding: appends `0x80` followed by `0x00` bytes
 * up to `targetLength`.
 */
export function applyIso7816Padding(data: Uint8Array, targetLength: number): Uint8Array {
  if (targetLength <= data.length) {
    throw new PantryError(
      `Target length (${targetLength}) must be greater than data length (${data.length}) for ISO/IEC 7816-4 padding`,
    );
  }
  const padded = new Uint8Array(targetLength);
  padded.set(data, 0);
  padded[data.length] = 0x80;
  // Remaining bytes are initialized to 0 by Uint8Array
  return padded;
}

/**
 * Parses and normalizes a 256-bit (32-byte) room key from:
 * - A 32-byte `Uint8Array`
 * - A 64-character hex string (optional `#` prefix or URL fragment)
 * - A Base64 or Base64URL string (43/44 chars)
 * - A full URL string with fragment (e.g. `https://host/rooms/1#abcdef...`)
 */
export function parseRoomKey(keyInput: string | Uint8Array): Uint8Array {
  if (keyInput instanceof Uint8Array) {
    if (keyInput.length !== 32) {
      throw new PantryError(
        `Room key Uint8Array must be 32 bytes (256 bits); received ${keyInput.length}`,
      );
    }
    return new Uint8Array(keyInput);
  }

  let str = keyInput.trim();

  // If input is a URL or fragment containing '#'
  const hashIndex = str.indexOf('#');
  if (hashIndex !== -1) {
    str = str.slice(hashIndex + 1).trim();
  }

  // Check 64-character hex
  if (/^[0-9a-fA-F]{64}$/.test(str)) {
    return hexToBytes(str);
  }

  // Try base64 / base64url decoding
  try {
    const base64 = str.replace(/-/g, '+').replace(/_/g, '/');
    const padded = base64.padEnd(Math.ceil(base64.length / 4) * 4, '=');
    const binStr = atob(padded);
    if (binStr.length === 32) {
      const bytes = new Uint8Array(32);
      for (let i = 0; i < 32; i++) {
        bytes[i] = binStr.charCodeAt(i);
      }
      return bytes;
    }
  } catch {
    // Ignore base64 decode failure and throw general error below
  }

  throw new PantryError(
    `Invalid room key format. Expected 32-byte Uint8Array, 64-hex string, or 32-byte base64/URL fragment.`,
  );
}

/**
 * Normalizes a Tin ID (hex string or 32-byte Uint8Array) into both hex and byte representations.
 */
export function normalizeTinId(id: string | Uint8Array): { hex: string; bytes: Uint8Array } {
  if (id instanceof Uint8Array) {
    if (id.length !== 32) {
      throw new PantryError(`Tin ID Uint8Array must be 32 bytes; received ${id.length}`);
    }
    const hex = bytesToHex(id).toLowerCase();
    return { hex, bytes: new Uint8Array(id) };
  }

  const trimmed = id.trim().toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(trimmed)) {
    throw new PantryError(
      `Tin ID string must be a 64-character hexadecimal string; received "${id}"`,
    );
  }
  const bytes = hexToBytes(trimmed);
  return { hex: trimmed, bytes };
}

export { bytesToHex, hexToBytes };
