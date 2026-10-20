/**
 * BGM slot deletion and the persistent `removed` tombstone (Issue #226 / #249).
 *
 * A row exists iff its slot has a track (Issue #249), so both NONE and 🗑️ make
 * the row disappear by deleting the track / playing / volume state and writing
 * `bgm::<slot>::removed = true`. The tombstone stays behind so a stale
 * broadcast or a re-applied snapshot cannot resurrect the row.
 *
 * The component-level operation tests live in `bgm_controls.test.ts`; these are
 * the App-level checks that the marker is part of the scene model and survives
 * the operation's own broadcasts, a second client, a reload and a snapshot.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### BGM / 背景画像`
 * - GitHub Issue #226, GitHub Issue #249
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { jsonToTin } from '../src/utils/roomSocket.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { BgmSlot, Scene } from '../src/types/index.js';

const TIN_A = 'a'.repeat(64);
const TIN_B = 'b'.repeat(64);

function settle(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** A BGM row with a playable track (Issue #249: a row needs a track). */
function bgmRow(slot: number, tinId: string, overrides: Partial<BgmSlot> = {}): BgmSlot {
  return {
    slot,
    tinId,
    mimeType: 'audio/ogg',
    name: `slot-${slot}.ogg`,
    volume: 0.8,
    isPlaying: false,
    ...overrides,
  };
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
        id: 'room-bgm-slot-deletion-249',
        name: 'BGMスロット削除テスト部屋',
        url: `https://alhalte.example/rooms/room-bgm-slot-deletion-249#${'0'.repeat(64)}`,
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

describe('BGM slot deletion and tombstones (Issue #226 / #249)', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  // spec: Issue #249 NONE と削除は同じく行を消し、削除済み slot は snapshot で復活しない
  it('removes the row on NONE, deletes track / playing / volume and keeps it gone across a snapshot', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          bgmRow(1, TIN_A, { isPlaying: true }),
          bgmRow(2, TIN_B),
        ],
      },
    });
    const { target, instance, socket } = mountApp(scene);
    await settle();
    await openSettings(target);
    expect(row(target, 1)).toBeTruthy();
    expect(row(target, 2)).toBeTruthy();

    // The AssetPicker's NONE card is the "no track" selection.
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-1"]')!.click();
    await settle();
    target.querySelector<HTMLElement>('[data-testid="asset-card-none"]')!.click();
    await settle();

    // Same writes as 🗑️ (Issue #249 / #250).
    expect(socket.requestDeleteStateCalls.map((call) => call.key)).toEqual([
      'std::bgm',
      'bgm::1::started_at',
      'bgm::1::paused_at',
      'bgm::1::volume',
    ]);
    expect(
      socket.requestUpdateStateCalls.some(
        (call) => call.key === 'bgm::1::playing' && call.value === false,
      ),
    ).toBe(true);
    expect(
      socket.requestUpdateStateCalls.some(
        (call) => call.key === 'bgm::1::removed' && call.value === true,
      ),
    ).toBe(true);

    // The row is gone, the other slot keeps its own row.
    expect(row(target, 1)).toBeNull();
    expect(row(target, 2)).toBeTruthy();
    expect(
      target.querySelectorAll('[data-testid^="bgm-slot-block-"]'),
    ).toHaveLength(1);

    // A reload / reconnect snapshot still carries the stale track: the
    // tombstone keeps the row from coming back.
    const staleSnapshot = {
      'std::bgm': jsonToTin(TIN_A),
      'bgm::1::playing': jsonToTin(true),
      'bgm::1::volume': jsonToTin(0.8),
      'bgm::1::removed': jsonToTin(true),
      'std::bgm2': jsonToTin({ tinId: TIN_B, mimeType: 'audio/ogg', name: 'slot-2.ogg' }),
    };
    socket.emitFrame({ StateSnapshot: { scene_id: '/', states: staleSnapshot } });
    await settle();

    expect(row(target, 1)).toBeNull();
    expect(row(target, 2)).toBeTruthy();

    // A snapshot re-application stays idempotent.
    socket.emitFrame({ StateSnapshot: { scene_id: '/', states: staleSnapshot } });
    await settle();
    expect(row(target, 1)).toBeNull();
    expect(row(target, 2)).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: Issue #226 削除後の自己broadcastでスロットが復活しない
  it('keeps the deleted slot gone across its own track / playing / volume broadcasts', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          bgmRow(1, TIN_A),
          bgmRow(4, TIN_B, { name: 'fourth.ogg' }),
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
    // for a row that is re-created from a track key.
    socket.emitFrame({ StateDeleted: { scene_id: '/', key: 'std::bgm4' } });
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::4::playing', value: jsonToTin(false) },
    });
    socket.emitFrame({ StateDeleted: { scene_id: '/', key: 'bgm::4::volume' } });
    await settle();

    expect(row(target, 4)).toBeNull();
    expect(row(target, 1)).toBeTruthy();

    // The marker broadcast itself is idempotent.
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::4::removed', value: jsonToTin(true) },
    });
    await settle();
    expect(row(target, 4)).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: Issue #226 他クライアントのremovedマーカーは後続のtrack/playingでも復活しない
  it('hides a slot for another client when the marker arrives before the track broadcasts', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [bgmRow(1, TIN_A), bgmRow(2, TIN_B, { name: 'second.ogg' })],
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
        value: jsonToTin({ tinId: TIN_B, mimeType: 'audio/ogg', name: 'second.ogg' }),
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

    unmount(instance);
    target.remove();
  });

  // spec: Issue #249 削除済み slot は snapshot の stale な std::bgm<N> でも復活しない
  it('does not resurrect a removed slot from a stale std::bgm<N> in a snapshot', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [bgmRow(1, TIN_A), bgmRow(3, TIN_B)],
      },
    });
    const { target, instance, socket } = mountApp(scene);
    await settle();
    await openSettings(target);
    expect(row(target, 1)).toBeTruthy();
    expect(row(target, 3)).toBeTruthy();

    socket.emitFrame({
      StateSnapshot: {
        scene_id: '/',
        states: {
          // Stale track / transport keys for slot 1 are intentionally present:
          // the marker must win regardless of the map order.
          'std::bgm': jsonToTin(TIN_A),
          'bgm::1::playing': jsonToTin(true),
          'bgm::1::removed': jsonToTin(true),
          'std::bgm3': jsonToTin({ tinId: TIN_B, mimeType: 'audio/ogg', name: 'slot-3.ogg' }),
        },
      },
    });
    await settle();

    expect(row(target, 1)).toBeNull();
    expect(row(target, 3)).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: Issue #226 initialScenesのremovedマーカーを初期表示から尊重する
  it('honors a persisted marker in initialScenes before the first snapshot', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmRemovedSlots: [1],
        // A stale track left behind by a partially failed delete.
        bgmSlots: [bgmRow(1, TIN_A), bgmRow(2, TIN_B)],
      },
    });
    const { target, instance } = mountApp(scene);
    await settle();
    await openSettings(target);

    expect(row(target, 1)).toBeNull();
    expect(row(target, 2)).toBeTruthy();
    expect(scene.juices.bgmSlots?.map((slot) => slot.slot)).toEqual([2]);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #226 removedマーカーのStateDeletedでフィルタを解除する
  it('drops the marker on StateDeleted so a later track update can re-create the slot', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [bgmRow(1, TIN_A), bgmRow(2, TIN_B)],
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
    // The marker deletion alone does not invent a row without a track.
    expect(row(target, 2)).toBeNull();

    socket.emitFrame({
      StateUpdated: {
        scene_id: '/',
        key: 'std::bgm2',
        value: jsonToTin({ tinId: TIN_B, mimeType: 'audio/ogg', name: 'back.ogg' }),
      },
    });
    await settle();
    expect(row(target, 2)).toBeTruthy();

    unmount(instance);
    target.remove();
  });
});
