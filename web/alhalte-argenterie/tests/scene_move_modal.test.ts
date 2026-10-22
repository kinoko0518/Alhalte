/**
 * 「キャラクターを移動」 modal (Issue #269).
 *
 * Component-level tests pin the modal's own DOM (title, switcher, cancel /
 * 移動, failure text) and that 「移動」 is exactly the switcher's Enter path.
 * App-level tests cover the server round trips:
 *
 * - existing name → `MoveCharacter` only (no `CreateScene`),
 * - unknown name → `CreateScene` ack, then `MoveCharacter` ack,
 * - the current scene never changes (Issue #228),
 * - the modal is root-hoisted (outside the control panel / bottom sheet),
 * - a failure leaves the model untouched and keeps the modal open.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### scene の管理操作`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### キャラクター同期`
 * - GitHub Issue #269
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import SceneMoveModal from '../src/components/Control/SceneMoveModal.svelte';
import { clearAssets } from '../src/utils/assets.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { resetOutbox } from '../src/utils/outbox.js';
import { OperationTimeoutError, type RoomSocket } from '../src/utils/roomSocket.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

const ACCOUNT = '1001';
const ERROR_MESSAGE = 'サーバーと通信できませんでした';

function tick(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function makeScene(overrides: Partial<Scene> = {}): Scene {
  return {
    id: '/',
    name: '広場',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
    ...overrides,
  };
}

function makeCharacter(overrides: Partial<Character> = {}): Character {
  return {
    id: '42',
    name: '勇者ロラン',
    initiative: 10,
    description: 'テスト用',
    statuses: [],
    parameters: [],
    chatPalette: '',
    hideStatus: false,
    placedAsPiece: false,
    ...overrides,
  };
}

const SCENES: Scene[] = [
  makeScene({ id: '/', name: '広場', members: ['42'] }),
  makeScene({ id: 'scene-b', name: '森', members: [] }),
];

describe('SceneMoveModal (Issue #269)', () => {
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

  function mountModal(props: Record<string, unknown> = {}) {
    const instance = mount(SceneMoveModal, {
      target,
      props: {
        character: makeCharacter(),
        scenes: SCENES,
        characterSceneId: '/',
        canCreate: true,
        ...props,
      },
    });
    mounted.push(instance);
    return instance;
  }

  function modalInput(): HTMLInputElement {
    const element = target.querySelector<HTMLInputElement>(
      '[data-testid="move-scene-search-input"]',
    );
    if (!element) throw new Error('move modal search input is missing');
    return element;
  }

  async function typeAndEnter(value: string): Promise<void> {
    const element = modalInput();
    element.focus();
    element.value = value;
    element.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await tick(30);
  }

  // spec: Issue #269 モーダルはタイトル・移動対象・共有スイッチャー・操作行を持つ
  it('renders the title, the character and the shared scene switcher', () => {
    mountModal();

    expect(
      target.querySelector('[data-testid="move-character-modal-title"]')?.textContent?.trim(),
    ).toBe('キャラクターを移動');
    expect(
      target.querySelector('[data-testid="move-character-modal-description"]')?.textContent,
    ).toContain('勇者ロラン');
    expect(target.querySelector('[data-testid="move-character-modal"]')?.getAttribute('role')).toBe(
      'dialog',
    );
    expect(modalInput()).toBeTruthy();
    expect(target.querySelector('[data-testid="move-scene-search-menu"]')).toBeTruthy();
    // Every scene is listed, including the current one (Issue #269).
    expect(target.querySelector('[data-testid="move-scene-search-item-/"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="move-scene-search-item-scene-b"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="move-character-confirm"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="move-character-cancel"]')).toBeTruthy();
  });

  // spec: Issue #269 Enter と「移動」は同じ解決経路
  it('uses the same selection path for Enter and the 移動 button', async () => {
    const selected: string[] = [];
    mountModal({
      onSelectScene: (sceneId: string) => {
        selected.push(sceneId);
      },
    });

    await typeAndEnter('森');
    expect(selected).toEqual(['scene-b']);

    // 「移動」 goes through switcher.resolve(), i.e. the same Enter logic.
    const element = modalInput();
    element.value = '広場';
    element.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    target.querySelector<HTMLElement>('[data-testid="move-character-confirm"]')?.click();
    await tick(30);

    expect(selected).toEqual(['scene-b', '/']);
  });

  // spec: Issue #269 不明な名前はスイッチャー生成 id で作成する
  it('creates the typed scene with a generated id through the switcher', async () => {
    const created: Array<{ name: string; sceneId: string }> = [];
    mountModal({
      onCreateScene: (name: string, sceneId: string) => {
        created.push({ name, sceneId });
      },
    });

    await typeAndEnter('地下迷宮');

    expect(created).toHaveLength(1);
    expect(created[0].name).toBe('地下迷宮');
    expect(created[0].sceneId).toMatch(/^scene-\d+-[a-z0-9]+$/);
  });

  // spec: Issue #269 失敗はモーダル内に サーバーと通信できませんでした を出す
  it('shows the failure inside the switcher and stays open', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const onClose = vi.fn();
    mountModal({ onSelectScene: () => false, onClose });

    await typeAndEnter('森');

    const error = target.querySelector('[data-testid="move-scene-search-error"]');
    expect(error?.getAttribute('role')).toBe('alert');
    expect(error?.textContent?.trim()).toBe(ERROR_MESSAGE);
    expect(onClose).not.toHaveBeenCalled();
    expect(target.querySelector('[data-testid="move-character-modal"]')).toBeTruthy();

    warn.mockRestore();
  });

  // spec: Issue #269 参加者向けに新規作成の案内は出さない
  it('hides the create affordance when canCreate is false', async () => {
    mountModal({ canCreate: false });

    const element = modalInput();
    element.value = '存在しない間';
    element.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();

    expect(
      target.querySelector('[data-testid="move-scene-search-empty-notice"]')?.textContent?.trim(),
    ).toBe('該当するシーンがありません');
    expect(target.querySelector('[data-testid="move-scene-search-create-item"]')).toBeNull();
  });

  // spec: Issue #269 キャンセルはモーダルを閉じるだけ
  it('closes without moving anything on キャンセル', async () => {
    const onClose = vi.fn();
    const onSelectScene = vi.fn();
    mountModal({ onClose, onSelectScene });

    target.querySelector<HTMLElement>('[data-testid="move-character-cancel"]')?.click();
    await tick();

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onSelectScene).not.toHaveBeenCalled();
  });
});

describe('Character move App integration (Issue #269)', () => {
  const ROOM: VisitedRoom = {
    id: 'aabbccddeeff0269',
    name: 'キャラクター移動テスト部屋',
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
    localStorage.setItem('alhalte:account-id', ACCOUNT);
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

  function mountApp(socket: FakeRoomSocket, props: Record<string, unknown> = {}) {
    app = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [
          makeScene({ id: '/', name: '広場', members: ['42'] }),
          makeScene({ id: 'scene-b', name: '森', members: [] }),
        ],
        initialCharacters: [makeCharacter({ id: '42', ownerAccountId: ACCOUNT })],
        userRole: 'admin',
        roomSocket: asRoomSocket(socket) as RoomSocket,
        ...props,
      },
    });
    return app;
  }

  function card(characterId: string): HTMLElement | null {
    return target.querySelector<HTMLElement>(`[data-testid="character-card-${characterId}"]`);
  }

  async function openCharacters(): Promise<void> {
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-characters"]')?.click();
    await tick(40);
  }

  async function openMoveModal(characterId = '42'): Promise<HTMLInputElement> {
    target.querySelector<HTMLButtonElement>(`[data-testid="move-char-btn-${characterId}"]`)?.click();
    await tick(30);
    const input = target.querySelector<HTMLInputElement>('[data-testid="move-scene-search-input"]');
    if (!input) throw new Error('move modal did not open');
    return input;
  }

  async function typeInto(input: HTMLInputElement, value: string): Promise<void> {
    input.focus();
    input.value = value;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
  }

  async function pressEnter(input: HTMLInputElement): Promise<void> {
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await tick(40);
  }

  function sceneName(): string {
    return target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]')?.value ?? '';
  }

  async function openSettings(): Promise<void> {
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
    await tick(40);
  }

  // spec: Issue #269 既存シーン名は CreateScene なしで移動し、現在シーンは変えない
  it('moves to an existing scene without creating one and keeps the current scene', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    socket.characterListSnapshot = [makeCharacter({ id: '42', ownerAccountId: ACCOUNT })];
    mountApp(socket);
    await tick(30);
    await openCharacters();

    const input = await openMoveModal();
    await typeInto(input, '森');
    await pressEnter(input);

    expect(socket.requestMoveCharacterCalls).toEqual([{ characterId: 42, toSceneId: 'scene-b' }]);
    expect(socket.requestCreateSceneCalls).toHaveLength(0);
    // The acknowledgement is still pending: nothing is committed yet.
    expect(target.querySelector('[data-testid="move-char-pending-42"]')).toBeTruthy();
    expect(card('42')).toBeTruthy();
    expect(target.querySelector('[data-testid="move-character-modal"]')).toBeTruthy();

    socket.resolveSceneAction('move');
    await tick(40);

    // The modal closes, the row moves, the admin's scene is unchanged.
    expect(target.querySelector('[data-testid="move-character-modal"]')).toBeNull();
    expect(target.querySelector('[data-testid="move-char-pending-42"]')).toBeNull();
    expect(card('42')).toBeNull();
    await openSettings();
    expect(sceneName()).toBe('広場');
    expect(socket.enterSceneCalls).not.toContain('scene-b');
  });

  // spec: Issue #269 未知の名前は CreateScene ack → MoveCharacter ack の順で送る
  it('creates the scene first and then moves the character (both acks)', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    socket.characterListSnapshot = [makeCharacter({ id: '42', ownerAccountId: ACCOUNT })];
    mountApp(socket);
    await tick(30);
    await openCharacters();

    const input = await openMoveModal();
    await typeInto(input, '地下迷宮');
    await pressEnter(input);

    // CreateScene first, generated id, Juices inherited from the current scene.
    expect(socket.requestCreateSceneCalls).toHaveLength(1);
    const created = socket.requestCreateSceneCalls[0];
    expect(created.name).toBe('地下迷宮');
    expect(created.inheritJuices).toBe(true);
    expect(created.sceneId).toMatch(/^scene-\d+-[a-z0-9]+$/);
    // The move is not sent before the create is acknowledged.
    expect(socket.requestMoveCharacterCalls).toHaveLength(0);
    expect(target.querySelector('[data-testid="move-scene-search-pending"]')).toBeTruthy();

    socket.resolveSceneAction('create');
    await tick(30);
    expect(socket.requestMoveCharacterCalls).toEqual([
      { characterId: 42, toSceneId: created.sceneId },
    ]);
    // Still waiting for the move ack: nothing is committed.
    expect(card('42')).toBeTruthy();

    socket.resolveSceneAction('move');
    await tick(40);

    expect(target.querySelector('[data-testid="move-character-modal"]')).toBeNull();
    expect(card('42')).toBeNull();
    // The new scene is in the local list with the acknowledged name.
    const headerInput = target.querySelector<HTMLInputElement>(
      '[data-testid="scene-search-input"]',
    )!;
    headerInput.focus();
    await tick(20);
    expect(
      target.querySelector(`[data-testid="scene-search-item-${created.sceneId}"]`)?.textContent,
    ).toContain('地下迷宮');
    // The move never switches the admin's scene (Issue #228).
    await openSettings();
    expect(sceneName()).toBe('広場');
    expect(socket.enterSceneCalls).not.toContain(created.sceneId);
  });

  // spec: Issue #269 「移動」ボタンは Enter と同じ経路（既存シーン）
  it('moves through the 移動 button exactly like Enter', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [makeCharacter({ id: '42', ownerAccountId: ACCOUNT })];
    mountApp(socket);
    await tick(30);
    await openCharacters();

    const input = await openMoveModal();
    await typeInto(input, '森');
    target.querySelector<HTMLElement>('[data-testid="move-character-confirm"]')?.click();
    await tick(40);

    expect(socket.requestMoveCharacterCalls).toEqual([{ characterId: 42, toSceneId: 'scene-b' }]);
    expect(socket.requestCreateSceneCalls).toHaveLength(0);
    expect(card('42')).toBeNull();
    expect(target.querySelector('[data-testid="move-character-modal"]')).toBeNull();
  });

  // spec: Issue #269 移動失敗はローカル状態と現在シーンを変えずモーダルにエラーを出す
  it('keeps the model and the current scene when the move fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    socket.characterListSnapshot = [makeCharacter({ id: '42', ownerAccountId: ACCOUNT })];
    mountApp(socket);
    await tick(30);
    await openCharacters();

    const input = await openMoveModal();
    await typeInto(input, '地下迷宮');
    await pressEnter(input);
    socket.resolveSceneAction('create');
    await tick(30);
    socket.rejectSceneAction(new OperationTimeoutError('moveCharacter', 8000), 'move');
    await tick(40);

    expect(
      target.querySelector('[data-testid="move-scene-search-error"]')?.textContent?.trim(),
    ).toBe(ERROR_MESSAGE);
    // The modal stays open for a retry, the character stays put and no scene was
    // added locally beyond the acknowledged create.
    expect(target.querySelector('[data-testid="move-character-modal"]')).toBeTruthy();
    expect(card('42')).toBeTruthy();
    await openSettings();
    expect(sceneName()).toBe('広場');

    warn.mockRestore();
  });

  // spec: Issue #269 現在シーンを選んでも MoveCharacter を送らない
  it('treats the character current scene as a no-op', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [makeCharacter({ id: '42', ownerAccountId: ACCOUNT })];
    mountApp(socket);
    await tick(30);
    await openCharacters();

    const input = await openMoveModal();
    await typeInto(input, '広場');
    await pressEnter(input);

    expect(socket.requestMoveCharacterCalls).toHaveLength(0);
    expect(socket.requestCreateSceneCalls).toHaveLength(0);
    expect(target.querySelector('[data-testid="move-character-modal"]')).toBeNull();
    expect(card('42')).toBeTruthy();
  });

  // spec: Issue #269 縦画面でも移動モーダルはルート直下（シートの外）に出す
  it('hoists the modal to the root in the portrait layout', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 420 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 900 });
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [makeCharacter({ id: '42', ownerAccountId: ACCOUNT })];
    mountApp(socket);
    await tick(40);
    await openCharacters();

    const input = await openMoveModal('42');
    expect(input).toBeTruthy();

    const modal = target.querySelector('[data-testid="move-character-modal"]');
    const controlPanel = target.querySelector('[data-testid="control-panel"]');
    const bottomSheet = target.querySelector('[data-testid="portrait-bottom-sheet"]');
    expect(modal).toBeTruthy();
    expect(controlPanel).toBeTruthy();
    expect(bottomSheet).toBeTruthy();
    expect(controlPanel?.contains(modal)).toBe(false);
    expect(bottomSheet?.contains(modal)).toBe(false);
    // The modal is rendered inside the root wrapper, next to the App's overlays.
    const wrapper = target.querySelector('[data-testid="root-scene-move-wrapper"]');
    expect(wrapper?.contains(modal)).toBe(true);
  });

  // spec: Issue #269 参加者・閲覧者には移動ボタンを出さない
  it('does not offer the move button to a participant or a viewer', async () => {
    for (const role of ['participant', 'viewer'] as const) {
      const socket = new FakeRoomSocket();
      socket.characterListSnapshot = [makeCharacter({ id: '42', ownerAccountId: ACCOUNT })];
      mountApp(socket, { userRole: role });
      await tick(30);
      await openCharacters();

      expect(target.querySelector('[data-testid="move-char-btn-42"]')).toBeNull();

      unmount(app as never);
      app = null;
      document.body.innerHTML = '';
      target = document.createElement('div');
      document.body.appendChild(target);
    }
  });
});
