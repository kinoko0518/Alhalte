/**
 * XXH64 label hashing and standard label constants.
 *
 * References:
 * - ARCHITECTURE.md: `## tin-rs -> ### label`
 */

const PRIME64_1 = 0x9e3779b185ebca87n;
const PRIME64_2 = 0xc2b2ae3d27d4eb4fn;
const PRIME64_3 = 0x165667b19e3779f9n;
const PRIME64_4 = 0x85ebca77c2b2ae63n;
const PRIME64_5 = 0x27d4eb2f165667c5n;
const MASK64 = 0xffffffffffffffffn;

function rotl64(x: bigint, r: bigint): bigint {
  return (((x << r) & MASK64) | ((x & MASK64) >> (64n - r))) & MASK64;
}

function round64(acc: bigint, input: bigint): bigint {
  acc = (acc + input * PRIME64_2) & MASK64;
  acc = rotl64(acc, 31n);
  return (acc * PRIME64_1) & MASK64;
}

function mergeRound64(acc: bigint, val: bigint): bigint {
  val = round64(0n, val);
  acc = acc ^ val;
  return (acc * PRIME64_1 + PRIME64_4) & MASK64;
}

/**
 * Computes 64-bit XXH64 hash with seed 0.
 * Produces identical values to `xxhash-rust::const_xxh64(input, 0)`.
 */
export function xxh64(input: string | Uint8Array, seed = 0n): bigint {
  const data = typeof input === 'string' ? new TextEncoder().encode(input) : input;
  const len = BigInt(data.length);
  let pos = 0;
  let h64 = 0n;

  if (data.length >= 32) {
    let v1 = (seed + PRIME64_1 + PRIME64_2) & MASK64;
    let v2 = (seed + PRIME64_2) & MASK64;
    let v3 = seed & MASK64;
    let v4 = (seed - PRIME64_1) & MASK64;

    const limit = data.length - 32;
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);

    while (pos <= limit) {
      v1 = round64(v1, view.getBigUint64(pos, true));
      pos += 8;
      v2 = round64(v2, view.getBigUint64(pos, true));
      pos += 8;
      v3 = round64(v3, view.getBigUint64(pos, true));
      pos += 8;
      v4 = round64(v4, view.getBigUint64(pos, true));
      pos += 8;
    }

    h64 = rotl64(v1, 1n) + rotl64(v2, 7n) + rotl64(v3, 12n) + rotl64(v4, 18n);
    h64 = mergeRound64(h64, v1);
    h64 = mergeRound64(h64, v2);
    h64 = mergeRound64(h64, v3);
    h64 = mergeRound64(h64, v4);
  } else {
    h64 = (seed + PRIME64_5) & MASK64;
  }

  h64 = (h64 + len) & MASK64;

  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);

  while (pos + 8 <= data.length) {
    const k1 = round64(0n, view.getBigUint64(pos, true));
    h64 = rotl64(h64 ^ k1, 27n);
    h64 = (h64 * PRIME64_1 + PRIME64_4) & MASK64;
    pos += 8;
  }

  if (pos + 4 <= data.length) {
    const k1 = BigInt(view.getUint32(pos, true));
    h64 = rotl64(h64 ^ ((k1 * PRIME64_1) & MASK64), 23n);
    h64 = (h64 * PRIME64_2 + PRIME64_3) & MASK64;
    pos += 4;
  }

  while (pos < data.length) {
    const k1 = BigInt(data[pos]);
    h64 = rotl64(h64 ^ ((k1 * PRIME64_5) & MASK64), 11n);
    h64 = (h64 * PRIME64_1) & MASK64;
    pos++;
  }

  // Final avalanche
  h64 = h64 ^ ((h64 >> 33n) & MASK64);
  h64 = (h64 * PRIME64_2) & MASK64;
  h64 = h64 ^ ((h64 >> 29n) & MASK64);
  h64 = (h64 * PRIME64_3) & MASK64;
  h64 = h64 ^ ((h64 >> 32n) & MASK64);

  return h64;
}

/**
 * Computes the 64-bit label for a format identifier string using XXH64 seed 0.
 */
export function labelHash(ident: string): bigint {
  return xxh64(ident, 0n);
}

// Standard labels
export const LABEL_NEED_NOT_KNOW = 0n;
export const LABEL_PRIMITIVE_STRING = labelHash('primitive/string');
export const LABEL_PRIMITIVE_BOOL = labelHash('primitive/bool');
export const LABEL_PRIMITIVE_U8 = labelHash('primitive/u8');
export const LABEL_PRIMITIVE_U16 = labelHash('primitive/u16');
export const LABEL_PRIMITIVE_U32 = labelHash('primitive/u32');
export const LABEL_PRIMITIVE_U64 = labelHash('primitive/u64');
export const LABEL_ALHALTE_UNIX_TIME = labelHash('alhalte/unix-time');
export const LABEL_ALHALTE_TIN_ID = labelHash('alhalte/tin-id');
