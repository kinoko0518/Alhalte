/**
 * CacheStorage adapter for pantry in browser environments.
 * Stores tins as binary HTTP Responses keyed by `tinId`.
 */

import { parseOpener, serializeOpener } from '../opener.js';
import type { DecryptedTin, PantryCacheAdapter } from '../types.js';

export class CacheStoragePantryCacheAdapter implements PantryCacheAdapter {
  constructor(public readonly cacheName = 'alhalte-pantry-v1') {}

  private async getCache(): Promise<Cache> {
    if (typeof caches === 'undefined') {
      throw new Error('CacheStorage API (caches) is not available in the current environment');
    }
    return caches.open(this.cacheName);
  }

  private makeRequestUrl(tinId: string): string {
    return `https://alhalte.internal/tin/${tinId.toLowerCase()}`;
  }

  async get(tinId: string): Promise<DecryptedTin | null> {
    const cache = await this.getCache();
    const url = this.makeRequestUrl(tinId);
    const response = await cache.match(url);
    if (!response) return null;

    const buffer = await response.arrayBuffer();
    const bytes = new Uint8Array(buffer);
    if (bytes.length < 9) return null;

    const opener = parseOpener(bytes[0]);
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const label = view.getBigUint64(1, true);
    const payload = bytes.subarray(9);

    return {
      tinId: tinId.toLowerCase(),
      label,
      payload,
      opener,
    };
  }

  async set(tinId: string, tin: DecryptedTin): Promise<void> {
    const cache = await this.getCache();
    const url = this.makeRequestUrl(tinId);

    // Wire: opener(1B) || label(8B, u64 LE) || payload
    const data = new Uint8Array(1 + 8 + tin.payload.length);
    data[0] = serializeOpener(tin.opener);
    const view = new DataView(data.buffer);
    view.setBigUint64(1, tin.label, true);
    data.set(tin.payload, 9);

    const response = new Response(data, {
      headers: {
        'Content-Type': 'application/octet-stream',
        'X-Alhalte-Tin-Id': tinId.toLowerCase(),
      },
    });

    await cache.put(url, response);
  }

  async has(tinId: string): Promise<boolean> {
    const cache = await this.getCache();
    const url = this.makeRequestUrl(tinId);
    const response = await cache.match(url);
    return response !== undefined;
  }

  async delete(tinId: string): Promise<void> {
    const cache = await this.getCache();
    const url = this.makeRequestUrl(tinId);
    await cache.delete(url);
  }

  async clear(): Promise<void> {
    if (typeof caches !== 'undefined') {
      await caches.delete(this.cacheName);
    }
  }
}
