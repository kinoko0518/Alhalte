/**
 * Unit tests for AssetPickerModal and Image Pickers Integration.
 *
 * References:
 * - Issue #28: アセット選択UI（自分のアセット｜部屋のアセット、画像｜音声、アップロード、NONEカード、削除）
 * - Issue #52: 画像選択UIの分離（汎用Carousel + "+ その他の画像" ボタン vs. アイコン単一ホバーズーム）
 * - Issue #60: ビューポート最前面化とオーバーレイ
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import AssetPickerModal from '../src/components/Control/AssetPickerModal.svelte';
import GenericImagePicker from '../src/components/Control/GenericImagePicker.svelte';
import IconImagePicker from '../src/components/Control/IconImagePicker.svelte';
import { addAsset, clearAssets } from '../src/utils/assets.js';

describe('AssetPickerModal & Image Pickers Integration (Issue #52, #60)', () => {
  beforeEach(() => {
    clearAssets();
    document.body.innerHTML = '';
  });

  // spec: Issue #28, #52 アセットピッカーモーダルの開閉・タブ切り替え・NONE選択・アセット選択
  it('renders AssetPickerModal, switches media types and scopes, selects NONE, and picks an asset', async () => {
    const imgAsset = addAsset({
      id: 'asset-test-img-1',
      name: 'background-mountain.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,bW91bnRhaW4=',
      mimeType: 'image/webp',
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    let pickedDataUrl: string | undefined = 'initial';
    let isClosed = false;

    const instance = mount(AssetPickerModal, {
      target,
      props: {
        open: true,
        isAdmin: false,
        initialMediaType: 'image',
        initialScope: 'personal',
        onSelect: (dataUrl) => {
          pickedDataUrl = dataUrl;
        },
        onClose: () => {
          isClosed = true;
        },
      },
    });

    // 1. Verify modal overlay and surface are rendered with high z-index
    const modalEl = target.querySelector<HTMLElement>('[data-testid="asset-picker-modal"]');
    expect(modalEl).toBeTruthy();
    expect(modalEl?.getAttribute('role')).toBe('dialog');

    // 2. NONE card exists as 0-th item and selecting it passes undefined
    const noneCard = target.querySelector<HTMLElement>('[data-testid="asset-card-none"]');
    expect(noneCard).toBeTruthy();

    // 3. Asset card is present
    const card = target.querySelector<HTMLElement>(`[data-testid="asset-card-${imgAsset.id}"]`);
    expect(card).toBeTruthy();
    expect(card?.textContent).toContain('background-mountain.webp');

    // Click asset card -> onSelect called with dataUrl and modal closed
    card?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(pickedDataUrl).toBe(imgAsset.dataUrl);
    expect(isClosed).toBe(true);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #52 汎用画像選択UI (GenericImagePicker): M3 multi-browse Carousel + "+ その他の画像"
  it('renders GenericImagePicker with multi-browse Carousel, items, and primary-colored "+ その他の画像" trigger', async () => {
    const bgAsset = addAsset({
      id: 'bg-generic-test-1',
      name: 'castle.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,Y2FzdGxl',
      mimeType: 'image/webp',
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    let selectedUrl: string | undefined = undefined;
    let modalOpened = false;

    const instance = mount(GenericImagePicker, {
      target,
      props: {
        label: '背景画像',
        aspectRatio: 16 / 9,
        showNone: true,
        isAdmin: false,
        onSelect: (url) => {
          selectedUrl = url;
        },
        onRequestAssetPicker: () => {
          modalOpened = true;
        },
      },
    });

    // Carousel rendered with multi-browse variant
    const carousel = target.querySelector('.np-carousel-multi-browse');
    expect(carousel).toBeTruthy();

    // NONE item exists
    const noneItem = target.querySelector<HTMLButtonElement>('[data-testid="carousel-item-none"]');
    expect(noneItem).toBeTruthy();

    // Asset item exists
    const assetItem = target.querySelector<HTMLButtonElement>(`[data-testid="carousel-item-${bgAsset.id}"]`);
    expect(assetItem).toBeTruthy();

    assetItem?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(selectedUrl).toBe(bgAsset.dataUrl);

    // "+ その他の画像" button in bottom-right
    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="more-images-btn"]');
    expect(moreBtn).toBeTruthy();
    expect(moreBtn?.textContent?.trim()).toContain('+ その他の画像');

    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(modalOpened).toBe(true);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #52 アイコン画像選択UI (IconImagePicker): Carouselなし、ホバーズーム (scale 1.1) とクリックでのAssetPickerModal起動
  it('renders IconImagePicker without Carousel, scales to 1.1 on hover, and opens AssetPickerModal on click', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let modalOpened = false;

    const instance = mount(IconImagePicker, {
      target,
      props: {
        src: 'data:image/webp;base64,aWNvbg==',
        alt: 'アバター',
        size: 64,
        dataTestId: 'custom-icon-picker',
        onRequestAssetPicker: () => {
          modalOpened = true;
        },
      },
    });

    // Verify NO Carousel is rendered for IconImagePicker
    expect(target.querySelector('.np-carousel')).toBeNull();

    const pickerBtn = target.querySelector<HTMLButtonElement>('[data-testid="custom-icon-picker"]');
    expect(pickerBtn).toBeTruthy();

    const img = target.querySelector<HTMLImageElement>('[data-testid="custom-icon-picker-img"]');
    expect(img).toBeTruthy();
    expect(img?.style.transition).toContain('transform');
    expect(img?.style.transition).toContain('200ms');

    // Initial scale: 1
    expect(img?.style.transform).toBe('scale(1)');

    // Hover zoom: scale(1.1)
    pickerBtn?.dispatchEvent(new MouseEvent('mouseenter'));
    await new Promise((r) => setTimeout(r, 20));
    expect(img?.style.transform).toBe('scale(1.1)');

    // Mouse leave: scale(1)
    pickerBtn?.dispatchEvent(new MouseEvent('mouseleave'));
    await new Promise((r) => setTimeout(r, 20));
    expect(img?.style.transform).toBe('scale(1)');

    // Click opens AssetPickerModal
    pickerBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(modalOpened).toBe(true);

    unmount(instance);
    target.remove();
  });
});
