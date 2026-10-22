import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  applyPan,
  applyPinch,
  applyZoom,
  chunkStatusesInPairs,
  formatStatusItem,
  sortCharactersByInitiative,
} from '../src/utils/field.js';
import FieldPanel from '../src/components/Field/FieldPanel.svelte';
import type { Character, Scene, ViewTransform } from '../src/types/index.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';

describe('Field Panel & Initiatives', () => {
  // spec: alhalte-argenterie/field パン/ズーム/ピンチ計算
  it('calculates pan, zoom, and pinch transformations correctly', () => {
    const initial: ViewTransform = { x: 100, y: 50, scale: 1.0 };

    // Pan
    const panned = applyPan(initial, 20, -10);
    expect(panned).toEqual({ x: 120, y: 40, scale: 1.0 });

    // Zoom around center
    const zoomed = applyZoom(initial, 2.0, { x: 100, y: 50 });
    expect(zoomed.scale).toBe(2.0);
    expect(zoomed.x).toBe(100);
    expect(zoomed.y).toBe(50);

    // Zoom bounds clamping
    const clamped = applyZoom(initial, 100, undefined, { min: 0.5, max: 5.0 });
    expect(clamped.scale).toBe(5.0);

    // Pinch calculation
    const pinched = applyPinch(initial, 100, 150);
    expect(pinched.scale).toBe(1.5);
  });

  // spec: alhalte-argenterie/field イニシアチブ降順ソート
  it('sorts characters in descending order of initiative', () => {
    const chars: Character[] = [
      {
        id: 'c1',
        name: 'Fighter',
        initiative: 12,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
      {
        id: 'c2',
        name: 'Rogue',
        initiative: 18,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
      {
        id: 'c3',
        name: 'Cleric',
        initiative: 8,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];

    const sorted = sortCharactersByInitiative(chars);
    expect(sorted.map((c) => c.name)).toEqual(['Rogue', 'Fighter', 'Cleric']);
    expect(sorted.map((c) => c.initiative)).toEqual([18, 12, 8]);
  });

  // spec: alhalte-argenterie/field 2個1行ステータス表示・プログレスバー
  it('chunks statuses in pairs of 2 per row and formats progress bar values', () => {
    const statuses = [
      { label: 'HP', current: 8, max: 10 },
      { label: 'MP', current: 15, max: 20 },
      { label: 'SAN', current: 50, max: 80 },
    ];

    const pairs = chunkStatusesInPairs(statuses);
    expect(pairs).toHaveLength(2);
    expect(pairs[0]).toHaveLength(2);
    expect(pairs[1]).toHaveLength(1);

    const hpFormatted = formatStatusItem(statuses[0]);
    expect(hpFormatted.text).toBe('HP 10/8'); // ラベル名 最大値/現在地
    expect(hpFormatted.percentage).toBe(80); // (8 / 10) * 100%
  });

  // spec: alhalte-argenterie/field FieldPanel コンポーネント独立状態
  it('renders FieldPanel with top-left overlay and independently maintained view transform', () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-field',
      name: '遭遇戦マップ',
      juices: createDefaultJuices(),
      viewTransform: { x: 50, y: 30, scale: 1.2 },
      members: ['c1', 'c2'],
    };

    const characters: Character[] = [
      {
        id: 'c1',
        name: 'アリス',
        initiative: 15,
        description: '探索者',
        statuses: [
          { label: 'HP', current: 12, max: 12 },
          { label: 'MP', current: 8, max: 10 },
        ],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
        placedAsPiece: true,
        piecePosition: { x: 120, y: 150 },
      },
      {
        id: 'c2',
        name: 'ボブ',
        initiative: 20,
        description: '用心棒',
        statuses: [{ label: 'HP', current: 5, max: 15 }],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];

    let recordedTransform: ViewTransform | null = null;
    const instance = mount(FieldPanel, {
      target,
      props: {
        roomName: 'テストルーム',
        scene: testScene,
        characters,
        onTransformChange: (scId, t) => {
          recordedTransform = t;
        },
      },
    });

    // Check room name badge
    expect(target.querySelector('[data-testid="room-name-badge"]')?.textContent?.trim()).toBe('テストルーム');

    // Check initiative sorting: Bob (20) before Alice (15)
    const charEntries = target.querySelectorAll('[data-testid^="char-entry-"]');
    expect(charEntries).toHaveLength(2);
    expect(charEntries[0].getAttribute('data-testid')).toBe('char-entry-c2');
    expect(charEntries[1].getAttribute('data-testid')).toBe('char-entry-c1');

    // Check initiative badges
    expect(target.querySelector('[data-testid="initiative-badge-c2"]')?.textContent?.trim()).toBe('20');
    expect(target.querySelector('[data-testid="initiative-badge-c1"]')?.textContent?.trim()).toBe('15');

    // Check status display
    const hpItem = target.querySelector('[data-testid="status-item-c1-HP"]');
    expect(hpItem?.textContent).toContain('HP 12/12');

    // Check piece placed on canvas
    expect(target.querySelector('[data-testid="piece-c1"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });
});

/**
 * Issue #268: the field container captured the pointer on every primary
 * pointerdown bubbling through it, including presses on the account avatar in
 * `.field-top-right-layer`. The retargeted `pointerup` made the browser deliver
 * the `click` to `.field-container`, so `AccountMenu`'s `onclick` never ran and
 * the menu never opened. Pressing the avatar also panned the board.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### アカウントUI / Googleサインイン` (7. 配置)
 * - GitHub Issue #268: フィールドパネルのアカウントアイコンからMenuが開かない
 */
describe('Field overlay controls (Issue #268)', () => {
  let target: HTMLDivElement;

  function makeScene(): Scene {
    return {
      id: 'scene-268',
      name: 'アカウント操作テスト',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
  }

  function pointerDown(pointerId: number, clientX = 10, clientY = 10): PointerEvent {
    return new PointerEvent('pointerdown', {
      bubbles: true,
      cancelable: true,
      button: 0,
      pointerId,
      clientX,
      clientY,
    });
  }

  function pointerMove(pointerId: number, clientX: number, clientY: number): PointerEvent {
    return new PointerEvent('pointermove', {
      bubbles: true,
      pointerId,
      clientX,
      clientY,
    });
  }

  function signIn(): void {
    localStorage.setItem('alhalte:account-id', '42');
    localStorage.setItem('alhalte:session-id', '99');
  }

  async function flush(): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, 20));
  }

  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    document.cookie = 'session_id=; path=/; max-age=0; SameSite=Lax';
    // `AccountMenu` reads `/api/account/{id}` on mount; tests never use the network.
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({
        ok: true,
        status: 200,
        statusText: 'OK',
        json: async () => ({ account_id: 42, handle_name: 'Alice', icon_tin_id: null }),
      })),
    );
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    target.remove();
  });

  // spec: Issue #268 フィールドのアバターからアカウントメニューを開ける
  it('opens the account menu when the field avatar is pressed', async () => {
    signIn();
    const instance = mount(FieldPanel, { target, props: { scene: makeScene() } });
    await flush();

    const layer = target.querySelector<HTMLElement>('[data-testid="field-account-layer"]');
    expect(layer).toBeTruthy();
    const avatar = layer?.querySelector<HTMLButtonElement>('[data-testid="account-menu-button"]');
    expect(avatar).toBeTruthy();
    expect(avatar?.getAttribute('aria-expanded')).toBe('false');

    // Reproduce the browser sequence: pointerdown on the avatar, then its click.
    avatar?.dispatchEvent(pointerDown(7));
    avatar?.click();
    await flush();

    expect(avatar?.getAttribute('aria-expanded')).toBe('true');
    const menu = target.querySelector('[data-testid="account-menu"]');
    expect(menu).toBeTruthy();
    expect(
      menu?.querySelector('[data-testid="account-switch-item"]')?.textContent,
    ).toContain('別のアカウントでログイン');
    expect(menu?.querySelector('[data-testid="account-signout-item"]')?.textContent).toContain(
      'サインアウト',
    );

    unmount(instance);
  });

  // spec: Issue #268 フィールドのオーバーレイ上の pointerdown は盤面の pan に使わない
  it('does not capture the pointer or pan the board from overlay controls', async () => {
    signIn();
    const capture = vi.spyOn(Element.prototype, 'setPointerCapture');
    const scene = makeScene();
    const instance = mount(FieldPanel, { target, props: { scene } });
    await flush();

    const container = target.querySelector<HTMLElement>('[data-testid="field-panel"]');
    const accountLayer = target.querySelector<HTMLElement>('[data-testid="field-account-layer"]');
    const avatar = target.querySelector<HTMLElement>('[data-testid="account-menu-button"]');
    const overlayLayer = target.querySelector<HTMLElement>('[data-testid="field-overlay-layer"]');
    const badge = target.querySelector<HTMLElement>('[data-testid="room-name-badge"]');
    const canvas = target.querySelector<HTMLElement>('[data-testid="field-canvas"]');
    expect(container && accountLayer && avatar && overlayLayer && badge && canvas).toBeTruthy();

    // The account overlay (root and avatar) and the top-left overlay are not
    // board drags: no capture, and the following pointermove must not pan.
    accountLayer!.dispatchEvent(pointerDown(11));
    avatar!.dispatchEvent(pointerDown(12));
    badge!.dispatchEvent(pointerDown(13));
    expect(capture).not.toHaveBeenCalled();

    container!.dispatchEvent(pointerMove(11, 40, 40));
    container!.dispatchEvent(pointerMove(13, 40, 40));
    expect(scene.viewTransform).toEqual({ x: 0, y: 0, scale: 1 });

    // The board background keeps capturing the pointer and panning.
    canvas!.dispatchEvent(pointerDown(14));
    expect(capture).toHaveBeenCalledTimes(1);
    expect(capture).toHaveBeenCalledWith(14);
    expect(capture.mock.contexts[0]).toBe(container);

    container!.dispatchEvent(pointerMove(14, 45, 35));
    expect(scene.viewTransform).toEqual({ x: 35, y: 25, scale: 1 });

    unmount(instance);
  });
});
