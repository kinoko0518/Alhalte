/**
 * BGM スロット追加フローの可視エラー (GitHub Issue #275).
 *
 * Issue #249 以降、行は「track の書き込みが ack された」ときにだけ生まれる。
 * つまり「スロット追加」の失敗は必ず *行を持たないスロット* で起きるため、
 * 行の中にしかエラーを描かないと失敗がまったく見えない (Issue #275 の原因)。
 * ここでは節レベルの `bgm-add-error` が
 *
 * 1. 書き込み拒否 (行なし・楽観的な行なし),
 * 2. prepare 失敗 (部屋鍵なし),
 * 3. tinId の無い音源,
 * 4. 読み取り専用ロール,
 *
 * を可視化し、正常系 (4.) が従来どおり 1 行を増やすことを固定する。
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### BGM`
 * - GitHub Issue #249 (行 = track), #250 (トランスポート), #275 (この修正)
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { clearAssets, addAsset } from '../src/utils/assets.js';
import { OperationTimeoutError } from '../src/utils/roomSocket.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { AssetItem } from '../src/utils/assets.js';
import type { RoomRole } from '../src/utils/session.js';
import type { Scene } from '../src/types/index.js';

const TIN_ID_A = 'a'.repeat(64);
const ERROR_MESSAGE = 'サーバーと通信できませんでした';

/** Waits for the microtask queue plus Svelte's follow-up render. */
async function settle(ms = 20): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

function makeScene(overrides?: Partial<Scene>): Scene {
  return {
    id: 'scene-bgm-add',
    name: 'BGM追加テスト',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
    ...overrides,
  };
}

/** An audio asset that can be published to the cellar (64 lowercase hex). */
function audioAsset(overrides: Partial<AssetItem> = {}): AssetItem {
  return addAsset({
    id: 'bgm-add-asset',
    name: 'add.ogg',
    scope: 'personal',
    mediaType: 'audio',
    dataUrl: 'data:audio/ogg;base64,QURN',
    mimeType: 'audio/ogg',
    createdAt: 1000,
    tinId: TIN_ID_A,
    ...overrides,
  });
}

interface MountOptions {
  scene?: Scene;
  socket?: FakeRoomSocket;
  role?: RoomRole;
  roomKey?: Uint8Array | null;
  onRequestAssetPicker?: (options: any) => void;
  onUpdateScene?: (scene: Scene) => void;
}

function mountTab(options: MountOptions = {}) {
  const scene = options.scene ?? makeScene();
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(SceneSettingsTab, {
    target,
    props: {
      currentScene: scene,
      allScenes: [scene],
      ...(options.role ? { role: options.role } : {}),
      ...(options.socket ? { roomSocket: asRoomSocket(options.socket) } : {}),
      ...(options.roomKey !== undefined ? { roomKey: options.roomKey } : {}),
      ...(options.onRequestAssetPicker
        ? { onRequestAssetPicker: options.onRequestAssetPicker }
        : {}),
      ...(options.onUpdateScene ? { onUpdateScene: options.onUpdateScene } : {}),
    },
  });
  return { scene, target, instance };
}

function cleanup(target: HTMLElement, instance: Record<string, any>): void {
  unmount(instance);
  target.remove();
}

describe('BGM slot add flow — visible errors (Issue #275)', () => {
  beforeEach(() => {
    clearAssets();
  });

  // spec: Issue #275 追加フローが失敗したら行が無くてもエラーを表示する
  it('shows the communication error under the section header when the first write of a new slot is rejected', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const asset = audioAsset();
    const scene = makeScene();
    let requestedOptions: any = null;
    let updateCount = 0;
    const { target, instance } = mountTab({
      scene,
      socket,
      onRequestAssetPicker: (options) => {
        requestedOptions = options;
      },
      onUpdateScene: () => {
        updateCount += 1;
      },
    });
    await settle();

    // No row yet, so there is nothing to render the error under and nothing to
    // show before the failure (Issue #249).
    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeNull();
    expect(target.querySelectorAll('[role="status"]')).toHaveLength(0);

    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.click();
    await settle();
    expect(requestedOptions).toBeTruthy();
    expect(requestedOptions.mediaType).toBe('audio');
    expect(target.querySelector('[data-testid="bgm-add-error"]')).toBeNull();

    requestedOptions.onSelect(asset.dataUrl, asset);
    await settle(0);

    // The track write of the new slot is in flight: still no row, no optimistic
    // row and no error.
    expect(socket.requestUpdateStateCalls.map((call) => call.key)).toEqual(['std::bgm']);
    expect(target.querySelector('[data-testid="bgm-slot-block-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="bgm-add-error"]')).toBeNull();

    socket.rejectStateWrite(new OperationTimeoutError('updateState', 8000));
    await settle();

    // The failure is visible even though the slot has no row.
    const error = target.querySelector<HTMLElement>('[data-testid="bgm-add-error"]');
    expect(error).toBeTruthy();
    expect(error!.getAttribute('role')).toBe('status');
    expect(error!.textContent).toContain(ERROR_MESSAGE);
    // It lives in the section, not inside a row.
    expect(target.querySelector('[data-testid="bgm-controls"]')!.contains(error!)).toBe(true);
    expect(target.querySelectorAll('[role="status"]')).toHaveLength(1);

    // No row, no optimistic local model: the state was never written.
    expect(target.querySelector('[data-testid="bgm-slot-block-1"]')).toBeNull();
    expect(updateCount).toBe(0);
    expect(scene.juices.bgmSlots).toEqual([]);

    warn.mockRestore();
    cleanup(target, instance);
  });

  // spec: Issue #275 prepare の失敗 (部屋鍵なし) も可視エラーにする
  it('shows the audio manager error when the prepare of a new slot fails', async () => {
    const socket = new FakeRoomSocket();
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    const asset = audioAsset();
    const scene = makeScene();
    let requestedOptions: any = null;
    const { target, instance } = mountTab({
      scene,
      socket,
      // `null` means a room exists but its key was not resolved: the manager
      // cannot decrypt the tin, which must never write the state (Issue #275).
      roomKey: null,
      onRequestAssetPicker: (options) => {
        requestedOptions = options;
      },
    });
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.click();
    await settle();
    requestedOptions.onSelect(asset.dataUrl, asset);
    await settle();

    const error = target.querySelector<HTMLElement>('[data-testid="bgm-add-error"]');
    expect(error).toBeTruthy();
    expect(error!.getAttribute('role')).toBe('status');
    // The manager's own user-facing message is preferred over the generic one.
    expect(error!.textContent).toContain('部屋鍵がないため、BGMを再生できません');
    // The prepare gate is before every state write.
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(socket.requestDeleteStateCalls).toHaveLength(0);
    expect(target.querySelector('[data-testid="bgm-slot-block-1"]')).toBeNull();
    expect(scene.juices.bgmSlots).toEqual([]);

    warn.mockRestore();
    cleanup(target, instance);
  });

  // spec: Issue #275 tinId の無い音源は可視エラーにする
  it('shows the missing tin id error when the selected audio asset has no tinId', async () => {
    const socket = new FakeRoomSocket();
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    // `addAsset` without a `tinId`: a local-only audio entry.
    const asset = audioAsset({ id: 'bgm-add-no-tin', name: 'local.ogg', tinId: undefined });
    const scene = makeScene();
    let requestedOptions: any = null;
    const { target, instance } = mountTab({
      scene,
      socket,
      onRequestAssetPicker: (options) => {
        requestedOptions = options;
      },
    });
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.click();
    await settle();
    requestedOptions.onSelect(asset.dataUrl, asset);
    await settle();

    const error = target.querySelector<HTMLElement>('[data-testid="bgm-add-error"]');
    expect(error).toBeTruthy();
    expect(error!.getAttribute('role')).toBe('status');
    expect(error!.textContent).toContain('この音源はサーバーに保存されていません');
    // An unsynchronisable value is never written and no row is created.
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(socket.requestDeleteStateCalls).toHaveLength(0);
    expect(target.querySelector('[data-testid="bgm-slot-block-1"]')).toBeNull();
    expect(scene.juices.bgmSlots).toEqual([]);

    warn.mockRestore();
    cleanup(target, instance);
  });

  // spec: Issue #275 読み取り専用ロールはスロット追加できない
  it('disables the add button for a read-only role and never starts the flow', async () => {
    for (const role of ['viewer', 'denied'] as const) {
      const socket = new FakeRoomSocket();
      const scene = makeScene();
      let requestedOptions: any = null;
      const { target, instance } = mountTab({
        scene,
        socket,
        role,
        onRequestAssetPicker: (options) => {
          requestedOptions = options;
        },
      });
      await settle();

      // Same capability predicate as the row controls (`canSend`).
      const addBtn = target.querySelector<HTMLButtonElement>(
        '[data-testid="add-bgm-slot-btn"]',
      )!;
      expect(addBtn.disabled).toBe(true);

      addBtn.click();
      await settle();

      // No picker, no state write, no row.
      expect(requestedOptions).toBeNull();
      expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeNull();
      expect(socket.requestUpdateStateCalls).toHaveLength(0);
      expect(socket.requestDeleteStateCalls).toHaveLength(0);
      expect(target.querySelector('[data-testid="bgm-slot-block-1"]')).toBeNull();
      expect(scene.juices.bgmSlots).toEqual([]);

      cleanup(target, instance);
    }

    // A participant (canSend) still gets a usable button.
    const socket = new FakeRoomSocket();
    const { target, instance } = mountTab({ socket, role: 'participant' });
    await settle();
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.disabled,
    ).toBe(false);
    cleanup(target, instance);
  });

  // spec: Issue #275 正常系は従来どおり行が増える
  it('still adds exactly one row with the acknowledged track and transport writes', async () => {
    const socket = new FakeRoomSocket();
    const asset = audioAsset();
    const scene = makeScene();
    let requestedOptions: any = null;
    let updateCount = 0;
    let lastUpdatedScene: Scene | null = null;
    const { target, instance } = mountTab({
      scene,
      socket,
      onRequestAssetPicker: (options) => {
        requestedOptions = options;
      },
      onUpdateScene: (updated) => {
        updateCount += 1;
        lastUpdatedScene = updated;
      },
    });
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="add-bgm-slot-btn"]')!.click();
    await settle();
    requestedOptions.onSelect(asset.dataUrl, asset);
    await settle();

    // Issue #250: track -> started_at -> delete paused_at -> playing=true.
    expect(socket.requestUpdateStateCalls.map((call) => call.key)).toEqual([
      'std::bgm',
      'bgm::1::started_at',
      'bgm::1::playing',
    ]);
    expect(socket.requestDeleteStateCalls.map((call) => call.key)).toEqual([
      'bgm::1::paused_at',
    ]);
    // The room state stores the tin reference, never the local data URL.
    const track = socket.requestUpdateStateCalls[0].value as {
      tinId: string;
      mimeType: string;
    };
    expect(track.tinId).toBe(TIN_ID_A);
    expect(track.mimeType).toBe('audio/ogg');

    // Exactly one row, committed once after the acks.
    expect(updateCount).toBe(1);
    expect(lastUpdatedScene!.juices.bgmSlots?.map((slot) => slot.slot)).toEqual([1]);
    expect(lastUpdatedScene!.juices.bgm1).toBe(TIN_ID_A);
    expect(target.querySelector('[data-testid="bgm-row-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="bgm-slot-block-2"]')).toBeNull();
    // Nothing failed, so the section-level error stays absent.
    expect(target.querySelector('[data-testid="bgm-add-error"]')).toBeNull();

    cleanup(target, instance);
  });
});
