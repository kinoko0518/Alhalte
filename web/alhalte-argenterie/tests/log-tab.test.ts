import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  parseLogRecord,
  ChunkLogManager,
  LOG_FALLBACK_CONTENT,
  LOG_FALLBACK_SPEAKER,
  LOG_FALLBACK_TIMESTAMP,
} from '../src/utils/log.js';
import LogTab from '../src/components/Control/tabs/LogTab.svelte';
import App from '../src/components/App.svelte';
import { DEFAULT_OFFICIAL_EXTENSIONS, loadDefaultExtensions } from '../src/wasm/runner.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';

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

    // Admin options must be present
    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="rename-scene-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="delete-scene-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="manage-members-item"]')).toBeTruthy();

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

    const instance = mount(App, {
      target,
      props: {
        initialRoom: testRoom,
        initialCharacters: testChars,
        isAdmin: true,
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

    // Input should be cleared and message rendered in LogTab
    expect(inputEl!.value).toBe('');
    const messagesList = target.querySelector('[data-testid="messages-list"]');
    expect(messagesList?.textContent).toContain('こんにちは、世界！');
    expect(messagesList?.textContent).toContain('アルハルテ太郎');

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
        roomId: 'room-abc-789',
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
    expect(fetchMock).toHaveBeenCalledWith(`${window.location.origin}/api/room/room-abc-789/export/zip?type=full`);
    expect(createObjectURLMock).toHaveBeenCalledWith(mockBlob);
    expect(anchorClickSpy).toHaveBeenCalled();
    expect(createdDownloadAnchor?.download).toBe('room_room-abc-789_full.zip');
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
    expect(fetchMock).toHaveBeenCalledWith(`${window.location.origin}/api/room/room-abc-789/export/zip?type=logs`);
    expect(createObjectURLMock).toHaveBeenCalledWith(mockBlob);
    expect(anchorClickSpy).toHaveBeenCalled();
    expect(createdDownloadAnchor?.download).toBe('room_room-abc-789_logs.zip');
    expect(revokeObjectURLMock).toHaveBeenCalledWith('blob:http://localhost/test-blob-uuid');

    // Cleanup
    anchorClickSpy.mockRestore();
    createElementSpy.mockRestore();
    vi.unstubAllGlobals();
    unmount(instance);
    target.remove();
  });
});


