/**
 * Asset Store & Pantry Integration for `@alhalte/argenterie`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-pantry`, `## alhalte-argenterie -> ### 部屋内 -> シーン設定タブ / キャラクタータブ`
 * - Issue #28: アセット選択UI（自分のアセット｜部屋のアセット、画像｜音声、アップロード、NONEカード、削除）
 */

import { MemoryPantryCacheAdapter } from '@alhalte/pantry';

export type AssetScope = 'personal' | 'room';
export type AssetMediaType = 'image' | 'audio';

export interface AssetItem {
  id: string;
  name: string;
  scope: AssetScope;
  mediaType: AssetMediaType;
  dataUrl: string; // Object URL or data URL for preview/playback
  mimeType: string;
  size?: number; // File size in bytes
  createdAt: number;
}

export const ASSETS_STORAGE_KEY = 'alhalte_pantry_assets';

type AssetListener = (assets: AssetItem[]) => void;

const listeners = new Set<AssetListener>();
let memoryFallbackAssets: AssetItem[] = [];
const pantryCache = new MemoryPantryCacheAdapter();

/**
 * Returns the shared Pantry cache adapter for binary asset caching.
 */
export function getAssetPantryCache(): MemoryPantryCacheAdapter {
  return pantryCache;
}

function getStorage(customStorage?: Storage): Storage | undefined {
  if (customStorage) return customStorage;
  if (typeof localStorage !== 'undefined') return localStorage;
  return undefined;
}

function notifyListeners(assets: AssetItem[]): void {
  for (const listener of listeners) {
    listener(assets);
  }
}

/**
 * Subscribes to asset store updates.
 * Returns an unsubscribe function.
 */
export function subscribeAssets(listener: AssetListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Formats a byte size into a human-readable string (e.g. "120.5 KB", "1.2 MB", "-").
 */
export function formatFileSize(bytes?: number): string {
  if (bytes === undefined || bytes === null || Number.isNaN(bytes) || bytes < 0) {
    return '-';
  }
  if (bytes === 0) {
    return '0 B';
  }
  const k = 1024;
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  if (i <= 0) {
    return `${bytes} B`;
  }
  const size = bytes / Math.pow(k, i);
  return `${size.toFixed(1)} ${units[i] ?? 'B'}`;
}

/**
 * Loads all stored assets from localStorage (or in-memory fallback), sorted by createdAt ascending.
 */
export function loadAssets(customStorage?: Storage): AssetItem[] {
  const storage = getStorage(customStorage);
  if (!storage) {
    return [...memoryFallbackAssets];
  }

  try {
    const raw = storage.getItem(ASSETS_STORAGE_KEY);
    if (!raw) {
      return [...memoryFallbackAssets];
    }
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [...memoryFallbackAssets];
    }
    const valid = parsed.filter(
      (item): item is AssetItem =>
        item &&
        typeof item === 'object' &&
        typeof item.id === 'string' &&
        typeof item.name === 'string' &&
        (item.scope === 'personal' || item.scope === 'room') &&
        (item.mediaType === 'image' || item.mediaType === 'audio') &&
        typeof item.dataUrl === 'string' &&
        typeof item.mimeType === 'string' &&
        (item.size === undefined || typeof item.size === 'number') &&
        typeof item.createdAt === 'number',
    );
    valid.sort((a, b) => a.createdAt - b.createdAt);
    return valid;
  } catch {
    return [...memoryFallbackAssets];
  }
}

/**
 * Persists the given asset list to localStorage and notifies subscribers.
 */
export function saveAssets(assets: AssetItem[], customStorage?: Storage): void {
  const sorted = [...assets].sort((a, b) => a.createdAt - b.createdAt);
  memoryFallbackAssets = sorted;
  const storage = getStorage(customStorage);
  if (storage) {
    try {
      storage.setItem(ASSETS_STORAGE_KEY, JSON.stringify(sorted));
    } catch {
      // QuotaExceededError or private browsing: keep in memoryFallbackAssets
    }
  }
  notifyListeners(sorted);
}

/**
 * Returns assets matching the specified scope ('personal' | 'room') and mediaType ('image' | 'audio').
 */
export function getAssetsByFilter(
  scope: AssetScope,
  mediaType: AssetMediaType,
  customStorage?: Storage,
): AssetItem[] {
  return loadAssets(customStorage).filter(
    (asset) => asset.scope === scope && asset.mediaType === mediaType,
  );
}

/**
 * Returns all assets matching the specified mediaType ('image' | 'audio') across scopes.
 */
export function getAssetsByMediaType(
  mediaType: AssetMediaType,
  customStorage?: Storage,
): AssetItem[] {
  return loadAssets(customStorage).filter((asset) => asset.mediaType === mediaType);
}

/**
 * Adds a new asset item to the store and notifies subscribers.
 */
export function addAsset(
  input: Omit<AssetItem, 'id' | 'createdAt'> & { id?: string; size?: number; createdAt?: number },
  customStorage?: Storage,
): AssetItem {
  const existing = loadAssets(customStorage);
  const newItem: AssetItem = {
    id: input.id ?? `asset-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name: input.name,
    scope: input.scope,
    mediaType: input.mediaType,
    dataUrl: input.dataUrl,
    mimeType: input.mimeType || (input.mediaType === 'image' ? 'image/webp' : 'audio/webm'),
    size: input.size,
    createdAt: input.createdAt ?? Date.now(),
  };

  const filtered = existing.filter((item) => item.id !== newItem.id);
  const updated = [...filtered, newItem];
  saveAssets(updated, customStorage);
  return newItem;
}

/**
 * Removes an asset by ID from the store and notifies subscribers.
 */
export function removeAsset(id: string, customStorage?: Storage): AssetItem[] {
  const existing = loadAssets(customStorage).filter((item) => item.id !== id);
  saveAssets(existing, customStorage);
  void pantryCache.delete(id).catch(() => {});
  return existing;
}

/**
 * Clears all assets from the store and notifies subscribers.
 */
export function clearAssets(customStorage?: Storage): void {
  memoryFallbackAssets = [];
  const storage = getStorage(customStorage);
  if (storage) {
    try {
      storage.removeItem(ASSETS_STORAGE_KEY);
    } catch {
      // Ignore storage errors
    }
  }
  void pantryCache.clear().catch(() => {});
  notifyListeners([]);
}

/**
 * Reads a File object into a Data URL string (or Object URL fallback).
 */
export async function readFileAsDataUrl(file: File): Promise<string> {
  const mime = file.type || 'application/octet-stream';
  if (typeof file.arrayBuffer === 'function') {
    try {
      const buf = await file.arrayBuffer();
      const bytes = new Uint8Array(buf);
      let binary = '';
      for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]!);
      }
      const b64 = typeof btoa === 'function' ? btoa(binary) : '';
      return `data:${mime};base64,${b64}`;
    } catch {
      // Fallback to FileReader below
    }
  }

  return new Promise((resolve) => {
    if (typeof FileReader !== 'undefined') {
      try {
        const reader = new FileReader();
        reader.onload = () => {
          if (typeof reader.result === 'string') {
            resolve(reader.result);
          } else {
            resolve(`data:${mime};base64,`);
          }
        };
        reader.onerror = () => resolve(`data:${mime};base64,`);
        reader.readAsDataURL(file);
        return;
      } catch {
        // Fallback below
      }
    }

    if (typeof URL !== 'undefined' && typeof URL.createObjectURL === 'function') {
      try {
        resolve(URL.createObjectURL(file));
        return;
      } catch {
        // Fallback below
      }
    }

    resolve(`data:${mime};base64,`);
  });
}

/**
 * Infers mediaType ('image' | 'audio') from a File's MIME type or filename.
 */
export function inferMediaType(file: File, fallback: AssetMediaType = 'image'): AssetMediaType {
  if (file.type.startsWith('audio/')) return 'audio';
  if (file.type.startsWith('image/')) return 'image';
  if (/\.(mp3|ogg|opus|wav|flac|m4a|aac)$/i.test(file.name)) return 'audio';
  if (/\.(png|jpe?g|webp|gif|svg|avif|bmp)$/i.test(file.name)) return 'image';
  return fallback;
}

/**
 * Reads an uploaded File, caches it in Pantry & localStorage, and returns the created AssetItem.
 */
export async function addAssetFromFile(
  file: File,
  scope: AssetScope,
  mediaType?: AssetMediaType,
  customStorage?: Storage,
): Promise<AssetItem> {
  const dataUrl = await readFileAsDataUrl(file);
  const resolvedMediaType = mediaType ?? inferMediaType(file);
  const resolvedMime =
    file.type || (resolvedMediaType === 'audio' ? 'audio/ogg' : 'image/webp');

  const item = addAsset(
    {
      name: file.name || 'unnamed-asset',
      scope,
      mediaType: resolvedMediaType,
      dataUrl,
      mimeType: resolvedMime,
      size: typeof file.size === 'number' ? file.size : undefined,
    },
    customStorage,
  );

  // Also cache raw encoded bytes in Pantry cache adapter when possible
  try {
    const encoder = new TextEncoder();
    await (pantryCache as any).put?.(item.id, encoder.encode(dataUrl));
  } catch {
    // Non-fatal if cache adapter fails
  }

  return item;
}
