import { describe, it, expect, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  parseLogRecord,
  ChunkLogManager,
  LOG_FALLBACK_CONTENT,
  LOG_FALLBACK_SPEAKER,
  LOG_FALLBACK_TIMESTAMP,
} from '../src/utils/log.js';
import LogTab from '../src/components/Control/tabs/LogTab.svelte';
import type { Scene } from '../src/types/index.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';

describe('Log Tab & Chunk Lazy Loading', () => {
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
});
