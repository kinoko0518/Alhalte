import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  calculateDimensionsAndCrop,
  createDefaultJuices,
  inheritJuices,
  moveMemberToScene,
} from '../src/utils/sceneSettings.js';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import type { Character, Scene } from '../src/types/index.js';

describe('Scene Settings & Juices', () => {
  // spec: alhalte-argenterie/scene-settings 背景画像縦横自動計算
  it('automatically calculates the other dimension from aspect ratio when only one is specified', () => {
    // 1920x1080 (16:9 = 1.7777...)
    const params = { naturalWidth: 1920, naturalHeight: 1080 };

    // Specify width only
    const resWidth = calculateDimensionsAndCrop({ ...params, width: 960 });
    expect(resWidth.width).toBe(960);
    expect(resWidth.height).toBe(540); // 960 / (1920/1080)
    expect(resWidth.cropped).toBe(false);

    // Specify height only
    const resHeight = calculateDimensionsAndCrop({ ...params, height: 720 });
    expect(resHeight.height).toBe(720);
    expect(resHeight.width).toBe(1280); // 720 * (1920/1080)
    expect(resHeight.cropped).toBe(false);
  });

  // spec: alhalte-argenterie/scene-settings 背景画像端切り落とし
  it('crops edges when both width and height are specified', () => {
    // 1920x1080 (16:9), target requested 1000x1000 (1:1)
    const res = calculateDimensionsAndCrop({
      naturalWidth: 1920,
      naturalHeight: 1080,
      width: 1000,
      height: 1000,
    });

    expect(res.width).toBe(1000);
    expect(res.height).toBe(1000);
    expect(res.cropped).toBe(true);
    expect(res.cropWidth).toBe(1080); // Cropped horizontally
    expect(res.cropX).toBe(420); // (1920 - 1080) / 2
  });

  // spec: alhalte-argenterie/scene-settings Juices継承
  it('inherits Juices (background, dimensions, BGM1..3 and volumes) from source scene', () => {
    const parentJuices = {
      bgImage: 'https://alhalte.example/bg.webp',
      bgWidth: 1920,
      bgHeight: 1080,
      bgAspectRatio: 1920 / 1080,
      bgm1: 'https://alhalte.example/bgm1.ogg',
      bgm1Volume: 0.7,
      bgm2: 'https://alhalte.example/bgm2.ogg',
      bgm2Volume: 0.5,
      bgm3: undefined,
      bgm3Volume: 0.8,
    };

    const inherited = inheritJuices(parentJuices);
    expect(inherited).toEqual(parentJuices);
    // Mutating copy should not affect parent
    inherited.bgm1Volume = 0.2;
    expect(parentJuices.bgm1Volume).toBe(0.7);
  });

  // spec: alhalte-argenterie/scene-settings 未存在シーンメンバー移動時の自動作成
  it('automatically creates a new scene inheriting Juices when moving member to a non-existent scene', () => {
    const currentScene: Scene = {
      id: 'scene-1',
      name: '本陣',
      juices: {
        bgImage: 'https://alhalte.example/camp.webp',
        bgWidth: 1280,
        bgHeight: 720,
        bgAspectRatio: 16 / 9,
        bgm1: 'march.ogg',
        bgm1Volume: 0.9,
        bgm2: undefined,
        bgm2Volume: 0.8,
        bgm3: undefined,
        bgm3Volume: 0.8,
      },
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: ['char-hero'],
    };

    const existingScenes = [currentScene];

    // Move member to a non-existent scene name "前線基地"
    const result = moveMemberToScene('char-hero', '前線基地', currentScene, existingScenes);

    expect(result.isNewSceneCreated).toBe(true);
    expect(result.targetScene.name).toBe('前線基地');
    expect(result.targetScene.members).toContain('char-hero');

    // Verify Juices were inherited
    expect(result.targetScene.juices.bgImage).toBe('https://alhalte.example/camp.webp');
    expect(result.targetScene.juices.bgm1).toBe('march.ogg');
    expect(result.targetScene.juices.bgm1Volume).toBe(0.9);

    // Old scene should no longer have char-hero
    const updatedOld = result.updatedScenes.find((s) => s.id === 'scene-1');
    expect(updatedOld?.members).not.toContain('char-hero');
  });

  // spec: alhalte-argenterie/scene-settings SceneSettingsTab コンポーネント描画
  it('renders SceneSettingsTab with split button, Juices controls, and delete confirmation dialog', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const currentScene: Scene = {
      id: 'scene-test',
      name: '設定対象シーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const characters: Character[] = [
      {
        id: 'char-1',
        name: 'エルフ',
        initiative: 12,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];

    let deletedSceneId: string | null = null;
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        characters,
        onDeleteScene: (id) => {
          deletedSceneId = id;
        },
      },
    });

    // Check scene name split button
    expect(target.querySelector('[data-testid="scene-name-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="scene-split-dropdown-toggle"]')).toBeTruthy();

    // Check Juices headings and controls
    expect(target.querySelector('[data-testid="heading-juices"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-image-card"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-description-text"]')?.textContent).toContain(
      '画像は一方を指定すればもう一方を自動で計算します',
    );
    expect(target.querySelector('[data-testid="bg-width-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-height-input"]')).toBeTruthy();

    // Check BGM pills 1 to 3
    expect(target.querySelector('[data-testid="bgm-pill-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-pill-2"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-pill-3"]')).toBeTruthy();

    // Check Management heading and buttons
    expect(target.querySelector('[data-testid="heading-management"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="open-move-members-btn"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="open-create-scene-btn"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="delete-scene-btn"]')).toBeTruthy();

    // Click red "シーンを削除" button -> opens full-screen confirmation dialog
    target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="delete-confirm-dialog"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="delete-dialog-title"]')?.textContent).toContain(
      '設定対象シーン',
    );

    // Confirm deletion
    target.querySelector<HTMLButtonElement>('[data-testid="confirm-delete-button"]')?.click();
    expect(deletedSceneId).toBe('scene-test');

    unmount(instance);
    target.remove();
  });
});
