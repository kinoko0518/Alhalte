/**
 * In-memory cache adapter for pantry.
 * Ideal for Node.js, SSR, and unit testing environments.
 */

import type { DecryptedTin, PantryCacheAdapter } from '../types.js';

export class MemoryPantryCacheAdapter implements PantryCacheAdapter {
  private readonly store = new Map<string, DecryptedTin>();

  async get(tinId: string): Promise<DecryptedTin | null> {
    const item = this.store.get(tinId.toLowerCase());
    if (!item) return null;
    return {
      tinId: item.tinId,
      label: item.label,
      payload: new Uint8Array(item.payload),
      opener: { ...item.opener },
    };
  }

  async set(tinId: string, tin: DecryptedTin): Promise<void> {
    this.store.set(tinId.toLowerCase(), {
      tinId: tin.tinId,
      label: tin.label,
      payload: new Uint8Array(tin.payload),
      opener: { ...tin.opener },
    });
  }

  async has(tinId: string): Promise<boolean> {
    return this.store.has(tinId.toLowerCase());
  }

  async delete(tinId: string): Promise<void> {
    this.store.delete(tinId.toLowerCase());
  }

  async clear(): Promise<void> {
    this.store.clear();
  }

  get size(): number {
    return this.store.size;
  }
}
