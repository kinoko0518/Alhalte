/**
 * Issue #162: lazy older chunks, server-authoritative log cache and the scene
 * graph rebuilt from came/gone system events.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 最新 chunk の読み込み`,
 *   `### ローカル保存`, `### 発言とログ表示`
 * - GitHub Issue #162
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import LogTab from '../src/components/Control/tabs/LogTab.svelte';
import {
  ChunkLogManager,
  deserializeRoomLogs,
  serializeRoomLogs,
} from '../src/utils/log.js';
import {
  buildMovementsFromLogRecords,
  reconcileMovements,
  type SceneMovementEvent,
} from '../src/utils/sceneGraph.js';
import { createConnectionManager } from '../src/utils/connection.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { LogRecord, Scene, VisitedRoom } from '../src/types/index.js';

const ROOM: VisitedRoom = {
  id: 'room-log-chunks-test',
  name: 'ログチャンクテスト部屋',
  url: 'https://alhalte.example/rooms/room-log-chunks-test#key',
  lastVisited: Date.now(),
  isAdmin: true,
};

const MAIN_SCENE: Scene = {
  id: '/',
  name: 'メインシーン',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: [],
};

const SCENE_B: Scene = {
  id: 'scene-b',
  name: 'シーンB',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: [],
};

const SCENE_A: Scene = {
  id: 'scene-a',
  name: 'シーンA',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: [],
};

async function tick(ms = 20): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

function record(overrides: Partial<LogRecord> & { id: string }): LogRecord {
  return {
    sceneId: '/',
    content: '本文',
    speaker: 'ルシア',
    timestamp: 1000,
    kind: 'chat',
    ...overrides,
  };
}

function cacheKey(): string {
  return `alhalte:room-logs:${ROOM.id}`;
}

describe('Issue #162 log chunks and cache', () => {
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

  function mountApp(
    socket: FakeRoomSocket,
    initialState: 'connected' | 'disconnected' = 'connected',
  ) {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });
    const connectionManager = createConnectionManager({ initialState });
    const instance = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [MAIN_SCENE, SCENE_B],
        initialCharacters: [
          {
            id: '1',
            name: 'ルシア',
            initiative: 0,
            description: '',
            statuses: [],
            parameters: [],
            chatPalette: '',
            hideStatus: false,
          },
        ],
        isAdmin: true,
        connectionManager,
        roomSocket: asRoomSocket(socket),
      },
    });
    return { instance, connectionManager };
  }

  function messagesText(): string {
    return target.querySelector('[data-testid="messages-list"]')?.textContent ?? '';
  }

  function mountTab(socket: FakeRoomSocket, scene: Scene, chunkManager = new ChunkLogManager()) {
    socket.setChunkLogManager(chunkManager);
    const instance = mount(LogTab, {
      target,
      props: { scene, chunkManager, roomSocket: asRoomSocket(socket), hideHeader: true },
    });
    return { instance, chunkManager };
  }

  // spec: alhalte-argenterie/log-tab 2 chunk のログを上端スクロールで一度だけ遡る
  it('loads the previous chunk once when the latest chunk is scrolled to the top', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    const { instance } = mountTab(socket, SCENE_A);

    await waitFor(() => socket.queryLogBiblioCalls.length === 1);
    socket.resolveLogBiblio(
      { chunks: [new Uint8Array(32), new Uint8Array(32)], head: [1, 1] },
      'scene-a',
    );
    await waitFor(() =>
      socket.queryLogChunkCalls.some((call) => call.sceneId === 'scene-a' && call.chunkIndex === 1),
    );

    socket.resolveLogChunk(
      [record({ id: 'msg-latest', sceneId: 'scene-a', content: '最新チャンク', timestamp: 2000 })],
      'scene-a',
      1,
    );
    await waitFor(() => messagesText().includes('最新チャンク'));

    const list = target.querySelector<HTMLElement>('[data-testid="messages-list"]')!;
    list.scrollTop = 0;
    list.dispatchEvent(new Event('scroll'));

    await waitFor(() =>
      socket.queryLogChunkCalls.some((call) => call.sceneId === 'scene-a' && call.chunkIndex === 0),
    );
    expect(socket.queryLogChunkCalls.filter((call) => call.chunkIndex === 0)).toHaveLength(1);

    socket.resolveLogChunk(
      [record({ id: 'msg-older', sceneId: 'scene-a', content: '過去チャンク', timestamp: 1000 })],
      'scene-a',
      0,
    );
    await waitFor(() => messagesText().includes('過去チャンク'));

    // Older chunk is prepended in log order.
    expect(messagesText().indexOf('過去チャンク')).toBeLessThan(
      messagesText().indexOf('最新チャンク'),
    );

    // Repeated top-scroll events at the oldest chunk never issue another query.
    for (let i = 0; i < 3; i++) {
      list.scrollTop = 0;
      list.dispatchEvent(new Event('scroll'));
      await tick(20);
    }
    expect(socket.queryLogChunkCalls.filter((call) => call.chunkIndex === 0)).toHaveLength(1);
    expect(socket.queryLogChunkCalls).toHaveLength(2);

    unmount(instance);
  });

  // spec: alhalte-argenterie/log-tab キャッシュとサーバーの同一レコードを二重表示しない
  it('dedupes a live record against the authoritative server snapshot', () => {
    const manager = new ChunkLogManager();
    manager.appendRecord(
      record({
        id: 'live-duplicate',
        sceneId: 'scene-a',
        chunkIndex: 0,
        position: 0,
        content: 'ライブ側',
      }),
    );

    // The server entry occupies the same (chunkIndex, position) but has its own
    // positional id. It must replace the live row instead of adding a second.
    manager.ingestChunkSnapshot('scene-a', 0, [
      record({
        id: 'server-duplicate',
        sceneId: 'scene-a',
        chunkIndex: 0,
        position: 0,
        content: 'サーバー側',
      }),
    ]);

    const records = manager.getLoadedRecords('scene-a');
    expect(records).toHaveLength(1);
    expect(records[0].id).toBe('server-duplicate');
    expect(records[0].content).toBe('サーバー側');
  });

  // spec: alhalte-argenterie/ローカル保存 サーバー削除をキャッシュから復活させない
  it('drops cached records when the server biblio reports an empty log', () => {
    const manager = new ChunkLogManager();
    manager.appendCachedRecord(
      record({ id: 'deleted-record', sceneId: 'scene-a', content: '削除済み' }),
    );

    manager.applyServerBiblio('scene-a', { chunks: [], head: null });
    expect(manager.getLoadedRecords('scene-a')).toHaveLength(0);

    // A late cache restore after the authoritative answer is ignored too.
    manager.appendCachedRecord(
      record({ id: 'late-cache', sceneId: 'scene-a', content: '遅れて復元' }),
    );
    expect(manager.getLoadedRecords('scene-a')).toHaveLength(0);
  });

  // spec: alhalte-argenterie/ローカル保存 system レコードを kind/event/contentTin ごと永続化する
  it('round-trips system events and contentTin through the offline cache', () => {
    const system = record({
      id: 'sys-came',
      kind: 'system',
      content: '',
      speaker: 'System',
      event: { type: 'character-came', characterId: '1' },
    });
    const chat = record({
      id: 'chat-tin',
      contentTin: [1, 2, 3],
      speakerId: '1',
    });

    const restored = deserializeRoomLogs(serializeRoomLogs([system, chat]));

    expect(restored).toHaveLength(2);
    expect(restored[0].kind).toBe('system');
    expect(restored[0].event).toEqual({ type: 'character-came', characterId: '1' });
    expect(restored[1].contentTin).toEqual([1, 2, 3]);
    expect(restored[1].speakerId).toBe('1');
    expect(restored.every((entry) => entry.cached === true)).toBe(true);
  });

  // spec: alhalte-argenterie/ローカル保存 オフラインでキャッシュ表示中を明示する
  it('shows キャッシュ表示中 only while offline and displaying cached records', async () => {
    localStorage.setItem(
      cacheKey(),
      JSON.stringify([record({ id: 'cached-1', sceneId: '/', content: 'キャッシュのログ' })]),
    );
    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    socket.autoAnswerLogQueries = false;
    const { instance } = mountApp(socket, 'disconnected');

    await waitFor(() => messagesText().includes('キャッシュのログ'));
    expect(messagesText()).toContain('キャッシュのログ');
    const indicator = target.querySelector<HTMLElement>('[data-testid="log-cache-indicator"]');
    expect(indicator?.textContent?.trim()).toBe('キャッシュ表示中');

    // The server answer replaces the cache: the indicator disappears because
    // there is no cached row left to display.
    socket.resolveLogBiblio({ chunks: [], head: null }, '/');
    await waitFor(
      () => target.querySelector('[data-testid="log-cache-indicator"]') === null &&
        !messagesText().includes('キャッシュのログ'),
    );
    expect(messagesText()).not.toContain('キャッシュのログ');
    expect(target.querySelector('[data-testid="log-cache-indicator"]')).toBeNull();

    unmount(instance);
  });

  // spec: alhalte-argenterie/ローカル保存 サーバー応答後はキャッシュのみのログを残さない
  it('does not resurrect a locally cached log after the server answers with an empty biblio', async () => {
    localStorage.setItem(
      cacheKey(),
      JSON.stringify([record({ id: 'server-deleted', sceneId: '/', content: 'サーバーで削除済み' })]),
    );
    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    const { instance } = mountApp(socket, 'connected');

    await waitFor(() => messagesText().includes('サーバーで削除済み'));
    expect(localStorage.getItem(cacheKey())).toContain('サーバーで削除済み');

    socket.resolveLogBiblio({ chunks: [], head: null }, '/');
    await waitFor(() => !messagesText().includes('サーバーで削除済み'));
    await waitFor(() => !(localStorage.getItem(cacheKey()) ?? '').includes('サーバーで削除済み'));

    expect(messagesText()).not.toContain('サーバーで削除済み');
    expect(localStorage.getItem(cacheKey()) ?? '').not.toContain('サーバーで削除済み');

    unmount(instance);
  });

  // spec: alhalte-argenterie/scene-graph came/gone から移動履歴を再構築する
  it('rebuilds movements from character-came / character-gone log records', () => {
    const records: LogRecord[] = [
      record({
        id: 'came-main',
        sceneId: '/',
        chunkIndex: 0,
        position: 0,
        kind: 'system',
        content: '',
        speaker: 'System',
        event: { type: 'character-came', characterId: '1' },
      }),
      record({
        id: 'gone-main',
        sceneId: '/',
        chunkIndex: 0,
        position: 1,
        kind: 'system',
        content: '',
        speaker: 'System',
        event: { type: 'character-gone', characterId: '1' },
      }),
      record({
        id: 'came-b',
        sceneId: 'scene-b',
        chunkIndex: 1,
        position: 0,
        kind: 'system',
        content: '',
        speaker: 'System',
        // Issue #227: came/gone carry the append time on the wire.
        timestamp: 2000,
        event: { type: 'character-came', characterId: '1' },
      }),
    ];

    const movements = buildMovementsFromLogRecords(
      records,
      new Map([['1', 'ルシア']]),
    );

    expect(movements).toHaveLength(2);
    expect(movements[0]).toMatchObject({
      characterId: '1',
      characterName: 'ルシア',
      toSceneId: '/',
      source: 'log',
    });
    expect(movements[0].fromSceneId).toBeUndefined();
    expect(movements[0].timestamp).toBe(1000);
    expect(movements[1]).toMatchObject({
      characterId: '1',
      fromSceneId: '/',
      toSceneId: 'scene-b',
      source: 'log',
    });
    // The append time decoded from the came event is kept on the movement.
    expect(movements[1].timestamp).toBe(2000);

    // A live CharacterMoved for the same route is not counted twice.
    const live: SceneMovementEvent[] = [
      {
        characterId: '1',
        characterName: 'ルシア',
        fromSceneId: '/',
        toSceneId: 'scene-b',
        timestamp: Math.floor(Date.now() / 1000),
        source: 'live',
      },
    ];
    const reconciled = reconcileMovements(live, movements);
    expect(reconciled).toHaveLength(2);
    expect(reconciled.filter((movement) => movement.source === 'live')).toHaveLength(0);
  });

  // spec: alhalte-argenterie/scene-graph 再入室後も came/gone から graph を復元する
  it('restores the scene graph from cached came/gone records after a reload', async () => {
    localStorage.setItem(
      cacheKey(),
      JSON.stringify([
        record({
          id: 'cached-came',
          sceneId: '/',
          chunkIndex: 0,
          position: 0,
          kind: 'system',
          content: '',
          speaker: 'System',
          event: { type: 'character-came', characterId: '1' },
        }),
        record({
          id: 'cached-gone',
          sceneId: '/',
          chunkIndex: 0,
          position: 1,
          kind: 'system',
          content: '',
          speaker: 'System',
          event: { type: 'character-gone', characterId: '1' },
        }),
        record({
          id: 'cached-came-b',
          sceneId: 'scene-b',
          chunkIndex: 1,
          position: 0,
          kind: 'system',
          content: '',
          speaker: 'System',
          event: { type: 'character-came', characterId: '1' },
        }),
      ]),
    );

    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    socket.autoAnswerLogQueries = false;
    const { instance } = mountApp(socket, 'disconnected');

    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-graph"]')!.click();
    await tick(40);

    const graph = target.querySelector('[data-testid="scene-graph-tab"]');
    expect(graph).toBeTruthy();
    expect(graph?.textContent).toContain('メインシーン');
    expect(graph?.textContent).toContain('シーンB');

    const rowsBefore = target.querySelectorAll('[data-testid^="graph-row-"]').length;
    expect(rowsBefore).toBeGreaterThanOrEqual(2);

    // Live CharacterMoved for the already-recorded move: the graph must not
    // grow (no double count).
    socket.emitFrame({ CharacterMoved: { character_id: 1, to_scene_id: 'scene-b' } });
    await tick(40);
    expect(target.querySelectorAll('[data-testid^="graph-row-"]').length).toBe(rowsBefore);

    unmount(instance);
  });
});
