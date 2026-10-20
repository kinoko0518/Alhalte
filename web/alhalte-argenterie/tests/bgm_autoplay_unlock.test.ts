/**
 * BGM autoplay unlock on the first user interaction (Issue #248).
 *
 * The browser autoplay policy used to surface as a `blocked` slot plus a
 * 「クリックして有効化」 button. That user-facing burden is gone: `AudioManager`
 * leaves a refused slot prepared and `paused` (no error), and `App` retries it
 * from a one-shot window `pointerdown` listener.
 *
 * Covers:
 * - the first `pointerdown` resumes every intended-playing, prepared slot,
 * - a gesture that arrives before the track is prepared is a no-op and the
 *   listener stays armed for the next gesture,
 * - no 「クリックして有効化」 UI / handler exists any more,
 * - the listener is removed once playback is established.
 *
 * No network: the audio manager is injected with a fake downloader and
 * `HTMLAudioElement`; the socket is `tests/helpers/fakeRoomSocket.ts`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### BGM`
 * - GitHub Issue #248 (parent #243)
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import App from '../src/components/App.svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { clearAssets } from '../src/utils/assets.js';
import { AudioManager, type AudioElementLike } from '../src/utils/audio.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Scene } from '../src/types/index.js';

const ROOM_KEY = new Uint8Array(32).fill(7);
const TIN_A = 'a'.repeat(64);

const SETTINGS_SOURCE = readFileSync(
  path.resolve(process.cwd(), 'src/components/Control/tabs/SceneSettingsTab.svelte'),
  'utf8',
);
const AUDIO_SOURCE = readFileSync(
  path.resolve(process.cwd(), 'src/utils/audio.ts'),
  'utf8',
);

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
 * `HTMLAudioElement` double.
 *
 * `autoplayRejections` is the number of upcoming `play()` calls that reject with
 * `NotAllowedError`, exactly like a browser refusing playback before a gesture.
 */
class FakeAudioElement implements AudioElementLike {
  src = '';
  volume = 1;
  paused = true;
  autoplayRejections = 0;
  playCalls = 0;
  play = vi.fn(async () => {
    this.playCalls += 1;
    if (this.autoplayRejections > 0) {
      this.autoplayRejections -= 1;
      throw Object.assign(new Error('play blocked'), { name: 'NotAllowedError' });
    }
    this.paused = false;
  });
  pause = vi.fn(() => {
    this.paused = true;
  });
}

interface UnlockHarness {
  manager: AudioManager;
  elements: FakeAudioElement[];
}

function createHarness(
  options: {
    download?: () => Promise<Uint8Array>;
    /** Rejections queued on every element created (browser autoplay policy). */
    autoplayRejections?: number;
  } = {},
): UnlockHarness {
  const elements: FakeAudioElement[] = [];
  let urlCounter = 0;
  const manager = new AudioManager({
    createElement: () => {
      const element = new FakeAudioElement();
      element.autoplayRejections = options.autoplayRejections ?? 0;
      elements.push(element);
      return element;
    },
    download: vi.fn(options.download ?? (async () => new Uint8Array([1, 2, 3, 4]))),
    createObjectUrl: () => `blob:unlock/${(urlCounter += 1)}`,
    revokeObjectUrl: vi.fn(),
  });
  return { manager, elements };
}

/** A scene whose authoritative state says slot 1 must be playing. */
function makePlayingScene(): Scene {
  return {
    id: 'scene-bgm-unlock',
    name: 'BGM自動解除テスト',
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
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
  };
}

function mountApp(scene: Scene, socket: FakeRoomSocket, audio: UnlockHarness) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(App, {
    target,
    props: {
      initialRoom: {
        id: 'room-bgm-autoplay-248',
        name: 'BGM自動解除テスト部屋',
        url: `https://alhalte.example/rooms/room-bgm-autoplay-248#${'0'.repeat(64)}`,
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

function pressPointer(target: Window = window): void {
  target.dispatchEvent(new Event('pointerdown'));
}

describe('BGM autoplay unlock (Issue #248)', () => {
  beforeEach(() => {
    clearAssets();
    localStorage.clear();
  });

  // spec: Issue #248 最初の pointerdown で再生中の BGM を再開する
  it('resumes a prepared, intended playing slot on the first pointerdown', async () => {
    const audio = createHarness({ autoplayRejections: 1 });
    const socket = new FakeRoomSocket();
    const scene = makePlayingScene();
    const { target, instance } = mountApp(scene, socket, audio);

    // Mount prepares the source and the browser refuses autoplay: the slot is
    // paused, not an error, and the element was never started.
    await waitFor(() => audio.manager.getState(1).status === 'paused');
    expect(audio.elements).toHaveLength(1);
    const element = audio.elements[0];
    expect(element.playCalls).toBe(1);
    expect(element.paused).toBe(true);
    expect(audio.manager.getState(1).error).toBeNull();

    // The first user interaction unlocks the prepared element. `play()` is
    // invoked synchronously inside the gesture: Safari only accepts
    // `HTMLAudioElement.play()` in the same task as the event.
    pressPointer();
    expect(element.playCalls).toBe(2);
    await waitFor(() => audio.manager.getState(1).status === 'playing');
    expect(audio.manager.getState(1).status).toBe('playing');
    expect(element.playCalls).toBe(2);
    expect(element.paused).toBe(false);
    expect(audio.manager.getState(1).error).toBeNull();

    // No user-facing autoplay notice is rendered anywhere.
    expect(target.querySelector('[data-testid^="bgm-enable-audio"]')).toBeNull();
    expect(document.body.textContent).not.toContain('クリックして有効化');

    unmount(instance);
    target.remove();
  });

  // spec: Issue #248 音源が未準備なら次の pointerdown で再試行する
  it('keeps waiting when the source is not prepared yet and retries on the next pointerdown', async () => {
    let resolveDownload: ((payload: Uint8Array) => void) | null = null;
    const audio = createHarness({
      autoplayRejections: 1,
      download: () =>
        new Promise<Uint8Array>((resolve) => {
          resolveDownload = resolve;
        }),
    });
    const socket = new FakeRoomSocket();
    const scene = makePlayingScene();
    const { target, instance } = mountApp(scene, socket, audio);
    await waitFor(() => resolveDownload !== null);
    expect(audio.elements).toHaveLength(0);

    // The download is still pending: the first gesture cannot start anything,
    // so it must not consume the only unlock.
    pressPointer();
    await settle();
    expect(audio.elements).toHaveLength(0);

    // The payload arrives, the element is prepared, and the automatic play is
    // refused by the autoplay policy (the queued rejection).
    resolveDownload!(new Uint8Array([1, 2, 3, 4]));
    await waitFor(() => audio.manager.getState(1).status === 'paused');
    expect(audio.elements).toHaveLength(1);
    expect(audio.manager.getState(1).status).toBe('paused');
    expect(audio.manager.getState(1).error).toBeNull();
    const element = audio.elements[0];
    expect(element.playCalls).toBe(1);
    await settle();

    // The listener survived the no-op gesture and plays on the next one.
    pressPointer();
    await waitFor(() => audio.manager.getState(1).status === 'playing');
    expect(audio.manager.getState(1).status).toBe('playing');
    expect(element.playCalls).toBe(2);
    expect(element.paused).toBe(false);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #248 「クリックして有効化」の UI と handler を持たない
  it('has no click-to-enable UI, handler or blocked surface', async () => {
    const audio = createHarness();
    const socket = new FakeRoomSocket();
    const scene = makePlayingScene();
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
      },
    });
    await settle();

    // The rendered BGM rows carry no autoplay affordance.
    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid^="bgm-enable-audio"]')).toBeNull();
    expect(target.textContent).not.toContain('クリックして有効化');

    // The source has neither the button nor its handler.
    expect(SETTINGS_SOURCE).not.toContain('クリックして有効化');
    expect(SETTINGS_SOURCE).not.toContain('bgm-enable-audio');
    expect(SETTINGS_SOURCE).not.toContain('handleEnableBgmAudio');

    // The audio manager no longer exposes the blocked status / retry surface.
    expect(AUDIO_SOURCE).not.toContain("'blocked'");
    expect(AUDIO_SOURCE).not.toContain('isAutoplayBlockedError');
    expect(AUDIO_SOURCE).not.toContain('async retry');
    expect('blocked' in audio.manager.getState(1)).toBe(false);
    expect('retry' in (audio.manager as unknown as object)).toBe(false);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #248 listener は再生が成立した後に解除される
  it('removes the unlock listener once playback is established', async () => {
    const audio = createHarness({ autoplayRejections: 1 });
    const socket = new FakeRoomSocket();
    const scene = makePlayingScene();
    const { target, instance } = mountApp(scene, socket, audio);

    await waitFor(() => audio.manager.getState(1).status === 'paused');
    const element = audio.elements[0];
    expect(element.playCalls).toBe(1);

    pressPointer();
    await waitFor(() => audio.manager.getState(1).status === 'playing');
    expect(audio.manager.getState(1).status).toBe('playing');
    expect(element.playCalls).toBe(2);
    // Let the async re-arm / disarm reconciliation settle.
    await settle();

    // Re-pause the element through the manager only (a paused, prepared source
    // that nothing re-arms): if the listener were still registered, the next
    // gesture would start it again.
    audio.manager.pause(1);
    await audio.manager.prepare(
      1,
      { tinId: TIN_A, mimeType: 'audio/ogg', name: 'theme.ogg' },
      ROOM_KEY,
    );
    expect(audio.manager.getState(1).status).toBe('paused');
    const playCallsBeforeSecondGesture = element.playCalls;

    pressPointer();
    await settle();
    expect(element.playCalls).toBe(playCallsBeforeSecondGesture);
    expect(audio.manager.getState(1).status).toBe('paused');

    unmount(instance);
    target.remove();
  });
});
