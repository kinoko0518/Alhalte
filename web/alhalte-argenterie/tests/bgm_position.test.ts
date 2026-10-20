/**
 * BGM playback position synchronization & transport UI (Issue #250).
 *
 * The scene state stores only wall-clock timestamps — `bgm::<slot>::started_at`
 * and `bgm::<slot>::paused_at` — never a duration. Every client derives the
 * same playback position from them, so:
 *
 * - a client joining a playing slot starts mid-track,
 * - pause / resume continues from the paused position without downloading the
 *   tin again,
 * - ⏪ rewrites `started_at` and every client seeks back to 0,
 * - the element loops (`loop = true`).
 *
 * The state writes are acked in order: track (when it changed) → `started_at` →
 * delete `paused_at` → `playing=true` for a play, `paused_at` → `playing=false`
 * for a pause.
 *
 * No network: the audio manager is injected with a fake downloader and
 * `HTMLAudioElement`; the socket is `tests/helpers/fakeRoomSocket.ts`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### BGM / 背景画像`
 * - GitHub Issue #250 (parent #243)
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { addAsset, clearAssets } from '../src/utils/assets.js';
import { jsonToTin } from '../src/utils/roomSocket.js';
import {
  AudioManager,
  computeBgmOffsetMs,
  type AudioElementLike,
} from '../src/utils/audio.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { BgmSlot, Scene } from '../src/types/index.js';

const ROOM_KEY = new Uint8Array(32).fill(5);
const TIN_A = 'a'.repeat(64);
const TIN_B = 'b'.repeat(64);

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

/**
 * `HTMLAudioElement` double with the transport surface of Issue #250.
 *
 * `duration` is in seconds and finite by default (a 3 minute track), so the
 * manager wraps the derived offset by it exactly like a browser.
 */
class FakeAudioElement implements AudioElementLike {
  src = '';
  volume = 1;
  paused = true;
  loop = false;
  currentTime = 0;
  duration = 180;
  play = vi.fn(async () => {
    this.paused = false;
  });
  pause = vi.fn(() => {
    this.paused = true;
  });
}

interface PositionHarness {
  manager: AudioManager;
  elements: FakeAudioElement[];
  download: ReturnType<typeof vi.fn>;
  createObjectUrl: ReturnType<typeof vi.fn>;
  revokeObjectUrl: ReturnType<typeof vi.fn>;
}

function createHarness(options: { duration?: number } = {}): PositionHarness {
  const elements: FakeAudioElement[] = [];
  let urlCounter = 0;
  const download = vi.fn(async () => new Uint8Array([1, 2, 3, 4]));
  const createObjectUrl = vi.fn(() => `blob:position/${(urlCounter += 1)}`);
  const revokeObjectUrl = vi.fn();
  const manager = new AudioManager({
    createElement: () => {
      const element = new FakeAudioElement();
      element.duration = options.duration ?? 180;
      elements.push(element);
      return element;
    },
    download,
    createObjectUrl,
    revokeObjectUrl,
  });
  return { manager, elements, download, createObjectUrl, revokeObjectUrl };
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
    name: 'BGM位置同期テスト',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
    ...overrides,
  };
}

function mountApp(scene: Scene, socket: FakeRoomSocket, audio: PositionHarness) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(App, {
    target,
    props: {
      initialRoom: {
        id: 'room-bgm-position-250',
        name: 'BGM位置同期テスト部屋',
        url: `https://alhalte.example/rooms/room-bgm-position-250#${'0'.repeat(64)}`,
        lastVisited: Date.now(),
        isAdmin: true,
      },
      initialScenes: [scene],
      initialCharacters: [],
      isAdmin: true,
      roomKey: ROOM_KEY,
      roomSocket: asRoomSocket(socket),
      audioManager: audio.manager,
    },
  });
  return { target, instance };
}

function mountSettings(
  scene: Scene,
  socket: FakeRoomSocket,
  audio: PositionHarness,
  onRequestAssetPicker?: (options: any) => void,
) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(SceneSettingsTab, {
    target,
    props: {
      currentScene: scene,
      allScenes: [scene],
      role: 'admin',
      roomSocket: asRoomSocket(socket),
      roomKey: ROOM_KEY,
      audioManager: audio.manager,
      ...(onRequestAssetPicker ? { onRequestAssetPicker } : {}),
    },
  });
  return { target, instance };
}

/** Latest recorded state write for `key`. */
function lastWrite(calls: Array<{ key: string; value: unknown }>, key: string) {
  return calls.filter((call) => call.key === key).at(-1);
}

describe('BGM playback position (Issue #250)', () => {
  beforeEach(() => {
    clearAssets();
    localStorage.clear();
  });

  // spec: Issue #250 started_at と duration から再生位置を求める
  it('derives the playback offset from started_at and wraps it by the duration', () => {
    const now = 1_700_000_000_000;
    const durationMs = 5_000;

    expect(computeBgmOffsetMs({ now, startedAt: now, durationMs })).toBe(0);
    expect(computeBgmOffsetMs({ now, startedAt: now - 2_000, durationMs })).toBe(2_000);
    // 12s into a 5s track is 2s into the loop.
    expect(computeBgmOffsetMs({ now, startedAt: now - 12_000, durationMs })).toBe(2_000);
    // A whole number of loops is back at 0.
    expect(computeBgmOffsetMs({ now, startedAt: now - 15_000, durationMs })).toBe(0);
  });

  // spec: Issue #250 paused_at で位置を固定する
  it('freezes the position at paused_at while the wall clock keeps running', () => {
    const startedAt = 1_700_000_000_000;
    const pausedAt = startedAt + 7_000;
    // `now` is long after the pause: the position must not advance.
    expect(
      computeBgmOffsetMs({ now: pausedAt + 60_000, startedAt, pausedAt }),
    ).toBe(7_000);
    // The pause instant wins even when `now` is earlier (clock skew backwards).
    expect(computeBgmOffsetMs({ now: pausedAt - 5_000, startedAt, pausedAt })).toBe(2_000);
  });

  // spec: Issue #250 負の経過時間を 0 に正規化する
  it('clamps a negative or malformed elapsed time to 0', () => {
    expect(computeBgmOffsetMs({ now: 1_000, startedAt: 5_000 })).toBe(0);
    expect(computeBgmOffsetMs({ now: 1_000, startedAt: 5_000, durationMs: 4_000 })).toBe(0);
    expect(
      computeBgmOffsetMs({ now: 1_000, startedAt: 5_000, pausedAt: 6_000, durationMs: 4_000 }),
    ).toBe(0);
    expect(computeBgmOffsetMs({ now: Number.NaN, startedAt: 5_000 })).toBe(0);
    expect(computeBgmOffsetMs({ now: 10_000, startedAt: Number.NaN })).toBe(0);
    expect(computeBgmOffsetMs({ now: -1, startedAt: -1 })).toBe(0);
  });

  // spec: Issue #250 duration 不明なら 0 から
  it('starts a legacy slot without timestamps at 0 and keeps the raw offset when the duration is unknown', () => {
    // A room written before Issue #250 has no `started_at`: the slot starts at
    // 0 and the browser plays from the beginning.
    expect(computeBgmOffsetMs({ now: Date.now(), startedAt: undefined })).toBe(0);
    expect(computeBgmOffsetMs({ now: Date.now(), startedAt: null, pausedAt: 1 })).toBe(0);

    // The duration is only known once the element reports it; until then the
    // raw, non-negative offset is handed to the element (it clamps / loops).
    expect(computeBgmOffsetMs({ now: 9_000, startedAt: 1_000 })).toBe(8_000);
    expect(computeBgmOffsetMs({ now: 9_000, startedAt: 1_000, durationMs: Number.NaN })).toBe(8_000);
    expect(computeBgmOffsetMs({ now: 9_000, startedAt: 1_000, durationMs: 0 })).toBe(8_000);
  });

  // spec: Issue #250 別クライアントは started_at から途中参加する
  it('seeks a joining client to the elapsed position of the playing slot', async () => {
    const audio = createHarness();
    const socket = new FakeRoomSocket();
    const { target, instance } = mountApp(makeScene(), socket, audio);
    await settle(30);

    const emitAt = Date.now();
    const startedAt = emitAt - 5_000;
    socket.emitFrame({
      StateSnapshot: {
        scene_id: '/',
        states: {
          'std::bgm1': jsonToTin({ tinId: TIN_A, mimeType: 'audio/ogg', name: 'mid.ogg' }),
          'bgm::1::playing': jsonToTin(true),
          'bgm::1::started_at': jsonToTin(startedAt),
        },
      },
    });
    await waitFor(
      () => audio.elements.length > 0 && audio.elements[0].paused === false,
    );

    expect(audio.download).toHaveBeenCalledWith(TIN_A, ROOM_KEY);
    // The element was seeked to the elapsed position, not to 0. The upper
    // bound is the elapsed time at assertion time, so a slow CI run cannot make
    // a correct seek look wrong.
    const elapsedSeconds = (Date.now() - startedAt) / 1000;
    expect(audio.elements[0].currentTime).toBeGreaterThan(4.5);
    expect(audio.elements[0].currentTime).toBeLessThanOrEqual(elapsedSeconds);
    expect(audio.manager.getState(1).status).toBe('playing');

    unmount(instance);
    target.remove();
  });

  // spec: Issue #250 pause → resume は停止位置から再開し再ダウンロードしない
  it('pauses, then resumes from the paused position without downloading again', async () => {
    const audio = createHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [bgmRow(1, TIN_A, { isPlaying: true, startedAt: Date.now() - 5_000 })],
      },
    });
    const { target, instance } = mountSettings(scene, socket, audio);
    await settle();

    // The client is already mid-track when the test starts.
    audio.manager.setVolume(1, 0.8);
    await audio.manager.prepare(
      1,
      { tinId: TIN_A, mimeType: 'audio/ogg', name: 'theme.ogg' },
      ROOM_KEY,
    );
    await audio.manager.play(1, 5_000);
    expect(audio.download).toHaveBeenCalledTimes(1);
    expect(audio.elements[0].currentTime).toBeCloseTo(5, 1);
    expect(audio.elements[0].paused).toBe(false);

    const playBtn = target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]')!;

    // Pause: `paused_at` freezes the position; the source is kept.
    playBtn.click();
    await settle(30);
    expect(audio.elements[0].paused).toBe(true);
    expect(audio.revokeObjectUrl).not.toHaveBeenCalled();
    expect(lastWrite(socket.requestUpdateStateCalls, 'bgm::1::paused_at')?.value).toBeTypeOf(
      'number',
    );
    expect(lastWrite(socket.requestUpdateStateCalls, 'bgm::1::playing')?.value).toBe(false);

    // The paused position is `paused_at - started_at`; the resume must keep it.
    const pausedRow = scene.juices.bgmSlots?.find((slot) => slot.slot === 1);
    const pausedElapsedMs = (pausedRow?.pausedAt ?? 0) - (pausedRow?.startedAt ?? 0);
    expect(pausedElapsedMs).toBeGreaterThanOrEqual(5_000);

    // Resume: `started_at = now - (paused_at - started_at)` keeps the offset.
    playBtn.click();
    await settle(30);
    expect(audio.elements[0].paused).toBe(false);
    // No second download: the prepared object URL was reused.
    expect(audio.download).toHaveBeenCalledTimes(1);
    expect(audio.elements[0].currentTime).toBeGreaterThanOrEqual(pausedElapsedMs / 1000 - 0.5);
    expect(audio.elements[0].currentTime).toBeLessThanOrEqual(pausedElapsedMs / 1000 + 0.5);
    expect(lastWrite(socket.requestUpdateStateCalls, 'bgm::1::playing')?.value).toBe(true);
    expect(socket.requestDeleteStateCalls.map((call) => call.key)).toContain(
      'bgm::1::paused_at',
    );

    unmount(instance);
    target.remove();
  });

  // spec: Issue #250 ⏪ は started_at を now にして全クライアントを 0 秒へ戻す
  it('restarts every client at 0 by rewriting started_at (⏪)', async () => {
    // (a) Another client follows the broadcast and seeks back to 0.
    const appAudio = createHarness();
    const appSocket = new FakeRoomSocket();
    const { target: appTarget, instance: appInstance } = mountApp(
      makeScene(),
      appSocket,
      appAudio,
    );
    await settle(30);
    appSocket.emitFrame({
      StateSnapshot: {
        scene_id: '/',
        states: {
          'std::bgm': jsonToTin(TIN_A),
          'bgm::1::playing': jsonToTin(true),
          'bgm::1::started_at': jsonToTin(Date.now() - 5_000),
        },
      },
    });
    await waitFor(() => (appAudio.elements[0]?.currentTime ?? 0) > 4);
    expect(appAudio.elements[0].currentTime).toBeGreaterThan(4);

    appSocket.emitFrame({
      StateUpdated: {
        scene_id: '/',
        key: 'bgm::1::started_at',
        value: jsonToTin(Date.now()),
      },
    });
    await waitFor(() => appAudio.elements[0].currentTime < 1);
    expect(appAudio.elements[0].currentTime).toBeLessThan(1);
    expect(appAudio.elements[0].paused).toBe(false);
    expect(appAudio.download).toHaveBeenCalledTimes(1);
    unmount(appInstance);
    appTarget.remove();

    // (b) The admin's ⏪ writes `started_at = now`, deletes `paused_at` and does
    // not rewrite `playing` (the slot is already playing).
    const audio = createHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [bgmRow(1, TIN_A, { isPlaying: true, startedAt: Date.now() - 5_000 })],
      },
    });
    const { target, instance } = mountSettings(scene, socket, audio);
    await settle();
    await audio.manager.prepare(
      1,
      { tinId: TIN_A, mimeType: 'audio/ogg', name: 'theme.ogg' },
      ROOM_KEY,
    );
    await audio.manager.play(1, 5_000);
    expect(audio.elements[0].currentTime).toBeCloseTo(5, 1);

    const beforeRestart = Date.now();
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-restart-btn-1"]')!.click();
    await settle(30);

    const startedAtWrite = lastWrite(socket.requestUpdateStateCalls, 'bgm::1::started_at');
    expect(startedAtWrite?.value).toBeGreaterThanOrEqual(beforeRestart);
    expect(lastWrite(socket.requestUpdateStateCalls, 'bgm::1::playing')).toBeUndefined();
    expect(socket.requestDeleteStateCalls.map((call) => call.key)).toContain(
      'bgm::1::paused_at',
    );
    expect(audio.elements[0].currentTime).toBeLessThan(0.5);
    expect(audio.elements[0].paused).toBe(false);
    expect(audio.download).toHaveBeenCalledTimes(1);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #250 新しい track の選択は started_at = now から始まる
  it('starts a newly selected track at 0 even when the slot was paused mid-track', async () => {
    const audio = createHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          bgmRow(1, TIN_A, {
            isPlaying: false,
            startedAt: Date.now() - 10_000,
            pausedAt: Date.now() - 5_000,
          }),
        ],
      },
    });
    let requestedOptions: any = null;
    const { target, instance } = mountSettings(scene, socket, audio, (options) => {
      requestedOptions = options;
    });
    await settle();

    const beforeSelect = Date.now();
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-1"]')!.click();
    await settle();
    const asset = addAsset({
      id: 'bgm-position-new-track',
      name: 'fresh.ogg',
      scope: 'personal',
      mediaType: 'audio',
      dataUrl: 'data:audio/ogg;base64,RlJFU0g=',
      mimeType: 'audio/ogg',
      createdAt: 2_000,
      tinId: TIN_B,
    });
    requestedOptions.onSelect(asset.dataUrl, asset);
    await settle(40);

    // The new selection starts a new timeline: `started_at = now`, not the
    // paused position of the previous track.
    const startedAtWrite = lastWrite(socket.requestUpdateStateCalls, 'bgm::1::started_at');
    expect(startedAtWrite?.value).toBeGreaterThanOrEqual(beforeSelect);
    expect(lastWrite(socket.requestUpdateStateCalls, 'bgm::1::playing')?.value).toBe(true);
    expect(socket.requestDeleteStateCalls.map((call) => call.key)).toContain(
      'bgm::1::paused_at',
    );
    expect(audio.elements[0].currentTime).toBeLessThan(0.5);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #250 viewer はトランスポート UI を操作できない
  it('disables every transport control for a read-only viewer', async () => {
    const audio = createHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [bgmRow(1, TIN_A, { isPlaying: true, startedAt: Date.now() - 5_000 })],
      },
    });
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene,
        allScenes: [scene],
        role: 'viewer',
        roomSocket: asRoomSocket(socket),
        roomKey: ROOM_KEY,
        audioManager: audio.manager,
      },
    });
    await settle();

    const ids = ['bgm-restart-btn-1', 'bgm-play-btn-1', 'bgm-select-btn-1', 'bgm-delete-btn-1'];
    for (const id of ids) {
      expect(target.querySelector<HTMLButtonElement>(`[data-testid="${id}"]`)!.disabled).toBe(true);
    }
    // The volume slider is read-only too.
    const input = target.querySelector<HTMLInputElement>(
      '[data-testid="bgm-slider-wrapper-1"] input',
    );
    expect(input?.disabled).toBe(true);
    // Clicking a disabled control never writes the scene state.
    for (const id of ids) {
      target.querySelector<HTMLButtonElement>(`[data-testid="${id}"]`)!.click();
    }
    await settle();
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(socket.requestDeleteStateCalls).toHaveLength(0);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #250 loop を有効にする
  it('enables loop on the prepared element', async () => {
    const audio = createHarness();
    await audio.manager.prepare(
      1,
      { tinId: TIN_A, mimeType: 'audio/ogg', name: 'loop.ogg' },
      ROOM_KEY,
    );
    expect(audio.elements[0].loop).toBe(true);

    // The App-side prepare does the same for a slot hydrated from a snapshot.
    const appAudio = createHarness();
    const socket = new FakeRoomSocket();
    const { target, instance } = mountApp(makeScene(), socket, appAudio);
    await settle(30);
    socket.emitFrame({
      StateSnapshot: {
        scene_id: '/',
        states: {
          'std::bgm': jsonToTin(TIN_A),
          'bgm::1::playing': jsonToTin(true),
        },
      },
    });
    await waitFor(() => appAudio.elements.length > 0);
    expect(appAudio.elements[0].loop).toBe(true);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #250 再生/一時停止/再開の state 書込順
  it('writes the transport state in ack order (track → started_at → paused_at → playing)', async () => {
    const audio = createHarness();
    const socket = new FakeRoomSocket();
    // Record the merged update / delete order, not just the per-kind lists.
    const order: string[] = [];
    const update = socket.requestUpdateState.bind(socket);
    socket.requestUpdateState = (sceneId, key, value) => {
      order.push(`update:${key}`);
      return update(sceneId, key, value);
    };
    const remove = socket.requestDeleteState.bind(socket);
    socket.requestDeleteState = (sceneId, key) => {
      order.push(`delete:${key}`);
      return remove(sceneId, key);
    };

    const scene = makeScene({ juices: { ...createDefaultJuices(), bgmSlots: [bgmRow(1, TIN_A)] } });
    let requestedOptions: any = null;
    const { target, instance } = mountSettings(scene, socket, audio, (options) => {
      requestedOptions = options;
    });
    await settle();

    // Play a new track: track → started_at → delete paused_at → playing.
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-1"]')!.click();
    await settle();
    const asset = addAsset({
      id: 'bgm-position-next',
      name: 'next.ogg',
      scope: 'personal',
      mediaType: 'audio',
      dataUrl: 'data:audio/ogg;base64,TkVYVA==',
      mimeType: 'audio/ogg',
      createdAt: 1_000,
      tinId: TIN_B,
    });
    requestedOptions.onSelect(asset.dataUrl, asset);
    await settle(40);
    expect(order).toEqual([
      'update:std::bgm',
      'update:bgm::1::started_at',
      'delete:bgm::1::paused_at',
      'update:bgm::1::playing',
    ]);

    // Pause: started_at is kept, paused_at then playing=false.
    order.length = 0;
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]')!.click();
    await settle(30);
    expect(order).toEqual(['update:bgm::1::paused_at', 'update:bgm::1::playing']);

    // Resume: started_at → delete paused_at → playing (the track is unchanged).
    order.length = 0;
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]')!.click();
    await settle(30);
    expect(order).toEqual([
      'update:bgm::1::started_at',
      'delete:bgm::1::paused_at',
      'update:bgm::1::playing',
    ]);

    unmount(instance);
    target.remove();
  });
});
