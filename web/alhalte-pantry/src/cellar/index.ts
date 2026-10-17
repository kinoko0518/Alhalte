/**
 * Cellar clients and communication interfaces.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-cellar`, `## alhalte-pantry`
 */

import { CellarError } from '../errors.js';
import type { CellarClient, CellarResponse } from '../types.js';

export type { CellarClient, CellarResponse };

export interface HttpCellarClientOptions {
  /**
   * Base URL of the `alhalte-cellar` service (e.g. `http://localhost:8081` or `/api/cellar`).
   */
  baseUrl: string;

  /**
   * Optional custom fetch implementation (useful for tests or custom proxies).
   */
  fetchFn?: typeof fetch;

  /**
   * Additional HTTP headers sent with download requests.
   */
  headers?: Record<string, string>;
}

/**
 * Standard HTTP client for downloading tins from `alhalte-cellar`.
 */
export class HttpCellarClient implements CellarClient {
  private readonly baseUrl: string;
  private readonly fetchFn: typeof fetch;
  private readonly headers: Record<string, string>;

  constructor(options: HttpCellarClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, '');
    this.fetchFn = options.fetchFn ?? globalThis.fetch;
    this.headers = options.headers ?? {};

    if (!this.fetchFn) {
      throw new CellarError('Global fetch is not available; please provide options.fetchFn');
    }
  }

  async fetchTin(tinId: string): Promise<CellarResponse> {
    const url = `${this.baseUrl}/tin/${tinId.toLowerCase()}`;

    let response: Response;
    try {
      response = await this.fetchFn(url, {
        method: 'GET',
        headers: {
          Accept: 'application/octet-stream, application/json;q=0.9',
          ...this.headers,
        },
      });
    } catch (err) {
      throw new CellarError(`Failed to fetch tin from cellar at ${url}: ${String(err)}`);
    }

    if (!response.ok) {
      throw new CellarError(
        `Cellar returned HTTP ${response.status} (${response.statusText}) for tin ${tinId}`,
        response.status,
      );
    }

    // Check for wrapped key in response headers
    let wrappedKey: Uint8Array | null = null;
    const wkHeader =
      response.headers.get('X-Alhalte-Wrapped-Key') ||
      response.headers.get('x-alhalte-wrapped-key');

    if (wkHeader) {
      wrappedKey = parseWrappedKeyHeader(wkHeader);
    }

    const contentType = response.headers.get('content-type') || '';
    if (contentType.includes('application/json')) {
      const json = await response.json();
      const tinBytes = parseBase64OrHex(json.tin || json.wireBytes);
      if (json.wrappedKey && !wrappedKey) {
        wrappedKey = parseBase64OrHex(json.wrappedKey);
      }
      return { tinBytes, wrappedKey };
    }

    const arrayBuffer = await response.arrayBuffer();
    const tinBytes = new Uint8Array(arrayBuffer);

    return { tinBytes, wrappedKey };
  }
}

/**
 * In-memory cellar mock client for unit testing and offline development.
 */
export class InMemoryCellarClient implements CellarClient {
  private readonly storage = new Map<
    string,
    { tinBytes: Uint8Array; wrappedKey?: Uint8Array | null }
  >();

  public requestCount = 0;

  /**
   * Stores a tin in the mock cellar.
   */
  store(tinId: string, tinBytes: Uint8Array, wrappedKey?: Uint8Array | null): void {
    this.storage.set(tinId.toLowerCase(), {
      tinBytes: new Uint8Array(tinBytes),
      wrappedKey: wrappedKey ? new Uint8Array(wrappedKey) : null,
    });
  }

  async fetchTin(tinId: string): Promise<CellarResponse> {
    this.requestCount++;
    const entry = this.storage.get(tinId.toLowerCase());
    if (!entry) {
      throw new CellarError(`Tin not found in cellar: ${tinId}`, 404);
    }
    return {
      tinBytes: new Uint8Array(entry.tinBytes),
      wrappedKey: entry.wrappedKey ? new Uint8Array(entry.wrappedKey) : null,
    };
  }

  has(tinId: string): boolean {
    return this.storage.has(tinId.toLowerCase());
  }

  clear(): void {
    this.storage.clear();
    this.requestCount = 0;
  }
}

function parseWrappedKeyHeader(header: string): Uint8Array {
  const trimmed = header.trim();
  // 120 hex chars = 60 bytes (12B nonce + 48B ciphertext)
  if (/^[0-9a-fA-F]+$/.test(trimmed) && trimmed.length % 2 === 0) {
    const bytes = new Uint8Array(trimmed.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = parseInt(trimmed.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  }
  // Try base64
  return parseBase64(trimmed);
}

function parseBase64OrHex(str: string): Uint8Array {
  const trimmed = str.trim();
  if (/^[0-9a-fA-F]+$/.test(trimmed) && trimmed.length % 2 === 0) {
    const bytes = new Uint8Array(trimmed.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = parseInt(trimmed.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  }
  return parseBase64(trimmed);
}

function parseBase64(str: string): Uint8Array {
  const base64 = str.replace(/-/g, '+').replace(/_/g, '/');
  const padded = base64.padEnd(Math.ceil(base64.length / 4) * 4, '=');
  const binStr = atob(padded);
  const bytes = new Uint8Array(binStr.length);
  for (let i = 0; i < binStr.length; i++) {
    bytes[i] = binStr.charCodeAt(i);
  }
  return bytes;
}
