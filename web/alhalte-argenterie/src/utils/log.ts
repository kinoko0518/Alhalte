/**
 * Log processing, fallbacks, and chunk lazy-loading manager.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> ログタブ`
 * - ARCHITECTURE.md: `## alhalte-banquette -> #### log`
 */

import type { LogChunk, LogRecord } from '../types/index.js';

export const LOG_FALLBACK_CONTENT = '';
export const LOG_FALLBACK_SPEAKER = 'Anonymous';
export const LOG_FALLBACK_TIMESTAMP = 0; // 1970-01-01T00:00:00Z

/**
 * Parses a raw log entry dictionary, applying specified fallback values.
 *
 * Fallbacks:
 * - content: "" (empty string)
 * - speaker: "Anonymous"
 * - timestamp: 0 (1970-01-01 00:00:00)
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
  const speakerId = typeof raw.speakerId === 'string' ? raw.speakerId : undefined;
  const chunkId = typeof raw.chunkId === 'string' ? raw.chunkId : undefined;

  return {
    id,
    sceneId,
    content,
    speaker,
    speakerId,
    timestamp,
    chunkId,
  };
}

/**
 * Manages lazy loading and caching of log chunks for scenes.
 */
export class ChunkLogManager {
  private chunks: Map<string, LogChunk> = new Map();
  private sceneChunks: Map<string, string[]> = new Map(); // sceneId -> ordered list of chunkIds
  private loadedChunkIds: Set<string> = new Set();
  private chunkFetchHandler?: (chunkId: string) => Promise<Record<string, unknown>[]>;

  constructor(fetchHandler?: (chunkId: string) => Promise<Record<string, unknown>[]>) {
    this.chunkFetchHandler = fetchHandler;
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
