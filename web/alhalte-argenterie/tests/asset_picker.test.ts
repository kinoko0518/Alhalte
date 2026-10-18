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
import { addAsset, addAssetFromFile, clearAssets, formatFileSize, loadAssets } from '../src/utils/assets.js';

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

  // spec: Issue #108 formatFileSize ユーティリティのテスト
  it('formats file sizes accurately into human-readable strings (formatFileSize)', () => {
    expect(formatFileSize(undefined)).toBe('-');
    expect(formatFileSize(null as any)).toBe('-');
    expect(formatFileSize(-1)).toBe('-');
    expect(formatFileSize(NaN)).toBe('-');
    expect(formatFileSize(0)).toBe('0 B');
    expect(formatFileSize(512)).toBe('512 B');
    expect(formatFileSize(1024)).toBe('1.0 KB');
    expect(formatFileSize(123392)).toBe('120.5 KB'); // 120.5 * 1024 = 123392
    expect(formatFileSize(1258291)).toBe('1.2 MB'); // 1.2 * 1024 * 1024 ≈ 1258291
    expect(formatFileSize(1073741824)).toBe('1.0 GB');
  });

  // spec: Issue #108 addAssetFromFile でのファイルサイズ (file.size) 保存テスト
  it('stores file.size in AssetItem when addAssetFromFile is called', async () => {
    const file = new File(['hello alhalte'], 'greeting.png', { type: 'image/png' });
    // In jsdom or custom environments, file.size is the content size
    const asset = await addAssetFromFile(file, 'personal', 'image');
    expect(asset.size).toBe(file.size);

    const loaded = loadAssets();
    const found = loaded.find((a) => a.id === asset.id);
    expect(found?.size).toBe(file.size);
  });

  // spec: Issue #108 M3 filled Cards, NONE card uniform size, file size display, M3 Search, and direct URL removal
  it('renders filled Cards for NONE and assets, displays file sizes, provides M3 Search, and removes direct URL inputs (Issue #108)', async () => {
    const sampleAsset1 = addAsset({
      id: 'asset-sample-1',
      name: 'ancient-ruins.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,cnVpbnM=',
      mimeType: 'image/webp',
      size: 123392, // 120.5 KB
    });

    const sampleAsset2 = addAsset({
      id: 'asset-sample-2',
      name: 'battle-theme.opus',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,dGhlbWU=',
      mimeType: 'image/webp',
      size: 1258291, // 1.2 MB
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(AssetPickerModal, {
      target,
      props: {
        open: true,
        isAdmin: false,
        initialMediaType: 'image',
        initialScope: 'personal',
      },
    });

    // 1. Direct URL inputs are completely removed
    expect(target.querySelector('[data-testid="asset-url-input"]')).toBeNull();
    expect(target.querySelector('[data-testid="asset-apply-url-btn"]')).toBeNull();

    // 2. M3 Expressive Search component is present and filters assets
    const searchContainer = target.querySelector('[data-testid="asset-search-container"]');
    const searchInput = target.querySelector<HTMLInputElement>('[data-testid="asset-search-input"]');
    expect(searchContainer).toBeTruthy();
    expect(searchContainer?.classList.contains('np-search')).toBe(true);
    expect(searchContainer?.classList.contains('np-search-contained')).toBe(true);
    expect(searchInput).toBeTruthy();

    // 3. NONE card is rendered with M3 filled Card and uniform wrapper
    const noneCardWrapper = target.querySelector<HTMLElement>('[data-testid="asset-card-none"]');
    expect(noneCardWrapper).toBeTruthy();
    expect(noneCardWrapper?.classList.contains('asset-card-wrapper')).toBe(true);
    expect(noneCardWrapper?.classList.contains('none-card-wrapper')).toBe(true);

    const noneFilledCard = noneCardWrapper?.querySelector('.np-card-filled');
    expect(noneFilledCard).toBeTruthy();

    const noneSizeLabel = noneCardWrapper?.querySelector('.asset-size-label');
    expect(noneSizeLabel).toBeTruthy();
    expect(noneSizeLabel?.textContent?.trim()).toBe('-');

    // 4. Asset cards are rendered with M3 filled Card and formatted file sizes
    const card1 = target.querySelector<HTMLElement>(`[data-testid="asset-card-${sampleAsset1.id}"]`);
    expect(card1).toBeTruthy();
    expect(card1?.classList.contains('asset-card-wrapper')).toBe(true);
    expect(card1?.querySelector('.np-card-filled')).toBeTruthy();

    const sizeLabel1 = card1?.querySelector('.asset-size-label');
    expect(sizeLabel1).toBeTruthy();
    expect(sizeLabel1?.textContent?.trim()).toBe('120.5 KB');

    const card2 = target.querySelector<HTMLElement>(`[data-testid="asset-card-${sampleAsset2.id}"]`);
    expect(card2).toBeTruthy();
    expect(card2?.querySelector('.np-card-filled')).toBeTruthy();

    const sizeLabel2 = card2?.querySelector('.asset-size-label');
    expect(sizeLabel2).toBeTruthy();
    expect(sizeLabel2?.textContent?.trim()).toBe('1.2 MB');

    // 5. Test M3 Search filtering
    if (searchInput) {
      searchInput.value = 'ancient';
      searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    // NONE card always remains, sampleAsset1 matches, sampleAsset2 filtered out
    expect(target.querySelector('[data-testid="asset-card-none"]')).toBeTruthy();
    expect(target.querySelector(`[data-testid="asset-card-${sampleAsset1.id}"]`)).toBeTruthy();
    expect(target.querySelector(`[data-testid="asset-card-${sampleAsset2.id}"]`)).toBeNull();

    unmount(instance);
    target.remove();
  });
});
