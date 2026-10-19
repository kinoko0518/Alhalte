import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import {
  NON_RETRYABLE_CLOSE_CODES,
  RoomSocket,
  buildRoomWebSocketUrl,
  entryToLogRecord,
  parseCharacter,
  createRoomSocket,
  type ClientFrame,
  type ServerFrame,
} from '../src/utils/roomSocket.js';
import { isValidRoomId } from '../src/utils/home.js';
import { ConnectionManager } from '../src/utils/connection.js';
import { ChunkLogManager } from '../src/utils/log.js';
import type { Character, VisitedRoom } from '../src/types/index.js';

interface RoomIdContract {
  generated_hex: string[];
  legacy_decimal: string[];
}

function loadRoomIdContract(): RoomIdContract {
  // `pnpm test` runs from `web/alhalte-argenterie`.
  const contractPath = resolve(process.cwd(), '../../contracts/room_ids.json');
  return JSON.parse(readFileSync(contractPath, 'utf8')) as RoomIdContract;
}

/**
 * Mock WebSocket implementation for unit and integration tests.
 */
class MockWebSocket {
  static instances: MockWebSocket[] = [];
  url: string;
  readyState = 0; // 0 = CONNECTING, 1 = OPEN, 2 = CLOSING, 3 = CLOSED
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

  error(err?: any) {
    this.onerror?.(err || new Error('WebSocket error'));
  }

  close(code = 1000, reason = '') {
    this.readyState = 3;
    this.onclose?.({ code, reason });
  }
}

describe('RoomSocket - WebSocket Synchronization (Issue #56)', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
  });

  // spec: roomSocket url construction (Issue #154)
  describe('URL Construction (buildRoomWebSocketUrl)', () => {
    it('constructs default ws URL with room ID and credentials', () => {
      const url = buildRoomWebSocketUrl({
        roomId: '0123456789abcdef',
        host: 'localhost:8080',
        accountId: 42,
        sessionId: 99,
      });
      expect(url).toBe(
        'ws://localhost:8080/ws/room/0123456789abcdef?account_id=42&session_id=99',
      );
    });

    it('uses only the origin of an invitation page URL and builds /ws/room/<id>', () => {
      const urlHttps = buildRoomWebSocketUrl({
        roomId: 'abcdef0123456789',
        url: 'https://alhalte.example/rooms/abcdef0123456789#room-key',
        accountId: 10,
      });
      expect(urlHttps).toBe(
        'wss://alhalte.example/ws/room/abcdef0123456789?account_id=10',
      );

      const urlHttp = buildRoomWebSocketUrl({
        roomId: '777',
        url: 'http://127.0.0.1:5173/rooms/777#room-key',
      });
      expect(urlHttp).toBe('ws://127.0.0.1:5173/ws/room/777');
    });

    it('does not inherit application paths such as /api or /room from page URLs', () => {
      const apiUrl = buildRoomWebSocketUrl({
        roomId: '9f3a7c1e5b2d8046',
        url: 'http://example.com/api/room/9f3a7c1e5b2d8046',
      });
      expect(apiUrl).toBe('ws://example.com/ws/room/9f3a7c1e5b2d8046');

      const pageUrl = buildRoomWebSocketUrl({
        roomId: '9f3a7c1e5b2d8046',
        url: 'https://example.com/room/9f3a7c1e5b2d8046?custom=1',
        accountId: 10,
      });
      expect(pageUrl).toBe('wss://example.com/ws/room/9f3a7c1e5b2d8046?account_id=10');
    });

    it('uses an explicit ws(s)://.../ws/room/... URL as-is', () => {
      const url = buildRoomWebSocketUrl({
        roomId: 'fedcba9876543210',
        url: 'wss://alhalte.org/ws/room/fedcba9876543210',
        sessionId: 'sess-abc',
      });
      expect(url).toBe('wss://alhalte.org/ws/room/fedcba9876543210?session_id=sess-abc');
    });

    it('rebuilds an explicit ws URL outside /ws/room/ from its origin', () => {
      const url = buildRoomWebSocketUrl({
        roomId: 'fedcba9876543210',
        url: 'wss://alhalte.org/socket/legacy',
      });
      expect(url).toBe('wss://alhalte.org/ws/room/fedcba9876543210');
    });

    it('builds /ws/room/<id> from a relative invitation path', () => {
      const url = buildRoomWebSocketUrl({
        roomId: 'a1b2c3d4e5f60718',
        url: '/rooms/a1b2c3d4e5f60718#room-key',
        host: 'alhalte.example:8443',
      });
      expect(url).toBe('ws://alhalte.example:8443/ws/room/a1b2c3d4e5f60718');
    });

    // spec: contracts/room_ids.json room ID contract (TypeScript side)
    it('accepts every contract room ID and builds the same /ws/room path', () => {
      const contract = loadRoomIdContract();
      expect(contract.generated_hex.length).toBeGreaterThan(0);
      expect(contract.legacy_decimal.length).toBeGreaterThan(0);

      for (const roomId of [...contract.generated_hex, ...contract.legacy_decimal]) {
        expect(isValidRoomId(roomId)).toBe(true);
        expect(
          buildRoomWebSocketUrl({ roomId, host: 'alhalte.example' }),
        ).toBe(`ws://alhalte.example/ws/room/${roomId}`);
      }
    });
  });

  // spec: roomSocket connection lifecycle & connectionManager coordination
  describe('Connection Lifecycle & ConnectionManager Coordination', () => {
    it('updates connectionManager to connecting then connected on socket open', () => {
      const connectionManager = new ConnectionManager({ initialState: 'disconnected' });
      const states: string[] = [];
      connectionManager.subscribe((st) => states.push(st));

      const socket = new RoomSocket({
        roomId: '00000000000000aa',
        connectionManager,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      socket.connect();
      expect(connectionManager.getState()).toBe('connecting');

      const mockWs = MockWebSocket.instances[0];
      expect(mockWs).toBeDefined();

      mockWs.open();
      expect(connectionManager.getState()).toBe('connected');
      expect(socket.isConnected).toBe(true);
      expect(states).toContain('connecting');
      expect(states).toContain('connected');
    });

    it('handles unexpected disconnect, transitions to disconnected, and supports reconnection', async () => {
      vi.useFakeTimers();
      const connectionManager = new ConnectionManager();
      const socket = new RoomSocket({
        roomId: '00000000000000aa',
        connectionManager,
        reconnectInterval: 1000,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      socket.connect();
      const mockWs1 = MockWebSocket.instances[0];
      mockWs1.open();
      expect(connectionManager.getState()).toBe('connected');

      // Unexpected disconnect from server
      mockWs1.close(1006, 'Abnormal closure');
      expect(connectionManager.getState()).toBe('disconnected');
      expect(socket.isConnected).toBe(false);

      // Fast-forward auto-reconnection timer
      vi.advanceTimersByTime(1000);

      // Reconnect attempt triggered
      expect(MockWebSocket.instances.length).toBe(2);
      expect(connectionManager.getState()).toBe('reconnecting');

      const mockWs2 = MockWebSocket.instances[1];
      mockWs2.open();
      expect(connectionManager.getState()).toBe('connected');
      expect(socket.isConnected).toBe(true);

      vi.useRealTimers();
    });

    it('triggers manual reconnection via connectionManager.reconnect()', async () => {
      const connectionManager = new ConnectionManager({ initialState: 'disconnected' });
      const socket = new RoomSocket({
        roomId: '00000000000000aa',
        connectionManager,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      const reconnectPromise = connectionManager.reconnect();
      expect(connectionManager.getState()).toBe('reconnecting');

      const mockWs = MockWebSocket.instances[0];
      mockWs.open();

      await reconnectPromise;
      expect(connectionManager.getState()).toBe('connected');
      expect(socket.isConnected).toBe(true);
    });

    it('does not auto-reconnect on intentional disconnect()', () => {
      vi.useFakeTimers();
      const connectionManager = new ConnectionManager();
      const socket = new RoomSocket({
        roomId: '00000000000000aa',
        connectionManager,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      socket.connect();
      const mockWs = MockWebSocket.instances[0];
      mockWs.open();

      socket.disconnect();
      expect(connectionManager.getState()).toBe('disconnected');

      vi.advanceTimersByTime(5000);
      expect(MockWebSocket.instances.length).toBe(1); // No new instance spawned

      vi.useRealTimers();
    });

    // spec: alhalte-argenterie/roomSocket onclose event and non-retryable close codes (Issue #154)
    it('captures onclose(event) and exposes it via onClose and getLastCloseEvent', () => {
      const onClose = vi.fn();
      const socket = new RoomSocket({
        roomId: '0123456789abcdef',
        onClose,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      socket.connect();
      const mockWs = MockWebSocket.instances[0];
      mockWs.open();
      mockWs.close(4403, 'room access denied');

      expect(onClose).toHaveBeenCalledTimes(1);
      const event = onClose.mock.calls[0][0] as CloseEvent;
      expect(event.code).toBe(4403);
      expect(event.reason).toBe('room access denied');
      expect(socket.getLastCloseEvent()).toBe(event);
    });

    it('stops automatic reconnection on non-retryable close codes', () => {
      vi.useFakeTimers();

      for (const code of [...NON_RETRYABLE_CLOSE_CODES]) {
        MockWebSocket.instances = [];
        const connectionManager = new ConnectionManager();
        const socket = new RoomSocket({
          roomId: '0123456789abcdef',
          connectionManager,
          reconnectInterval: 1000,
          webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
        });

        socket.connect();
        const mockWs = MockWebSocket.instances[0];
        mockWs.open();
        mockWs.close(code, 'rejected');

        expect(connectionManager.getState()).toBe('disconnected');
        vi.advanceTimersByTime(5000);
        expect(MockWebSocket.instances.length).toBe(1);
        vi.clearAllTimers();
      }

      vi.useRealTimers();
    });
  });

  // spec: roomSocket ClientFrame helper methods
  describe('ClientFrame Helper Methods & Serialization', () => {
    let socket: RoomSocket;
    let mockWs: MockWebSocket;

    beforeEach(() => {
      socket = new RoomSocket({
        roomId: '00000000000000af',
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      mockWs = MockWebSocket.instances[0];
      mockWs.open();
      mockWs.sentMessages = []; // Clear initial listScenes/characters messages
    });

    it('sends EnterScene, CreateScene, and DeleteScene frames', () => {
      socket.enterScene('tavern');
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toEqual({
        EnterScene: { scene_id: 'tavern' },
      });

      socket.createScene('dungeon', true);
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toEqual({
        CreateScene: { scene_id: 'dungeon', inherit_juices: true },
      });

      socket.deleteScene('old-scene');
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toEqual({
        DeleteScene: { scene_id: 'old-scene' },
      });
    });

    it('sends UpdateState and DeleteState frames with normalized Tin values', () => {
      socket.updateState('tavern', 'std::bgm', 'bgm-audio-tin-id');
      const updateMsg = JSON.parse(mockWs.sentMessages.pop()!);
      expect(updateMsg.UpdateState).toBeDefined();
      expect(updateMsg.UpdateState.scene_id).toBe('tavern');
      expect(updateMsg.UpdateState.key).toBe('std::bgm');
      expect(updateMsg.UpdateState.value.label).toBeDefined();
      expect(Array.isArray(updateMsg.UpdateState.value.payload)).toBe(true);

      socket.deleteState('tavern', 'std::bgm');
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toEqual({
        DeleteState: { scene_id: 'tavern', key: 'std::bgm' },
      });
    });

    it('sends AppendLog frame for chat and event entries', () => {
      socket.appendLog('tavern', { content: 'Hello world', speaker: 'Alice' });
      const appendMsg = JSON.parse(mockWs.sentMessages.pop()!);
      expect(appendMsg.AppendLog).toBeDefined();
      expect(appendMsg.AppendLog.scene_id).toBe('tavern');
      expect(appendMsg.AppendLog.entry).toBeDefined();

      socket.appendLog('tavern', null);
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toEqual({
        AppendLog: { scene_id: 'tavern', entry: null },
      });
    });

    it('sends CreateCharacter, UpdateCharacter, and DeleteCharacter frames', () => {
      socket.createCharacter(1, { name: 'Hero' });
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toEqual({
        CreateCharacter: { character_id: 1, data: { name: 'Hero' } },
      });

      socket.updateCharacter(1, { name: 'Hero Lv2' });
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toEqual({
        UpdateCharacter: { character_id: 1, data: { name: 'Hero Lv2' } },
      });

      socket.deleteCharacter(1);
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toEqual({
        DeleteCharacter: { character_id: 1 },
      });
    });

    it('sends SetDone, ListScenes, and ListCharacters frames', () => {
      socket.setDone(true);
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toEqual({
        SetDone: { done: true },
      });

      socket.listScenes();
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toBe('ListScenes');

      socket.listCharacters();
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toBe('ListCharacters');
    });

    it('automatically responds to HeartbeatPing with HeartbeatResponse', () => {
      mockWs.receive('HeartbeatPing');
      expect(JSON.parse(mockWs.sentMessages.pop()!)).toBe('HeartbeatResponse');
    });

    it('queues outgoing frames while disconnected and flushes upon connection', () => {
      const disconnectedSocket = new RoomSocket({
        roomId: '00000000000000ab',
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      // Send operations before connect
      disconnectedSocket.enterScene('queue-scene');
      disconnectedSocket.appendLog('queue-scene', { content: 'queued message' });

      disconnectedSocket.connect();
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      expect(ws.sentMessages.length).toBe(0);

      ws.open();
      // Flushed queued frames + initial enterScene, listScenes, listCharacters
      const messages = ws.sentMessages.map((m) => JSON.parse(m));
      expect(messages).toContainEqual({ EnterScene: { scene_id: 'queue-scene' } });
      expect(messages.some((m) => m?.AppendLog?.scene_id === 'queue-scene')).toBe(true);
    });

    // spec: alhalte-argenterie/roomSocket offline message queueing & bulk sending (Issue #88)
    it('accumulates chat messages in queue while disconnected and bulk-sends upon reconnecting', () => {
      const socket = new RoomSocket({
        roomId: '00000000000000ac',
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      expect(socket.getPendingMessageCount()).toBe(0);

      // Append multiple messages while disconnected
      socket.appendLog('scene-1', { content: 'First message', speaker: 'Alice' });
      socket.appendLog('scene-1', { content: 'Second message', speaker: 'Bob' });
      socket.appendLog('scene-1', { content: 'Third message', speaker: 'Charlie' });

      expect(socket.getPendingMessageCount()).toBe(3);
      const queue = socket.getMessageQueue();
      expect(queue).toHaveLength(3);

      // Connect socket
      socket.connect();
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      ws.open();

      // Messages should be bulk-sent in FIFO order
      const sentFrames = ws.sentMessages.map((m) => JSON.parse(m));
      const appendLogs = sentFrames.filter((f) => typeof f === 'object' && f !== null && 'AppendLog' in f);
      expect(appendLogs).toHaveLength(3);
      expect(socket.getPendingMessageCount()).toBe(0);
    });

    // spec: alhalte-argenterie/roomSocket offline state changes final state sync (Issue #88)
    it('deduplicates offline state updates and transmits only the final state upon reconnecting', () => {
      const socket = new RoomSocket({
        roomId: '00000000000000ad',
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      // Update piece position multiple times while offline
      socket.updateState('scene-1', 'piece::hero::pos', { x: 10, y: 10 });
      socket.updateState('scene-1', 'piece::hero::pos', { x: 50, y: 50 });
      socket.updateState('scene-1', 'piece::hero::pos', { x: 200, y: 350 });

      // Update BGM and then delete it
      socket.updateState('scene-1', 'std::bgm', 'bgm-temp');
      socket.deleteState('scene-1', 'std::bgm');

      // Update another key
      socket.updateState('scene-1', 'view::transform', { x: 0, y: 0, scale: 1.5 });

      // Pending state map should only contain deduplicated final values
      const pendingMap = socket.getPendingStateChanges();
      expect(pendingMap.size).toBe(3);
      expect(pendingMap.get('scene-1:::piece::hero::pos')).toEqual({
        UpdateState: {
          scene_id: 'scene-1',
          key: 'piece::hero::pos',
          value: { x: 200, y: 350 },
        },
      });
      expect(pendingMap.get('scene-1:::std::bgm')).toEqual({
        DeleteState: {
          scene_id: 'scene-1',
          key: 'std::bgm',
        },
      });

      // Connect
      socket.connect();
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      ws.open();

      // Sent frames should only have the 3 final updates (not the intermediate 10,10 or 50,50 or bgm-temp)
      const sentFrames = ws.sentMessages.map((m) => JSON.parse(m));
      const heroPosUpdates = sentFrames.filter(
        (f) => f?.UpdateState?.key === 'piece::hero::pos',
      );
      expect(heroPosUpdates).toHaveLength(1);
      expect(heroPosUpdates[0].UpdateState.value).toEqual({ x: 200, y: 350 });

      const bgmUpdates = sentFrames.filter((f) => f?.UpdateState?.key === 'std::bgm');
      expect(bgmUpdates).toHaveLength(0); // Overwritten by DeleteState
      const bgmDeletes = sentFrames.filter((f) => f?.DeleteState?.key === 'std::bgm');
      expect(bgmDeletes).toHaveLength(1);

      expect(socket.getPendingStateChanges().size).toBe(0);
    });
  });

  // spec: roomSocket ServerFrame reception & state synchronization
  describe('ServerFrame Reception & State Synchronization', () => {
    let chunkManager: ChunkLogManager;
    let socket: RoomSocket;
    let mockWs: MockWebSocket;

    beforeEach(() => {
      chunkManager = new ChunkLogManager();
      socket = new RoomSocket({
        roomId: '00000000000000ae',
        chunkLogManager: chunkManager,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      mockWs = MockWebSocket.instances[0];
      mockWs.open();
    });

    it('synchronizes SceneList frame into getScenes() and triggers onSceneList', () => {
      const sceneListCallback = vi.fn();
      socket = new RoomSocket({
        roomId: '00000000000000ae',
        onSceneList: sceneListCallback,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      ws.open();

      ws.receive({
        SceneList: {
          scenes: ['/', 'tavern', 'dungeon'],
        },
      });

      expect(socket.getScenes()).toEqual(['/', 'tavern', 'dungeon']);
      expect(sceneListCallback).toHaveBeenCalledWith(['/', 'tavern', 'dungeon']);
    });

    // spec: roomSocket SceneListはSceneSummaryの列でシーン名を持つ (Issue #159)
    it('accepts SceneSummary entries in SceneList and caches the server names', () => {
      const sceneListCallback = vi.fn();
      socket = new RoomSocket({
        roomId: '00000000000000ae',
        onSceneList: sceneListCallback,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      ws.open();

      ws.receive({
        SceneList: {
          scenes: [
            { id: '/', name: 'メインシーン', present_character_count: 2 },
            { id: 'tavern', name: '酒場', active_client_count: 1 },
          ],
        },
      });

      expect(socket.getScenes()).toEqual(['/', 'tavern']);
      expect(sceneListCallback).toHaveBeenCalledWith(['/', 'tavern']);
      expect(socket.getSceneSummaries()).toEqual([
        { id: '/', name: 'メインシーン', present_character_count: 2 },
        { id: 'tavern', name: '酒場', active_client_count: 1 },
      ]);
    });

    // spec: roomSocket SceneInfoは他クライアントのrenameを通知する (Issue #159)
    it('forwards a SceneInfo rename to onSceneInfo', () => {
      const sceneInfoCallback = vi.fn();
      socket = new RoomSocket({
        roomId: '00000000000000ae',
        onSceneInfo: sceneInfoCallback,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      ws.open();

      ws.receive({ SceneInfo: { id: 'tavern', name: '酒場', present_character_count: 1 } });

      expect(sceneInfoCallback).toHaveBeenCalledWith({
        id: 'tavern',
        name: '酒場',
        present_character_count: 1,
      });
      expect(socket.getSceneSummaries()).toEqual([
        { id: 'tavern', name: '酒場', present_character_count: 1 },
      ]);
    });

    it('appends LogEventAppended frame into ChunkLogManager and notifies listener', () => {
      const logAppendedCallback = vi.fn();
      socket = new RoomSocket({
        roomId: '00000000000000ae',
        chunkLogManager: chunkManager,
        onLogAppended: logAppendedCallback,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      ws.open();

      ws.receive({
        LogEventAppended: {
          scene_id: 'tavern',
          chunk_index: 0,
          position: 1,
          entry: {
            id: 'log-1',
            speaker: 'Bob',
            content: 'Hello everyone!',
            timestamp: 1700000000,
          },
        },
      });

      const records = chunkManager.getLoadedRecords('tavern');
      expect(records.length).toBe(1);
      expect(records[0].content).toBe('Hello everyone!');
      expect(records[0].speaker).toBe('Bob');
      expect(logAppendedCallback).toHaveBeenCalled();
    });

    it('processes LogChunkSnapshot and loads multiple entries into ChunkLogManager', () => {
      mockWs.receive({
        LogChunkSnapshot: {
          scene_id: 'dungeon',
          chunk_index: 0,
          entries: [
            { id: 'log-1', speaker: 'Hero', content: 'Step 1' },
            { id: 'log-2', speaker: 'Hero', content: 'Step 2' },
          ],
        },
      });

      const records = chunkManager.getLoadedRecords('dungeon');
      expect(records.length).toBe(2);
      expect(records[0].content).toBe('Step 1');
      expect(records[1].content).toBe('Step 2');
    });

    it('synchronizes CharacterList, CharacterCreated, and CharacterDeleted', () => {
      const charListCallback = vi.fn();
      const charDeletedCallback = vi.fn();
      socket = new RoomSocket({
        roomId: '00000000000000ae',
        onCharacterList: charListCallback,
        onCharacterDeleted: charDeletedCallback,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      ws.open();

      ws.receive({
        CharacterList: {
          characters: {
            '1': { name: 'Fighter', initiative: 12 },
            '2': { name: 'Mage', initiative: 8 },
          },
        },
      });

      const chars = socket.getCharacters();
      expect(chars.length).toBe(2);
      expect(chars.find((c) => c.name === 'Fighter')).toBeDefined();
      expect(charListCallback).toHaveBeenCalled();

      ws.receive({
        CharacterDeleted: {
          character_id: 1,
        },
      });

      expect(socket.getCharacters().length).toBe(1);
      expect(socket.getCharacters()[0].name).toBe('Mage');
      expect(charDeletedCallback).toHaveBeenCalledWith(1);
    });

    it('synchronizes StateUpdated and StateDeleted into state map', () => {
      const stateUpdatedCallback = vi.fn();
      const stateDeletedCallback = vi.fn();
      socket = new RoomSocket({
        roomId: '00000000000000ae',
        onStateUpdated: stateUpdatedCallback,
        onStateDeleted: stateDeletedCallback,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      ws.open();

      ws.receive({
        StateUpdated: {
          scene_id: 'dungeon',
          key: 'atmosphere::fog',
          value: 'thick',
        },
      });

      expect(socket.getState('dungeon')?.get('atmosphere::fog')).toBe('thick');
      expect(stateUpdatedCallback).toHaveBeenCalledWith('dungeon', 'atmosphere::fog', 'thick');

      ws.receive({
        StateDeleted: {
          scene_id: 'dungeon',
          key: 'atmosphere::fog',
        },
      });

      expect(socket.getState('dungeon')?.get('atmosphere::fog')).toBeUndefined();
      expect(stateDeletedCallback).toHaveBeenCalledWith('dungeon', 'atmosphere::fog');
    });
  });

  // spec: App.svelte component integration with RoomSocket
  describe('App.svelte Component Integration', () => {
    const dummyRoom: VisitedRoom = {
      id: 'aabbccddeeff0011',
      name: '結合テスト部屋',
      url: 'ws://localhost:8080/ws/room/aabbccddeeff0011',
      lastVisited: Date.now(),
    };

    it('connects RoomSocket when entering room and sends ClientFrame on chat and scene operations', async () => {
      const socket = new RoomSocket({
        roomId: 'aabbccddeeff0011',
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      const mockWs = MockWebSocket.instances[0];
      mockWs.open();
      mockWs.sentMessages = [];

      const target = document.createElement('div');
      document.body.appendChild(target);

      const instance = mount(App, {
        target,
        props: {
          initialRoom: dummyRoom,
          roomSocket: socket,
        },
      });

      await new Promise((r) => setTimeout(r, 30));

      // 1. Verify sending message emits AppendLog ClientFrame
      const chatTextarea = target.querySelector<HTMLTextAreaElement>('textarea');
      if (chatTextarea) {
        chatTextarea.value = 'Hello from App integration test';
        chatTextarea.dispatchEvent(new Event('input', { bubbles: true }));

        const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
        sendBtn?.click();
        await new Promise((r) => setTimeout(r, 30));

        const sentFrames = mockWs.sentMessages.map((m) => JSON.parse(m));
        const appendFrame = sentFrames.find((f) => f?.AppendLog !== undefined);
        expect(appendFrame).toBeDefined();
        expect(appendFrame.AppendLog.scene_id).toBe('/');
      }

      // 2. Verify server frame receiving updates App state
      mockWs.receive({
        SceneList: {
          scenes: ['/', 'secret-dungeon'],
        },
      });
      await new Promise((r) => setTimeout(r, 30));

      // App should reflect newly received scene
      expect(socket.getScenes()).toContain('secret-dungeon');

      unmount(instance);
      target.remove();
    });

    // spec: argenterie/roomSocket Appのシーン管理操作はrequest*のack後にのみローカルへ反映する (Issue #109 / #143)
    it('sends CreateScene through requestCreateScene and applies the scene only after the SceneCreated ack', async () => {
      const socket = new RoomSocket({
        roomId: 'aabbccddeeff0011',
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });
      socket.connect();
      const mockWs = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      mockWs.open();
      mockWs.sentMessages = [];

      const target = document.createElement('div');
      document.body.appendChild(target);

      const instance = mount(App, {
        target,
        props: {
          initialRoom: dummyRoom,
          roomSocket: socket,
        },
      });

      await new Promise((r) => setTimeout(r, 30));

      // Open the management section and submit a new scene name.
      target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
      await new Promise((r) => setTimeout(r, 30));
      target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')?.click();
      await new Promise((r) => setTimeout(r, 20));

      const nameInput = target.querySelector<HTMLInputElement>(
        '[data-testid="new-scene-name-input"]',
      );
      expect(nameInput).toBeTruthy();
      nameInput!.value = '新しい間';
      nameInput!.dispatchEvent(new Event('input', { bubbles: true }));
      target.querySelector<HTMLButtonElement>('[data-testid="submit-create-scene-btn"]')?.click();
      await new Promise((r) => setTimeout(r, 20));

      // The request is the same `CreateScene` wire frame as before, but it is
      // now a promise that resolves on the `SceneCreated` ack.
      const createFrame = mockWs.sentMessages
        .map((m) => JSON.parse(m))
        .find((f) => f?.CreateScene !== undefined);
      expect(createFrame).toBeTruthy();
      expect(createFrame.CreateScene.inherit_juices).toBe(true);
      const newSceneId = createFrame.CreateScene.scene_id as string;
      expect(newSceneId.startsWith('scene-')).toBe(true);

      // Pending: the spinner replaces the create icon and nothing is selected yet.
      expect(target.querySelector('[data-testid="create-action-pending"]')).toBeTruthy();
      expect(
        target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]')?.value,
      ).toBe('メインシーン');

      mockWs.receive({ SceneCreated: { scene_id: newSceneId } });
      await new Promise((r) => setTimeout(r, 30));

      expect(target.querySelector('[data-testid="create-action-pending"]')).toBeNull();
      expect(
        target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]')?.value,
      ).toBe('新しい間');

      unmount(instance);
      target.remove();
    });
  });
});

// spec: alhalte-argenterie/roomSocket SetDone resolves only after the banquette ack
describe('RoomSocket - SetDone acknowledgement', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
  });

  it('resolves requestSetDone on DoneUpdated and rejects on SetDoneFailed', async () => {
    const socket = new RoomSocket({
      roomId: 'aabbccddeeff0011',
      webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
    });
    socket.connect();
    const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    ws.open();
    ws.sentMessages = [];

    const pending = socket.requestSetDone(true, 500);
    expect(socket.getPendingOperationCount()).toBe(1);
    expect(JSON.parse(ws.sentMessages[0])).toEqual({ SetDone: { done: true } });

    ws.receive({ DoneUpdated: { done: true } });
    await expect(pending).resolves.toEqual({ done: true });
    expect(socket.getPendingOperationCount()).toBe(0);

    const denied = socket.requestSetDone(true, 500);
    ws.receive({ Error: { code: 'SetDoneFailed', message: 'not an admin' } });
    await expect(denied).rejects.toMatchObject({ code: 'SetDoneFailed' });
    expect(socket.getPendingOperationCount()).toBe(0);
  });

  it('rejects requestSetDone when the server reports PermissionDenied', async () => {
    const socket = new RoomSocket({
      roomId: 'aabbccddeeff0011',
      webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
    });
    socket.connect();
    const ws = MockWebSocket.instances[MockWebSocket.instances.length - 1];
    ws.open();
    ws.sentMessages = [];

    const pending = socket.requestSetDone(true, 500);
    ws.receive({ Error: { code: 'PermissionDenied', message: 'viewer' } });
    await expect(pending).rejects.toMatchObject({ code: 'PermissionDenied' });
  });
});
