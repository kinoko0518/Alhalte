/**
 * Character list views, row actions and role matrix (Issue #228).
 *
 * The component-level tests mount `CharacterManagementTab` in isolation and
 * assert the exact DOM order of the row actions for every role / view /
 * ownership combination. The App-level tests cover the server round trip
 * (ack-only piece flip, focus scene switch, move ack / failure, delete via
 * その他).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### キャラクター同期`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
 * - GitHub Issue #228
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import CharacterManagementTab from '../src/components/Control/tabs/CharacterManagementTab.svelte';
import { clearAssets } from '../src/utils/assets.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { resetOutbox } from '../src/utils/outbox.js';
import {
  OperationTimeoutError,
  type RoomSocket,
} from '../src/utils/roomSocket.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';
import type { RoomRole } from '../src/utils/session.js';

const ACCOUNT = '1001';
const OTHER_ACCOUNT = '2002';

function tick(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

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

function makeCharacter(overrides: Partial<Character> = {}): Character {
  return {
    id: 'own-in-scene',
    name: 'アリス',
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

/** Room / character fixtures shared by the component-level matrix. */
const SCENE_A = makeScene({
  id: 'scene-a',
  name: '広場',
  members: ['own-in-scene', 'other-in-scene', 'unowned-in-scene', 'own-placed'],
});
const SCENE_B = makeScene({
  id: 'scene-b',
  name: '森',
  members: ['own-elsewhere', 'other-elsewhere', 'unowned-elsewhere'],
});
const SCENE_C = makeScene({ id: 'scene-c', name: ' 森 ', members: [] });

const CHARACTERS: Character[] = [
  makeCharacter({ id: 'own-in-scene', name: 'アリス', ownerAccountId: ACCOUNT }),
  makeCharacter({ id: 'other-in-scene', name: 'ボブ', ownerAccountId: OTHER_ACCOUNT }),
  makeCharacter({ id: 'unowned-in-scene', name: '名前なし' }),
  makeCharacter({
    id: 'own-placed',
    name: '置かれたアリス',
    ownerAccountId: ACCOUNT,
    placedAsPiece: true,
    piecePosition: { x: 10, y: 20 },
  }),
  makeCharacter({ id: 'own-elsewhere', name: '森のアリス', ownerAccountId: ACCOUNT }),
  makeCharacter({ id: 'other-elsewhere', name: '森のボブ', ownerAccountId: OTHER_ACCOUNT }),
  makeCharacter({ id: 'unowned-elsewhere', name: '森の名前なし' }),
  makeCharacter({ id: 'own-nowhere', name: '未所属アリス', ownerAccountId: ACCOUNT }),
];

describe('Character list views and row actions (Issue #228)', () => {
  let target: HTMLDivElement;
  const mounted: unknown[] = [];

  beforeEach(() => {
    clearAssets();
    resetOutbox();
    document.body.innerHTML = '';
    localStorage.clear();
    localStorage.setItem('alhalte:account-id', ACCOUNT);
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    while (mounted.length > 0) unmount(mounted.pop() as never);
    target.remove();
    document.body.innerHTML = '';
    localStorage.clear();
  });

  function mountTab(props: Record<string, unknown> = {}) {
    const instance = mount(CharacterManagementTab, {
      target,
      props: {
        characters: CHARACTERS,
        allScenes: [SCENE_A, SCENE_B, SCENE_C],
        currentScene: SCENE_A,
        currentAccountId: ACCOUNT,
        role: 'admin',
        ...props,
      },
    });
    mounted.push(instance);
    return instance;
  }

  /** `data-testid`s of the direct row action buttons, in DOM order. */
  function actionIds(characterId: string): string[] {
    const container = target.querySelector(
      `[data-testid="character-card-${characterId}"] .char-actions`,
    );
    if (!container) return [];
    return Array.from(container.children)
      .filter(
        (element): element is HTMLElement =>
          element instanceof HTMLElement && element.classList.contains('np-icon-button'),
      )
      .map((element) => element.getAttribute('data-testid') ?? '')
      .filter((value) => value.length > 0);
  }

  function rowIds(): string[] {
    return Array.from(target.querySelectorAll('[data-testid^="character-card-"]')).map(
      (element) => element.getAttribute('data-testid')?.replace('character-card-', '') ?? '',
    );
  }

  function segmented(): HTMLElement | null {
    return target.querySelector<HTMLElement>('[data-testid="character-view-segmented"]');
  }

  function segmentedValues(): string[] {
    return Array.from(segmented()?.querySelectorAll('input') ?? []).map(
      (input) => (input as HTMLInputElement).value,
    );
  }

  function segmentedLabels(): string[] {
    return Array.from(segmented()?.querySelectorAll('label') ?? []).map(
      (label) => label.textContent?.trim() ?? '',
    );
  }

  /** Text of the noph-ui Tooltip referenced by the button (`title` prop). */
  function tooltipText(button: Element | null | undefined): string | undefined {
    const id = button?.getAttribute('aria-describedby');
    if (!id) return undefined;
    return document.getElementById(id)?.textContent?.trim();
  }

  async function switchView(value: 'scene' | 'mine' | 'all'): Promise<void> {
    segmented()?.querySelector<HTMLInputElement>(`input[value="${value}"]`)?.click();
    await tick();
  }

  // spec: Issue #228 閲覧者には Segmented button を出さず、行だけを表示する
  it('hides the segmented button for a viewer and renders rows without actions', () => {
    mountTab({ role: 'viewer' });

    expect(target.querySelector('[data-testid="character-view-tabs"]')).toBeNull();
    expect(segmented()).toBeNull();
    // Viewer only has the scene view: the four scene-A rows.
    expect(rowIds()).toEqual([
      'own-in-scene',
      'other-in-scene',
      'unowned-in-scene',
      'own-placed',
    ]);
    for (const id of rowIds()) {
      expect(actionIds(id)).toEqual([]);
    }
  });

  // spec: Issue #228 参加者/管理者の Segmented button 選択肢とラベル
  it('offers the scene / mine views to a participant and all three to an admin', () => {
    mountTab({ role: 'participant' });
    expect(target.querySelector('[data-testid="character-view-tabs"]')).toBeTruthy();
    expect(segmentedValues()).toEqual(['scene', 'mine']);
    expect(segmentedLabels()).toEqual(['シーン内のキャラクター', '自分のキャラクター']);
    expect(
      segmented()?.querySelector<HTMLInputElement>('input[value="scene"]')?.checked,
    ).toBe(true);

    unmount(mounted.pop() as never);
    target.innerHTML = '';

    mountTab({ role: 'admin' });
    expect(segmentedValues()).toEqual(['scene', 'mine', 'all']);
    expect(segmentedLabels()).toEqual([
      'シーン内のキャラクター',
      '自分のキャラクター',
      'すべてのキャラクター',
    ]);
  });

  // spec: Issue #228 シーン内のキャラクターの既定表示と行アクション
  it('defaults to the scene view and applies the exact per-role action arrays', () => {
    mountTab({ role: 'participant' });
    expect(rowIds()).toEqual([
      'own-in-scene',
      'other-in-scene',
      'unowned-in-scene',
      'own-placed',
    ]);
    // Participant: only own characters get the piece toggle and その他.
    expect(actionIds('own-in-scene')).toEqual([
      'piece-char-btn-own-in-scene',
      'more-char-btn-own-in-scene',
    ]);
    expect(actionIds('own-placed')).toEqual([
      'piece-char-btn-own-placed',
      'more-char-btn-own-placed',
    ]);
    expect(actionIds('other-in-scene')).toEqual([]);
    expect(actionIds('unowned-in-scene')).toEqual([]);
    // No 別シーン / 視点 in the scene view for a participant.
    expect(target.querySelector('[data-testid="move-char-btn-own-in-scene"]')).toBeNull();
    expect(target.querySelector('[data-testid="focus-char-btn-own-in-scene"]')).toBeNull();

    unmount(mounted.pop() as never);
    target.innerHTML = '';

    // Admin: every scene row gets 別シーン → 駒 → その他, in that order.
    mountTab({ role: 'admin' });
    for (const id of [
      'own-in-scene',
      'other-in-scene',
      'unowned-in-scene',
      'own-placed',
    ]) {
      expect(actionIds(id)).toEqual([
        `move-char-btn-${id}`,
        `piece-char-btn-${id}`,
        `more-char-btn-${id}`,
      ]);
    }
  });

  // spec: Issue #228 自分のキャラクター（参加者）
  it('shows a participant only their own characters with その他 and 視点を合わせる outside the scene', async () => {
    mountTab({ role: 'participant' });
    await switchView('mine');

    // Only owned characters, in list order.
    expect(rowIds()).toEqual([
      'own-in-scene',
      'own-placed',
      'own-elsewhere',
      'own-nowhere',
    ]);
    expect(actionIds('own-in-scene')).toEqual(['more-char-btn-own-in-scene']);
    expect(actionIds('own-placed')).toEqual(['more-char-btn-own-placed']);
    expect(actionIds('own-elsewhere')).toEqual([
      'focus-char-btn-own-elsewhere',
      'more-char-btn-own-elsewhere',
    ]);
    // 未所属: no scene to switch to, so no inert 視点 button.
    expect(actionIds('own-nowhere')).toEqual(['more-char-btn-own-nowhere']);
  });

  // spec: Issue #228 自分のキャラクター（管理者）とすべてのキャラクター
  it('gives an admin その他 everywhere and 視点を合わせる only for characters outside the scene', async () => {
    mountTab({ role: 'admin' });

    await switchView('mine');
    expect(rowIds()).toEqual([
      'own-in-scene',
      'own-placed',
      'own-elsewhere',
      'own-nowhere',
    ]);
    expect(actionIds('own-in-scene')).toEqual(['more-char-btn-own-in-scene']);
    expect(actionIds('own-elsewhere')).toEqual([
      'focus-char-btn-own-elsewhere',
      'more-char-btn-own-elsewhere',
    ]);

    await switchView('all');
    expect(rowIds()).toEqual(CHARACTERS.map((character) => character.id));
    for (const id of ['own-in-scene', 'other-in-scene', 'unowned-in-scene', 'own-placed']) {
      expect(actionIds(id)).toEqual([`more-char-btn-${id}`]);
    }
    for (const id of ['own-elsewhere', 'other-elsewhere', 'unowned-elsewhere']) {
      expect(actionIds(id)).toEqual([`focus-char-btn-${id}`, `more-char-btn-${id}`]);
    }
    // 未所属 has no scene to focus on (Issue #228).
    expect(actionIds('own-nowhere')).toEqual(['more-char-btn-own-nowhere']);
  });

  // spec: Issue #228 シーン外ビューでは名前の下に現在シーン名を小さく出す
  it('shows the scene name (or 未所属) only outside the scene view', async () => {
    mountTab({ role: 'admin' });

    // Scene view: no per-row scene label.
    expect(target.querySelector('[data-testid="char-scene-own-in-scene"]')).toBeNull();

    await switchView('mine');
    expect(
      target.querySelector('[data-testid="char-scene-own-in-scene"]')?.textContent?.trim(),
    ).toBe('広場');
    expect(
      target.querySelector('[data-testid="char-scene-own-elsewhere"]')?.textContent?.trim(),
    ).toBe('森');
    expect(
      target.querySelector('[data-testid="char-scene-own-nowhere"]')?.textContent?.trim(),
    ).toBe('未所属');

    await switchView('all');
    expect(
      target.querySelector('[data-testid="char-scene-other-elsewhere"]')?.textContent?.trim(),
    ).toBe('森');
  });

  // spec: Issue #228 駒トグルのアイコン・ツールチップ・aria-pressed
  it('switches the piece toggle icon, label and aria-pressed with placedAsPiece', async () => {
    let toggled: string | null = null;
    mountTab({
      role: 'participant',
      onTogglePiece: (id: string) => {
        toggled = id;
      },
    });
    await tick();

    const unplaced = target.querySelector<HTMLButtonElement>(
      '[data-testid="piece-char-btn-own-in-scene"]',
    );
    expect(unplaced?.getAttribute('aria-label')).toBe('駒として配置');
    expect(tooltipText(unplaced)).toBe('駒として配置');
    expect(unplaced?.getAttribute('aria-pressed')).toBe('false');
    expect(unplaced?.querySelector('.np-icon')?.textContent?.trim()).toBe('person_pin_circle');

    const placed = target.querySelector<HTMLButtonElement>(
      '[data-testid="piece-char-btn-own-placed"]',
    );
    expect(placed?.getAttribute('aria-label')).toBe('盤面から削除');
    expect(tooltipText(placed)).toBe('盤面から削除');
    expect(placed?.getAttribute('aria-pressed')).toBe('true');
    expect(placed?.querySelector('.np-icon')?.textContent?.trim()).toBe('remove_circle');

    unplaced?.click();
    expect(toggled).toBe('own-in-scene');
  });

  // spec: Issue #228 別シーンに配置は既存シーンの Menu で、現在シーンは除外する
  it('opens a move menu with every other existing scene, anchored to its own button', async () => {
    mountTab({ role: 'admin' });

    const moveBtn = target.querySelector<HTMLButtonElement>(
      '[data-testid="move-char-btn-own-in-scene"]',
    );
    expect(moveBtn?.getAttribute('aria-haspopup')).toBe('menu');
    expect(moveBtn?.getAttribute('aria-expanded')).toBe('false');

    moveBtn?.click();
    await tick();

    expect(moveBtn?.getAttribute('aria-expanded')).toBe('true');
    expect(target.querySelector('[data-testid="move-char-menu-own-in-scene"]')).toBeTruthy();
    // Other scenes only: never the character's current scene.
    expect(
      target.querySelector('[data-testid="move-char-scene-own-in-scene-scene-b"]'),
    ).toBeTruthy();
    expect(
      target.querySelector('[data-testid="move-char-scene-own-in-scene-scene-c"]'),
    ).toBeTruthy();
    expect(
      target.querySelector('[data-testid="move-char-scene-own-in-scene-scene-a"]'),
    ).toBeNull();
    // The button itself carries the menu anchor.
    expect(moveBtn?.style.getPropertyValue('anchor-name')).toBe(
      '--character-menu-anchor-move-own-in-scene',
    );
  });

  // spec: Issue #228 別シーンに配置: 行き先が無ければ空メニュー
  it('shows an empty move menu when there is no other scene', async () => {
    mountTab({
      role: 'admin',
      currentScene: makeScene({ id: 'only', name: '唯一のシーン', members: ['own-in-scene'] }),
      allScenes: [makeScene({ id: 'only', name: '唯一のシーン', members: ['own-in-scene'] })],
      characters: [makeCharacter({ id: 'own-in-scene', ownerAccountId: ACCOUNT })],
    });

    target.querySelector<HTMLButtonElement>('[data-testid="move-char-btn-own-in-scene"]')?.click();
    await tick();
    expect(target.querySelector('[data-testid="move-char-menu-empty"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="move-char-scene-own-in-scene-only"]')).toBeNull();
  });

  // spec: Issue #228 メニューは外側クリックで閉じ、同時に1つだけ開く
  it('closes menus on an outside click and keeps only one menu open', async () => {
    mountTab({ role: 'admin' });

    const moveBtn = target.querySelector<HTMLButtonElement>(
      '[data-testid="move-char-btn-own-in-scene"]',
    )!;
    const moreBtn = target.querySelector<HTMLButtonElement>(
      '[data-testid="more-char-btn-own-in-scene"]',
    )!;

    moveBtn.click();
    await tick();
    expect(moveBtn.getAttribute('aria-expanded')).toBe('true');

    // Outside click closes it.
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await tick();
    expect(moveBtn.getAttribute('aria-expanded')).toBe('false');

    // Opening その他 closes the move menu.
    moveBtn.click();
    await tick();
    moreBtn.click();
    await tick();
    expect(moveBtn.getAttribute('aria-expanded')).toBe('false');
    expect(moreBtn.getAttribute('aria-expanded')).toBe('true');
    expect(target.querySelector('[data-testid="character-menu-own-in-scene"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="delete-char-btn-own-in-scene"]')).toBeTruthy();

    // Opening another row's move menu closes the previous one.
    const otherMove = target.querySelector<HTMLButtonElement>(
      '[data-testid="move-char-btn-other-in-scene"]',
    )!;
    otherMove.click();
    await tick();
    expect(moreBtn.getAttribute('aria-expanded')).toBe('false');
    expect(otherMove.getAttribute('aria-expanded')).toBe('true');
  });

  // spec: Issue #228 別シーンに配置はシーンIDで App の移動経路を呼ぶ
  it('calls the move callback with the target scene ID and reports a failed callback', async () => {
    let moved: { characterId: string; sceneId: string } | null = null;
    mountTab({
      role: 'admin',
      onMoveCharacter: (characterId: string, sceneId: string) => {
        moved = { characterId, sceneId };
        return false;
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="move-char-btn-own-in-scene"]')?.click();
    await tick();
    target
      .querySelector<HTMLElement>('[data-testid="move-char-scene-own-in-scene-scene-b"]')
      ?.click();
    await tick();

    expect(moved).toEqual({ characterId: 'own-in-scene', sceneId: 'scene-b' });
    const error = target.querySelector<HTMLElement>('[data-testid="character-action-error"]');
    expect(error).toBeTruthy();
    expect(error?.getAttribute('role')).toBe('status');
    expect(error?.textContent?.trim()).toBe('サーバーと通信できませんでした');
    // The menu closed and the row stayed.
    expect(target.querySelector('[data-testid="move-char-btn-own-in-scene"]')).toBeTruthy();
  });

  // spec: Issue #228 視点を合わせるはキャラクターのシーンIDを通知する
  it('reports the character scene ID for 視点を合わせる', async () => {
    let focused: string | null = null;
    mountTab({
      role: 'admin',
      onFocusCharacterScene: (sceneId: string) => {
        focused = sceneId;
      },
    });

    await switchView('all');
    target.querySelector<HTMLButtonElement>('[data-testid="focus-char-btn-own-elsewhere"]')?.click();
    expect(focused).toBe('scene-b');
  });

  // spec: Issue #228 削除は その他 → 削除 → 既存の確認ダイアログ
  it('routes delete through その他 and the existing confirmation callback', async () => {
    let requested: { id: string; name: string } | null = null;
    mountTab({
      role: 'participant',
      onRequestDeleteCharacter: (char: { id: string; name: string }) => {
        requested = char;
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="more-char-btn-own-in-scene"]')?.click();
    await tick();
    target.querySelector<HTMLElement>('[data-testid="delete-char-btn-own-in-scene"]')?.click();
    await tick();

    expect(requested).toEqual({ id: 'own-in-scene', name: 'アリス' });
  });
});

describe('Character list App integration (Issue #228)', () => {
  const ROOM: VisitedRoom = {
    id: 'aabbccddeeff0228',
    name: 'キャラクター一覧テスト部屋',
    url: 'https://alhalte.example/rooms/aabbccddeeff0228#key',
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
  });

  function mountApp(socket: FakeRoomSocket, props: Record<string, unknown> = {}) {
    app = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [
          makeScene({ id: '/', name: '広場', members: ['42'] }),
          makeScene({ id: 'scene-b', name: '森', members: ['43'] }),
        ],
        initialCharacters: [
          makeCharacter({ id: '42', name: '勇者ロラン', ownerAccountId: ACCOUNT }),
          makeCharacter({ id: '43', name: '魔法使いミナ', ownerAccountId: ACCOUNT }),
        ],
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

  async function openSettings(): Promise<void> {
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
    await tick(40);
  }

  function sceneName(): string {
    return target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]')?.value ?? '';
  }

  // spec: Issue #228 駒トグルは ack 後にだけ反転する
  it('flips the piece toggle only after the state ack', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    socket.characterListSnapshot = [
      makeCharacter({ id: '42', name: '勇者ロラン', ownerAccountId: ACCOUNT }),
    ];
    mountApp(socket);
    await tick(30);
    await openCharacters();

    const pieceBtn = target.querySelector<HTMLButtonElement>('[data-testid="piece-char-btn-42"]');
    expect(pieceBtn?.getAttribute('aria-label')).toBe('駒として配置');
    expect(pieceBtn?.getAttribute('aria-pressed')).toBe('false');
    pieceBtn?.click();
    await tick();

    expect(socket.requestUpdateStateCalls).toEqual([
      { sceneId: '/', key: 'piece::42::placed', value: true },
    ]);
    // Not flipped optimistically.
    expect(pieceBtn?.getAttribute('aria-label')).toBe('駒として配置');
    expect(pieceBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('person_pin_circle');

    socket.resolveStateWrite();
    await tick(40);
    const placedBtn = target.querySelector<HTMLButtonElement>('[data-testid="piece-char-btn-42"]');
    expect(placedBtn?.getAttribute('aria-label')).toBe('盤面から削除');
    expect(placedBtn?.getAttribute('aria-pressed')).toBe('true');
    expect(placedBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('remove_circle');
  });

  // spec: Issue #228 視点を合わせるは現在シーンを切り替える
  it('switches the current scene when 視点を合わせる is used', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [
      makeCharacter({ id: '42', name: '勇者ロラン', ownerAccountId: ACCOUNT }),
      makeCharacter({ id: '43', name: '魔法使いミナ', ownerAccountId: ACCOUNT }),
    ];
    mountApp(socket);
    await tick(30);
    await openCharacters();

    // 43 is not in the current scene, so the admin's all view offers 視点.
    target
      .querySelector<HTMLInputElement>('[data-testid="character-view-segmented"] input[value="all"]')
      ?.click();
    await tick();
    target.querySelector<HTMLButtonElement>('[data-testid="focus-char-btn-43"]')?.click();
    await tick(30);

    expect(socket.enterSceneCalls.at(-1)).toBe('scene-b');

    // The scene view now lists the focused scene's character.
    target
      .querySelector<HTMLInputElement>('[data-testid="character-view-segmented"] input[value="scene"]')
      ?.click();
    await tick();
    expect(card('43')).toBeTruthy();
    expect(card('42')).toBeNull();
  });

  // spec: Issue #228 別シーンに配置は ack 後にだけ行を消し、管理者の現在シーンは変えない
  it('removes the row after the move ack and keeps the current scene', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    socket.characterListSnapshot = [
      makeCharacter({ id: '42', name: '勇者ロラン', ownerAccountId: ACCOUNT }),
      makeCharacter({ id: '43', name: '魔法使いミナ', ownerAccountId: ACCOUNT }),
    ];
    mountApp(socket, {
      initialScenes: [
        makeScene({ id: '/', name: '広場', members: ['42', '43'] }),
        makeScene({ id: 'scene-b', name: '森', members: [] }),
      ],
    });
    await tick(30);
    await openCharacters();

    target.querySelector<HTMLButtonElement>('[data-testid="move-char-btn-42"]')?.click();
    await tick();
    target
      .querySelector<HTMLElement>('[data-testid="move-char-scene-42-scene-b"]')
      ?.click();
    await tick();

    expect(socket.requestMoveCharacterCalls).toEqual([{ characterId: 42, toSceneId: 'scene-b' }]);
    // The character list never creates a scene implicitly.
    expect(socket.requestCreateSceneCalls).toHaveLength(0);
    expect(target.querySelector('[data-testid="move-char-pending-42"]')).toBeTruthy();
    // No optimistic removal.
    expect(card('42')).toBeTruthy();
    expect(target.querySelector('[data-testid="character-action-error"]')).toBeNull();

    socket.resolveSceneAction('move');
    await tick(40);

    expect(target.querySelector('[data-testid="move-char-pending-42"]')).toBeNull();
    expect(card('42')).toBeNull();
    expect(card('43')).toBeTruthy();
    // The admin stays in the scene they were curating (Issue #228).
    await openSettings();
    expect(sceneName()).toBe('広場');
    expect(socket.enterSceneCalls).not.toContain('scene-b');
  });

  // spec: Issue #228 別シーンに配置の失敗は行を残してエラーを出す
  it('keeps the row and reports a move failure', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    socket.characterListSnapshot = [
      makeCharacter({ id: '42', name: '勇者ロラン', ownerAccountId: ACCOUNT }),
    ];
    mountApp(socket);
    await tick(30);
    await openCharacters();

    target.querySelector<HTMLButtonElement>('[data-testid="move-char-btn-42"]')?.click();
    await tick();
    target
      .querySelector<HTMLElement>('[data-testid="move-char-scene-42-scene-b"]')
      ?.click();
    await tick();

    socket.rejectSceneAction(new OperationTimeoutError('moveCharacter', 8000), 'move');
    await tick(40);

    const error = target.querySelector<HTMLElement>('[data-testid="character-action-error"]');
    expect(error).toBeTruthy();
    expect(error?.getAttribute('role')).toBe('status');
    expect(error?.textContent?.trim()).toBe('サーバーと通信できませんでした');
    expect(card('42')).toBeTruthy();
    expect(target.querySelector('[data-testid="move-char-pending-42"]')).toBeNull();
    await openSettings();
    expect(sceneName()).toBe('広場');
  });

  // spec: Issue #228 削除は その他 → 削除 → ルート確認ダイアログ → ack
  it('deletes through その他 and the root confirmation dialog', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerCharacterRequests = false;
    socket.characterListSnapshot = [
      makeCharacter({ id: '42', name: '勇者ロラン', ownerAccountId: ACCOUNT }),
    ];
    mountApp(socket);
    await tick(30);
    await openCharacters();

    target.querySelector<HTMLButtonElement>('[data-testid="more-char-btn-42"]')?.click();
    await tick();
    target.querySelector<HTMLElement>('[data-testid="delete-char-btn-42"]')?.click();
    await tick();

    expect(target.querySelector('[data-testid="root-delete-char-dialog"]')).toBeTruthy();
    target.querySelector<HTMLButtonElement>('[data-testid="confirm-char-delete-button"]')?.click();
    await tick();

    expect(socket.requestDeleteCharacterCalls).toEqual([{ characterId: 42 }]);
    expect(card('42')).toBeTruthy();

    socket.resolveCharacterRequest('delete');
    await tick(40);
    expect(card('42')).toBeNull();
  });
});
