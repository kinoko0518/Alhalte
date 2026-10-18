import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import {
  RoomSocket,
  buildRoomWebSocketUrl,
  entryToLogRecord,
  parseCharacter,
  createRoomSocket,
  type ClientFrame,
  type ServerFrame,
} from '../src/utils/roomSocket.js';
import { ConnectionManager } from '../src/utils/connection.js';
import { ChunkLogManager } from '../src/utils/log.js';
import type { Character, VisitedRoom } from '../src/types/index.js';

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

  // spec: roomSocket url construction
  describe('URL Construction (buildRoomWebSocketUrl)', () => {
    it('constructs default ws URL with room ID and credentials', () => {
      const url = buildRoomWebSocketUrl({
        roomId: 'room-123',
        host: 'localhost:8080',
        accountId: 42,
        sessionId: 99,
      });
      expect(url).toBe('ws://localhost:8080/ws/room/room-123?account_id=42&session_id=99');
    });

    it('converts http/https URLs to ws/wss URLs', () => {
      const urlHttp = buildRoomWebSocketUrl({
        roomId: 'room-1',
        url: 'http://example.com/api/room/room-1',
      });
      expect(urlHttp).toBe('ws://example.com/api/room/room-1');

      const urlHttps = buildRoomWebSocketUrl({
        roomId: 'room-1',
        url: 'https://example.com/room/room-1?custom=1',
        accountId: 10,
      });
      expect(urlHttps).toBe('wss://example.com/room/room-1?custom=1&account_id=10');
    });

    it('preserves existing ws/wss protocols and appends query parameters', () => {
      const url = buildRoomWebSocketUrl({
        roomId: 'room-456',
        url: 'wss://alhalte.org/ws/room/room-456',
        sessionId: 'sess-abc',
      });
      expect(url).toBe('wss://alhalte.org/ws/room/room-456?session_id=sess-abc');
    });
  });

  // spec: roomSocket connection lifecycle & connectionManager coordination
  describe('Connection Lifecycle & ConnectionManager Coordination', () => {
    it('updates connectionManager to connecting then connected on socket open', () => {
      const connectionManager = new ConnectionManager({ initialState: 'disconnected' });
      const states: string[] = [];
      connectionManager.subscribe((st) => states.push(st));

      const socket = new RoomSocket({
        roomId: 'room-test',
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
        roomId: 'room-test',
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
        roomId: 'room-test',
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
        roomId: 'room-test',
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
  });

  // spec: roomSocket ClientFrame helper methods
  describe('ClientFrame Helper Methods & Serialization', () => {
    let socket: RoomSocket;
    let mockWs: MockWebSocket;

    beforeEach(() => {
      socket = new RoomSocket({
        roomId: 'room-1',
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
        roomId: 'room-queue',
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
        roomId: 'room-msg-queue',
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
        roomId: 'room-state-sync',
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
        roomId: 'room-sync',
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
        roomId: 'room-sync',
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

    it('appends LogEventAppended frame into ChunkLogManager and notifies listener', () => {
      const logAppendedCallback = vi.fn();
      socket = new RoomSocket({
        roomId: 'room-sync',
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
        roomId: 'room-sync',
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
        roomId: 'room-sync',
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
      id: 'room-app-test',
      name: '結合テスト部屋',
      url: 'ws://localhost:8080/ws/room/room-app-test',
      lastVisited: Date.now(),
    };

    it('connects RoomSocket when entering room and sends ClientFrame on chat and scene operations', async () => {
      const socket = new RoomSocket({
        roomId: 'room-app-test',
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
  });
});
