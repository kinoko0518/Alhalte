import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  RoomSocket,
  DEFAULT_OPERATION_TIMEOUT_MS,
  OperationTimeoutError,
  ServerOperationError,
  LABEL_JSON,
  LABEL_LOG_RECORD,
  jsonToTin,
  tinToJson,
  logRecordToTin,
  tinToLogRecord,
  characterToTinMap,
  tinMapToCharacter,
  type TinJson,
  type ClientFrame,
} from '../src/utils/roomSocket.js';
import { ChunkLogManager } from '../src/utils/log.js';
import type { Character, LogRecord } from '../src/types/index.js';

/**
 * Minimal MockWebSocket mirroring the one used by `room_socket.test.ts`.
 */
class MockWebSocket {
  static instances: MockWebSocket[] = [];
  url: string;
  readyState = 0;
  sentMessages: string[] = [];

  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onerror: ((event: any) => void) | null = null;
  onclose: ((event: any) => void) | null = null;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  open() {
    this.readyState = 1;
    this.onopen?.();
  }

  send(data: string) {
    this.sentMessages.push(data);
  }

  receive(data: any) {
    const text = typeof data === 'string' ? data : JSON.stringify(data);
    this.onmessage?.({ data: text });
  }

  close(code = 1000, reason = '') {
    this.readyState = 3;
    this.onclose?.({ code, reason });
  }
}

/** Creates a connected RoomSocket backed by a MockWebSocket. */
function connectSocket(options: Record<string, any> = {}): {
  socket: RoomSocket;
  ws: MockWebSocket;
} {
  const socket = new RoomSocket({
    roomId: 'room-ops',
    webSocketFactory: (u: string) => new MockWebSocket(u) as unknown as WebSocket,
    ...options,
  });
  socket.connect();
  const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
  ws.open();
  ws.sentMessages = [];
  return { socket, ws };
}

/** Reads the most recently sent frame. */
function lastSentFrame(ws: MockWebSocket): ClientFrame {
  return JSON.parse(ws.sentMessages[ws.sentMessages.length - 1]);
}

/** Decodes a Tin JSON payload as UTF-8 JSON. */
function decodeJsonPayload(tin: TinJson): any {
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(tin.payload)));
}

const sampleRecord: LogRecord = {
  id: 'local-1',
  sceneId: '/',
  content: 'Hello world',
  speaker: 'Alice',
  speakerId: '1',
  timestamp: 1_700_000_000,
};

const sampleCharacter: Character = {
  id: '1',
  name: 'Hero',
  initiative: 12,
  iconUrl: 'icon-tin-id',
  color: '#ff0000',
  description: 'A brave hero',
  reference: 'ref',
  statuses: [{ name: 'HP', value: 10, max: 10 } as any],
  parameters: [{ name: 'STR', value: 3 } as any],
  chatPalette: 'palette',
  hideStatus: false,
  isNarrative: false,
  placedAsPiece: true,
  piecePosition: { x: 10, y: 20 },
};

describe('Tin codec (Issue #136)', () => {
  // spec: alhalte-banquette/protocol Tin JSON is { label: u64, payload: number[] }
  it('round-trips primitives, objects, arrays, and existing Tins', () => {
    expect(tinToJson(jsonToTin('hello'))).toBe('hello');
    expect(tinToJson(jsonToTin(true))).toBe(true);
    expect(tinToJson(jsonToTin(42))).toBe(42);
    expect(tinToJson(jsonToTin(-1.5))).toBe(-1.5);
    expect(tinToJson(jsonToTin(null))).toBe('');
    expect(tinToJson(jsonToTin({ x: 10, y: 20 }))).toEqual({ x: 10, y: 20 });
    expect(tinToJson(jsonToTin([1, 'two', false]))).toEqual([1, 'two', false]);

    // Existing Tin values pass through unchanged.
    const tin = jsonToTin('hi');
    expect(jsonToTin(tin)).toEqual(tin);

    // Explicit labels override the type-derived one.
    expect(jsonToTin({ a: 1 }, LABEL_JSON).label).toBe(LABEL_JSON);
    expect(jsonToTin('x').label).not.toBe(LABEL_LOG_RECORD);
  });

  it('encodes log records with a clientOpId and decodes them back to LogRecord', () => {
    const tin = logRecordToTin(sampleRecord, 'op-42');
    expect(tin.label).toBe(LABEL_LOG_RECORD);

    const json = decodeJsonPayload(tin);
    expect(json.content).toBe('Hello world');
    expect(json.speaker).toBe('Alice');
    expect(json.timestamp).toBe(1_700_000_000);
    expect(json.clientOpId).toBe('op-42');

    const record = tinToLogRecord(tin, '/', 2, 7);
    expect(record).not.toBeNull();
    expect(record!.content).toBe('Hello world');
    expect(record!.speaker).toBe('Alice');
    expect(record!.id).toBe('local-1');
    expect(record!.sceneId).toBe('/');

    // Tombstones decode to null.
    expect(tinToLogRecord(null, '/')).toBeNull();
  });

  it('maps Character models to Tin attribute maps and back', () => {
    const map = characterToTinMap(sampleCharacter);
    expect(map['std::name'].payload).toEqual(
      Array.from(new TextEncoder().encode('Hero')),
    );
    expect(Array.isArray(map.initiative.payload)).toBe(true);
    expect(map.statuses.label).toBe(LABEL_JSON);
    expect(map.piecePosition.label).toBe(LABEL_JSON);

    const restored = tinMapToCharacter('1', map);
    expect(restored).toEqual(sampleCharacter);
  });
});

describe('ClientFrame wire payloads (Issue #136)', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
  });

  it('sends AppendLog with a Tin JSON payload instead of a raw record', () => {
    const { socket, ws } = connectSocket();
    socket.appendLog('/', sampleRecord);

    const frame = lastSentFrame(ws) as any;
    expect(frame.AppendLog.scene_id).toBe('/');
    const entry = frame.AppendLog.entry as TinJson;
    expect(typeof entry.label).toBe('number');
    expect(Array.isArray(entry.payload)).toBe(true);
    expect(decodeJsonPayload(entry).content).toBe('Hello world');
  });

  it('keeps null AppendLog entries as tombstones', () => {
    const { socket, ws } = connectSocket();
    socket.appendLog('/', null);
    expect(lastSentFrame(ws)).toEqual({ AppendLog: { scene_id: '/', entry: null } });
  });

  it('sends CreateCharacter / UpdateCharacter as HashMap<String, Tin>', async () => {
    const { socket, ws } = connectSocket();

    const createPromise = socket.requestCreateCharacter(sampleCharacter);
    const created = lastSentFrame(ws) as any;
    expect(created.CreateCharacter.data['std::name']).toEqual({
      label: jsonToTin('Hero').label,
      payload: Array.from(new TextEncoder().encode('Hero')),
    });
    expect(Array.isArray(created.CreateCharacter.data.initiative.payload)).toBe(true);
    ws.receive({ CharacterCreated: { character_id: 3 } });
    await createPromise;

    const updatePromise = socket.requestUpdateCharacter(7, sampleCharacter);
    const updated = lastSentFrame(ws) as any;
    expect(updated.UpdateCharacter.character_id).toBe(7);
    expect(updated.UpdateCharacter.data['std::name']).toBeDefined();
    ws.receive({ CharacterUpdated: { character_id: 7 } });
    await updatePromise;
  });
});

describe('Promise-based server operations (Issue #136)', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('resolves requestAppendLog only on the echo carrying its own clientOpId', async () => {
    const { socket, ws } = connectSocket();
    const promise = socket.requestAppendLog('/', sampleRecord);

    const sent = lastSentFrame(ws) as any;
    const clientOpId = decodeJsonPayload(sent.AppendLog.entry).clientOpId;
    expect(typeof clientOpId).toBe('string');

    // Another client's broadcast must not resolve the operation.
    ws.receive({
      LogEventAppended: {
        scene_id: '/',
        chunk_index: 0,
        position: 0,
        entry: logRecordToTin({ ...sampleRecord, content: 'other' }, 'someone-else'),
      },
    });
    expect(socket.getPendingOperationCount()).toBe(1);

    ws.receive({
      LogEventAppended: {
        scene_id: '/',
        chunk_index: 0,
        position: 1,
        entry: logRecordToTin(sampleRecord, clientOpId),
      },
    });

    const ack = await promise;
    expect(ack.scene_id).toBe('/');
    expect(ack.chunk_index).toBe(0);
    expect(ack.position).toBe(1);
    expect(socket.getPendingOperationCount()).toBe(0);
  });

  it('resolves requestUpdateState / requestDeleteState on matching broadcasts', async () => {
    const { socket, ws } = connectSocket();

    const updatePromise = socket.requestUpdateState('/', 'piece::1::pos', { x: 1, y: 2 });
    const updateFrame = lastSentFrame(ws) as any;
    const sentValue = updateFrame.UpdateState.value as TinJson;

    // A different value for the same key does not resolve it.
    ws.receive({
      StateUpdated: {
        scene_id: '/',
        key: 'piece::1::pos',
        value: jsonToTin({ x: 9, y: 9 }),
      },
    });
    expect(socket.getPendingOperationCount()).toBe(1);

    ws.receive({ StateUpdated: { scene_id: '/', key: 'piece::1::pos', value: sentValue } });
    // The acknowledgement exposes the decoded value.
    await expect(updatePromise).resolves.toEqual({
      scene_id: '/',
      key: 'piece::1::pos',
      value: { x: 1, y: 2 },
    });

    // banquette also broadcasts to the scene, so the sender can receive the same
    // frame twice: the duplicate must be harmless (no pending op, same cache value).
    ws.receive({ StateUpdated: { scene_id: '/', key: 'piece::1::pos', value: sentValue } });
    expect(socket.getPendingOperationCount()).toBe(0);
    expect(socket.getState('/')?.get('piece::1::pos')).toEqual({ x: 1, y: 2 });

    const deletePromise = socket.requestDeleteState('/', 'piece::1::pos');
    ws.receive({ StateDeleted: { scene_id: '/', key: 'piece::1::pos' } });
    await expect(deletePromise).resolves.toEqual({ scene_id: '/', key: 'piece::1::pos' });
    expect(socket.getPendingOperationCount()).toBe(0);
  });

  it('resolves state requests for a scene the client is not currently in', async () => {
    const { socket, ws } = connectSocket();
    // The socket entered '/' on open; the request targets another scene, so only the
    // direct ack (not a scene broadcast) can reach this client.
    expect(socket.getScenes()).not.toContain('tavern');

    const updatePromise = socket.requestUpdateState('tavern', 'std::b-img', 'asset-1');
    const sent = lastSentFrame(ws) as any;
    expect(sent.UpdateState.scene_id).toBe('tavern');
    ws.receive({
      StateUpdated: { scene_id: 'tavern', key: 'std::b-img', value: sent.UpdateState.value },
    });
    await expect(updatePromise).resolves.toEqual({
      scene_id: 'tavern',
      key: 'std::b-img',
      value: 'asset-1',
    });

    const deletePromise = socket.requestDeleteState('tavern', 'std::b-img');
    ws.receive({ StateDeleted: { scene_id: 'tavern', key: 'std::b-img' } });
    await expect(deletePromise).resolves.toEqual({ scene_id: 'tavern', key: 'std::b-img' });
    expect(socket.getPendingOperationCount()).toBe(0);
  });

  it('resolves scene create/delete on the new acks', async () => {
    const { socket, ws } = connectSocket();

    const createPromise = socket.requestCreateScene('tavern', true);
    expect(lastSentFrame(ws)).toEqual({
      CreateScene: { scene_id: 'tavern', inherit_juices: true },
    });
    ws.receive({ SceneCreated: { scene_id: 'tavern' } });
    await expect(createPromise).resolves.toEqual({ scene_id: 'tavern' });

    const deletePromise = socket.requestDeleteScene('tavern');
    ws.receive({ SceneDeleted: { scene_id: 'tavern' } });
    await expect(deletePromise).resolves.toEqual({ scene_id: 'tavern' });
    // The local scene cache follows the ack.
    expect(socket.getScenes()).not.toContain('tavern');
  });

  it('sends the optional scene name with CreateScene', async () => {
    const { socket, ws } = connectSocket();

    const createPromise = socket.requestCreateScene('tavern', true, '酒場');
    expect(lastSentFrame(ws)).toEqual({
      CreateScene: { scene_id: 'tavern', inherit_juices: true, name: '酒場' },
    });
    ws.receive({ SceneCreated: { scene_id: 'tavern' } });
    await expect(createPromise).resolves.toEqual({ scene_id: 'tavern' });
  });

  it('resolves requestMoveCharacter on CharacterMoved', async () => {
    const { socket, ws } = connectSocket();

    const promise = socket.requestMoveCharacter(7, 'dungeon');
    expect(lastSentFrame(ws)).toEqual({
      MoveCharacter: { character_id: 7, to_scene_id: 'dungeon' },
    });

    ws.receive({ CharacterMoved: { character_id: 9, to_scene_id: 'dungeon' } });
    expect(socket.getPendingOperationCount()).toBe(1);

    ws.receive({ CharacterMoved: { character_id: 7, to_scene_id: 'dungeon' } });
    await expect(promise).resolves.toEqual({ character_id: 7, to_scene_id: 'dungeon' });
  });

  it('resolves character CRUD acks FIFO and rejects on failure codes', async () => {
    const { socket, ws } = connectSocket();

    const first = socket.requestCreateCharacter(sampleCharacter);
    const second = socket.requestCreateCharacter({ ...sampleCharacter, name: 'Mage' });

    ws.receive({ CharacterCreated: { character_id: 1 } });
    await expect(first).resolves.toEqual({ character_id: 1 });
    expect(socket.getPendingOperationCount()).toBe(1);

    ws.receive({ Error: { code: 'CreateCharacterFailed', message: 'nope' } });
    await expect(second).rejects.toMatchObject({
      name: 'ServerOperationError',
      code: 'CreateCharacterFailed',
    });
    expect(socket.getPendingOperationCount()).toBe(0);

    const update = socket.requestUpdateCharacter(1, sampleCharacter);
    ws.receive({ CharacterUpdated: { character_id: 1 } });
    await expect(update).resolves.toEqual({ character_id: 1 });

    const remove = socket.requestDeleteCharacter(1);
    ws.receive({ CharacterDeleted: { character_id: 1 } });
    await expect(remove).resolves.toEqual({ character_id: 1 });
  });

  it('resolves requestRewindLog on LogEventRewound', async () => {
    const { socket, ws } = connectSocket();

    const promise = socket.requestRewindLog('/', 3, 4);
    expect(lastSentFrame(ws)).toEqual({
      RewindLog: { scene_id: '/', chunk_index: 3, position: 4 },
    });
    ws.receive({ LogEventRewound: { scene_id: '/', chunk_index: 3, position: 4 } });
    await expect(promise).resolves.toEqual({ scene_id: '/', chunk_index: 3, position: 4 });
  });

  it('rejects the matching operation on Error and all operations on PermissionDenied', async () => {
    const { socket, ws } = connectSocket();

    const append = socket.requestAppendLog('/', sampleRecord);
    const appendSettled = append.catch((error) => error);
    ws.receive({ Error: { code: 'AppendLogFailed', message: 'denied by scene' } });
    const appendError = await appendSettled;
    expect(appendError).toBeInstanceOf(ServerOperationError);
    expect(appendError.code).toBe('AppendLogFailed');
    expect(socket.getPendingOperationCount()).toBe(0);

    const scene = socket.requestCreateScene('tavern');
    const state = socket.requestUpdateState('tavern', 'std::bgm', 'ting');
    ws.receive({ Error: { code: 'PermissionDenied', message: '閲覧者権限' } });
    await expect(scene).rejects.toMatchObject({ code: 'PermissionDenied' });
    await expect(state).rejects.toMatchObject({ code: 'PermissionDenied' });
    expect(socket.getPendingOperationCount()).toBe(0);
  });

  it('rejects with OperationTimeoutError and cleans up pending state after 8s', async () => {
    vi.useFakeTimers();
    const { socket } = connectSocket();

    const promise = socket.requestCreateScene('tavern');
    const settled = promise.catch((error) => error);
    expect(socket.getPendingOperationCount()).toBe(1);

    vi.advanceTimersByTime(DEFAULT_OPERATION_TIMEOUT_MS - 1);
    expect(socket.getPendingOperationCount()).toBe(1);

    vi.advanceTimersByTime(1);
    const error = await settled;
    expect(error).toBeInstanceOf(OperationTimeoutError);
    expect(error.code).toBe('timeout');
    expect(error.operation).toBe('createScene');
    expect(error.timeoutMs).toBe(DEFAULT_OPERATION_TIMEOUT_MS);
    expect(socket.getPendingOperationCount()).toBe(0);
  });

  it('honours an explicit timeout and rejects pending operations when the socket closes', async () => {
    vi.useFakeTimers();
    const { socket, ws } = connectSocket();

    const promise = socket.requestDeleteScene('tavern', 500);
    const settled = promise.catch((error) => error);
    vi.advanceTimersByTime(500);
    const error = await settled;
    expect(error.code).toBe('timeout');
    expect(error.timeoutMs).toBe(500);

    const pending = socket.requestCreateScene('offline-scene');
    const pendingSettled = pending.catch((closeError) => closeError);
    ws.close(1006, 'lost');
    const closeError = await pendingSettled;
    expect(closeError).toBeInstanceOf(ServerOperationError);
    expect(closeError.code).toBe('disconnected');
    expect(socket.getPendingOperationCount()).toBe(0);
  });

  it('resolves query APIs on their snapshot frames', async () => {
    const { socket, ws } = connectSocket();

    const sceneList = socket.querySceneList();
    expect(JSON.parse(ws.sentMessages[ws.sentMessages.length - 1])).toBe('ListScenes');
    ws.receive({ SceneList: { scenes: ['/', 'tavern'] } });
    await expect(sceneList).resolves.toEqual(['/', 'tavern']);

    const state = socket.queryState('tavern');
    ws.receive({ StateSnapshot: { scene_id: 'tavern', states: { 'std::bgm': jsonToTin('t') } } });
    await expect(state).resolves.toEqual({ 'std::bgm': 't' });

    const biblio = socket.queryLogBiblio('tavern');
    ws.receive({ LogBiblioSnapshot: { scene_id: 'tavern', biblio: { chunks: [], head: [0, 3] } } });
    await expect(biblio).resolves.toEqual({ chunks: [], head: [0, 3] });

    const chunk = socket.queryLogChunk('tavern', 0);
    ws.receive({ LogChunkSnapshot: { scene_id: 'tavern', chunk_index: 0, entries: [null] } });
    await expect(chunk).resolves.toEqual([null]);

    const characters = socket.queryListCharacters();
    ws.receive({ CharacterList: { characters: { '1': characterToTinMap(sampleCharacter) } } });
    const list = await characters;
    expect(list).toHaveLength(1);
    expect(list[0].name).toBe('Hero');
    expect(socket.getPendingOperationCount()).toBe(0);
  });
});

describe('ServerFrame dispatch for previously unhandled frames (Issue #136)', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
  });

  it('dispatches StateSnapshot, LogBiblioSnapshot, CharacterInfo, and LogEventRewound', () => {
    const onStateSnapshot = vi.fn();
    const onLogBiblioSnapshot = vi.fn();
    const onCharacterInfo = vi.fn();
    const onLogEventRewound = vi.fn();
    const onServerFrame = vi.fn();

    const { socket, ws } = connectSocket({
      chunkLogManager: new ChunkLogManager(),
      onStateSnapshot,
      onLogBiblioSnapshot,
      onCharacterInfo,
      onLogEventRewound,
      onServerFrame,
    });
    const received: string[] = [];
    socket.subscribe((frame) => {
      if (typeof frame === 'object' && frame !== null) {
        received.push(Object.keys(frame)[0]);
      }
    });

    ws.receive({ StateSnapshot: { scene_id: 'tavern', states: { 'std::bgm': 'x' } } });
    expect(onStateSnapshot).toHaveBeenCalledWith('tavern', { 'std::bgm': 'x' });
    expect(socket.getState('tavern')?.get('std::bgm')).toBe('x');

    ws.receive({ LogBiblioSnapshot: { scene_id: 'tavern', biblio: { chunks: [], head: [0, 1] } } });
    expect(onLogBiblioSnapshot).toHaveBeenCalledWith('tavern', { chunks: [], head: [0, 1] });

    ws.receive({ CharacterInfo: { character_id: 5, data: { 'std::name': jsonToTin('Hero') } } });
    expect(onCharacterInfo).toHaveBeenCalledWith(5, { 'std::name': jsonToTin('Hero') });
    expect(socket.getCharacters().find((c) => c.id === '5')?.name).toBe('Hero');

    ws.receive({ LogEventRewound: { scene_id: 'tavern', chunk_index: 0, position: 2 } });
    expect(onLogEventRewound).toHaveBeenCalledWith('tavern', 0, 2);

    expect(received).toContain('StateSnapshot');
    expect(received).toContain('LogBiblioSnapshot');
    expect(received).toContain('CharacterInfo');
    expect(received).toContain('LogEventRewound');
    expect(onServerFrame).toHaveBeenCalledTimes(4);
  });

  it('dispatches the remaining ServerFrame variants to their callbacks', () => {
    const onSceneInfo = vi.fn();
    const onLogEventEdited = vi.fn();
    const onLogEventDeleted = vi.fn();
    const onSceneCreated = vi.fn();
    const onSceneDeleted = vi.fn();
    const onCharacterMoved = vi.fn();
    const onWebRtcSignal = vi.fn();
    const onServerFrame = vi.fn();

    const { socket, ws } = connectSocket({
      onSceneInfo,
      onLogEventEdited,
      onLogEventDeleted,
      onSceneCreated,
      onSceneDeleted,
      onCharacterMoved,
      onWebRtcSignal,
      onServerFrame,
    });

    ws.receive({ SceneInfo: { id: 'tavern', present_character_count: 1 } });
    expect(onSceneInfo).toHaveBeenCalledWith({ id: 'tavern', present_character_count: 1 });

    ws.receive({ SceneCreated: { scene_id: 'tavern' } });
    expect(onSceneCreated).toHaveBeenCalledWith('tavern');
    expect(socket.getScenes()).toContain('tavern');

    ws.receive({ SceneDeleted: { scene_id: 'tavern' } });
    expect(onSceneDeleted).toHaveBeenCalledWith('tavern');
    expect(socket.getScenes()).not.toContain('tavern');

    ws.receive({
      LogEventEdited: { scene_id: '/', chunk_index: 0, position: 1, entry: jsonToTin('new') },
    });
    expect(onLogEventEdited).toHaveBeenCalledWith('/', 0, 1, jsonToTin('new'));

    ws.receive({ LogEventDeleted: { scene_id: '/', chunk_index: 0, position: 2 } });
    expect(onLogEventDeleted).toHaveBeenCalledWith('/', 0, 2);

    ws.receive({ CharacterMoved: { character_id: 3, to_scene_id: 'dungeon' } });
    expect(onCharacterMoved).toHaveBeenCalledWith(3, 'dungeon');

    ws.receive({ WebRtcSignal: { from_client_id: 9, payload: { Offer: 'sdp' } } });
    expect(onWebRtcSignal).toHaveBeenCalledWith(9, { Offer: 'sdp' });

    expect(onServerFrame).toHaveBeenCalledTimes(7);
  });
});
