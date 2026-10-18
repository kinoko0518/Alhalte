/**
 * Integration tests for the character tab/editor server synchronization.
 *
 * Contract under test (Issue #109 / sub-issue #144):
 * - opening the character tab sends `ListCharacters` and shows a contained M3
 *   `LoadingIndicator` over the cached list,
 * - a timeout keeps the cached list and shows the wavy
 *   `OfflineQueueNotice kind="character"`,
 * - saving stands still until the server ack: the panel is inert (`aria-busy`,
 *   disabled `fieldset`) and the save icon is a wavy indeterminate
 *   `CircularProgress`; the panel closes only on success,
 * - a save timeout enqueues exactly one `kind: 'character'` outbox entry, closes
 *   the panel and flips the tab into its queueing state,
 * - an offline save goes straight to the outbox (never through the socket's
 *   in-memory queue, which would resend on reconnect),
 * - reconnecting replays queued character operations and reconciles the
 *   server-allocated id of a queued create,
 * - `PermissionDenied` keeps the panel open with an inline error,
 * - delete removes a row only after the ack (and queues the deletion offline).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内`, `### ローカル保存`
 * - GitHub Issue #109 / sub-issue #144
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import { createConnectionManager } from '../src/utils/connection.js';
import { getOutbox, resetOutbox } from '../src/utils/outbox.js';
import { OperationTimeoutError, ServerOperationError } from '../src/utils/roomSocket.js';
import { clearAssets } from '../src/utils/assets.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

const ROOM: VisitedRoom = {
  id: 'room-character-sync-144',
  name: 'キャラクター同期テスト部屋',
  url: 'https://alhalte.example/rooms/room-character-sync-144#key',
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

const HERO: Character = {
  id: '42',
  name: '勇者ロラン',
  initiative: 15,
  description: '前衛の騎士。',
  reference: '',
  statuses: [{ label: 'HP', current: 30, max: 30 }],
  parameters: [{ label: 'STR', value: 16 }],
  chatPalette: '1D100<=80 【近接攻撃】',
  hideStatus: false,
  isNarrative: false,
  placedAsPiece: false,
};

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

describe('Character tab/editor server synchronization (Issue #109 / #144)', () => {
  let target: HTMLDivElement;
  let app: Record<string, any> | null = null;

  beforeEach(() => {
    clearAssets();
    document.body.innerHTML = '';
    localStorage.clear();
    resetOutbox();
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
    resetOutbox();
  });

  function mountApp(
    socket: FakeRoomSocket,
    initialState: 'connected' | 'disconnected' = 'connected',
  ) {
    const connectionManager = createConnectionManager({ initialState });
    app = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [SCENE],
        initialCharacters: [{ ...HERO }],
        isAdmin: true,
        connectionManager,
        roomSocket: asRoomSocket(socket),
      },
    }) as unknown as Record<string, any>;
    return { instance: app, connectionManager };
  }

  function openCharactersTab(): void {
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-characters"]')?.click();
  }

  function card(characterId: string): HTMLElement | null {
    return target.querySelector<HTMLElement>(`[data-testid="character-card-${characterId}"]`);
  }

  function editPanel(): HTMLElement | null {
    return target.querySelector<HTMLElement>('[data-testid="character-edit-panel"]');
  }

  function saveButton(): HTMLButtonElement | null {
    return target.querySelector<HTMLButtonElement>('[data-testid="save-character-btn"]');
  }

  function cancelButton(): HTMLButtonElement | null {
    return target.querySelector<HTMLButtonElement>('[data-testid="cancel-character-btn"]');
  }

  function queueNotice(): HTMLElement | null {
    return target.querySelector<HTMLElement>('[data-testid="character-queue-notice"]');
  }

  function noticeCount(): string | null | undefined {
    return target
      .querySelector('[data-testid="character-queue-notice"] [data-testid="offline-queue-notice-count"]')
      ?.textContent?.trim();
  }

  async function openCharacterEditor(characterId: string): Promise<void> {
    card(characterId)?.click();
    await tick(30);
  }

  async function renameAndSave(name: string): Promise<void> {
    const nameInput = target.querySelector<HTMLInputElement>('[data-testid="edit-name-input"]');
    expect(nameInput).toBeTruthy();
    nameInput!.value = name;
    nameInput!.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    saveButton()?.click();
    await tick(0);
  }

  // spec: Issue #144 タブ表示時に ListCharacters を発行し、contained LoadingIndicator を出す
  it('queries ListCharacters when the tab opens and shows a contained LoadingIndicator over the cached list', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerCharacterQueries = false;
    mountApp(socket);
    await tick(30);

    openCharactersTab();
    await tick(30);

    expect(socket.queryListCharactersCalls).toHaveLength(1);

    // Contained M3 LoadingIndicator floating over the cached rows.
    const indicator = target.querySelector<HTMLElement>(
      '[data-testid="character-loading-indicator"]',
    );
    expect(indicator).toBeTruthy();
    expect(indicator?.classList.contains('np-loading-indicator')).toBe(true);
    expect(indicator?.classList.contains('contained')).toBe(true);
    const overlay = target.querySelector<HTMLElement>('[data-testid="character-loading-overlay"]');
    expect(overlay?.getAttribute('role')).toBe('status');
    expect(overlay?.getAttribute('aria-label')).toBe('キャラクターを読み込み中');

    // The cached list stays visible while the query is in flight.
    expect(card(HERO.id)).toBeTruthy();

    // The authoritative answer replaces (never duplicates) the cached rows.
    socket.resolveCharacterListQuery([
      { ...HERO, name: '勇者ロラン改' },
      { ...HERO, id: '43', name: '魔法使いミナ' },
    ]);
    await tick(30);

    expect(target.querySelector('[data-testid="character-loading-indicator"]')).toBeNull();
    expect(card(HERO.id)?.textContent).toContain('勇者ロラン改');
    expect(card('43')?.textContent).toContain('魔法使いミナ');
    expect(target.querySelectorAll('[data-testid^="character-card-"]')).toHaveLength(2);

    // An empty authoritative list also replaces the local rows (the previous
    // code kept the stale list when it was empty).
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-log"]')?.click();
    await tick(20);
    socket.autoAnswerCharacterQueries = true;
    socket.characterListSnapshot = [];
    openCharactersTab();
    await tick(30);
    expect(target.querySelectorAll('[data-testid^="character-card-"]')).toHaveLength(0);
  });

  // spec: Issue #144 タイムアウト時はキャッシュを保持したままキュー通知を出す
  it('keeps the cached list and shows the wavy character queue notice when the query times out', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerCharacterQueries = false;
    mountApp(socket);
    await tick(30);

    openCharactersTab();
    await tick(30);
    expect(target.querySelector('[data-testid="character-loading-indicator"]')).toBeTruthy();

    socket.rejectCharacterListQuery(new OperationTimeoutError('queryListCharacters', 8000));
    await tick(40);

    expect(target.querySelector('[data-testid="character-loading-indicator"]')).toBeNull();
    const notice = queueNotice();
    expect(notice).toBeTruthy();
    const aside = notice?.querySelector('[data-testid="offline-queue-notice"]');
    expect(aside?.getAttribute('data-kind')).toBe('character');
    expect(
      notice?.querySelector('[data-testid="offline-queue-notice-description"]')?.textContent,
    ).toContain('キャラクターは次回の接続時に更新されます');
    // Nothing was queued by a failed *query*, so the count is zero.
    expect(noticeCount()).toBe('0 件を蓄積中');

    // The cached list survived the failure.
    expect(card(HERO.id)).toBeTruthy();
    expect(card(HERO.id)?.textContent).toContain('勇者ロラン');
  });

  // spec: Issue #144 保存はackまで待ち、パネルは非活性・保存アイコンは波線CircularProgress・成功時のみ閉じる
  it('disables the panel, swaps the save icon for a wavy spinner and closes only after the create ack, applying the server id', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [{ ...HERO }];
    socket.autoAnswerCharacterRequests = false;
    mountApp(socket);
    await tick(30);

    openCharactersTab();
    await tick(30);
    expect(card(HERO.id)).toBeTruthy();

    // 新規作成 opens the hoisted editor with a draft; nothing is sent yet.
    target.querySelector<HTMLButtonElement>('[data-testid="create-character-btn"]')?.click();
    await tick(30);
    expect(editPanel()).toBeTruthy();
    expect(socket.requestCreateCharacterCalls).toHaveLength(0);
    expect(target.querySelectorAll('[data-testid^="character-card-"]')).toHaveLength(1);

    await renameAndSave('新規勇者リュウ');

    // The request reached banquette and the panel is inert while pending.
    expect(socket.requestCreateCharacterCalls).toHaveLength(1);
    expect(socket.requestCreateCharacterCalls[0].character.name).toBe('新規勇者リュウ');

    const panel = editPanel();
    expect(panel).toBeTruthy();
    expect(panel?.getAttribute('aria-busy')).toBe('true');

    const inert = panel?.querySelector<HTMLFieldSetElement>('.edit-dialog-content');
    expect(inert?.tagName.toLowerCase()).toBe('fieldset');
    expect(inert?.disabled).toBe(true);
    expect(saveButton()?.disabled).toBe(true);
    expect(cancelButton()?.disabled).toBe(true);

    // The save icon is a wavy indeterminate CircularProgress, not the plain one
    // (the Button `loading` slot is not wavy, so it must not be used).
    const progress = target.querySelector<HTMLElement>('[data-testid="character-save-progress"]');
    expect(progress).toBeTruthy();
    expect(progress?.classList.contains('progress')).toBe(true);
    expect(progress?.classList.contains('indeterminate')).toBe(true);
    expect(progress?.classList.contains('wavy')).toBe(true);

    // Still open: nothing was applied optimistically.
    expect(editPanel()).toBeTruthy();
    expect(target.querySelectorAll('[data-testid^="character-card-"]')).toHaveLength(1);

    const serverId = String(socket.requestCreateCharacterCalls[0].allocatedId);
    socket.resolveCharacterRequest('create');
    await tick(40);

    // Closed only after the ack; the draft row was replaced by the server id.
    expect(editPanel()).toBeNull();
    expect(card(serverId)).toBeTruthy();
    expect(card(serverId)?.textContent).toContain('新規勇者リュウ');
    expect(target.querySelectorAll('[data-testid^="character-card-"]')).toHaveLength(2);
    expect(queueNotice()).toBeNull();
    expect(await getOutbox().list(ROOM.id!, 'character')).toHaveLength(0);
  });

  // spec: Issue #144 接続中の更新はack後に一覧へ反映し、成功時のみ閉じる
  it('applies a connected update only after the ack and closes the panel on success', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [{ ...HERO }];
    socket.autoAnswerCharacterRequests = false;
    mountApp(socket);
    await tick(30);
    openCharactersTab();
    await tick(30);

    await openCharacterEditor(HERO.id);
    await renameAndSave('勇者ロラン改');

    expect(socket.requestUpdateCharacterCalls).toHaveLength(1);
    expect(socket.requestUpdateCharacterCalls[0].characterId).toBe(42);
    expect(socket.requestUpdateCharacterCalls[0].character.name).toBe('勇者ロラン改');
    // No optimistic update before the ack.
    expect(card(HERO.id)?.textContent).toContain('勇者ロラン');
    expect(editPanel()).toBeTruthy();

    socket.resolveCharacterRequest('update');
    await tick(40);

    expect(editPanel()).toBeNull();
    expect(card(HERO.id)?.textContent).toContain('勇者ロラン改');
    expect(await getOutbox().list(ROOM.id!, 'character')).toHaveLength(0);
  });

  // spec: Issue #144 タイムアウト時は outbox へ1件積み、閉じてキューイング状態へ遷移する
  it('queues exactly one outbox entry on save timeout, closes the panel and shows the queue notice', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [{ ...HERO }];
    socket.autoAnswerCharacterRequests = false;
    mountApp(socket);
    await tick(30);
    openCharactersTab();
    await tick(30);

    await openCharacterEditor(HERO.id);
    await renameAndSave('タイムアウトする勇者');
    expect(socket.requestUpdateCharacterCalls).toHaveLength(1);

    socket.rejectCharacterRequest(new OperationTimeoutError('updateCharacter', 8000));
    await waitFor(() => queueNotice() !== null);
    await tick(40);

    const queued = await getOutbox().list(ROOM.id!, 'character');
    expect(queued).toHaveLength(1);
    const payload = queued[0].payload as { operation?: string; character?: Character };
    expect(payload.operation).toBe('update');
    expect(payload.character?.name).toBe('タイムアウトする勇者');

    expect(editPanel()).toBeNull();
    expect(queueNotice()).toBeTruthy();
    expect(noticeCount()).toBe('1 件を蓄積中');
    // The local list keeps the last acknowledged value until the flush.
    expect(card(HERO.id)?.textContent).toContain('勇者ロラン');
  });

  // spec: Issue #144 オフライン保存は RoomSocket のメモリキューを使わず outbox へ直接積む
  it('queues an offline save directly in the outbox without touching the socket queues', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [{ ...HERO }];
    socket.isConnected = false;
    mountApp(socket, 'disconnected');
    await tick(30);
    openCharactersTab();
    await tick(30);

    await openCharacterEditor(HERO.id);
    await renameAndSave('オフラインで編集した勇者');
    await waitFor(() => queueNotice() !== null);
    await tick(40);

    // The socket was never asked to send anything: `RoomSocket.send` would keep
    // its own copy in the in-memory queue and resend it on reconnect.
    expect(socket.requestUpdateCharacterCalls).toHaveLength(0);
    expect(socket.requestCreateCharacterCalls).toHaveLength(0);

    const queued = await getOutbox().list(ROOM.id!, 'character');
    expect(queued).toHaveLength(1);
    const payload = queued[0].payload as { operation?: string; character?: Character };
    expect(payload.operation).toBe('update');
    expect(payload.character?.name).toBe('オフラインで編集した勇者');

    expect(editPanel()).toBeNull();
    expect(noticeCount()).toBe('1 件を蓄積中');
  });

  // spec: Issue #144 再接続時に create/update を flush し、サーバー採番 ID を反映してエントリを消す
  it('flushes queued character creates and updates on reconnect, reconciles the server id and drains the outbox', async () => {
    const draft: Character = {
      ...HERO,
      id: 'char-draft-queued',
      name: 'キューの新規キャラクター',
    };
    await getOutbox().enqueue({
      roomId: ROOM.id!,
      kind: 'character',
      createdAt: Date.now() - 2000,
      payload: { operation: 'create', character: draft },
    });
    await getOutbox().enqueue({
      roomId: ROOM.id!,
      kind: 'character',
      createdAt: Date.now() - 1000,
      payload: { operation: 'update', character: { ...HERO, name: 'キューの更新後勇者' } },
    });

    const socket = new FakeRoomSocket();
    socket.isConnected = false;
    socket.autoAnswerCharacterQueries = false;
    const { connectionManager } = mountApp(socket, 'disconnected');
    await tick(40);

    // Nothing is flushed while offline; the tab reports the queued count.
    expect(socket.requestCreateCharacterCalls).toHaveLength(0);
    openCharactersTab();
    await tick(30);
    await waitFor(() => noticeCount() === '2 件を蓄積中');
    expect(noticeCount()).toBe('2 件を蓄積中');

    // Reconnect: FIFO create then update.
    socket.isConnected = true;
    connectionManager.setState('connected');
    await waitFor(() => socket.requestCreateCharacterCalls.length > 0);
    await waitFor(() => socket.requestUpdateCharacterCalls.length > 0);
    await waitFor(() => queueNotice() === null);
    await tick(40);

    expect(socket.requestCreateCharacterCalls[0].character.name).toBe('キューの新規キャラクター');
    expect(socket.requestUpdateCharacterCalls[0].characterId).toBe(42);
    expect(socket.requestUpdateCharacterCalls[0].character.name).toBe('キューの更新後勇者');

    // The queued create was reconciled to the server-allocated id (1000).
    expect(card('1000')?.textContent).toContain('キューの新規キャラクター');
    expect(card('char-draft-queued')).toBeNull();
    expect(card(HERO.id)?.textContent).toContain('キューの更新後勇者');
    expect(await getOutbox().list(ROOM.id!, 'character')).toHaveLength(0);
    expect(queueNotice()).toBeNull();
  });

  // spec: Issue #144 PermissionDenied はパネルを開いたままエラーを表示する
  it('keeps the panel open and shows the error on PermissionDenied without queueing anything', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [{ ...HERO }];
    socket.autoAnswerCharacterRequests = false;
    mountApp(socket);
    await tick(30);
    openCharactersTab();
    await tick(30);

    await openCharacterEditor(HERO.id);
    await renameAndSave('権限がない勇者');
    expect(socket.requestUpdateCharacterCalls).toHaveLength(1);

    socket.rejectCharacterRequest(new ServerOperationError('PermissionDenied', 'denied'));
    await tick(60);

    const panel = editPanel();
    expect(panel).toBeTruthy();
    // No longer busy: the panel is interactive again.
    expect(panel?.getAttribute('aria-busy')).toBeNull();
    const error = target.querySelector<HTMLElement>('[data-testid="character-save-error"]');
    expect(error).toBeTruthy();
    expect(error?.getAttribute('role')).toBe('alert');
    expect(error?.textContent).toContain('権限');
    // The panel is interactive again for a retry / cancel.
    expect(saveButton()?.disabled).toBe(false);
    expect(cancelButton()?.disabled).toBe(false);
    // Nothing was queued and nothing changed locally.
    expect(await getOutbox().list(ROOM.id!, 'character')).toHaveLength(0);
    expect(card(HERO.id)?.textContent).toContain('勇者ロラン');
    expect(queueNotice()).toBeNull();
  });

  // spec: Issue #144 削除はack後に一覧から消し、オフライン時は delete をキューして行を残す
  it('removes a character only after the delete ack, and queues an offline deletion while keeping the row', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [{ ...HERO }];
    socket.autoAnswerCharacterRequests = false;
    mountApp(socket);
    await tick(30);
    openCharactersTab();
    await tick(30);

    target.querySelector<HTMLButtonElement>(`[data-testid="delete-char-btn-${HERO.id}"]`)?.click();
    await tick(30);
    target.querySelector<HTMLButtonElement>('[data-testid="confirm-char-delete-button"]')?.click();
    await tick(30);

    expect(socket.requestDeleteCharacterCalls).toHaveLength(1);
    expect(socket.requestDeleteCharacterCalls[0].characterId).toBe(42);
    // Not removed before the ack.
    expect(card(HERO.id)).toBeTruthy();

    socket.resolveCharacterRequest('delete');
    await tick(40);
    expect(card(HERO.id)).toBeNull();

    // Offline deletion: queued and the row is kept (the server still has it).
    socket.isConnected = false;
    target.querySelector<HTMLButtonElement>(`[data-testid="create-character-btn"]`)?.click();
    await tick(20);
    cancelButton()?.click();
    await tick(20);

    // Re-add the hero locally through a tab refresh so the delete button exists.
    socket.autoAnswerCharacterQueries = true;
    socket.characterListSnapshot = [{ ...HERO }];
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-log"]')?.click();
    await tick(20);
    openCharactersTab();
    await tick(30);
    expect(card(HERO.id)).toBeTruthy();

    target.querySelector<HTMLButtonElement>(`[data-testid="delete-char-btn-${HERO.id}"]`)?.click();
    await tick(20);
    target.querySelector<HTMLButtonElement>('[data-testid="confirm-char-delete-button"]')?.click();
    await tick(40);

    expect(socket.requestDeleteCharacterCalls).toHaveLength(1);
    const queued = await getOutbox().list(ROOM.id!, 'character');
    expect(queued).toHaveLength(1);
    expect((queued[0].payload as { operation?: string }).operation).toBe('delete');
    // The row stays until the flush acknowledges the deletion.
    expect(card(HERO.id)).toBeTruthy();
  });
});
