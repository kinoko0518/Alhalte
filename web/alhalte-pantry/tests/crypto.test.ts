import { describe, expect, it } from 'vitest';
import {
  ZERO_NONCE,
  applyIso7816Padding,
  bytesToHex,
  computeBlake3Hash,
  decryptAes256GcmSiv,
  deriveConvergentIdAndKey,
  encryptAes256GcmSiv,
  hasIso7816Padding,
  hexToBytes,
  normalizeTinId,
  parseRoomKey,
  removeIso7816Padding,
} from '../src/index.js';

describe('Crypto & Utilities', () => {
  const sampleKey = new Uint8Array(32).fill(0x5a);
  const sampleNonce = new Uint8Array(12).fill(0x3c);
  const sampleData = new TextEncoder().encode('Cryptographic primitives test payload');

  // spec: alhalte-pantry/crypto aes-256-gcm-siv
  it('encrypts and decrypts with AES-256-GCM-SIV', () => {
    const ciphertext = encryptAes256GcmSiv(sampleKey, sampleNonce, sampleData);
    // Ciphertext has 16-byte authentication tag appended
    expect(ciphertext.length).toBe(sampleData.length + 16);

    const decrypted = decryptAes256GcmSiv(sampleKey, sampleNonce, ciphertext);
    expect(decrypted).toEqual(sampleData);
  });

  // spec: alhalte-pantry/crypto aes-256-gcm-siv zero nonce
  it('supports zero nonce encryption and decryption for convergent mode', () => {
    const ciphertext = encryptAes256GcmSiv(sampleKey, ZERO_NONCE, sampleData);
    const decrypted = decryptAes256GcmSiv(sampleKey, ZERO_NONCE, ciphertext);
    expect(decrypted).toEqual(sampleData);
  });

  // spec: alhalte-pantry/crypto convergent-key-derivation blake3 xof 512
  it('derives 256-bit tinId and 256-bit convergentKey via BLAKE3 XOF 512', () => {
    const { tinId, convergentKey } = deriveConvergentIdAndKey(sampleData);
    expect(tinId.length).toBe(32);
    expect(convergentKey.length).toBe(32);
    expect(tinId).not.toEqual(convergentKey);

    // Deterministic: same plaintext yields identical tinId and key
    const second = deriveConvergentIdAndKey(sampleData);
    expect(second.tinId).toEqual(tinId);
    expect(second.convergentKey).toEqual(convergentKey);
  });

  // spec: alhalte-pantry/crypto padme iso-iec-7816-4 padding
  it('applies, detects, and removes ISO/IEC 7816-4 (PADMÉ) padding', () => {
    const data = new Uint8Array([1, 2, 3, 4, 5]);
    expect(hasIso7816Padding(data)).toBe(false);

    // Pad to 16 bytes
    const padded = applyIso7816Padding(data, 16);
    expect(padded.length).toBe(16);
    expect(padded[5]).toBe(0x80);
    expect(padded.slice(6).every((b) => b === 0)).toBe(true);
    expect(hasIso7816Padding(padded)).toBe(true);

    const unpadded = removeIso7816Padding(padded);
    expect(unpadded).toEqual(data);
  });

  // spec: alhalte-pantry/crypto padme single byte 0x80 padding
  it('removes padding when only 0x80 is appended with no trailing zeros', () => {
    const data = new Uint8Array([10, 20, 30]);
    const padded = applyIso7816Padding(data, 4);
    expect(padded).toEqual(new Uint8Array([10, 20, 30, 0x80]));
    expect(hasIso7816Padding(padded)).toBe(true);

    const unpadded = removeIso7816Padding(padded);
    expect(unpadded).toEqual(data);
  });

  // spec: alhalte-pantry/crypto room-key url-fragment parsing
  it('parses room keys from hex, raw bytes, URL fragments, and full URLs', () => {
    const keyBytes = new Uint8Array(32).fill(0xab);
    const hex = bytesToHex(keyBytes);

    // 1. Direct Uint8Array
    expect(parseRoomKey(keyBytes)).toEqual(keyBytes);

    // 2. 64-char hex string
    expect(parseRoomKey(hex)).toEqual(keyBytes);

    // 3. URL fragment (#hex)
    expect(parseRoomKey(`#${hex}`)).toEqual(keyBytes);

    // 4. Full room invitation URL
    const roomUrl = `https://alhalte.vtt.example/rooms/42#${hex}`;
    expect(parseRoomKey(roomUrl)).toEqual(keyBytes);
  });

  // spec: alhalte-pantry/crypto normalize-tin-id
  it('normalizes tin IDs from 32-byte Uint8Array and 64-char hex strings', () => {
    const bytes = computeBlake3Hash(sampleData);
    const hex = bytesToHex(bytes);

    const fromBytes = normalizeTinId(bytes);
    expect(fromBytes.hex).toBe(hex.toLowerCase());
    expect(fromBytes.bytes).toEqual(bytes);

    const fromHex = normalizeTinId(hex.toUpperCase());
    expect(fromHex.hex).toBe(hex.toLowerCase());
    expect(fromHex.bytes).toEqual(bytes);
  });
});
