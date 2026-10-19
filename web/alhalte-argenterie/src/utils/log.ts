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
    case 'change-bgm': {
      const slot = Number(event.slot);
      return {
        type: 'change-bgm',
        slot: Number.isFinite(slot) ? slot : 1,
        ...(typeof event.bgmTinId === 'string' ? { bgmTinId: event.bgmTinId } : {}),
        ...(typeof event.volume === 'number' && Number.isFinite(event.volume)
          ? { volume: event.volume }
          : {}),
        ...(typeof event.trackIndex === 'number' && Number.isFinite(event.trackIndex)
          ? { trackIndex: event.trackIndex }
          : {}),
      };
    }
    case 'change-bg-img':
      return {
        type: 'change-bg-img',
        ...(typeof event.imageTinId === 'string' ? { imageTinId: event.imageTinId } : {}),
        ...(typeof event.width === 'number' && Number.isFinite(event.width)
          ? { width: event.width }
          : {}),
        ...(typeof event.height === 'number' && Number.isFinite(event.height)
          ? { height: event.height }
          : {}),
      };
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

  const id =
    typeof raw.id === 'string'
      ? raw.id
      : `${idPrefix}-${timestamp}-${Math.random().toString(36).slice(2, 8)}`;

  const sceneId = typeof raw.sceneId === 'string' ? raw.sceneId : '/';
  const speakerId =
    typeof raw.speakerId === 'string'
      ? raw.speakerId
      : typeof raw.speakerId === 'number'
        ? String(raw.speakerId)
        : undefined;
  const chunkId = typeof raw.chunkId === 'string' ? raw.chunkId : undefined;

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
    ...(contentTinBytes ? { contentTin: Array.from(contentTinBytes) } : {}),
    kind,
    ...(event ? { event } : {}),
  };
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

/**
 * Manages lazy loading and caching of log chunks for scenes.
 */
export class ChunkLogManager {
  private chunks: Map<string, LogChunk> = new Map();
  private sceneChunks: Map<string, string[]> = new Map(); // sceneId -> ordered list of chunkIds
  private loadedChunkIds: Set<string> = new Set();
  private chunkFetchHandler?: (chunkId: string) => Promise<Record<string, unknown>[]>;
  private listeners: Set<() => void> = new Set();

  constructor(fetchHandler?: (chunkId: string) => Promise<Record<string, unknown>[]>) {
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

  /**
   * Registers known chunk metadata for a scene.
   */
  public registerSceneChunks(sceneId: string, chunkIds: string[]): void {
    this.sceneChunks.set(sceneId, [...chunkIds]);
    for (const chunkId of chunkIds) {
      if (!this.chunks.has(chunkId)) {
        this.chunks.set(chunkId, {
          chunkId,
          sceneId,
          entries: [],
          isLoaded: false,
        });
      }
    }
    this.notifyListeners();
  }

  /**
   * Clears all log records for a scene across all chunks and notifies subscribers.
   */
  public clearSceneRecords(sceneId: string): void {
    const chunkList = this.sceneChunks.get(sceneId) || [];
    for (const chunkId of chunkList) {
      const chunk = this.chunks.get(chunkId);
      if (chunk) {
        chunk.entries = [];
      }
    }
    const liveChunkId = `chunk-${sceneId}-live`;
    const liveChunk = this.chunks.get(liveChunkId);
    if (liveChunk) {
      liveChunk.entries = [];
    }
    this.notifyListeners();
  }

  /**
   * Appends a new log record to the latest loaded chunk for its scene (creating a live chunk if needed)
   * and notifies all registered subscribers.
   */
  public appendRecord(record: LogRecord): void {
    let chunkList = this.sceneChunks.get(record.sceneId);
    if (!chunkList) {
      chunkList = [];
      this.sceneChunks.set(record.sceneId, chunkList);
    }

    let targetChunk: LogChunk | undefined;
    for (let i = chunkList.length - 1; i >= 0; i--) {
      const c = this.chunks.get(chunkList[i]);
      if (c && c.isLoaded) {
        targetChunk = c;
        break;
      }
    }

    if (!targetChunk) {
      const liveChunkId = `chunk-${record.sceneId}-live`;
      targetChunk = this.chunks.get(liveChunkId);
      if (!targetChunk) {
        targetChunk = {
          chunkId: liveChunkId,
          sceneId: record.sceneId,
          entries: [],
          isLoaded: true,
        };
        this.chunks.set(liveChunkId, targetChunk);
      } else {
        targetChunk.isLoaded = true;
      }
      if (!chunkList.includes(liveChunkId)) {
        chunkList.push(liveChunkId);
      }
      this.loadedChunkIds.add(liveChunkId);
    }

    if (targetChunk.entries.some((e) => e.id === record.id)) {
      return;
    }

    const normalizedRecord: LogRecord = {
      ...record,
      chunkId: record.chunkId ?? targetChunk.chunkId,
    };
    targetChunk.entries.push(normalizedRecord);
    this.notifyListeners();
  }

  /**
   * Returns whether a chunk is currently loaded.
   */
  public isChunkLoaded(chunkId: string): boolean {
    return this.loadedChunkIds.has(chunkId);
  }

  /**
   * Fetches and interprets a chunk on demand.
   */
  public async loadChunk(chunkId: string): Promise<LogRecord[]> {
    const chunk = this.chunks.get(chunkId);
    if (!chunk) {
      throw new Error(`Unknown chunk ID: ${chunkId}`);
    }

    if (chunk.isLoaded) {
      return chunk.entries;
    }

    let rawRecords: Record<string, unknown>[] = [];
    if (this.chunkFetchHandler) {
      rawRecords = await this.chunkFetchHandler(chunkId);
    }

    const entries = rawRecords.map((r, i) =>
      parseLogRecord({ ...r, chunkId, sceneId: chunk.sceneId }, `${chunkId}-${i}`),
    );

    chunk.entries = entries;
    chunk.isLoaded = true;
    this.loadedChunkIds.add(chunkId);
    this.notifyListeners();

    return entries;
  }

  /**
   * Triggered when scroll reaches edge ('top' for older messages, 'bottom' for newer).
   * Loads the next pending chunk if available.
   */
  public async handleScrollEdge(
    sceneId: string,
    edge: 'top' | 'bottom',
  ): Promise<LogRecord[] | null> {
    const chunkList = this.sceneChunks.get(sceneId) || [];
    if (chunkList.length === 0) return null;

    if (edge === 'top') {
      // Find oldest unloaded chunk (moving toward beginning of list)
      for (let i = chunkList.length - 1; i >= 0; i--) {
        const id = chunkList[i];
        if (!this.loadedChunkIds.has(id)) {
          return await this.loadChunk(id);
        }
      }
    } else {
      // Find newest unloaded chunk (moving toward end of list)
      for (let i = 0; i < chunkList.length; i++) {
        const id = chunkList[i];
        if (!this.loadedChunkIds.has(id)) {
          return await this.loadChunk(id);
        }
      }
    }

    return null;
  }

  /**
   * Returns all currently loaded records for a scene, sorted chronologically.
   */
  public getLoadedRecords(sceneId: string): LogRecord[] {
    const chunkList = this.sceneChunks.get(sceneId) || [];
    const records: LogRecord[] = [];

    for (const chunkId of chunkList) {
      const chunk = this.chunks.get(chunkId);
      if (chunk && chunk.isLoaded) {
        records.push(...chunk.entries);
      }
    }

    return records.sort((a, b) => a.timestamp - b.timestamp);
  }
}
