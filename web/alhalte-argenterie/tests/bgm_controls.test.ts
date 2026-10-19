import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { clearAssets, addAsset } from '../src/utils/assets.js';
import { OperationTimeoutError, ServerOperationError } from '../src/utils/roomSocket.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Scene } from '../src/types/index.js';

const TIN_ID_A = 'a'.repeat(64);
const TIN_ID_B = 'b'.repeat(64);
const ERROR_MESSAGE = 'サーバーと通信できませんでした';

/** Waits for the microtask queue plus Svelte's follow-up render. */
async function settle(ms = 20): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

function makeScene(overrides?: Partial<Scene>): Scene {
  return {
    id: 'scene-bgm-connected',
    name: 'BGM接続テスト',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
    ...overrides,
  };
}

describe('BGM Controls (Issue #53: Reconstructed slot-aware controls)', () => {
  beforeEach(() => {
    clearAssets();
  });

  // spec: argenterie/bgm-controls 制御行レイアウト順序 [停止/再生] [選択] [音量バー] およびボタン幅
  it('renders BGM control row in order: [Play/Stop button] [Select button] [Volume slider], and Play/Stop button has narrower width', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const currentScene: Scene = {
      id: 'scene-bgm-test',
      name: 'BGMテストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
      },
    });

    const row1 = target.querySelector<HTMLElement>('[data-testid="bgm-row-1"]');
    expect(row1).toBeTruthy();

    const playBtn = target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]');
    const selectBtn = target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-1"]');
    const sliderWrapper = target.querySelector<HTMLElement>('[data-testid="bgm-slider-wrapper-1"]');

    expect(playBtn).toBeTruthy();
    expect(selectBtn).toBeTruthy();
    expect(sliderWrapper).toBeTruthy();

    // Verify DOM order: playBtn -> selectBtn -> sliderWrapper
    const children = Array.from(row1!.children);
    const playIndex = children.indexOf(playBtn!);
    const selectIndex = children.indexOf(selectBtn!);
    const sliderIndex = children.indexOf(sliderWrapper!);

    expect(playIndex).toBeLessThan(selectIndex);
    expect(selectIndex).toBeLessThan(sliderIndex);

    // Verify M3 IconButton components used
    expect(playBtn!.classList.contains('np-icon-button')).toBe(true);
    expect(selectBtn!.classList.contains('np-icon-button')).toBe(true);

    // Verify narrower width for Play/Stop button compared to 1:1 square Select button
    expect(playBtn!.classList.contains('narrow')).toBe(true);
    expect(playBtn!.classList.contains('narrow-btn')).toBe(true);
    expect(selectBtn!.classList.contains('square-btn')).toBe(true);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 停止/再生トグルおよび None(null)/TinId 切り替え
  it('toggles playback state between None (null) and TinId on Play/Stop button click', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let lastUpdatedScene: Scene | null = null;
    const currentScene: Scene = {
      id: 'scene-bgm-toggle',
      name: 'トグルテストシーン',
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          {
            slot: 1,
            tinId: TIN_ID_A,
            mimeType: 'audio/ogg',
            name: 'castle_theme.ogg',
            volume: 0.8,
            isPlaying: true,
          },
          {
            slot: 2,
            tinId: null,
            volume: 0.8,
            isPlaying: false,
          },
          {
            slot: 3,
            tinId: null,
            volume: 0.8,
            isPlaying: false,
          },
        ],
      },
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        onUpdateScene: (sc) => {
          lastUpdatedScene = sc;
        },
      },
    });

    const playBtn = target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]');
    expect(playBtn).toBeTruthy();

    // Currently playing: aria-label is "BGM停止" and icon is "stop"
    expect(playBtn!.getAttribute('aria-label')).toBe('BGM停止');
    expect(playBtn!.textContent).toContain('stop');

    // 1. Click to stop -> the selected track stays, isPlaying becomes false
    //    (Issue #164: stopping no longer deletes `std::bgm`).
    playBtn!.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(lastUpdatedScene).toBeTruthy();
    const updatedSlot1 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 1);
    expect(updatedSlot1?.tinId).toBe(TIN_ID_A);
    expect(updatedSlot1?.isPlaying).toBe(false);

    // Label and icon toggle to "BGM再生" and "play_arrow"
    expect(playBtn!.getAttribute('aria-label')).toBe('BGM再生');
    expect(playBtn!.textContent).toContain('play_arrow');

    // 2. Click to resume play -> the same track starts again
    playBtn!.click();
    await new Promise((r) => setTimeout(r, 20));

    const resumedSlot1 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 1);
    expect(resumedSlot1?.tinId).toBe(TIN_ID_A);
    expect(resumedSlot1?.isPlaying).toBe(true);
    expect(playBtn!.getAttribute('aria-label')).toBe('BGM停止');
    expect(playBtn!.textContent).toContain('stop');

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 選択ボタンによる全画面アセットピッカー展開
  it('opens audio asset picker modal when clicking Select button and updates slot TinId', async () => {
    const audioAsset = addAsset({
      id: 'bgm-track-alpha',
      name: 'castle_theme.ogg',
      scope: 'personal',
      mediaType: 'audio',
      dataUrl: 'data:audio/ogg;base64,VEhFTUU=',
      mimeType: 'audio/ogg',
      createdAt: 5000,
      tinId: TIN_ID_A,
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    let lastUpdatedScene: Scene | null = null;
    let requestedOptions: any = null;

    const currentScene: Scene = {
      id: 'scene-select-test',
      name: 'アセット選択テスト',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        onUpdateScene: (sc) => {
          lastUpdatedScene = sc;
        },
        onRequestAssetPicker: (opts) => {
          requestedOptions = opts;
        },
      },
    });

    const selectBtn = target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-2"]');
    expect(selectBtn).toBeTruthy();
    expect(selectBtn!.getAttribute('aria-label')).toBe('BGM選択');

    // Click select button for slot 2
    selectBtn!.click();
    await new Promise((r) => setTimeout(r, 20));

    // Verify onRequestAssetPicker was called with audio mediaType
    expect(requestedOptions).toBeTruthy();
    expect(requestedOptions.mediaType).toBe('audio');
    expect(requestedOptions.initialScope).toBe('personal');

    // Simulate asset selection
    requestedOptions.onSelect(audioAsset.dataUrl, audioAsset);
    await new Promise((r) => setTimeout(r, 20));

    expect(lastUpdatedScene).toBeTruthy();
    const updatedSlot2 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 2);
    // Issue #109 / #142: room state stores the cellar tin id, not the data URL.
    expect(updatedSlot2?.tinId).toBe(audioAsset.tinId);
    expect(updatedSlot2?.tinId).not.toBe(audioAsset.dataUrl);
    expect(updatedSlot2?.isPlaying).toBe(true);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 0〜100 音量バースライダー
  it('functions as a 0-100 volume slider and updates slot volume', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let lastUpdatedScene: Scene | null = null;
    const currentScene: Scene = {
      id: 'scene-slider-test',
      name: '音量スライダーテスト',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        onUpdateScene: (sc) => {
          lastUpdatedScene = sc;
        },
      },
    });

    const sliderWrapper = target.querySelector<HTMLElement>('[data-testid="bgm-slider-wrapper-1"]');
    expect(sliderWrapper).toBeTruthy();

    const sliderEl = sliderWrapper!.querySelector<HTMLElement>('.np-slider');
    expect(sliderEl).toBeTruthy();

    // Check volume label (default 0.8 -> 80%)
    const volLabel = target.querySelector<HTMLElement>('[data-testid="bgm-vol-label-1"]');
    expect(volLabel?.textContent?.trim()).toBe('80%');

    // Input new slider value 45 (45%)
    const inputEl = sliderWrapper!.querySelector<HTMLInputElement>('input[type="range"]')
      || sliderWrapper!.querySelector<HTMLInputElement>('input');
    if (inputEl) {
      inputEl.value = '45';
      inputEl.dispatchEvent(new Event('input', { bubbles: true }));
      inputEl.dispatchEvent(new Event('change', { bubbles: true }));
    } else {
      // Dispatch custom event if no direct input
      sliderEl!.dispatchEvent(new CustomEvent('input', { detail: { value: 45 }, bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    expect(lastUpdatedScene).toBeTruthy();
    const updatedSlot1 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 1);
    expect(updatedSlot1?.volume).toBe(0.45);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls スロット番号(u32)の保持および複数スロット追加・個別制御
  it('manages u32 slot numbers, supports adding multiple slots, and maintains independent BGM states per slot', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let lastUpdatedScene: Scene | null = null;
    const currentScene: Scene = {
      id: 'scene-multi-slot-test',
      name: '複数スロットテスト',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        onUpdateScene: (sc) => {
          lastUpdatedScene = sc;
        },
      },
    });

    // Check initial slots 1, 2, 3 have badges #1, #2, #3
    const badge1 = target.querySelector<HTMLElement>('[data-testid="bgm-slot-badge-1"]');
    const badge2 = target.querySelector<HTMLElement>('[data-testid="bgm-slot-badge-2"]');
    const badge3 = target.querySelector<HTMLElement>('[data-testid="bgm-slot-badge-3"]');
    expect(badge1?.textContent?.trim()).toBe('#1');
    expect(badge2?.textContent?.trim()).toBe('#2');
    expect(badge3?.textContent?.trim()).toBe('#3');

    // Click "スロット追加" button
    const addBtn = target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]');
    expect(addBtn).toBeTruthy();
    addBtn!.click();
    await new Promise((r) => setTimeout(r, 20));

    // Verify slot 4 (u32: 4) is added and rendered
    expect(lastUpdatedScene).toBeTruthy();
    const slot4 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 4);
    expect(slot4).toBeTruthy();
    expect(slot4?.slot).toBe(4);
    expect(slot4?.tinId).toBeNull();
    expect(slot4?.volume).toBe(0.8);

    const row4 = target.querySelector<HTMLElement>('[data-testid="bgm-row-4"]');
    expect(row4).toBeTruthy();
    const badge4 = target.querySelector<HTMLElement>('[data-testid="bgm-slot-badge-4"]');
    expect(badge4?.textContent?.trim()).toBe('#4');

    unmount(instance);
    target.remove();
  });
});

describe('BGM Controls — server acknowledgement (Issue #109 / #142)', () => {
  beforeEach(() => {
    clearAssets();
  });

  function mountWithSocket(
    scene: Scene,
    socket: FakeRoomSocket,
    onUpdateScene?: (updated: Scene) => void,
  ) {
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene,
        allScenes: [scene],
        roomSocket: asRoomSocket(socket),
        ...(onUpdateScene ? { onUpdateScene } : {}),
      },
    });
    return { target, instance };
  }

  // spec: argenterie/bgm-controls 再生/停止はUpdateState/DeleteStateを送り、ack後にのみローカル確定
  it('sends UpdateState for play, shows the wavy spinner while pending, and commits only after the acks', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgm1: TIN_ID_A,
      },
    });
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;

    let updateCount = 0;
    let lastUpdatedScene: Scene | null = null;
    const { target, instance } = mountWithSocket(scene, socket, (updated) => {
      updateCount += 1;
      lastUpdatedScene = updated;
    });

    const playBtn = target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]')!;
    const selectBtn = target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-1"]')!;
    expect(playBtn.getAttribute('aria-label')).toBe('BGM再生');

    playBtn.click();
    await settle(0);

    // Both playback intent and the legacy track migration are in flight, and
    // the select icon is the wavy spinner.
    const playingWrite = socket.requestUpdateStateCalls.find(
      (call) => call.key === 'bgm::1::playing',
    );
    const trackWrite = socket.requestUpdateStateCalls.find((call) => call.key === 'std::bgm');
    expect(playingWrite?.value).toBe(true);
    expect(trackWrite?.value).toMatchObject({ tinId: TIN_ID_A, mimeType: 'audio/mpeg' });
    expect(target.querySelector('[data-testid="bgm-pending-1"]')).toBeTruthy();
    expect(selectBtn.disabled).toBe(true);
    expect(playBtn.disabled).toBe(true);
    // The play/stop icon itself is not replaced.
    expect(playBtn.textContent).toContain('play_arrow');
    expect(updateCount).toBe(0);

    // The first StateUpdated ack arrives: one write is still outstanding.
    socket.resolveStateWrite();
    await settle(0);
    expect(target.querySelector('[data-testid="bgm-pending-1"]')).toBeTruthy();
    expect(updateCount).toBe(0);

    // The second StateUpdated ack arrives: now the local model is committed once.
    socket.resolveStateWrite();
    await settle();
    expect(target.querySelector('[data-testid="bgm-pending-1"]')).toBeNull();
    expect(updateCount).toBe(1);
    const slot1 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 1);
    expect(slot1?.tinId).toBe(TIN_ID_A);
    expect(slot1?.isPlaying).toBe(true);
    expect(playBtn.getAttribute('aria-label')).toBe('BGM停止');

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 停止は bgm::<slot>::playing=false (Issue #164)
  it('sends playing=false when stopping a playing slot', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: TIN_ID_A, volume: 0.8, isPlaying: true },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;

    let lastUpdatedScene: Scene | null = null;
    const { target, instance } = mountWithSocket(scene, socket, (updated) => {
      lastUpdatedScene = updated;
    });

    const playBtn = target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]')!;
    expect(playBtn.getAttribute('aria-label')).toBe('BGM停止');

    playBtn.click();
    await settle(0);

    expect(socket.requestDeleteStateCalls).toHaveLength(0);
    expect(socket.requestUpdateStateCalls).toHaveLength(1);
    expect(socket.requestUpdateStateCalls[0].key).toBe('bgm::1::playing');
    expect(socket.requestUpdateStateCalls[0].value).toBe(false);
    expect(lastUpdatedScene).toBeNull();

    socket.resolveStateWrite();
    await settle();

    const slot1 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 1);
    // Issue #164: the track stays selected while stopped.
    expect(slot1?.tinId).toBe(TIN_ID_A);
    expect(slot1?.isPlaying).toBe(false);
    expect(lastUpdatedScene!.juices.bgm1).toBe(TIN_ID_A);
    expect(playBtn.getAttribute('aria-label')).toBe('BGM再生');

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls タイムアウト時はローカル状態を変更せずエラーを表示
  it('shows the communication error and keeps the previous state when an ack times out', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: TIN_ID_A, volume: 0.8, isPlaying: true },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    let updateCount = 0;
    const { target, instance } = mountWithSocket(scene, socket, () => {
      updateCount += 1;
    });

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]')!.click();
    await settle(0);

    socket.rejectStateWrite(new OperationTimeoutError('updateState', 8000));
    await settle();

    // No local mutation and no onUpdateScene; the row shows the error.
    expect(updateCount).toBe(0);
    const slot1 = scene.juices.bgmSlots?.find((s) => s.slot === 1);
    expect(slot1?.tinId).toBe(TIN_ID_A);
    expect(slot1?.isPlaying).toBe(true);
    const error = target.querySelector<HTMLElement>('[data-testid="bgm-error-1"]');
    expect(error?.getAttribute('role')).toBe('status');
    expect(error?.textContent).toContain(ERROR_MESSAGE);
    expect(target.querySelector('[data-testid="bgm-pending-1"]')).toBeNull();
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]')!.getAttribute('aria-label'),
    ).toBe('BGM停止');

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls PermissionDeniedは区別してログしUIを壊さない
  it('logs PermissionDenied distinctly and keeps the UI usable', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: TIN_ID_A, volume: 0.8, isPlaying: true },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const { target, instance } = mountWithSocket(scene, socket);

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]')!.click();
    await settle(0);
    socket.rejectStateWrite(new ServerOperationError('PermissionDenied', 'admin only'));
    await settle();

    expect(
      warn.mock.calls.some((call) => String(call[0]).includes('PermissionDenied')),
    ).toBe(true);
    expect(target.querySelector('[data-testid="bgm-error-1"]')?.textContent).toContain(ERROR_MESSAGE);
    // The buttons are re-enabled after the failure.
    expect(target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]')!.disabled).toBe(false);

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 音量は確定(change)時のみ bgm::<slot>::volume を送る
  it('sends bgm::<slot>::volume only on a committed volume change', async () => {
    const scene = makeScene();
    const socket = new FakeRoomSocket();

    let updateCount = 0;
    let lastUpdatedScene: Scene | null = null;
    const { target, instance } = mountWithSocket(scene, socket, (updated) => {
      updateCount += 1;
      lastUpdatedScene = updated;
    });

    const sliderWrapper = target.querySelector<HTMLElement>('[data-testid="bgm-slider-wrapper-1"]')!;
    const inputEl =
      sliderWrapper.querySelector<HTMLInputElement>('input[type="range"]') ??
      sliderWrapper.querySelector<HTMLInputElement>('input');
    expect(inputEl).toBeTruthy();

    inputEl!.value = '35';
    inputEl!.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    // Input ticks only preview the label; nothing is sent or committed.
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(target.querySelector('[data-testid="bgm-vol-label-1"]')?.textContent?.trim()).toBe('35%');
    expect(updateCount).toBe(0);

    inputEl!.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();

    expect(socket.requestUpdateStateCalls).toHaveLength(1);
    expect(socket.requestUpdateStateCalls[0].key).toBe('bgm::1::volume');
    expect(socket.requestUpdateStateCalls[0].value).toBeCloseTo(0.35, 5);
    expect(updateCount).toBe(1);
    const slot1 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 1);
    expect(slot1?.volume).toBeCloseTo(0.35, 5);
    expect(target.querySelector('[data-testid="bgm-vol-label-1"]')?.textContent?.trim()).toBe('35%');

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 音量失敗時はドラフトを戻しエラー表示
  it('reverts the volume preview and shows the error when the volume ack fails', async () => {
    const scene = makeScene();
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const { target, instance } = mountWithSocket(scene, socket);
    const sliderWrapper = target.querySelector<HTMLElement>('[data-testid="bgm-slider-wrapper-1"]')!;
    const inputEl =
      sliderWrapper.querySelector<HTMLInputElement>('input[type="range"]') ??
      sliderWrapper.querySelector<HTMLInputElement>('input');

    inputEl!.value = '35';
    inputEl!.dispatchEvent(new Event('input', { bubbles: true }));
    inputEl!.dispatchEvent(new Event('change', { bubbles: true }));
    await settle(0);
    expect(socket.pendingStateOperationCount).toBe(1);

    socket.rejectStateWrite(new OperationTimeoutError('updateState', 8000));
    await settle();

    expect(target.querySelector('[data-testid="bgm-vol-label-1"]')?.textContent?.trim()).toBe('80%');
    expect(target.querySelector('[data-testid="bgm-error-1"]')?.textContent).toContain(ERROR_MESSAGE);
    expect(scene.juices.bgmSlots?.find((s) => s.slot === 1)?.volume).toBe(0.8);

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls アセット選択は64桁hexのtinIdを使いdata URLを使わない
  it('uses the selected asset tinId (64 hex) for the state writes', async () => {
    const audioAsset = addAsset({
      id: 'bgm-track-tin',
      name: 'tin_track.ogg',
      scope: 'personal',
      mediaType: 'audio',
      dataUrl: 'data:audio/ogg;base64,VElO',
      mimeType: 'audio/ogg',
      createdAt: 6000,
      tinId: TIN_ID_B,
    });
    const scene = makeScene();
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;

    let updateCount = 0;
    let lastUpdatedScene: Scene | null = null;
    let requestedOptions: any = null;
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene,
        allScenes: [scene],
        roomSocket: asRoomSocket(socket),
        onRequestAssetPicker: (options) => {
          requestedOptions = options;
        },
        onUpdateScene: (updated) => {
          updateCount += 1;
          lastUpdatedScene = updated;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-2"]')!.click();
    await settle();
    expect(requestedOptions).toBeTruthy();

    requestedOptions.onSelect(audioAsset.dataUrl, audioAsset);
    await settle(0);

    const playingWrite = socket.requestUpdateStateCalls.find(
      (call) => call.key === 'bgm::2::playing',
    );
    const trackWrite = socket.requestUpdateStateCalls.find((call) => call.key === 'std::bgm2');
    expect(playingWrite?.value).toBe(true);
    expect(trackWrite?.value).toMatchObject({
      tinId: TIN_ID_B,
      mimeType: 'audio/ogg',
      name: 'tin_track.ogg',
    });
    expect(trackWrite?.value).not.toBe(audioAsset.dataUrl);
    expect(target.querySelector('[data-testid="bgm-pending-2"]')).toBeTruthy();
    expect(updateCount).toBe(0);

    socket.resolveStateWrite();
    socket.resolveStateWrite();
    await settle();

    const slot2 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 2);
    expect(slot2?.tinId).toBe(TIN_ID_B);
    expect(slot2?.mimeType).toBe('audio/ogg');
    expect(slot2?.name).toBe('tin_track.ogg');
    expect(slot2?.isPlaying).toBe(true);
    expect(updateCount).toBe(1);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls tinIdのない旧アセットはローカル状態を更新しない
  it('rejects a legacy audio asset without a tinId and leaves the local state untouched', async () => {
    const legacyAsset = addAsset({
      id: 'bgm-track-legacy',
      name: 'legacy.ogg',
      scope: 'personal',
      mediaType: 'audio',
      dataUrl: 'data:audio/ogg;base64,TEdDWQ==',
      mimeType: 'audio/ogg',
      createdAt: 7000,
    });
    const scene = makeScene();
    const socket = new FakeRoomSocket();
    let updateCount = 0;
    let requestedOptions: any = null;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene,
        allScenes: [scene],
        roomSocket: asRoomSocket(socket),
        onRequestAssetPicker: (options) => {
          requestedOptions = options;
        },
        onUpdateScene: () => {
          updateCount += 1;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-2"]')!.click();
    await settle();
    requestedOptions.onSelect(legacyAsset.dataUrl, legacyAsset);
    await settle();

    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(updateCount).toBe(0);
    expect(scene.juices.bgmSlots?.find((s) => s.slot === 2)?.tinId).toBeNull();
    const error = target.querySelector<HTMLElement>('[data-testid="bgm-error-2"]');
    expect(error?.getAttribute('role')).toBe('status');
    expect(error?.textContent).not.toBe('');

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });
});
