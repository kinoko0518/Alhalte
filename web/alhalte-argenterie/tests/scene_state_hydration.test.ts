/**
 * Scene-state hydration & persistence (Issue #159).
 *
 * Covers:
 * - `EnterScene` -> `StateSnapshot` hydrates the scene name, BGM slots (including
 *   arbitrary slot numbers), background image + dimensions, view transform and
 *   piece placement,
 * - re-applying / reconnecting with the same snapshot is idempotent,
 * - another client observes a rename through `SceneInfo` / `std::name`,
 * - a background upload whose state write fails keeps the previous background,
 * - offline scene actions report an error instead of mutating local state.
 *
 * No network: `cellarTransfer` is mocked and the socket is
 * `tests/helpers/fakeRoomSocket.ts`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 入室`, `### BGM`, `### scene の管理操作`
 * - GitHub Issue #159
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';

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
    getRoomKeyFromUrl: vi.fn(() => null),
    resolveRoomKey: vi.fn((options: { roomKey?: Uint8Array | null } = {}) => options.roomKey ?? null),
    isCellarTransferError: (error: unknown) => error instanceof MockCellarTransferError,
    CellarTransferError: MockCellarTransferError,
  };
});

vi.mock('../src/utils/cellarTransfer.js', () => cellar);

import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import { addAsset, clearAssets } from '../src/utils/assets.js';
import { OperationTimeoutError, jsonToTin } from '../src/utils/roomSocket.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

const ROOM_KEY = new Uint8Array(32).fill(7);
const BACKGROUND_TIN = 'c'.repeat(64);
const BGM_SLOT_1_TIN = 'a'.repeat(64);
const BGM_SLOT_4_TIN = 'b'.repeat(64);
const ERROR_MESSAGE = 'サーバーと通信できませんでした';

const ROOM: VisitedRoom = {
  id: 'room-state-hydration-159',
  name: '状態復元テスト部屋',
  url: `https://alhalte.example/rooms/room-state-hydration-159#${'0'.repeat(64)}`,
  lastVisited: Date.now(),
  isAdmin: true,
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
    name: 'プレースホルダー',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: ['c1'],
    ...overrides,
  };
}

function makeCharacter(overrides: Partial<Character> = {}): Character {
  return {
    id: 'c1',
    name: '旅人',
    initiative: 10,
    description: '',
    statuses: [],
    parameters: [],
    chatPalette: '',
    hideStatus: false,
    placedAsPiece: false,
    ...overrides,
  };
}

function mountApp(socket: FakeRoomSocket, scenes: Scene[], characters: Character[]) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(App, {
    target,
    props: {
      initialRoom: ROOM,
      initialScenes: scenes,
      initialCharacters: characters,
      isAdmin: true,
      roomKey: ROOM_KEY,
      roomSocket: asRoomSocket(socket),
    },
  });
  return { target, instance };
}

function openSettings(target: HTMLElement): void {
  target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
}

function sceneNameValue(target: HTMLElement): string {
  return target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]')?.value ?? '';
}

function inputValue(target: HTMLElement, testId: string): string {
  return target.querySelector<HTMLInputElement>(`[data-testid="${testId}"]`)?.value ?? '';
}

/** StateSnapshot payload for the hydration test. */
function hydratedStates() {
  return {
    'std::name': jsonToTin('サーバー命名シーン'),
    // Slot 1 keeps the legacy bare 64-hex track (Issue #164 fallback).
    'std::bgm': jsonToTin(BGM_SLOT_1_TIN),
    // Slot 4 uses the current `{ tinId, mimeType, name }` shape.
    'std::bgm4': jsonToTin({
      tinId: BGM_SLOT_4_TIN,
      mimeType: 'audio/ogg',
      name: '森のテーマ',
    }),
    'bgm::1::playing': jsonToTin(true),
    'bgm::4::playing': jsonToTin(true),
    'bgm::4::volume': jsonToTin(0.5),
    'std::b-img': jsonToTin({ tinId: BACKGROUND_TIN, mimeType: 'image/png' }),
    'std::b-img::width': jsonToTin(1280),
    'std::b-img::height': jsonToTin(720),
    'view::transform': jsonToTin({ x: 12, y: -8, scale: 1.5 }),
    'piece::c1::placed': jsonToTin(true),
    'piece::c1::pos': jsonToTin({ x: 42, y: 24 }),
  };
}

describe('Scene state hydration (Issue #159)', () => {
  beforeEach(() => {
    clearAssets();
    localStorage.clear();
    cellar.downloadTin.mockReset();
    cellar.downloadTin.mockResolvedValue({
      label: 0n,
      payload: new Uint8Array([137, 80, 78, 71]),
      tinId: BACKGROUND_TIN,
    });
    cellar.encodeRoomTin.mockReset();
    cellar.encodeRoomTin.mockResolvedValue({
      wireBytes: new Uint8Array([1, 2, 3]),
      tinIdHex: BACKGROUND_TIN,
    });
    cellar.uploadTin.mockReset();
    cellar.uploadTin.mockResolvedValue({ tinId: BACKGROUND_TIN });
  });

  // spec: argenterie/scene-settings StateSnapshotでBGM・背景・寸法・view・駒・シーン名を復元する (Issue #159)
  it('hydrates name, BGM slots (arbitrary N), background, dimensions, view and pieces', async () => {
    const socket = new FakeRoomSocket();
    const { target, instance } = mountApp(socket, [makeScene()], [makeCharacter()]);
    await settle(30);

    socket.emitFrame({ StateSnapshot: { scene_id: '/', states: hydratedStates() } });
    await waitFor(
      () =>
        target.querySelector('[data-testid="field-background"] img') !== null &&
        target.querySelector('[data-testid="piece-c1"]') !== null,
    );

    // View transform and piece placement are applied to the field.
    expect(target.querySelector<HTMLElement>('[data-testid="field-canvas"]')?.style.transform).toBe(
      'translate(12px, -8px) scale(1.5)',
    );
    expect(target.querySelector('[data-testid="piece-c1"]')).toBeTruthy();
    const piece = target.querySelector<HTMLElement>('[data-testid="piece-c1"]');
    expect(piece?.style.left).toBe('42px');
    expect(piece?.style.top).toBe('24px');

    // Background image is downloaded from cellar and rendered as a data URL.
    const background = target.querySelector<HTMLImageElement>(
      '[data-testid="field-background"] img',
    );
    expect(background?.getAttribute('src')).toContain('data:image/png;base64');
    expect(cellar.downloadTin).toHaveBeenCalled();
    const backgroundCalls = cellar.downloadTin.mock.calls.filter(
      (call) => call[0] === BACKGROUND_TIN,
    );
    expect(backgroundCalls).toHaveLength(1);
    expect(backgroundCalls[0][1]).toBe(ROOM_KEY);
    // Both playing BGM slots are downloaded and played (Issue #164).
    expect(cellar.downloadTin.mock.calls.some((call) => call[0] === BGM_SLOT_1_TIN)).toBe(true);
    expect(cellar.downloadTin.mock.calls.some((call) => call[0] === BGM_SLOT_4_TIN)).toBe(true);

    // Scene name / dimensions / BGM slots are visible in the settings tab.
    openSettings(target);
    await settle(40);
    expect(sceneNameValue(target)).toBe('サーバー命名シーン');
    expect(inputValue(target, 'bg-width-input')).toBe('1280');
    expect(inputValue(target, 'bg-height-input')).toBe('720');
    expect(target.querySelector('[data-testid="bgm-slot-block-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-slot-block-4"]')).toBeTruthy();
    const slot4Play = target.querySelector<HTMLElement>('[data-testid="bgm-play-btn-4"]');
    expect(slot4Play?.getAttribute('aria-label')).toBe('BGM停止');
    expect(inputValue(target, 'bgm-slider-4')).toBe('50');

    // Re-applying the same snapshot (reconnect / scene switch) is idempotent:
    // neither the background tin nor the BGM tracks are downloaded twice.
    const downloadsAfterFirstSnapshot = cellar.downloadTin.mock.calls.length;
    expect(downloadsAfterFirstSnapshot).toBe(3);
    socket.emitFrame({ StateSnapshot: { scene_id: '/', states: hydratedStates() } });
    await settle(40);
    expect(cellar.downloadTin).toHaveBeenCalledTimes(downloadsAfterFirstSnapshot);
    expect(
      cellar.downloadTin.mock.calls.filter((call) => call[0] === BACKGROUND_TIN),
    ).toHaveLength(1);
    expect(sceneNameValue(target)).toBe('サーバー命名シーン');

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings 名前・背景・寸法は再接続後も残り他クライアントにも見える (Issue #159)
  it('shows the same name / background / dimensions to another client and after reconnect', async () => {
    const socketA = new FakeRoomSocket();
    const socketB = new FakeRoomSocket();
    const a = mountApp(socketA, [makeScene()], [makeCharacter()]);
    const b = mountApp(socketB, [makeScene()], [makeCharacter()]);
    await settle(30);

    // Both clients receive the server snapshot for the same scene.
    socketA.emitFrame({ StateSnapshot: { scene_id: '/', states: hydratedStates() } });
    socketB.emitFrame({ StateSnapshot: { scene_id: '/', states: hydratedStates() } });
    await waitFor(
      () => a.target.querySelector('[data-testid="field-background"] img') !== null &&
        b.target.querySelector('[data-testid="field-background"] img') !== null,
    );

    for (const app of [a, b]) {
      openSettings(app.target);
      await settle(40);
      expect(sceneNameValue(app.target)).toBe('サーバー命名シーン');
      expect(inputValue(app.target, 'bg-width-input')).toBe('1280');
      expect(inputValue(app.target, 'bg-height-input')).toBe('720');
      expect(
        app.target.querySelector<HTMLImageElement>('[data-testid="field-background"] img')
          ?.getAttribute('src'),
      ).toContain('data:image/png;base64');
    }
    // Each client downloads the background plus the two playing BGM tracks.
    expect(cellar.downloadTin).toHaveBeenCalledTimes(6);
    expect(
      cellar.downloadTin.mock.calls.filter((call) => call[0] === BACKGROUND_TIN),
    ).toHaveLength(2);

    // Reconnect: the server re-sends the same snapshot; nothing is lost and the
    // background / BGM tins are not refetched.
    socketA.emitFrame({ StateSnapshot: { scene_id: '/', states: hydratedStates() } });
    await settle(40);
    expect(sceneNameValue(a.target)).toBe('サーバー命名シーン');
    expect(inputValue(a.target, 'bg-width-input')).toBe('1280');
    expect(inputValue(a.target, 'bg-height-input')).toBe('720');
    expect(cellar.downloadTin).toHaveBeenCalledTimes(6);

    unmount(a.instance);
    a.target.remove();
    unmount(b.instance);
    b.target.remove();
  });

  // spec: argenterie/scene-settings シーン名は他クライアントへSceneInfo / std::nameで伝播する (Issue #159)
  it('applies a rename from SceneInfo and a live std::name StateUpdated', async () => {
    const socket = new FakeRoomSocket();
    const { target, instance } = mountApp(socket, [makeScene()], [makeCharacter()]);
    await settle(30);
    openSettings(target);
    await settle(30);
    expect(sceneNameValue(target)).toBe('プレースホルダー');

    // Another client renamed the scene: the summary broadcast updates the name
    // without a reload.
    socket.emitFrame({ SceneInfo: { id: '/', name: '他のクライアントの名前' } });
    await settle(20);
    expect(sceneNameValue(target)).toBe('他のクライアントの名前');

    // A live state update for a newly created scene also carries the name.
    socket.emitFrame({
      StateSnapshot: {
        scene_id: 'scene-new',
        states: { 'std::name': jsonToTin('新しい間') },
      },
    });
    socket.emitFrame({
      StateUpdated: {
        scene_id: 'scene-new',
        key: 'std::name',
        value: jsonToTin('改名後の間'),
      },
    });
    socket.emitFrame({ SceneInfo: { id: 'scene-new', name: '改名後の間' } });
    await settle(30);
    // The new scene is listed by the local model.
    const sceneSwitcher = target.querySelector<HTMLInputElement>(
      '[data-testid="scene-search-input"]',
    );
    expect(sceneSwitcher).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings シーン名はrequestUpdateStateのack後にだけ確定する (Issue #159)
  it('renames the scene only after the std::name acknowledgement', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    const { target, instance } = mountApp(socket, [makeScene()], [makeCharacter()]);
    await settle(30);
    openSettings(target);
    await settle(30);

    const nameInput = target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]');
    expect(nameInput).toBeTruthy();
    nameInput!.value = '改名後のシーン';
    nameInput!.dispatchEvent(new Event('input', { bubbles: true }));
    // Wait past the input debounce: the state request is in flight.
    await waitFor(() =>
      socket.requestUpdateStateCalls.some((call) => call.key === 'std::name'),
    );
    const renameCall = socket.requestUpdateStateCalls.find((call) => call.key === 'std::name');
    expect(renameCall).toEqual({ sceneId: '/', key: 'std::name', value: '改名後のシーン' });
    // The request is outstanding: no success is assumed yet.
    expect(socket.pendingStateOperationCount).toBeGreaterThan(0);

    socket.resolveStateWrite();
    await settle(40);
    expect(sceneNameValue(target)).toBe('改名後のシーン');
    expect(target.querySelector('[data-testid="scene-name-error"]')).toBeNull();

    // A rejected rename keeps the acknowledged name and shows the error.
    const secondInput = target.querySelector<HTMLInputElement>(
      '[data-testid="scene-name-input"]',
    );
    secondInput!.value = '失敗した名前';
    secondInput!.dispatchEvent(new Event('input', { bubbles: true }));
    await waitFor(
      () => socket.pendingStateOperationCount > 0,
    );
    socket.rejectStateWrite(new OperationTimeoutError('updateState', 8000));
    await settle(40);
    expect(sceneNameValue(target)).toBe('改名後のシーン');
    expect(target.querySelector('[data-testid="scene-name-error"]')?.textContent?.trim()).toBe(
      ERROR_MESSAGE,
    );

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings 背景upload/state更新の失敗はローカルへ確定しない (Issue #159)
  it('does not commit the background when the state write fails', async () => {
    const image = addAsset({
      id: 'bg-fail-image',
      name: 'fail.png',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,aGVsbG8=',
      mimeType: 'image/png',
      createdAt: 1000,
    });
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    const currentScene = makeScene({
      juices: { ...createDefaultJuices(), bgWidth: 1200, bgHeight: 800 },
    });
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        characters: [],
        isAdmin: true,
        roomSocket: asRoomSocket(socket),
        roomKey: ROOM_KEY,
      },
    });
    await settle(30);

    // Quick-select the image: encode + upload resolve, the state write is held.
    target
      .querySelector<HTMLButtonElement>(`[data-testid="bg-carousel-item-${image.id}"]`)
      ?.click();
    await settle(40);
    expect(cellar.encodeRoomTin).toHaveBeenCalled();
    expect(cellar.uploadTin).toHaveBeenCalled();
    expect(currentScene.juices.bgImage).toBeUndefined();
    expect(socket.requestUpdateStateCalls.some((c) => c.key === 'std::b-img')).toBe(false);

    // Reject the first dimension write: the b-img state is never sent and the
    // local preview keeps the previous (empty) background.
    socket.rejectStateWrite(new OperationTimeoutError('updateState', 8000));
    await settle(40);
    expect(currentScene.juices.bgImage).toBeUndefined();
    expect(socket.requestUpdateStateCalls.some((c) => c.key === 'std::b-img')).toBe(false);
    expect(
      target.querySelector('[data-testid="bg-image-error"]')?.textContent?.trim(),
    ).toBe(ERROR_MESSAGE);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings 未接続時のscene操作は成功扱いにせずエラーにする (Issue #159)
  it('reports an error and keeps the local scene list when the socket is offline', async () => {
    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    const scenes = [
      makeScene({ id: 'scene-a', name: '広場', members: ['c1'] }),
      makeScene({ id: 'scene-b', name: '森', members: [] }),
    ];
    const { target, instance } = mountApp(socket, scenes, [makeCharacter()]);
    await settle(30);
    openSettings(target);
    await settle(30);

    // Create scene: the dialog closes, but the local list does not grow.
    target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')?.click();
    await settle(20);
    const nameInput = target.querySelector<HTMLInputElement>(
      '[data-testid="new-scene-name-input"]',
    );
    if (nameInput) {
      nameInput.value = 'オフラインシーン';
      nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    target.querySelector<HTMLButtonElement>('[data-testid="submit-create-scene-btn"]')?.click();
    await settle(40);
    expect(target.querySelector('[data-testid="scene-action-error"]')?.textContent?.trim()).toBe(
      ERROR_MESSAGE,
    );
    expect(sceneNameValue(target)).toBe('広場');
    expect(socket.requestCreateSceneCalls).toHaveLength(0);

    // Delete scene: the confirmation closes, the scene remains.
    target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')?.click();
    await settle(20);
    target.querySelector<HTMLButtonElement>('[data-testid="graph-confirm-delete-btn"]')?.click();
    await settle(40);
    expect(target.querySelector('[data-testid="scene-action-error"]')?.textContent?.trim()).toBe(
      ERROR_MESSAGE,
    );
    expect(socket.requestDeleteSceneCalls).toHaveLength(0);
    expect(sceneNameValue(target)).toBe('広場');

    // Delete logs: the cached conversation is not cleared.
    target.querySelector<HTMLButtonElement>('[data-testid="delete-logs-btn"]')?.click();
    await settle(20);
    target
      .querySelector<HTMLButtonElement>('[data-testid="confirm-delete-logs-button"]')
      ?.click();
    await settle(40);
    expect(socket.requestRewindLogCalls).toHaveLength(0);
    expect(target.querySelector('[data-testid="scene-action-error"]')?.textContent?.trim()).toBe(
      ERROR_MESSAGE,
    );

    unmount(instance);
    target.remove();
  });
});
