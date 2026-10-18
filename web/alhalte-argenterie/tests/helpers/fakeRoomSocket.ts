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
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`
 * - GitHub Issue #109 / sub-issue #139
 */
import type { ChunkLogManager } from '../../src/utils/log.js';
import type { RoomSocket } from '../../src/utils/roomSocket.js';
import type { LogRecord } from '../../src/types/index.js';

type FrameListener = (frame: unknown) => void;

interface AppendCall {
  sceneId: string;
  record: LogRecord;
}

export class FakeRoomSocket {
  /** Whether `App` should treat the socket as connected. */
  isConnected = true;
  /** Chunk manager handed over by `App` on mount. */
  chunkManager: ChunkLogManager | null = null;

  readonly requestAppendLogCalls: AppendCall[] = [];
  readonly appendLogCalls: AppendCall[] = [];
  readonly updateStates: Array<{ sceneId: string; key: string; value: unknown }> = [];

  private readonly listeners = new Set<FrameListener>();
  private readonly pending: Array<{
    sceneId: string;
    record: LogRecord;
    resolve: (ack: unknown) => void;
    reject: (error: unknown) => void;
  }> = [];

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
