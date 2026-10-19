/**
 * Asset Store & Pantry Integration for `@alhalte/argenterie`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-pantry`, `## alhalte-argenterie -> ### アセット`
 * - Issue #28: アセット選択UI（自分のアセット｜部屋のアセット、画像｜音声、アップロード、NONEカード、削除）
 * - GitHub Issue #109 / sub-issue #141: 部屋アセットの cellar 転送・進捗・同期
 * - GitHub Issue #138: `utils/cellarTransfer.ts`（`encodeRoomTin` / `uploadTin` / `downloadTin`）
 *
 * ## Scope (Issue #109 / #141)
 *
 * Asset bytes always live in `alhalte-cellar`, encrypted with the 32-byte room
 * key (`Encryption.RoomKey`), regardless of scope. The scope only decides where
 * the **metadata** is published:
 *
 * - `room`: metadata is written to the `/` scene state under
 *   `asset::<assetId>` as a JSON Tin (`LABEL_JSON`) so every participant can
 *   render the card and download the tin.
 * - `personal`: metadata stays in `localStorage` only; the tin id is never
 *   published. The bytes are still uploaded to the cellar encrypted with the
 *   room key, so an owner who knows the tin id (from local storage) can fetch
 *   them again.
 *
 * ## Upload state machine (per `AssetItem`)
 *
 * ```
 * idle ──uploadAssetFromFile / retryAssetUpload──▶ uploading ──success──▶ idle (tinId set)
 *                                                  │
 *                                                  └──timeout / HTTP / no key / no socket──▶ failed
 * failed ──retryAssetUpload──▶ uploading ──▶ ...
 * ```
 *
 * Transient progress lives in an in-memory overlay that `loadAssets` merges on
 * top of `localStorage`, so progress ticks never rewrite the (potentially large)
 * persisted asset list. Only terminal transitions are persisted. A persisted
 * `uploadState: 'uploading'` is stale after a reload and is normalized to
 * `idle` when read back, because an XHR cannot resume across page loads.
 */

import {
  MemoryPantryCacheAdapter,
  labelHash,
  type DecryptedTin,
} from '@alhalte/pantry';
import {
  DEFAULT_OPERATION_TIMEOUT_MS,
  downloadTin,
  encodeRoomTin,
  getRoomKeyFromLocation,
  isCellarTransferError,
  uploadTin,
} from './cellarTransfer.js';
import { getSavedAccountId } from './session.js';

export type AssetScope = 'personal' | 'room';
export type AssetMediaType = 'image' | 'audio';

/**
 * Per-asset cellar transfer state.
 *
 * Older persisted items have none of the fields below; a missing
 * `uploadState` means `idle` (backward compatible).
 */
export type AssetUploadState = 'idle' | 'uploading' | 'failed';

export interface AssetItem {
  id: string;
  name: string;
  scope: AssetScope;
  mediaType: AssetMediaType;
  dataUrl: string; // Object URL or data URL for preview/playback
  mimeType: string;
  size?: number; // File size in bytes
  createdAt: number;
  /** Cellar tin id (64 lowercase hex) once the bytes were uploaded. */
  tinId?: string;
  /** Transfer state; `undefined` is equivalent to `'idle'`. */
  uploadState?: AssetUploadState;
  /** Last reported upload progress (`0..1`); `undefined` until known. */
  uploadProgress?: number;
  /** Human readable reason the last upload/download attempt failed. */
  uploadError?: string;
}

export const ASSETS_STORAGE_KEY = 'alhalte_pantry_assets';

/** Label of an asset body tin (`alhalte/argenterie/asset`). */
export const LABEL_ASSET: bigint = labelHash('alhalte/argenterie/asset');

/** Scene state key prefix of room asset metadata (`asset::<assetId>`). */
export const ASSET_STATE_KEY_PREFIX = 'asset::';

/** Scene whose state stores room asset metadata (the room root scene). */
export const ASSET_STATE_SCENE_ID = '/';

/** 64 hex characters: a 256-bit tin id. */
const TIN_ID_PATTERN = /^[0-9a-fA-F]{64}$/;

type AssetListener = (assets: AssetItem[]) => void;

/** Transfer fields patched onto a stored asset without persisting them. */
type AssetTransferPatch = Pick<
  AssetItem,
  'uploadState' | 'uploadProgress' | 'uploadError'
>;

const listeners = new Set<AssetListener>();
let memoryFallbackAssets: AssetItem[] = [];
const pantryCache = new MemoryPantryCacheAdapter();

/**
 * In-memory overlay of live transfer state, keyed by asset id.
 *
 * Progress ticks are high frequency, so they are only announced through
 * {@link subscribeAssets}; the persisted list is written on terminal
 * transitions ({@link finalizeAssetTransfer}). {@link loadAssets} merges the
 * overlay so synchronous readers still observe the live state.
 */
const transferOverlay = new Map<string, AssetTransferPatch>();

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
 * Merges the live transfer overlay onto persisted assets.
 *
 * Entries without an overlay patch are returned unchanged, so the common
 * (non-uploading) path allocates nothing.
 */
function withTransferOverlay(items: AssetItem[]): AssetItem[] {
  if (transferOverlay.size === 0) return items;
  return items.map((item) => {
    const patch = transferOverlay.get(item.id);
    return patch ? { ...item, ...patch } : item;
  });
}

/** Generates a new locally unique asset id. */
function createAssetId(): string {
  return `asset-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
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
 * Reads the persisted asset list (without the live transfer overlay).
 *
 * A persisted `uploadState: 'uploading'` is stale after a reload, so it is
 * normalized to `idle` here (see the module docstring).
 */
function loadStoredAssets(customStorage?: Storage): AssetItem[] {
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
    const valid = parsed
      .filter(
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
          typeof item.createdAt === 'number' &&
          (item.tinId === undefined || typeof item.tinId === 'string') &&
          (item.uploadState === undefined ||
            item.uploadState === 'idle' ||
            item.uploadState === 'uploading' ||
            item.uploadState === 'failed') &&
          (item.uploadProgress === undefined || typeof item.uploadProgress === 'number') &&
          (item.uploadError === undefined || typeof item.uploadError === 'string'),
      )
      .map((item) =>
        item.uploadState === 'uploading'
          ? { ...item, uploadState: 'idle' as const, uploadProgress: undefined }
          : item,
      );
    valid.sort((a, b) => a.createdAt - b.createdAt);
    return valid;
  } catch {
    return [...memoryFallbackAssets];
  }
}

/**
 * Loads all stored assets from localStorage (or in-memory fallback), sorted by
 * createdAt ascending.
 *
 * Live upload progress from {@link uploadAssetFromFile} is merged on top of the
 * persisted list.
 */
export function loadAssets(customStorage?: Storage): AssetItem[] {
  return withTransferOverlay(loadStoredAssets(customStorage));
}

/**
 * Announces a live (non-persisted) transfer patch for one asset.
 */
function reportAssetTransfer(
  id: string,
  patch: AssetTransferPatch,
  customStorage?: Storage,
): void {
  transferOverlay.set(id, { ...(transferOverlay.get(id) ?? {}), ...patch });
  notifyListeners(withTransferOverlay(loadStoredAssets(customStorage)));
}

/**
 * Applies a terminal transfer patch, persists it, and drops the live overlay.
 *
 * @returns The updated asset, or `undefined` when the id is no longer stored.
 */
function finalizeAssetTransfer(
  id: string,
  patch: AssetTransferPatch & { tinId?: string },
  customStorage?: Storage,
): AssetItem | undefined {
  transferOverlay.delete(id);
  const next = loadStoredAssets(customStorage).map((item) =>
    item.id === id ? { ...item, ...patch } : item,
  );
  saveAssets(next, customStorage);
  return next.find((item) => item.id === id);
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
  notifyListeners(withTransferOverlay(sorted));
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
    id: input.id ?? createAssetId(),
    name: input.name,
    scope: input.scope,
    mediaType: input.mediaType,
    dataUrl: input.dataUrl,
    mimeType: input.mimeType || (input.mediaType === 'image' ? 'image/webp' : 'audio/webm'),
    size: input.size,
    createdAt: input.createdAt ?? Date.now(),
    tinId: input.tinId,
    uploadState: input.uploadState,
    uploadProgress: input.uploadProgress,
    uploadError: input.uploadError,
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
  transferOverlay.delete(id);
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
  transferOverlay.clear();
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

// =========================================================================
// Cellar transfer (Issue #109 / #141)
// =========================================================================

/**
 * Structural subset of `RoomSocket` used to publish and query room asset
 * metadata. The real `RoomSocket` satisfies it; tests pass a fake.
 */
export interface AssetMetadataSocket {
  /** Requests the state snapshot of a scene. */
  queryState(sceneId: string, timeoutMs?: number): Promise<Record<string, any>>;
  /** Writes one scene state key and resolves on the server acknowledgement. */
  requestUpdateState(
    sceneId: string,
    key: string,
    value: any,
    timeoutMs?: number,
  ): Promise<unknown>;
  /** True while the socket is usable; `undefined` is treated as connected. */
  readonly isConnected?: boolean;
}

/**
 * Metadata published in the `/` scene state under `asset::<assetId>`.
 *
 * Personal assets keep the same shape in `localStorage` only; it is never
 * written to the scene state.
 */
export interface RoomAssetMetadata {
  /** Asset id (also the state key suffix). */
  id: string;
  /** Human readable file name. */
  name: string;
  /** Media kind used by the picker. */
  mediaType: AssetMediaType;
  /** MIME type of the original file. */
  mimeType: string;
  /** Original size in bytes, when known. */
  size?: number;
  /** 64-character lowercase hex cellar tin id holding the encrypted bytes. */
  tinId: string;
  /** Creation timestamp (ms since epoch). */
  createdAt: number;
  /** Account that uploaded the asset, when signed in. */
  ownerAccountId?: number;
}

/**
 * Options shared by {@link uploadAssetFromFile} and {@link retryAssetUpload}.
 *
 * For tests, always pass an explicit 32-byte `roomKey` instead of relying on
 * `window.location.hash`.
 */
export interface AssetTransferOptions {
  /** 32-byte room key; defaults to the URL fragment (`#<64 hex>`). */
  roomKey?: Uint8Array | null;
  /** Socket used to publish room metadata (`requestUpdateState`). */
  roomSocket?: AssetMetadataSocket | null;
  /** Caller cancellation, forwarded to the cellar transfer. */
  signal?: AbortSignal;
  /** Per-transfer-step timeout in ms (defaults to 8000). */
  timeoutMs?: number;
  /** Receives real XHR upload progress (`0..1`). */
  onProgress?: (progress: number) => void;
  /** Storage override (tests); defaults to `localStorage`. */
  storage?: Storage;
  /** Account id recorded in room metadata; defaults to the saved account. */
  ownerAccountId?: number | null;
}

/**
 * Result of one upload / retry attempt.
 *
 * Transfer failures are reported through `asset.uploadState === 'failed'` and
 * `error`; the functions never throw for timeout / HTTP / missing-key failures
 * so a UI handler can render the card instead of unwinding.
 */
export interface AssetUploadResult {
  /** The stored asset after the attempt (failed attempts keep the card). */
  asset: AssetItem;
  /** Metadata published for `room` scope; `undefined` for `personal`. */
  metadata?: RoomAssetMetadata;
  /** Machine-ish failure description when `asset.uploadState === 'failed'`. */
  error?: string;
}

/**
 * Result of {@link syncRoomAssets}.
 */
export interface AssetSyncResult {
  /** Asset list after merging the server metadata into the local store. */
  assets: AssetItem[];
  /** True when the query itself failed; the local cache is returned as is. */
  syncFailed: boolean;
  /** Failure description when `syncFailed` is true. */
  error?: string;
  /** Ids whose tin was downloaded and decoded during this sync. */
  downloadedIds: string[];
  /** Ids whose metadata exists server-side but whose bytes failed to download. */
  failedIds: string[];
  /** Number of server metadata entries merged (existing or new). */
  mergedCount: number;
}

/** True for a 32-byte room key. */
function isRoomKey(roomKey: unknown): roomKey is Uint8Array {
  return roomKey instanceof Uint8Array && roomKey.length === 32;
}

/** True for a plain record (not an array / null). */
function isPlainRecord(value: unknown): value is Record<string, any> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Formats a transfer failure for `AssetItem.uploadError`.
 *
 * `CellarTransferError` exposes a stable `code` (`timeout`, `http-error`, ...);
 * `OperationTimeoutError` from `roomSocket` exposes `code: 'timeout'` too.
 */
function describeTransferError(error: unknown): string {
  if (isCellarTransferError(error)) {
    return `${error.code}: ${error.message}`;
  }
  if (error && typeof error === 'object' && 'code' in error) {
    const code = (error as { code?: unknown }).code;
    if (typeof code === 'string') {
      const message = error instanceof Error ? error.message : String(error);
      return `${code}: ${message}`;
    }
  }
  if (error instanceof Error) return error.message;
  return String(error);
}

/** Encodes bytes as base64 (chunked to stay below the spread argument limit). */
function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }
  return typeof btoa === 'function' ? btoa(binary) : '';
}

/** Decodes base64 into bytes. */
function base64ToBytes(value: string): Uint8Array {
  const binary = atob(value.replace(/\s+/g, ''));
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/**
 * Extracts the raw bytes of a `data:<mime>;base64,<data>` URL.
 *
 * @returns The decoded bytes, or `null` for object URLs / malformed input.
 */
function decodeDataUrlBytes(dataUrl: string): Uint8Array | null {
  if (typeof dataUrl !== 'string' || !dataUrl.startsWith('data:')) return null;
  const comma = dataUrl.indexOf(',');
  if (comma < 0) return null;
  const meta = dataUrl.slice('data:'.length, comma);
  if (!meta.endsWith(';base64')) return null;
  try {
    return base64ToBytes(dataUrl.slice(comma + 1));
  } catch {
    return null;
  }
}

/** Builds a `data:` URL from decrypted tin bytes. */
function dataUrlFromTin(tin: DecryptedTin, mimeType: string): string {
  return `data:${mimeType};base64,${bytesToBase64(tin.payload)}`;
}

/**
 * Reads the raw bytes of an uploaded file.
 *
 * Uses `File.arrayBuffer()` when available and falls back to decoding the
 * preview data URL (jsdom / older browsers).
 */
async function readFileBytes(file: File, dataUrl: string): Promise<Uint8Array | null> {
  if (typeof file.arrayBuffer === 'function') {
    try {
      const buffer = await file.arrayBuffer();
      return new Uint8Array(buffer);
    } catch {
      // Fall back to the data URL below.
    }
  }
  return decodeDataUrlBytes(dataUrl);
}

/** Reads the saved account id as a number, when one is signed in. */
function savedOwnerAccountId(): number | undefined {
  const raw = getSavedAccountId();
  if (!raw) return undefined;
  const parsed = Number(raw);
  return Number.isFinite(parsed) ? parsed : undefined;
}

/**
 * Builds the room asset metadata published under `asset::<id>`.
 *
 * @returns `null` when the asset has no tin id yet.
 */
export function buildRoomAssetMetadata(
  asset: AssetItem,
  ownerAccountId?: number | null,
): RoomAssetMetadata | null {
  if (!asset.tinId || !TIN_ID_PATTERN.test(asset.tinId)) return null;
  const metadata: RoomAssetMetadata = {
    id: asset.id,
    name: asset.name,
    mediaType: asset.mediaType,
    mimeType: asset.mimeType,
    tinId: asset.tinId.toLowerCase(),
    createdAt: asset.createdAt,
  };
  if (typeof asset.size === 'number') {
    metadata.size = asset.size;
  }
  if (typeof ownerAccountId === 'number' && Number.isFinite(ownerAccountId)) {
    metadata.ownerAccountId = ownerAccountId;
  }
  return metadata;
}

/**
 * Marks an asset as failed and persists the card so the UI can offer a retry.
 */
function failAssetTransfer(
  asset: AssetItem,
  message: string,
  customStorage?: Storage,
): AssetUploadResult {
  const patch: AssetTransferPatch & { tinId?: string } = {
    uploadState: 'failed',
    uploadProgress: undefined,
    uploadError: message,
  };
  if (asset.tinId) patch.tinId = asset.tinId;
  const updated = finalizeAssetTransfer(asset.id, patch, customStorage);
  return {
    asset: updated ?? {
      ...asset,
      uploadState: 'failed',
      uploadProgress: undefined,
      uploadError: message,
    },
    error: message,
  };
}

/**
 * Encodes, uploads, and (for `room` scope) publishes one asset.
 *
 * Shared by the initial upload and the retry path so both follow the exact same
 * state machine.
 */
async function transferAssetBytes(
  asset: AssetItem,
  bytes: Uint8Array,
  options: AssetTransferOptions,
): Promise<AssetUploadResult> {
  const storage = options.storage;
  // An explicit `null` means "no key for this room"; only fall back to the
  // location fragment when the caller did not supply a key at all.
  const roomKey = options.roomKey !== undefined ? options.roomKey : getRoomKeyFromLocation();
  if (!isRoomKey(roomKey)) {
    return failAssetTransfer(
      asset,
      'room key is missing; the asset cannot be encrypted for the cellar',
      storage,
    );
  }

  const timeoutMs = options.timeoutMs ?? DEFAULT_OPERATION_TIMEOUT_MS;

  let encoded: { wireBytes: Uint8Array; tinIdHex: string };
  try {
    encoded = await encodeRoomTin(bytes, LABEL_ASSET, roomKey);
  } catch (error) {
    return failAssetTransfer(asset, describeTransferError(error), storage);
  }

  let tinId = encoded.tinIdHex;
  try {
    // Real XHR `upload.onprogress` is the only progress source (no fake ticks).
    const uploaded = await uploadTin(encoded.wireBytes, {
      tinId: encoded.tinIdHex,
      timeoutMs,
      ...(options.signal ? { signal: options.signal } : {}),
      onProgress: (progress: number) => {
        const clamped = Math.min(1, Math.max(0, progress));
        reportAssetTransfer(
          asset.id,
          { uploadState: 'uploading', uploadProgress: clamped },
          storage,
        );
        options.onProgress?.(clamped);
      },
    });
    tinId = uploaded.tinId;
  } catch (error) {
    return failAssetTransfer(asset, describeTransferError(error), storage);
  }

  const uploaded: AssetItem = { ...asset, tinId };
  let metadata: RoomAssetMetadata | undefined;
  if (asset.scope === 'room') {
    const roomMetadata = buildRoomAssetMetadata(
      uploaded,
      options.ownerAccountId ?? savedOwnerAccountId(),
    );
    if (!roomMetadata) {
      return failAssetTransfer(uploaded, 'uploaded tin id is invalid', storage);
    }
    metadata = roomMetadata;
    const socket = options.roomSocket;
    if (!socket || typeof socket.requestUpdateState !== 'function') {
      return failAssetTransfer(
        uploaded,
        'room socket is unavailable; the room asset metadata was not published',
        storage,
      );
    }
    try {
      await socket.requestUpdateState(
        ASSET_STATE_SCENE_ID,
        `${ASSET_STATE_KEY_PREFIX}${asset.id}`,
        metadata,
        timeoutMs,
      );
    } catch (error) {
      return failAssetTransfer(uploaded, describeTransferError(error), storage);
    }
  }

  const updated = finalizeAssetTransfer(
    asset.id,
    {
      tinId,
      uploadState: 'idle',
      uploadProgress: 1,
      uploadError: undefined,
    },
    storage,
  );

  return {
    asset: updated ?? { ...uploaded, uploadState: 'idle', uploadProgress: 1 },
    ...(metadata ? { metadata } : {}),
  };
}

/**
 * Uploads a picked file to the cellar and returns its (already stored) card.
 *
 * Flow (spec: Issue #109 / #141):
 * 1. Read the preview data URL.
 * 2. Create the asset immediately with `uploadState: 'uploading'` so a card
 *    appears while the upload runs.
 * 3. `encodeRoomTin(bytes, LABEL_ASSET, roomKey)` — encryption with the room key.
 * 4. `uploadTin(wireBytes, { tinId, onProgress, signal, timeoutMs })`.
 * 5. On success set `tinId` / `uploadState: 'idle'`; for `room` scope also
 *    `requestUpdateState('/', 'asset::<id>', metadata)` before the card settles.
 * 6. On failure keep the card with `uploadState: 'failed'` + `uploadError`
 *    (no throw, so other cards / the modal stay usable).
 *
 * Personal assets are uploaded with the same room-key encryption, but their
 * metadata is never published (localStorage only).
 */
export async function uploadAssetFromFile(
  file: File,
  scope: AssetScope,
  mediaType?: AssetMediaType,
  options: AssetTransferOptions = {},
): Promise<AssetUploadResult> {
  const storage = options.storage;
  const resolvedMediaType = mediaType ?? inferMediaType(file);
  const resolvedMime =
    file.type || (resolvedMediaType === 'audio' ? 'audio/ogg' : 'image/webp');
  const dataUrl = await readFileAsDataUrl(file);

  const item = addAsset(
    {
      id: createAssetId(),
      name: file.name || 'unnamed-asset',
      scope,
      mediaType: resolvedMediaType,
      dataUrl,
      mimeType: resolvedMime,
      size: typeof file.size === 'number' ? file.size : undefined,
      uploadState: 'uploading',
    },
    storage,
  );
  // Live state for synchronous readers (progress is overlay-only).
  reportAssetTransfer(
    item.id,
    { uploadState: 'uploading', uploadProgress: undefined, uploadError: undefined },
    storage,
  );

  const bytes = await readFileBytes(file, dataUrl);
  if (!bytes) {
    return failAssetTransfer(item, 'failed to read the file bytes', storage);
  }
  return transferAssetBytes(item, bytes, options);
}

/**
 * Retries a failed upload (or re-publishes room metadata) from the stored
 * preview, without creating a second card.
 *
 * The bytes are decoded from `asset.dataUrl`; assets whose preview is an object
 * URL (`blob:`) cannot be retried and must be re-picked.
 */
export async function retryAssetUpload(
  asset: AssetItem,
  options: AssetTransferOptions = {},
): Promise<AssetUploadResult> {
  const storage = options.storage;
  const bytes = decodeDataUrlBytes(asset.dataUrl);
  if (!bytes) {
    return failAssetTransfer(
      asset,
      'the local preview cannot be retried; pick the file again',
      storage,
    );
  }

  reportAssetTransfer(
    asset.id,
    { uploadState: 'uploading', uploadProgress: undefined, uploadError: undefined },
    storage,
  );
  const retrying: AssetItem = {
    ...asset,
    uploadState: 'uploading',
    uploadProgress: undefined,
    uploadError: undefined,
  };
  return transferAssetBytes(retrying, bytes, options);
}

/**
 * Validates one `asset::*` state value into {@link RoomAssetMetadata}.
 *
 * @param key Scene state key (`asset::<id>`).
 * @param value Decoded state value (the Tin was already decoded by `queryState`).
 * @returns The metadata, or `null` when the value is not a usable record.
 */
export function parseRoomAssetMetadata(
  key: string,
  value: unknown,
): RoomAssetMetadata | null {
  if (!isPlainRecord(value)) return null;
  const fallbackId = key.startsWith(ASSET_STATE_KEY_PREFIX)
    ? key.slice(ASSET_STATE_KEY_PREFIX.length)
    : '';
  const id = typeof value.id === 'string' && value.id.length > 0 ? value.id : fallbackId;
  if (!id) return null;

  const mediaType: AssetMediaType | null =
    value.mediaType === 'audio' ? 'audio' : value.mediaType === 'image' ? 'image' : null;
  if (!mediaType) return null;
  if (typeof value.tinId !== 'string' || !TIN_ID_PATTERN.test(value.tinId.trim())) {
    return null;
  }

  const metadata: RoomAssetMetadata = {
    id,
    name: typeof value.name === 'string' && value.name.length > 0 ? value.name : id,
    mediaType,
    mimeType:
      typeof value.mimeType === 'string' && value.mimeType.length > 0
        ? value.mimeType
        : mediaType === 'audio'
          ? 'audio/ogg'
          : 'image/webp',
    tinId: value.tinId.trim().toLowerCase(),
    // Deterministic fallback so validation stays side-effect free.
    createdAt: typeof value.createdAt === 'number' ? value.createdAt : 0,
  };
  if (typeof value.size === 'number') {
    metadata.size = value.size;
  }
  if (typeof value.ownerAccountId === 'number') {
    metadata.ownerAccountId = value.ownerAccountId;
  }
  return metadata;
}

/** Builds an {@link AssetItem} for a room asset downloaded during a sync. */
function assetFromRoomMetadata(
  metadata: RoomAssetMetadata,
  dataUrl: string,
  size?: number,
): AssetItem {
  const item: AssetItem = {
    id: metadata.id,
    name: metadata.name,
    scope: 'room',
    mediaType: metadata.mediaType,
    dataUrl,
    mimeType: metadata.mimeType,
    createdAt: metadata.createdAt,
    tinId: metadata.tinId,
    uploadState: 'idle',
    uploadProgress: undefined,
    uploadError: undefined,
  };
  const resolvedSize = metadata.size ?? size;
  if (typeof resolvedSize === 'number') {
    item.size = resolvedSize;
  }
  return item;
}

/**
 * Syncs room asset metadata from the `/` scene state into the local store.
 *
 * - Queries `/` state and merges every `asset::*` entry into the `room` scope.
 * - Assets that already have a local preview are updated in place (no download).
 * - Missing tins are fetched with `downloadTin(tinId, roomKey)` and converted to
 *   a `data:<mime>;base64,<...>` preview.
 * - A failed download marks only that card `uploadState: 'failed'` with a
 *   download error; the rest of the modal keeps working.
 * - A timeout / connection failure keeps the local cache untouched and reports
 *   `syncFailed: true` (the modal stays silent but the report records it).
 * - Local assets absent server-side are never deleted.
 * - No socket, or a disconnected socket, skips the sync silently.
 *
 * @param roomSocket Connected room socket (or a structurally equivalent fake).
 * @param roomKey 32-byte room key used to decrypt downloaded tins.
 * @param storage Storage override (tests); defaults to `localStorage`.
 */
export async function syncRoomAssets(
  roomSocket: AssetMetadataSocket | null | undefined,
  roomKey: Uint8Array | null | undefined,
  storage?: Storage,
): Promise<AssetSyncResult> {
  const local = loadAssets(storage);
  const idleResult = (patch: Partial<AssetSyncResult> = {}): AssetSyncResult => ({
    assets: local,
    syncFailed: false,
    downloadedIds: [],
    failedIds: [],
    mergedCount: 0,
    ...patch,
  });

  if (!roomSocket || typeof roomSocket.queryState !== 'function') {
    return idleResult();
  }
  if (roomSocket.isConnected === false) {
    return idleResult();
  }
  if (!isRoomKey(roomKey)) {
    return idleResult({
      syncFailed: true,
      error: 'room key is missing; room assets cannot be decrypted',
    });
  }

  let states: Record<string, any>;
  try {
    states = await roomSocket.queryState(ASSET_STATE_SCENE_ID);
  } catch (error) {
    return idleResult({ syncFailed: true, error: describeTransferError(error) });
  }

  const byId = new Map(local.map((asset) => [asset.id, asset]));
  const downloadedIds: string[] = [];
  const failedIds: string[] = [];
  let mergedCount = 0;

  for (const [key, value] of Object.entries(states ?? {})) {
    if (!key.startsWith(ASSET_STATE_KEY_PREFIX)) continue;
    const metadata = parseRoomAssetMetadata(key, value);
    if (!metadata) continue;
    mergedCount += 1;

    const existing = byId.get(metadata.id);
    if (existing && typeof existing.dataUrl === 'string' && existing.dataUrl.length > 0) {
      byId.set(metadata.id, {
        ...assetFromRoomMetadata(metadata, existing.dataUrl, existing.size),
        createdAt: metadata.createdAt,
      });
      continue;
    }

    try {
      const tin = await downloadTin(metadata.tinId, roomKey);
      downloadedIds.push(metadata.id);
      byId.set(
        metadata.id,
        assetFromRoomMetadata(
          metadata,
          dataUrlFromTin(tin, metadata.mimeType),
          tin.payload.byteLength,
        ),
      );
    } catch (error) {
      failedIds.push(metadata.id);
      byId.set(metadata.id, {
        ...assetFromRoomMetadata(metadata, existing?.dataUrl ?? '', existing?.size),
        uploadState: 'failed',
        uploadError: describeTransferError(error),
      });
    }
  }

  if (mergedCount === 0) {
    return idleResult();
  }

  saveAssets([...byId.values()], storage);
  return {
    assets: loadAssets(storage),
    syncFailed: false,
    downloadedIds,
    failedIds,
    mergedCount,
  };
}
