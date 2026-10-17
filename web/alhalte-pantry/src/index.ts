/**
 * `@alhalte/pantry` - Client-side tin cache and decryption manager for Alhalte.
 */

// Core class
export { Pantry } from './pantry.js';

// Types and enums
export {
  Compression,
  Encryption,
  type CellarClient,
  type CellarResponse,
  type DecryptedTin,
  type GetTinOptions,
  type Opener,
  type PantryCacheAdapter,
  type PantryOptions,
} from './types.js';

// Opener
export { hasWireNonce, parseOpener, serializeOpener } from './opener.js';

// Cryptographic primitives
export {
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
} from './crypto.js';

// Compression
export {
  compressLz4BlockSizePrefixed,
  decompress,
  decompressLz4BlockSizePrefixed,
} from './compression.js';

// Codec
export { decodeTin, type DecodeTinOptions } from './codec.js';

// Encoder helper
export { encodeTin, type EncodeTinOptions, type EncodedTinResult } from './encoder.js';

// Labels
export {
  LABEL_ALHALTE_TIN_ID,
  LABEL_ALHALTE_UNIX_TIME,
  LABEL_NEED_NOT_KNOW,
  LABEL_PRIMITIVE_BOOL,
  LABEL_PRIMITIVE_STRING,
  LABEL_PRIMITIVE_U8,
  LABEL_PRIMITIVE_U16,
  LABEL_PRIMITIVE_U32,
  LABEL_PRIMITIVE_U64,
  labelHash,
  xxh64,
} from './label.js';

// Cache adapters
export {
  CacheStoragePantryCacheAdapter,
  IndexedDbPantryCacheAdapter,
  MemoryPantryCacheAdapter,
  createDefaultCacheAdapter,
  type DefaultCacheAdapterOptions,
} from './cache/index.js';

// Cellar clients
export {
  HttpCellarClient,
  InMemoryCellarClient,
  type HttpCellarClientOptions,
} from './cellar/index.js';

// Errors
export {
  CellarError,
  DecompressionError,
  DecryptionError,
  IntegrityError,
  MissingKeyError,
  MissingWrappedKeyError,
  OpenerError,
  PantryError,
  UnsupportedCompressionError,
  UnsupportedEncryptionError,
  WireFormatError,
} from './errors.js';
