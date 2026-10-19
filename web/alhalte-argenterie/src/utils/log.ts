/**
 * Log processing, fallbacks, and chunk lazy-loading manager.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> ログタブ`
 * - ARCHITECTURE.md: `## alhalte-banquette -> #### log`
 */

import type {
  LogChunk,
  LogRecord,
  LogRecordKind,
  LogSystemEvent,
} from '../types/index.js';

export const LOG_FALLBACK_CONTENT = '';
export const LOG_FALLBACK_SPEAKER = 'Anonymous';
export const LOG_FALLBACK_TIMESTAMP = 0; // 1970-01-01T00:00:00Z

/**
 * Normalizes the optional `contentTin` field of a log record to bytes.
 *
 * The canonical JSON representation is a byte array. Older / external producers
 * may use a base64 string or a self-typed Tin JSON object (`{ label, payload }`);
 * both are accepted so a record is never dropped only because its encoding
 * differs. Returns `null` for missing or empty values.
 */
export function contentTinToBytes(value: unknown): Uint8Array | null {
  if (value instanceof Uint8Array) {
    return value.length > 0 ? value : null;
  }
  if (Array.isArray(value)) {
    if (value.length === 0) return null;
    const bytes = new Uint8Array(value.length);
    for (let i = 0; i < value.length; i++) {
      const byte = Number(value[i]);
      if (!Number.isFinite(byte)) return null;
      bytes[i] = byte & 0xff;
    }
    return bytes;
  }
  if (typeof value === 'string') {
    if (value.length === 0) return null;
    if (typeof atob !== 'function') return null;
    try {
      const binary = atob(value);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
      }
      return bytes.length > 0 ? bytes : null;
    } catch {
      return null;
    }
  }
  if (value && typeof value === 'object') {
    const record = value as Record<string, unknown>;
    const label = Number(record.label);
    const payload = contentTinToBytes(record.payload);
    if (!Number.isFinite(label) || !payload) return null;
    const plaintext = new Uint8Array(8 + payload.length);
    const view = new DataView(plaintext.buffer);
    view.setBigUint64(0, BigInt.asUintN(64, BigInt(label)), true);
    plaintext.set(payload, 8);
    return plaintext;
  }
  return null;
}

/** Returns the byte-array form of a record's `contentTin`, when present. */
export function logRecordContentTin(record: LogRecord): Uint8Array | null {
  return contentTinToBytes(record.contentTin);
}

/**
 * Normalizes a decoded {@link LogSystemEvent}; returns `undefined` for unknown
 * or malformed payloads so rendering can fall back to a text row.
 */
export function normalizeLogSystemEvent(value: unknown): LogSystemEvent | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const event = value as Record<string, unknown>;
  switch (event.type) {
    case 'character-came':
    case 'character-gone':
      if (typeof event.characterId !== 'string' && typeof event.characterId !== 'number') {
        return undefined;
      }
      return { type: event.type, characterId: String(event.characterId) };
    default:
      return undefined;
  }
}

/**
 * Parses a raw log entry dictionary, applying specified fallback values.
 *
 * Fallbacks:
 * - content: "" (empty string)
 * - speaker: "Anonymous"
 * - timestamp: 0 (1970-01-01 00:00:00)
 *
 * Shared schema fields (`contentTin`, `speakerId`, `kind`, `event`) are decoded
 * here so the WebSocket, cache and archive paths all agree on one shape.
 */
export function parseLogRecord(raw: Record<string, unknown>, idPrefix = 'log'): LogRecord {
  const content =
    typeof raw.content === 'string'
      ? raw.content
      : raw.content !== undefined && raw.content !== null
        ? String(raw.content)
        : LOG_FALLBACK_CONTENT;

  const speaker =
    typeof raw.speaker === 'string' && raw.speaker.trim().length > 0
      ? raw.speaker
      : LOG_FALLBACK_SPEAKER;

  let timestamp = LOG_FALLBACK_TIMESTAMP;
  if (typeof raw.timestamp === 'number' && Number.isFinite(raw.timestamp)) {
    timestamp = raw.timestamp;
  } else if (typeof raw.timestamp === 'bigint') {
    timestamp = Number(raw.timestamp);
  } else if (typeof raw.timestamp === 'string') {
    const parsed = Number(raw.timestamp);
    if (!Number.isNaN(parsed)) {
      timestamp = parsed;
    }
  }

  // Chunk-snapshot entries get a stable positional id: the same server entry
  // must keep the same id across reloads so the cache / live / snapshot paths
  // can dedupe it. The generic `log` prefix is only used for local-only records
  // and keeps the random suffix to stay unique.
  const fallbackId =
    idPrefix === 'log'
      ? `log-${timestamp}-${Math.random().toString(36).slice(2, 8)}`
      : `${idPrefix}-${timestamp}`;
  const id = typeof raw.id === 'string' && raw.id.length > 0 ? raw.id : fallbackId;

  const sceneId = typeof raw.sceneId === 'string' ? raw.sceneId : '/';
  const speakerId =
    typeof raw.speakerId === 'string'
      ? raw.speakerId
      : typeof raw.speakerId === 'number'
        ? String(raw.speakerId)
        : undefined;
  const chunkId = typeof raw.chunkId === 'string' ? raw.chunkId : undefined;
  const chunkIndex = toIndex(raw.chunkIndex);
  const position = toIndex(raw.position);

  const contentTinBytes = contentTinToBytes(raw.contentTin);
  const kind: LogRecordKind = raw.kind === 'system' ? 'system' : 'chat';
  const event = kind === 'system' ? normalizeLogSystemEvent(raw.event) : undefined;

  return {
    id,
    sceneId,
    content,
    speaker,
    speakerId,
    timestamp,
    chunkId,
    ...(chunkIndex !== null ? { chunkIndex } : {}),
    ...(position !== null ? { position } : {}),
    ...(raw.cached === true ? { cached: true } : {}),
    ...(contentTinBytes ? { contentTin: Array.from(contentTinBytes) } : {}),
    kind,
    ...(event ? { event } : {}),
  };
}

/**
 * Stable chunk id for a scene's chunk index.
 *
 * The biblio's sealed-chunk tin ids change when a chunk is edited or rewritten,
 * so they are not stable enough for cache / dedupe keys. The scene id and chunk
 * index are stable for the lifetime of the log (a rewind truncates the tail but
 * keeps the indices of the remaining chunks).
 */
export function logChunkId(sceneId: string, chunkIndex: number): string {
  return `log-chunk:${sceneId}:${chunkIndex}`;
}

/**
 * Reads the ordered chunk ids of a scene log from a `LogBiblio` payload.
 *
 * `chunks` lists the sealed chunks (`chunks[i]` is chunk index `i`) and the
 * active chunk is `head.chunkIndex` (normally `chunks.length`). The returned
 * array is indexed by chunk index, so callers can pass it to
 * {@link ChunkLogManager.registerSceneChunks}.
 */
export function readLogBiblioChunkIds(sceneId: string, biblio: unknown): string[] {
  if (!biblio || typeof biblio !== 'object') return [];
  const rawChunks = (biblio as Record<string, unknown>).chunks;
  const sealedCount = Array.isArray(rawChunks) ? rawChunks.length : 0;
  const head = readLogBiblioHead(biblio);
  const count = head ? Math.max(sealedCount, head.chunkIndex + 1) : sealedCount;
  return Array.from({ length: count }, (_, index) => logChunkId(sceneId, index));
}

/**
 * Serializes loaded records for the offline `localStorage` cache.
 *
 * System records keep their decoded `kind` / `event`, and `contentTin` stays a
 * byte array, so an offline restart renders the same rows without re-running
 * the extension pipeline. The `cached` flag is deliberately dropped: it is an
 * in-memory origin marker, not part of the persisted schema.
 */
export function serializeRoomLogs(records: LogRecord[]): string {
  return JSON.stringify(records.map((record) => normalizeLogRecord(record)));
}

/**
 * Parses the offline cache back into records, marking them as cache-origin.
 *
 * Malformed entries are skipped instead of failing the whole cache. Values are
 * run through {@link parseLogRecord} so records written by older versions
 * (missing `kind` / `event` / `contentTin`) are normalized.
 */
export function deserializeRoomLogs(raw: string | null | undefined): LogRecord[] {
  if (!raw) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const records: LogRecord[] = [];
  for (const entry of parsed) {
    if (!entry || typeof entry !== 'object') continue;
    const record = entry as Record<string, unknown>;
    if (typeof record.id !== 'string' || typeof record.sceneId !== 'string') continue;
    records.push(parseLogRecord({ ...record, cached: true }));
  }
  return records;
}

/**
 * Normalizes one record before persisting it to the offline cache.
 */
export function normalizeLogRecord(record: LogRecord): LogRecord {
  return parseLogRecord({
    ...record,
    cached: false,
  });
}

/**
 * Head of a scene log biblio: the newest appended entry.
 */
export interface LogBiblioHead {
  /** Index of the chunk that holds the newest entry. */
  chunkIndex: number;
  /** Position of the newest entry inside that chunk. */
  position: number;
}

/**
 * Reads a non-negative integer from a JSON value (number, bigint, or numeric
 * string). Returns `null` for anything else.
 */
function toIndex(value: unknown): number | null {
  if (typeof value === 'number') {
    return Number.isInteger(value) && value >= 0 ? value : null;
  }
  if (typeof value === 'bigint') {
    return value >= 0n && value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : null;
  }
  if (typeof value === 'string' && /^\d+$/.test(value.trim())) {
    return Number(value.trim());
  }
  return null;
}

/**
 * Extracts the head `(chunk_index, position)` from a `LogBiblio` payload.
 *
 * banquette serializes `LogBiblio.head` as a Rust tuple, i.e. a two-element
 * JSON array `[chunk_index, position]`. Some payloads wrap the same pair in an
 * object (`{ chunk_index, position }` or `{ chunkIndex, position }`), so both
 * shapes are accepted. A missing or malformed head means "no entries yet" and
 * resolves to `null`; the caller treats that as an already-empty log.
 *
 * This is the read-side companion of `RoomSocket.queryLogBiblio`: the wire
 * format is `{ chunks: [32-byte tin ids], head: [chunk_index, position] }`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-banquette -> #### log`
 * - GitHub Issue #109 / sub-issue #140: `ログタブの最新チャンク自動読み込み`
 */
export function readLogBiblioHead(biblio: unknown): LogBiblioHead | null {
  if (!biblio || typeof biblio !== 'object') return null;

  const head = (biblio as Record<string, unknown>).head;
  if (head === null || head === undefined) return null;

  let rawChunkIndex: unknown;
  let rawPosition: unknown;
  if (Array.isArray(head)) {
    rawChunkIndex = head[0];
    rawPosition = head[1];
  } else if (typeof head === 'object') {
    const record = head as Record<string, unknown>;
    rawChunkIndex = record.chunk_index ?? record.chunkIndex;
    rawPosition = record.position;
  } else {
    return null;
  }

  const chunkIndex = toIndex(rawChunkIndex);
  if (chunkIndex === null) return null;
  return { chunkIndex, position: toIndex(rawPosition) ?? 0 };
}

/** Fetches one chunk's raw entries by chunk id, index and scene id. */
export type ChunkFetchHandler = (
  chunkId: string,
  chunkIndex: number,
  sceneId: string,
) => Promise<Record<string, unknown>[]>;

/** Returns a record's `(chunkIndex, position)` dedupe key, when both are known. */
function logPositionKey(record: LogRecord): string | null {
  if (typeof record.chunkIndex !== 'number' || typeof record.position !== 'number') {
    return null;
  }
  return `${record.chunkIndex}:${record.position}`;
}

/** Returns true when two records represent the same server log entry. */
function isSameLogRecord(a: LogRecord, b: LogRecord): boolean {
  if (a.id && b.id && a.id === b.id) return true;
  // Positional dedupe covers records whose ids differ (cache vs server entry
  // without an id, a re-created chunk) but that occupy the same server slot.
  const aKey = logPositionKey(a);
  const bKey = logPositionKey(b);
  return aKey !== null && bKey !== null && aKey === bKey;
}

/**
 * Orders records by their log position when both are known, otherwise by
 * timestamp. `(chunkIndex, position)` is the server's authoritative order and
 * also keeps timestamp-less system events (timestamp 0) in place. Records
 * loaded from a live append / local-only source fall back to time.
 */
export function compareLogRecords(a: LogRecord, b: LogRecord): number {
  const aIndex = a.chunkIndex;
  const bIndex = b.chunkIndex;
  if (typeof aIndex === 'number' && typeof bIndex === 'number') {
    if (aIndex !== bIndex) return aIndex - bIndex;
    const aPosition = a.position ?? 0;
    const bPosition = b.position ?? 0;
    if (aPosition !== bPosition) return aPosition - bPosition;
    return 0;
  }
  if (typeof aIndex === 'number') return -1;
  if (typeof bIndex === 'number') return 1;
  if (a.timestamp !== b.timestamp) return a.timestamp - b.timestamp;
  return 0;
}

/**
 * Manages lazy loading, ordering and the offline cache of log chunks.
 *
 * The server is authoritative: a biblio snapshot clears cache-origin records
 * for its scene, and a chunk snapshot replaces the registered chunk's entries.
 * Cache-origin records are tagged with {@link LogRecord.cached} so the log tab
 * can mark them as a read-only offline view.
 */
export class ChunkLogManager {
  private chunks: Map<string, LogChunk> = new Map();
  /** sceneId -> registered chunk ids; array index is the banquette chunk index. */
  private sceneChunkIds: Map<string, string[]> = new Map();
  /** sceneId -> live / cache chunks (not backed by a chunk index). */
  private sceneExtraChunkIds: Map<string, string[]> = new Map();
  private chunkIndices: Map<string, number> = new Map();
  /** Scenes whose server biblio has arrived at least once this session. */
  private serverAuthoritativeScenes: Set<string> = new Set();
  private loadingChunks: Map<string, Promise<LogRecord[]>> = new Map();
  private chunkFetchHandler?: ChunkFetchHandler;
  private listeners: Set<() => void> = new Set();

  constructor(fetchHandler?: ChunkFetchHandler) {
    this.chunkFetchHandler = fetchHandler;
  }

  /**
   * Subscribes a listener that is invoked whenever log chunks or records change.
   * Returns an unsubscribe callback.
   */
  public subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private notifyListeners(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }

  /** Installs (or clears) the lazy chunk fetch handler. */
  public setChunkFetchHandler(handler?: ChunkFetchHandler): void {
    this.chunkFetchHandler = handler;
  }

  /** Returns the banquette chunk index registered for a chunk id. */
  public getChunkIndex(chunkId: string): number | undefined {
    return this.chunkIndices.get(chunkId);
  }

  private liveChunkId(sceneId: string): string {
    return `chunk-${sceneId}-live`;
  }

  private cacheChunkId(sceneId: string): string {
    return `chunk-${sceneId}-cache`;
  }

  private ensureChunk(chunkId: string, sceneId: string, chunkIndex?: number): LogChunk {
    let chunk = this.chunks.get(chunkId);
    if (!chunk) {
      chunk = { chunkId, sceneId, entries: [], isLoaded: false };
      this.chunks.set(chunkId, chunk);
    }
    if (chunkIndex !== undefined) {
      chunk.chunkIndex = chunkIndex;
      this.chunkIndices.set(chunkId, chunkIndex);
    }
    return chunk;
  }

  private addExtraChunk(sceneId: string, chunkId: string): void {
    let extras = this.sceneExtraChunkIds.get(sceneId);
    if (!extras) {
      extras = [];
      this.sceneExtraChunkIds.set(sceneId, extras);
    }
    if (!extras.includes(chunkId)) extras.push(chunkId);
  }

  /** All chunk ids of a scene in display order (registered first, extras last). */
  private allChunkIds(sceneId: string): string[] {
    const registered = this.sceneChunkIds.get(sceneId) ?? [];
    const extras = this.sceneExtraChunkIds.get(sceneId) ?? [];
    return [...registered, ...extras.filter((id) => !registered.includes(id))];
  }

  private findDuplicate(sceneId: string, record: LogRecord): LogRecord | undefined {
    for (const chunkId of this.allChunkIds(sceneId)) {
      const chunk = this.chunks.get(chunkId);
      if (chunk?.entries.some((entry) => isSameLogRecord(entry, record))) {
        return chunk.entries.find((entry) => isSameLogRecord(entry, record));
      }
    }
    return undefined;
  }

  /**
   * Registers the known chunk ids of a scene. The array index is the banquette
   * chunk index, so callers pass `readLogBiblioChunkIds(sceneId, biblio)`.
   */
  public registerSceneChunks(sceneId: string, chunkIds: string[]): void {
    const registered = [...new Set(chunkIds)];
    this.sceneChunkIds.set(sceneId, registered);
    for (let index = 0; index < registered.length; index++) {
      this.ensureChunk(registered[index], sceneId, index);
    }
    // Drop index entries for chunks the server no longer lists (rewind).
    const extras = this.sceneExtraChunkIds.get(sceneId) ?? [];
    for (const chunkId of extras) {
      const chunk = this.chunks.get(chunkId);
      if (chunk) delete chunk.chunkIndex;
      this.chunkIndices.delete(chunkId);
    }
    this.notifyListeners();
  }

  /**
   * Applies a server biblio snapshot: cache-origin records for the scene are
   * discarded, and the registered chunk list follows the server. An empty
   * biblio (no head) means the server has no logs, so cache / server-loaded
   * records are cleared (a live echo of this client's own append is kept).
   */
  public applyServerBiblio(sceneId: string, biblio: unknown): void {
    this.serverAuthoritativeScenes.add(sceneId);
    const head = readLogBiblioHead(biblio);
    if (!head) {
      // No server entries: drop the cache and every chunk we loaded from the
      // server. A live echo (a record this client just appended) is kept until
      // the matching snapshot arrives, so remounting the log tab does not erase
      // a message whose echo raced an empty biblio response.
      this.clearCachedRecords(sceneId);
      this.clearRegisteredRecords(sceneId);
      return;
    }
    this.clearCachedRecords(sceneId);
    this.registerSceneChunks(sceneId, readLogBiblioChunkIds(sceneId, biblio));
  }

  /**
   * Appends a cache-origin record restored from `localStorage`.
   *
   * Cache records live outside the registered chunk list and disappear as soon
   * as a server biblio / chunk snapshot arrives.
   */
  public appendCachedRecord(record: LogRecord): void {
    // A late restore (e.g. an effect ordering race) must not bring back a cache
    // record after the server has already answered for this scene.
    if (this.serverAuthoritativeScenes.has(record.sceneId)) return;
    const chunkId = this.cacheChunkId(record.sceneId);
    const chunk = this.ensureChunk(chunkId, record.sceneId);
    chunk.isLoaded = true;
    this.addExtraChunk(record.sceneId, chunkId);

    const normalized = parseLogRecord({
      ...record,
      chunkId,
      cached: true,
    });
    if (this.findDuplicate(record.sceneId, normalized)) return;
    chunk.entries.push(normalized);
    this.notifyListeners();
  }

  /** Removes every cache-origin record of a scene. Returns whether any changed. */
  public clearCachedRecords(sceneId: string): boolean {
    let changed = false;
    for (const chunkId of this.allChunkIds(sceneId)) {
      const chunk = this.chunks.get(chunkId);
      if (!chunk) continue;
      const next = chunk.entries.filter((entry) => !entry.cached);
      if (next.length !== chunk.entries.length) {
        chunk.entries = next;
        changed = true;
      }
    }
    const cacheId = this.cacheChunkId(sceneId);
    const cacheChunk = this.chunks.get(cacheId);
    if (cacheChunk && cacheChunk.entries.length === 0) {
      const extras = this.sceneExtraChunkIds.get(sceneId) ?? [];
      this.sceneExtraChunkIds.set(
        sceneId,
        extras.filter((id) => id !== cacheId),
      );
    }
    if (changed) this.notifyListeners();
    return changed;
  }

  /**
   * Clears records loaded from the server (registered chunks).
   *
   * Live-echo chunks are retained: they are this client's own pending records,
   * not the offline cache, and an empty biblio response racing the echo must not
   * erase them.
   */
  private clearRegisteredRecords(sceneId: string): void {
    const registered = this.sceneChunkIds.get(sceneId) ?? [];
    let changed = false;
    for (const chunkId of registered) {
      const chunk = this.chunks.get(chunkId);
      if (!chunk) continue;
      if (chunk.entries.length > 0 || chunk.isLoaded) {
        chunk.entries = [];
        chunk.isLoaded = false;
        changed = true;
      }
      delete chunk.chunkIndex;
      this.chunkIndices.delete(chunkId);
    }
    // The empty biblio also means there is no chunk to scroll to.
    this.sceneChunkIds.set(sceneId, []);
    if (changed) this.notifyListeners();
  }

  /**
   * Clears all log records for a scene across all chunks and notifies subscribers.
   *
   * Registered chunks are marked unloaded so a later scroll / eager load can
   * fetch the server's current contents again.
   */
  public clearSceneRecords(sceneId: string): void {
    for (const chunkId of this.allChunkIds(sceneId)) {
      const chunk = this.chunks.get(chunkId);
      if (!chunk) continue;
      chunk.entries = [];
      if (typeof chunk.chunkIndex === 'number') chunk.isLoaded = false;
    }
    this.notifyListeners();
  }

  /**
   * Appends a new log record to the chunk it belongs to (the head chunk when
   * `chunkIndex` is known, otherwise the latest loaded chunk / a live chunk)
   * and notifies all registered subscribers.
   */
  public appendRecord(record: LogRecord): void {
    const sceneId = record.sceneId;
    const normalizedCandidate = parseLogRecord({ ...record, cached: false });
    if (this.findDuplicate(sceneId, normalizedCandidate)) return;

    let targetChunk: LogChunk | undefined;
    if (typeof record.chunkIndex === 'number') {
      const chunkId = this.sceneChunkIds.get(sceneId)?.[record.chunkIndex];
      if (chunkId) {
        targetChunk = this.ensureChunk(chunkId, sceneId, record.chunkIndex);
      }
    }

    if (!targetChunk) {
      const registered = this.sceneChunkIds.get(sceneId) ?? [];
      for (let i = registered.length - 1; i >= 0; i--) {
        const chunk = this.chunks.get(registered[i]);
        if (chunk?.isLoaded) {
          targetChunk = chunk;
          break;
        }
      }
    }

    if (!targetChunk) {
      const liveId = this.liveChunkId(sceneId);
      targetChunk = this.ensureChunk(liveId, sceneId);
      targetChunk.isLoaded = true;
      this.addExtraChunk(sceneId, liveId);
    }

    targetChunk.isLoaded = true;
    const normalizedRecord = parseLogRecord({
      ...record,
      cached: false,
      chunkId: record.chunkId ?? targetChunk.chunkId,
    });
    targetChunk.entries.push(normalizedRecord);
    this.notifyListeners();
  }

  /**
   * Ingests a `LogChunkSnapshot` as the authoritative content of one chunk.
   *
   * Any previously loaded (including cache-origin) entries in that chunk are
   * replaced, so a server-side deletion cannot be resurrected by the cache.
   */
  public ingestChunkSnapshot(
    sceneId: string,
    chunkIndex: number,
    records: LogRecord[],
  ): void {
    // The snapshot is server-authoritative even when it arrives without a
    // preceding biblio (resumed subscription / direct query).
    this.serverAuthoritativeScenes.add(sceneId);
    this.clearCachedRecords(sceneId);
    let registered = this.sceneChunkIds.get(sceneId);
    if (!registered) {
      registered = [];
      this.sceneChunkIds.set(sceneId, registered);
    }
    while (registered.length <= chunkIndex) {
      registered.push(logChunkId(sceneId, registered.length));
    }
    const chunkId = registered[chunkIndex];
    const chunk = this.ensureChunk(chunkId, sceneId, chunkIndex);
    chunk.entries = records.map((record, position) =>
      parseLogRecord({
        ...record,
        chunkId,
        chunkIndex,
        position,
        cached: false,
      }),
    );
    chunk.isLoaded = true;
    this.notifyListeners();
  }

  /**
   * Returns whether a chunk is currently loaded.
   */
  public isChunkLoaded(chunkId: string): boolean {
    return this.chunks.get(chunkId)?.isLoaded === true;
  }

  /**
   * Fetches and interprets a chunk on demand.
   *
   * When the same chunk is requested concurrently the in-flight promise is
   * shared, so a burst of scroll events produces exactly one query. A snapshot
   * that arrived while the query was in flight is authoritative and wins.
   */
  public async loadChunk(chunkId: string): Promise<LogRecord[]> {
    const chunk = this.chunks.get(chunkId);
    if (!chunk) {
      throw new Error(`Unknown chunk ID: ${chunkId}`);
    }

    if (chunk.isLoaded) {
      return chunk.entries;
    }

    const inFlight = this.loadingChunks.get(chunkId);
    if (inFlight) return inFlight;

    const chunkIndex = chunk.chunkIndex;
    const promise = (async () => {
      if (!this.chunkFetchHandler || chunkIndex === undefined) {
        // No loader / not an indexed chunk: nothing can be fetched.
        return chunk.entries;
      }
      const rawRecords = await this.chunkFetchHandler(chunkId, chunkIndex, chunk.sceneId);
      // A `LogChunkSnapshot` may have been ingested while the query was in
      // flight (RoomSocket appends the snapshot after settling the promise).
      if (chunk.isLoaded) return chunk.entries;

      chunk.entries = rawRecords.map((raw, position) =>
        parseLogRecord(
          {
            ...raw,
            sceneId: chunk.sceneId,
            chunkId,
            chunkIndex,
            position,
            cached: false,
          },
          `${chunkId}-${position}`,
        ),
      );
      chunk.isLoaded = true;
      this.notifyListeners();
      return chunk.entries;
    })();

    this.loadingChunks.set(chunkId, promise);
    try {
      return await promise;
    } finally {
      this.loadingChunks.delete(chunkId);
    }
  }

  /**
   * Triggered when scroll reaches edge ('top' for older messages, 'bottom' for newer).
   *
   * `top` loads the chunk immediately before the oldest loaded chunk, so a
   * two-chunk conversation fetches chunk 0 only after chunk 1 is present. At
   * the beginning of the log it returns `null` and never loops.
   */
  public async handleScrollEdge(
    sceneId: string,
    edge: 'top' | 'bottom',
  ): Promise<LogRecord[] | null> {
    const registered = this.sceneChunkIds.get(sceneId) ?? [];
    if (registered.length === 0) return null;

    const loadedIndices: number[] = [];
    for (let index = 0; index < registered.length; index++) {
      const chunk = this.chunks.get(registered[index]);
      if (chunk?.isLoaded) loadedIndices.push(index);
    }

    let targetIndex: number | null = null;
    if (edge === 'top') {
      if (loadedIndices.length === 0) {
        // The eager latest load has not landed yet: treat the newest chunk as
        // the current position so repeated top scrolls still walk backwards.
        targetIndex = registered.length - 1;
      } else {
        const oldest = Math.min(...loadedIndices);
        targetIndex = oldest > 0 ? oldest - 1 : null;
      }
    } else if (loadedIndices.length === 0) {
      targetIndex = 0;
    } else {
      const newest = Math.max(...loadedIndices);
      targetIndex = newest + 1 < registered.length ? newest + 1 : null;
    }

    if (targetIndex === null) return null;
    return await this.loadChunk(registered[targetIndex]);
  }

  /**
   * Returns all currently loaded records for a scene in log order.
   */
  public getLoadedRecords(sceneId: string): LogRecord[] {
    const records: LogRecord[] = [];
    const seenIds = new Set<string>();
    const seenPositions = new Set<string>();
    for (const chunkId of this.allChunkIds(sceneId)) {
      const chunk = this.chunks.get(chunkId);
      if (!chunk?.isLoaded) continue;
      for (const record of chunk.entries) {
        // A live echo / cache record can coexist with the authoritative server
        // snapshot in another chunk. Ids are the primary key; positional keys
        // cover records without an id. Only one copy is displayed.
        if (record.id) {
          if (seenIds.has(record.id)) continue;
          seenIds.add(record.id);
        }
        const positionKey = logPositionKey(record);
        if (positionKey !== null) {
          if (seenPositions.has(positionKey)) continue;
          seenPositions.add(positionKey);
        }
        records.push(record);
      }
    }
    return records.sort(compareLogRecords);
  }

  /**
   * Returns loaded records of every scene (used to rebuild the scene graph).
   */
  public getAllLoadedRecords(): LogRecord[] {
    const sceneIds = new Set<string>([
      ...this.sceneChunkIds.keys(),
      ...this.sceneExtraChunkIds.keys(),
    ]);
    const records: LogRecord[] = [];
    for (const sceneId of sceneIds) records.push(...this.getLoadedRecords(sceneId));
    return records.sort(compareLogRecords);
  }
}
