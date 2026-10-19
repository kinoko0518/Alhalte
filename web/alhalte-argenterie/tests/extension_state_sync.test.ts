/**
 * Integration tests for the extension tab server synchronization.
 *
 * Contract under test (Issue #109 / sub-issue #145):
 * - opening the extensions tab sends `QueryState('/')` and shows a contained M3
 *   `LoadingIndicator` over the cached list,
 * - the server metadata is merged into the local list (built-in official ids
 *   stay, custom ids absent remotely are removed, local wasm is kept when the
 *   tin id did not change),
 * - a timeout flips the tab into the offline state ("サーバーと通信できませんでした",
 *   dimmed, `aria-disabled`, inert even for admins) and a reconnect retries,
 * - toggle / reorder / delete send the matching state write and update the local
 *   model only after the acknowledgement (wavy `CircularProgress` while pending),
 * - a failed write leaves the list unchanged and flips the tab offline,
 * - uploading a `.wasm` validates it, stores it in the cellar (room-key
 *   encrypted) and publishes only `{ ..., tinId }` in `extension::<id>`, so the
 *   state value stays far below the 4 KiB limit.
 *
 * `cellarTransfer` is mocked so the suite never touches the network.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 拡張機能`
 * - GitHub Issue #109 / sub-issue #145
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import fs from 'node:fs';
import path from 'node:path';
import { MemoryPantryCacheAdapter } from '@alhalte/pantry';
import App from '../src/components/App.svelte';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import { createConnectionManager } from '../src/utils/connection.js';
import { OperationTimeoutError } from '../src/utils/roomSocket.js';
import { clearAssets } from '../src/utils/assets.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import {
  EXTENSION_SEEDED_MARKER_VALUE,
  EXTENSION_SEEDED_STATE_KEY,
  EXTENSION_STATE_SCENE_ID,
  EXTENSION_STATE_KEY_PREFIX,
  EXTENSION_WASM_LABEL,
  ExtensionStateError,
  applyRemoteExtensions,
  buildExtensionSeed,
  extensionFromMetadata,
  extensionStateKey,
  extensionToMetadata,
  isExtensionSeedMarkerKey,
  isExtensionStateKey,
  isExtensionStateSeeded,
  loadExtensionWasm,
  mergeExtensionState,
  uploadExtensionWasm,
} from '../src/utils/extensions.js';
import { DEFAULT_OFFICIAL_EXTENSIONS } from '../src/wasm/runner.js';
import { downloadTin, uploadTin } from '../src/utils/cellarTransfer.js';
import type { ExtensionItem, Scene, VisitedRoom } from '../src/types/index.js';

vi.mock('../src/utils/cellarTransfer.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../src/utils/cellarTransfer.js')>();
  return {
    ...actual,
    uploadTin: vi.fn(),
    downloadTin: vi.fn(),
  };
});

const ROOM: VisitedRoom = {
  id: 'room-extension-sync-145',
  name: '拡張機能同期テスト部屋',
  url: 'https://alhalte.example/rooms/room-extension-sync-145#key',
  lastVisited: Date.now(),
  isAdmin: true,
};

const SCENE: Scene = {
  id: '/',
  name: 'メインシーン',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: [],
};

/** Deterministic 32-byte room key (never the URL fragment in tests). */
const ROOM_KEY = new Uint8Array(32).fill(7);

/** 64-hex cellar tin id used by the mocked transfers. */
const WASM_TIN_ID = 'a'.repeat(64);

/**
 * Server state of a room whose official defaults were already seeded
 * (Issue #165): the marker plus one metadata entry per official extension.
 */
function seededState(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  const state: Record<string, unknown> = {
    [EXTENSION_SEEDED_STATE_KEY]: EXTENSION_SEEDED_MARKER_VALUE,
  };
  for (const ext of DEFAULT_OFFICIAL_EXTENSIONS) {
    state[extensionStateKey(ext.id)] = extensionToMetadata(ext);
  }
  return { ...state, ...overrides };
}

const markdownWasm = fs.readFileSync(
  path.resolve(__dirname, '../public/extensions/markdown.wasm'),
);

function tick(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(predicate: () => boolean, timeoutMs = 2000): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await tick(10);
  }
}

describe('Extension tab server synchronization (Issue #109 / #145)', () => {
  let target: HTMLDivElement;
  let app: Record<string, any> | null = null;

  beforeEach(() => {
    clearAssets();
    document.body.innerHTML = '';
    localStorage.clear();
    vi.mocked(uploadTin).mockReset();
    vi.mocked(downloadTin).mockReset();
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    if (app) {
      unmount(app);
      app = null;
    }
    document.body.innerHTML = '';
  });

  function mountApp(
    socket: FakeRoomSocket,
    initialState: 'connected' | 'disconnected' = 'connected',
    initialExtensions: ExtensionItem[] = [],
    isAdmin = true,
  ) {
    const connectionManager = createConnectionManager({ initialState });
    app = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [SCENE],
        initialCharacters: [],
        initialExtensions,
        isAdmin,
        connectionManager,
        roomSocket: asRoomSocket(socket),
        roomKey: ROOM_KEY,
      },
    }) as unknown as Record<string, any>;
    return { connectionManager };
  }

  function openExtensionsTab(): void {
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-extensions"]')?.click();
  }

  function extensionTabRoot(): HTMLElement | null {
    return target.querySelector<HTMLElement>('[data-testid="extension-management-tab"]');
  }

  function offlineNotice(): HTMLElement | null {
    return target.querySelector<HTMLElement>('[data-testid="extension-offline-notice"]');
  }

  function toggle(extensionId: string): HTMLInputElement | null {
    return target.querySelector<HTMLInputElement>(`[data-testid="ext-toggle-${extensionId}"]`);
  }

  function extName(extensionId: string): string | null | undefined {
    return target
      .querySelector(`[data-testid="ext-name-${extensionId}"]`)
      ?.textContent?.trim();
  }

  function renderedOrder(): string[] {
    return [...target.querySelectorAll<HTMLElement>('[data-testid^="ext-name-"]')].map(
      (el) => el.getAttribute('data-testid') ?? '',
    );
  }

  async function mountAndOpen(
    socket: FakeRoomSocket,
    options: { seeded?: boolean } = {},
  ): Promise<void> {
    if (options.seeded !== false && Object.keys(socket.stateSnapshot).length === 0) {
      socket.stateSnapshot = seededState();
    }
    mountApp(socket);
    await tick(30);
    openExtensionsTab();
    await tick(30);
  }

  /** Unmounts the App, remounts it against the same (persisted) server state. */
  async function reload(
    socket: FakeRoomSocket,
    options: { seeded?: boolean } = {},
  ): Promise<void> {
    if (app) {
      unmount(app);
      app = null;
    }
    document.body.innerHTML = '';
    target = document.createElement('div');
    document.body.appendChild(target);
    await mountAndOpen(socket, options);
  }

  /** Deletes a row through the `more_vert` menu (menu needs a layout rect). */
  async function deleteViaMenu(extensionId: string): Promise<void> {
    const moreBtn = target.querySelector<HTMLButtonElement>(
      `[data-testid="ext-more-btn-${extensionId}"]`,
    );
    expect(moreBtn).toBeTruthy();
    vi.spyOn(moreBtn!, 'getBoundingClientRect').mockReturnValue({
      top: 150,
      bottom: 186,
      left: 600,
      right: 636,
      width: 36,
      height: 36,
      x: 600,
      y: 150,
      toJSON: () => {},
    } as DOMRect);
    moreBtn?.click();
    await tick(20);
    target
      .querySelector<HTMLElement>(`[data-testid="ext-delete-btn-${extensionId}"]`)
      ?.click();
    await tick(20);
  }

  // spec: Issue #145 タブ表示時に QueryState('/') を発行し、contained LoadingIndicator を出す
  // spec: Issue #165 marker がある部屋ではサーバー state が正（無い公式 id は削除扱い）
  it('queries the root scene when the tab opens and shows a contained LoadingIndicator', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateQueries = false;
    mountApp(socket);
    await tick(30);

    openExtensionsTab();
    await tick(30);

    expect(socket.queryStateCalls).toEqual([{ sceneId: EXTENSION_STATE_SCENE_ID }]);

    const indicator = target.querySelector<HTMLElement>(
      '[data-testid="extension-loading-indicator"]',
    );
    expect(indicator).toBeTruthy();
    expect(indicator?.classList.contains('np-loading-indicator')).toBe(true);
    expect(indicator?.classList.contains('contained')).toBe(true);
    const overlay = target.querySelector<HTMLElement>('[data-testid="extension-loading-overlay"]');
    expect(overlay?.getAttribute('role')).toBe('status');
    expect(overlay?.getAttribute('aria-label')).toBe('拡張機能を読み込み中');

    // The cached official list is readable while the query is in flight.
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');

    socket.resolveStateQuery({
      [EXTENSION_SEEDED_STATE_KEY]: true,
      'extension::markdown': {
        id: 'markdown',
        name: 'Markdown 改',
        priority: 0,
        enabled: false,
      },
      // The server metadata is authoritative; dsl is listed with priority 1 so
      // the remote markdown (#0) sorts first.
      [extensionStateKey('alhalte-dsl')]: {
        ...extensionToMetadata(DEFAULT_OFFICIAL_EXTENSIONS[0]),
        priority: 1,
      },
      [extensionStateKey('aozora')]: extensionToMetadata(DEFAULT_OFFICIAL_EXTENSIONS[2]),
    });
    await tick(40);

    expect(target.querySelector('[data-testid="extension-loading-indicator"]')).toBeNull();
    expect(extName('markdown')).toBe('Markdown 改');
    expect(toggle('markdown')?.checked).toBe(false);
    // Official defaults listed by the server are rendered from its metadata.
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');
    expect(extName('aozora')).toBe('青空文庫ルビ・傍点');
    // The marker is present, so the server is authoritative: an official id it
    // does not list (coc-6) was deleted and must not be re-added (Issue #165).
    expect(extName('coc-6')).toBeUndefined();
    expect(isExtensionSeedMarkerKey(EXTENSION_SEEDED_STATE_KEY)).toBe(true);
    expect(extName('__seeded')).toBeUndefined();
    // Priorities are re-sorted: the remote markdown (#0) precedes the DSL.
    expect(renderedOrder()[0]).toBe('ext-name-markdown');
    expect(extensionTabRoot()?.classList.contains('extensions-offline')).toBe(false);
    // A seeded room never writes the seed again.
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
  });

  // spec: Issue #145 タイムアウトでオフライン表示・管理者でも操作不可・再接続で再試行
  it('enters the offline state on a timeout, ignores admin input, and retries after reconnect', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateQueries = false;
    const { connectionManager } = mountApp(socket);
    await tick(30);

    openExtensionsTab();
    await tick(30);
    expect(target.querySelector('[data-testid="extension-loading-indicator"]')).toBeTruthy();

    socket.rejectStateQuery(new OperationTimeoutError('queryState', 8000));
    await tick(40);

    const root = extensionTabRoot();
    expect(root?.classList.contains('extensions-offline')).toBe(true);
    expect(root?.getAttribute('aria-disabled')).toBe('true');
    expect(offlineNotice()?.textContent).toContain('サーバーと通信できませんでした');
    expect(target.querySelector('[data-testid="extension-loading-indicator"]')).toBeNull();
    // The cached list is kept (not cleared) while offline.
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');

    // Even an admin cannot mutate the room state while offline.
    const writesBefore = socket.requestUpdateStateCalls.length;
    toggle('alhalte-dsl')?.click();
    await tick(20);
    expect(socket.requestUpdateStateCalls).toHaveLength(writesBefore);
    expect(offlineNotice()).toBeTruthy();

    // The connection returns: the tab retries `QueryState('/')` and recovers.
    socket.autoAnswerStateQueries = true;
    socket.stateSnapshot = seededState();
    connectionManager.setState('disconnected');
    await tick(20);
    connectionManager.setState('connected');
    await waitFor(() => socket.queryStateCalls.length === 2);
    await tick(30);

    expect(socket.queryStateCalls).toHaveLength(2);
    expect(extensionTabRoot()?.classList.contains('extensions-offline')).toBe(false);
    expect(offlineNotice()).toBeNull();
    expect(toggle('alhalte-dsl')?.checked).toBe(true);
  });

  // spec: Issue #145 toggle は ack 後にのみローカルへ反映し、送信中は行アイコンを CircularProgress にする
  it('toggles only after the state ack and swaps the row icon for a wavy CircularProgress while pending', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    await mountAndOpen(socket);

    toggle('markdown')?.click();
    await tick(20);

    expect(socket.requestUpdateStateCalls).toHaveLength(1);
    const write = socket.requestUpdateStateCalls[0];
    expect(write.sceneId).toBe(EXTENSION_STATE_SCENE_ID);
    expect(write.key).toBe('extension::markdown');
    expect(write.value).toMatchObject({ id: 'markdown', enabled: false });

    const pendingIcon = target.querySelector<HTMLElement>('[data-testid="ext-pending-markdown"]');
    expect(pendingIcon).toBeTruthy();
    expect(pendingIcon?.classList.contains('progress')).toBe(true);
    expect(pendingIcon?.classList.contains('indeterminate')).toBe(true);
    expect(pendingIcon?.classList.contains('wavy')).toBe(true);

    // Nothing was applied optimistically; the row is disabled while pending.
    // (`aria-checked` mirrors the model, unlike the native checkbox property
    // that the click already flipped locally.)
    expect(toggle('markdown')?.getAttribute('aria-checked')).toBe('true');
    expect(toggle('markdown')?.disabled).toBe(true);

    socket.resolveStateWrite();
    await tick(30);

    expect(toggle('markdown')?.getAttribute('aria-checked')).toBe('false');
    expect(toggle('markdown')?.checked).toBe(false);
    expect(target.querySelector('[data-testid="ext-pending-markdown"]')).toBeNull();
    expect(extensionTabRoot()?.classList.contains('extensions-offline')).toBe(false);
  });

  // spec: Issue #145 reorder は優先度の変わった行だけを ack 後に反映する
  it('reorders only after both priority writes are acknowledged', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    await mountAndOpen(socket);
    expect(renderedOrder()[0]).toBe('ext-name-alhalte-dsl');

    target.querySelector<HTMLButtonElement>('[data-testid="move-down-btn-alhalte-dsl"]')?.click();
    await tick(20);

    // A one-step move touches exactly the two neighbours; the first write is in
    // flight and the second is only sent after its ack.
    expect(socket.requestUpdateStateCalls).toHaveLength(1);
    const first = socket.requestUpdateStateCalls[0];
    expect(first.key).toMatch(/^extension::(alhalte-dsl|markdown)$/);
    expect(target.querySelector('[data-testid="ext-pending-markdown"]')).toBeTruthy();
    // Order is unchanged while the ack is outstanding.
    expect(renderedOrder()[0]).toBe('ext-name-alhalte-dsl');

    socket.resolveStateWrite();
    await waitFor(() => socket.requestUpdateStateCalls.length === 2);
    expect(socket.requestUpdateStateCalls).toHaveLength(2);
    const priorities = socket.requestUpdateStateCalls.map(
      (call) => (call.value as { priority: number }).priority,
    );
    expect(priorities.sort()).toEqual([0, 1]);

    socket.resolveStateWrite();
    await tick(30);

    expect(renderedOrder()[0]).toBe('ext-name-markdown');
    expect(renderedOrder()[1]).toBe('ext-name-alhalte-dsl');
    expect(target.querySelector('[data-testid="ext-pending-markdown"]')).toBeNull();
    expect(target.querySelector('[data-testid="ext-pending-alhalte-dsl"]')).toBeNull();
  });

  // spec: Issue #145 delete は requestDeleteState の ack 後にのみ行を消す
  it('deletes a row only after requestDeleteState is acknowledged', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    await mountAndOpen(socket);

    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="ext-more-btn-emoklore"]');
    expect(moreBtn).toBeTruthy();
    vi.spyOn(moreBtn!, 'getBoundingClientRect').mockReturnValue({
      top: 150,
      bottom: 186,
      left: 600,
      right: 636,
      width: 36,
      height: 36,
      x: 600,
      y: 150,
      toJSON: () => {},
    } as DOMRect);
    moreBtn?.click();
    await tick(20);
    target.querySelector<HTMLElement>('[data-testid="ext-delete-btn-emoklore"]')?.click();
    await tick(20);

    expect(socket.requestDeleteStateCalls).toEqual([
      { sceneId: EXTENSION_STATE_SCENE_ID, key: 'extension::emoklore' },
    ]);
    // Deleting the last priority does not renumber the survivors.
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    // The row stays visible (and pending) until the delete is acknowledged.
    expect(target.querySelector('[data-testid="extension-item-emoklore"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="ext-pending-emoklore"]')).toBeTruthy();

    socket.resolveStateWrite();
    await tick(30);

    expect(target.querySelector('[data-testid="extension-item-emoklore"]')).toBeNull();
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');
    expect(extensionTabRoot()?.classList.contains('extensions-offline')).toBe(false);
  });

  // spec: Issue #145 書き込み失敗時は一覧を変えずにオフラインへ遷移する
  it('keeps the list unchanged and flips offline when a state write times out', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    await mountAndOpen(socket);

    toggle('markdown')?.click();
    await tick(20);
    expect(socket.requestUpdateStateCalls).toHaveLength(1);

    socket.rejectStateWrite(new OperationTimeoutError('updateState', 8000));
    await tick(40);

    expect(extensionTabRoot()?.classList.contains('extensions-offline')).toBe(true);
    expect(extensionTabRoot()?.getAttribute('aria-disabled')).toBe('true');
    expect(offlineNotice()?.textContent).toContain('サーバーと通信できませんでした');
    // The local model was not updated.
    expect(toggle('markdown')?.getAttribute('aria-checked')).toBe('true');
    expect(target.querySelector('[data-testid="ext-pending-markdown"]')).toBeNull();
  });

  // spec: Issue #145 アップロードは検証→cellar 保存→state 更新の順で行い、wasm は state に載せない
  it('uploads a validated wasm to the cellar and publishes only its tin id', async () => {
    vi.mocked(uploadTin).mockResolvedValue({ tinId: WASM_TIN_ID });

    const socket = new FakeRoomSocket();
    socket.autoAnswerStateWrites = false;
    await mountAndOpen(socket);

    const input = target.querySelector<HTMLInputElement>('[data-testid="extension-file-input"]');
    expect(input).toBeTruthy();
    const file = new File([markdownWasm], 'custom-ext.wasm', {
      type: 'application/wasm',
    });
    Object.defineProperty(input, 'files', { configurable: true, value: [file] });
    input!.dispatchEvent(new Event('change', { bubbles: true }));

    await waitFor(() => vi.mocked(uploadTin).mock.calls.length > 0);
    expect(vi.mocked(uploadTin)).toHaveBeenCalledTimes(1);

    // The wasm bytes were encrypted with the room key, not written to the state.
    const [wireBytes] = vi.mocked(uploadTin).mock.calls[0];
    expect(wireBytes).toBeInstanceOf(Uint8Array);
    const uploadOptions = vi.mocked(uploadTin).mock.calls[0][1];
    expect(uploadOptions?.tinId).toMatch(/^[0-9a-f]{64}$/);
    expect(typeof uploadOptions?.onProgress).toBe('function');

    await waitFor(() => socket.requestUpdateStateCalls.length === 1);
    const write = socket.requestUpdateStateCalls[0];
    expect(write.sceneId).toBe(EXTENSION_STATE_SCENE_ID);
    expect(write.key.startsWith(EXTENSION_STATE_KEY_PREFIX)).toBe(true);
    const value = write.value as Record<string, unknown>;
    expect(value.id).toBe(write.key.slice(EXTENSION_STATE_KEY_PREFIX.length));
    expect(value.tinId).toBe(WASM_TIN_ID);
    // Metadata comes from `self_introduction()` of the validated wasm.
    expect(value.name).toBe('Markdown スタイル');
    expect(typeof value.iconSvg).toBe('string');
    expect(typeof value.description).toBe('string');
    // 4 KiB limit: the metadata never carries the wasm bytes.
    expect('wasmBytes' in value).toBe(false);
    expect(JSON.stringify(value).length).toBeLessThan(4096);
    expect(JSON.stringify(value)).not.toContain(markdownWasm.toString('base64').slice(0, 32));

    // The progress notice is in its "registering" phase while the state write
    // is held back; the row does not exist yet.
    expect(target.querySelector('[data-testid="extension-upload-progress"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="extension-upload-pending"]')).toBeTruthy();
    const extensionId = String(value.id);
    expect(target.querySelector(`[data-testid="extension-item-${extensionId}"]`)).toBeNull();

    socket.resolveStateWrite();
    await tick(40);

    expect(target.querySelector(`[data-testid="extension-item-${extensionId}"]`)).toBeTruthy();
    expect(extName(extensionId)).toBe('Markdown スタイル');
    expect(target.querySelector('[data-testid="extension-upload-progress"]')).toBeNull();
    expect(extensionTabRoot()?.classList.contains('extensions-offline')).toBe(false);
  });

  // spec: Issue #145 サーバーに tinId だけがある拡張は wasm を cellar から復元する
  it('downloads the wasm of a remote extension from the cellar and renders it', async () => {
    vi.mocked(downloadTin).mockResolvedValue({
      tinId: WASM_TIN_ID,
      label: EXTENSION_WASM_LABEL,
      payload: new Uint8Array(markdownWasm),
      opener: { encryption: 2, compression: 0 },
    });

    const socket = new FakeRoomSocket();
    socket.stateSnapshot = seededState({
      'extension::remote-ext': {
        id: 'remote-ext',
        name: 'Remote Extension',
        priority: 0,
        enabled: true,
        tinId: WASM_TIN_ID,
      },
    });
    await mountAndOpen(socket);

    await waitFor(() => vi.mocked(downloadTin).mock.calls.length > 0);
    expect(vi.mocked(downloadTin)).toHaveBeenCalledWith(WASM_TIN_ID, ROOM_KEY, undefined);
    await waitFor(() => extName('remote-ext') !== null);

    expect(extName('remote-ext')).toBe('Remote Extension');
    // The official set seeded into the room is also listed by the server.
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');
    expect(extensionTabRoot()?.classList.contains('extensions-offline')).toBe(false);
  });

  // spec: Issue #165 marker が無い新規 room では公式既定を一度だけ seed し marker を保存する
  it('seeds the official defaults once and writes the marker in a new room', async () => {
    const socket = new FakeRoomSocket();
    socket.applyStateWritesToSnapshot = true;
    socket.stateSnapshot = {};
    await mountAndOpen(socket, { seeded: false });

    await waitFor(() => socket.stateSnapshot[EXTENSION_SEEDED_STATE_KEY] === true);
    await tick(20);

    const seedKeys = socket.requestUpdateStateCalls
      .map((call) => call.key)
      .filter((key) => key !== EXTENSION_SEEDED_STATE_KEY);
    expect([...seedKeys].sort()).toEqual(
      DEFAULT_OFFICIAL_EXTENSIONS.map((ext) => extensionStateKey(ext.id)).sort(),
    );
    // The marker is written last and only after every metadata write.
    expect(
      socket.requestUpdateStateCalls[socket.requestUpdateStateCalls.length - 1],
    ).toEqual({
      sceneId: EXTENSION_STATE_SCENE_ID,
      key: EXTENSION_SEEDED_STATE_KEY,
      value: EXTENSION_SEEDED_MARKER_VALUE,
    });
    expect(socket.stateSnapshot[EXTENSION_SEEDED_STATE_KEY]).toBe(true);
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');
    expect(extName('markdown')).toBe('Markdown スタイル');
    // Seed values are metadata only: the wasm body never enters the state.
    for (const call of socket.requestUpdateStateCalls) {
      if (call.key === EXTENSION_SEEDED_STATE_KEY) continue;
      const value = call.value as Record<string, unknown>;
      expect(value.id).toBe(call.key.slice(EXTENSION_STATE_KEY_PREFIX.length));
      expect('wasmBytes' in value).toBe(false);
      expect(JSON.stringify(value).length).toBeLessThan(4096);
    }

    // Reload: the marker is present, so nothing is seeded a second time.
    const writesAfterSeed = socket.requestUpdateStateCalls.length;
    await reload(socket);
    expect(socket.requestUpdateStateCalls).toHaveLength(writesAfterSeed);
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');
  });

  // spec: Issue #165 公式拡張を削除 → 再入室しても復活しない
  it('keeps a deleted official extension deleted after reload', async () => {
    const socket = new FakeRoomSocket();
    socket.applyStateWritesToSnapshot = true;
    socket.stateSnapshot = {};
    await mountAndOpen(socket, { seeded: false });
    await waitFor(() => socket.stateSnapshot[EXTENSION_SEEDED_STATE_KEY] === true);
    await tick(20);
    expect(extName('markdown')).toBe('Markdown スタイル');

    await deleteViaMenu('markdown');
    await waitFor(
      () => target.querySelector('[data-testid="extension-item-markdown"]') === null,
    );
    expect(socket.requestDeleteStateCalls).toContainEqual({
      sceneId: EXTENSION_STATE_SCENE_ID,
      key: extensionStateKey('markdown'),
    });
    expect(socket.stateSnapshot[extensionStateKey('markdown')]).toBeUndefined();

    const writesBeforeReload = socket.requestUpdateStateCalls.length;
    await reload(socket);

    // The server state (marker + remaining metadata) is authoritative: the
    // deleted official extension is not re-added and not seeded again.
    expect(extName('markdown')).toBeUndefined();
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');
    expect(socket.requestUpdateStateCalls).toHaveLength(writesBeforeReload);
    expect(socket.stateSnapshot[extensionStateKey('markdown')]).toBeUndefined();
  });

  // spec: Issue #165 marker が無い既存 room は移行として一度だけ seed する
  it('migrates an existing room without the marker by seeding once without overwriting metadata', async () => {
    const socket = new FakeRoomSocket();
    socket.applyStateWritesToSnapshot = true;
    socket.stateSnapshot = {
      'extension::markdown': {
        id: 'markdown',
        name: 'Markdown 改',
        priority: 0,
        enabled: false,
      },
    };
    await mountAndOpen(socket, { seeded: false });
    await waitFor(() => socket.stateSnapshot[EXTENSION_SEEDED_STATE_KEY] === true);
    await tick(20);

    // The existing server metadata wins; only the missing official ids are seeded.
    expect(
      socket.requestUpdateStateCalls.some(
        (call) => call.key === extensionStateKey('markdown'),
      ),
    ).toBe(false);
    expect(
      socket.requestUpdateStateCalls.some(
        (call) => call.key === extensionStateKey('alhalte-dsl'),
      ),
    ).toBe(true);
    expect(extName('markdown')).toBe('Markdown 改');
    expect(toggle('markdown')?.getAttribute('aria-checked')).toBe('false');

    const writesAfterMigration = socket.requestUpdateStateCalls.length;
    await reload(socket);
    expect(socket.requestUpdateStateCalls).toHaveLength(writesAfterMigration);
    expect(extName('markdown')).toBe('Markdown 改');
  });

  // spec: Issue #165 marker が無い room を seed できるのは GM だけ
  it('does not seed an unseeded room for non-admins', async () => {
    const socket = new FakeRoomSocket();
    socket.stateSnapshot = {};
    mountApp(socket, 'connected', [], false);
    await tick(30);
    openExtensionsTab();
    await tick(40);

    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(socket.stateSnapshot[EXTENSION_SEEDED_STATE_KEY]).toBeUndefined();
    // The read-only local official set is still visible until a GM seeds.
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');
    expect(target.querySelector('[data-testid="lock-indicator"]')).toBeTruthy();
  });

  // spec: Issue #165 wasm 欠落を UI で可視化し、有効化を無効にする
  it('marks an extension whose wasm cannot be downloaded as unavailable', async () => {
    // A tin id the shared wasm cache never saw, so the rejected cellar download
    // is the only source and the row really ends up without wasm.
    const brokenTinId = 'c'.repeat(64);
    vi.mocked(downloadTin).mockRejectedValue(new Error('cellar unavailable'));

    const socket = new FakeRoomSocket();
    socket.stateSnapshot = seededState({
      'extension::broken-ext': {
        id: 'broken-ext',
        name: 'Broken Extension',
        priority: 0,
        enabled: true,
        tinId: brokenTinId,
      },
    });
    await mountAndOpen(socket);

    await waitFor(
      () => target.querySelector('[data-testid="ext-wasm-missing-broken-ext"]') !== null,
    );
    const missing = target.querySelector<HTMLElement>(
      '[data-testid="ext-wasm-missing-broken-ext"]',
    );
    expect(missing?.textContent?.trim()).toBe('wasm がありません');
    expect(
      target
        .querySelector('[data-testid="extension-item-broken-ext"]')
        ?.getAttribute('data-wasm-missing'),
    ).toBe('true');
    // A metadata-only row cannot be enabled, but it stays deletable.
    expect(toggle('broken-ext')?.disabled).toBe(true);
    expect(
      target.querySelector<HTMLButtonElement>(
        '[data-testid="ext-more-btn-broken-ext"]',
      )?.disabled,
    ).toBe(false);
    // The rest of the list is unaffected.
    expect(extName('alhalte-dsl')).toBe('Alhalte DSL');
    expect(extensionTabRoot()?.classList.contains('extensions-offline')).toBe(false);
  });

  // spec: Issue #165 executePipeline は wasm の無い有効拡張を黙って skip せず警告を残す
  it('shows a pipeline warning instead of silently skipping an enabled extension without wasm', async () => {
    const socket = new FakeRoomSocket();
    mountApp(socket, 'connected', [
      { id: 'markdown', name: 'Markdown スタイル', enabled: true, priority: 0 },
    ]);
    await tick(30);

    const input = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(input).toBeTruthy();
    input!.value = '**こんにちは**';
    input!.dispatchEvent(new Event('input', { bubbles: true }));
    await tick(20);
    target.querySelector<HTMLElement>('[data-testid="send-message-button"]')?.click();
    await tick(40);

    const warning = target.querySelector<HTMLElement>('[data-testid="pipeline-warning"]');
    expect(warning).toBeTruthy();
    expect(warning?.textContent).toContain('Markdown スタイル');
    expect(warning?.textContent).toContain('wasm がありません');
    // The message is still sent with the raw/ext-io Tin (warning, not a block).
    expect(socket.requestAppendLogCalls).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// Pure helpers in utils/extensions.ts
// ---------------------------------------------------------------------------

describe('utils/extensions merge & cellar helpers (Issue #109 / #145)', () => {
  const LOCAL: ExtensionItem[] = [
    {
      id: 'alhalte-dsl',
      name: 'Alhalte DSL',
      enabled: true,
      priority: 0,
      wasmBytes: new Uint8Array([8]),
    },
    {
      id: 'markdown',
      name: 'Markdown スタイル',
      iconSvg: '<svg/>',
      description: 'Markdown',
      enabled: true,
      priority: 1,
      wasmBytes: new Uint8Array([1, 2, 3]),
    },
    {
      id: 'ext-custom',
      name: 'Custom',
      enabled: false,
      priority: 9,
      wasmBytes: new Uint8Array([4, 5, 6]),
    },
  ];

  beforeEach(() => {
    vi.mocked(uploadTin).mockReset();
    vi.mocked(downloadTin).mockReset();
  });

  // spec: Issue #145 state key / metadata schema
  it('projects ExtensionItem onto the tiny JSON metadata and back', () => {
    const item: ExtensionItem = {
      id: 'ext-1',
      name: 'Ext 1',
      description: 'desc',
      iconSvg: '<svg/>',
      enabled: true,
      priority: 3,
      wasmBytes: new Uint8Array([9, 9]),
      cellarTinId: WASM_TIN_ID,
    };

    const metadata = extensionToMetadata(item);
    expect(metadata).toEqual({
      id: 'ext-1',
      name: 'Ext 1',
      description: 'desc',
      iconSvg: '<svg/>',
      priority: 3,
      enabled: true,
      tinId: WASM_TIN_ID,
    });
    expect('wasmBytes' in metadata).toBe(false);

    const restored = extensionFromMetadata('ext-1', metadata, item.wasmBytes);
    expect(restored).toEqual(item);
  });

  it('validates the state key prefix and rejects malformed metadata', () => {
    expect(isExtensionStateKey('extension::markdown')).toBe(true);
    expect(isExtensionStateKey('asset::markdown')).toBe(false);

    expect(extensionFromMetadata('fallback', { name: 'Only name' })).toMatchObject({
      id: 'fallback',
      name: 'Only name',
      priority: 0,
      enabled: true,
    });

    expect(() => extensionFromMetadata('bad', null)).toThrow(ExtensionStateError);
    expect(() => extensionFromMetadata('bad', [])).toThrow(ExtensionStateError);
  });

  // spec: Issue #165 marker がありサーバーが正のときは公式既定 id の欠落を削除扱いにする
  it('merges server metadata: remote authoritative after seeding, custom removed, wasm retained', () => {
    const merged = mergeExtensionState(LOCAL, {
      'extension::markdown': {
        id: 'markdown',
        name: 'Markdown 改',
        priority: 0,
        enabled: false,
      },
      'extension::aozora': {
        id: 'aozora',
        name: '青空文庫',
        priority: 2,
        enabled: true,
        tinId: WASM_TIN_ID,
      },
      [EXTENSION_SEEDED_STATE_KEY]: true,
    });

    // Remote metadata wins for listed ids.
    const markdown = merged.find((ext) => ext.id === 'markdown');
    expect(markdown?.name).toBe('Markdown 改');
    expect(markdown?.enabled).toBe(false);
    expect(markdown?.priority).toBe(0);
    // No new tin id => the locally loaded wasm survives.
    expect(markdown?.wasmBytes).toEqual(new Uint8Array([1, 2, 3]));

    // The official `aozora` is created from the remote entry (no local bytes).
    const aozora = merged.find((ext) => ext.id === 'aozora');
    expect(aozora?.name).toBe('青空文庫');
    expect(aozora?.cellarTinId).toBe(WASM_TIN_ID);
    expect(aozora?.wasmBytes).toBeUndefined();

    // Custom local ids absent remotely are removed.
    expect(merged.some((ext) => ext.id === 'ext-custom')).toBe(false);
    // After seeding the server is authoritative: an absent official default is
    // a deletion and is not re-added with its local wasm (Issue #165).
    expect(merged.some((ext) => ext.id === 'alhalte-dsl')).toBe(false);
    // The seed marker is bookkeeping, never an extension row.
    expect(merged.some((ext) => ext.id === '__seeded')).toBe(false);

    expect(merged.map((ext) => ext.id)).toEqual(['markdown', 'aozora']);
    expect(merged.map((ext) => ext.priority)).toEqual([0, 2]);
  });

  // spec: Issue #165 marker が無い legacy room だけ公式既定の欠落をローカル保持する
  it('keeps missing official defaults only for legacy unseeded rooms', () => {
    const merged = mergeExtensionState(
      LOCAL,
      {
        'extension::markdown': {
          id: 'markdown',
          name: 'Markdown 改',
          priority: 1,
          enabled: false,
        },
      },
      { keepMissingOfficialDefaults: true },
    );

    const dsl = merged.find((ext) => ext.id === 'alhalte-dsl');
    expect(dsl?.name).toBe('Alhalte DSL');
    expect(dsl?.wasmBytes).toEqual(new Uint8Array([8]));
    expect(merged.map((ext) => ext.id)).toEqual(['alhalte-dsl', 'markdown']);
    // Custom ids are still removed.
    expect(merged.some((ext) => ext.id === 'ext-custom')).toBe(false);
  });

  it('drops local wasm when the server tin id changed', () => {
    const otherTin = 'b'.repeat(64);
    const merged = mergeExtensionState(LOCAL, {
      'extension::markdown': {
        id: 'markdown',
        name: 'Markdown',
        priority: 1,
        enabled: true,
        tinId: otherTin,
      },
    });

    const markdown = merged.find((ext) => ext.id === 'markdown');
    expect(markdown?.cellarTinId).toBe(otherTin);
    expect(markdown?.wasmBytes).toBeUndefined();

    // `ext-custom` is not official and not listed remotely; the official dsl is
    // absent remotely and treated as deleted.
    expect(merged.map((ext) => ext.id)).not.toContain('ext-custom');
    expect(merged.map((ext) => ext.id)).not.toContain('alhalte-dsl');
  });

  // spec: Issue #165 seed marker の判定と seed 内容
  it('detects the seed marker and computes the one-time seed for missing official defaults', () => {
    expect(isExtensionStateSeeded({ [EXTENSION_SEEDED_STATE_KEY]: true })).toBe(true);
    expect(isExtensionStateSeeded({ [EXTENSION_SEEDED_STATE_KEY]: 'seeded' })).toBe(true);
    expect(isExtensionStateSeeded({ [EXTENSION_SEEDED_STATE_KEY]: false })).toBe(false);
    expect(isExtensionStateSeeded({ [EXTENSION_SEEDED_STATE_KEY]: '' })).toBe(false);
    expect(isExtensionStateSeeded({ 'extension::markdown': {} })).toBe(false);
    expect(isExtensionSeedMarkerKey(EXTENSION_SEEDED_STATE_KEY)).toBe(true);
    expect(isExtensionStateKey(EXTENSION_SEEDED_STATE_KEY)).toBe(true);

    // Nothing to seed for a fresh room: every official default is missing.
    expect(buildExtensionSeed(DEFAULT_OFFICIAL_EXTENSIONS, {}).map((seed) => seed.key)).toEqual(
      DEFAULT_OFFICIAL_EXTENSIONS.map((ext) => extensionStateKey(ext.id)),
    );

    // Existing entries are kept; custom local ids are not seeded.
    const seed = buildExtensionSeed(LOCAL, {
      'extension::markdown': { id: 'markdown', priority: 0, enabled: false },
    });
    expect(seed.map((entry) => entry.key)).toEqual([extensionStateKey('alhalte-dsl')]);
    expect(seed[0].value).toMatchObject({
      id: 'alhalte-dsl',
      name: 'Alhalte DSL',
      enabled: true,
      priority: 0,
    });
    expect('wasmBytes' in seed[0].value).toBe(false);
  });

  it('keeps a manifest wasm-missing flag while the server has no new tin id', () => {
    const local: ExtensionItem[] = [
      { id: 'markdown', name: 'Markdown', enabled: true, priority: 0, wasmMissing: true },
    ];
    const merged = mergeExtensionState(local, {
      'extension::markdown': { id: 'markdown', priority: 0, enabled: true },
    });
    expect(merged[0].wasmMissing).toBe(true);
  });

  it('downloads only the missing wasm bodies and flags a failed download', async () => {
    const loader = vi.fn(async (tinId: string, extensionId: string) => {
      if (extensionId === 'broken') return undefined;
      expect(tinId).toBe(WASM_TIN_ID);
      return new Uint8Array([7, 7, 7]);
    });

    const merged = await applyRemoteExtensions(
      [],
      {
        'extension::downloaded': {
          id: 'downloaded',
          name: 'Downloaded',
          priority: 0,
          enabled: true,
          tinId: WASM_TIN_ID,
        },
        'extension::broken': {
          id: 'broken',
          name: 'Broken',
          priority: 1,
          enabled: true,
          tinId: WASM_TIN_ID,
        },
      },
      loader,
    );

    expect(loader).toHaveBeenCalledTimes(2);
    expect(merged.find((ext) => ext.id === 'downloaded')?.wasmBytes).toEqual(
      new Uint8Array([7, 7, 7]),
    );
    expect(merged.find((ext) => ext.id === 'downloaded')?.wasmMissing).toBeUndefined();
    expect(merged.find((ext) => ext.id === 'broken')?.wasmBytes).toBeUndefined();
    expect(merged.find((ext) => ext.id === 'broken')?.name).toBe('Broken');
    // Issue #165: a failed download is surfaced instead of being skipped silently.
    expect(merged.find((ext) => ext.id === 'broken')?.wasmMissing).toBe(true);
  });

  it('restores a wasm body through the cache and falls back to the cellar on a miss', async () => {
    const cache = new MemoryPantryCacheAdapter();
    vi.mocked(downloadTin).mockResolvedValue({
      tinId: WASM_TIN_ID,
      label: EXTENSION_WASM_LABEL,
      payload: new Uint8Array([1, 2, 3, 4]),
      opener: { encryption: 2, compression: 0 },
    });

    const downloaded = await loadExtensionWasm(WASM_TIN_ID, ROOM_KEY, cache);
    expect(downloaded).toEqual(new Uint8Array([1, 2, 3, 4]));
    expect(vi.mocked(downloadTin)).toHaveBeenCalledTimes(1);

    // Second call is served from the cache: no extra download.
    const cached = await loadExtensionWasm(WASM_TIN_ID, ROOM_KEY, cache);
    expect(cached).toEqual(new Uint8Array([1, 2, 3, 4]));
    expect(vi.mocked(downloadTin)).toHaveBeenCalledTimes(1);

    // A tin carrying another label is rejected.
    vi.mocked(downloadTin).mockResolvedValueOnce({
      tinId: WASM_TIN_ID,
      label: BigInt(1),
      payload: new Uint8Array([9]),
      opener: { encryption: 2, compression: 0 },
    });
    await expect(
      loadExtensionWasm(WASM_TIN_ID, ROOM_KEY, new MemoryPantryCacheAdapter()),
    ).rejects.toThrow(ExtensionStateError);
  });

  it('rejects an invalid room key when uploading the wasm', async () => {
    await expect(
      uploadExtensionWasm(new Uint8Array([1]), new Uint8Array(16)),
    ).rejects.toThrow(ExtensionStateError);
  });
});
