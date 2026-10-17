/**
 * Types and interfaces for `@alhalte/pantry`.
 *
 * References:
 * - ARCHITECTURE.md: `## tin-rs`, `## alhalte-pantry`, `## alhalte-canner`
 */

/**
 * Encryption mode representation in the upper 4 bits of the Opener.
 */
export enum Encryption {
  /** 0000: Plaintext (unencrypted payload). */
  Plaintext = 0x0,
  /** 0001: Convergent encryption (BLAKE3 XOF 512, zero nonce AES-256-GCM-SIV). */
  Convergent = 0x1,
  /** 0010: Room key encryption (random 96-bit Nonce, AES-256-GCM-SIV). */
  RoomKey = 0x2,
}

/**
 * Compression algorithm representation in the lower 4 bits of the Opener.
 */
export enum Compression {
  /** 0000: No compression. */
  None = 0x0,
  /** 0001: Zstandard compression. */
  Zstd = 0x1,
  /** 0010: LZ4 block compression with prepended uncompressed size (u32 LE). */
  Lz4 = 0x2,
  /** 0011: Brotli compression. */
  Brotli = 0x3,
}

/**
 * Parsed Opener header (1 byte).
 */
export interface Opener {
  /** Upper 4 bits: encryption mode. */
  encryption: Encryption;
  /** Lower 4 bits: compression algorithm. */
  compression: Compression;
}

/**
 * Decrypted and decompressed Tin container.
 */
export interface DecryptedTin {
  /** 256-bit Tin ID as 64-character lowercase hex string. */
  tinId: string;
  /** Format identifier as 64-bit unsigned integer (u64 LE). */
  label: bigint;
  /** Raw payload bytes. */
  payload: Uint8Array;
  /** Opener metadata specifying original encryption and compression. */
  opener: Opener;
}

/**
 * Response received when downloading a Tin from `alhalte-cellar`.
 */
export interface CellarResponse {
  /** Encrypted or plaintext wire bytes of the tin. */
  tinBytes: Uint8Array;
  /** Optional wrapped key (60 bytes: 12B nonce || 48B ciphertext) for convergent tins. */
  wrappedKey?: Uint8Array | null;
}

/**
 * Client interface for downloading tins from `alhalte-cellar`.
 */
export interface CellarClient {
  /**
   * Downloads the raw tin wire bytes (and wrapped key if convergent) from cellar.
   *
   * @param tinId 64-character hex Tin ID or 32-byte Uint8Array.
   */
  fetchTin(tinId: string): Promise<CellarResponse>;
}

/**
 * Pluggable cache storage adapter interface for pantry.
 * Supports CacheStorage, IndexedDB, and in-memory implementations.
 */
export interface PantryCacheAdapter {
  /**
   * Retrieves a cached decrypted Tin by its 64-character hex ID.
   */
  get(tinId: string): Promise<DecryptedTin | null>;

  /**
   * Stores a decrypted Tin in the cache.
   */
  set(tinId: string, tin: DecryptedTin): Promise<void>;

  /**
   * Checks if a Tin exists in the cache.
   */
  has(tinId: string): Promise<boolean>;

  /**
   * Removes a Tin from the cache.
   */
  delete(tinId: string): Promise<void>;

  /**
   * Clears all cached Tins.
   */
  clear(): Promise<void>;
}

/**
 * Options for configuring the `Pantry` instance.
 */
export interface PantryOptions {
  /**
   * Pluggable cache adapter. Defaults to `MemoryPantryCacheAdapter` in Node/SSR,
   * or auto-detected browser cache (`CacheStorage` / `IndexedDB`) when available.
   */
  cache?: PantryCacheAdapter;

  /**
   * Cellar client used to download tins when cache misses occur.
   */
  cellar?: CellarClient;

  /**
   * 256-bit room key (32 bytes or 64 hex characters or URL/fragment with `#...`).
   * Can also be set dynamically or extracted from URL fragment.
   */
  roomKey?: string | Uint8Array | (() => string | Uint8Array | null | undefined);

  /**
   * Whether to remove ISO/IEC 7816-4 (PADMÉ) padding for RoomKey encrypted tins.
   * Defaults to `true`.
   */
  stripPadme?: boolean;
}

/**
 * Options passed to `pantry.getTin()`.
 */
export interface GetTinOptions {
  /**
   * Overrides the room key for this specific request.
   */
  roomKey?: string | Uint8Array;

  /**
   * Explicitly provided wrapped key (if not fetched from cellar).
   */
  wrappedKey?: Uint8Array | null;

  /**
   * Whether to bypass the cache and force fetching from cellar.
   */
  forceFetch?: boolean;

  /**
   * Whether to strip ISO/IEC 7816-4 (PADMÉ) padding on RoomKey decryption.
   */
  stripPadme?: boolean;
}
