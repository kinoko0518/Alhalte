/**
 * Room-wide character synchronization (Issue #160).
 *
 * Covers:
 * - full Tin attribute decoding for `CharacterList` / `CharacterInfo`
 *   (statuses, parameters, chat palette, icon, color, piece position),
 * - create / update / delete / move broadcasts applied by every client,
 * - `scene.members` rebuilt from `SceneInfo` / `SceneList` summaries after a
 *   reload / reconnect,
 * - piece placement and position applied from `StateSnapshot` / `StateUpdated`
 *   and visible to another client,
 * - offline move / piece operations reported as failures instead of mutating the
 *   local model as if they succeeded.
 *
 * No network is used: the socket is `tests/helpers/fakeRoomSocket.ts`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 入室`, `### キャラクター同期`
 * - GitHub Issue #160
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import {
  RoomSocket,
  characterToTinMap,
  jsonToTin,
} from '../src/utils/roomSocket.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

const ROOM: VisitedRoom = {
  id: 'room-character-sync-160',
  name: 'キャラクター同期テスト部屋',
  url: 'https://alhalte.example/rooms/room-character-sync-160#key',
  lastVisited: Date.now(),
  isAdmin: true,
};

const ERROR_MESSAGE = 'サーバーと通信できませんでした';

const HERO: Character = {
  id: '42',
  name: '勇者ロラン',
  initiative: 17,
  iconUrl: 'https://example.com/hero.png',
  color: '#6750a4',
  description: '前衛の騎士。',
  reference: '英雄譚',
  statuses: [{ label: 'HP', current: 30, max: 30 }],
  parameters: [{ label: 'STR', value: 16 }],
  chatPalette: '1D100<=80 【近接攻撃】',
  hideStatus: false,
  isNarrative: false,
  placedAsPiece: false,
};

function settle(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(predicate: () => boolean, timeoutMs = 2000): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await settle(10);
  }
}

function makeScene(overrides: Partial<Scene> = {}): Scene {
  return {
    id: '/',
    name: 'メインシーン',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
    ...overrides,
  };
}

describe('Room-wide character synchronization (Issue #160)', () => {
  const mounted: Array<{ target: HTMLElement; instance: Record<string, any> }> = [];

  beforeEach(() => {
    document.body.innerHTML = '';
    localStorage.clear();
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });
  });

  afterEach(() => {
    for (const app of mounted.splice(0)) {
      unmount(app.instance);
      app.target.remove();
    }
    document.body.innerHTML = '';
  });

  function mountApp(socket: FakeRoomSocket, scenes: Scene[], characters: Character[]) {
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        // Clone per app: two clients must not share the same scene / character
        // object graph (App keeps them in reactive state).
        initialScenes: JSON.parse(JSON.stringify(scenes)) as Scene[],
        initialCharacters: JSON.parse(JSON.stringify(characters)) as Character[],
        isAdmin: true,
        roomSocket: asRoomSocket(socket),
      },
    }) as unknown as Record<string, any>;
    mounted.push({ target, instance });
    return { target, instance };
  }

  function card(target: HTMLElement, characterId: string): HTMLElement | null {
    return target.querySelector<HTMLElement>(`[data-testid="character-card-${characterId}"]`);
  }

  function fieldEntry(target: HTMLElement, characterId: string): HTMLElement | null {
    return target.querySelector<HTMLElement>(`[data-testid="char-entry-${characterId}"]`);
  }

  function piece(target: HTMLElement, characterId: string): HTMLElement | null {
    return target.querySelector<HTMLElement>(`[data-testid="piece-${characterId}"]`);
  }

  function openCharacters(target: HTMLElement): void {
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-characters"]')?.click();
  }

  // spec: argenterie/character CharacterList / CharacterInfo decode every Tin attribute (Issue #160)
  it('decodes statuses, parameters, palette, icon, color and piece position from the Tin map', () => {
    const socket = new RoomSocket({ roomId: '00000000000000ae' });

    socket.handleServerFrame({
      CharacterList: { characters: { '42': characterToTinMap(HERO) } },
    });
    const [fromList] = socket.getCharacters();
    expect(fromList).toMatchObject({
      id: '42',
      name: '勇者ロラン',
      initiative: 17,
      iconUrl: 'https://example.com/hero.png',
      color: '#6750a4',
      description: '前衛の騎士。',
      reference: '英雄譚',
      statuses: [{ label: 'HP', current: 30, max: 30 }],
      parameters: [{ label: 'STR', value: 16 }],
      chatPalette: '1D100<=80 【近接攻撃】',
      hideStatus: false,
      isNarrative: false,
      placedAsPiece: false,
    });

    // CharacterInfo uses the same decoder (a partial raw map would have dropped
    // the statuses / parameters / palette before Issue #160).
    socket.handleServerFrame({
      CharacterInfo: {
        character_id: 42,
        data: characterToTinMap({
          ...HERO,
          name: '勇者ロラン改',
          statuses: [{ label: 'MP', current: 4, max: 9 }],
          piecePosition: { x: 33, y: 44 },
        }),
      },
    });
    const updated = socket.getCharacters().find((character) => character.id === '42');
    expect(updated?.name).toBe('勇者ロラン改');
    expect(updated?.statuses).toEqual([{ label: 'MP', current: 4, max: 9 }]);
    expect(updated?.piecePosition).toEqual({ x: 33, y: 44 });
  });

  // spec: argenterie/character create / update / delete / move are visible to every client (Issue #160)
  it('applies CharacterInfo / CharacterMoved / CharacterDeleted broadcasts on two clients', async () => {
    const socketA = new FakeRoomSocket();
    const socketB = new FakeRoomSocket();
    const scenes = [
      makeScene({ members: ['42'] }),
      makeScene({ id: 'forest', name: '森' }),
    ];
    const a = mountApp(socketA, scenes, []);
    const b = mountApp(socketB, scenes, []);
    await settle(30);
    openCharacters(a.target);
    openCharacters(b.target);
    await settle(20);

    // Create: both clients receive the room-wide CharacterInfo.
    for (const socket of [socketA, socketB]) {
      socket.emitFrame({ CharacterInfo: { character_id: 42, data: characterToTinMap(HERO) } });
    }
    await waitFor(() => card(a.target, '42') !== null && card(b.target, '42') !== null);
    expect(card(a.target, '42')?.textContent).toContain('勇者ロラン');
    expect(card(b.target, '42')?.textContent).toContain('勇者ロラン');

    // Membership follows the server SceneInfo.
    for (const socket of [socketA, socketB]) {
      socket.emitFrame({
        SceneInfo: {
          id: '/',
          name: 'メインシーン',
          present_character_count: 1,
          present_characters: [42],
        },
      });
    }
    await waitFor(
      () => fieldEntry(a.target, '42') !== null && fieldEntry(b.target, '42') !== null,
    );
    // The field decodes the same statuses as the character tab.
    expect(
      a.target.querySelector('[data-testid="status-item-42-HP"]')?.textContent,
    ).toContain('30');

    // Update: both clients replace the row.
    for (const socket of [socketA, socketB]) {
      socket.emitFrame({
        CharacterInfo: {
          character_id: 42,
          data: characterToTinMap({ ...HERO, name: '勇者ロラン改' }),
        },
      });
    }
    await waitFor(
      () =>
        (card(a.target, '42')?.textContent ?? '').includes('勇者ロラン改') &&
        (card(b.target, '42')?.textContent ?? '').includes('勇者ロラン改'),
    );

    // Move: CharacterMoved plus the authoritative summaries.
    for (const socket of [socketA, socketB]) {
      socket.emitFrame({ CharacterMoved: { character_id: 42, to_scene_id: 'forest' } });
      socket.emitFrame({
        SceneInfo: {
          id: '/',
          name: 'メインシーン',
          present_character_count: 0,
          present_characters: [],
        },
      });
      socket.emitFrame({
        SceneInfo: {
          id: 'forest',
          name: '森',
          present_character_count: 1,
          present_characters: [42],
        },
      });
    }
    await waitFor(
      () => fieldEntry(a.target, '42') === null && fieldEntry(b.target, '42') === null,
    );
    // The character tab's scene view lists the current scene only, so the moved
    // row disappears from both clients without reloading.
    expect(card(a.target, '42')).toBeNull();
    expect(card(b.target, '42')).toBeNull();

    // Delete: both clients drop the row.
    for (const socket of [socketA, socketB]) {
      socket.emitFrame({ CharacterDeleted: { character_id: 42 } });
    }
    await waitFor(() => card(a.target, '42') === null && card(b.target, '42') === null);
    expect(card(a.target, '42')).toBeNull();
    expect(card(b.target, '42')).toBeNull();
  });

  // spec: argenterie/character scene.members is restored from SceneList on reload / reconnect (Issue #160)
  it('rebuilds scene.members from SceneList summaries after a reload / reconnect', async () => {
    const socket = new FakeRoomSocket();
    const { target } = mountApp(socket, [makeScene()], [{ ...HERO }]);
    await settle(30);

    // Before any authoritative summary the cached membership is empty.
    expect(fieldEntry(target, '42')).toBeNull();

    // Reload: the server lists the room with the persisted membership.
    socket.emitFrame({
      SceneList: {
        scenes: [
          {
            id: '/',
            name: 'メインシーン',
            present_character_count: 1,
            present_characters: [42],
          },
        ],
      },
    });
    await waitFor(() => fieldEntry(target, '42') !== null);
    expect(fieldEntry(target, '42')).toBeTruthy();

    // Another client removes the membership, then a reconnect restores it.
    socket.emitFrame({
      SceneList: {
        scenes: [
          {
            id: '/',
            name: 'メインシーン',
            present_character_count: 0,
            present_characters: [],
          },
        ],
      },
    });
    await waitFor(() => fieldEntry(target, '42') === null);

    socket.emitFrame({
      SceneList: {
        scenes: [
          {
            id: '/',
            name: 'メインシーン',
            present_character_count: 1,
            present_characters: [42],
          },
        ],
      },
    });
    await waitFor(() => fieldEntry(target, '42') !== null);
    expect(fieldEntry(target, '42')).toBeTruthy();
  });

  // spec: argenterie/field piece placement / position from StateSnapshot / StateUpdated is shared (Issue #160)
  it('applies piece placement and position from StateSnapshot / StateUpdated on two clients', async () => {
    const socketA = new FakeRoomSocket();
    const socketB = new FakeRoomSocket();
    const scenes = [makeScene({ members: ['42'] })];
    const a = mountApp(socketA, scenes, [{ ...HERO }]);
    const b = mountApp(socketB, scenes, [{ ...HERO }]);
    await settle(30);

    for (const socket of [socketA, socketB]) {
      socket.emitFrame({
        StateSnapshot: {
          scene_id: '/',
          states: {
            'piece::42::placed': jsonToTin(true),
            'piece::42::pos': jsonToTin({ x: 33, y: 44 }),
          },
        },
      });
    }
    await waitFor(() => piece(a.target, '42') !== null && piece(b.target, '42') !== null);
    expect(piece(a.target, '42')?.style.left).toBe('33px');
    expect(piece(a.target, '42')?.style.top).toBe('44px');
    expect(piece(b.target, '42')?.style.left).toBe('33px');
    expect(piece(b.target, '42')?.style.top).toBe('44px');

    // A live drag of one client reaches the other via StateUpdated.
    for (const socket of [socketA, socketB]) {
      socket.emitFrame({
        StateUpdated: {
          scene_id: '/',
          key: 'piece::42::pos',
          value: jsonToTin({ x: 60, y: 70 }),
        },
      });
    }
    await waitFor(() => piece(a.target, '42')?.style.left === '60px');
    expect(piece(a.target, '42')?.style.top).toBe('70px');
    expect(piece(b.target, '42')?.style.left).toBe('60px');
    expect(piece(b.target, '42')?.style.top).toBe('70px');

    // Removing the placement clears the piece everywhere.
    for (const socket of [socketA, socketB]) {
      socket.emitFrame({
        StateUpdated: {
          scene_id: '/',
          key: 'piece::42::placed',
          value: jsonToTin(false),
        },
      });
    }
    await waitFor(() => piece(a.target, '42') === null && piece(b.target, '42') === null);
  });

  // spec: argenterie/offline offline piece toggle is not applied locally (Issue #160)
  it('reports an error and keeps the local model when toggling a piece offline', async () => {
    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    socket.characterListSnapshot = [{ ...HERO }];
    const { target } = mountApp(socket, [makeScene({ members: ['42'] })], [{ ...HERO }]);
    await settle(30);
    openCharacters(target);
    await settle(30);

    target
      .querySelector<HTMLButtonElement>('[data-testid="piece-char-btn-42"]')
      ?.click();
    await settle(30);

    expect(target.querySelector('[data-testid="piece-action-error"]')?.textContent?.trim()).toBe(
      'ルームに接続していないため、駒を操作できません',
    );
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    // Placement was not committed locally.
    expect(piece(target, '42')).toBeNull();
  });

  // spec: argenterie/offline offline piece drag reverts and reports an error (Issue #160)
  it('reverts an offline piece drag and reports an error instead of looking successful', async () => {
    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    const placed: Character = {
      ...HERO,
      placedAsPiece: true,
      piecePosition: { x: 100, y: 100 },
    };
    const { target } = mountApp(socket, [makeScene({ members: ['42'] })], [placed]);
    await settle(30);
    const pieceEl = piece(target, '42');
    expect(pieceEl).toBeTruthy();

    pieceEl!.dispatchEvent(
      new PointerEvent('pointerdown', {
        bubbles: true,
        cancelable: true,
        button: 0,
        pointerId: 7,
        clientX: 100,
        clientY: 100,
      }),
    );
    pieceEl!.dispatchEvent(
      new PointerEvent('pointermove', {
        bubbles: true,
        cancelable: true,
        pointerId: 7,
        clientX: 160,
        clientY: 140,
      }),
    );
    pieceEl!.dispatchEvent(
      new PointerEvent('pointerup', {
        bubbles: true,
        cancelable: true,
        pointerId: 7,
        clientX: 160,
        clientY: 140,
      }),
    );
    await settle(40);

    expect(target.querySelector('[data-testid="piece-action-error"]')?.textContent?.trim()).toBe(
      'ルームに接続していないため、駒を操作できません',
    );
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    // The pre-drag position is restored.
    const reverted = piece(target, '42');
    expect(reverted?.style.left).toBe('100px');
    expect(reverted?.style.top).toBe('100px');
  });

  // spec: argenterie/offline offline member move fails without mutating members (Issue #160, #228, #269)
  it('does not apply an offline member move from the character list', async () => {
    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    // The character tab re-queries the list on mount; keep the fixture row.
    socket.characterListSnapshot = [{ ...HERO }];
    const scenes = [
      makeScene({ name: '広場', members: ['42'] }),
      makeScene({ id: 'scene-b', name: '森', members: [] }),
    ];
    const { target } = mountApp(socket, scenes, [{ ...HERO }]);
    await settle(30);
    openCharacters(target);
    await settle(30);

    // 「キャラクターを移動」 → root modal → the existing scene, then the failure
    // is reported by the modal's switcher (Issue #269).
    target.querySelector<HTMLButtonElement>('[data-testid="move-char-btn-42"]')?.click();
    await settle(20);
    const input = target.querySelector<HTMLInputElement>(
      '[data-testid="move-scene-search-input"]',
    );
    expect(input).toBeTruthy();
    input!.value = '森';
    input!.dispatchEvent(new Event('input', { bubbles: true }));
    input!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await settle(40);

    expect(
      target.querySelector('[data-testid="move-scene-search-error"]')?.textContent?.trim(),
    ).toBe(ERROR_MESSAGE);
    expect(socket.requestMoveCharacterCalls).toHaveLength(0);
    // The modal stays open so the admin can retry, and the row is unchanged.
    expect(target.querySelector('[data-testid="move-character-modal"]')).toBeTruthy();
    expect(card(target, '42')).toBeTruthy();
    vi.restoreAllMocks();
  });
});
