/**
 * Reusable scene switcher (Issue #269).
 *
 * The control panel header and the 「キャラクターを移動」 modal share
 * `SceneSwitcher`. These component-level tests pin:
 *
 * - the header-compatible test ids / placeholder defaults,
 * - the Enter resolution order (exact name -> first candidate -> create),
 * - the admin-only create affordance and the hint text swap,
 * - the pending spinner and the failure text of a `false` / throwing callback,
 * - the App-level header path (exact name, create + enter, admin only).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### scene の管理操作`
 * - GitHub Issue #269
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import SceneSwitcher from '../src/components/Control/SceneSwitcher.svelte';
import { clearAssets } from '../src/utils/assets.js';
import { resetOutbox } from '../src/utils/outbox.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { OperationTimeoutError, type RoomSocket } from '../src/utils/roomSocket.js';
import type { RoomRole } from '../src/utils/session.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Scene, VisitedRoom } from '../src/types/index.js';

const ERROR_MESSAGE = 'サーバーと通信できませんでした';

function makeScene(overrides: Partial<Scene> = {}): Scene {
  return {
    id: 'scene-a',
    name: '広場',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
    ...overrides,
  };
}

const SCENES: Scene[] = [
  makeScene({ id: '/', name: '広場' }),
  makeScene({ id: 'scene-b', name: '森' }),
  makeScene({ id: 'scene-c', name: '森の奥' }),
];

function tick(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

describe('SceneSwitcher (Issue #269)', () => {
  let target: HTMLDivElement;
  const mounted: unknown[] = [];

  beforeEach(() => {
    document.body.innerHTML = '';
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    while (mounted.length > 0) unmount(mounted.pop() as never);
    target.remove();
    document.body.innerHTML = '';
  });

  function mountSwitcher(props: Record<string, unknown> = {}) {
    const instance = mount(SceneSwitcher, {
      target,
      props: { scenes: SCENES, currentSceneId: '/', ...props },
    });
    mounted.push(instance);
    return instance;
  }

  function input(): HTMLInputElement {
    const element = target.querySelector<HTMLInputElement>('[data-testid="scene-search-input"]');
    if (!element) throw new Error('scene search input is missing');
    return element;
  }

  /** Types into the search field and presses Enter (Search's `onsearch`). */
  async function pressEnter(value: string): Promise<void> {
    const element = input();
    element.focus();
    element.value = value;
    element.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await tick(30);
  }

  // spec: Issue #269 ヘッダー互換の testid / placeholder を既定にする
  it('renders the header-compatible test ids, placeholder and candidates', async () => {
    mountSwitcher({ placeholder: 'シーンを検索' });

    expect(input().getAttribute('placeholder')).toBe('シーンを検索');
    expect(input().getAttribute('aria-label')).toBe('シーンを検索・切り替え');
    expect(target.querySelector('[data-testid="scene-search-menu"]')).toBeTruthy();
    for (const scene of SCENES) {
      expect(
        target.querySelector(`[data-testid="scene-search-item-${scene.id}"]`)?.textContent,
      ).toContain(scene.name);
    }
    // The current scene is the selected row (selected class + check icon).
    const current = target.querySelector('[data-testid="scene-search-item-/"]');
    expect(current?.classList.contains('selected')).toBe(true);
    expect(current?.querySelector('.np-item-start .np-icon')?.textContent?.trim()).toBe('check');
  });

  // spec: Issue #269 検索語で候補を絞り込む
  it('filters the candidates by the typed query', async () => {
    mountSwitcher();

    const element = input();
    element.value = '森';
    element.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();

    expect(target.querySelector('[data-testid="scene-search-item-scene-b"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="scene-search-item-scene-c"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="scene-search-item-/"]')).toBeNull();
  });

  // spec: Issue #269 Enter は完全一致のシーンを選ぶ（大文字小文字・前後空白を無視）
  it('selects the exact scene name on Enter (trimmed, case-insensitive)', async () => {
    const selected: string[] = [];
    mountSwitcher({ onSelectScene: (sceneId: string) => void selected.push(sceneId) });

    await pressEnter('  森  ');

    expect(selected).toEqual(['scene-b']);
    // The search collapses after a successful selection.
    expect(input().value).toBe('');
  });

  // spec: Issue #269 完全一致が無ければ最初の候補を選ぶ
  it('selects the first candidate when no name matches exactly', async () => {
    const selected: string[] = [];
    mountSwitcher({ onSelectScene: (sceneId: string) => void selected.push(sceneId) });

    // Both 「森」 and 「森の奥」 contain the query; the first candidate wins.
    await pressEnter('森の');

    expect(selected).toEqual(['scene-c']);
  });

  // spec: Issue #269 候補が無く canCreate なら新規 id で作成する
  it('creates a scene with a generated id when nothing matches and canCreate is set', async () => {
    const created: Array<{ name: string; sceneId: string }> = [];
    mountSwitcher({
      canCreate: true,
      onCreateScene: (name: string, sceneId: string) => {
        created.push({ name, sceneId });
        return true;
      },
    });

    await pressEnter('新しい間');

    expect(created).toHaveLength(1);
    expect(created[0].name).toBe('新しい間');
    expect(created[0].sceneId).toMatch(/^scene-\d+-[a-z0-9]+$/);
  });

  // spec: Issue #269 canCreate が無ければ作成せず、該当なしの案内を出す
  it('keeps the plain empty notice and creates nothing without canCreate', async () => {
    const created: unknown[] = [];
    mountSwitcher({ onCreateScene: (...args: unknown[]) => void created.push(args) });

    await pressEnter('存在しない間');

    expect(created).toEqual([]);
    const notice = target.querySelector('[data-testid="scene-search-empty-notice"]');
    expect(notice?.textContent?.trim()).toBe('該当するシーンがありません');
    expect(target.querySelector('[data-testid="scene-search-create-item"]')).toBeNull();
  });

  // spec: Issue #269 canCreate のときは「シーンは新規作成されます」に差し替える
  it('replaces the empty notice with the create hint for an admin', async () => {
    const created: string[] = [];
    mountSwitcher({
      canCreate: true,
      onCreateScene: (name: string) => {
        created.push(name);
        return true;
      },
    });

    // While typing, the hint and the explicit create row are visible.
    const element = input();
    element.value = '存在しない間';
    element.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();

    const notice = target.querySelector('[data-testid="scene-search-empty-notice"]');
    expect(notice?.textContent?.trim()).toBe('シーンは新規作成されます');
    const createItem = target.querySelector('[data-testid="scene-search-create-item"]');
    expect(createItem?.textContent).toContain('「存在しない間」を新規作成');

    // Enter creates it (and the search collapses afterwards).
    element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await tick(30);
    expect(created).toEqual(['存在しない間']);
  });

  // spec: Issue #269 新規作成の行をクリックしても作成できる
  it('creates the typed scene when the create row is clicked', async () => {
    const created: string[] = [];
    mountSwitcher({
      canCreate: true,
      onCreateScene: (name: string) => {
        created.push(name);
        return true;
      },
    });

    const element = input();
    element.value = '屋上';
    element.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    target.querySelector<HTMLElement>('[data-testid="scene-search-create-item"]')?.click();
    await tick(30);

    expect(created).toEqual(['屋上']);
  });

  // spec: Issue #269 候補の行クリックで切り替える
  it('selects the scene of a clicked candidate row', async () => {
    const selected: string[] = [];
    mountSwitcher({ onSelectScene: (sceneId: string) => void selected.push(sceneId) });

    target.querySelector<HTMLElement>('[data-testid="scene-search-item-scene-b"]')?.click();
    await tick(30);

    expect(selected).toEqual(['scene-b']);
  });

  // spec: Issue #269 失敗は サーバーと通信できませんでした を role=alert で出す
  it('reports a callback that returns false as a failure without clearing the query', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mountSwitcher({ onSelectScene: () => false });

    await pressEnter('森');

    const error = target.querySelector('[data-testid="scene-search-error"]');
    expect(error).toBeTruthy();
    expect(error?.getAttribute('role')).toBe('alert');
    expect(error?.textContent?.trim()).toBe('サーバーと通信できませんでした');

    warn.mockRestore();
  });

  // spec: Issue #269 例外も同じエラー文言にする
  it('reports a throwing callback with the same message', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mountSwitcher({
      onSelectScene: () => {
        throw new Error('OperationTimeoutError');
      },
    });

    await pressEnter('森');

    expect(
      target.querySelector('[data-testid="scene-search-error"]')?.textContent?.trim(),
    ).toBe('サーバーと通信できませんでした');

    warn.mockRestore();
  });

  // spec: Issue #269 送信中はスピナーを出し、二重送信しない
  it('shows the pending spinner and ignores a second Enter while waiting', async () => {
    let resolveSelect!: (value: boolean) => void;
    const request = new Promise<boolean>((resolve) => {
      resolveSelect = resolve;
    });
    const onSelectScene = vi.fn(() => request);
    mountSwitcher({ onSelectScene });

    await pressEnter('森');
    expect(target.querySelector('[data-testid="scene-search-pending"]')).toBeTruthy();

    // A second Enter while pending is ignored.
    input().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await tick();
    expect(onSelectScene).toHaveBeenCalledTimes(1);

    resolveSelect(true);
    await tick(30);
    expect(target.querySelector('[data-testid="scene-search-pending"]')).toBeNull();
  });

  // spec: Issue #269 resolve() は Enter と同じ経路（モーダルの「移動」用）
  it('resolve() follows the same order as Enter', async () => {
    const selected: string[] = [];
    const instance = mountSwitcher({
      onSelectScene: (sceneId: string) => void selected.push(sceneId),
    });

    const element = input();
    element.value = '広場';
    element.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    await (instance as unknown as { resolve: () => Promise<boolean> }).resolve();

    expect(selected).toEqual(['/']);
  });
});

/**
 * Header integration (Issue #269): the control panel header's scene search is the
 * shared switcher, and Enter on an unknown name creates + selects + enters the
 * scene for an admin only.
 */
describe('Header scene switcher App integration (Issue #269)', () => {
  const ROOM: VisitedRoom = {
    id: 'aabbccddeeff0269',
    name: 'ヘッダーシーン検索テスト部屋',
    url: 'https://alhalte.example/rooms/aabbccddeeff0269#key',
    lastVisited: Date.now(),
  };

  let target: HTMLDivElement;
  let app: unknown = null;

  beforeEach(() => {
    clearAssets();
    resetOutbox();
    document.body.innerHTML = '';
    localStorage.clear();
    localStorage.setItem('alhalte:account-id', '1001');
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    if (app) {
      unmount(app as never);
      app = null;
    }
    target.remove();
    document.body.innerHTML = '';
    localStorage.clear();
    vi.restoreAllMocks();
  });

  function mountApp(socket: FakeRoomSocket, role: RoomRole) {
    app = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [
          makeScene({ id: '/', name: '広場', members: [] }),
          makeScene({ id: 'scene-b', name: '森', members: [] }),
        ],
        initialCharacters: [],
        userRole: role,
        roomSocket: asRoomSocket(socket) as RoomSocket,
      },
    });
    return app;
  }

  function sceneName(): string {
    return (
      target.querySelector('[data-testid="scene-name-header"]')?.textContent?.trim() ?? ''
    );
  }

  async function typeHeaderSearch(value: string): Promise<HTMLInputElement> {
    const input = target.querySelector<HTMLInputElement>('[data-testid="scene-search-input"]');
    if (!input) throw new Error('header scene search is missing');
    input.focus();
    input.value = value;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    return input;
  }

  // spec: Issue #269 ヘッダーの Enter は完全一致 / 先頭候補のシーンへ切り替える
  it('switches to the exact scene on Enter', async () => {
    const socket = new FakeRoomSocket();
    mountApp(socket, 'admin');
    await tick(30);

    const input = await typeHeaderSearch('森');
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await tick(40);

    expect(sceneName()).toBe('森');
    expect(socket.enterSceneCalls).toContain('scene-b');
    expect(socket.requestCreateSceneCalls).toHaveLength(0);
  });

  // spec: Issue #269 管理人の Enter は未知の名前を新規作成し、ack 後に切り替えて入室する
  it('creates the scene for an unknown name and enters it after the ack', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    mountApp(socket, 'admin');
    await tick(30);

    const input = await typeHeaderSearch('地下迷宮');
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await tick(40);

    expect(socket.requestCreateSceneCalls).toHaveLength(1);
    const created = socket.requestCreateSceneCalls[0];
    expect(created.name).toBe('地下迷宮');
    expect(created.inheritJuices).toBe(true);
    expect(created.sceneId).toMatch(/^scene-\d+-[a-z0-9]+$/);
    // Nothing is added or switched before the ack.
    expect(sceneName()).toBe('広場');
    expect(socket.enterSceneCalls).not.toContain(created.sceneId);
    expect(target.querySelector('[data-testid="scene-search-pending"]')).toBeTruthy();

    socket.resolveSceneAction('create');
    await tick(40);

    // The header now shows the new scene and the field entered it.
    expect(sceneName()).toBe('地下迷宮');
    expect(socket.enterSceneCalls).toContain(created.sceneId);
    expect(target.querySelector('[data-testid="scene-search-pending"]')).toBeNull();
    expect(target.querySelector('[data-testid="scene-search-error"]')).toBeNull();
  });

  // spec: Issue #269 参加者には新規作成の導線を出さず、CreateScene も送らない
  it('offers no create affordance to a participant', async () => {
    const socket = new FakeRoomSocket();
    mountApp(socket, 'participant');
    await tick(30);

    const input = await typeHeaderSearch('存在しない間');
    expect(
      target.querySelector('[data-testid="empty-search-notice"]')?.textContent?.trim(),
    ).toBe('該当するシーンがありません');
    expect(target.querySelector('[data-testid="scene-search-create-item"]')).toBeNull();

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await tick(40);

    expect(socket.requestCreateSceneCalls).toHaveLength(0);
    // No local scene was invented and the current scene is unchanged.
    expect(sceneName()).toBe('広場');
    input.value = '';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    expect(target.querySelector('[data-testid="scene-search-item-scene-b"]')).toBeTruthy();
  });

  // spec: Issue #269 作成失敗はヘッダーのスイッチャーにエラーを出し、シーンを追加しない
  it('reports a failed create in the switcher and adds no scene', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    mountApp(socket, 'admin');
    await tick(30);

    const input = await typeHeaderSearch('失敗する間');
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await tick(30);

    const requestedId = socket.requestCreateSceneCalls[0].sceneId;
    socket.rejectSceneAction(new OperationTimeoutError('createScene', 8000), 'create');
    await tick(40);

    expect(
      target.querySelector('[data-testid="scene-search-error"]')?.textContent?.trim(),
    ).toBe(ERROR_MESSAGE);
    expect(sceneName()).toBe('広場');
    expect(socket.enterSceneCalls).not.toContain(requestedId);

    // The local scene list is untouched: only the two initial scenes exist.
    input.value = '';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    expect(target.querySelector('[data-testid="scene-search-item-scene-b"]')).toBeTruthy();
    expect(target.querySelector(`[data-testid="scene-search-item-${requestedId}"]`)).toBeNull();

    warn.mockRestore();
  });
});
