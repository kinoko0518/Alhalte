import { describe, expect, it } from 'vitest';
import {
  Compression,
  Encryption,
  UnsupportedCompressionError,
  UnsupportedEncryptionError,
  hasWireNonce,
  parseOpener,
  serializeOpener,
} from '../src/index.js';

describe('Opener', () => {
  // spec: alhalte-pantry/opener 0000 plaintext
  it('parses and serializes 0000 Plaintext opener with all compression algorithms', () => {
    const compressions = [
      { comp: Compression.None, byte: 0x00 },
      { comp: Compression.Zstd, byte: 0x01 },
      { comp: Compression.Lz4, byte: 0x02 },
      { comp: Compression.Brotli, byte: 0x03 },
    ];

    for (const { comp, byte } of compressions) {
      const opener = parseOpener(byte);
      expect(opener.encryption).toBe(Encryption.Plaintext);
      expect(opener.compression).toBe(comp);
      expect(serializeOpener(opener)).toBe(byte);
      expect(hasWireNonce(opener.encryption)).toBe(false);
    }
  });

  // spec: alhalte-pantry/opener 0001 convergent
  it('parses and serializes 0001 Convergent opener with all compression algorithms', () => {
    const compressions = [
      { comp: Compression.None, byte: 0x10 },
      { comp: Compression.Zstd, byte: 0x11 },
      { comp: Compression.Lz4, byte: 0x12 },
      { comp: Compression.Brotli, byte: 0x13 },
    ];

    for (const { comp, byte } of compressions) {
      const opener = parseOpener(byte);
      expect(opener.encryption).toBe(Encryption.Convergent);
      expect(opener.compression).toBe(comp);
      expect(serializeOpener(opener)).toBe(byte);
      expect(hasWireNonce(opener.encryption)).toBe(false);
    }
  });

  // spec: alhalte-pantry/opener 0010 room-key
  it('parses and serializes 0010 RoomKey opener and identifies wire nonce requirement', () => {
    const compressions = [
      { comp: Compression.None, byte: 0x20 },
      { comp: Compression.Zstd, byte: 0x21 },
      { comp: Compression.Lz4, byte: 0x22 },
      { comp: Compression.Brotli, byte: 0x23 },
    ];

    for (const { comp, byte } of compressions) {
      const opener = parseOpener(byte);
      expect(opener.encryption).toBe(Encryption.RoomKey);
      expect(opener.compression).toBe(comp);
      expect(serializeOpener(opener)).toBe(byte);
      expect(hasWireNonce(opener.encryption)).toBe(true);
    }
  });

  // spec: alhalte-pantry/opener unsupported encryption
  it('rejects unsupported encryption modes (> 0x2)', () => {
    for (let enc = 0x3; enc <= 0x0f; enc++) {
      const byte = (enc << 4) | 0x00;
      expect(() => parseOpener(byte)).toThrow(UnsupportedEncryptionError);
    }
  });

  // spec: alhalte-pantry/opener unsupported compression
  it('rejects unsupported compression algorithms (> 0x3)', () => {
    for (let comp = 0x4; comp <= 0x0f; comp++) {
      const byte = (0x0 << 4) | comp;
      expect(() => parseOpener(byte)).toThrow(UnsupportedCompressionError);
    }
  });
});
