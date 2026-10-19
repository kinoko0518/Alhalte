/**
 * Integration tests for the send path, the durable offline outbox and the
 * wavy `OfflineQueueNotice` shown above the common footer.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### ローカル保存`
 * - GitHub Issue #109 / sub-issue #139: `送信中インジケーターとオフラインキュー`
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import { createConnectionManager } from '../src/utils/connection.js';
import { getOutbox, resetOutbox } from '../src/utils/outbox.js';
import { OperationTimeoutError } from '../src/utils/roomSocket.js';
import type { LogRecord, VisitedRoom } from '../src/types/index.js';

const ROOM: VisitedRoom = {
  id: 'room-queue-notice-test',
  name: 'キュー通知テスト部屋',
  url: 'https://alhalte.example/rooms/room-queue-notice-test#key',
  lastVisited: Date.now(),
  isAdmin: true,
};

async function tick(ms = 20): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

/** Polls `predicate` until it holds (keeps wasm/log rendering deterministic). */
async function waitFor(predicate: () => boolean, timeoutMs = 1500): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

describe('Send path, offline outbox and queue notice (Issue #139)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    document.body.innerHTML = '';
    localStorage.clear();
    resetOutbox();
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    document.body.innerHTML = '';
    resetOutbox();
  });

  function mountApp(socket: FakeRoomSocket, initialState: 'connected' | 'disconnected' = 'connected') {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const connectionManager = createConnectionManager({ initialState });
    const instance = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        isAdmin: true,
        connectionManager,
        roomSocket: asRoomSocket(socket),
      },
    });
    return { instance, connectionManager };
  }

  function sendButton(): HTMLButtonElement {
    const button = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
    expect(button).toBeTruthy();
    return button!;
  }

  async function typeAndSend(text: string): Promise<void> {
    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea).toBeTruthy();
    textarea!.value = text;
    textarea!.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    sendButton().click();
    await tick();
  }

  function footer(): HTMLElement {
    const element = target.querySelector<HTMLElement>('[data-testid="common-footer"]');
    expect(element).toBeTruthy();
    return element!;
  }

  function noticeDescription(): string | null | undefined {
    return target
      .querySelector('[data-testid="offline-queue-notice-description"]')
      ?.textContent?.trim();
  }

  function noticeCount(): string | null | undefined {
    return target.querySelector('[data-testid="offline-queue-notice-count"]')?.textContent?.trim();
  }

  function messagesText(): string {
    return target.querySelector('[data-testid="messages-list"]')?.textContent ?? '';
  }

  // spec: alhalte-argenterie/発言とログ表示 タイムアウトしたメッセージはログ内のキュー区切りに描画する (Issue #148)
  it('renders a timed-out message inline in the log queue instead of above the footer', async () => {
    const socket = new FakeRoomSocket();
    const { instance } = mountApp(socket);
    await tick(30);

    await typeAndSend('タイムアウトするメッセージ');

    // In flight: the footer reports busy and the message is not rendered yet.
    expect(socket.requestAppendLogCalls).toHaveLength(1);
    expect(socket.requestAppendLogCalls[0].sceneId).toBe('/');
    expect(socket.requestAppendLogCalls[0].record.content).toBe('タイムアウトするメッセージ');
    expect(footer().getAttribute('aria-busy')).toBe('true');
    expect(target.querySelector('[data-testid="send-progress-indicator"]')).toBeTruthy();
    expect(messagesText()).not.toContain('タイムアウトするメッセージ');

    // The operation times out: the message is persisted in the outbox instead.
    socket.rejectPending(new OperationTimeoutError('appendLog', 8000));
    await tick(40);

    const queued = await getOutbox().list(ROOM.id!, 'message');
    expect(queued).toHaveLength(1);
    const payload = queued[0].payload as { sceneId: string; record: LogRecord };
    expect(payload.sceneId).toBe('/');
    expect(payload.record.content).toBe('タイムアウトするメッセージ');

    // The queue is rendered inline, below the wavy divider, not above the
    // footer (Issue #148).
    const queuedMessage = target.querySelector<HTMLElement>(
      `[data-testid="log-record-${payload.record.id}"]`,
    );
    expect(queuedMessage).toBeTruthy();
    expect(queuedMessage?.getAttribute('data-queued')).toBe('true');
    expect(messagesText()).toContain('タイムアウトするメッセージ');
    expect(
      target.querySelector('[data-testid="messages-list"] [data-testid="queued-log-divider"]'),
    ).toBeTruthy();

    const notice = target.querySelector<HTMLElement>('[data-testid="offline-queue-notice"]');
    expect(notice).toBeTruthy();
    expect(notice?.getAttribute('data-kind')).toBe('message');
    expect(notice?.getAttribute('role')).toBe('status');
    expect(notice?.getAttribute('aria-live')).toBe('polite');
    expect(noticeDescription()).toBe('メッセージは次回の接続時に送信されます');
    expect(noticeCount()).toBe('1 件を蓄積中');
    expect(notice?.querySelectorAll('.wavy-divider').length).toBe(2);
    // The duplicated footer notice was removed (#148): connection state stays in
    // `OfflineIndicator`, the queue count now lives in the log.
    expect(footer().querySelector('[data-testid="offline-queue-notice"]')).toBeNull();
    expect(target.querySelector('[data-testid="footer-queue-notice"]')).toBeNull();

    // The failed message is NOT restored into the input.
    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea?.value).toBe('');

    // The sending state is cleared so the next message can be sent.
    expect(footer().getAttribute('aria-busy')).toBe('false');
    expect(target.querySelector('[data-testid="send-progress-indicator"]')).toBeNull();

    // No second local append, no fire-and-forget duplicate on the socket.
    expect(socket.appendLogCalls).toHaveLength(0);

    unmount(instance);
  });

  // spec: alhalte-argenterie/発言とログ表示 サーバー echo の確認後に一度だけ描画する
  it('renders the message exactly once after the server echo and never appends it locally', async () => {
    const socket = new FakeRoomSocket();
    const { instance } = mountApp(socket);
    await tick(30);

    await typeAndSend('echo を待つメッセージ');

    expect(messagesText()).not.toContain('echo を待つメッセージ');
    expect(footer().getAttribute('aria-busy')).toBe('true');

    // Double send is blocked while the echo is pending.
    sendButton().click();
    await tick();
    expect(socket.requestAppendLogCalls).toHaveLength(1);

    socket.deliverEcho();
    await waitFor(() => messagesText().includes('echo を待つメッセージ'));

    expect(messagesText()).toContain('echo を待つメッセージ');
    // Only the echo produced the record (no local append, so exactly one match).
    expect(target.querySelectorAll('[data-testid^="log-record-"]')).toHaveLength(1);
    expect(footer().getAttribute('aria-busy')).toBe('false');
    expect(target.querySelector('[data-testid="offline-queue-notice"]')).toBeNull();

    // The echoed record was persisted for the room.
    const stored = localStorage.getItem(`alhalte:room-logs:${ROOM.id}`);
    expect(stored).toContain('echo を待つメッセージ');

    unmount(instance);
  });

  // spec: alhalte-argenterie/発言とログ表示 オフライン時は IndexedDB アウトボックスへ直接積む
  it('queues directly in the outbox while offline without touching the RoomSocket queue', async () => {
    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    const { instance } = mountApp(socket, 'disconnected');
    await tick(30);

    await typeAndSend('オフラインで送るメッセージ');
    await tick(40);

    // The disconnected socket is never asked to append: RoomSocket would keep
    // its own copy in `messageQueue` and resend it on reconnect (double send).
    expect(socket.requestAppendLogCalls).toHaveLength(0);
    expect(socket.appendLogCalls).toHaveLength(0);
    expect(socket.pendingCount).toBe(0);

    const queued = await getOutbox().list(ROOM.id!, 'message');
    expect(queued).toHaveLength(1);
    expect((queued[0].payload as { record: LogRecord }).record.content).toBe(
      'オフラインで送るメッセージ',
    );
    expect(noticeCount()).toBe('1 件を蓄積中');
    // The queued message is visible inline even before a reconnect.
    expect(messagesText()).toContain('オフラインで送るメッセージ');
    expect(target.querySelectorAll('[data-queued="true"]')).toHaveLength(1);

    // A second offline message is appended FIFO to the same queue.
    await typeAndSend('オフラインで送るメッセージ2');
    await tick(40);
    const queued2 = await getOutbox().list(ROOM.id!, 'message');
    expect(queued2).toHaveLength(2);
    expect((queued2[1].payload as { record: LogRecord }).record.content).toBe(
      'オフラインで送るメッセージ2',
    );
    expect(noticeCount()).toBe('2 件を蓄積中');
    expect(target.querySelectorAll('[data-queued="true"]')).toHaveLength(2);
    expect(messagesText()).toContain('オフラインで送るメッセージ2');
    expect(socket.requestAppendLogCalls).toHaveLength(0);
    expect(socket.appendLogCalls).toHaveLength(0);

    unmount(instance);
  });

  // spec: alhalte-argenterie/発言とログ表示 再接続時にアウトボックスを FIFO で flush し echo 後に描画する
  it('flushes the outbox in FIFO order on reconnect and renders each message after its echo', async () => {
    // Two messages left over from a previous offline session.
    await getOutbox().enqueue({
      roomId: ROOM.id!,
      kind: 'message',
      createdAt: Date.now() - 2000,
      payload: {
        sceneId: '/',
        record: {
          id: 'msg-queued-1',
          sceneId: '/',
          content: 'キューの1通目',
          speaker: 'アルハルテ太郎',
          timestamp: 1000,
        } satisfies LogRecord,
      },
    });
    await getOutbox().enqueue({
      roomId: ROOM.id!,
      kind: 'message',
      createdAt: Date.now() - 1000,
      payload: {
        sceneId: '/',
        record: {
          id: 'msg-queued-2',
          sceneId: '/',
          content: 'キューの2通目',
          speaker: 'アルハルテ太郎',
          timestamp: 1001,
        } satisfies LogRecord,
      },
    });

    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    const { instance, connectionManager } = mountApp(socket, 'disconnected');
    await tick(30);

    // The restored entries are visible inline even before reconnecting.
    await waitFor(() => noticeCount() === '2 件を蓄積中');
    expect(noticeCount()).toBe('2 件を蓄積中');
    expect(messagesText()).toContain('キューの1通目');
    expect(messagesText()).toContain('キューの2通目');
    expect(target.querySelectorAll('[data-queued="true"]')).toHaveLength(2);

    // Reconnect: the outbox is flushed, one entry at a time.
    socket.isConnected = true;
    connectionManager.setState('connected');
    await waitFor(() => socket.requestAppendLogCalls.length > 0);
    await tick(20);

    expect(socket.requestAppendLogCalls).toHaveLength(1);
    expect(socket.requestAppendLogCalls[0].record.content).toBe('キューの1通目');
    expect(socket.appendLogCalls).toHaveLength(0);

    // The first entry is still shown as queued until its echo arrives.
    expect(messagesText()).toContain('キューの1通目');
    expect(target.querySelectorAll('[data-testid="log-record-msg-queued-1"]')).toHaveLength(1);

    socket.deliverEcho();
    await waitFor(() => target.querySelector('[data-testid="log-record-msg-queued-1"][data-queued]') === null);
    expect(messagesText()).toContain('キューの1通目');
    // The echo replaced the queued copy: exactly one row, never two.
    expect(target.querySelectorAll('[data-testid="log-record-msg-queued-1"]')).toHaveLength(1);
    expect(messagesText()).toContain('キューの2通目');
    expect(target.querySelectorAll('[data-queued="true"]')).toHaveLength(1);

    // The second entry follows only after the first one settled (FIFO).
    await waitFor(() => socket.requestAppendLogCalls.length > 1);
    expect(socket.requestAppendLogCalls).toHaveLength(2);
    expect(socket.requestAppendLogCalls[1].record.content).toBe('キューの2通目');

    socket.deliverEcho(1);
    await waitFor(
      () => target.querySelector('[data-testid="log-record-msg-queued-2"][data-queued]') === null,
    );
    expect(messagesText()).toContain('キューの2通目');
    expect(target.querySelectorAll('[data-testid="log-record-msg-queued-2"]')).toHaveLength(1);

    // The queue is drained and the inline notice disappears.
    await waitFor(
      () => target.querySelector('[data-testid="queued-log-divider"]') === null,
    );
    expect(await getOutbox().list(ROOM.id!, 'message')).toHaveLength(0);
    expect(target.querySelector('[data-testid="offline-queue-notice"]')).toBeNull();
    expect(target.querySelectorAll('[data-queued="true"]')).toHaveLength(0);

    unmount(instance);
  });

  // spec: alhalte-argenterie/発言とログ表示 flush 中に切断されたエントリはキューに残す
  it('keeps an entry queued when the flush fails because the connection dropped', async () => {
    await getOutbox().enqueue({
      roomId: ROOM.id!,
      kind: 'message',
      payload: {
        sceneId: '/',
        record: {
          id: 'msg-queued-retry',
          sceneId: '/',
          content: '再送されるメッセージ',
          speaker: 'アルハルテ太郎',
          timestamp: 1002,
        } satisfies LogRecord,
      },
    });

    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    const { instance, connectionManager } = mountApp(socket, 'disconnected');
    await tick(30);

    socket.isConnected = true;
    connectionManager.setState('connected');
    await waitFor(() => socket.requestAppendLogCalls.length > 0);

    // The socket drops before the echo: the entry must survive.
    socket.isConnected = false;
    socket.rejectPending(new OperationTimeoutError('appendLog', 8000));
    await tick(60);

    expect(await getOutbox().list(ROOM.id!, 'message')).toHaveLength(1);
    expect(noticeCount()).toBe('1 件を蓄積中');
    // The entry stayed queued, so its inline preview stays visible.
    expect(messagesText()).toContain('再送されるメッセージ');
    expect(target.querySelectorAll('[data-queued="true"]')).toHaveLength(1);

    unmount(instance);
  });
});
