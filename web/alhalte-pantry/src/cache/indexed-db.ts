/**
 * IndexedDB adapter for pantry.
 * Stores tins in a local browser IndexedDB object store.
 */

import type { DecryptedTin, PantryCacheAdapter } from '../types.js';

interface DbRecord {
  tinId: string;
  label: string; // u64 serialized as decimal string to preserve 64-bit precision
  payload: Uint8Array;
  opener: {
    encryption: number;
    compression: number;
  };
}

export class IndexedDbPantryCacheAdapter implements PantryCacheAdapter {
  private dbPromise: Promise<IDBDatabase> | null = null;

  constructor(
    public readonly dbName = 'alhalte-pantry-db',
    public readonly storeName = 'tins',
  ) {}

  private getDb(): Promise<IDBDatabase> {
    if (this.dbPromise) return this.dbPromise;

    if (typeof indexedDB === 'undefined') {
      return Promise.reject(
        new Error('IndexedDB API is not available in the current environment'),
      );
    }

    this.dbPromise = new Promise((resolve, reject) => {
      const request = indexedDB.open(this.dbName, 1);
      request.onupgradeneeded = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains(this.storeName)) {
          db.createObjectStore(this.storeName, { keyPath: 'tinId' });
        }
      };
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });

    return this.dbPromise;
  }

  async get(tinId: string): Promise<DecryptedTin | null> {
    const db = await this.getDb();
    const key = tinId.toLowerCase();

    return new Promise((resolve, reject) => {
      const tx = db.transaction(this.storeName, 'readonly');
      const store = tx.objectStore(this.storeName);
      const req = store.get(key);

      req.onsuccess = () => {
        const record = req.result as DbRecord | undefined;
        if (!record) {
          resolve(null);
          return;
        }
        resolve({
          tinId: record.tinId,
          label: BigInt(record.label),
          payload: new Uint8Array(record.payload),
          opener: {
            encryption: record.opener.encryption,
            compression: record.opener.compression,
          },
        });
      };
      req.onerror = () => reject(req.error);
    });
  }

  async set(tinId: string, tin: DecryptedTin): Promise<void> {
    const db = await this.getDb();
    const key = tinId.toLowerCase();

    const record: DbRecord = {
      tinId: key,
      label: tin.label.toString(),
      payload: new Uint8Array(tin.payload),
      opener: { ...tin.opener },
    };

    return new Promise((resolve, reject) => {
      const tx = db.transaction(this.storeName, 'readwrite');
      const store = tx.objectStore(this.storeName);
      const req = store.put(record);

      req.onsuccess = () => resolve();
      req.onerror = () => reject(req.error);
    });
  }

  async has(tinId: string): Promise<boolean> {
    const db = await this.getDb();
    const key = tinId.toLowerCase();

    return new Promise((resolve, reject) => {
      const tx = db.transaction(this.storeName, 'readonly');
      const store = tx.objectStore(this.storeName);
      const req = store.count(key);

      req.onsuccess = () => resolve(req.result > 0);
      req.onerror = () => reject(req.error);
    });
  }

  async delete(tinId: string): Promise<void> {
    const db = await this.getDb();
    const key = tinId.toLowerCase();

    return new Promise((resolve, reject) => {
      const tx = db.transaction(this.storeName, 'readwrite');
      const store = tx.objectStore(this.storeName);
      const req = store.delete(key);

      req.onsuccess = () => resolve();
      req.onerror = () => reject(req.error);
    });
  }

  async clear(): Promise<void> {
    const db = await this.getDb();

    return new Promise((resolve, reject) => {
      const tx = db.transaction(this.storeName, 'readwrite');
      const store = tx.objectStore(this.storeName);
      const req = store.clear();

      req.onsuccess = () => resolve();
      req.onerror = () => reject(req.error);
    });
  }
}
