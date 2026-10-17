/**
 * Opener parsing and serialization.
 *
 * References:
 * - ARCHITECTURE.md: `## tin-rs -> ### opener`
 */

import { UnsupportedCompressionError, UnsupportedEncryptionError } from './errors.js';
import { Compression, Encryption, type Opener } from './types.js';

/**
 * Checks whether the given encryption mode includes a 12-byte Nonce in the wire format.
 * Only `RoomKey` (`0010`) includes a nonce directly in the wire format.
 */
export function hasWireNonce(encryption: Encryption): boolean {
  return encryption === Encryption.RoomKey;
}

/**
 * Parses an 8-bit byte into an `Opener`.
 *
 * Upper 4 bits represent the encryption mode.
 * Lower 4 bits represent the compression algorithm.
 */
export function parseOpener(byte: number): Opener {
  const encNibble = (byte >> 4) & 0x0f;
  const compNibble = byte & 0x0f;

  if (encNibble !== Encryption.Plaintext &&
      encNibble !== Encryption.Convergent &&
      encNibble !== Encryption.RoomKey) {
    throw new UnsupportedEncryptionError(encNibble);
  }

  if (compNibble !== Compression.None &&
      compNibble !== Compression.Zstd &&
      compNibble !== Compression.Lz4 &&
      compNibble !== Compression.Brotli) {
    throw new UnsupportedCompressionError(compNibble);
  }

  return {
    encryption: encNibble as Encryption,
    compression: compNibble as Compression,
  };
}

/**
 * Serializes an `Opener` into a single byte.
 */
export function serializeOpener(opener: Opener): number {
  return ((opener.encryption & 0x0f) << 4) | (opener.compression & 0x0f);
}
