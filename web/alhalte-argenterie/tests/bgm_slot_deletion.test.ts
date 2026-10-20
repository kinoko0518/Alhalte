/**
 * Persistent BGM slot deletion marker (Issue #226).
 *
 * The component-level deletion tests live in `bgm_controls.test.ts`; these are
 * the App-level checks that the `bgm::<slot>::removed` marker is part of the
 * scene model and survives the operation's own StateDeleted / StateUpdated
 * broadcasts, a second client, a reload and an add-after-delete.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### BGM / 背景画像`
 * - GitHub Issue #226
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { jsonToTin } from '../src/utils/roomSocket.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Scene } from '../src/types/index.js';

const TIN_A = 'a'.repeat(64);

function settle(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function makeScene(overrides: Partial<Scene> = {}): Scene {
  return {
    id: '/',
    name: 'BGM削除テスト',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
    ...overrides,
  };
}

function mountApp(scene: Scene, socket = new FakeRoomSocket()) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(App, {
    target,
    props: {
      initialRoom: {
        id: 'room-bgm-slot-deletion-226',
        name: 'BGMスロット削除テスト部屋',
        url: `https://alhalte.example/rooms/room-bgm-slot-deletion-226#${'0'.repeat(64)}`,
        lastVisited: Date.now(),
        isAdmin: true,
      },
      initialScenes: [scene],
      initialCharacters: [],
      isAdmin: true,
      roomSocket: asRoomSocket(socket),
    },
  });
  return { target, instance, socket };
}

async function openSettings(target: HTMLElement): Promise<void> {
  target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
  await settle();
}

function row(target: HTMLElement, slot: number): HTMLElement | null {
  return target.querySelector<HTMLElement>(`[data-testid="bgm-row-${slot}"]`);
}

describe('BGM slot deletion marker (Issue #226)', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  // spec: argenterie/bgm-controls 削除後の自己broadcastでスロットが復活しない
  it('keeps the deleted slot gone across its own track / playing / volume broadcasts', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
          {
            slot: 4,
            tinId: TIN_A,
            mimeType: 'audio/ogg',
            name: 'fourth.ogg',
            volume: 0.8,
            isPlaying: false,
          },
        ],
      },
    });
    const { target, instance, socket } = mountApp(scene);
    await settle();
    await openSettings(target);
    expect(row(target, 4)).toBeTruthy();

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-4"]')!.click();
    await settle();

    // The component's ack path records the marker in the App model.
    expect(row(target, 4)).toBeNull();
    expect(
      socket.requestUpdateStateCalls.some(
        (call) => call.key === 'bgm::4::removed' && call.value === true,
      ),
    ).toBe(true);
    // The operation's own broadcasts arrive afterwards, and in the worst order
    // for the old code (track delete, playing=false, volume delete).
    socket.emitFrame({ StateDeleted: { scene_id: '/', key: 'std::bgm4' } });
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::4::playing', value: jsonToTin(false) },
    });
    socket.emitFrame({ StateDeleted: { scene_id: '/', key: 'bgm::4::volume' } });
    await settle();

    expect(row(target, 4)).toBeNull();
    expect(row(target, 3)).toBeTruthy();

    // The marker broadcast itself is idempotent.
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::4::removed', value: jsonToTin(true) },
    });
    await settle();
    expect(row(target, 4)).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 他クライアントのremovedマーカーは後続のtrack/playingでも復活しない
  it('hides a slot for another client when the marker arrives before the track broadcasts', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: null, volume: 0.8, isPlaying: false },
          {
            slot: 2,
            tinId: TIN_A,
            mimeType: 'audio/ogg',
            name: 'second.ogg',
            volume: 0.8,
            isPlaying: false,
          },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });
    const { target, instance, socket } = mountApp(scene);
    await settle();
    await openSettings(target);
    expect(row(target, 2)).toBeTruthy();

    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::2::removed', value: jsonToTin(true) },
    });
    await settle();
    expect(row(target, 2)).toBeNull();

    // Later state for the removed slot must not re-create it.
    socket.emitFrame({
      StateUpdated: {
        scene_id: '/',
        key: 'std::bgm2',
        value: jsonToTin({ tinId: TIN_A, mimeType: 'audio/ogg', name: 'second.ogg' }),
      },
    });
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::2::playing', value: jsonToTin(true) },
    });
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::2::volume', value: jsonToTin(0.2) },
    });
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::2::removed', value: jsonToTin(true) },
    });
    await settle();

    expect(row(target, 2)).toBeNull();
    expect(row(target, 1)).toBeTruthy();
    expect(row(target, 3)).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls StateSnapshotのremovedマーカーはreload後も既定slot1-3を隠す
  it('hides removed default slots 1-3 when a snapshot hydrates the marker', async () => {
    const scene = makeScene();
    const { target, instance, socket } = mountApp(scene);
    await settle();
    await openSettings(target);
    expect(row(target, 1)).toBeTruthy();
    expect(row(target, 3)).toBeTruthy();

    socket.emitFrame({
      StateSnapshot: {
        scene_id: '/',
        states: {
          // Stale NONE/track keys are intentionally present: the marker must
          // win regardless of the map order.
          'std::bgm': jsonToTin(TIN_A),
          'bgm::1::playing': jsonToTin(true),
          'bgm::1::removed': jsonToTin(true),
          'bgm::3::removed': jsonToTin(true),
        },
      },
    });
    await settle();

    expect(row(target, 1)).toBeNull();
    expect(row(target, 2)).toBeTruthy();
    expect(row(target, 3)).toBeNull();

    // A snapshot re-application stays idempotent.
    socket.emitFrame({
      StateSnapshot: {
        scene_id: '/',
        states: {
          'std::bgm': jsonToTin(TIN_A),
          'bgm::1::playing': jsonToTin(true),
          'bgm::1::removed': jsonToTin(true),
          'bgm::3::removed': jsonToTin(true),
        },
      },
    });
    await settle();
    expect(row(target, 1)).toBeNull();
    expect(row(target, 2)).toBeTruthy();
    expect(row(target, 3)).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls initialScenesのremovedマーカーを初期表示から尊重する
  it('honors a persisted marker in initialScenes before the first snapshot', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmRemovedSlots: [1],
      },
    });
    const { target, instance } = mountApp(scene);
    await settle();
    await openSettings(target);

    expect(row(target, 1)).toBeNull();
    expect(row(target, 2)).toBeTruthy();
    expect(row(target, 3)).toBeTruthy();
    expect(scene.juices.bgmSlots?.map((slot) => slot.slot)).toEqual([2, 3]);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 再追加はremovedマーカーを外して行を復活させる
  it('clears the marker and re-renders the slot when it is added again', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });
    const { target, instance } = mountApp(scene);
    await settle();
    await openSettings(target);

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-3"]')!.click();
    await settle();
    expect(row(target, 3)).toBeNull();

    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.click();
    await settle();

    // max(1, 2) + 1 = 3: the number is re-opened as a fresh empty slot. The
    // row only renders if the local removal marker was cleared.
    expect(row(target, 3)).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-play-btn-3"]')).toBeTruthy();
    expect(
      target.querySelector('[data-testid="bgm-play-btn-3"]')?.getAttribute('aria-label'),
    ).toBe('BGM再生');
    expect(
      target.querySelector('[data-testid="bgm-vol-label-3"]')?.textContent?.trim(),
    ).toBe('80%');

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls removedマーカーのStateDeletedでフィルタを解除する
  it('drops the marker on StateDeleted so a later track update can re-create the slot', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });
    const { target, instance, socket } = mountApp(scene);
    await settle();
    await openSettings(target);

    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::2::removed', value: jsonToTin(true) },
    });
    await settle();
    expect(row(target, 2)).toBeNull();

    socket.emitFrame({ StateDeleted: { scene_id: '/', key: 'bgm::2::removed' } });
    await settle();
    // The marker deletion alone does not invent an empty row.
    expect(row(target, 2)).toBeNull();

    socket.emitFrame({
      StateUpdated: {
        scene_id: '/',
        key: 'std::bgm2',
        value: jsonToTin({ tinId: TIN_A, mimeType: 'audio/ogg', name: 'back.ogg' }),
      },
    });
    await settle();
    expect(row(target, 2)).toBeTruthy();

    unmount(instance);
    target.remove();
  });
});
