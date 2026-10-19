import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import fs from 'node:fs';
import path from 'node:path';
import {
  parseLogRecord,
  ChunkLogManager,
  LOG_FALLBACK_CONTENT,
  LOG_FALLBACK_SPEAKER,
  LOG_FALLBACK_TIMESTAMP,
} from '../src/utils/log.js';
import LogTab from '../src/components/Control/tabs/LogTab.svelte';
import App from '../src/components/App.svelte';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import { hexToBytes } from '@alhalte/pantry';
import {
  DEFAULT_OFFICIAL_EXTENSIONS,
  ExtensionPipeline,
  loadDefaultExtensions,
} from '../src/wasm/runner.js';
import type { Character, ExtensionItem, Scene, VisitedRoom } from '../src/types/index.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';

/**
 * Polls `predicate` until it holds. Used where WASM log rendering is involved so
 * the assertions do not depend on a fixed sleep duration (Issue #109).
 */
async function waitFor(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

// Valid WebAssembly binary implementing:
// - memory (1 page)
// - alhalte_abi_version() -> 1
// - alloc(len) -> 0
// - dealloc(ptr, len) -> void
// - process_generast(ptr, len) -> copies input to 1024, appends '!' (0x21), returns (1024n << 32) | (len + 1)
const VALID_WASM_HEX =
  '0061736d010000000115046000017f60017f017f60027f7f0060027f7f017e030504000102030503010001074505066d656d6f7279020013616c68616c74655f6162695f76657273696f6e000005616c6c6f630001076465616c6c6f6300021070726f636573735f67656e657261737400030a3404040041010b040041000b02000b250041800820002001fc0a000041800820016a41213a0000428008422086200141016aad840b';

const markdownWasm = fs.readFileSync(
  path.resolve(__dirname, '../public/extensions/markdown.wasm'),
);
const dslWasm = fs.readFileSync(
  path.resolve(__dirname, '../public/extensions/alhalte-dsl.wasm'),
);

// Polyfill AnimationEvent in jsdom environment if not defined
if (typeof globalThis.AnimationEvent === 'undefined') {
  class MockAnimationEvent extends Event {
    animationName: string;
    constructor(type: string, init?: { animationName?: string; bubbles?: boolean; cancelable?: boolean }) {
      super(type, init);
      this.animationName = init?.animationName ?? '';
    }
  }
  (globalThis as unknown as { AnimationEvent: typeof MockAnimationEvent }).AnimationEvent = MockAnimationEvent;
}

describe('Log Tab & Chunk Lazy Loading', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  // spec: alhalte-argenterie/log-tab フォールバック値
  it('applies fallback values when content, speaker, or timestamp are absent or empty', () => {
    // Missing all fields
    const record1 = parseLogRecord({});
    expect(record1.content).toBe(LOG_FALLBACK_CONTENT); // ""
    expect(record1.speaker).toBe(LOG_FALLBACK_SPEAKER); // "Anonymous"
    expect(record1.timestamp).toBe(LOG_FALLBACK_TIMESTAMP); // 0 (1970-01-01 00:00:00)

    // Provided fields
    const record2 = parseLogRecord({
      content: 'ダイスロール結果: 42',
      speaker: 'GM',
      timestamp: 1700000000,
    });
    expect(record2.content).toBe('ダイスロール結果: 42');
    expect(record2.speaker).toBe('GM');
    expect(record2.timestamp).toBe(1700000000);
  });

  // spec: alhalte-argenterie/log-tab チャンク遅延取得
  it('loads chunks lazily only when scroll reaches an edge', async () => {
    const fetchHandler = vi.fn().mockImplementation(async (chunkId: string) => {
      if (chunkId === 'chunk-1') {
        return [
          { content: 'メッセージ1', speaker: 'Alice', timestamp: 100 },
          { content: 'メッセージ2', speaker: 'Bob', timestamp: 105 },
        ];
      }
      if (chunkId === 'chunk-2') {
        return [{ content: 'メッセージ3', speaker: 'Charlie', timestamp: 110 }];
      }
      return [];
    });

    const manager = new ChunkLogManager(fetchHandler);
    manager.registerSceneChunks('scene-1', ['chunk-1', 'chunk-2']);

    // Initially nothing loaded
    expect(manager.isChunkLoaded('chunk-1')).toBe(false);
    expect(manager.isChunkLoaded('chunk-2')).toBe(false);
    expect(fetchHandler).not.toHaveBeenCalled();

    // Reaching top edge loads oldest chunk
    const loaded = await manager.handleScrollEdge('scene-1', 'top');
    expect(loaded).toHaveLength(1); // chunk-2 in reverse or chunk-1
    expect(fetchHandler).toHaveBeenCalledTimes(1);

    const records = manager.getLoadedRecords('scene-1');
    expect(records.length).toBeGreaterThan(0);
  });

  // spec: alhalte-argenterie/log-tab LogTab コンポーネントヘッダーと管理者メニュー
  it('renders LogTab header, collapse button, scene name, and gear dropdown with admin options', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-tavern',
      name: '酒場「竜の息吹」亭',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let collapsed = false;
    let exportedZip = false;

    // Non-admin render first
    const instanceNonAdmin = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        isAdmin: false,
        onCollapse: () => {
          collapsed = true;
        },
        onExportZip: () => {
          exportedZip = true;
        },
      },
    });

    // Check header elements
    expect(target.querySelector('[data-testid="collapse-button"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="scene-name-header"]')?.textContent?.trim()).toBe(
      '酒場「竜の息吹」亭',
    );

    // Test collapse
    target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]')?.click();
    expect(collapsed).toBe(true);

    // Open gear menu
    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="zip-export-item"]')).toBeTruthy();
    // Non-admin should not see admin section
    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeNull();

    unmount(instanceNonAdmin);
    target.innerHTML = '';

    // Admin render
    const instanceAdmin = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        isAdmin: true,
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Admin options: Issue #94 replaces rename/delete/members with manage-permissions-item
    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="manage-permissions-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="rename-scene-item"]')).toBeNull();
    expect(target.querySelector('[data-testid="delete-scene-item"]')).toBeNull();
    expect(target.querySelector('[data-testid="manage-members-item"]')).toBeNull();

    unmount(instanceAdmin);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab appendRecord によるリアクティブなログ描画更新
  it('immediately renders appended log records in a mounted LogTab', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: '/',
      name: 'メインシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const chunkManager = new ChunkLogManager();
    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        chunkManager,
      },
    });

    expect(target.querySelector('[data-testid="empty-log-notice"]')).toBeTruthy();

    chunkManager.appendRecord({
      id: 'msg-live-1',
      sceneId: '/',
      content: '2D6+3 攻撃ロール！',
      speaker: '探索者A',
      speakerId: 'char-1',
      timestamp: 1700000123,
    });

    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="empty-log-notice"]')).toBeNull();
    expect(target.querySelector('[data-testid="msg-speaker-msg-live-1"]')?.textContent).toBe(
      '探索者A',
    );
    expect(
      target.querySelector('[data-testid="msg-content-msg-live-1"]')?.textContent?.trim(),
    ).toBe('2D6+3 攻撃ロール！');

    const msgEl = target.querySelector('[data-testid="log-record-msg-live-1"]');
    expect(msgEl?.classList.contains('message-entry-new')).toBe(true);
    expect(msgEl?.getAttribute('data-is-new')).toBe('true');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/room コントロールパネル共通フッターからのメッセージ送信とログ表示
  it('sends a message from App footer input and displays it in LogTab while populating default extensions', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const testRoom: VisitedRoom = {
      id: 'room-send-test',
      name: 'メッセージ送信テスト部屋',
      url: 'https://alhalte.example/rooms/room-send-test#key',
      lastVisited: Date.now(),
      isAdmin: true,
    };

    const testChars: Character[] = [
      {
        id: 'char-hero',
        name: 'アルハルテ太郎',
        initiative: 12,
        description: 'テストPC',
        statuses: [],
        parameters: [],
        chatPalette: '1D100<=50 【目星】',
        hideStatus: false,
      },
    ];

    const fakeSocket = new FakeRoomSocket();

    const instance = mount(App, {
      target,
      props: {
        initialRoom: testRoom,
        initialCharacters: testChars,
        isAdmin: true,
        // The message is only rendered once the server's `LogEventAppended`
        // echo arrives (Issue #109), so drive a connected socket explicitly.
        roomSocket: asRoomSocket(fakeSocket),
      },
    });

    const inputEl = target.querySelector<HTMLInputElement>('[data-testid="chat-input-field"]');
    const sendBtn = target.querySelector<HTMLElement>('[data-testid="send-message-button"]');
    expect(inputEl).toBeTruthy();
    expect(sendBtn).toBeTruthy();

    // Type message and click send
    inputEl!.value = 'こんにちは、世界！';
    inputEl!.dispatchEvent(new Event('input', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    sendBtn!.click();
    await new Promise((r) => setTimeout(r, 30));

    // Input is cleared immediately, but nothing is rendered before the echo.
    expect(inputEl!.value).toBe('');
    expect(fakeSocket.requestAppendLogCalls).toHaveLength(1);
    expect(fakeSocket.requestAppendLogCalls[0].record.content).toBe('こんにちは、世界！');
    const messagesBeforeEcho = target.querySelector('[data-testid="messages-list"]');
    expect(messagesBeforeEcho?.textContent).not.toContain('こんにちは、世界！');

    // The server echo confirms the append: only now does it appear in LogTab.
    fakeSocket.deliverEcho();
    await waitFor(
      () =>
        !!target
          .querySelector('[data-testid="messages-list"]')
          ?.textContent?.includes('こんにちは、世界！'),
    );

    const messagesList = target.querySelector('[data-testid="messages-list"]');
    expect(messagesList?.textContent).toContain('こんにちは、世界！');
    expect(messagesList?.textContent).toContain('アルハルテ太郎');
    // Exactly one record was appended (the echo), not a local copy plus echo.
    expect(target.querySelectorAll('[data-testid^="log-record-"]')).toHaveLength(1);

    const newMsgEl = target.querySelector('.message-entry.message-entry-new');
    expect(newMsgEl).toBeTruthy();
    expect(newMsgEl?.getAttribute('data-is-new')).toBe('true');

    // Verify default official extensions are populated when switching to Extensions tab
    expect(DEFAULT_OFFICIAL_EXTENSIONS).toHaveLength(6);
    const fallbackExts = await loadDefaultExtensions(undefined);
    expect(fallbackExts.map((e) => e.id)).toEqual([
      'alhalte-dsl',
      'markdown',
      'aozora',
      'coc-6',
      'coc-7',
      'emoklore',
    ]);

    const extTabBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-extensions"]');
    extTabBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="ext-name-alhalte-dsl"]')?.textContent?.trim()).toBe(
      'Alhalte DSL',
    );
    expect(target.querySelector('[data-testid="ext-name-markdown"]')?.textContent?.trim()).toBe(
      'Markdown スタイル',
    );
    expect(target.querySelector('[data-testid="ext-name-aozora"]')?.textContent?.trim()).toBe(
      '青空文庫ルビ・傍点',
    );

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab 歯車メニューの外側クリック閉じ (Issue #31)
  it('closes gear dropdown menu when clicking outside', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-gear-test',
      name: 'テストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        isAdmin: true,
      },
    });

    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    expect(gearBtn).toBeTruthy();
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="gear-dropdown-menu"]')).toBeTruthy();

    // Click outside on document.body
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="gear-dropdown-menu"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab 新規メッセージ追加時の Material 3 Expressive バネアニメーション (Issue #90)
  it('applies M3 Expressive spring animation class and attribute only to newly appended messages, preserving natural static rendering for initial messages', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-spring-test',
      name: 'バネテストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const chunkManager = new ChunkLogManager();
    // Pre-populate with initial existing message before mount
    chunkManager.appendRecord({
      id: 'msg-initial-1',
      sceneId: 'scene-spring-test',
      content: '既存の初期メッセージ',
      speaker: 'ルシア',
      timestamp: 1000,
    });

    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        chunkManager,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // 1. Initial message must render naturally without animation classes or attributes
    const initialEl = target.querySelector('[data-testid="log-record-msg-initial-1"]');
    expect(initialEl).toBeTruthy();
    expect(initialEl?.classList.contains('message-entry')).toBe(true);
    expect(initialEl?.classList.contains('message-entry-new')).toBe(false);
    expect(initialEl?.getAttribute('data-is-new')).toBeNull();

    // 2. Append new message 1
    chunkManager.appendRecord({
      id: 'msg-live-1',
      sceneId: 'scene-spring-test',
      content: '新着メッセージ1',
      speaker: 'ベルナルド',
      timestamp: 1001,
    });
    await new Promise((r) => setTimeout(r, 20));

    const newEl1 = target.querySelector('[data-testid="log-record-msg-live-1"]');
    expect(newEl1).toBeTruthy();
    expect(newEl1?.classList.contains('message-entry')).toBe(true);
    expect(newEl1?.classList.contains('message-entry-new')).toBe(true);
    expect(newEl1?.getAttribute('data-is-new')).toBe('true');

    // Initial message still does not have the animation class
    expect(initialEl?.classList.contains('message-entry-new')).toBe(false);

    // 3. Append new message 2
    chunkManager.appendRecord({
      id: 'msg-live-2',
      sceneId: 'scene-spring-test',
      content: '新着メッセージ2',
      speaker: 'ルシア',
      timestamp: 1002,
    });
    await new Promise((r) => setTimeout(r, 20));

    const newEl2 = target.querySelector('[data-testid="log-record-msg-live-2"]');
    expect(newEl2).toBeTruthy();
    expect(newEl2?.classList.contains('message-entry-new')).toBe(true);
    expect(newEl2?.getAttribute('data-is-new')).toBe('true');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab シーン切り替え時の既存メッセージの自然描画と新規メッセージのアニメーション (Issue #90)
  it('resets new record animation state when switching scenes, treating next scene records as existing', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const sceneA: Scene = {
      id: 'scene-a',
      name: 'シーンA',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
    const sceneB: Scene = {
      id: 'scene-b',
      name: 'シーンB',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const chunkManager = new ChunkLogManager();
    chunkManager.appendRecord({
      id: 'msg-a-1',
      sceneId: 'scene-a',
      content: 'シーンAの既存メッセージ',
      speaker: 'ルシア',
      timestamp: 100,
    });
    chunkManager.appendRecord({
      id: 'msg-b-1',
      sceneId: 'scene-b',
      content: 'シーンBの既存メッセージ',
      speaker: 'ベルナルド',
      timestamp: 101,
    });

    const instance = mount(LogTab, {
      target,
      props: {
        scene: sceneA,
        allScenes: [sceneA, sceneB],
        chunkManager,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Scene A initial record: no animation
    const msgA1 = target.querySelector('[data-testid="log-record-msg-a-1"]');
    expect(msgA1?.classList.contains('message-entry-new')).toBe(false);

    // Append new record to Scene A
    chunkManager.appendRecord({
      id: 'msg-a-new',
      sceneId: 'scene-a',
      content: 'シーンAの新規メッセージ',
      speaker: 'ルシア',
      timestamp: 102,
    });
    await new Promise((r) => setTimeout(r, 20));

    const msgANew = target.querySelector('[data-testid="log-record-msg-a-new"]');
    expect(msgANew?.classList.contains('message-entry-new')).toBe(true);

    unmount(instance);
    target.innerHTML = '';

    // Mount in Scene B: msg-b-1 is existing
    const instanceB = mount(LogTab, {
      target,
      props: {
        scene: sceneB,
        allScenes: [sceneA, sceneB],
        chunkManager,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    const msgB1 = target.querySelector('[data-testid="log-record-msg-b-1"]');
    expect(msgB1).toBeTruthy();
    expect(msgB1?.classList.contains('message-entry-new')).toBe(false);

    // Append new record to Scene B
    chunkManager.appendRecord({
      id: 'msg-b-new',
      sceneId: 'scene-b',
      content: 'シーンBの新規メッセージ',
      speaker: 'ベルナルド',
      timestamp: 103,
    });
    await new Promise((r) => setTimeout(r, 20));

    const msgBNew = target.querySelector('[data-testid="log-record-msg-b-new"]');
    expect(msgBNew?.classList.contains('message-entry-new')).toBe(true);
    expect(msgBNew?.getAttribute('data-is-new')).toBe('true');

    unmount(instanceB);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab Material 3 Expressive バネアニメーション規則の定義 (Issue #90)
  it('defines M3 Expressive spatial cubic-bezier, effects curve, and spring keyframes in LogTab', async () => {
    const fs = await import('node:fs');
    const path = await import('node:path');
    const filePath = path.resolve(__dirname, '../src/components/Control/tabs/LogTab.svelte');
    const content = fs.readFileSync(filePath, 'utf-8');

    // M3 Expressive spatial motion token & cubic-bezier: 500ms cubic-bezier(0.38, 1.21, 0.22, 1)
    expect(content).toContain('--np-motion-expressive-default-spatial');
    expect(content).toContain('cubic-bezier(0.38, 1.21, 0.22, 1)');
    expect(content).toContain('np-message-spring-spatial');

    // M3 Expressive effects motion token for fade-in: 150ms cubic-bezier(0.31, 0.94, 0.34, 1)
    expect(content).toContain('--np-motion-expressive-fast-effects');
    expect(content).toContain('np-message-fade-in');

    // Spring translate from right offset to 0
    expect(content).toContain('translateX(var(--msg-entry-offset-x, 28px))');
    expect(content).toContain('transform: translateX(0)');

    // Accessibility: prefers-reduced-motion
    expect(content).toContain('@media (prefers-reduced-motion: reduce)');
  });

  // spec: alhalte-argenterie/log-tab ZIP エクスポートエンドポイントへのフェッチとダウンロード実行 (Issue #66)
  it('triggers fetch to the correct ZIP export endpoint and executes download when clicking export menu item', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-export-test',
      name: 'エクスポートテストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const mockBlob = new Blob(['PK\x03\x04mock-zip-content'], { type: 'application/zip' });
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      blob: async () => mockBlob,
    });
    vi.stubGlobal('fetch', fetchMock);

    const createObjectURLMock = vi.fn().mockReturnValue('blob:http://localhost/test-blob-uuid');
    const revokeObjectURLMock = vi.fn();
    URL.createObjectURL = createObjectURLMock;
    URL.revokeObjectURL = revokeObjectURLMock;

    let createdDownloadAnchor: HTMLAnchorElement | null = null;
    const originalCreateElement = document.createElement.bind(document);
    const createElementSpy = vi.spyOn(document, 'createElement').mockImplementation((tagName: string, options?: ElementCreationOptions) => {
      const el = originalCreateElement(tagName, options);
      if (tagName.toLowerCase() === 'a') {
        createdDownloadAnchor = el as HTMLAnchorElement;
      }
      return el;
    });
    const anchorClickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});

    let exportedType: 'full' | 'logs' | null = null;
    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        roomId: '9f3a7c1e5b2d8046',
        onExportZip: (type) => {
          exportedType = type;
        },
      },
    });

    // 1. Test clicking full ZIP export item
    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const fullZipItem = target.querySelector<HTMLElement>('[data-testid="zip-export-item"]');
    expect(fullZipItem).toBeTruthy();
    fullZipItem?.click();
    await new Promise((r) => setTimeout(r, 50));

    expect(exportedType).toBe('full');
    expect(fetchMock).toHaveBeenCalledWith(`${window.location.origin}/api/room/9f3a7c1e5b2d8046/export/zip?type=full`);
    expect(createObjectURLMock).toHaveBeenCalledWith(mockBlob);
    expect(anchorClickSpy).toHaveBeenCalled();
    expect(createdDownloadAnchor?.download).toBe('room_9f3a7c1e5b2d8046_full.zip');
    expect(revokeObjectURLMock).toHaveBeenCalledWith('blob:http://localhost/test-blob-uuid');

    // 2. Test clicking logs ZIP export item
    fetchMock.mockClear();
    createObjectURLMock.mockClear();
    revokeObjectURLMock.mockClear();
    anchorClickSpy.mockClear();
    createdDownloadAnchor = null;

    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const logsZipItem = target.querySelector<HTMLElement>('[data-testid="export-zip-logs-item"]');
    expect(logsZipItem).toBeTruthy();
    logsZipItem?.click();
    await new Promise((r) => setTimeout(r, 50));

    expect(exportedType).toBe('logs');
    expect(fetchMock).toHaveBeenCalledWith(`${window.location.origin}/api/room/9f3a7c1e5b2d8046/export/zip?type=logs`);
    expect(createObjectURLMock).toHaveBeenCalledWith(mockBlob);
    expect(anchorClickSpy).toHaveBeenCalled();
    expect(createdDownloadAnchor?.download).toBe('room_9f3a7c1e5b2d8046_logs.zip');
    expect(revokeObjectURLMock).toHaveBeenCalledWith('blob:http://localhost/test-blob-uuid');

    // Cleanup
    anchorClickSpy.mockRestore();
    createElementSpy.mockRestore();
    vi.unstubAllGlobals();
    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab アニメーション終了時の新規メッセージクラス解除と再オープン時のアニメーション抑止 (Issue #112)
  it('removes animation class and data-is-new attribute upon animationend, preventing re-animation when reopening panel', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-animation-end',
      name: 'アニメーション解除テストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const chunkManager = new ChunkLogManager();
    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        chunkManager,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Append new message
    chunkManager.appendRecord({
      id: 'msg-end-1',
      sceneId: 'scene-animation-end',
      content: 'アニメーション終了テスト',
      speaker: 'ルシア',
      timestamp: 2000,
    });
    await new Promise((r) => setTimeout(r, 20));

    const msgEl = target.querySelector('[data-testid="log-record-msg-end-1"]');
    expect(msgEl).toBeTruthy();
    expect(msgEl?.classList.contains('message-entry-new')).toBe(true);
    expect(msgEl?.getAttribute('data-is-new')).toBe('true');

    // 1. Dispatch animationend for faster fade-in effect (150ms).
    // Spatial spring animation (500ms) is still running, so new class should NOT be removed yet.
    msgEl?.dispatchEvent(new AnimationEvent('animationend', { animationName: 'np-message-fade-in' }));
    await new Promise((r) => setTimeout(r, 20));

    expect(msgEl?.classList.contains('message-entry-new')).toBe(true);
    expect(msgEl?.getAttribute('data-is-new')).toBe('true');

    // 2. Dispatch animationend for spatial spring animation (500ms).
    // Animation is now fully complete, so message-entry-new class and data-is-new should be removed.
    msgEl?.dispatchEvent(new AnimationEvent('animationend', { animationName: 'np-message-spring-spatial' }));
    await new Promise((r) => setTimeout(r, 20));

    expect(msgEl?.classList.contains('message-entry-new')).toBe(false);
    expect(msgEl?.getAttribute('data-is-new')).toBeNull();

    // 3. Append another message and verify animationend without animationName also clears it naturally
    chunkManager.appendRecord({
      id: 'msg-end-2',
      sceneId: 'scene-animation-end',
      content: 'アニメーション終了テスト2',
      speaker: 'ベルナルド',
      timestamp: 2001,
    });
    await new Promise((r) => setTimeout(r, 20));

    const msgEl2 = target.querySelector('[data-testid="log-record-msg-end-2"]');
    expect(msgEl2?.classList.contains('message-entry-new')).toBe(true);
    expect(msgEl2?.getAttribute('data-is-new')).toBe('true');

    msgEl2?.dispatchEvent(new Event('animationend'));
    await new Promise((r) => setTimeout(r, 20));

    expect(msgEl2?.classList.contains('message-entry-new')).toBe(false);
    expect(msgEl2?.getAttribute('data-is-new')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/room コントロールパネルの再オープン時に最後のメッセージのアニメーションが再トリガーされないこと (Issue #112)
  it('does not re-trigger animation on the last message when closing and reopening the control panel after animation ends', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const testRoom: VisitedRoom = {
      id: 'room-reopen-test',
      name: 'コントロールパネル再オープン部屋',
      url: 'https://alhalte.example/rooms/room-reopen-test#key',
      lastVisited: Date.now(),
      isAdmin: true,
    };

    const fakeSocket = new FakeRoomSocket();

    const instance = mount(App, {
      target,
      props: {
        initialRoom: testRoom,
        isAdmin: true,
        // Rendering waits for the server echo (Issue #109).
        roomSocket: asRoomSocket(fakeSocket),
      },
    });

    await new Promise((r) => setTimeout(r, 30));

    const inputEl = target.querySelector<HTMLInputElement>('[data-testid="chat-input-field"]');
    const sendBtn = target.querySelector<HTMLElement>('[data-testid="send-message-button"]');
    expect(inputEl).toBeTruthy();
    expect(sendBtn).toBeTruthy();

    // Send a message
    inputEl!.value = 'こんにちは！';
    inputEl!.dispatchEvent(new Event('input', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    sendBtn!.click();
    await new Promise((r) => setTimeout(r, 30));
    fakeSocket.deliverEcho();
    await waitFor(() => target.querySelector('.message-entry.message-entry-new') !== null);

    const msgEl = target.querySelector('.message-entry.message-entry-new');
    expect(msgEl).toBeTruthy();
    expect(msgEl?.getAttribute('data-is-new')).toBe('true');

    // Finish animation
    msgEl?.dispatchEvent(new AnimationEvent('animationend', { animationName: 'np-message-spring-spatial' }));
    await new Promise((r) => setTimeout(r, 20));

    expect(msgEl?.classList.contains('message-entry-new')).toBe(false);
    expect(msgEl?.getAttribute('data-is-new')).toBeNull();

    // Collapse the Side Sheet
    const collapseBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(collapseBtn).toBeTruthy();
    collapseBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Reopen the Side Sheet
    const reopenBtn = target.querySelector<HTMLButtonElement>('[data-testid="reopen-sheet-button"]');
    expect(reopenBtn).toBeTruthy();
    reopenBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Check that the last message still does NOT have the message-entry-new class or data-is-new attribute
    const reopenedMsgEl = target.querySelector('.messages-list .message-entry');
    expect(reopenedMsgEl).toBeTruthy();
    expect(reopenedMsgEl?.classList.contains('message-entry-new')).toBe(false);
    expect(reopenedMsgEl?.getAttribute('data-is-new')).toBeNull();
    expect(target.querySelector('.message-entry.message-entry-new')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab 拡張機能Generast左畳み込みとSaignant WASM描画 (Issue #117, Issue #127)
  it('applies extension pipeline left fold and Saignant WASM rendering to log records in LogTab without label mismatch', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-ext-test',
      name: '拡張機能テストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const extensions: ExtensionItem[] = [
      {
        id: 'markdown',
        name: 'Markdown スタイル',
        enabled: true,
        priority: 0,
        wasmBytes: markdownWasm,
      },
    ];

    const chunkManager = new ChunkLogManager();
    const pipeline = new ExtensionPipeline();

    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        extensions,
        extensionPipeline: pipeline,
        chunkManager,
      },
    });

    // 1. Japanese plain text: should render without label mismatch error
    chunkManager.appendRecord({
      id: 'msg-fold-1',
      sceneId: 'scene-ext-test',
      content: 'ほげほげ',
      speaker: '探索者',
      speakerId: 'char-1',
      timestamp: 1000,
    });

    // 2. Markdown text: should render styled bold html
    chunkManager.appendRecord({
      id: 'msg-fold-2',
      sceneId: 'scene-ext-test',
      content: '**強調テキスト**',
      speaker: '探索者',
      speakerId: 'char-1',
      timestamp: 1001,
    });

    // Rendering runs through the WASM extension, so wait for the rendered HTML
    // instead of a fixed delay (the old 60 ms sleep flaked under parallel load,
    // Issue #109).
    await waitFor(() => {
      const el = target.querySelector('[data-testid="msg-content-msg-fold-2"]');
      return !!el?.innerHTML?.includes('saignant-bold');
    });

    const content1 = target.querySelector('[data-testid="msg-content-msg-fold-1"]');
    expect(content1).toBeTruthy();
    expect(content1?.textContent?.trim()).toBe('ほげほげ');
    expect(content1?.textContent).not.toContain('label mismatch');
    expect(content1?.textContent).not.toContain('0xd38f9ce233c2315a');

    const content2 = target.querySelector('[data-testid="msg-content-msg-fold-2"]');
    expect(content2).toBeTruthy();
    expect(content2?.innerHTML).toContain('saignant-bold');
    expect(content2?.textContent?.trim()).toBe('強調テキスト');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab 拡張機能優先度順の左畳み込み適用 (Issue #117, Issue #127)
  it('applies multiple active extensions sequentially in priority order (left fold)', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-prio-test',
      name: '優先度テストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const extensions: ExtensionItem[] = [
      {
        id: 'alhalte-dsl',
        name: 'Alhalte DSL',
        enabled: true,
        priority: 0,
        wasmBytes: dslWasm,
      },
      {
        id: 'markdown',
        name: 'Markdown スタイル',
        enabled: true,
        priority: 1,
        wasmBytes: markdownWasm,
      },
    ];

    const chunkManager = new ChunkLogManager();
    const pipeline = new ExtensionPipeline();

    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        extensions,
        extensionPipeline: pipeline,
        chunkManager,
      },
    });

    chunkManager.appendRecord({
      id: 'msg-prio-1',
      sceneId: 'scene-prio-test',
      content: '**判定** 1D100<=50',
      speaker: '騎士',
      timestamp: 1000,
    });

    // Rendering runs through two WASM extensions, so wait for the rendered HTML
    // instead of relying on a fixed delay (this assertion used to flake under
    // parallel load without weakening either expectation).
    await waitFor(
      () =>
        !!target
          .querySelector('[data-testid="msg-content-msg-prio-1"]')
          ?.innerHTML?.includes('saignant-bold'),
    );

    const contentEl = target.querySelector('[data-testid="msg-content-msg-prio-1"]');
    expect(contentEl).toBeTruthy();
    // Both active extensions process in left fold:
    // alhalte-dsl handles dice roll and markdown formats bold
    expect(contentEl?.innerHTML).toContain('saignant-bold');
    expect(contentEl?.innerHTML).toContain('1D100');
    expect(contentEl?.textContent).not.toContain('label mismatch');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab 拡張機能の有効化・無効化・更新時のログ描画キャッシュ更新 (Issue #117)
  it('invalidates rendered HTML cache and re-renders log messages when extensions are updated or toggled in App', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const testRoom: VisitedRoom = {
      id: 'room-ext-toggle-test',
      name: '拡張機能切替部屋',
      url: 'https://alhalte.example/rooms/room-ext-toggle-test#key',
      lastVisited: Date.now(),
      isAdmin: true,
    };

    const initialExtensions: ExtensionItem[] = [
      {
        id: 'markdown',
        name: 'Markdown スタイル',
        enabled: true,
        priority: 0,
        wasmBytes: markdownWasm,
      },
    ];

    const fakeSocket = new FakeRoomSocket();

    const instance = mount(App, {
      target,
      props: {
        initialRoom: testRoom,
        initialExtensions,
        isAdmin: true,
        // Rendering waits for the server echo (Issue #109).
        roomSocket: asRoomSocket(fakeSocket),
      },
    });

    await new Promise((r) => setTimeout(r, 30));

    // Send a message
    const inputEl = target.querySelector<HTMLInputElement>('[data-testid="chat-input-field"]');
    const sendBtn = target.querySelector<HTMLElement>('[data-testid="send-message-button"]');
    expect(inputEl).toBeTruthy();
    expect(sendBtn).toBeTruthy();

    inputEl!.value = '**こんにちは**';
    inputEl!.dispatchEvent(new Event('input', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    sendBtn!.click();
    await new Promise((r) => setTimeout(r, 30));
    fakeSocket.deliverEcho();
    await waitFor(
      () =>
        !!target.querySelector('[data-testid="messages-list"]')?.textContent?.includes('こんにちは'),
    );

    // Initially with extension enabled, message has bold styling
    const messagesList = target.querySelector('[data-testid="messages-list"]');
    expect(messagesList?.innerHTML).toContain('saignant-bold');
    expect(messagesList?.textContent).toContain('こんにちは');

    // Switch to Extensions Tab
    const extTabBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-extensions"]');
    expect(extTabBtn).toBeTruthy();
    extTabBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Toggle the extension off
    const toggleCheckbox = target.querySelector<HTMLInputElement>('[data-testid="ext-toggle-markdown"]');
    expect(toggleCheckbox).toBeTruthy();
    toggleCheckbox?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Switch back to Log Tab. Issue #161 persists the extension result in
    // `contentTin` at send time, so toggling the local extension off must not
    // change the display of the already-sent record.
    const logTabBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-log"]');
    expect(logTabBtn).toBeTruthy();
    logTabBtn?.click();
    await waitFor(() => {
      const list = target.querySelector('[data-testid="messages-list"]');
      return !!list && list.innerHTML.includes('saignant-bold');
    });

    // The persisted contentTin still renders the same bold message; the raw
    // Markdown source is only the fallback field.
    const updatedMessagesList = target.querySelector('[data-testid="messages-list"]');
    expect(updatedMessagesList?.innerHTML).toContain('saignant-bold');
    expect(updatedMessagesList?.textContent).toContain('こんにちは');
    expect(updatedMessagesList?.textContent).not.toContain('**こんにちは**');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab 既存およびキュー積載ログへの左畳み込み適用 (Issue #117)
  it('applies Generast left fold and Saignant HTML rendering to pre-existing and queued records upon mounting LogTab', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-pre-test',
      name: '事前ログテストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const chunkManager = new ChunkLogManager();
    chunkManager.appendRecord({
      id: 'msg-pre-1',
      sceneId: 'scene-pre-test',
      content: '**過去ログ1**',
      speaker: 'ルシア',
      timestamp: 100,
    });
    chunkManager.appendRecord({
      id: 'msg-pre-2',
      sceneId: 'scene-pre-test',
      content: '*キューログ2*',
      speaker: 'ベルナルド',
      timestamp: 101,
    });

    const extensions: ExtensionItem[] = [
      {
        id: 'markdown',
        name: 'Markdown スタイル',
        enabled: true,
        priority: 0,
        wasmBytes: markdownWasm,
      },
    ];

    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        extensions,
        chunkManager,
      },
    });

    // Rendering runs through the WASM extension, so wait for both rendered
    // records instead of a fixed delay (the old 60 ms sleep flaked under
    // parallel load, Issue #109).
    await waitFor(() => {
      const first = target.querySelector('[data-testid="msg-content-msg-pre-1"]');
      const second = target.querySelector('[data-testid="msg-content-msg-pre-2"]');
      return (
        !!first?.innerHTML?.includes('saignant-bold') &&
        !!second?.innerHTML?.includes('saignant-italic')
      );
    });

    const content1 = target.querySelector('[data-testid="msg-content-msg-pre-1"]');
    const content2 = target.querySelector('[data-testid="msg-content-msg-pre-2"]');
    expect(content1?.innerHTML).toContain('saignant-bold');
    expect(content1?.textContent?.trim()).toBe('過去ログ1');
    expect(content2?.innerHTML).toContain('saignant-italic');
    expect(content2?.textContent?.trim()).toBe('キューログ2');

    unmount(instance);
    target.remove();
  });
});


