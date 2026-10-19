/**
 * BGM audio playback & synchronization (Issue #164).
 *
 * Covers:
 * - selecting a track downloads / decrypts the tin and plays through one
 *   `HTMLAudioElement` per slot after the server ack,
 * - stop pauses and revokes the object URL while keeping the selection,
 * - volume changes reach `element.volume`,
 * - remote `bgm::<slot>::playing` / track updates apply to App,
 * - a scene switch pauses and revokes the previous scene's audio,
 * - autoplay rejection shows the click-to-enable affordance and the gesture
 *   retries playback,
 * - a missing / undecryptable tin shows the error and never writes `playing`,
 * - the legacy bare 64-hex track state still plays.
 *
 * No network: the audio manager is injected with a fake downloader and
 * `HTMLAudioElement`; the socket is `tests/helpers/fakeRoomSocket.ts`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### BGM`
 * - GitHub Issue #164
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
  BGM_AUDIO_LOAD_ERROR,
  type AudioElementLike,
} from '../src/utils/audio.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Scene } from '../src/types/index.js';

const ROOM_KEY = new Uint8Array(32).fill(9);
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

class FakeAudioElement implements AudioElementLike {
  src = '';
  volume = 1;
  paused = true;
  play = vi.fn(async () => {
    this.paused = false;
  });
  pause = vi.fn(() => {
    this.paused = true;
  });
}

interface AudioHarness {
  manager: AudioManager;
  elements: FakeAudioElement[];
  download: ReturnType<typeof vi.fn>;
  createObjectUrl: ReturnType<typeof vi.fn>;
  revokeObjectUrl: ReturnType<typeof vi.fn>;
}

function createAudioHarness(
  options: { failDownload?: boolean; blockFirstPlay?: boolean } = {},
): AudioHarness {
  const elements: FakeAudioElement[] = [];
  let urlCounter = 0;
  const download = vi.fn(async () => {
    if (options.failDownload) throw new Error('decode failed');
    return new Uint8Array([1, 2, 3, 4]);
  });
  const createObjectUrl = vi.fn(() => `blob:bgm/${(urlCounter += 1)}`);
  const revokeObjectUrl = vi.fn();
  const manager = new AudioManager({
    createElement: () => {
      const element = new FakeAudioElement();
      if (options.blockFirstPlay && elements.length === 0) {
        element.play.mockRejectedValueOnce(
          Object.assign(new Error('play blocked'), { name: 'NotAllowedError' }),
        );
      }
      elements.push(element);
      return element;
    },
    download,
    createObjectUrl,
    revokeObjectUrl,
  });
  return { manager, elements, download, createObjectUrl, revokeObjectUrl };
}

function makeScene(overrides: Partial<Scene> = {}): Scene {
  return {
    id: '/',
    name: 'BGMテスト',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
    ...overrides,
  };
}

function mountSettings(
  scene: Scene,
  socket: FakeRoomSocket,
  audio: AudioHarness,
  onSelect?: (options: any) => void,
) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(SceneSettingsTab, {
    target,
    props: {
      currentScene: scene,
      allScenes: [scene],
      isAdmin: true,
      roomSocket: asRoomSocket(socket),
      roomKey: ROOM_KEY,
      audioManager: audio.manager,
      ...(onSelect ? { onRequestAssetPicker: onSelect } : {}),
    },
  });
  return { target, instance };
}

function mountApp(scene: Scene, socket: FakeRoomSocket, audio: AudioHarness) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(App, {
    target,
    props: {
      initialRoom: {
        id: 'room-bgm-audio-164',
        name: 'BGM再生テスト部屋',
        url: `https://alhalte.example/rooms/room-bgm-audio-164#${'0'.repeat(64)}`,
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

/** Latest recorded state write for `key` (a slot can be written more than once). */
function trackStateUpdater(calls: Array<{ key: string; value: unknown }>, key: string) {
  return calls.filter((call) => call.key === key).at(-1);
}

describe('BGM audio playback (Issue #164)', () => {
  beforeEach(() => {
    clearAssets();
    localStorage.clear();
  });

  // spec: argenterie/bgm-audio 選択→downloadTin→ack→play、停止→pause/revoke (Issue #164)
  it('downloads the selected track, commits after the ack, and plays one element per slot', async () => {
    const audio = createAudioHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene();
    let pickerOptions: any = null;
    const { target, instance } = mountSettings(scene, socket, audio, (options) => {
      pickerOptions = options;
    });
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-2"]')!.click();
    await settle();
    expect(pickerOptions).toBeTruthy();

    const asset = addAsset({
      id: 'audio-track-1',
      name: 'forest.ogg',
      scope: 'personal',
      mediaType: 'audio',
      dataUrl: 'data:audio/ogg;base64,AAAA',
      mimeType: 'audio/ogg',
      createdAt: Date.now(),
      tinId: TIN_A,
    });
    pickerOptions.onSelect(asset.dataUrl, asset);
    await settle(30);

    // The track bytes are downloaded / decrypted with the room key.
    expect(audio.download).toHaveBeenCalledWith(TIN_A, ROOM_KEY);
    expect(audio.createObjectUrl).toHaveBeenCalledTimes(1);
    // Exactly one element was created for slot 2 and it was played.
    expect(audio.elements).toHaveLength(1);
    expect(audio.elements[0].play).toHaveBeenCalled();
    expect(audio.elements[0].src).toContain('blob:bgm/');

    // The state writes use the new schema: playing + `{ tinId, mimeType, name }`.
    const playing = trackStateUpdater(socket.requestUpdateStateCalls, 'bgm::2::playing');
    const track = trackStateUpdater(socket.requestUpdateStateCalls, 'std::bgm2');
    expect(playing?.value).toBe(true);
    expect(track?.value).toMatchObject({
      tinId: TIN_A,
      mimeType: 'audio/ogg',
      name: 'forest.ogg',
    });
    const slot2 = scene.juices.bgmSlots?.find((slot) => slot.slot === 2);
    expect(slot2?.tinId).toBe(TIN_A);
    expect(slot2?.isPlaying).toBe(true);

    // Stop: ack `playing=false`, pause and revoke, keep the selection.
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-2"]')!.click();
    await settle(30);
    expect(
      trackStateUpdater(socket.requestUpdateStateCalls, 'bgm::2::playing')?.value,
    ).toBe(false);
    expect(audio.elements[0].pause).toHaveBeenCalled();
    expect(audio.revokeObjectUrl).toHaveBeenCalledWith(expect.stringContaining('blob:bgm/'));
    expect(scene.juices.bgmSlots?.find((slot) => slot.slot === 2)?.tinId).toBe(TIN_A);
    expect(scene.juices.bgmSlots?.find((slot) => slot.slot === 2)?.isPlaying).toBe(false);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-audio track変更時は前の音源をpauseしobject URLをrevokeする (Issue #164)
  it('pauses and revokes the previous source when the track changes', async () => {
    const audio = createAudioHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          {
            slot: 1,
            tinId: TIN_A,
            mimeType: 'audio/ogg',
            name: 'old.ogg',
            volume: 0.8,
            isPlaying: true,
          },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });
    let pickerOptions: any = null;
    const { target, instance } = mountSettings(scene, socket, audio, (options) => {
      pickerOptions = options;
    });
    await settle();
    audio.manager.setVolume(1, 0.8);
    await audio.manager.prepare(1, { tinId: TIN_A, mimeType: 'audio/ogg', name: '' }, ROOM_KEY);
    await audio.manager.play(1);
    expect(audio.elements).toHaveLength(1);

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-1"]')!.click();
    await settle();
    pickerOptions.onSelect(undefined, {
      tinId: TIN_B,
      mimeType: 'audio/ogg',
      name: 'new.ogg',
    });
    await settle(30);

    expect(audio.download).toHaveBeenCalledWith(TIN_B, ROOM_KEY);
    expect(audio.elements).toHaveLength(1);
    expect(audio.elements[0].pause).toHaveBeenCalled();
    expect(audio.revokeObjectUrl).toHaveBeenCalledWith('blob:bgm/1');
    expect(audio.elements[0].src).toBe('blob:bgm/2');

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-audio 音量はaudio elementへ即時反映する (Issue #164)
  it('applies volume changes to the audio element', async () => {
    const audio = createAudioHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          {
            slot: 1,
            tinId: TIN_A,
            mimeType: 'audio/ogg',
            name: 'theme.ogg',
            volume: 0.8,
            isPlaying: true,
          },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });
    const { target, instance } = mountSettings(scene, socket, audio);
    await settle();
    // Prepare the element the same way App would (App also applies the state
    // volume before preparing).
    audio.manager.setVolume(1, 0.8);
    await audio.manager.prepare(1, { tinId: TIN_A, mimeType: 'audio/ogg', name: '' }, ROOM_KEY);
    expect(audio.elements[0].volume).toBe(0.8);

    const sliderWrapper = target.querySelector<HTMLElement>(
      '[data-testid="bgm-slider-wrapper-1"]',
    )!;
    const inputEl =
      sliderWrapper.querySelector<HTMLInputElement>('input[type="range"]') ??
      sliderWrapper.querySelector<HTMLInputElement>('input');
    inputEl!.value = '35';
    inputEl!.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    expect(audio.elements[0].volume).toBeCloseTo(0.35, 5);

    inputEl!.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    expect(
      trackStateUpdater(socket.requestUpdateStateCalls, 'bgm::1::volume')?.value,
    ).toBeCloseTo(0.35, 5);
    expect(audio.elements[0].volume).toBeCloseTo(0.35, 5);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-audio autoplay制限はクリックして有効化を出し操作で再開する (Issue #164)
  it('shows the click-to-enable affordance when play() is blocked and retries on the gesture', async () => {
    const audio = createAudioHarness({ blockFirstPlay: true });
    const socket = new FakeRoomSocket();
    const scene = makeScene();
    let pickerOptions: any = null;
    const { target, instance } = mountSettings(scene, socket, audio, (options) => {
      pickerOptions = options;
    });
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-1"]')!.click();
    await settle();
    pickerOptions.onSelect(undefined, {
      tinId: TIN_A,
      mimeType: 'audio/ogg',
      name: 'blocked.ogg',
    });
    await waitFor(
      () => target.querySelector('[data-testid="bgm-enable-audio-1"]') !== null,
    );

    // Playback intent is committed; only the browser autoplay policy is blocked.
    expect(
      trackStateUpdater(socket.requestUpdateStateCalls, 'bgm::1::playing')?.value,
    ).toBe(true);
    const enable = target.querySelector<HTMLButtonElement>(
      '[data-testid="bgm-enable-audio-1"]',
    );
    expect(enable?.textContent).toContain('クリックして有効化');

    enable!.click();
    await settle(30);
    expect(audio.elements[0].play).toHaveBeenCalledTimes(2);
    expect(target.querySelector('[data-testid="bgm-enable-audio-1"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-audio 音源取得/復号失敗はエラー表示しplayingを確定しない (Issue #164)
  it('shows an error and does not write playing when the track cannot be decrypted', async () => {
    const audio = createAudioHarness({ failDownload: true });
    const socket = new FakeRoomSocket();
    const scene = makeScene();
    let pickerOptions: any = null;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { target, instance } = mountSettings(scene, socket, audio, (options) => {
      pickerOptions = options;
    });
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-2"]')!.click();
    await settle();
    pickerOptions.onSelect(undefined, {
      tinId: TIN_A,
      mimeType: 'audio/ogg',
      name: 'broken.ogg',
    });
    await waitFor(() => target.querySelector('[data-testid="bgm-error-2"]') !== null);

    const error = target.querySelector<HTMLElement>('[data-testid="bgm-error-2"]');
    expect(error?.getAttribute('role')).toBe('status');
    expect(error?.textContent).toContain(BGM_AUDIO_LOAD_ERROR);
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(scene.juices.bgmSlots?.find((slot) => slot.slot === 2)?.isPlaying).toBe(false);
    expect(audio.elements).toHaveLength(0);

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-audio 部屋鍵なしはエラー表示しplayingを確定しない (Issue #164)
  it('shows an error and does not write playing when the room key is missing', async () => {
    const audio = createAudioHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene();
    let pickerOptions: any = null;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene,
        allScenes: [scene],
        isAdmin: true,
        roomSocket: asRoomSocket(socket),
        roomKey: null,
        audioManager: audio.manager,
        onRequestAssetPicker: (options) => {
          pickerOptions = options;
        },
      },
    });
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-1"]')!.click();
    await settle();
    pickerOptions.onSelect(undefined, {
      tinId: TIN_A,
      mimeType: 'audio/ogg',
      name: 'no-key.ogg',
    });
    await waitFor(() => target.querySelector('[data-testid="bgm-error-1"]') !== null);

    expect(
      target.querySelector('[data-testid="bgm-error-1"]')?.textContent,
    ).toContain('部屋鍵');
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(scene.juices.bgmSlots?.find((slot) => slot.slot === 1)?.isPlaying).toBe(false);

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });
});

describe('BGM audio synchronization (Issue #164)', () => {
  beforeEach(() => {
    clearAssets();
    localStorage.clear();
  });

  // spec: argenterie/bgm-audio remote playing/track/volume更新を再生へ反映しlegacy 64hexも再生する (Issue #164)
  it('plays on a remote playing update, pauses on stop, and accepts the legacy 64-hex track', async () => {
    const audio = createAudioHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene();
    const { target, instance } = mountApp(scene, socket, audio);
    await settle(30);

    // Legacy bare 64-hex `std::bgm` + explicit playing flag.
    socket.emitFrame({
      StateSnapshot: {
        scene_id: '/',
        states: {
          'std::bgm': jsonToTin(TIN_A),
          'bgm::1::playing': jsonToTin(true),
          'bgm::1::volume': jsonToTin(0.3),
        },
      },
    });
    await waitFor(() => audio.elements.length > 0 && audio.elements[0].play.mock.calls.length > 0);

    expect(audio.download).toHaveBeenCalledWith(TIN_A, ROOM_KEY);
    expect(audio.elements).toHaveLength(1);
    expect(audio.elements[0].src).toContain('blob:bgm/');
    expect(audio.elements[0].volume).toBeCloseTo(0.3, 5);

    // Remote stop: pause + revoke. The track selection is retained, which the
    // next play proves by reusing the same tin.
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::1::playing', value: jsonToTin(false) },
    });
    await waitFor(() => audio.elements[0].pause.mock.calls.length > 0);
    expect(audio.revokeObjectUrl).toHaveBeenCalled();
    expect(
      audio.download.mock.calls.filter((call) => call[0] === TIN_A),
    ).toHaveLength(1);

    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::1::playing', value: jsonToTin(true) },
    });
    await waitFor(
      () => audio.download.mock.calls.filter((call) => call[0] === TIN_A).length === 2,
    );

    // Remote track change while stopped updates the selection; the next play
    // downloads the new tin instead of the previous one.
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::1::playing', value: jsonToTin(false) },
    });
    socket.emitFrame({
      StateUpdated: {
        scene_id: '/',
        key: 'std::bgm',
        value: jsonToTin({ tinId: TIN_B, mimeType: 'audio/ogg', name: 'new.ogg' }),
      },
    });
    await settle(30);
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::1::playing', value: jsonToTin(true) },
    });
    await waitFor(
      () => audio.download.mock.calls.filter((call) => call[0] === TIN_B).length === 1,
    );

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-audio シーン切替で前sceneの音源をpause/revokeする (Issue #164)
  it('pauses and revokes the previous scene audio on a scene switch', async () => {
    const audio = createAudioHarness();
    const socket = new FakeRoomSocket();
    const first = makeScene({ id: 'scene-a', name: '広場' });
    const second = makeScene({ id: 'scene-b', name: '森' });
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(App, {
      target,
      props: {
        initialRoom: {
          id: 'room-bgm-scene-switch',
          name: 'BGM切替部屋',
          url: `https://alhalte.example/rooms/room-bgm-scene-switch#${'0'.repeat(64)}`,
          lastVisited: Date.now(),
          isAdmin: true,
        },
        initialScenes: [first, second],
        initialCharacters: [],
        isAdmin: true,
        roomKey: ROOM_KEY,
        roomSocket: asRoomSocket(socket),
        audioManager: audio.manager,
      },
    });
    await settle(30);

    // `scene-a` is the first local scene, so it is the current scene on mount.
    socket.emitFrame({
      StateSnapshot: {
        scene_id: 'scene-a',
        states: {
          'std::bgm': jsonToTin(TIN_A),
          'bgm::1::playing': jsonToTin(true),
        },
      },
    });
    await waitFor(() => audio.elements.some((element) => element.play.mock.calls.length > 0));
    const playedElement = audio.elements.find((element) => element.play.mock.calls.length > 0)!;
    expect(playedElement).toBeTruthy();

    // Switch to scene-b: the previous element is paused and its URL revoked.
    target.querySelector<HTMLInputElement>('[data-testid="scene-search-input"]')?.focus();
    await settle();
    target.querySelector<HTMLElement>('[data-testid="scene-search-item-scene-b"]')?.click();
    await settle(40);
    expect(playedElement.pause).toHaveBeenCalled();
    expect(audio.revokeObjectUrl).toHaveBeenCalled();

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-audio scene切替/unmount時に進行中のdownloadをcancelする (Issue #164)
  it('cancels an in-flight download on stopAll without creating an object URL', async () => {
    let resolveDownload: ((payload: Uint8Array) => void) | null = null;
    const elements: FakeAudioElement[] = [];
    const download = vi.fn(
      () =>
        new Promise<Uint8Array>((resolve) => {
          resolveDownload = resolve;
        }),
    );
    const createObjectUrl = vi.fn(() => 'blob:cancelled');
    const manager = new AudioManager({
      createElement: () => {
        const element = new FakeAudioElement();
        elements.push(element);
        return element;
      },
      download,
      createObjectUrl,
      revokeObjectUrl: vi.fn(),
    });

    const loading = manager.prepare(
      1,
      { tinId: TIN_A, mimeType: 'audio/ogg', name: 'slow.ogg' },
      ROOM_KEY,
    );
    manager.stopAll();
    expect(resolveDownload).toBeTruthy();
    resolveDownload!(new Uint8Array([1, 2, 3]));
    await loading;

    expect(createObjectUrl).not.toHaveBeenCalled();
    expect(elements).toHaveLength(0);
    expect(manager.getState(1).status).toBe('idle');
  });

  // spec: argenterie/bgm-audio unmountで音源を停止しobject URLをrevokeする (Issue #164)
  it('stops and revokes audio when the app unmounts', async () => {
    const audio = createAudioHarness();
    const socket = new FakeRoomSocket();
    const scene = makeScene();
    const { instance } = mountApp(scene, socket, audio);
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
    await waitFor(() => audio.elements.length > 0 && audio.elements[0].play.mock.calls.length > 0);

    unmount(instance);
    expect(audio.elements[0].pause).toHaveBeenCalled();
    expect(audio.revokeObjectUrl).toHaveBeenCalled();
  });
});
