/**
 * Eager latest-chunk loading for the log tab (Issue #109 / sub-issue #140).
 *
 * Covers the `QueryLogBiblio` → `QueryLogChunk` sequence on mount / scene
 * switch, the contained `LoadingIndicator`, the hidden empty state while the
 * request is in flight, the queueing state on timeout, the reconnect retry and
 * the per-scene request dedupe.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`
 * - ARCHITECTURE.md: `## alhalte-banquette -> #### log`
 * - GitHub Issue #109 / sub-issue #140
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import LogTab from '../src/components/Control/tabs/LogTab.svelte';
import App from '../src/components/App.svelte';
import { ChunkLogManager, readLogBiblioHead } from '../src/utils/log.js';
import { createConnectionManager } from '../src/utils/connection.js';
import { OperationTimeoutError } from '../src/utils/roomSocket.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { LogRecord, Scene, VisitedRoom } from '../src/types/index.js';

const SCENE_A: Scene = {
  id: 'scene-a',
  name: 'シーンA',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: [],
};

const MAIN_SCENE: Scene = {
  id: '/',
  name: 'メインシーン',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: [],
};

const SCENE_B: Scene = {
  id: '/scene-b',
  name: 'シーンB',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: [],
};

const ROOM: VisitedRoom = {
  id: 'room-log-loading-test',
  name: 'ログ読み込みテスト部屋',
  url: 'https://alhalte.example/rooms/room-log-loading-test#key',
  lastVisited: Date.now(),
  isAdmin: true,
};

async function tick(ms = 20): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

/** Polls `predicate` until it holds (keeps async query/render ordering deterministic). */
async function waitFor(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

function record(overrides: Partial<LogRecord> & { id: string }): LogRecord {
  return {
    sceneId: SCENE_A.id,
    content: '本文',
    speaker: 'ルシア',
    timestamp: 1000,
    ...overrides,
  };
}

describe('LogTab eager latest-chunk loading (Issue #109 / #140)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    document.body.innerHTML = '';
    localStorage.clear();
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  function loadingOverlay(): HTMLElement | null {
    return target.querySelector<HTMLElement>('[data-testid="log-loading-overlay"]');
  }

  function emptyNotice(): HTMLElement | null {
    return target.querySelector<HTMLElement>('[data-testid="empty-log-notice"]');
  }

  function queueNotice(): HTMLElement | null {
    return target.querySelector<HTMLElement>('[data-testid="log-queue-notice"]');
  }

  function messagesText(): string {
    return target.querySelector('[data-testid="messages-list"]')?.textContent ?? '';
  }

  function mountTab(socket: FakeRoomSocket, scene: Scene, chunkManager = new ChunkLogManager()) {
    // App wires the socket to its chunk manager; mirror that wiring here so the
    // fake's `LogChunkSnapshot` ingestion reaches the tab under test.
    socket.setChunkLogManager(chunkManager);
    const instance = mount(LogTab, {
      target,
      props: { scene, chunkManager, roomSocket: asRoomSocket(socket) },
    });
    return { instance, chunkManager };
  }

  describe('readLogBiblioHead', () => {
    // spec: alhalte-banquette/log ビブリオの head は Rust tuple (2 要素配列) で届く
    it('accepts the tuple form and both object spellings of the biblio head', () => {
      expect(readLogBiblioHead({ chunks: [], head: [2, 5] })).toEqual({
        chunkIndex: 2,
        position: 5,
      });
      expect(readLogBiblioHead({ head: { chunk_index: 4, position: 9 } })).toEqual({
        chunkIndex: 4,
        position: 9,
      });
      expect(readLogBiblioHead({ head: { chunkIndex: 1, position: 0 } })).toEqual({
        chunkIndex: 1,
        position: 0,
      });
      expect(readLogBiblioHead({ head: ['3', '7'] })).toEqual({ chunkIndex: 3, position: 7 });
    });

    it('returns null for an empty or malformed biblio', () => {
      expect(readLogBiblioHead(null)).toBeNull();
      expect(readLogBiblioHead({ chunks: [] })).toBeNull();
      expect(readLogBiblioHead({ head: null })).toBeNull();
      expect(readLogBiblioHead({ head: [null, 0] })).toBeNull();
      expect(readLogBiblioHead('nope')).toBeNull();
    });
  });

  // spec: alhalte-argenterie/log-tab 入室時に QueryLogBiblio → QueryLogChunk を送る
  it('queries the biblio then the newest chunk on mount and renders entries after the snapshot', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    const { instance } = mountTab(socket, SCENE_A);

    await waitFor(() => socket.queryLogBiblioCalls.length === 1);
    expect(socket.queryLogBiblioCalls).toEqual([{ sceneId: 'scene-a' }]);
    expect(socket.queryLogChunkCalls).toHaveLength(0);

    // In flight: contained M3 indicator, polite status, list busy, no empty state.
    await waitFor(() => loadingOverlay() !== null);
    expect(loadingOverlay()?.getAttribute('role')).toBe('status');
    expect(
      loadingOverlay()?.querySelector('[data-testid="log-loading-indicator"]')?.classList.contains(
        'contained',
      ),
    ).toBe(true);
    expect(
      target.querySelector('[data-testid="messages-list"]')?.getAttribute('aria-busy'),
    ).toBe('true');
    expect(emptyNotice()).toBeNull();

    // The biblio head selects the chunk index to request.
    socket.resolveLogBiblio({ chunks: [new Uint8Array(32)], head: [3, 7] }, 'scene-a');
    await waitFor(() => socket.queryLogChunkCalls.length === 1);
    expect(socket.queryLogChunkCalls).toEqual([{ sceneId: 'scene-a', chunkIndex: 3 }]);

    // Still loading the chunk body: the empty state must stay hidden.
    expect(loadingOverlay()).not.toBeNull();
    expect(emptyNotice()).toBeNull();

    socket.resolveLogChunk(
      [record({ id: 'msg-latest-1', content: '最新チャンクのログ', timestamp: 1700000000 })],
      'scene-a',
      3,
    );

    await waitFor(() => loadingOverlay() === null && messagesText().includes('最新チャンクのログ'));
    expect(messagesText()).toContain('最新チャンクのログ');
    expect(emptyNotice()).toBeNull();
    expect(
      target.querySelector('[data-testid="messages-list"]')?.getAttribute('aria-busy'),
    ).toBe('false');

    unmount(instance);
  });

  // spec: alhalte-argenterie/log-tab 読込中は「ログはありません」を表示しない
  it('hides the empty state while loading and shows it once an empty biblio settles', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    const { instance } = mountTab(socket, SCENE_A);

    await waitFor(() => loadingOverlay() !== null);
    expect(emptyNotice()).toBeNull();

    socket.resolveLogBiblio({ chunks: [], head: null }, 'scene-a');
    await waitFor(() => loadingOverlay() === null);

    // No head means nothing to fetch; the log really is empty.
    expect(socket.queryLogChunkCalls).toHaveLength(0);
    expect(emptyNotice()?.textContent?.trim()).toBe('ログはありません');

    unmount(instance);
  });

  // spec: alhalte-argenterie/log-tab タイムアウト時はキューイング状態（波線通知）へ遷移する
  it('shows the generic queue notice on a biblio timeout and drops the loading state', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    const { instance } = mountTab(socket, SCENE_A);

    await waitFor(() => socket.queryLogBiblioCalls.length === 1);
    socket.rejectLogQuery(new OperationTimeoutError('queryLogBiblio', 8000), 'scene-a');

    await waitFor(() => queueNotice() !== null);
    const notice = queueNotice()?.querySelector('[data-testid="offline-queue-notice"]');
    expect(notice).toBeTruthy();
    expect(notice?.getAttribute('data-kind')).toBe('generic');
    expect(notice?.getAttribute('role')).toBe('status');
    expect(
      queueNotice()?.querySelector('[data-testid="offline-queue-notice-description"]')?.textContent,
    ).toBe('接続が回復したらログを再読み込みします');

    expect(loadingOverlay()).toBeNull();
    expect(emptyNotice()).toBeNull();
    expect(
      target.querySelector('[data-testid="messages-list"]')?.getAttribute('aria-busy'),
    ).toBe('false');

    // Nothing is retried while the connection never becomes `connected` again.
    await tick(40);
    expect(socket.queryLogBiblioCalls).toHaveLength(1);

    unmount(instance);
  });

  // spec: alhalte-argenterie/log-tab タイムアウト時も取得済みレコードは保持する
  it('keeps cached records visible behind the queue notice after a chunk timeout', async () => {
    const chunkManager = new ChunkLogManager();
    chunkManager.appendRecord(record({ id: 'msg-cached-1', content: 'キャッシュ済みログ' }));

    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    const { instance } = mountTab(socket, SCENE_A, chunkManager);

    await waitFor(() => socket.queryLogBiblioCalls.length === 1);
    expect(messagesText()).toContain('キャッシュ済みログ');

    socket.resolveLogBiblio({ chunks: [new Uint8Array(32)], head: [0, 1] }, 'scene-a');
    await waitFor(() => socket.queryLogChunkCalls.length === 1);

    socket.rejectLogQuery(new OperationTimeoutError('queryLogChunk', 8000), 'scene-a');
    await waitFor(() => queueNotice() !== null);

    expect(messagesText()).toContain('キャッシュ済みログ');
    expect(emptyNotice()).toBeNull();
    expect(loadingOverlay()).toBeNull();

    unmount(instance);
  });

  // spec: alhalte-argenterie/log-tab 同じシーンへの二重クエリを防ぐ
  it('does not re-query the same scene once it is loading or loaded', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    const { instance, chunkManager } = mountTab(socket, SCENE_A);

    await waitFor(() => socket.queryLogBiblioCalls.length === 1);
    socket.resolveLogBiblio({ chunks: [new Uint8Array(32)], head: [0, 0] }, 'scene-a');
    await waitFor(() => socket.queryLogChunkCalls.length === 1);
    socket.resolveLogChunk([record({ id: 'msg-once-1' })], 'scene-a', 0);

    await waitFor(() => loadingOverlay() === null);
    // Unrelated chunk-manager changes re-render the list but must not reload.
    chunkManager.appendRecord(record({ id: 'msg-once-2', content: '追加メッセージ' }));
    await tick(60);

    expect(socket.queryLogBiblioCalls).toHaveLength(1);
    expect(socket.queryLogChunkCalls).toHaveLength(1);
    expect(messagesText()).toContain('追加メッセージ');

    unmount(instance);
  });

  // spec: alhalte-argenterie/log-tab 再接続時にキューイング状態から自動再試行する
  it('retries the latest chunk on reconnect after a timeout (App wiring)', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    socket.isConnected = false;
    const connectionManager = createConnectionManager({ initialState: 'disconnected' });

    const instance = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        isAdmin: true,
        connectionManager,
        roomSocket: asRoomSocket(socket),
      },
    });

    // The tab attempts a load even while offline (the socket queues the frame).
    await waitFor(() => socket.queryLogBiblioCalls.length === 1);
    socket.rejectLogQuery(new OperationTimeoutError('queryLogBiblio', 8000), '/');
    await waitFor(() => queueNotice() !== null);
    expect(socket.queryLogBiblioCalls).toHaveLength(1);

    // Reconnect: the app retries once.
    socket.isConnected = true;
    connectionManager.setState('connected');
    await waitFor(() => socket.queryLogBiblioCalls.length === 2);
    expect(socket.queryLogBiblioCalls[1]).toEqual({ sceneId: '/' });
    expect(loadingOverlay()).not.toBeNull();

    socket.resolveLogBiblio({ chunks: [new Uint8Array(32)], head: [1, 4] }, '/');
    await waitFor(() => socket.queryLogChunkCalls.length === 1);
    expect(socket.queryLogChunkCalls[0]).toEqual({ sceneId: '/', chunkIndex: 1 });

    socket.resolveLogChunk(
      [record({ id: 'msg-retry-1', sceneId: '/', content: '再接続で読み込んだログ' })],
      '/',
      1,
    );
    await waitFor(() => messagesText().includes('再接続で読み込んだログ'));

    expect(queueNotice()).toBeNull();
    expect(loadingOverlay()).toBeNull();
    expect(messagesText()).toContain('再接続で読み込んだログ');

    unmount(instance);
  });

  // spec: alhalte-argenterie/log-tab シーン切り替えで新シーンの最新チャンクを読み、古い応答は無視する
  it('loads the new scene on a scene switch and ignores the stale response of the old one', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    const connectionManager = createConnectionManager({ initialState: 'connected' });

    const instance = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [MAIN_SCENE, SCENE_B],
        isAdmin: true,
        connectionManager,
        roomSocket: asRoomSocket(socket),
      },
    });

    await waitFor(() => socket.queryLogBiblioCalls.length === 1);
    expect(socket.queryLogBiblioCalls[0]).toEqual({ sceneId: '/' });

    // Switch scenes while the first biblio request is still in flight.
    const searchInput = target.querySelector<HTMLInputElement>(
      '[data-testid="scene-search-input"]',
    );
    expect(searchInput).toBeTruthy();
    searchInput!.focus();
    await tick(30);
    const sceneBItem = target.querySelector<HTMLElement>('[data-testid="scene-search-item-/scene-b"]');
    expect(sceneBItem).toBeTruthy();
    sceneBItem!.click();

    await waitFor(() => socket.queryLogBiblioCalls.some((call) => call.sceneId === '/scene-b'));
    expect(socket.queryLogBiblioCalls[1]).toEqual({ sceneId: '/scene-b' });

    // A late answer for the old scene must not trigger a chunk request.
    socket.resolveLogBiblio({ chunks: [], head: [0, 0] }, '/');
    await tick(40);
    expect(socket.queryLogChunkCalls.filter((call) => call.sceneId === '/')).toHaveLength(0);

    // The new scene loads normally.
    socket.resolveLogBiblio({ chunks: [new Uint8Array(32)], head: [2, 1] }, '/scene-b');
    await waitFor(() =>
      socket.queryLogChunkCalls.some(
        (call) => call.sceneId === '/scene-b' && call.chunkIndex === 2,
      ),
    );

    socket.resolveLogChunk(
      [
        record({
          id: 'msg-scene-b-1',
          sceneId: '/scene-b',
          content: 'シーンBのログ',
          speaker: 'ベルナルド',
          timestamp: 2000,
        }),
      ],
      '/scene-b',
      2,
    );
    await waitFor(() => messagesText().includes('シーンBのログ'));

    expect(messagesText()).toContain('シーンBのログ');
    expect(loadingOverlay()).toBeNull();
    expect(emptyNotice()).toBeNull();

    unmount(instance);
  });
});
