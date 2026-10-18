/**
 * Test double for `RoomSocket` used by the App-level send-path tests.
 *
 * It reproduces only the surface `App.svelte` depends on:
 *
 * - `isConnected` — toggled by the test to simulate going offline/online.
 * - `setChunkLogManager` / `subscribe` — the wiring `App` performs on mount.
 * - `requestAppendLog` — returns a Promise that stays pending until the test
 *   delivers a `LogEventAppended` echo via {@link FakeRoomSocket.deliverEcho},
 *   which appends the record to the chunk manager *before* notifying
 *   subscribers, exactly like `RoomSocket.handleServerFrame`.
 * - `appendLog` — recorded so tests can prove the offline path never uses the
 *   in-memory `messageQueue` (that would double-send on reconnect, Issue #109).
 * - `queryLogBiblio` / `queryLogChunk` — the eager latest-chunk load path
 *   (Issue #109 / sub-issue #140). By default they answer with an empty biblio
 *   so unrelated tests settle; set {@link FakeRoomSocket.autoAnswerLogQueries}
 *   to `false` to drive them manually via {@link FakeRoomSocket.resolveLogBiblio}
 *   / {@link FakeRoomSocket.resolveLogChunk}.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`
 * - GitHub Issue #109 / sub-issue #139, #140
 */
import type { ChunkLogManager } from '../../src/utils/log.js';
import type { RoomSocket } from '../../src/utils/roomSocket.js';
import type { LogRecord } from '../../src/types/index.js';

type FrameListener = (frame: unknown) => void;

interface AppendCall {
  sceneId: string;
  record: LogRecord;
}

interface BiblioCall {
  sceneId: string;
}

interface ChunkCall {
  sceneId: string;
  chunkIndex: number;
}

interface PendingLogQuery {
  kind: 'biblio' | 'chunk';
  sceneId: string;
  chunkIndex: number;
  resolve: (value: any) => void;
  reject: (error: unknown) => void;
}

export class FakeRoomSocket {
  /** Whether `App` should treat the socket as connected. */
  isConnected = true;
  /** Chunk manager handed over by `App` on mount. */
  chunkManager: ChunkLogManager | null = null;
  /**
   * When `true` (the default) log queries resolve immediately: `queryLogBiblio`
   * with an empty biblio (no head, so no chunk request) and `queryLogChunk`
   * with no entries. Tests that exercise the loading state set this to `false`
   * and answer the recorded calls themselves.
   */
  autoAnswerLogQueries = true;

  readonly requestAppendLogCalls: AppendCall[] = [];
  readonly appendLogCalls: AppendCall[] = [];
  readonly updateStates: Array<{ sceneId: string; key: string; value: unknown }> = [];
  readonly queryLogBiblioCalls: BiblioCall[] = [];
  readonly queryLogChunkCalls: ChunkCall[] = [];

  private readonly listeners = new Set<FrameListener>();
  private readonly pending: Array<{
    sceneId: string;
    record: LogRecord;
    resolve: (ack: unknown) => void;
    reject: (error: unknown) => void;
  }> = [];
  private readonly pendingLogQueries: PendingLogQuery[] = [];

  setChunkLogManager(manager?: ChunkLogManager): void {
    this.chunkManager = manager ?? null;
  }

  subscribe(listener: FrameListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  requestAppendLog(sceneId: string, record: LogRecord): Promise<unknown> {
    this.requestAppendLogCalls.push({ sceneId, record });
    return new Promise((resolve, reject) => {
      this.pending.push({ sceneId, record, resolve, reject });
    });
  }

  /** Fire-and-forget path: `App` must NOT use this while offline (Issue #109). */
  appendLog(sceneId: string, record: LogRecord): void {
    this.appendLogCalls.push({ sceneId, record });
  }

  updateState(sceneId: string, key: string, value: unknown): void {
    this.updateStates.push({ sceneId, key, value });
  }

  deleteState(): void {}

  enterScene(): void {}

  /**
   * Eager latest-chunk load: records the request and either answers with an
   * empty biblio or waits for {@link FakeRoomSocket.resolveLogBiblio}.
   */
  queryLogBiblio(sceneId: string): Promise<any> {
    this.queryLogBiblioCalls.push({ sceneId });
    if (this.autoAnswerLogQueries) {
      return Promise.resolve({ chunks: [], head: null });
    }
    return new Promise((resolve, reject) => {
      this.pendingLogQueries.push({ kind: 'biblio', sceneId, chunkIndex: -1, resolve, reject });
    });
  }

  /**
   * Eager latest-chunk load: records the request and either answers with an
   * empty chunk or waits for {@link FakeRoomSocket.resolveLogChunk}.
   */
  queryLogChunk(sceneId: string, chunkIndex: number): Promise<any[]> {
    this.queryLogChunkCalls.push({ sceneId, chunkIndex });
    if (this.autoAnswerLogQueries) {
      return Promise.resolve([]);
    }
    return new Promise((resolve, reject) => {
      this.pendingLogQueries.push({ kind: 'chunk', sceneId, chunkIndex, resolve, reject });
    });
  }

  /** Number of biblio/chunk queries still waiting for a manual answer. */
  get pendingLogQueryCount(): number {
    return this.pendingLogQueries.length;
  }

  /**
   * Resolves the oldest pending biblio query (optionally for a specific scene).
   */
  resolveLogBiblio(biblio: unknown, sceneId?: string): void {
    this.takeLogQuery('biblio', sceneId).resolve(biblio);
  }

  /**
   * Resolves the oldest pending chunk query and appends its entries to the
   * chunk manager, mirroring `RoomSocket.handleServerFrame`: the pending
   * operation settles first, then the `LogChunkSnapshot` entries are ingested
   * with positional IDs (`log-<chunkIndex>-<position>`) unless they carry one.
   */
  resolveLogChunk(entries: LogRecord[], sceneId?: string, chunkIndex?: number): void {
    const operation = this.takeLogQuery('chunk', sceneId);
    const resolvedChunkIndex = chunkIndex ?? operation.chunkIndex;

    // Same order as the real socket: dispatch the acknowledgement, then append
    // the snapshot entries (a microtask continuation sees them either way).
    operation.resolve(entries);
    entries.forEach((entry, position) => {
      this.chunkManager?.appendRecord({
        ...entry,
        id: entry.id || `log-${resolvedChunkIndex}-${position}`,
        sceneId: entry.sceneId || operation.sceneId,
        chunkId: entry.chunkId ?? `log-chunk-${resolvedChunkIndex}`,
      });
    });
  }

  /** Rejects the oldest pending log query (optionally for a specific scene). */
  rejectLogQuery(error: unknown, sceneId?: string): void {
    const operation = this.pendingLogQueries.find(
      (query) => sceneId === undefined || query.sceneId === sceneId,
    );
    if (!operation) {
      throw new Error('FakeRoomSocket: no pending log query');
    }
    this.pendingLogQueries.splice(this.pendingLogQueries.indexOf(operation), 1);
    operation.reject(error);
  }

  private takeLogQuery(kind: 'biblio' | 'chunk', sceneId?: string): PendingLogQuery {
    const operation = this.pendingLogQueries.find(
      (query) => query.kind === kind && (sceneId === undefined || query.sceneId === sceneId),
    );
    if (!operation) {
      throw new Error(`FakeRoomSocket: no pending ${kind} query`);
    }
    this.pendingLogQueries.splice(this.pendingLogQueries.indexOf(operation), 1);
    return operation;
  }

  disconnect(): void {
    this.isConnected = false;
  }

  /** Number of append operations still waiting for an echo. */
  get pendingCount(): number {
    return this.pending.length;
  }

  /** Resolves the oldest pending append by delivering its `LogEventAppended` echo. */
  deliverEcho(position = 0): void {
    const operation = this.pending.shift();
    if (!operation) throw new Error('FakeRoomSocket: no pending append operation');
    this.chunkManager?.appendRecord(operation.record);
    const frame = {
      LogEventAppended: {
        scene_id: operation.sceneId,
        chunk_index: 0,
        position,
        entry: null,
      },
    };
    for (const listener of this.listeners) listener(frame);
    operation.resolve(frame.LogEventAppended);
  }

  /** Rejects the oldest pending append, e.g. with an `OperationTimeoutError`. */
  rejectPending(error: unknown): void {
    const operation = this.pending.shift();
    if (!operation) throw new Error('FakeRoomSocket: no pending append operation');
    operation.reject(error);
  }
}

/** `App` takes a real `RoomSocket`; tests hand it this structural double. */
export function asRoomSocket(socket: FakeRoomSocket): RoomSocket {
  return socket as unknown as RoomSocket;
}
