/**
 * Unit tests for Image Picker Separation (Generic Image Picker vs. Icon Image Picker).
 *
 * References:
 * - GitHub Issue #52: 汎用画像ピッカーの分離とアイコン拡大表示の実装
 * - M3 Expressive Carousel & Motion
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import GenericImagePicker from '../src/components/Control/GenericImagePicker.svelte';
import IconImagePicker from '../src/components/Control/IconImagePicker.svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import CharacterManagementTab from '../src/components/Control/tabs/CharacterManagementTab.svelte';
import { addAsset, clearAssets } from '../src/utils/assets.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { Character, Scene } from '../src/types/index.js';

describe('Image Picker Separation (Issue #52)', () => {
  beforeEach(() => {
    clearAssets();
    document.body.innerHTML = '';
  });

  // spec: Issue #52 汎用画像選択UI (M3 Carousel multi-browse, クリック選択, + その他の画像 でアセットピッカー表示)
  it('renders GenericImagePicker with M3 multi-browse Carousel, allows quick-select, and opens AssetPickerModal via "+ その他の画像"', async () => {
    const testAsset = addAsset({
      id: 'img-generic-1',
      name: 'landscape.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,bGFuZHNjYXBl',
      mimeType: 'image/webp',
      createdAt: 1000,
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    let selectedDataUrl: string | undefined = undefined;
    let pickerOpened = false;

    const instance = mount(GenericImagePicker, {
      target,
      props: {
        label: '背景画像アセット',
        aspectRatio: 16 / 9,
        showNone: true,
        isAdmin: false,
        onSelect: (url) => {
          selectedDataUrl = url;
        },
        onRequestAssetPicker: () => {
          pickerOpened = true;
        },
      },
    });

    // 1. Verify M3 Carousel multi-browse is rendered
    const carouselEl = target.querySelector('.np-carousel');
    expect(carouselEl).toBeTruthy();
    expect(carouselEl?.classList.contains('np-carousel-multi-browse')).toBe(true);

    // 2. Verify CarouselItem type="button" items exist
    const noneItem = target.querySelector<HTMLButtonElement>('[data-testid="carousel-item-none"]');
    expect(noneItem).toBeTruthy();
    expect(noneItem?.tagName.toLowerCase()).toBe('button');

    const assetItem = target.querySelector<HTMLButtonElement>(`[data-testid="carousel-item-${testAsset.id}"]`);
    expect(assetItem).toBeTruthy();
    expect(assetItem?.tagName.toLowerCase()).toBe('button');

    // Quick-select asset via CarouselItem
    assetItem?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(selectedDataUrl).toBe(testAsset.dataUrl);

    // Quick-select NONE via CarouselItem
    noneItem?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(selectedDataUrl).toBeUndefined();

    // 3. Verify "+ その他の画像" text/button is located in footer row with primary color
    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="more-images-btn"]');
    expect(moreBtn).toBeTruthy();
    expect(moreBtn?.textContent?.trim()).toContain('+ その他の画像');

    // Click "+ その他の画像" -> calls onRequestAssetPicker
    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(pickerOpened).toBe(true);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #52 汎用画像選択UI (SceneSettingsTab での統合動作)
  it('integrates GenericImagePicker in SceneSettingsTab with multi-browse Carousel and "+ その他の画像" modal trigger', async () => {
    const bgAsset = addAsset({
      id: 'bg-asset-1',
      name: 'dungeon.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,ZHVuZ2Vvbg==',
      mimeType: 'image/webp',
      createdAt: 1000,
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const currentScene: Scene = {
      id: 'scene-generic-test',
      name: 'テストシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let updatedScene: Scene | null = null;

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene,
        allScenes: [currentScene],
        characters: [],
        isAdmin: false,
        onUpdateScene: (sc) => {
          updatedScene = sc;
        },
      },
    });

    // Carousel is present and multi-browse
    const carouselEl = target.querySelector('.np-carousel');
    expect(carouselEl?.classList.contains('np-carousel-multi-browse')).toBe(true);

    // Click item in Carousel to set background image
    const item = target.querySelector<HTMLButtonElement>(`[data-testid="bg-carousel-item-${bgAsset.id}"]`);
    expect(item).toBeTruthy();
    item?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(updatedScene?.juices.bgImage).toBe(bgAsset.dataUrl);

    // "+ その他の画像" button opens AssetPickerModal
    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="more-images-btn"]');
    expect(moreBtn).toBeTruthy();
    expect(moreBtn?.textContent?.trim()).toContain('+ その他の画像');
    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const modal = target.querySelector('[data-testid="asset-picker-modal"]');
    expect(modal).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: Issue #52 アイコン画像選択UI (IconImagePicker ホバーズーム & アセットピッカー起動)
  it('applies smooth hover zoom style to IconImagePicker and opens AssetPickerModal on click', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let pickedUrl: string | undefined = undefined;

    const instance = mount(IconImagePicker, {
      target,
      props: {
        src: 'data:image/webp;base64,YXZhdGFy',
        alt: 'キャラクターアイコン',
        size: 64,
        dataTestId: 'test-icon-picker',
        onSelect: (url) => {
          pickedUrl = url;
        },
      },
    });

    const pickerBtn = target.querySelector<HTMLButtonElement>('[data-testid="test-icon-picker"]');
    expect(pickerBtn).toBeTruthy();

    const img = target.querySelector<HTMLImageElement>('[data-testid="test-icon-picker-img"]');
    expect(img).toBeTruthy();

    // 1. Verify default transition and scale
    expect(img?.style.transition).toContain('transform');
    expect(img?.style.transition).toContain('200ms');
    expect(img?.style.transform).toBe('scale(1)');

    // 2. Mouse enter triggers hover zoom (scale > 1)
    pickerBtn?.dispatchEvent(new MouseEvent('mouseenter'));
    await new Promise((r) => setTimeout(r, 20));
    expect(img?.style.transform).toBe('scale(1.1)');
    expect(img?.classList.contains('hovered')).toBe(true);

    // 3. Mouse leave restores scale to 1
    pickerBtn?.dispatchEvent(new MouseEvent('mouseleave'));
    await new Promise((r) => setTimeout(r, 20));
    expect(img?.style.transform).toBe('scale(1)');
    expect(img?.classList.contains('hovered')).toBe(false);

    // 4. Click opens AssetPickerModal
    pickerBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const modal = target.querySelector('[data-testid="asset-picker-modal"]');
    expect(modal).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: Issue #52 アイコン画像選択UI (CharacterManagementTab での統合動作)
  it('integrates IconImagePicker with hover zoom in CharacterManagementTab edit panel and character list', async () => {
    const avatarAsset = addAsset({
      id: 'char-icon-1',
      name: 'avatar.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,aWNvbg==',
      mimeType: 'image/webp',
      createdAt: 1000,
    });

    const testChar: Character = {
      id: 'char-mage',
      name: '魔法使い',
      initiative: 5,
      description: '後衛',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
    };

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(CharacterManagementTab, {
      target,
      props: {
        characters: [testChar],
        isAdmin: false,
      },
    });

    // 1. Character list avatar has hover zoom styling
    const listAvatarBtn = target.querySelector<HTMLButtonElement>('[data-testid="edit-char-btn-char-mage"]');
    expect(listAvatarBtn).toBeTruthy();
    const listImg = listAvatarBtn?.querySelector<HTMLImageElement>('.char-list-avatar');
    expect(listImg).toBeTruthy();

    // Open character edit panel
    listAvatarBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // 2. Edit panel contains IconImagePicker for character avatar
    const editPicker = target.querySelector<HTMLButtonElement>('[data-testid="edit-avatar-picker"]');
    expect(editPicker).toBeTruthy();
    const editImg = target.querySelector<HTMLImageElement>('[data-testid="edit-avatar-img"]');
    expect(editImg).toBeTruthy();

    // Verify hover zoom interaction
    expect(editImg?.style.transform).toBe('scale(1)');
    editPicker?.dispatchEvent(new MouseEvent('mouseenter'));
    await new Promise((r) => setTimeout(r, 20));
    expect(editImg?.style.transform).toBe('scale(1.1)');

    editPicker?.dispatchEvent(new MouseEvent('mouseleave'));
    await new Promise((r) => setTimeout(r, 20));
    expect(editImg?.style.transform).toBe('scale(1)');

    // 3. Click opens AssetPickerModal
    editPicker?.click();
    await new Promise((r) => setTimeout(r, 20));

    const modal = target.querySelector('[data-testid="asset-picker-modal"]');
    expect(modal).toBeTruthy();

    // Select asset card in modal
    target.querySelector<HTMLElement>(`[data-testid="asset-card-${avatarAsset.id}"]`)?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeNull();
    expect(editImg?.getAttribute('src')).toBe(avatarAsset.dataUrl);

    unmount(instance);
    target.remove();
  });
});
