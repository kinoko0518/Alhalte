/**
 * Scene graph tab — グラフ / リスト表示 (Issue #273).
 *
 * The tab keeps the git-graph view as the default and adds a list view below a
 * `SegmentedButton`. The list shows every scene with a `Divider` between rows,
 * moves the viewpoint with the same callback the graph nodes use, marks the
 * current scene and offers an admin-only trash button that goes through the
 * root-hoisted delete confirmation.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> シーングラフタブ`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
 * - GitHub Issue #273
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import SceneGraphTab from '../src/components/Control/tabs/SceneGraphTab.svelte';
import App from '../src/components/App.svelte';
import { clearAssets } from '../src/utils/assets.js';
import { resetOutbox } from '../src/utils/outbox.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { SceneMovementEvent } from '../src/utils/sceneGraph.js';
import type { RoomRole } from '../src/utils/session.js';
import type { Scene, VisitedRoom } from '../src/types/index.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';

function settle(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function makeScene(id: string, name: string): Scene {
  return {
    id,
    name,
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
  };
}

const scenes: Scene[] = [
  makeScene('/', 'メインシーン'),
  makeScene('scene-a', '街頭'),
  makeScene('scene-b', '地下迷宮'),
];

const sceneNames = new Map<string, string>(scenes.map((scene) => [scene.id, scene.name]));
const accessibleSceneIds = new Set<string>(scenes.map((scene) => scene.id));

const movements: SceneMovementEvent[] = [
  {
    characterId: 'c1',
    characterName: '探検家',
    fromSceneId: '/',
    toSceneId: 'scene-a',
    timestamp: 100,
  },
  {
    characterId: 'c1',
    characterName: '探検家',
    fromSceneId: 'scene-a',
    toSceneId: 'scene-b',
    timestamp: 200,
  },
];

const dummyRoom: VisitedRoom = {
  id: '0000000000000273',
  name: 'シーングラフ表示切替テスト',
  url: 'https://alhalte.example/rooms/0000000000000273#secret',
  lastVisited: Date.now(),
  isAdmin: true,
};

describe('Scene Graph Tab — グラフ / リスト (Issue #273)', () => {
  let target: HTMLElement;
  const mounted: unknown[] = [];

  function mountTab(
    overrides: {
      role?: RoomRole;
      scenes?: Scene[];
      currentSceneId?: string;
      movements?: SceneMovementEvent[];
      withScenesProp?: boolean;
      onNavigateToScene?: (sceneId: string) => void;
      onDeleteScene?: (sceneId: string) => void;
      onRequestDeleteScene?: (scene: { id: string; name: string }) => void;
    } = {},
  ): void {
    const {
      role = 'admin',
      scenes: scenesOverride = scenes,
      currentSceneId,
      movements: movementsOverride = movements,
      withScenesProp = true,
      onNavigateToScene,
      onDeleteScene,
      onRequestDeleteScene,
    } = overrides;
    mounted.push(
      mount(SceneGraphTab, {
        target,
        props: {
          movements: movementsOverride,
          sceneNames,
          accessibleSceneIds,
          ...(withScenesProp ? { scenes: scenesOverride } : {}),
          ...(currentSceneId !== undefined ? { currentSceneId } : {}),
          role,
          onNavigateToScene,
          onDeleteScene,
          onRequestDeleteScene,
        },
      }),
    );
  }

  function rows(): Element[] {
    return Array.from(target.querySelectorAll('[data-testid^="scene-list-row-"]'));
  }

  function rowIds(): string[] {
    return rows().map((row) => row.getAttribute('data-testid')?.replace('scene-list-row-', '') ?? '');
  }

  function switchTo(view: 'graph' | 'list'): void {
    target
      .querySelector<HTMLInputElement>(
        `[data-testid="scene-graph-view-segmented"] input[value="${view}"]`,
      )
      ?.click();
  }

  function unmountAll(): void {
    while (mounted.length > 0) unmount(mounted.pop() as never);
    target.remove();
  }

  beforeEach(() => {
    clearAssets();
    resetOutbox();
    localStorage.clear();
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  // spec: Issue #273 グラフ/リストの SegmentedButton を表示し、既定はグラフ
  it('renders the グラフ / リスト SegmentedButton and keeps the graph as the default view', async () => {
    mountTab();
    await settle();

    const tabs = target.querySelector('[data-testid="scene-graph-view-tabs"]');
    expect(tabs).toBeTruthy();
    const segmented = target.querySelector('[data-testid="scene-graph-view-segmented"]');
    expect(segmented).toBeTruthy();

    const inputs = Array.from(segmented?.querySelectorAll<HTMLInputElement>('input') ?? []);
    expect(inputs.map((input) => input.value)).toEqual(['graph', 'list']);
    expect(inputs[0].checked).toBe(true);
    expect(inputs[1].checked).toBe(false);

    // The graph is rendered, the list is not.
    expect(target.querySelector('[data-testid="graph-canvas-container"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="scene-graph-list"]')).toBeNull();

    switchTo('list');
    await settle();
    expect(target.querySelector('[data-testid="scene-graph-list"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="graph-canvas-container"]')).toBeNull();

    switchTo('graph');
    await settle();
    expect(target.querySelector('[data-testid="graph-canvas-container"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="scene-graph-list"]')).toBeNull();

    unmountAll();
  });

  // spec: Issue #273 リストは全シーンを Divider 区切りで表示する
  it('lists every scene in allScenes order with a Divider between rows', async () => {
    mountTab();
    await settle();
    switchTo('list');
    await settle();

    expect(rowIds()).toEqual(['/', 'scene-a', 'scene-b']);

    const list = target.querySelector('[data-testid="scene-graph-list"]');
    expect(list).toBeTruthy();
    // One divider fewer than rows.
    expect(list?.querySelectorAll('.np-divider').length).toBe(scenes.length - 1);

    const names = Array.from(list?.querySelectorAll('[data-testid^="scene-list-item-"]') ?? []).map(
      (element) => element.textContent?.trim(),
    );
    expect(names).toEqual(['メインシーン', '街頭', '地下迷宮']);

    unmountAll();
  });

  // spec: Issue #273 リスト行のクリックで視点を移動する
  it('navigates on a list row click through the same App path a graph node uses (enterScene)', async () => {
    const socket = new FakeRoomSocket();
    mounted.push(
      mount(App, {
        target,
        props: {
          initialRoom: dummyRoom,
          initialScenes: scenes,
          initialMovements: movements,
          isAdmin: true,
          roomSocket: asRoomSocket(socket),
        },
      }),
    );
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-graph"]')?.click();
    await settle(30);

    const before = socket.enterSceneCalls.length;
    switchTo('list');
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="scene-list-item-scene-b"]')?.click();
    await settle();
    expect(socket.enterSceneCalls.slice(before)).toContain('scene-b');

    // The graph node reaches the same callback: both are `handleSelectScene`.
    switchTo('graph');
    await settle();
    const graphLabel = Array.from(
      target.querySelectorAll<HTMLButtonElement>('[data-testid^="node-label-"]'),
    ).find((element) => element.textContent?.includes('地下迷宮'));
    expect(graphLabel).toBeTruthy();
    graphLabel?.click();
    await settle();

    expect(socket.enterSceneCalls.slice(-2)).toEqual(['scene-b', 'scene-b']);

    unmountAll();
  });

  // spec: Issue #273 現在シーンの行を aria-current と check で示す
  it('marks the current scene row with aria-current and a check icon', async () => {
    mountTab({ currentSceneId: 'scene-a' });
    await settle();
    switchTo('list');
    await settle();

    const current = target.querySelector('[data-testid="scene-list-item-scene-a"]');
    expect(current?.getAttribute('aria-current')).toBe('true');
    expect(target.querySelector('[data-testid="scene-list-current-scene-a"]')?.textContent).toContain(
      'check',
    );

    for (const id of ['/', 'scene-b']) {
      const row = target.querySelector(`[data-testid="scene-list-item-${id}"]`);
      expect(row?.getAttribute('aria-current')).toBeNull();
      expect(target.querySelector(`[data-testid="scene-list-current-${id}"]`)).toBeNull();
    }

    unmountAll();
  });

  // spec: Issue #273 リストの削除はルートの確認ダイアログを経由し ack 後に行が消える
  it('deletes a list row through the root confirmation dialog and removes it after the ack', async () => {
    const socket = new FakeRoomSocket();
    mounted.push(
      mount(App, {
        target,
        props: {
          initialRoom: dummyRoom,
          initialScenes: scenes,
          initialMovements: movements,
          isAdmin: true,
          roomSocket: asRoomSocket(socket),
        },
      }),
    );
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-graph"]')?.click();
    await settle(30);
    switchTo('list');
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="scene-list-delete-scene-b"]')?.click();
    await settle(30);

    // Root-hoisted confirmation, not a dialog owned by the tab.
    const rootDialog = target.querySelector('[data-testid="root-delete-scene-dialog"]');
    expect(rootDialog).toBeTruthy();
    expect(target.querySelector('[data-testid="graph-delete-dialog-title"]')?.textContent).toContain(
      'シーン「地下迷宮」を削除しますか？',
    );

    target.querySelector<HTMLButtonElement>('[data-testid="graph-confirm-delete-btn"]')?.click();
    await settle(40);

    expect(socket.requestDeleteSceneCalls).toEqual([{ sceneId: 'scene-b' }]);
    expect(target.querySelector('[data-testid="root-delete-scene-dialog"]')).toBeNull();
    // `App`'s `removeSceneLocally` dropped the acknowledged scene from `allScenes`.
    expect(target.querySelector('[data-testid="scene-list-row-scene-b"]')).toBeNull();
    expect(rowIds()).toEqual(['/', 'scene-a']);

    unmountAll();
  });

  // spec: Issue #273 非管理者には削除ボタンを出さない
  it('hides the trash button from non-admins while rows and navigation keep working', async () => {
    for (const role of ['participant', 'viewer'] as RoomRole[]) {
      const navigated: string[] = [];
      mountTab({ role, onNavigateToScene: (id) => navigated.push(id) });
      await settle();
      switchTo('list');
      await settle();

      expect(target.querySelector('[data-testid^="scene-list-delete-"]')).toBeNull();
      expect(rowIds()).toEqual(['/', 'scene-a', 'scene-b']);

      target.querySelector<HTMLButtonElement>('[data-testid="scene-list-item-scene-a"]')?.click();
      await settle();
      expect(navigated).toEqual(['scene-a']);

      unmount(mounted.pop() as never);
      target.innerHTML = '';
    }

    unmountAll();
  });

  // spec: Issue #273 "/" は削除できない
  it('never offers a trash button for the root scene "/"', async () => {
    const navigated: string[] = [];
    mountTab({ onNavigateToScene: (id) => navigated.push(id) });
    await settle();
    switchTo('list');
    await settle();

    expect(target.querySelector('[data-testid="scene-list-row-/"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="scene-list-delete-/"]')).toBeNull();
    // Other scenes still have one.
    expect(target.querySelector('[data-testid="scene-list-delete-scene-a"]')).toBeTruthy();

    target.querySelector<HTMLButtonElement>('[data-testid="scene-list-item-/"]')?.click();
    await settle();
    expect(navigated).toEqual(['/']);

    unmountAll();
  });

  // spec: Issue #273 リストからグラフへ戻してもグラフの状態が壊れない
  it('keeps the graph node actions and the tab-local confirmation across view switches', async () => {
    mountTab();
    await settle();

    const label = target.querySelector<HTMLButtonElement>('[data-testid="node-label-node-scene-a-1"]');
    expect(label).toBeTruthy();
    label?.click();
    await settle();

    const actions = target.querySelector('[data-testid="node-actions-node-scene-a-1"]');
    expect(actions).toBeTruthy();

    // Open the tab-local fallback dialog (no `onRequestDeleteScene`), then switch.
    target.querySelector<HTMLButtonElement>('[data-testid="node-delete-btn-node-scene-a-1"]')?.click();
    await settle();
    expect(target.querySelector('[data-testid="graph-delete-confirm-dialog"]')).toBeTruthy();

    switchTo('list');
    await settle();
    expect(target.querySelector('[data-testid="scene-graph-list"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="graph-delete-confirm-dialog"]')).toBeTruthy();

    switchTo('graph');
    await settle();
    expect(target.querySelector('[data-testid="graph-canvas-container"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="graph-delete-confirm-dialog"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="node-actions-node-scene-a-1"]')).toBeTruthy();

    target.querySelector<HTMLButtonElement>('[data-testid="graph-cancel-delete-btn"]')?.click();
    await settle();
    expect(target.querySelector('[data-testid="graph-delete-confirm-dialog"]')).toBeNull();

    unmountAll();
  });

  // spec: Issue #273 scenes 未指定時は graph ノードから一覧を導出する
  it('derives a deduped list from the graph nodes when the scenes prop is omitted', async () => {
    const fallbackAccessible = new Set<string>(['scene-a', 'scene-b']);
    const fallbackMovements: SceneMovementEvent[] = [
      { characterId: 'c1', characterName: '探検家', toSceneId: 'scene-a', timestamp: 100 },
      {
        characterId: 'c1',
        characterName: '探検家',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-b',
        timestamp: 200,
      },
      {
        characterId: 'c1',
        characterName: '探検家',
        fromSceneId: 'scene-b',
        toSceneId: 'scene-a',
        timestamp: 300,
      },
      {
        characterId: 'c1',
        characterName: '探検家',
        fromSceneId: 'scene-a',
        toSceneId: 'scene-secret',
        timestamp: 400,
      },
    ];
    mounted.push(
      mount(SceneGraphTab, {
        target,
        props: {
          movements: fallbackMovements,
          sceneNames: new Map([
            ['scene-a', '街頭'],
            ['scene-b', '地下迷宮'],
          ]),
          accessibleSceneIds: fallbackAccessible,
          role: 'participant',
        },
      }),
    );
    await settle();
    switchTo('list');
    await settle();

    // The revisit of scene-a is deduped and the frontier "?" node is dropped.
    expect(rowIds()).toEqual(['scene-a', 'scene-b']);
    expect(target.querySelector('[data-testid="scene-graph-list"]')?.textContent).not.toContain('？');

    unmountAll();
  });

  // spec: Issue #273 シーン一覧が空のときの案内
  it('shows 「シーンがありません」 when the scene list is empty', async () => {
    mountTab({ scenes: [] });
    await settle();
    switchTo('list');
    await settle();

    expect(rows()).toHaveLength(0);
    expect(target.querySelector('[data-testid="empty-scene-list"]')?.textContent).toContain(
      'シーンがありません',
    );

    unmountAll();
  });
});
