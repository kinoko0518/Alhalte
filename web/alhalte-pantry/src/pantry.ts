/**
 * The core `Pantry` client-side cache and decryption manager.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-pantry`, `## tin-rs`, `## alhalte-cellar`
 */

import { createDefaultCacheAdapter } from './cache/index.js';
import { decodeTin } from './codec.js';
import { normalizeTinId, parseRoomKey } from './crypto.js';
import { CellarError, PantryError } from './errors.js';
import type {
  CellarClient,
  DecryptedTin,
  GetTinOptions,
  PantryCacheAdapter,
  PantryOptions,
} from './types.js';

export class Pantry {
  private readonly cache: PantryCacheAdapter;
  private readonly cellar?: CellarClient;
  private roomKeyProvider?: () => string | Uint8Array | null | undefined;
  private staticRoomKey: Uint8Array | null = null;
  private readonly stripPadme: boolean;

  /**
   * Active in-flight download and decryption requests for request deduplication.
   * Maps 64-character hex tinId to the active Promise.
   */
  private readonly inFlightRequests = new Map<string, Promise<DecryptedTin>>();

  constructor(options?: PantryOptions) {
    this.cache = options?.cache ?? createDefaultCacheAdapter();
    this.cellar = options?.cellar;
    this.stripPadme = options?.stripPadme !== false;

    if (options?.roomKey) {
      if (typeof options.roomKey === 'function') {
        this.roomKeyProvider = options.roomKey;
      } else {
        this.staticRoomKey = parseRoomKey(options.roomKey);
      }
    }
  }

  /**
   * Sets or clears the active 256-bit room key.
   */
  setRoomKey(key: string | Uint8Array | null | undefined): void {
    if (!key) {
      this.staticRoomKey = null;
      this.roomKeyProvider = undefined;
    } else {
      this.staticRoomKey = parseRoomKey(key);
    }
  }

  /**
   * Sets a dynamic room key provider function (e.g. reading from URL fragment).
   */
  setRoomKeyProvider(provider: () => string | Uint8Array | null | undefined): void {
    this.roomKeyProvider = provider;
  }

  /**
   * Resolves the current 256-bit room key, checking:
   * 1. Explicit static room key
   * 2. Custom room key provider function
   * 3. Browser `window.location.hash` URL fragment if available
   */
  getRoomKey(): Uint8Array | null {
    if (this.staticRoomKey) {
      return this.staticRoomKey;
    }
    if (this.roomKeyProvider) {
      const provided = this.roomKeyProvider();
      if (provided) {
        return parseRoomKey(provided);
      }
    }
    if (
      typeof globalThis !== 'undefined' &&
      typeof globalThis.location !== 'undefined' &&
      globalThis.location.hash
    ) {
      try {
        return parseRoomKey(globalThis.location.hash);
      } catch {
        // Not a valid 32-byte key in hash
        return null;
      }
    }
    return null;
  }

  /**
   * Gets a Tin by its ID.
   *
   * Flow:
   * 1. Checks pantry cache. If present, returns immediately.
   * 2. If a request for this Tin ID is already in flight, reuses the existing Promise
   *    (deduplicating concurrent cellar requests).
   * 3. Downloads from `alhalte-cellar`.
   * 4. Decrypts and decompresses using the room key (Plaintext / RoomKey / Convergent).
   * 5. Saves the decrypted Tin in the cache and returns it.
   *
   * @param tinId 64-character hex Tin ID or 32-byte Uint8Array.
   * @param options Request-specific overrides (room key, forced refetch, etc.).
   */
  async getTin(tinId: string | Uint8Array, options?: GetTinOptions): Promise<DecryptedTin> {
    const { hex: hexId, bytes: idBytes } = normalizeTinId(tinId);

    // 1. Check browser / persistent cache (unless forceFetch is true)
    if (!options?.forceFetch) {
      const cached = await this.cache.get(hexId);
      if (cached) {
        return cached;
      }
    }

    // 2. In-flight request deduplication
    const inFlight = this.inFlightRequests.get(hexId);
    if (inFlight) {
      return inFlight;
    }

    // 3. Initiate download, decrypt, and cache task
    const task = (async () => {
      try {
        if (!this.cellar) {
          throw new CellarError(`Cannot fetch tin ${hexId}: no cellar client configured in Pantry`);
        }

        const cellarResponse = await this.cellar.fetchTin(hexId);
        const wrappedKey = options?.wrappedKey ?? cellarResponse.wrappedKey;

        // Resolve room key
        let roomKey: Uint8Array | null = null;
        if (options?.roomKey) {
          roomKey = parseRoomKey(options.roomKey);
        } else {
          roomKey = this.getRoomKey();
        }

        // Decrypt and decompress
        const decrypted = decodeTin(cellarResponse.tinBytes, {
          roomKey,
          wrappedKey,
          expectedTinId: idBytes,
          stripPadme: options?.stripPadme ?? this.stripPadme,
        });

        // Store into pantry cache
        await this.cache.set(hexId, decrypted);

        return decrypted;
      } finally {
        this.inFlightRequests.delete(hexId);
      }
    })();

    this.inFlightRequests.set(hexId, task);
    return task;
  }

  /**
   * Manually stores a decrypted Tin in the cache.
   */
  async putTin(tin: DecryptedTin): Promise<void> {
    const { hex: hexId } = normalizeTinId(tin.tinId);
    await this.cache.set(hexId, tin);
  }

  /**
   * Checks whether the Tin exists in the cache without downloading from cellar.
   */
  async hasTin(tinId: string | Uint8Array): Promise<boolean> {
    const { hex: hexId } = normalizeTinId(tinId);
    return this.cache.has(hexId);
  }

  /**
   * Removes a Tin from the cache.
   */
  async deleteTin(tinId: string | Uint8Array): Promise<void> {
    const { hex: hexId } = normalizeTinId(tinId);
    await this.cache.delete(hexId);
  }

  /**
   * Clears the entire pantry cache.
   */
  async clearCache(): Promise<void> {
    await this.cache.clear();
  }

  /**
   * Number of currently pending in-flight requests.
   */
  get inFlightCount(): number {
    return this.inFlightRequests.size;
  }
}
