/**
 * Cache module exports and default factory.
 */

import type { PantryCacheAdapter } from '../types.js';
import { CacheStoragePantryCacheAdapter } from './cache-storage.js';
import { IndexedDbPantryCacheAdapter } from './indexed-db.js';
import { MemoryPantryCacheAdapter } from './memory.js';

export { CacheStoragePantryCacheAdapter } from './cache-storage.js';
export { IndexedDbPantryCacheAdapter } from './indexed-db.js';
export { MemoryPantryCacheAdapter } from './memory.js';

export interface DefaultCacheAdapterOptions {
  /**
   * If true and both IndexedDB and CacheStorage are supported, prefer IndexedDB.
   * Default is false (prefer CacheStorage for binary assets).
   */
  preferIndexedDb?: boolean;
  cacheName?: string;
}

/**
 * Creates the default cache adapter based on environment capabilities.
 * - If in browser with CacheStorage support -> CacheStoragePantryCacheAdapter
 * - If in browser with IndexedDB support -> IndexedDbPantryCacheAdapter
 * - Fallback (SSR, Node.js, testing) -> MemoryPantryCacheAdapter
 */
export function createDefaultCacheAdapter(
  options?: DefaultCacheAdapterOptions,
): PantryCacheAdapter {
  if (options?.preferIndexedDb && typeof indexedDB !== 'undefined') {
    return new IndexedDbPantryCacheAdapter(options.cacheName ?? 'alhalte-pantry-db');
  }

  if (typeof caches !== 'undefined') {
    return new CacheStoragePantryCacheAdapter(options?.cacheName ?? 'alhalte-pantry-v1');
  }

  if (typeof indexedDB !== 'undefined') {
    return new IndexedDbPantryCacheAdapter(options?.cacheName ?? 'alhalte-pantry-db');
  }

  return new MemoryPantryCacheAdapter();
}
