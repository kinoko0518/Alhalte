import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { clearAssets, addAsset } from '../src/utils/assets.js';
import type { Scene } from '../src/types/index.js';

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
            tinId: 'audio-tin-123',
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

    // 1. Click to stop -> tinId becomes null (None), isPlaying becomes false
    playBtn!.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(lastUpdatedScene).toBeTruthy();
    const updatedSlot1 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 1);
    expect(updatedSlot1?.tinId).toBeNull();
    expect(updatedSlot1?.isPlaying).toBe(false);

    // Label and icon toggle to "BGM再生" and "play_arrow"
    expect(playBtn!.getAttribute('aria-label')).toBe('BGM再生');
    expect(playBtn!.textContent).toContain('play_arrow');

    // 2. Click to resume play -> restores previous TinId ('audio-tin-123')
    playBtn!.click();
    await new Promise((r) => setTimeout(r, 20));

    const resumedSlot1 = lastUpdatedScene!.juices.bgmSlots?.find((s) => s.slot === 1);
    expect(resumedSlot1?.tinId).toBe('audio-tin-123');
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
    expect(updatedSlot2?.tinId).toBe(audioAsset.dataUrl);
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
