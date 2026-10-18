/**
 * Cross-feature integration regression tests for Issue #109 (parent) / #146.
 *
 * These tests deliberately drive several features through one mounted `App`
 * (or one `FakeRoomSocket`) instead of re-testing each sub-issue in isolation:
 *
 * - room entry → `QueryLogBiblio` / `QueryLogChunk` latest-chunk load → send
 *   that renders only after its `LogEventAppended` echo,
 * - send timeout → durable outbox → reconnect flush → echo,
 * - scene create ack applies the scene, timeout keeps the list untouched,
 * - character save timeout queues and the reconnect flush applies it,
 * - extension tab offline state dims and disables the tab even for an admin,
 * - asset upload progress / failure through a mocked cellar transfer.
 *
 * No network: `cellarTransfer` is mocked, the socket is
 * `tests/helpers/fakeRoomSocket.ts`, and IndexedDB falls back to memory.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 内部フロー`
 * - GitHub Issue #109 / sub-issue #146
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

const cellar = vi.hoisted(() => {
  class MockCellarTransferError extends Error {
    code: string;
    constructor(code: string, message: string) {
      super(message);
      this.name = 'CellarTransferError';
      this.code = code;
    }
  }
  return {
    DEFAULT_OPERATION_TIMEOUT_MS: 8000,
    CELLAR_PROXY_BASE_PATH: '/api/cellar',
    encodeRoomTin: vi.fn(),
    uploadTin: vi.fn(),
    downloadTin: vi.fn(),
    getRoomKeyFromLocation: vi.fn(() => null),
    isCellarTransferError: (error: unknown) => error instanceof MockCellarTransferError,
    CellarTransferError: MockCellarTransferError,
  };
});

vi.mock('../src/utils/cellarTransfer.js', () => cellar);

import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import { createConnectionManager, type ConnectionManager } from '../src/utils/connection.js';
import { getOutbox, resetOutbox } from '../src/utils/outbox.js';
import { OperationTimeoutError } from '../src/utils/roomSocket.js';
import { clearAssets, loadAssets } from '../src/utils/assets.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { Character, LogRecord, Scene, VisitedRoom } from '../src/types/index.js';

/** Deterministic 32-byte room key (never the URL fragment in tests). */
const ROOM_KEY = new Uint8Array(32).fill(7);
const TIN_ID = 'a'.repeat(64);

const ROOM: VisitedRoom = {
  id: 'room-integration-146',
  name: '統合テスト部屋',
  url: 'https://alhalte.example/rooms/room-integration-146#key',
  lastVisited: Date.now(),
  isAdmin: true,
};

const MAIN_SCENE: Scene = {
  id: '/',
  name: 'メインシーン',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: ['42'],
};

const SECOND_SCENE: Scene = {
  id: 'scene-second',
  name: '控えの間',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: [],
};

const HERO: Character = {
  id: '42',
  name: '勇者ロラン',
  initiative: 15,
  description: '前衛の騎士。',
  reference: '',
  statuses: [],
  parameters: [],
  chatPalette: '',
  hideStatus: false,
  isNarrative: false,
  placedAsPiece: false,
};

function tick(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Polls `predicate` until it holds; keeps async ordering independent of wall time. */
async function waitFor(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await tick(10);
  }
}

function logRecord(overrides: Partial<LogRecord> & { id: string }): LogRecord {
  return {
    sceneId: '/',
    content: '本文',
    speaker: 'ルシア',
    timestamp: 1000,
    ...overrides,
  };
}

describe('Issue #109 cross-feature integration (#146)', () => {
  let target: HTMLDivElement;
  let app: Record<string, any> | null = null;

  beforeEach(() => {
    clearAssets();
    document.body.innerHTML = '';
    localStorage.clear();
    resetOutbox();
    cellar.encodeRoomTin.mockReset();
    cellar.uploadTin.mockReset();
    cellar.downloadTin.mockReset();
    cellar.getRoomKeyFromLocation.mockReset();
    cellar.getRoomKeyFromLocation.mockReturnValue(null);
    cellar.encodeRoomTin.mockResolvedValue({
      wireBytes: new Uint8Array([1, 2, 3]),
      tinIdHex: TIN_ID,
    });
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    if (app) {
      unmount(app);
      app = null;
    }
    document.body.innerHTML = '';
    resetOutbox();
  });

  function mountApp(
    socket: FakeRoomSocket,
    initialState: 'connected' | 'disconnected' = 'connected',
  ): { connectionManager: ConnectionManager } {
    const connectionManager = createConnectionManager({ initialState });
    app = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [MAIN_SCENE, SECOND_SCENE],
        initialCharacters: [{ ...HERO }],
        isAdmin: true,
        roomKey: ROOM_KEY,
        connectionManager,
        roomSocket: asRoomSocket(socket),
      },
    }) as unknown as Record<string, any>;
    return { connectionManager };
  }

  function messagesText(): string {
    return target.querySelector('[data-testid="messages-list"]')?.textContent ?? '';
  }

  function footer(): HTMLElement {
    const element = target.querySelector<HTMLElement>('[data-testid="common-footer"]');
    expect(element).toBeTruthy();
    return element!;
  }

  function messageQueueCount(): string | null | undefined {
    return target.querySelector('[data-testid="offline-queue-notice-count"]')?.textContent?.trim();
  }

  async function typeAndSend(text: string): Promise<void> {
    const input = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(input).toBeTruthy();
    input!.value = text;
    input!.dispatchEvent(new Event('input', { bubbles: true }));
    await waitFor(() => footer().getAttribute('aria-busy') !== 'true');
    target.querySelector<HTMLElement>('[data-testid="send-message-button"]')!.click();
    // The handler reaches the socket request synchronously before yielding.
    await tick(0);
  }

  function openTab(id: 'log' | 'settings' | 'characters' | 'extensions'): void {
    target.querySelector<HTMLButtonElement>(`[data-testid="tab-btn-${id}"]`)?.click();
  }

  function sceneNameValue(): string {
    return target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]')?.value ?? '';
  }

  async function createScene(name: string): Promise<void> {
    target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')?.click();
    await waitFor(
      () => target.querySelector('[data-testid="new-scene-name-input"]') !== null,
    );
    const nameInput = target.querySelector<HTMLInputElement>(
      '[data-testid="new-scene-name-input"]',
    );
    nameInput!.value = name;
    nameInput!.dispatchEvent(new Event('input', { bubbles: true }));
    await tick(0);
    target
      .querySelector<HTMLButtonElement>('[data-testid="submit-create-scene-btn"]')!
      .click();
    await tick(0);
  }

  function characterCard(characterId: string): HTMLElement | null {
    return target.querySelector<HTMLElement>(`[data-testid="character-card-${characterId}"]`);
  }

  async function renameCharacter(characterId: string, name: string): Promise<void> {
    characterCard(characterId)?.click();
    await waitFor(() => target.querySelector('[data-testid="character-edit-panel"]') !== null);
    const nameInput = target.querySelector<HTMLInputElement>('[data-testid="edit-name-input"]');
    expect(nameInput).toBeTruthy();
    nameInput!.value = name;
    nameInput!.dispatchEvent(new Event('input', { bubbles: true }));
    await tick(0);
    target.querySelector<HTMLButtonElement>('[data-testid="save-character-btn"]')!.click();
    await tick(0);
  }

  function characterQueueCount(): string | null | undefined {
    return target
      .querySelector(
        '[data-testid="character-queue-notice"] [data-testid="offline-queue-notice-count"]',
      )
      ?.textContent?.trim();
  }

  function extensionTabRoot(): HTMLElement | null {
    return target.querySelector<HTMLElement>('[data-testid="extension-management-tab"]');
  }

  // spec: alhalte-argenterie/内部フロー 入室→最新チャンク→送信は echo 後にのみ描画する (Issue #109 / #146)
  it('enters a room, loads the latest log chunk, and renders a message only after its echo', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerLogQueries = false;
    mountApp(socket);

    // 1. Entering the room eagerly asks for the latest biblio.
    await waitFor(() => socket.queryLogBiblioCalls.length === 1);
    expect(socket.queryLogBiblioCalls[0]).toEqual({ sceneId: '/' });
    expect(target.querySelector('[data-testid="log-loading-overlay"]')).toBeTruthy();

    // 2. The latest chunk is fetched and its records are rendered.
    socket.resolveLogBiblio({ chunks: [new Uint8Array(32)], head: [2, 1] }, '/');
    await waitFor(() => socket.queryLogChunkCalls.length === 1);
    expect(socket.queryLogChunkCalls[0]).toEqual({ sceneId: '/', chunkIndex: 2 });

    socket.resolveLogChunk(
      [logRecord({ id: 'msg-latest-echo', content: '最新チャンクのログ', timestamp: 900 })],
      '/',
      2,
    );
    await waitFor(() => messagesText().includes('最新チャンクのログ'));
    expect(messagesText()).toContain('最新チャンクのログ');
    expect(target.querySelector('[data-testid="log-loading-overlay"]')).toBeNull();

    // 3. A send stays out of the log (and the footer is busy) until the echo.
    await typeAndSend('echo を待つ統合メッセージ');
    expect(socket.requestAppendLogCalls).toHaveLength(1);
    expect(socket.requestAppendLogCalls[0].sceneId).toBe('/');
    expect(socket.requestAppendLogCalls[0].record.content).toBe('echo を待つ統合メッセージ');
    expect(footer().getAttribute('aria-busy')).toBe('true');
    expect(messagesText()).not.toContain('echo を待つ統合メッセージ');

    socket.deliverEcho();
    await waitFor(() => messagesText().includes('echo を待つ統合メッセージ'));
    expect(messagesText()).toContain('echo を待つ統合メッセージ');
    // Exactly one record: no optimistic local append next to the echo.
    expect(messagesText().match(/echo を待つ統合メッセージ/g)).toHaveLength(1);
    expect(footer().getAttribute('aria-busy')).toBe('false');
  });

  // spec: alhalte-argenterie/内部フロー 送信タイムアウト→outbox→再接続 flush→echo (Issue #109 / #146)
  it('queues a timed-out message and renders it only after the reconnect flush echo', async () => {
    const socket = new FakeRoomSocket();
    const { connectionManager } = mountApp(socket);
    await waitFor(() => socket.queryLogBiblioCalls.length === 1);

    await typeAndSend('再接続で送る統合メッセージ');
    expect(socket.requestAppendLogCalls).toHaveLength(1);

    socket.rejectPending(new OperationTimeoutError('appendLog', 8000));
    await waitFor(() => messageQueueCount() === '1 件を蓄積中');
    expect(messageQueueCount()).toBe('1 件を蓄積中');
    expect(messagesText()).not.toContain('再接続で送る統合メッセージ');

    const queued = await getOutbox().list(ROOM.id!, 'message');
    expect(queued).toHaveLength(1);
    expect((queued[0].payload as { record: LogRecord }).record.content).toBe(
      '再接続で送る統合メッセージ',
    );

    // Reconnect flushes the outbox; the message still waits for its echo.
    socket.isConnected = true;
    connectionManager.setState('disconnected');
    await tick(20);
    connectionManager.setState('connected');
    await waitFor(() => socket.requestAppendLogCalls.length === 2);
    expect(socket.requestAppendLogCalls[1].record.content).toBe('再接続で送る統合メッセージ');
    expect(messagesText()).not.toContain('再接続で送る統合メッセージ');

    socket.deliverEcho();
    await waitFor(() => messagesText().includes('再接続で送る統合メッセージ'));
    expect(messagesText()).toContain('再接続で送る統合メッセージ');
    await waitFor(
      () => target.querySelector('[data-testid="offline-queue-notice"]') === null,
    );
    expect(await getOutbox().list(ROOM.id!, 'message')).toHaveLength(0);
  });

  // spec: argenterie/scene-settings ack 後にのみ一覧を更新し、タイムアウトでは変えない (Issue #109 / #143, #146)
  it('applies a scene create only after the ack and leaves the list unchanged on timeout', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    mountApp(socket);
    openTab('settings');
    await waitFor(() => sceneNameValue() === 'メインシーン');

    // Success: no optimistic update, then the ack selects the new scene.
    await createScene('統合シーン');
    await waitFor(() => socket.requestCreateSceneCalls.length === 1);
    expect(sceneNameValue()).toBe('メインシーン');
    socket.resolveSceneAction('create');
    await waitFor(() => sceneNameValue() === '統合シーン');
    expect(sceneNameValue()).toBe('統合シーン');
    expect(target.querySelector('[data-testid="scene-action-error"]')).toBeNull();

    // Timeout: the current scene and the list stay exactly as they were.
    await createScene('失敗する統合シーン');
    await waitFor(() => socket.requestCreateSceneCalls.length === 2);
    socket.rejectSceneAction(new OperationTimeoutError('createScene', 8000), 'create');
    await waitFor(
      () => target.querySelector('[data-testid="scene-action-error"]') !== null,
    );
    expect(target.querySelector('[data-testid="scene-action-error"]')?.textContent).toContain(
      'サーバーと通信できませんでした',
    );
    expect(sceneNameValue()).toBe('統合シーン');
    expect(target.textContent).not.toContain('失敗する統合シーン');
  });

  // spec: argenterie/character 保存タイムアウト→outbox→再接続 flush で反映する (Issue #109 / #144, #146)
  it('queues a character save on timeout and applies it after the reconnect flush', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [{ ...HERO }];
    socket.autoAnswerCharacterRequests = false;
    const { connectionManager } = mountApp(socket);
    openTab('characters');
    await waitFor(() => characterCard(HERO.id) !== null);

    await renameCharacter(HERO.id, '統合で更新した勇者');
    expect(socket.requestUpdateCharacterCalls).toHaveLength(1);

    socket.rejectCharacterRequest(new OperationTimeoutError('updateCharacter', 8000));
    await waitFor(() => characterQueueCount() === '1 件を蓄積中');
    // The local list keeps the last acknowledged value until the flush.
    expect(characterCard(HERO.id)?.textContent).toContain('勇者ロラン');
    expect(characterCard(HERO.id)?.textContent).not.toContain('統合で更新した勇者');
    expect(await getOutbox().list(ROOM.id!, 'character')).toHaveLength(1);

    // Reconnect: the queued update is replayed and applied on its ack.
    socket.autoAnswerCharacterRequests = true;
    connectionManager.setState('disconnected');
    await tick(20);
    connectionManager.setState('connected');
    await waitFor(() => socket.requestUpdateCharacterCalls.length === 2);
    expect(socket.requestUpdateCharacterCalls[1].characterId).toBe(42);
    expect(socket.requestUpdateCharacterCalls[1].character.name).toBe('統合で更新した勇者');

    await waitFor(
      () => characterCard(HERO.id)?.textContent?.includes('統合で更新した勇者') === true,
    );
    expect(characterCard(HERO.id)?.textContent).toContain('統合で更新した勇者');
    expect(await getOutbox().list(ROOM.id!, 'character')).toHaveLength(0);
  });

  // spec: Issue #145 オフライン時は管理者でも拡張機能タブを暗転・操作不可にする (Issue #146)
  it('dims and disables the extension tab while offline even for an admin', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateQueries = false;
    const { connectionManager } = mountApp(socket);
    openTab('extensions');
    await waitFor(() => socket.queryStateCalls.length === 1);
    expect(socket.queryStateCalls[0]).toEqual({ sceneId: '/' });

    socket.rejectStateQuery(new OperationTimeoutError('queryState', 8000));
    await waitFor(
      () => target.querySelector('[data-testid="extension-offline-notice"]') !== null,
    );

    const root = extensionTabRoot();
    expect(root?.classList.contains('extensions-offline')).toBe(true);
    expect(root?.getAttribute('aria-disabled')).toBe('true');
    expect(
      target.querySelector('[data-testid="extension-offline-notice"]')?.textContent,
    ).toContain('サーバーと通信できませんでした');

    // Admin is still `isAdmin`, yet the offline gate blocks the write.
    const writesBefore = socket.requestUpdateStateCalls.length;
    target.querySelector<HTMLInputElement>('[data-testid="ext-toggle-alhalte-dsl"]')?.click();
    await tick(20);
    expect(socket.requestUpdateStateCalls).toHaveLength(writesBefore);

    // Reconnect retries the state query and clears the offline state.
    socket.autoAnswerStateQueries = true;
    socket.stateSnapshot = {};
    connectionManager.setState('disconnected');
    await tick(20);
    connectionManager.setState('connected');
    await waitFor(() => socket.queryStateCalls.length === 2);
    await waitFor(
      () => target.querySelector('[data-testid="extension-offline-notice"]') === null,
    );
    expect(extensionTabRoot()?.classList.contains('extensions-offline')).toBe(false);
  });

  // spec: Issue #141 アセットアップロードの進捗と失敗をモック cellar で観測する (Issue #146)
  it('shows asset upload progress and failure through the mocked cellar transfer', async () => {
    let releaseUpload!: () => void;
    const uploadGate = new Promise<void>((resolve) => {
      releaseUpload = resolve;
    });
    cellar.uploadTin.mockImplementation(async (_bytes: Uint8Array, options: any) => {
      options.onProgress?.(0.4);
      await uploadGate;
      throw new cellar.CellarTransferError('timeout', 'Cellar request exceeded 8000 ms');
    });

    const socket = new FakeRoomSocket();
    mountApp(socket);
    openTab('settings');
    await waitFor(
      () => target.querySelector('[data-testid="more-images-btn"]') !== null,
    );
    target.querySelector<HTMLElement>('[data-testid="more-images-btn"]')!.click();
    await waitFor(
      () => target.querySelector('[data-testid="asset-file-input"]') !== null,
    );

    const input = target.querySelector<HTMLInputElement>('[data-testid="asset-file-input"]')!;
    const file = new File([new Uint8Array([1, 2, 3])], 'integration-asset.png', {
      type: 'image/png',
    });
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    input.dispatchEvent(new Event('change'));

    // Progress is reported by the (mocked) XHR callback, not a fake tick.
    await waitFor(() => {
      const asset = loadAssets().find((item) => item.name === 'integration-asset.png');
      return asset?.uploadState === 'uploading' && asset.uploadProgress === 0.4;
    });
    const asset = loadAssets().find((item) => item.name === 'integration-asset.png')!;
    expect(asset.uploadState).toBe('uploading');
    expect(asset.uploadProgress).toBe(0.4);
    expect(
      target.querySelector(`[data-testid="asset-upload-progress-${asset.id}"]`),
    ).toBeTruthy();

    // The mock rejects: only this card flips to failed, with a retry action.
    releaseUpload();
    await waitFor(
      () => target.querySelector(`[data-testid="asset-upload-failed-${asset.id}"]`) !== null,
    );
    expect(
      target.querySelector(`[data-testid="asset-upload-failed-${asset.id}"]`)?.textContent,
    ).toContain('アップロードに失敗しました');
    expect(
      target.querySelector(`[data-testid="asset-retry-btn-${asset.id}"]`),
    ).toBeTruthy();
    expect(cellar.uploadTin).toHaveBeenCalledTimes(1);
  });
});
