import { describe, expect, it } from 'vitest';
import {
  CacheStoragePantryCacheAdapter,
  Compression,
  Encryption,
  IndexedDbPantryCacheAdapter,
  LABEL_PRIMITIVE_STRING,
  MemoryPantryCacheAdapter,
  createDefaultCacheAdapter,
} from '../src/index.js';

describe('Cache Adapters', () => {
  const sampleTin = {
    tinId: 'abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789',
    label: LABEL_PRIMITIVE_STRING,
    payload: new TextEncoder().encode('Adapter storage test payload'),
    opener: {
      encryption: Encryption.RoomKey,
      compression: Compression.Lz4,
    },
  };

  // spec: alhalte-pantry/cache-adapter memory
  it('stores, retrieves, checks, and deletes in MemoryPantryCacheAdapter', async () => {
    const adapter = new MemoryPantryCacheAdapter();
    expect(await adapter.has(sampleTin.tinId)).toBe(false);
    expect(await adapter.get(sampleTin.tinId)).toBeNull();

    await adapter.set(sampleTin.tinId, sampleTin);
    expect(await adapter.has(sampleTin.tinId)).toBe(true);
    expect(adapter.size).toBe(1);

    const retrieved = await adapter.get(sampleTin.tinId);
    expect(retrieved).not.toBeNull();
    expect(retrieved?.tinId).toBe(sampleTin.tinId);
    expect(retrieved?.label).toBe(sampleTin.label);
    expect(retrieved?.payload).toEqual(sampleTin.payload);
    expect(retrieved?.opener).toEqual(sampleTin.opener);

    await adapter.delete(sampleTin.tinId);
    expect(await adapter.has(sampleTin.tinId)).toBe(false);
    expect(await adapter.get(sampleTin.tinId)).toBeNull();

    await adapter.set(sampleTin.tinId, sampleTin);
    await adapter.clear();
    expect(await adapter.has(sampleTin.tinId)).toBe(false);
    expect(adapter.size).toBe(0);
  });

  // spec: alhalte-pantry/cache-adapter default factory fallback
  it('creates default cache adapter falling back to Memory in Node environment', () => {
    const adapter = createDefaultCacheAdapter();
    expect(adapter).toBeInstanceOf(MemoryPantryCacheAdapter);
  });

  // spec: alhalte-pantry/cache-adapter cache-storage mock
  it('stores and retrieves binary data in CacheStorage adapter when caches API is present', async () => {
    const cacheStorageMap = new Map<string, Response>();
    const mockCache: Partial<Cache> = {
      async match(req: RequestInfo | URL) {
        const url = typeof req === 'string' ? req : req.toString();
        return cacheStorageMap.get(url);
      },
      async put(req: RequestInfo | URL, res: Response) {
        const url = typeof req === 'string' ? req : req.toString();
        cacheStorageMap.set(url, res);
      },
      async delete(req: RequestInfo | URL) {
        const url = typeof req === 'string' ? req : req.toString();
        return cacheStorageMap.delete(url);
      },
    };

    // Install mock caches onto globalThis
    const originalCaches = (globalThis as unknown as { caches?: unknown }).caches;
    (globalThis as unknown as { caches: unknown }).caches = {
      async open() {
        return mockCache as Cache;
      },
      async delete() {
        cacheStorageMap.clear();
        return true;
      },
    };

    try {
      const adapter = new CacheStoragePantryCacheAdapter('test-cache');
      expect(await adapter.has(sampleTin.tinId)).toBe(false);

      await adapter.set(sampleTin.tinId, sampleTin);
      expect(await adapter.has(sampleTin.tinId)).toBe(true);

      const retrieved = await adapter.get(sampleTin.tinId);
      expect(retrieved?.tinId).toBe(sampleTin.tinId);
      expect(retrieved?.label).toBe(sampleTin.label);
      expect(retrieved?.payload).toEqual(sampleTin.payload);
      expect(retrieved?.opener).toEqual(sampleTin.opener);

      await adapter.delete(sampleTin.tinId);
      expect(await adapter.has(sampleTin.tinId)).toBe(false);
    } finally {
      (globalThis as unknown as { caches: unknown }).caches = originalCaches;
    }
  });

  // spec: alhalte-pantry/cache-adapter indexed-db mock
  it('stores and retrieves records in IndexedDb adapter when indexedDB API is present', async () => {
    const storeMap = new Map<string, unknown>();

    const mockIDBDatabase: Partial<IDBDatabase> = {
      transaction() {
        const mockStore = {
          get(key: string) {
            const req = {
              result: storeMap.get(key),
              onsuccess: () => {},
              onerror: () => {},
            };
            queueMicrotask(() => req.onsuccess());
            return req;
          },
          put(val: { tinId: string }) {
            storeMap.set(val.tinId, val);
            const req = {
              result: val.tinId,
              onsuccess: () => {},
              onerror: () => {},
            };
            queueMicrotask(() => req.onsuccess());
            return req;
          },
          count(key: string) {
            const req = {
              result: storeMap.has(key) ? 1 : 0,
              onsuccess: () => {},
              onerror: () => {},
            };
            queueMicrotask(() => req.onsuccess());
            return req;
          },
          delete(key: string) {
            storeMap.delete(key);
            const req = {
              result: undefined,
              onsuccess: () => {},
              onerror: () => {},
            };
            queueMicrotask(() => req.onsuccess());
            return req;
          },
          clear() {
            storeMap.clear();
            const req = {
              result: undefined,
              onsuccess: () => {},
              onerror: () => {},
            };
            queueMicrotask(() => req.onsuccess());
            return req;
          },
        };
        return {
          objectStore() {
            return mockStore;
          },
        } as unknown as IDBTransaction;
      },
    };

    const originalIDB = (globalThis as unknown as { indexedDB?: unknown }).indexedDB;
    (globalThis as unknown as { indexedDB: unknown }).indexedDB = {
      open() {
        const req = {
          result: mockIDBDatabase,
          onsuccess: () => {},
          onerror: () => {},
          onupgradeneeded: () => {},
        };
        queueMicrotask(() => req.onsuccess());
        return req;
      },
    };

    try {
      const adapter = new IndexedDbPantryCacheAdapter('test-idb');
      expect(await adapter.has(sampleTin.tinId)).toBe(false);

      await adapter.set(sampleTin.tinId, sampleTin);
      expect(await adapter.has(sampleTin.tinId)).toBe(true);

      const retrieved = await adapter.get(sampleTin.tinId);
      expect(retrieved?.tinId).toBe(sampleTin.tinId);
      expect(retrieved?.label).toBe(sampleTin.label);
      expect(retrieved?.payload).toEqual(sampleTin.payload);
      expect(retrieved?.opener).toEqual(sampleTin.opener);

      await adapter.delete(sampleTin.tinId);
      expect(await adapter.has(sampleTin.tinId)).toBe(false);
    } finally {
      (globalThis as unknown as { indexedDB: unknown }).indexedDB = originalIDB;
    }
  });
});
