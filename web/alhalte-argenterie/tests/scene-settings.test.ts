import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  calculateDimensionsAndCrop,
  createDefaultJuices,
  getNormalizedBgmSlots,
  inheritJuices,
  moveMemberToScene,
} from '../src/utils/sceneSettings.js';
import {
  addAsset,
  clearAssets,
  loadAssets,
} from '../src/utils/assets.js';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import App from '../src/components/App.svelte';
import { jsonToTin } from '../src/utils/roomSocket.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

describe('Scene Settings & Juices', () => {
  beforeEach(() => {
    clearAssets();
  });

  // spec: Issue #249 既定の空 slot 1..3 を持たない
  it('creates no BGM rows by default and only renders a row once a slot has a track', () => {
    const juices = createDefaultJuices();
    // Issue #249: the legacy bgm1..3 mirrors stay, the empty slot list does not.
    expect(juices.bgmSlots).toEqual([]);
    expect(juices.bgm1Volume).toBe(0.8);
    expect(getNormalizedBgmSlots(juices)).toEqual([]);

    // A legacy scene without `bgmSlots` only yields the slots that are set.
    const legacy = createDefaultJuices();
    delete legacy.bgmSlots;
    legacy.bgm2 = 'b'.repeat(64);
    expect(getNormalizedBgmSlots(legacy).map((slot) => slot.slot)).toEqual([2]);

    // An explicitly empty list is authoritative even when mirrors linger.
    const explicit = createDefaultJuices();
    explicit.bgm1 = 'a'.repeat(64);
    expect(getNormalizedBgmSlots(explicit)).toEqual([]);
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
      juices: {
        ...createDefaultJuices(),
        bgmSlots: [
          {
            slot: 1,
            tinId: 'a'.repeat(64),
            mimeType: 'audio/ogg',
            name: 'theme.ogg',
            volume: 0.8,
            isPlaying: false,
          },
        ],
      },
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
        role: 'admin',
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

    // Check scene name input (dropdown toggle removed per Issue #107)
    expect(target.querySelector('[data-testid="scene-name-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="scene-split-dropdown-toggle"]')).toBeNull();

    // Check Juices headings and controls
    expect(target.querySelector('[data-testid="heading-juices"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-image-carousel"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-image-card"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-description-text"]')?.textContent).toContain(
      '画像は一方を指定すればもう一方を自動で計算します',
    );
    expect(target.querySelector('[data-testid="bg-width-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bg-height-input"]')).toBeTruthy();

    // Check the BGM row of the slot with a track uses noph-ui IconButton
    // (.np-icon-button) and Slider (.np-slider) (Issue #53). Issue #249: a slot
    // without a track has no row, so slots 2 / 3 are absent.
    const select1 = target.querySelector('[data-testid="bgm-select-btn-1"]');
    expect(select1?.classList.contains('np-icon-button')).toBe(true);
    expect(target.querySelector('[data-testid="bgm-select-btn-2"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-select-btn-3"]')).toBeNull();
    expect(target.querySelector('[data-testid="add-bgm-slot-btn"]')).toBeTruthy();

    const slider1 = target.querySelector('[data-testid="bgm-slider-1"]');
    expect(slider1?.closest('.np-slider')).toBeTruthy();

    // Check Management heading, scene name field above create scene, action buttons, and embedded move members section (Issue #92, Issue #123)
    const headingManagement = target.querySelector('[data-testid="heading-management"]');
    expect(headingManagement).toBeTruthy();
    const managementButtons = target.querySelector('[data-testid="management-buttons"]');
    expect(managementButtons).toBeTruthy();
    const sceneNameContainer = target.querySelector('[data-testid="scene-name-container"]');
    expect(sceneNameContainer).toBeTruthy();
    expect(managementButtons?.contains(sceneNameContainer!)).toBe(true);

    const colCreate = target.querySelector<HTMLElement>('[data-testid="action-col-create"]');
    const colDeleteLogs = target.querySelector<HTMLElement>('[data-testid="action-col-delete-logs"]');
    const colDeleteScene = target.querySelector<HTMLElement>('[data-testid="action-col-delete-scene"]');
    expect(colCreate?.style.flex).toContain('5');
    expect(colDeleteLogs?.style.flex).toContain('3.2');
    expect(colDeleteScene?.style.flex).toContain('2.2');

    // Issue #228: the member move UI lives in the character list now.
    expect(target.querySelector('[data-testid="move-members-section"]')).toBeNull();

    const createBtn = target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]');
    expect(
      Boolean(headingManagement?.compareDocumentPosition(sceneNameContainer!) & Node.DOCUMENT_POSITION_FOLLOWING),
    ).toBe(true);
    expect(
      Boolean(sceneNameContainer?.compareDocumentPosition(createBtn!) & Node.DOCUMENT_POSITION_FOLLOWING),
    ).toBe(true);
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
      // Issue #109 / #142: BGM state stores the cellar tin id.
      tinId: 'd'.repeat(64),
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
        role: 'participant',
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

    // "スロット追加" opens the audio picker for the next slot (Issue #249):
    // there is no row to click because no slot has a track yet.
    expect(target.querySelector('[data-testid="bgm-select-btn-1"]')).toBeNull();
    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeTruthy();
    expect(
      target.querySelector(`[data-testid="asset-card-${personalAudio.id}"]`),
    ).toBeTruthy();

    // Select an audio asset for the new slot 1 -> the tin id (not the data URL)
    // is stored and the row appears with the ack.
    target.querySelector<HTMLElement>(`[data-testid="asset-card-${personalAudio.id}"]`)?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(lastUpdatedScene?.juices.bgm1).toBe(personalAudio.tinId);
    expect(lastUpdatedScene?.juices.bgm1).not.toBe(personalAudio.dataUrl);
    expect(lastUpdatedScene?.juices.bgmSlots?.map((slot) => slot.slot)).toEqual([1]);
    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();

    unmount(instanceNonAdmin);

    // 2. Mount as admin (isAdmin = true) to verify room scope switching, file upload, delete, and cancel
    const instanceAdmin = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        characters: [],
        role: 'admin',
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

  // spec: alhalte-argenterie/scene-settings シーン管理固有のシーン選択UI削除 (Issue #107)
  it('does not render scene split dropdown menu or toggle in SceneSettingsTab (delegated to CommonHeader)', () => {
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

    expect(target.querySelector('[data-testid="scene-split-dropdown-toggle"]')).toBeNull();
    expect(target.querySelector('[data-testid="scene-split-menu"]')).toBeNull();
    // Scene name input is present as a standalone editing field
    expect(target.querySelector('[data-testid="scene-name-input"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });


  // spec: alhalte-argenterie/scene-settings シーン名入力欄を最上部固定からスクロール内「管理」セクション・「シーンを新規作成」直上へ移動 (Issue #123)
  it('places scene name input inside management section above create scene button instead of fixed top header', () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const currentScene: Scene = {
      id: 'scene-test-123',
      name: '作戦本部',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let updatedScene: Scene | null = null;
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        role: 'admin',
        allScenes: [currentScene],
        characters: [],
        onUpdateScene: (sc) => {
          updatedScene = sc;
        },
      },
    });

    const rootContainer = target.querySelector<HTMLElement>('[data-testid="scene-settings-tab"]');
    expect(rootContainer).toBeTruthy();

    const scrollContent = target.querySelector<HTMLElement>('.settings-scroll-content');
    expect(scrollContent).toBeTruthy();

    const sceneNameContainer = target.querySelector<HTMLElement>('[data-testid="scene-name-container"]');
    const sceneNameInput = target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]');
    const headingManagement = target.querySelector<HTMLElement>('[data-testid="heading-management"]');
    const managementButtons = target.querySelector<HTMLElement>('[data-testid="management-buttons"]');
    const sceneActionsRow = target.querySelector<HTMLElement>('[data-testid="scene-actions-row"]');
    const createSceneBtn = target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]');

    // 1. Scene name input is NOT at the top level of rootContainer
    expect(rootContainer?.firstElementChild).toBe(scrollContent);
    expect(rootContainer?.firstElementChild?.classList.contains('scene-name-container')).toBe(false);

    // 2. Scene name input and its container are inside scrollContent and managementButtons
    expect(scrollContent?.contains(sceneNameContainer!)).toBe(true);
    expect(scrollContent?.contains(sceneNameInput!)).toBe(true);
    expect(managementButtons?.contains(sceneNameContainer!)).toBe(true);

    // 3. Document order: heading-management -> scene-name-container/input -> scene-actions-row / open-create-scene-btn
    expect(
      Boolean(headingManagement?.compareDocumentPosition(sceneNameContainer!) & Node.DOCUMENT_POSITION_FOLLOWING),
    ).toBe(true);
    expect(
      Boolean(sceneNameContainer?.compareDocumentPosition(sceneActionsRow!) & Node.DOCUMENT_POSITION_FOLLOWING),
    ).toBe(true);
    expect(
      Boolean(sceneNameContainer?.compareDocumentPosition(createSceneBtn!) & Node.DOCUMENT_POSITION_FOLLOWING),
    ).toBe(true);

    // 4. Editing scene name updates currentScene
    if (sceneNameInput) {
      sceneNameInput.value = '司令室';
      sceneNameInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    expect(updatedScene?.name).toBe('司令室');

    unmount(instance);
    target.remove();
  });
});

describe('App — BGM StateUpdated / StateDeleted handling (Issue #109 / #142)', () => {
  // spec: argenterie/bgm-controls std::bgm / std::bgm2 / std::bgm3 / bgm::<slot>::volume の state を scene.juices と bgmSlots へ反映する
  it('applies legacy std::bgm, std::bgm2/3, bgm::<slot>::volume and StateDeleted to the scene BGM slots', async () => {
    const tinA = 'a'.repeat(64);
    const tinB = 'b'.repeat(64);
    const tinC = 'c'.repeat(64);
    const dummyRoom: VisitedRoom = {
      id: 'room-bgm-state',
      name: 'BGM状態テスト部屋',
      url: 'https://alhalte.example/rooms/room-bgm-state#abcdef',
      lastVisited: Date.now(),
    };

    const socket = new FakeRoomSocket();
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [
          {
            id: '/',
            name: 'メインシーン',
            juices: createDefaultJuices(),
            viewTransform: { x: 0, y: 0, scale: 1 },
            members: [],
          },
        ],
        roomSocket: asRoomSocket(socket),
      },
    });

    const settle = (ms = 20) => new Promise((resolve) => setTimeout(resolve, ms));
    await settle();

    // Open the scene settings tab where the BGM rows live.
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
    await settle();

    const play1 = () =>
      target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-1"]');
    const play2 = () =>
      target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-2"]');
    const play3 = () =>
      target.querySelector<HTMLButtonElement>('[data-testid="bgm-play-btn-3"]');

    // Issue #249: a scene without a track has no BGM row at all.
    expect(play1()).toBeNull();

    // Legacy `std::bgm` (slot 1) with a Tin-encoded 64-hex tin id creates the
    // row; a stopped track is still a row.
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'std::bgm', value: jsonToTin(tinA) },
    });
    await settle();
    expect(play1()?.getAttribute('aria-label')).toBe('BGM再生');
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::1::playing', value: jsonToTin(true) },
    });
    await settle();
    expect(play1()?.getAttribute('aria-label')).toBe('BGM停止');

    // New slot keys `std::bgm2` / `std::bgm3` with the current track shape.
    socket.emitFrame({
      StateUpdated: {
        scene_id: '/',
        key: 'std::bgm2',
        value: jsonToTin({ tinId: tinB, mimeType: 'audio/ogg', name: 'b' }),
      },
    });
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::2::playing', value: jsonToTin(true) },
    });
    await settle();
    expect(play2()?.getAttribute('aria-label')).toBe('BGM停止');

    // A plain legacy string value is still accepted.
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'std::bgm3', value: tinC },
    });
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::3::playing', value: jsonToTin(true) },
    });
    await settle();
    expect(play3()?.getAttribute('aria-label')).toBe('BGM停止');

    // Volume keys update the slot volume shown next to the slider.
    socket.emitFrame({
      StateUpdated: { scene_id: '/', key: 'bgm::2::volume', value: jsonToTin(0.42) },
    });
    await settle();
    expect(
      target.querySelector('[data-testid="bgm-vol-label-2"]')?.textContent?.trim(),
    ).toBe('42%');

    // Deleting the track removes the row (Issue #249): the slot has no track,
    // so there is nothing to stop or resume.
    socket.emitFrame({ StateDeleted: { scene_id: '/', key: 'std::bgm' } });
    await settle();
    expect(play1()).toBeNull();
    // The other slots keep their own row.
    expect(play2()?.getAttribute('aria-label')).toBe('BGM停止');
    expect(play3()?.getAttribute('aria-label')).toBe('BGM停止');

    unmount(instance);
    target.remove();
  });

  // spec: Issue #249 track の無い slot は行を作らない
  it('creates no row for a volume-only snapshot and keeps transport values around a track key', async () => {
    const tinA = 'a'.repeat(64);
    const socket = new FakeRoomSocket();
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(App, {
      target,
      props: {
        initialRoom: {
          id: 'room-bgm-state-249',
          name: 'BGM状態テスト部屋249',
          url: 'https://alhalte.example/rooms/room-bgm-state-249#abcdef',
          lastVisited: Date.now(),
        },
        initialScenes: [
          {
            id: '/',
            name: 'メインシーン',
            juices: createDefaultJuices(),
            viewTransform: { x: 0, y: 0, scale: 1 },
            members: [],
          },
        ],
        roomSocket: asRoomSocket(socket),
      },
    });
    const settle = (ms = 20) => new Promise((resolve) => setTimeout(resolve, ms));
    await settle();
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
    await settle();

    // Volume / playing keys alone never create a row.
    socket.emitFrame({
      StateSnapshot: {
        scene_id: '/',
        states: {
          'bgm::1::volume': jsonToTin(0.3),
          'bgm::1::playing': jsonToTin(true),
        },
      },
    });
    await settle();
    expect(target.querySelector('[data-testid="bgm-slot-block-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-slot-block-2"]')).toBeNull();

    // The snapshot map is lexicographic (`bgm::1::*` before `std::bgm`): the
    // transport values must still land on the row the track key creates.
    socket.emitFrame({
      StateSnapshot: {
        scene_id: '/',
        states: {
          'bgm::1::playing': jsonToTin(true),
          'bgm::1::volume': jsonToTin(0.3),
          'std::bgm': jsonToTin({ tinId: tinA, mimeType: 'audio/ogg', name: 'a.ogg' }),
        },
      },
    });
    await settle();
    expect(target.querySelector('[data-testid="bgm-slot-block-1"]')).toBeTruthy();
    expect(
      target.querySelector('[data-testid="bgm-vol-label-1"]')?.textContent?.trim(),
    ).toBe('30%');
    expect(
      target.querySelector('[data-testid="bgm-play-btn-1"]')?.getAttribute('aria-label'),
    ).toBe('BGM停止');

    unmount(instance);
    target.remove();
  });
});
