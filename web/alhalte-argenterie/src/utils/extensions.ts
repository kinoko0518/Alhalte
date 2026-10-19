/**
 * Extension room-state synchronization and wasm cellar transfer.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 拡張機能`
 * - GitHub Issue #109 / sub-issue #145
 *
 * ## Scope
 *
 * Extension **metadata** is published in the room root scene (`/`) under
 * `extension::<extensionId>` as a JSON Tin. The **wasm bytes** never go into the
 * scene state (banquette rejects values above 4 KiB): they are uploaded to
 * `alhalte-cellar`, encrypted with the 32-byte room key (the same `RoomKey`
 * encryption used for room assets), and only the 64-hex tin id is kept in the
 * metadata (`tinId`).
 *
 * ```
 * ExtensionItem (local model)               extension::<id> (JSON Tin)
 * ─────────────────────────                 ─────────────────────────
 * id, name, description, iconSvg  ────────▶ id, name, description, iconSvg
 * priority, enabled               ────────▶ priority, enabled
 * cellarTinId                     ────────▶ tinId
 * wasmBytes                       ────────▶ (never; cellar only)
 * ```
 *
 * ## Merge rule (Issue #145 / #165)
 *
 * - The server metadata is authoritative for every id it lists.
 * - After the one-time seed (marker {@link EXTENSION_SEEDED_STATE_KEY} present)
 *   an official default id that the server does not list is treated as deleted
 *   and is **not** re-added (Issue #165).
 * - Only in a legacy room without the seed marker (no GM opened the tab yet)
 *   are built-in official ids ({@link DEFAULT_OFFICIAL_EXTENSIONS}) kept when
 *   the server does not list them, so their locally loaded wasm stays usable.
 * - Custom local ids that the server does not list are removed.
 * - Local `wasmBytes` are kept when the id matches and the server entry has no
 *   new tin id (same tin id or no tin id at all). A different tin id means the
 *   bytes changed and must be downloaded again.
 *
 * ## Seeding
 *
 * A room starts without extension state. The first GM who opens the extensions
 * tab writes the missing official defaults and then the marker
 * (`extension::__seeded`) via `requestUpdateState`, so the server becomes
 * authoritative from that point on ({@link buildExtensionSeed}). The marker is
 * written **last**: a failed seed therefore retries on the next connection
 * instead of marking a half-seeded room.
 *
 * ## Cache
 *
 * Downloaded wasm bytes are cached in an IndexedDB-backed pantry adapter. When
 * IndexedDB is unavailable (jsdom, SSR, private browsing), the adapter silently
 * falls back to an in-memory map. Tests can inject their own
 * {@link PantryCacheAdapter} through {@link loadExtensionWasm}.
 */

import {
  IndexedDbPantryCacheAdapter,
  MemoryPantryCacheAdapter,
  labelHash,
  type DecryptedTin,
  type PantryCacheAdapter,
} from '@alhalte/pantry';
import type { ExtensionItem } from '../types/index.js';
import { DEFAULT_OFFICIAL_EXTENSIONS } from '../wasm/runner.js';
import {
  downloadTin,
  encodeRoomTin,
  uploadTin,
  type EncodeRoomTinOptions,
} from './cellarTransfer.js';

/** Scene holding the extension metadata (the room root scene). */
export const EXTENSION_STATE_SCENE_ID = '/';

/** Prefix of every extension metadata key (`extension::<extensionId>`). */
export const EXTENSION_STATE_KEY_PREFIX = 'extension::';

/**
 * Scene state key of the one-time seed marker.
 *
 * Written after the initial official-default seed; while it is absent the room
 * is treated as a legacy / fresh room and official defaults may still be seeded
 * (Issue #165).
 */
export const EXTENSION_SEEDED_STATE_KEY = 'extension::__seeded';

/** Value written to {@link EXTENSION_SEEDED_STATE_KEY} after a successful seed. */
export const EXTENSION_SEEDED_MARKER_VALUE = true;

/** Label of an extension wasm body tin (`alhalte/argenterie/extension-wasm`). */
export const EXTENSION_WASM_LABEL: bigint = labelHash('alhalte/argenterie/extension-wasm');

/** 64 hex characters: a 256-bit tin id. */
const TIN_ID_PATTERN = /^[0-9a-f]{64}$/i;

/** IndexedDB database name used by the shared extension wasm cache. */
const EXTENSION_WASM_CACHE_NAME = 'alhalte-extension-wasm';

/**
 * Metadata published in the `/` scene state under `extension::<id>`.
 *
 * The value is a plain JSON object (encoded to a Tin by
 * `RoomSocket.requestUpdateState`). It is intentionally small so it always
 * fits the 4 KiB state limit: the wasm body is referenced by {@link tinId}.
 */
export interface ExtensionMetadata {
  /** Extension id (also the state key suffix). */
  id: string;
  /** Display name (from `self_introduction()` when available). */
  name: string;
  /** Human readable description (from `self_introduction()`). */
  description?: string;
  /** Inline SVG icon (from `self_introduction()`). */
  iconSvg?: string;
  /** Execution order; lower runs first. */
  priority: number;
  /** Whether the extension participates in the pipeline. */
  enabled: boolean;
  /** 64-hex cellar tin id holding the encrypted wasm, when uploaded. */
  tinId?: string;
}

/** Error raised when an extension state value / wasm reference is invalid. */
export class ExtensionStateError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ExtensionStateError';
  }
}

/** True when `key` is an extension metadata state key (`extension::<id>`). */
export function isExtensionStateKey(key: string): boolean {
  return typeof key === 'string' && key.startsWith(EXTENSION_STATE_KEY_PREFIX);
}

/**
 * Returns the scene state key of one extension (`extension::<extensionId>`).
 */
export function extensionStateKey(extensionId: string): string {
  return `${EXTENSION_STATE_KEY_PREFIX}${extensionId}`;
}

/** Extracts the extension id from an `extension::<id>` state key. */
export function extensionIdFromStateKey(key: string): string {
  return isExtensionStateKey(key) ? key.slice(EXTENSION_STATE_KEY_PREFIX.length) : '';
}

/** True when `key` is the one-time seed marker (`extension::__seeded`). */
export function isExtensionSeedMarkerKey(key: string): boolean {
  return key === EXTENSION_SEEDED_STATE_KEY;
}

/** Reads the seed marker value: `true`, or a non-empty / non-`false` string. */
function isSeedMarkerValueSet(value: unknown): boolean {
  if (value === true) return true;
  if (typeof value === 'string') {
    const normalized = value.trim().toLowerCase();
    return normalized.length > 0 && normalized !== 'false' && normalized !== '0';
  }
  return false;
}

/**
 * True when the room root state carries a positive seed marker.
 *
 * A missing / `false` / empty marker means the room still has to be seeded;
 * after seeding the server state is authoritative (Issue #165).
 *
 * @param remoteEntries Decoded `/` scene state entries.
 */
export function isExtensionStateSeeded(
  remoteEntries:
    | Map<string, unknown>
    | Record<string, unknown>
    | Iterable<readonly [string, unknown]>,
): boolean {
  for (const [key, value] of remoteEntriesToPairs(remoteEntries)) {
    if (isExtensionSeedMarkerKey(key)) return isSeedMarkerValueSet(value);
  }
  return false;
}

/** True when `id` is one of the built-in official extension ids. */
function isOfficialExtensionId(id: string): boolean {
  return DEFAULT_OFFICIAL_EXTENSIONS.some((official) => official.id === id);
}

/** One `requestUpdateState` write produced by {@link buildExtensionSeed}. */
export interface ExtensionSeedEntry {
  /** `extension::<id>` state key. */
  key: string;
  /** Metadata published in the room root scene. */
  value: ExtensionMetadata;
}

/**
 * Computes the one-time seed writes for a room without the seed marker
 * (Issue #165).
 *
 * Only **official default** metadata that the server does not already list is
 * returned, in the local list order. Existing server metadata (e.g. a legacy
 * room whose priority / enabled flags were already edited) is never
 * overwritten; the seed only fills in the missing official ids. The caller
 * writes {@link EXTENSION_SEEDED_STATE_KEY} after these entries.
 *
 * @param local Local extension list (official metadata source).
 * @param remoteEntries Decoded `/` scene state entries (marker excluded).
 * @returns One metadata write per missing official extension.
 */
export function buildExtensionSeed(
  local: ExtensionItem[],
  remoteEntries:
    | Map<string, unknown>
    | Record<string, unknown>
    | Iterable<readonly [string, unknown]>,
): ExtensionSeedEntry[] {
  const present = new Set<string>();
  for (const [key, value] of remoteEntriesToPairs(remoteEntries)) {
    if (!isExtensionStateKey(key) || isExtensionSeedMarkerKey(key)) continue;
    const keyId = extensionIdFromStateKey(key);
    if (keyId) present.add(keyId);
    if (isPlainRecord(value) && typeof value.id === 'string' && value.id.length > 0) {
      present.add(value.id);
    }
  }

  const seed: ExtensionSeedEntry[] = [];
  for (const item of local) {
    if (!isOfficialExtensionId(item.id) || present.has(item.id)) continue;
    present.add(item.id);
    seed.push({ key: extensionStateKey(item.id), value: extensionToMetadata(item) });
  }
  return seed;
}

/** True for a plain record (not an array / null). */
function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** True for a 64-hex tin id. */
function isTinId(value: unknown): value is string {
  return typeof value === 'string' && TIN_ID_PATTERN.test(value.trim());
}

/**
 * Projects the local {@link ExtensionItem} onto the JSON metadata stored in the
 * scene state.
 *
 * `wasmBytes` are deliberately dropped: only the cellar `tinId` is published.
 */
export function extensionToMetadata(ext: ExtensionItem): ExtensionMetadata {
  const metadata: ExtensionMetadata = {
    id: ext.id,
    name: ext.name,
    priority: Number.isFinite(ext.priority) ? Math.trunc(ext.priority) : 0,
    enabled: ext.enabled === true,
  };
  if (typeof ext.description === 'string' && ext.description.length > 0) {
    metadata.description = ext.description;
  }
  if (typeof ext.iconSvg === 'string' && ext.iconSvg.length > 0) {
    metadata.iconSvg = ext.iconSvg;
  }
  if (isTinId(ext.cellarTinId)) {
    metadata.tinId = ext.cellarTinId.trim().toLowerCase();
  }
  return metadata;
}

/**
 * Validates one decoded `extension::<id>` state value into an
 * {@link ExtensionItem}.
 *
 * The server value is untrusted: only known fields are copied, numbers are
 * truncated, and an invalid tin id is ignored rather than failing the whole
 * sync.
 *
 * @param id Fallback id taken from the state key.
 * @param value Decoded state value (`queryState` already ran `tinToJson`).
 * @param wasmBytes Locally cached/downloaded wasm to attach, when available.
 * @throws {ExtensionStateError} when the value is not a JSON object.
 */
export function extensionFromMetadata(
  id: string,
  value: unknown,
  wasmBytes?: Uint8Array,
): ExtensionItem {
  if (!isPlainRecord(value)) {
    throw new ExtensionStateError(`extension state for "${id}" is not a JSON object`);
  }

  const resolvedId = typeof value.id === 'string' && value.id.length > 0 ? value.id : id;
  if (!resolvedId) {
    throw new ExtensionStateError('extension state has no id');
  }

  const item: ExtensionItem = {
    id: resolvedId,
    name:
      typeof value.name === 'string' && value.name.length > 0 ? value.name : resolvedId,
    priority:
      typeof value.priority === 'number' && Number.isFinite(value.priority)
        ? Math.trunc(value.priority)
        : 0,
    // A missing flag means "enabled"; everything else must be exactly `true`.
    enabled: value.enabled === undefined ? true : value.enabled === true,
  };

  if (typeof value.description === 'string' && value.description.length > 0) {
    item.description = value.description;
  }
  if (typeof value.iconSvg === 'string' && value.iconSvg.length > 0) {
    item.iconSvg = value.iconSvg;
  }
  if (isTinId(value.tinId)) {
    item.cellarTinId = value.tinId.trim().toLowerCase();
  }
  if (wasmBytes instanceof Uint8Array && wasmBytes.length > 0) {
    item.wasmBytes = wasmBytes;
  }

  return item;
}

/** Normalizes the accepted remote entry containers into `[key, value]` pairs. */
function remoteEntriesToPairs(
  remoteEntries: Map<string, unknown> | Record<string, unknown> | Iterable<readonly [string, unknown]>,
): Array<readonly [string, unknown]> {
  if (remoteEntries instanceof Map) return [...remoteEntries.entries()];
  if (typeof (remoteEntries as Iterable<readonly [string, unknown]>)[Symbol.iterator] === 'function') {
    return [...(remoteEntries as Iterable<readonly [string, unknown]>)];
  }
  return Object.entries(remoteEntries as Record<string, unknown>);
}

/** Options for {@link mergeExtensionState}. */
export interface ExtensionMergeOptions {
  /**
   * Keep official defaults that the server does not list.
   *
   * Only legacy rooms without the seed marker may set this: once
   * {@link EXTENSION_SEEDED_STATE_KEY} exists an absent official id means the
   * extension was deleted and must stay deleted (Issue #165). Defaults to
   * `false`.
   */
  keepMissingOfficialDefaults?: boolean;
}

/**
 * Merges server extension metadata into the local extension list.
 *
 * Pure and synchronous: the wasm download is the caller's job (see
 * {@link applyRemoteExtensions}). See the module documentation for the exact
 * merge rule. The seed marker (`extension::__seeded`) is never exposed as an
 * extension row.
 *
 * @param local Local list (`App.extensions`, seeded by `loadDefaultExtensions`).
 * @param remoteEntries Decoded `extension::<id>` state entries.
 * @param options Merge options (`keepMissingOfficialDefaults` for unseeded rooms).
 * @returns The merged list sorted by priority (ties keep server order).
 */
export function mergeExtensionState(
  local: ExtensionItem[],
  remoteEntries:
    | Map<string, unknown>
    | Record<string, unknown>
    | Iterable<readonly [string, unknown]>,
  options: ExtensionMergeOptions = {},
): ExtensionItem[] {
  const localById = new Map<string, ExtensionItem>();
  for (const item of local) localById.set(item.id, item);

  const merged: ExtensionItem[] = [];
  const seen = new Set<string>();

  for (const [key, value] of remoteEntriesToPairs(remoteEntries)) {
    // The seed marker is room bookkeeping, not an extension (Issue #165).
    if (!isExtensionStateKey(key) || isExtensionSeedMarkerKey(key)) continue;
    const fallbackId = extensionIdFromStateKey(key);

    let remoteItem: ExtensionItem;
    try {
      remoteItem = extensionFromMetadata(fallbackId, value);
    } catch {
      // A malformed entry must not hide the rest of the list.
      continue;
    }

    const localItem = localById.get(remoteItem.id);
    const keepLocalWasm =
      localItem?.wasmBytes &&
      (!remoteItem.cellarTinId || localItem.cellarTinId === remoteItem.cellarTinId)
        ? localItem.wasmBytes
        : undefined;

    if (keepLocalWasm) {
      remoteItem.wasmBytes = keepLocalWasm;
      if (!remoteItem.cellarTinId && localItem?.cellarTinId) {
        // The server predates the tin id (or a local upload): keep the local
        // reference so reordering does not drop it from the room state.
        remoteItem.cellarTinId = localItem.cellarTinId;
      }
    }

    // A manifest / earlier download failure for the same id stays visible until
    // the metadata points at a new wasm source (Issue #165).
    if (!remoteItem.wasmBytes && !remoteItem.cellarTinId && localItem?.wasmMissing) {
      remoteItem.wasmMissing = true;
    }

    merged.push(remoteItem);
    seen.add(remoteItem.id);
  }

  // Legacy rooms (no seed marker, so no GM seeded yet) keep the official set
  // even before it is published. Once the marker exists the server is
  // authoritative: an absent official id is a deletion and must not reappear
  // (Issue #165).
  if (options.keepMissingOfficialDefaults === true) {
    for (const official of DEFAULT_OFFICIAL_EXTENSIONS) {
      if (seen.has(official.id)) continue;
      const localItem = localById.get(official.id);
      if (!localItem) continue;
      merged.push({ ...localItem });
      seen.add(official.id);
    }
  }

  return merged.sort((a, b) => a.priority - b.priority);
}

/** Loads the wasm bytes referenced by one `tinId` (see {@link applyRemoteExtensions}). */
export type ExtensionWasmLoader = (
  tinId: string,
  extensionId: string,
) => Promise<Uint8Array | undefined>;

/**
 * Merges server metadata into the local list and downloads missing wasm bytes.
 *
 * The metadata merge is {@link mergeExtensionState}; every merged item that
 * references a `tinId` without local bytes is passed to `wasmLoader`. A failed
 * download keeps the metadata (the row is still listed) but marks it
 * `wasmMissing: true` so the tab renders `wasm がありません` instead of acting as
 * if the extension were runnable (Issue #165); one broken tin cannot hide the
 * whole tab.
 *
 * @param local Local extension list.
 * @param remoteEntries Decoded `extension::<id>` state entries.
 * @param wasmLoader Download hook (usually `loadExtensionWasm` bound to the room key).
 * @param options Merge options (`keepMissingOfficialDefaults` for unseeded rooms).
 */
export async function applyRemoteExtensions(
  local: ExtensionItem[],
  remoteEntries:
    | Map<string, unknown>
    | Record<string, unknown>
    | Iterable<readonly [string, unknown]>,
  wasmLoader: ExtensionWasmLoader,
  options: ExtensionMergeOptions = {},
): Promise<ExtensionItem[]> {
  const merged = mergeExtensionState(local, remoteEntries, options);
  const result: ExtensionItem[] = [];

  for (const item of merged) {
    if (item.wasmBytes || !item.cellarTinId) {
      result.push(item);
      continue;
    }

    let bytes: Uint8Array | undefined;
    try {
      bytes = await wasmLoader(item.cellarTinId, item.id);
    } catch {
      bytes = undefined;
    }
    result.push(withWasmMissing(item, !bytes || bytes.length === 0, bytes));
  }

  return result;
}

/**
 * Returns a copy of `item` with the derived `wasmMissing` flag set / cleared.
 *
 * @param item Extension row.
 * @param missing Whether the wasm body is known to be unavailable.
 * @param bytes Newly loaded bytes to attach when available.
 */
function withWasmMissing(
  item: ExtensionItem,
  missing: boolean,
  bytes?: Uint8Array,
): ExtensionItem {
  const next: ExtensionItem = { ...item };
  if (bytes && bytes.length > 0) next.wasmBytes = bytes;
  if (missing && !next.wasmBytes) {
    next.wasmMissing = true;
  } else {
    delete next.wasmMissing;
  }
  return next;
}

/** Options for {@link uploadExtensionWasm}. */
export interface ExtensionWasmUploadOptions {
  /** Real upload progress (`0..1`). */
  onProgress?: (progress: number) => void;
  /** Caller cancellation, forwarded to the cellar transfer. */
  signal?: AbortSignal;
  /** Per-step timeout in milliseconds (defaults to `DEFAULT_OPERATION_TIMEOUT_MS`). */
  timeoutMs?: number;
  /** Encoder overrides (tests / compression choice). */
  encoderOptions?: EncodeRoomTinOptions;
}

/**
 * Encrypts a wasm body with the room key and uploads it to the cellar.
 *
 * The caller only receives the tin id: the metadata published in the scene
 * state stores that id instead of the bytes.
 *
 * @param bytes Raw `.wasm` bytes (already validated by the caller).
 * @param roomKey 32-byte room key.
 * @returns The 64-hex tin id the bytes were stored under.
 * @throws {ExtensionStateError} for invalid arguments.
 * @throws {CellarTransferError} when encoding / uploading fails.
 */
export async function uploadExtensionWasm(
  bytes: Uint8Array,
  roomKey: Uint8Array,
  options?: ExtensionWasmUploadOptions,
): Promise<{ tinId: string }> {
  if (!(bytes instanceof Uint8Array) || bytes.length === 0) {
    throw new ExtensionStateError('extension wasm bytes must be a non-empty Uint8Array');
  }
  if (!(roomKey instanceof Uint8Array) || roomKey.length !== 32) {
    throw new ExtensionStateError('room key must be 32 bytes (256 bits)');
  }

  const encoded = await encodeRoomTin(
    bytes,
    EXTENSION_WASM_LABEL,
    roomKey,
    options?.encoderOptions,
  );
  const uploaded = await uploadTin(encoded.wireBytes, {
    tinId: encoded.tinIdHex,
    ...(options?.onProgress ? { onProgress: options.onProgress } : {}),
    ...(options?.signal ? { signal: options.signal } : {}),
    ...(options?.timeoutMs !== undefined ? { timeoutMs: options.timeoutMs } : {}),
  });

  return { tinId: uploaded.tinId.toLowerCase() };
}

/**
 * Pantry cache that prefers IndexedDB but never fails the caller.
 *
 * IndexedDB is unavailable in jsdom / SSR and can be disabled in private
 * browsing; every failure switches reads and writes to the in-memory map. A
 * successful `set` always stores a memory copy first so the fallback stays warm.
 */
class ResilientExtensionWasmCache implements PantryCacheAdapter {
  private readonly memory = new MemoryPantryCacheAdapter();
  private primary: PantryCacheAdapter | null;
  private primaryDisabled = false;

  constructor(primary: PantryCacheAdapter | null) {
    this.primary = primary;
  }

  public async get(tinId: string): Promise<DecryptedTin | null> {
    if (this.primary && !this.primaryDisabled) {
      try {
        return await this.primary.get(tinId);
      } catch {
        this.primaryDisabled = true;
      }
    }
    return this.memory.get(tinId);
  }

  public async set(tinId: string, tin: DecryptedTin): Promise<void> {
    await this.memory.set(tinId, tin);
    if (this.primary && !this.primaryDisabled) {
      try {
        await this.primary.set(tinId, tin);
      } catch {
        this.primaryDisabled = true;
      }
    }
  }

  public async has(tinId: string): Promise<boolean> {
    if (this.primary && !this.primaryDisabled) {
      try {
        return await this.primary.has(tinId);
      } catch {
        this.primaryDisabled = true;
      }
    }
    return this.memory.has(tinId);
  }

  public async delete(tinId: string): Promise<void> {
    await this.memory.delete(tinId);
    if (this.primary && !this.primaryDisabled) {
      try {
        await this.primary.delete(tinId);
      } catch {
        this.primaryDisabled = true;
      }
    }
  }

  public async clear(): Promise<void> {
    await this.memory.clear();
    if (this.primary && !this.primaryDisabled) {
      try {
        await this.primary.clear();
      } catch {
        this.primaryDisabled = true;
      }
    }
  }
}

let sharedExtensionWasmCache: PantryCacheAdapter | null = null;

/**
 * Returns the shared extension wasm cache.
 *
 * Backed by `IndexedDbPantryCacheAdapter` when the environment exposes
 * IndexedDB, with a transparent in-memory fallback.
 */
export function getExtensionWasmCache(): PantryCacheAdapter {
  if (sharedExtensionWasmCache) return sharedExtensionWasmCache;
  const primary =
    typeof indexedDB === 'undefined'
      ? null
      : new IndexedDbPantryCacheAdapter(EXTENSION_WASM_CACHE_NAME);
  sharedExtensionWasmCache = new ResilientExtensionWasmCache(primary);
  return sharedExtensionWasmCache;
}

/**
 * Creates an independent extension wasm cache (tests / embedding).
 *
 * Wraps an injected primary adapter with the same in-memory fallback used by
 * {@link getExtensionWasmCache}.
 */
export function createExtensionWasmCache(primary?: PantryCacheAdapter): PantryCacheAdapter {
  return new ResilientExtensionWasmCache(primary ?? null);
}

/** Options for {@link loadExtensionWasm}. */
export interface ExtensionWasmLoadOptions {
  /** Timeout in milliseconds (defaults to `DEFAULT_OPERATION_TIMEOUT_MS`). */
  timeoutMs?: number;
  /** Caller cancellation, forwarded to the cellar transfer. */
  signal?: AbortSignal;
}

/**
 * Restores one extension wasm body from the cellar (or the local cache).
 *
 * The cache is consulted first; on a miss the tin is downloaded with
 * `downloadTin(tinId, roomKey)` and stored back into the cache. A tin carrying
 * a different label is rejected instead of being handed to the pipeline.
 *
 * @param tinId 64-hex cellar tin id.
 * @param roomKey 32-byte room key used to decrypt the tin.
 * @param cache Cache adapter; defaults to {@link getExtensionWasmCache}.
 * @returns The raw wasm bytes.
 * @throws {ExtensionStateError} for an invalid tin id / wrong label.
 * @throws {CellarTransferError} when the download fails.
 */
export async function loadExtensionWasm(
  tinId: string,
  roomKey: Uint8Array,
  cache: PantryCacheAdapter = getExtensionWasmCache(),
  options?: ExtensionWasmLoadOptions,
): Promise<Uint8Array> {
  const normalizedTinId = typeof tinId === 'string' ? tinId.trim().toLowerCase() : '';
  if (!isTinId(normalizedTinId)) {
    throw new ExtensionStateError(`invalid extension wasm tin id: "${tinId}"`);
  }

  try {
    const cached = await cache.get(normalizedTinId);
    if (cached && cached.label === EXTENSION_WASM_LABEL) {
      return new Uint8Array(cached.payload);
    }
  } catch {
    // Treat a broken cache as a miss; the download below is authoritative.
  }

  const tin = await downloadTin(normalizedTinId, roomKey, options);
  if (tin.label !== EXTENSION_WASM_LABEL) {
    throw new ExtensionStateError(
      `tin ${normalizedTinId} is not an extension wasm body (label ${tin.label.toString()})`,
    );
  }

  try {
    await cache.set(normalizedTinId, tin);
  } catch {
    // Caching is best effort; the bytes are returned either way.
  }

  return new Uint8Array(tin.payload);
}
