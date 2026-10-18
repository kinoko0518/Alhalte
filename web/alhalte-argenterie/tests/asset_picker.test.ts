/**
 * Unit tests for AssetPickerModal and Image Pickers Integration.
 *
 * References:
 * - Issue #28: アセット選択UI（自分のアセット｜部屋のアセット、画像｜音声、アップロード、NONEカード、削除）
 * - Issue #52: 画像選択UIの分離（汎用Carousel + "+ その他の画像" ボタン vs. アイコン単一ホバーズーム）
 * - Issue #60: ビューポート最前面化とオーバーレイ
 * - Issue #109 / #141: カード内アップロード進捗・失敗・再試行、開くたびの同期
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

const cellar = vi.hoisted(() => {
  class MockCellarTransferError extends Error {
    code: string;
    constructor(code: string, message: string) {
      super(message);
      this.name = 'CellarTransferError';
      this.code = code;
    }
  }
  return {
    DEFAULT_OPERATION_TIMEOUT_MS: 8000,
    CELLAR_PROXY_BASE_PATH: '/api/cellar',
    encodeRoomTin: vi.fn(),
    uploadTin: vi.fn(),
    downloadTin: vi.fn(),
    getRoomKeyFromLocation: vi.fn(() => null),
    isCellarTransferError: (error: unknown) => error instanceof MockCellarTransferError,
    CellarTransferError: MockCellarTransferError,
  };
});

vi.mock('../src/utils/cellarTransfer.js', () => cellar);

import { mount, unmount } from 'svelte';
import AssetPickerModal from '../src/components/Control/AssetPickerModal.svelte';
import GenericImagePicker from '../src/components/Control/GenericImagePicker.svelte';
import IconImagePicker from '../src/components/Control/IconImagePicker.svelte';
import {
  ASSET_STATE_KEY_PREFIX,
  addAsset,
  addAssetFromFile,
  clearAssets,
  formatFileSize,
  loadAssets,
  type RoomAssetMetadata,
} from '../src/utils/assets.js';
import { FakeRoomSocket, asRoomSocket } from './helpers/fakeRoomSocket.js';

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

// =========================================================================
// Issue #109 / #141: cellar transfer, per-card progress, sync-on-open
// =========================================================================

const ROOM_KEY = new Uint8Array(32);
const TIN_ID = 'f0'.repeat(32);
const WIRE_BYTES = new Uint8Array([1, 2, 3]);

/** Builds a small image File. */
function imageFile(name: string): File {
  return new File([new Uint8Array([1, 2, 3])], name, { type: 'image/png' });
}

/** Builds a `CellarTransferError`-shaped failure (the mock recognises it). */
function cellarError(code: string, message: string): Error {
  return new cellar.CellarTransferError(code, message);
}

/** Builds the metadata another client publishes under `asset::<id>`. */
function roomMetadata(
  id: string,
  overrides: Partial<RoomAssetMetadata> = {},
): RoomAssetMetadata {
  return {
    id,
    name: `${id}.png`,
    mediaType: 'image',
    mimeType: 'image/png',
    size: 3,
    tinId: TIN_ID,
    createdAt: 1000,
    ...overrides,
  };
}

describe('AssetPickerModal cellar transfer (Issue #109 / #141)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    clearAssets();
    localStorage.clear();
    cellar.encodeRoomTin.mockReset();
    cellar.uploadTin.mockReset();
    cellar.downloadTin.mockReset();
    cellar.getRoomKeyFromLocation.mockReset();
    cellar.getRoomKeyFromLocation.mockReturnValue(null);
    cellar.encodeRoomTin.mockResolvedValue({ wireBytes: WIRE_BYTES, tinIdHex: TIN_ID });
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    target.remove();
  });

  /** Mounts the picker on the shared test target. */
  function mountPicker(props: Record<string, unknown>) {
    return mount(AssetPickerModal, {
      target,
      props: {
        open: true,
        isAdmin: false,
        initialMediaType: 'image',
        initialScope: 'personal',
        ...props,
      },
    });
  }

  /** Drives the hidden file input like a user picking files. */
  function pickFiles(files: File[]): void {
    const input = target.querySelector<HTMLInputElement>('[data-testid="asset-file-input"]');
    expect(input).toBeTruthy();
    Object.defineProperty(input!, 'files', { value: files, configurable: true });
    input!.dispatchEvent(new Event('change'));
  }

  it('shows a wavy determinate LinearProgress inside the card, blocks selection, and publishes asset:: metadata', async () => {
    let releaseUpload!: () => void;
    const uploadGate = new Promise<void>((resolve) => {
      releaseUpload = resolve;
    });
    cellar.uploadTin.mockImplementation(async (_bytes: Uint8Array, options: any) => {
      options.onProgress?.(0.4);
      await uploadGate;
      return { tinId: TIN_ID };
    });

    const socket = new FakeRoomSocket();
    let selected: string | undefined;
    mountPicker({
      isAdmin: true,
      initialScope: 'room',
      roomSocket: asRoomSocket(socket),
      roomKey: ROOM_KEY,
      onSelect: (dataUrl?: string) => {
        selected = dataUrl;
      },
    });
    // Let the mount effect attach the file input `change` listener.
    await new Promise((r) => setTimeout(r, 20));

    pickFiles([imageFile('progress.png')]);

    // Progress is only reported by the mocked XHR callback (no fake ticks).
    await vi.waitFor(() => {
      const pending = loadAssets().find((asset) => asset.name === 'progress.png');
      expect(pending?.uploadState).toBe('uploading');
      expect(pending?.uploadProgress).toBe(0.4);
    });
    const asset = loadAssets().find((item) => item.name === 'progress.png')!;
    const card = target.querySelector<HTMLElement>(`[data-testid="asset-card-${asset.id}"]`);
    expect(card).toBeTruthy();
    expect(card?.getAttribute('aria-busy')).toBe('true');
    expect(card?.getAttribute('aria-disabled')).toBe('true');
    expect(card?.getAttribute('tabindex')).toBe('-1');

    const progressHost = target.querySelector<HTMLElement>(
      `[data-testid="asset-upload-progress-${asset.id}"]`,
    );
    expect(progressHost).toBeTruthy();
    const bar = progressHost!.querySelector<HTMLElement>('[role="progressbar"]');
    expect(bar).toBeTruthy();
    expect(bar!.classList.contains('wavy')).toBe(true);
    expect(bar!.classList.contains('indeterminate')).toBe(false);
    expect(bar!.getAttribute('aria-valuenow')).toBe('0.4');
    expect(bar!.getAttribute('aria-valuemax')).toBe('1');

    // Selecting an uploading card is disabled and keeps the modal open.
    card!.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(selected).toBeUndefined();
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeTruthy();

    releaseUpload();
    await vi.waitFor(() => {
      expect(loadAssets().find((item) => item.id === asset.id)?.uploadState).toBe('idle');
    });
    await vi.waitFor(() => {
      expect(
        target.querySelector(`[data-testid="asset-upload-progress-${asset.id}"]`),
      ).toBeNull();
    });

    expect(socket.requestUpdateStateCalls).toHaveLength(1);
    expect(socket.requestUpdateStateCalls[0]!.sceneId).toBe('/');
    expect(socket.requestUpdateStateCalls[0]!.key).toBe(`${ASSET_STATE_KEY_PREFIX}${asset.id}`);
  });

  it('shows the failure message plus a retry action on the card and leaves other cards unaffected', async () => {
    addAsset({
      id: 'other-room-asset',
      name: 'other.png',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png',
      createdAt: 1,
    });
    cellar.uploadTin
      .mockRejectedValueOnce(cellarError('timeout', 'Cellar request exceeded 8000 ms'))
      .mockResolvedValueOnce({ tinId: TIN_ID });

    const socket = new FakeRoomSocket();
    mountPicker({
      isAdmin: true,
      initialScope: 'room',
      roomSocket: asRoomSocket(socket),
      roomKey: ROOM_KEY,
    });

    // Let the mount effect attach the file input `change` listener.
    await new Promise((r) => setTimeout(r, 20));
    pickFiles([imageFile('failing.png')]);

    await vi.waitFor(() => {
      expect(loadAssets().find((asset) => asset.name === 'failing.png')?.uploadState).toBe(
        'failed',
      );
    });
    const failed = loadAssets().find((asset) => asset.name === 'failing.png')!;
    const failedCard = target.querySelector<HTMLElement>(
      `[data-testid="asset-card-${failed.id}"]`,
    );
    expect(failedCard?.textContent).toContain('アップロードに失敗しました');
    expect(failedCard?.getAttribute('aria-busy')).not.toBe('true');
    // The unrelated card never shows the failure text.
    expect(
      target.querySelector(`[data-testid="asset-card-other-room-asset"]`)?.textContent,
    ).not.toContain('アップロードに失敗しました');
    expect(socket.requestUpdateStateCalls).toHaveLength(0);

    // Retry reuses the same card and publishes the metadata on success.
    const retry = failedCard?.querySelector<HTMLElement>(
      `[data-testid="asset-retry-btn-${failed.id}"]`,
    );
    expect(retry).toBeTruthy();
    retry!.click();

    await vi.waitFor(() => {
      const retried = loadAssets().find((asset) => asset.id === failed.id);
      expect(retried?.uploadState).toBe('idle');
      expect(retried?.tinId).toBe(TIN_ID);
    });
    await vi.waitFor(() => {
      expect(
        target.querySelector(`[data-testid="asset-upload-failed-${failed.id}"]`),
      ).toBeNull();
    });
    expect(cellar.uploadTin).toHaveBeenCalledTimes(2);
    expect(socket.requestUpdateStateCalls).toHaveLength(1);
  });

  it('syncs room assets once when the picker opens and merges published metadata', async () => {
    addAsset({
      id: 'remote-room',
      name: 'old-name.png',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png',
      createdAt: 1000,
    });
    const socket = new FakeRoomSocket();
    socket.stateSnapshot = {
      [`${ASSET_STATE_KEY_PREFIX}remote-room`]: roomMetadata('remote-room', {
        name: 'synced.png',
      }),
    };

    mountPicker({
      isAdmin: true,
      initialScope: 'room',
      roomSocket: asRoomSocket(socket),
      roomKey: ROOM_KEY,
    });

    await vi.waitFor(() => {
      expect(socket.queryStateCalls).toHaveLength(1);
    });
    expect(socket.queryStateCalls[0]).toEqual({ sceneId: '/' });
    // A cached preview is reused instead of downloading the tin again.
    expect(cellar.downloadTin).not.toHaveBeenCalled();
    await vi.waitFor(() => {
      expect(
        target.querySelector('[data-testid="asset-card-remote-room"]')?.textContent,
      ).toContain('synced.png');
    });

    // Reactive UI changes (scope / media type / search) never re-sync.
    const mediaSeg = target.querySelector('[data-testid="asset-type-segmented"]');
    mediaSeg?.querySelector<HTMLInputElement>('input[value="audio"]')?.click();
    mediaSeg?.querySelector<HTMLInputElement>('input[value="image"]')?.click();
    const searchInput = target.querySelector<HTMLInputElement>(
      '[data-testid="asset-search-input"]',
    );
    if (searchInput) {
      searchInput.value = 'sync';
      searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 30));
    expect(socket.queryStateCalls).toHaveLength(1);
  });

  it('keeps the local cache visible when the sync query fails', async () => {
    addAsset({
      id: 'cached-room-asset',
      name: 'cached.png',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png',
      createdAt: 1000,
    });
    const socket = new FakeRoomSocket();
    socket.queryState = () =>
      Promise.reject(cellarError('timeout', 'QueryState was not acknowledged'));

    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mountPicker({
      isAdmin: true,
      initialScope: 'room',
      roomSocket: asRoomSocket(socket),
      roomKey: ROOM_KEY,
    });

    await vi.waitFor(() => {
      expect(warn).toHaveBeenCalled();
    });
    const card = target.querySelector('[data-testid="asset-card-cached-room-asset"]');
    expect(card?.textContent).toContain('cached.png');
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeTruthy();
    warn.mockRestore();
  });

  it('deletes room assets through requestDeleteState and keeps the card when the delete fails', async () => {
    addAsset({
      id: 'room-deletable',
      name: 'deletable.png',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png',
      createdAt: 1000,
    });
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    // The rejected delete is expected: silence the diagnostic warn.
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    mountPicker({
      isAdmin: true,
      initialScope: 'room',
      roomSocket: asRoomSocket(socket),
      roomKey: ROOM_KEY,
    });

    const deleteBtn = target.querySelector<HTMLElement>(
      '[data-testid="asset-delete-btn-room-deletable"]',
    );
    expect(deleteBtn).toBeTruthy();
    deleteBtn!.click();

    await vi.waitFor(() => {
      expect(socket.pendingStateOperationCount).toBe(1);
    });
    expect(socket.requestDeleteStateCalls[0]).toEqual({
      sceneId: '/',
      key: `${ASSET_STATE_KEY_PREFIX}room-deletable`,
    });

    // Failed delete: the card stays and a notice explains the failure.
    socket.rejectDeleteState(cellarError('timeout', 'DeleteState was not acknowledged'));
    await vi.waitFor(() => {
      expect(target.querySelector('[data-testid="asset-picker-notice"]')?.textContent).toContain(
        '削除に失敗しました',
      );
    });
    expect(target.querySelector('[data-testid="asset-card-room-deletable"]')).toBeTruthy();
    expect(loadAssets().some((asset) => asset.id === 'room-deletable')).toBe(true);

    // Retrying the delete succeeds and removes the card.
    socket.autoAnswerStateWrites = true;
    target.querySelector<HTMLElement>('[data-testid="asset-delete-btn-room-deletable"]')!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('[data-testid="asset-card-room-deletable"]')).toBeNull();
    });
    expect(loadAssets().some((asset) => asset.id === 'room-deletable')).toBe(false);
    expect(socket.requestDeleteStateCalls).toHaveLength(2);
    warn.mockRestore();
  });

  it('keeps room-scope delete admin-only and deletes personal assets locally', async () => {
    addAsset({
      id: 'personal-deletable',
      name: 'personal.png',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png',
      createdAt: 1000,
    });
    const socket = new FakeRoomSocket();
    mountPicker({
      isAdmin: false,
      initialScope: 'personal',
      roomSocket: asRoomSocket(socket),
      roomKey: ROOM_KEY,
    });

    // Non-admins cannot switch to the room scope.
    const roomRadio = target.querySelector<HTMLInputElement>(
      '[data-testid="asset-scope-segmented"] input[value="room"]',
    );
    expect(roomRadio?.disabled).toBe(true);

    target
      .querySelector<HTMLElement>('[data-testid="asset-delete-btn-personal-deletable"]')!
      .click();
    await vi.waitFor(() => {
      expect(target.querySelector('[data-testid="asset-card-personal-deletable"]')).toBeNull();
    });
    expect(socket.requestDeleteStateCalls).toHaveLength(0);
    expect(loadAssets().some((asset) => asset.id === 'personal-deletable')).toBe(false);
  });
});
