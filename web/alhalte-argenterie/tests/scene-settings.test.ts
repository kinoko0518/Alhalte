import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  calculateDimensionsAndCrop,
  createDefaultJuices,
  inheritJuices,
  moveMemberToScene,
} from '../src/utils/sceneSettings.js';
import {
  addAsset,
  clearAssets,
  loadAssets,
} from '../src/utils/assets.js';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import type { Character, Scene } from '../src/types/index.js';

describe('Scene Settings & Juices', () => {
  beforeEach(() => {
    clearAssets();
  });

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

  // spec: alhalte-argenterie/scene-settings SceneSettingsTab コンポーネント描画 & M3 Expressive UI (Issue #32)
  it('renders SceneSettingsTab with M3 Switch, Slider, 7:5:3 management row, and tonal delete button', async () => {
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
    let createdSceneData: { name: string; inheritJuice: boolean } | null = null;
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        characters,
        onCreateScene: (name, inheritJuice) => {
          createdSceneData = { name, inheritJuice };
        },
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
    expect(target.querySelector('[data-testid="bg-image-carousel"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-image-card"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-description-text"]')?.textContent).toContain(
      '画像は一方を指定すればもう一方を自動で計算します',
    );
    expect(target.querySelector('[data-testid="bg-width-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-height-input"]')).toBeTruthy();

    // Check BGM buttons 1 to 3 use noph-ui IconButton (.np-icon-button) and Sliders use noph-ui Slider (.np-slider) (Issue #53)
    const select1 = target.querySelector('[data-testid="bgm-select-btn-1"]');
    const select2 = target.querySelector('[data-testid="bgm-select-btn-2"]');
    const select3 = target.querySelector('[data-testid="bgm-select-btn-3"]');
    expect(select1?.classList.contains('np-icon-button')).toBe(true);
    expect(select2?.classList.contains('np-icon-button')).toBe(true);
    expect(select3?.classList.contains('np-icon-button')).toBe(true);

    const slider1 = target.querySelector('[data-testid="bgm-slider-1"]');
    const slider2 = target.querySelector('[data-testid="bgm-slider-2"]');
    const slider3 = target.querySelector('[data-testid="bgm-slider-3"]');
    expect(slider1?.closest('.np-slider')).toBeTruthy();
    expect(slider2?.closest('.np-slider')).toBeTruthy();
    expect(slider3?.closest('.np-slider')).toBeTruthy();

    // Check Management heading, action buttons, and embedded move members section (Issue #92)
    expect(target.querySelector('[data-testid="heading-management"]')).toBeTruthy();
    const colCreate = target.querySelector<HTMLElement>('[data-testid="action-col-create"]');
    const colDeleteLogs = target.querySelector<HTMLElement>('[data-testid="action-col-delete-logs"]');
    const colDeleteScene = target.querySelector<HTMLElement>('[data-testid="action-col-delete-scene"]');
    expect(colCreate?.style.flex).toContain('5');
    expect(colDeleteLogs?.style.flex).toContain('3.2');
    expect(colDeleteScene?.style.flex).toContain('2.2');

    const moveSection = target.querySelector('[data-testid="move-members-section"]');
    expect(moveSection).toBeTruthy();
    expect(target.querySelector('[data-testid="move-members-title"]')?.textContent).toContain('メンバーを移動');

    const createBtn = target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]');
    const deleteLogsBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-logs-btn"]');
    const deleteBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]');
    expect(createBtn?.classList.contains('np-button')).toBe(true);
    expect(createBtn?.classList.contains('m')).toBe(true);
    expect(createBtn?.classList.contains('square')).toBe(true);
    expect(deleteLogsBtn?.classList.contains('np-button')).toBe(true);
    expect(deleteLogsBtn?.classList.contains('m')).toBe(true);
    expect(deleteLogsBtn?.classList.contains('square')).toBe(true);
    expect(deleteBtn?.classList.contains('np-button')).toBe(true);
    expect(deleteBtn?.classList.contains('m')).toBe(true);
    expect(deleteBtn?.classList.contains('square')).toBe(true);
    expect(deleteBtn?.classList.contains('delete-scene-tonal-btn')).toBe(true);

    // Open "シーンを新規作成" overlay and verify M3 Switch (.np-switch) is used for "Juicesを継承"
    createBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    const inheritSwitchInput = target.querySelector<HTMLInputElement>(
      '[data-testid="inherit-juices-toggle"]',
    );
    expect(inheritSwitchInput).toBeTruthy();
    expect(inheritSwitchInput?.getAttribute('role')).toBe('switch');
    expect(inheritSwitchInput?.closest('.np-switch')).toBeTruthy();

    const newSceneInput = target.querySelector<HTMLInputElement>(
      '[data-testid="new-scene-name-input"]',
    );
    if (newSceneInput) {
      newSceneInput.value = '派生シーン';
      newSceneInput.dispatchEvent(new Event('input'));
    }
    target.querySelector<HTMLButtonElement>('[data-testid="submit-create-scene-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(createdSceneData).toEqual({ name: '派生シーン', inheritJuice: true });

    // Click tonal "シーンを削除" button -> opens full-screen confirmation dialog
    deleteBtn?.click();
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

  // spec: alhalte-argenterie/scene-settings アセット選択オーバーレイUIとカルーセル (Issue #28)
  it('opens AssetPickerModal for background image and BGM, supports scope/media toggles, upload, NONE at index 0, delete, and cancel', async () => {
    const personalImg = addAsset({
      id: 'img-personal-1',
      name: 'forest.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,Zm9yZXN0',
      mimeType: 'image/webp',
      createdAt: 1000,
    });
    const roomImg = addAsset({
      id: 'img-room-1',
      name: 'castle.webp',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,Y2FzdGxl',
      mimeType: 'image/webp',
      createdAt: 2000,
    });
    const personalAudio = addAsset({
      id: 'aud-personal-1',
      name: 'battle.ogg',
      scope: 'personal',
      mediaType: 'audio',
      dataUrl: 'data:audio/ogg;base64,YmF0dGxl',
      mimeType: 'audio/ogg',
      createdAt: 3000,
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const currentScene: Scene = {
      id: 'scene-assets',
      name: 'アセットテストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let lastUpdatedScene: Scene | null = null;

    // 1. Mount as non-admin (!isAdmin) to verify room scope is disabled
    const instanceNonAdmin = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        characters: [],
        isAdmin: false,
        onUpdateScene: (sc) => {
          lastUpdatedScene = sc;
        },
      },
    });

    // Quick-select via Carousel
    const carouselItem = target.querySelector<HTMLButtonElement>(
      `[data-testid="bg-carousel-item-${personalImg.id}"]`,
    );
    expect(carouselItem).toBeTruthy();
    carouselItem?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(lastUpdatedScene?.juices.bgImage).toBe(personalImg.dataUrl);

    // Quick-select NONE via Carousel
    target.querySelector<HTMLButtonElement>('[data-testid="bg-carousel-item-none"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(lastUpdatedScene?.juices.bgImage).toBeUndefined();

    // Open AssetPickerModal from bg-image-card / open-bg-picker-btn
    target.querySelector<HTMLButtonElement>('[data-testid="open-bg-picker-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    const modal = target.querySelector('[data-testid="asset-picker-modal"]');
    expect(modal).toBeTruthy();

    // Verify scope segmented button: room option is disabled when !isAdmin
    const scopeSegmented = target.querySelector('[data-testid="asset-scope-segmented"]');
    expect(scopeSegmented).toBeTruthy();
    const roomRadio = scopeSegmented?.querySelector<HTMLInputElement>('input[value="room"]');
    expect(roomRadio).toBeTruthy();
    expect(roomRadio?.disabled).toBe(true);

    // Verify grid index 0 is ALWAYS "NONE" card
    const grid = target.querySelector('[data-testid="asset-cards-grid"]');
    expect(grid).toBeTruthy();
    expect(grid?.children[0]?.getAttribute('data-testid')).toBe('asset-card-none');
    expect(grid?.children[0]?.textContent).toContain('NONE');

    // Select personal image card
    target.querySelector<HTMLElement>(`[data-testid="asset-card-${personalImg.id}"]`)?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(lastUpdatedScene?.juices.bgImage).toBe(personalImg.dataUrl);
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeNull();

    // Reopen and select NONE card at index 0 -> clears bgImage to undefined
    target.querySelector<HTMLElement>('[data-testid="bg-image-card"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLElement>('[data-testid="asset-card-none"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(lastUpdatedScene?.juices.bgImage).toBeUndefined();

    // Open BGM1 picker -> initialMediaType is 'audio'
    target.querySelector<HTMLButtonElement>('[data-testid="bgm-select-btn-1"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeTruthy();
    expect(
      target.querySelector(`[data-testid="asset-card-${personalAudio.id}"]`),
    ).toBeTruthy();

    // Select audio asset for BGM1
    target.querySelector<HTMLElement>(`[data-testid="asset-card-${personalAudio.id}"]`)?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(lastUpdatedScene?.juices.bgm1).toBe(personalAudio.dataUrl);

    unmount(instanceNonAdmin);

    // 2. Mount as admin (isAdmin = true) to verify room scope switching, file upload, delete, and cancel
    const instanceAdmin = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        characters: [],
        isAdmin: true,
        onUpdateScene: (sc) => {
          lastUpdatedScene = sc;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="open-bg-picker-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    const adminScopeSeg = target.querySelector('[data-testid="asset-scope-segmented"]');
    const adminRoomRadio = adminScopeSeg?.querySelector<HTMLInputElement>('input[value="room"]');
    expect(adminRoomRadio?.disabled).toBe(false);

    // Switch to 'room' scope
    adminRoomRadio?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector(`[data-testid="asset-card-${roomImg.id}"]`)).toBeTruthy();

    // Switch mediaType toggle to 'audio' and back to 'image'
    const typeSeg = target.querySelector('[data-testid="asset-type-segmented"]');
    const audioRadio = typeSeg?.querySelector<HTMLInputElement>('input[value="audio"]');
    const imageRadio = typeSeg?.querySelector<HTMLInputElement>('input[value="image"]');
    audioRadio?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector(`[data-testid="asset-card-${roomImg.id}"]`)).toBeNull();
    imageRadio?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector(`[data-testid="asset-card-${roomImg.id}"]`)).toBeTruthy();

    // Test file upload via hidden file input
    expect(target.querySelector('[data-testid="asset-upload-btn"]')).toBeTruthy();
    const fileInput = target.querySelector<HTMLInputElement>('[data-testid="asset-file-input"]');
    expect(fileInput).toBeTruthy();
    const uploadedFile = new File(['dummy-image-bytes'], 'dungeon.webp', { type: 'image/webp' });
    Object.defineProperty(fileInput!, 'files', {
      value: [uploadedFile],
      configurable: true,
    });
    fileInput!.dispatchEvent(new Event('change'));
    await new Promise((r) => setTimeout(r, 50));

    const roomAssetsAfterUpload = loadAssets().map(
      (a) => `${a.name}:${a.scope}:${a.mediaType}`,
    );
    expect(roomAssetsAfterUpload).toContain('dungeon.webp:room:image');

    // Test deleting an asset card
    const deleteBtn = target.querySelector<HTMLButtonElement>(
      `[data-testid="asset-delete-btn-${roomImg.id}"]`,
    );
    expect(deleteBtn).toBeTruthy();
    deleteBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector(`[data-testid="asset-card-${roomImg.id}"]`)).toBeNull();
    // Modal remains open after deleting an asset
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeTruthy();

    // Test cancel button
    target.querySelector<HTMLButtonElement>('[data-testid="asset-cancel-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeNull();

    unmount(instanceAdmin);
    target.remove();
  });

  // spec: alhalte-argenterie/scene-settings シーン選択ドロップダウンの外側クリック閉じ (Issue #31)
  it('closes scene split dropdown menu when clicking outside', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const currentScene: Scene = {
      id: 'scene-dropdown-test',
      name: 'メインシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        characters: [],
      },
    });

    const toggleBtn = target.querySelector<HTMLButtonElement>(
      '[data-testid="scene-split-dropdown-toggle"]',
    );
    expect(toggleBtn).toBeTruthy();
    toggleBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="scene-split-menu"]')).toBeTruthy();

    // Click outside on document.body
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="scene-split-menu"]')).toBeNull();

    unmount(instance);
    target.remove();
  });
});
