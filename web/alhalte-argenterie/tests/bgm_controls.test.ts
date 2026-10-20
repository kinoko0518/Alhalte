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
    const deleteBtn = target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-1"]');

    expect(playBtn).toBeTruthy();
    expect(selectBtn).toBeTruthy();
    expect(sliderWrapper).toBeTruthy();
    expect(deleteBtn).toBeTruthy();

    // Verify DOM order: playBtn -> selectBtn -> sliderWrapper -> deleteBtn
    const children = Array.from(row1!.children);
    const playIndex = children.indexOf(playBtn!);
    const selectIndex = children.indexOf(selectBtn!);
    const sliderIndex = children.indexOf(sliderWrapper!);
    const deleteIndex = children.indexOf(deleteBtn!);

    expect(playIndex).toBeLessThan(selectIndex);
    expect(selectIndex).toBeLessThan(sliderIndex);
    expect(sliderIndex).toBeLessThan(deleteIndex);
    // The delete button is the last control in the row (the tooltip element
    // noph-ui renders next to it is not a control).
    expect(Array.from(row1!.querySelectorAll('button')).at(-1)).toBe(deleteBtn);

    // Verify M3 IconButton components used
    expect(playBtn!.classList.contains('np-icon-button')).toBe(true);
    expect(selectBtn!.classList.contains('np-icon-button')).toBe(true);
    expect(deleteBtn!.classList.contains('np-icon-button')).toBe(true);

    // Verify narrower width for Play/Stop button compared to 1:1 square Select button
    expect(playBtn!.classList.contains('narrow')).toBe(true);
    expect(playBtn!.classList.contains('narrow-btn')).toBe(true);
    expect(selectBtn!.classList.contains('square-btn')).toBe(true);

    // Issue #226: the slot number badge is gone; a filled tonal delete button
    // takes its place at the right end of the row.
    expect(target.querySelector('[data-testid="bgm-slot-badge-1"]')).toBeNull();
    expect(deleteBtn!.classList.contains('tonal')).toBe(true);
    expect(deleteBtn!.classList.contains('s')).toBe(true);
    expect(deleteBtn!.classList.contains('round')).toBe(true);
    expect(deleteBtn!.getAttribute('aria-label')).toBe('BGMスロットを削除');
    expect(deleteBtn!.textContent).toContain('delete');

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

    // Issue #226: the slot number badge is gone; every row ends with a delete
    // button instead.
    expect(target.querySelector('[data-testid="bgm-slot-badge-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-slot-badge-2"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-slot-badge-3"]')).toBeNull();
    for (const slot of [1, 2, 3]) {
      const row = target.querySelector<HTMLElement>(`[data-testid="bgm-row-${slot}"]`);
      const deleteBtn = target.querySelector<HTMLButtonElement>(
        `[data-testid="bgm-delete-btn-${slot}"]`,
      );
      expect(row).toBeTruthy();
      expect(deleteBtn).toBeTruthy();
      expect(row!.contains(deleteBtn!)).toBe(true);
    }

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
    expect(target.querySelector('[data-testid="bgm-slot-badge-4"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-delete-btn-4"]')).toBeTruthy();

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

describe('BGM slot deletion (Issue #226)', () => {
  beforeEach(() => {
    clearAssets();
  });

  function playableScene(): Scene {
    return makeScene({
      juices: {
        ...createDefaultJuices(),
        bgm1: TIN_ID_A,
        bgm1Volume: 0.7,
        bgmSlots: [
          {
            slot: 1,
            tinId: TIN_ID_A,
            mimeType: 'audio/ogg',
            name: 'theme.ogg',
            volume: 0.7,
            isPlaying: true,
          },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });
  }

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

  // spec: argenterie/bgm-controls 削除はtrack DeleteState / playing=false / volume DeleteStateのack後に確定
  it('deletes the slot only after the track / playing / volume acks and clears the legacy fields', async () => {
    const scene = playableScene();
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;

    let updateCount = 0;
    let lastUpdatedScene: Scene | null = null;
    const { target, instance } = mountWithSocket(scene, socket, (updated) => {
      updateCount += 1;
      lastUpdatedScene = updated;
    });

    const deleteBtn =
      target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-1"]')!;
    expect(deleteBtn.getAttribute('aria-label')).toBe('BGMスロットを削除');

    deleteBtn.click();
    await settle(0);

    // The three NONE writes are in flight; a second press is ignored.
    deleteBtn.click();
    await settle(0);
    expect(socket.pendingStateOperationCount).toBe(3);
    expect(socket.requestDeleteStateCalls.map((call) => call.key)).toEqual([
      'std::bgm',
      'bgm::1::volume',
    ]);
    expect(socket.requestUpdateStateCalls).toHaveLength(1);
    expect(socket.requestUpdateStateCalls[0].key).toBe('bgm::1::playing');
    expect(socket.requestUpdateStateCalls[0].value).toBe(false);
    expect(socket.requestDeleteStateCalls[0].sceneId).toBe(scene.id);
    // Issue #226: the persistent removal marker is only sent after the three
    // NONE writes succeeded, never in the same batch.
    expect(
      socket.requestUpdateStateCalls.some((call) => call.key === 'bgm::1::removed'),
    ).toBe(false);

    // No local removal before every ack succeeded.
    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();
    expect(updateCount).toBe(0);
    expect(scene.juices.bgmSlots?.some((slot) => slot.slot === 1)).toBe(true);
    expect(scene.juices.bgm1).toBe(TIN_ID_A);
    expect(scene.juices.bgmRemovedSlots ?? []).not.toContain(1);

    socket.resolveStateWrite();
    socket.resolveStateWrite();
    await settle();
    // One NONE ack is still outstanding: the row and the model stay.
    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();
    expect(updateCount).toBe(0);

    socket.resolveStateWrite();
    await settle(0);
    // All NONE writes are acknowledged: the marker is sent as a second step,
    // and the row stays until its own ack arrives.
    expect(
      socket.requestUpdateStateCalls.some(
        (call) => call.key === 'bgm::1::removed' && call.value === true,
      ),
    ).toBe(true);
    expect(socket.pendingStateOperationCount).toBe(1);
    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();
    expect(updateCount).toBe(0);

    socket.resolveStateWrite();
    await settle();

    // The row is gone; the remaining slots keep their numbers and state.
    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-delete-btn-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-row-2"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-row-3"]')).toBeTruthy();
    expect(updateCount).toBe(1);
    expect(lastUpdatedScene!.juices.bgmSlots?.map((slot) => slot.slot)).toEqual([2, 3]);
    expect(lastUpdatedScene!.juices.bgmRemovedSlots).toEqual([1]);
    expect(lastUpdatedScene!.juices.bgm1).toBeUndefined();
    expect(lastUpdatedScene!.juices.bgm1Volume).toBe(0.8);
    expect(lastUpdatedScene!.juices.bgmSlots?.find((slot) => slot.slot === 2)).toMatchObject({
      tinId: null,
      isPlaying: false,
    });

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 削除失敗時は行を残しBGM通信エラーを表示する
  it('keeps the row and shows the BGM error when a delete ack is rejected', async () => {
    const scene = playableScene();
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    let updateCount = 0;
    const { target, instance } = mountWithSocket(scene, socket, () => {
      updateCount += 1;
    });

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-1"]')!.click();
    await settle(0);

    socket.rejectDeleteState(new OperationTimeoutError('deleteState', 8000));
    // The other two NONE requests settle successfully; the batch still fails.
    socket.resolveStateWrite();
    socket.resolveStateWrite();
    await settle();

    expect(updateCount).toBe(0);
    const row1 = target.querySelector<HTMLElement>('[data-testid="bgm-row-1"]');
    expect(row1).toBeTruthy();
    expect(scene.juices.bgmSlots?.map((slot) => slot.slot)).toEqual([1, 2, 3]);
    expect(scene.juices.bgmSlots?.find((slot) => slot.slot === 1)?.tinId).toBe(TIN_ID_A);
    expect(scene.juices.bgmSlots?.find((slot) => slot.slot === 1)?.isPlaying).toBe(true);
    expect(scene.juices.bgm1).toBe(TIN_ID_A);
    // Issue #226: the removal marker is never sent when a NONE write failed,
    // and it is not applied locally either.
    expect(
      socket.requestUpdateStateCalls.some((call) => call.key === 'bgm::1::removed'),
    ).toBe(false);
    expect(scene.juices.bgmRemovedSlots ?? []).not.toContain(1);
    const error = target.querySelector<HTMLElement>('[data-testid="bgm-error-1"]');
    expect(error?.getAttribute('role')).toBe('status');
    expect(error?.textContent).toContain(ERROR_MESSAGE);
    // The button is usable again after the failure.
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-1"]')!.disabled,
    ).toBe(false);

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls playing更新が失敗したらremovedマーカーを書かない
  it('does not write the removal marker when the playing update fails', async () => {
    const scene = playableScene();
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { target, instance } = mountWithSocket(scene, socket);

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-1"]')!.click();
    await settle(0);

    socket.resolveStateWrite(); // track DeleteState
    socket.rejectStateWrite(new OperationTimeoutError('updateState', 8000)); // playing
    socket.resolveStateWrite(); // volume DeleteState
    await settle();

    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-error-1"]')?.textContent).toContain(
      ERROR_MESSAGE,
    );
    expect(
      socket.requestUpdateStateCalls.some((call) => call.key === 'bgm::1::removed'),
    ).toBe(false);

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls volume削除が失敗したらremovedマーカーを書かない
  it('does not write the removal marker when the volume delete fails', async () => {
    const scene = playableScene();
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { target, instance } = mountWithSocket(scene, socket);

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-1"]')!.click();
    await settle(0);

    socket.resolveStateWrite(); // track DeleteState
    socket.rejectDeleteState(new OperationTimeoutError('deleteState', 8000)); // volume
    socket.resolveStateWrite(); // playing UpdateState
    await settle();

    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-error-1"]')?.textContent).toContain(
      ERROR_MESSAGE,
    );
    expect(
      socket.requestUpdateStateCalls.some((call) => call.key === 'bgm::1::removed'),
    ).toBe(false);

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls standaloneではサーバー書き込みなしでローカル削除する
  it('removes the slot locally without server writes in standalone usage', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgm2: TIN_ID_B,
        bgm2Volume: 0.4,
        bgmSlots: [
          { slot: 1, tinId: null, volume: 0.8, isPlaying: false },
          {
            slot: 2,
            tinId: TIN_ID_B,
            mimeType: 'audio/ogg',
            name: 'second.ogg',
            volume: 0.4,
            isPlaying: false,
          },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });

    let updateCount = 0;
    let lastUpdatedScene: Scene | null = null;
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene,
        allScenes: [scene],
        onUpdateScene: (updated) => {
          updateCount += 1;
          lastUpdatedScene = updated;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-2"]')!.click();
    await settle();

    expect(updateCount).toBe(1);
    expect(target.querySelector('[data-testid="bgm-row-2"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-row-3"]')).toBeTruthy();
    expect(lastUpdatedScene!.juices.bgmSlots?.map((slot) => slot.slot)).toEqual([1, 3]);
    expect(lastUpdatedScene!.juices.bgmRemovedSlots).toEqual([2]);
    expect(lastUpdatedScene!.juices.bgm2).toBeUndefined();
    expect(lastUpdatedScene!.juices.bgm2Volume).toBe(0.8);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 削除後のスロット追加はmax+1で削除行を復活させない
  it('adds the next slot at max + 1 after a deletion and does not revive the deleted row', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: null, volume: 0.8, isPlaying: false },
          {
            slot: 2,
            tinId: TIN_ID_B,
            mimeType: 'audio/ogg',
            name: 'second.ogg',
            volume: 0.5,
            isPlaying: false,
          },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
        ],
      },
    });

    let lastUpdatedScene: Scene | null = null;
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene,
        allScenes: [scene],
        onUpdateScene: (updated) => {
          lastUpdatedScene = updated;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-2"]')!.click();
    await settle();
    expect(target.querySelector('[data-testid="bgm-row-2"]')).toBeNull();
    expect(lastUpdatedScene!.juices.bgmRemovedSlots).toEqual([2]);

    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.click();
    await settle();

    // max(1, 3) + 1 = 4, not the deleted 2.
    expect(target.querySelector('[data-testid="bgm-row-2"]')).toBeNull();
    const row4 = target.querySelector<HTMLElement>('[data-testid="bgm-row-4"]');
    expect(row4).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-delete-btn-4"]')).toBeTruthy();
    expect(lastUpdatedScene!.juices.bgmSlots?.map((slot) => slot.slot)).toEqual([1, 3, 4]);
    // The new slot was never removed, so the removal markers stay at [2].
    expect(lastUpdatedScene!.juices.bgmRemovedSlots).toEqual([2]);
    const slot4 = lastUpdatedScene!.juices.bgmSlots?.find((slot) => slot.slot === 4);
    expect(slot4).toMatchObject({ tinId: null, isPlaying: false, volume: 0.8 });

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 再追加したスロットは最初のtrack書き込み前にremovedマーカーを消す
  it('clears the server removal marker before writing the first track of a re-added slot', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
          {
            slot: 4,
            tinId: TIN_ID_A,
            mimeType: 'audio/ogg',
            name: 'fourth.ogg',
            volume: 0.8,
            isPlaying: false,
          },
        ],
      },
    });
    const socket = new FakeRoomSocket();
    let requestedOptions: any = null;
    let lastUpdatedScene: Scene | null = null;
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
          lastUpdatedScene = updated;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-4"]')!.click();
    await settle();
    expect(scene.juices.bgmRemovedSlots).toEqual([4]);

    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.click();
    await settle();
    // The local marker is dropped immediately so the row renders.
    expect(scene.juices.bgmRemovedSlots).toEqual([]);

    const audioAsset = addAsset({
      id: 'bgm-track-readded',
      name: 'readded.ogg',
      scope: 'personal',
      mediaType: 'audio',
      dataUrl: 'data:audio/ogg;base64,UkVBRA==',
      mimeType: 'audio/ogg',
      createdAt: 8000,
      tinId: TIN_ID_B,
    });
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-4"]')!.click();
    await settle();
    expect(requestedOptions).toBeTruthy();
    requestedOptions.onSelect(audioAsset.dataUrl, audioAsset);
    await settle();

    // Issue #226: DeleteState(`bgm::4::removed`) is sent together with the
    // first track write so the marker cannot hide the re-added row elsewhere.
    expect(
      socket.requestDeleteStateCalls.some((call) => call.key === 'bgm::4::removed'),
    ).toBe(true);
    expect(
      socket.requestUpdateStateCalls.some(
        (call) => call.key === 'bgm::4::playing' && call.value === true,
      ),
    ).toBe(true);
    expect(
      socket.requestUpdateStateCalls.some((call) => call.key === 'std::bgm4'),
    ).toBe(true);
    expect(lastUpdatedScene!.juices.bgmRemovedSlots).toEqual([]);
    expect(
      lastUpdatedScene!.juices.bgmSlots?.find((slot) => slot.slot === 4)?.tinId,
    ).toBe(TIN_ID_B);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 再追加時はremovedマーカー削除のack後にtrack/playingを送る
  it('awaits the removal-marker deletion before sending the re-added track write', async () => {
    const scene = makeScene({
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          { slot: 1, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 2, tinId: null, volume: 0.8, isPlaying: false },
          { slot: 3, tinId: null, volume: 0.8, isPlaying: false },
          {
            slot: 4,
            tinId: TIN_ID_A,
            mimeType: 'audio/ogg',
            name: 'fourth.ogg',
            volume: 0.8,
            isPlaying: false,
          },
        ],
      },
    });
    const socket = new FakeRoomSocket();
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
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="bgm-delete-btn-4"]')!.click();
    await settle();
    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.click();
    await settle();

    // Hold the marker deletion so the ordering is observable.
    socket.autoAnswerStateWrites = false;
    const updatesBeforeTrackWrite = socket.requestUpdateStateCalls.length;
    const audioAsset = addAsset({
      id: 'bgm-track-readded-order',
      name: 'readded.ogg',
      scope: 'personal',
      mediaType: 'audio',
      dataUrl: 'data:audio/ogg;base64,T1JERVI=',
      mimeType: 'audio/ogg',
      createdAt: 8100,
      tinId: TIN_ID_B,
    });
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-4"]')!.click();
    await settle();
    requestedOptions.onSelect(audioAsset.dataUrl, audioAsset);
    await settle(0);

    // Only the marker deletion is in flight; the track / playing writes must
    // not be sent before it is acknowledged.
    expect(socket.pendingStateOperationCount).toBe(1);
    expect(
      socket.requestDeleteStateCalls.filter((call) => call.key === 'bgm::4::removed'),
    ).toHaveLength(1);
    const updatesBeforeMarkerAck = socket.requestUpdateStateCalls.slice(
      updatesBeforeTrackWrite,
    );
    expect(
      updatesBeforeMarkerAck.some((call) => call.key === 'bgm::4::playing'),
    ).toBe(false);
    expect(
      updatesBeforeMarkerAck.some((call) => call.key === 'std::bgm4'),
    ).toBe(false);

    socket.resolveStateWrite();
    await settle(0);
    expect(
      socket.requestUpdateStateCalls.some(
        (call) => call.key === 'bgm::4::playing' && call.value === true,
      ),
    ).toBe(true);
    expect(
      socket.requestUpdateStateCalls.some((call) => call.key === 'std::bgm4'),
    ).toBe(true);
    expect(socket.pendingStateOperationCount).toBe(2);

    socket.resolveStateWrite();
    socket.resolveStateWrite();
    await settle();
    expect(scene.juices.bgmRemovedSlots).toEqual([]);
    expect(
      scene.juices.bgmSlots?.find((slot) => slot.slot === 4)?.tinId,
    ).toBe(TIN_ID_B);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/bgm-controls 全スロット削除後も空のスロット一覧を正とする
  it('keeps the slot list empty when every slot is deleted, and re-adds from slot 1', async () => {
    const scene = makeScene();
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene,
        allScenes: [scene],
      },
    });

    for (const slot of [1, 2, 3]) {
      target.querySelector<HTMLButtonElement>(`[data-testid="bgm-delete-btn-${slot}"]`)!.click();
      await settle();
    }

    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-row-2"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-row-3"]')).toBeNull();
    expect(scene.juices.bgmSlots).toEqual([]);
    expect(scene.juices.bgmRemovedSlots).toEqual([1, 2, 3]);

    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.click();
    await settle();

    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();
    expect(scene.juices.bgmSlots?.map((slot) => slot.slot)).toEqual([1]);
    expect(scene.juices.bgmSlots?.[0]).toMatchObject({ tinId: null, isPlaying: false });
    // Adding slot 1 again re-opens it: the local marker is cleared and the
    // server marker is deleted with the next track write.
    expect(scene.juices.bgmRemovedSlots).toEqual([2, 3]);

    unmount(instance);
    target.remove();
  });
});
