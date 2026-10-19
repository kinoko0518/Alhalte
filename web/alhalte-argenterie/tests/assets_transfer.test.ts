/**
 * Unit tests for the cellar-backed asset transfer (Issue #109 / #141).
 *
 * No network / XHR: `utils/cellarTransfer.ts` is fully mocked with `vi.mock`,
 * and the room socket is `tests/helpers/fakeRoomSocket.ts`. Every test passes an
 * explicit 32-byte room key instead of relying on `window.location.hash`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### アセット`
 * - GitHub Issue #109 / sub-issue #141
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';

const cellar = vi.hoisted(() => {
  class MockCellarTransferError extends Error {
    code: string;
    status?: number;
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
    getRoomKeyFromUrl: vi.fn(() => null),
    resolveRoomKey: vi.fn((options: { roomKey?: Uint8Array | null } = {}) => options.roomKey ?? null),
    isCellarTransferError: (error: unknown) => error instanceof MockCellarTransferError,
    CellarTransferError: MockCellarTransferError,
  };
});

vi.mock('../src/utils/cellarTransfer.js', () => cellar);

import {
  ASSET_STATE_KEY_PREFIX,
  ASSET_STATE_SCENE_ID,
  LABEL_ASSET,
  addAsset,
  clearAssets,
  loadAssets,
  syncRoomAssets,
  uploadAssetFromFile,
  type AssetMetadataSocket,
  type RoomAssetMetadata,
} from '../src/utils/assets.js';
import { FakeRoomSocket } from './helpers/fakeRoomSocket.js';

/** 32 zero bytes: the room key used by every test. */
const ROOM_KEY = new Uint8Array(32);

const TIN_ID_A = 'a1'.repeat(32);
const TIN_ID_B = 'b2'.repeat(32);
const TIN_ID_C = 'c3'.repeat(32);
const ENCODED_TIN_ID = 'd4'.repeat(32);
const WIRE_BYTES = new Uint8Array([9, 8, 7]);

/** Builds a `CellarTransferError`-shaped failure (the mock recognises it). */
function transferError(code: string, message: string): Error {
  return new cellar.CellarTransferError(code, message);
}

/** Encodes the payload as the mocked `encodeRoomTin` would. */
function mockEncode(tinIdHex = ENCODED_TIN_ID): void {
  cellar.encodeRoomTin.mockResolvedValue({ wireBytes: WIRE_BYTES, tinIdHex });
}

/** Builds a small image File. */
function imageFile(name = 'photo.png', bytes = [1, 2, 3]): File {
  return new File([new Uint8Array(bytes)], name, { type: 'image/png' });
}

/** Builds a room metadata record as another client would publish it. */
function metadata(
  id: string,
  tinId: string,
  overrides: Partial<RoomAssetMetadata> = {},
): RoomAssetMetadata {
  return {
    id,
    name: `${id}.png`,
    mediaType: 'image',
    mimeType: 'image/png',
    size: 3,
    tinId,
    createdAt: 1000,
    ...overrides,
  };
}

/** Decrypted tin returned by the mocked `downloadTin`. */
function decryptedTin(tinId: string, payload = new Uint8Array([1, 2, 3])): any {
  return { tinId, label: LABEL_ASSET, payload, opener: {} };
}

beforeEach(() => {
  clearAssets();
  localStorage.clear();
  cellar.encodeRoomTin.mockReset();
  cellar.uploadTin.mockReset();
  cellar.downloadTin.mockReset();
  cellar.getRoomKeyFromLocation.mockReset();
  cellar.getRoomKeyFromLocation.mockReturnValue(null);
});

describe('uploadAssetFromFile (Issue #141)', () => {
  it('uploads room assets with real progress and publishes asset:: metadata', async () => {
    mockEncode();
    const progress: number[] = [];
    cellar.uploadTin.mockImplementation(async (_bytes: Uint8Array, options: any) => {
      options.onProgress?.(0.25);
      options.onProgress?.(0.75);
      return { tinId: ENCODED_TIN_ID };
    });

    const socket = new FakeRoomSocket();
    const file = imageFile('room-photo.png');
    const result = await uploadAssetFromFile(file, 'room', 'image', {
      roomKey: ROOM_KEY,
      roomSocket: socket,
      timeoutMs: 4321,
      ownerAccountId: 42,
      onProgress: (value) => progress.push(value),
    });

    // Encoding is room-key encrypted with the asset label.
    expect(cellar.encodeRoomTin).toHaveBeenCalledTimes(1);
    expect(cellar.encodeRoomTin).toHaveBeenCalledWith(
      new Uint8Array([1, 2, 3]),
      LABEL_ASSET,
      ROOM_KEY,
    );

    // The upload used the encoded tin id, the caller timeout, and real progress.
    expect(cellar.uploadTin).toHaveBeenCalledTimes(1);
    const uploadOptions = cellar.uploadTin.mock.calls[0]![1] as any;
    expect(uploadOptions.tinId).toBe(ENCODED_TIN_ID);
    expect(uploadOptions.timeoutMs).toBe(4321);
    expect(progress).toEqual([0.25, 0.75]);

    // The card settles idle with the tin id and a preview.
    expect(result.error).toBeUndefined();
    expect(result.asset.uploadState).toBe('idle');
    expect(result.asset.tinId).toBe(ENCODED_TIN_ID);
    expect(result.asset.uploadProgress).toBe(1);
    expect(result.asset.size).toBe(file.size);
    expect(result.asset.dataUrl.startsWith('data:image/png;base64,')).toBe(true);

    const stored = loadAssets().find((asset) => asset.id === result.asset.id);
    expect(stored).toMatchObject({
      scope: 'room',
      name: 'room-photo.png',
      tinId: ENCODED_TIN_ID,
      uploadState: 'idle',
    });

    // Room metadata is published under `asset::<id>` in the `/` scene.
    expect(socket.requestUpdateStateCalls).toHaveLength(1);
    expect(socket.requestUpdateStateCalls[0]!.sceneId).toBe(ASSET_STATE_SCENE_ID);
    expect(socket.requestUpdateStateCalls[0]!.key).toBe(
      `${ASSET_STATE_KEY_PREFIX}${result.asset.id}`,
    );
    expect(socket.requestUpdateStateCalls[0]!.value).toEqual({
      id: result.asset.id,
      name: 'room-photo.png',
      mediaType: 'image',
      mimeType: 'image/png',
      size: file.size,
      tinId: ENCODED_TIN_ID,
      createdAt: result.asset.createdAt,
      ownerAccountId: 42,
    });
    expect(result.metadata).toEqual(socket.requestUpdateStateCalls[0]!.value);
  });

  it('keeps the card while uploading and reports only XHR progress (no fake ticks)', async () => {
    mockEncode();
    let releaseUpload!: () => void;
    const uploadGate = new Promise<void>((resolve) => {
      releaseUpload = resolve;
    });
    cellar.uploadTin.mockImplementation(async (_bytes: Uint8Array, options: any) => {
      options.onProgress?.(0.5);
      await uploadGate;
      return { tinId: ENCODED_TIN_ID };
    });

    const upload = uploadAssetFromFile(imageFile('pending.png'), 'room', 'image', {
      roomKey: ROOM_KEY,
      roomSocket: new FakeRoomSocket(),
    });

    // The card exists immediately with a preview and an uploading state.
    await vi.waitFor(() => {
      const pending = loadAssets().find((asset) => asset.name === 'pending.png');
      expect(pending?.uploadState).toBe('uploading');
      expect(pending?.uploadProgress).toBe(0.5);
    });
    const pending = loadAssets().find((asset) => asset.name === 'pending.png')!;
    expect(pending.dataUrl.startsWith('data:image/png;base64,')).toBe(true);

    releaseUpload();
    const result = await upload;
    expect(result.asset.uploadState).toBe('idle');
    expect(loadAssets().find((asset) => asset.id === pending.id)?.uploadState).toBe('idle');
  });

  it('marks the card failed on timeout without touching other cards', async () => {
    // An unrelated room card that must stay usable.
    addAsset({
      id: 'untouched-room-asset',
      name: 'untouched.png',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png',
      createdAt: 1,
    });

    mockEncode();
    const socket = new FakeRoomSocket();
    cellar.uploadTin.mockRejectedValue(
      transferError('timeout', 'Cellar request to /api/cellar/store exceeded 8000 ms'),
    );

    const result = await uploadAssetFromFile(imageFile('slow.png'), 'room', 'image', {
      roomKey: ROOM_KEY,
      roomSocket: socket,
    });

    expect(result.asset.uploadState).toBe('failed');
    expect(result.error).toContain('timeout');
    expect(result.asset.uploadError).toContain('timeout');
    expect(result.asset.tinId).toBeUndefined();
    // Nothing was published, and the card is still stored for a retry.
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(loadAssets().find((asset) => asset.name === 'slow.png')).toMatchObject({
      uploadState: 'failed',
    });
    const untouched = loadAssets().find((asset) => asset.id === 'untouched-room-asset');
    expect(untouched?.uploadState).toBeUndefined();
    expect(untouched?.dataUrl).toBe('data:image/png;base64,AAAA');
  });

  it('marks the card failed when the room metadata write is rejected', async () => {
    mockEncode();
    cellar.uploadTin.mockResolvedValue({ tinId: ENCODED_TIN_ID });
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;

    const pending = uploadAssetFromFile(imageFile('meta-fail.png'), 'room', 'image', {
      roomKey: ROOM_KEY,
      roomSocket: socket,
    });

    await vi.waitFor(() => expect(socket.pendingStateOperationCount).toBe(1));
    socket.rejectStateWrite(transferError('timeout', 'StateUpdated was not acknowledged'));

    const result = await pending;
    expect(result.asset.uploadState).toBe('failed');
    expect(result.error).toContain('timeout');
    // The bytes are on the cellar but the card stays failed until a retry.
    expect(result.asset.tinId).toBe(ENCODED_TIN_ID);
  });

  it('uploads personal assets but never publishes their metadata', async () => {
    mockEncode();
    cellar.uploadTin.mockResolvedValue({ tinId: ENCODED_TIN_ID });
    const socket = new FakeRoomSocket();

    const result = await uploadAssetFromFile(imageFile('mine.png'), 'personal', 'image', {
      roomKey: ROOM_KEY,
      roomSocket: socket,
    });

    expect(result.asset.scope).toBe('personal');
    expect(result.asset.tinId).toBe(ENCODED_TIN_ID);
    expect(result.metadata).toBeUndefined();
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    // The bytes still went through the cellar with room-key encryption.
    expect(cellar.uploadTin).toHaveBeenCalledTimes(1);
  });

  it('fails without calling the cellar when no room key is available', async () => {
    const socket = new FakeRoomSocket();
    const result = await uploadAssetFromFile(imageFile('no-key.png'), 'room', 'image', {
      roomKey: null,
      roomSocket: socket,
    });

    expect(result.asset.uploadState).toBe('failed');
    expect(result.error).toMatch(/room key/i);
    expect(cellar.encodeRoomTin).not.toHaveBeenCalled();
    expect(cellar.uploadTin).not.toHaveBeenCalled();
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    // The card stays so the user can retry once the key is back.
    expect(loadAssets().some((asset) => asset.name === 'no-key.png')).toBe(true);
  });

  it('fails a room upload when no socket can publish the metadata', async () => {
    mockEncode();
    cellar.uploadTin.mockResolvedValue({ tinId: ENCODED_TIN_ID });

    const result = await uploadAssetFromFile(imageFile('no-socket.png'), 'room', 'image', {
      roomKey: ROOM_KEY,
      roomSocket: null,
    });

    expect(result.asset.uploadState).toBe('failed');
    expect(result.error).toMatch(/socket/i);
  });
});

describe('syncRoomAssets (Issue #141)', () => {
  it('merges metadata, reuses cached previews, downloads missing tins, and keeps local extras', async () => {
    addAsset({
      id: 'room-cached',
      name: 'old-name.png',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png',
      createdAt: 1000,
    });
    addAsset({
      id: 'room-local-only',
      name: 'local-only.png',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,BBBB',
      mimeType: 'image/png',
      createdAt: 900,
    });
    addAsset({
      id: 'personal-only',
      name: 'mine.png',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,CCCC',
      mimeType: 'image/png',
      createdAt: 800,
    });

    const socket = new FakeRoomSocket();
    socket.stateSnapshot = {
      [`${ASSET_STATE_KEY_PREFIX}room-cached`]: metadata('room-cached', TIN_ID_A, {
        name: 'renamed.png',
      }),
      [`${ASSET_STATE_KEY_PREFIX}remote-new`]: metadata('remote-new', TIN_ID_B, {
        name: 'remote.png',
      }),
      'std::bgm': TIN_ID_C,
    };
    cellar.downloadTin.mockResolvedValue(decryptedTin(TIN_ID_B));

    const result = await syncRoomAssets(socket, ROOM_KEY);

    // One query for the room root scene; only the uncached tin is downloaded.
    expect(socket.queryStateCalls).toEqual([{ sceneId: ASSET_STATE_SCENE_ID }]);
    expect(cellar.downloadTin).toHaveBeenCalledTimes(1);
    expect(cellar.downloadTin).toHaveBeenCalledWith(TIN_ID_B, ROOM_KEY);
    expect(result.syncFailed).toBe(false);
    expect(result.mergedCount).toBe(2);
    expect(result.downloadedIds).toEqual(['remote-new']);
    expect(result.failedIds).toEqual([]);

    const assets = loadAssets();
    const cached = assets.find((asset) => asset.id === 'room-cached')!;
    expect(cached.name).toBe('renamed.png');
    expect(cached.tinId).toBe(TIN_ID_A);
    expect(cached.dataUrl).toBe('data:image/png;base64,AAAA');
    expect(cached.uploadState).toBe('idle');

    const downloaded = assets.find((asset) => asset.id === 'remote-new')!;
    expect(downloaded.scope).toBe('room');
    expect(downloaded.name).toBe('remote.png');
    expect(downloaded.tinId).toBe(TIN_ID_B);
    expect(downloaded.dataUrl).toBe('data:image/png;base64,AQID');
    expect(downloaded.size).toBe(3);
    expect(downloaded.uploadState).toBe('idle');

    // Local extras are never deleted just because they are absent server-side.
    expect(assets.some((asset) => asset.id === 'room-local-only')).toBe(true);
    expect(assets.some((asset) => asset.id === 'personal-only')).toBe(true);
  });

  it('marks only the failing card when a tin cannot be downloaded', async () => {
    addAsset({
      id: 'room-cached',
      name: 'cached.png',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png',
      createdAt: 1000,
    });
    const socket = new FakeRoomSocket();
    socket.stateSnapshot = {
      [`${ASSET_STATE_KEY_PREFIX}room-cached`]: metadata('room-cached', TIN_ID_A),
      [`${ASSET_STATE_KEY_PREFIX}broken`]: metadata('broken', TIN_ID_C, {
        name: 'broken.png',
      }),
    };
    cellar.downloadTin.mockRejectedValue(
      transferError('timeout', 'Cellar download of tin exceeded 8000 ms'),
    );

    const result = await syncRoomAssets(socket, ROOM_KEY);

    expect(result.failedIds).toEqual(['broken']);
    expect(result.downloadedIds).toEqual([]);
    const assets = loadAssets();
    const broken = assets.find((asset) => asset.id === 'broken')!;
    expect(broken.uploadState).toBe('failed');
    expect(broken.uploadError).toContain('timeout');
    expect(broken.dataUrl).toBe('');
    // The cached card is unaffected.
    expect(assets.find((asset) => asset.id === 'room-cached')?.uploadState).toBe('idle');
  });

  it('keeps the local cache when the state query times out', async () => {
    addAsset({
      id: 'room-cached',
      name: 'cached.png',
      scope: 'room',
      mediaType: 'image',
      dataUrl: 'data:image/png;base64,AAAA',
      mimeType: 'image/png',
      createdAt: 1000,
    });
    const socket = new FakeRoomSocket();
    socket.queryState = () =>
      Promise.reject(transferError('timeout', 'QueryState was not acknowledged'));

    const result = await syncRoomAssets(socket, ROOM_KEY);

    expect(result.syncFailed).toBe(true);
    expect(result.error).toContain('timeout');
    expect(result.assets).toHaveLength(1);
    expect(result.assets[0]!.id).toBe('room-cached');
    expect(cellar.downloadTin).not.toHaveBeenCalled();
  });

  it('reports a missing room key and does not query the server', async () => {
    const socket = new FakeRoomSocket();
    const result = await syncRoomAssets(socket, null);

    expect(result.syncFailed).toBe(true);
    expect(result.error).toMatch(/room key/i);
    expect(socket.queryStateCalls).toHaveLength(0);
  });

  it('skips silently without a socket or while disconnected', async () => {
    const noSocket = await syncRoomAssets(null, ROOM_KEY);
    expect(noSocket.syncFailed).toBe(false);
    expect(noSocket.assets).toEqual([]);

    const disconnected = new FakeRoomSocket();
    disconnected.isConnected = false;
    const result = await syncRoomAssets(disconnected, ROOM_KEY);
    expect(result.syncFailed).toBe(false);
    expect(disconnected.queryStateCalls).toHaveLength(0);
  });

  it('ignores malformed asset state entries', async () => {
    const socket = new FakeRoomSocket();
    socket.stateSnapshot = {
      [`${ASSET_STATE_KEY_PREFIX}no-tin`]: { id: 'no-tin', mediaType: 'image' },
      [`${ASSET_STATE_KEY_PREFIX}bad-media`]: {
        ...metadata('bad-media', TIN_ID_A),
        mediaType: 'video',
      },
      'other::key': metadata('ignored', TIN_ID_B),
    };
    cellar.downloadTin.mockResolvedValue(decryptedTin(TIN_ID_A));

    const result = await syncRoomAssets(socket, ROOM_KEY);

    expect(result.mergedCount).toBe(0);
    expect(result.downloadedIds).toEqual([]);
    expect(cellar.downloadTin).not.toHaveBeenCalled();
    expect(loadAssets()).toHaveLength(0);
  });
});

describe('AssetMetadataSocket contract', () => {
  it('accepts the real RoomSocket surface without adaptation', () => {
    // Compile-time check kept in a test so the structural type cannot drift
    // away from `RoomSocket.queryState` / `requestUpdateState`.
    const socket = new FakeRoomSocket();
    const adapter: AssetMetadataSocket = {
      queryState: socket.queryState.bind(socket),
      requestUpdateState: socket.requestUpdateState.bind(socket),
      isConnected: socket.isConnected,
    };
    expect(typeof adapter.queryState).toBe('function');
  });
});
