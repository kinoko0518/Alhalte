/**
 * Unit and integration tests for Server Connection Management and the durable
 * offline outbox.
 *
 * The global connection banner was removed in Issue #261; connection failures
 * surface inside the tab that performed the operation instead.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie`
 * - GitHub Issue #67: `feat(argenterie): サーバー接続状態の明示とオフライン検出インジケーターの実装`
 * - GitHub Issue #88: `オフライン動作に関する提言`
 * - GitHub Issue #261: シーン切替時、一瞬「サーバーに接続されていません」の赤い帯が出る
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import {
  ConnectionManager,
  createConnectionManager,
  type ConnectionState,
} from '../src/utils/connection.js';
import { RoomSocket } from '../src/utils/roomSocket.js';
import { getOutbox, resetOutbox } from '../src/utils/outbox.js';
import type { VisitedRoom } from '../src/types/index.js';

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

describe('Server Connection Management & Offline Synchronization (Issue #88 & #67)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.appendChild(target);
    MockWebSocket.instances = [];
  });

  afterEach(() => {
    target.remove();
    document.body.innerHTML = '';
  });

  const dummyRoom: VisitedRoom = {
    id: 'room-offline-test',
    name: 'オフラインテスト部屋',
    url: 'https://alhalte.example/rooms/room-offline-test#dummy-key',
    lastVisited: Date.now(),
  };

  // spec: alhalte-argenterie/connection ConnectionManager store and lifecycle
  describe('ConnectionManager Unit Tests', () => {
    it('manages connection states and notifies subscribers via Svelte store contract', () => {
      const manager = createConnectionManager({ initialState: 'connected' });
      expect(manager.state).toBe('connected');
      expect(manager.isConnected).toBe(true);
      expect(manager.isOffline).toBe(false);

      const states: ConnectionState[] = [];
      const unsubscribe = manager.subscribe((s) => states.push(s));

      expect(states).toEqual(['connected']);

      manager.setState('disconnected');
      expect(manager.state).toBe('disconnected');
      expect(manager.isOffline).toBe(true);
      expect(manager.isConnected).toBe(false);

      manager.setState('reconnecting');
      expect(manager.state).toBe('reconnecting');
      expect(manager.isOffline).toBe(true);
      expect(manager.isConnecting).toBe(true);

      manager.setState('connected');
      expect(manager.isConnected).toBe(true);

      unsubscribe();
      manager.setState('disconnected');
      expect(states).toEqual(['connected', 'disconnected', 'reconnecting', 'connected']);
    });

    it('triggers reconnect handler on reconnect() and handles success and error', async () => {
      const mockHandler = vi.fn().mockResolvedValue(undefined);
      const manager = new ConnectionManager({
        initialState: 'disconnected',
        reconnectHandler: mockHandler,
      });

      expect(manager.state).toBe('disconnected');
      await manager.reconnect();

      expect(mockHandler).toHaveBeenCalledTimes(1);
      expect(manager.state).toBe('connected');

      // Failure case
      const failingHandler = vi.fn().mockRejectedValue(new Error('Network error'));
      manager.setReconnectHandler(failingHandler);

      await expect(manager.reconnect()).rejects.toThrow('Network error');
      expect(manager.state).toBe('disconnected');
    });

    it('handles browser online and offline events when enabled', () => {
      const manager = new ConnectionManager({
        initialState: 'connected',
        autoDetectBrowserStatus: true,
      });

      window.dispatchEvent(new Event('offline'));
      expect(manager.state).toBe('disconnected');

      window.dispatchEvent(new Event('online'));
      expect(manager.state).toBe('connected');

      manager.destroy();
    });
  });

    // Requirement: オフライン動作中に送信したメッセージをqueueに蓄積、再接続できたあとにバルク送信
    // Issue #109: the queue is now the durable IndexedDB outbox (not RoomSocket's
    // in-memory `messageQueue`), because RoomSocket would resend its own copy on
    // reconnect and deliver every offline message twice.
    it('accumulates chat messages in the durable outbox while offline and flushes them upon reconnection', async () => {
      resetOutbox();
      const connectionManager = createConnectionManager({ initialState: 'connected' });
      const socket = new RoomSocket({
        roomId: 'room-offline-test',
        connectionManager,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      socket.connect();
      const mockWs1 = MockWebSocket.instances[0];
      mockWs1.open();
      mockWs1.sentMessages = [];

      const instance = mount(App, {
        target,
        props: {
          initialRoom: dummyRoom,
          connectionManager,
          roomSocket: socket,
        },
      });

      await new Promise((r) => setTimeout(r, 30));

      // Simulate network disconnection
      connectionManager.setState('disconnected');
      mockWs1.close(1006, 'Connection lost');
      await new Promise((r) => setTimeout(r, 20));

      // Send 3 chat messages while offline
      const chatTextarea = target.querySelector<HTMLTextAreaElement>('textarea');
      const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
      expect(chatTextarea).toBeTruthy();
      expect(sendBtn).toBeTruthy();

      if (chatTextarea && sendBtn) {
        // Message 1
        chatTextarea.value = 'オフラインメッセージ1';
        chatTextarea.dispatchEvent(new Event('input', { bubbles: true }));
        sendBtn.click();
        await new Promise((r) => setTimeout(r, 10));

        // Message 2
        chatTextarea.value = 'オフラインメッセージ2';
        chatTextarea.dispatchEvent(new Event('input', { bubbles: true }));
        sendBtn.click();
        await new Promise((r) => setTimeout(r, 10));

        // Message 3
        chatTextarea.value = 'オフラインメッセージ3';
        chatTextarea.dispatchEvent(new Event('input', { bubbles: true }));
        sendBtn.click();
        await new Promise((r) => setTimeout(r, 10));
      }

      // RoomSocket's in-memory queue stays untouched: the messages live in the
      // durable outbox instead (queued directly, never duplicated).
      expect(socket.getPendingMessageCount()).toBe(0);
      expect(socket.getMessageQueue()).toHaveLength(0);
      const queued = await getOutbox().list('room-offline-test', 'message');
      expect(queued).toHaveLength(3);
      expect(
        queued.map((entry) => (entry.payload as { record: { content: string } }).record.content),
      ).toEqual(['オフラインメッセージ1', 'オフラインメッセージ2', 'オフラインメッセージ3']);
      // The wavy queue notice reports the pending count.
      await new Promise((r) => setTimeout(r, 20));
      expect(
        target.querySelector('[data-testid="offline-queue-notice-count"]')?.textContent?.trim(),
      ).toBe('3 件を蓄積中');

      // Now trigger reconnection
      const reconnectPromise = connectionManager.reconnect();
      await new Promise((r) => setTimeout(r, 20));

      const mockWs2 = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      mockWs2.open();
      await reconnectPromise;
      await new Promise((r) => setTimeout(r, 30));

      // The outbox is flushed sequentially and each entry completes only once its
      // `LogEventAppended` echo comes back, so answer the frames as they arrive.
      let answered = 0;
      const startedAt = Date.now();
      while (answered < 3 && Date.now() - startedAt < 3000) {
        const appendFrames = mockWs2.sentMessages
          .map((m) => JSON.parse(m))
          .filter((f) => typeof f === 'object' && f !== null && 'AppendLog' in f);
        if (appendFrames.length > answered) {
          const frame = appendFrames[answered];
          mockWs2.receive({
            LogEventAppended: {
              scene_id: frame.AppendLog.scene_id,
              chunk_index: 0,
              position: answered,
              entry: frame.AppendLog.entry,
            },
          });
          answered += 1;
        }
        await new Promise((r) => setTimeout(r, 10));
      }

      // Verify bulk transmission of all 3 queued messages to the newly connected WebSocket
      const sentFrames = mockWs2.sentMessages.map((m) => JSON.parse(m));
      const appendFrames = sentFrames.filter((f) => typeof f === 'object' && f !== null && 'AppendLog' in f);
      expect(appendFrames.length).toBeGreaterThanOrEqual(3);

      // Every entry was acknowledged and removed from the outbox.
      expect(answered).toBe(3);
      expect(await getOutbox().list('room-offline-test', 'message')).toHaveLength(0);
      expect(socket.getPendingMessageCount()).toBe(0);
      await new Promise((r) => setTimeout(r, 20));
      expect(target.querySelector('[data-testid="offline-queue-notice"]')).toBeNull();
      // The echoed messages are now rendered in the log tab.
      expect(target.querySelector('[data-testid="messages-list"]')?.textContent).toContain(
        'オフラインメッセージ3',
      );

      unmount(instance);
      resetOutbox();
    });

    // Requirement: オフライン動作中のStateの変更は最終状態が変化したものを再接続後に送信
    it('deduplicates offline state updates and sends only the final state upon reconnection', async () => {
      const connectionManager = createConnectionManager({ initialState: 'connected' });
      const socket = new RoomSocket({
        roomId: 'room-offline-test',
        connectionManager,
        webSocketFactory: (u) => new MockWebSocket(u) as unknown as WebSocket,
      });

      socket.connect();
      const mockWs1 = MockWebSocket.instances[0];
      mockWs1.open();

      const instance = mount(App, {
        target,
        props: {
          initialRoom: dummyRoom,
          initialCharacters: [
            { id: '1', name: 'Hero', placedAsPiece: true, piecePosition: { x: 50, y: 50 } },
          ],
          connectionManager,
          roomSocket: socket,
        },
      });

      await new Promise((r) => setTimeout(r, 30));

      // Disconnect
      connectionManager.setState('disconnected');
      mockWs1.close(1006, 'Connection lost');
      await new Promise((r) => setTimeout(r, 20));

      // Perform multiple state modifications for the same key while offline
      // Move piece 1 through positions (100, 100) -> (150, 150) -> (250, 300)
      socket.updateState('/', 'piece::1::pos', { x: 100, y: 100 });
      socket.updateState('/', 'piece::1::pos', { x: 150, y: 150 });
      socket.updateState('/', 'piece::1::pos', { x: 250, y: 300 });

      // BGM updated to bgm-a then bgm-final
      socket.updateState('/', 'std::bgm', 'bgm-a');
      socket.updateState('/', 'std::bgm', 'bgm-final');

      // Pending state map should only hold the deduplicated final states
      const pendingStates = socket.getPendingStateChanges();
      expect(pendingStates.size).toBe(2);

      // Reconnect
      const reconnectPromise = connectionManager.reconnect();
      await new Promise((r) => setTimeout(r, 20));

      const mockWs2 = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      mockWs2.open();
      await reconnectPromise;
      await new Promise((r) => setTimeout(r, 30));

      // Verify transmitted frames on the new WebSocket connection
      const sentFrames = mockWs2.sentMessages.map((m) => JSON.parse(m));
      const stateUpdates = sentFrames.filter((f) => typeof f === 'object' && f !== null && 'UpdateState' in f);

      // Exactly ONE update for piece::1::pos with final coordinates (250, 300)
      const piecePosUpdates = stateUpdates.filter((f) => f.UpdateState.key === 'piece::1::pos');
      expect(piecePosUpdates).toHaveLength(1);
      expect(piecePosUpdates[0].UpdateState.value).toEqual({ x: 250, y: 300 });

      // Exactly ONE update for std::bgm
      const bgmUpdates = stateUpdates.filter((f) => f.UpdateState.key === 'std::bgm');
      expect(bgmUpdates).toHaveLength(1);

      // Pending states cleared after flush
      expect(socket.getPendingStateChanges().size).toBe(0);

      unmount(instance);
    });
});
