import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { clearAssets } from '../src/utils/assets.js';
import { ChunkLogManager, parseLogRecord } from '../src/utils/log.js';
import { OperationTimeoutError, ServerOperationError } from '../src/utils/roomSocket.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

describe('Scene Action Buttons & Management Row (Issue #57)', () => {
  const dummyRoom: VisitedRoom = {
    id: 'room-action-btn-test',
    name: 'シーン管理テスト部屋',
    url: 'https://alhalte.example/rooms/room-action-btn-test#secret',
    lastVisited: Date.now(),
    isAdmin: true,
  };

  const scene1: Scene = {
    id: 'scene-1',
    name: '広場シーン',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: ['char-1'],
  };

  const scene2: Scene = {
    id: 'scene-2',
    name: 'ダンジョンシーン',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
  };

  const testChar: Character = {
    id: 'char-1',
    name: '冒険者',
    initiative: 10,
    description: 'テスト用キャラクター',
    reference: '',
    statuses: [{ label: 'HP', current: 20, max: 20 }],
    parameters: [{ label: 'STR', value: 12 }],
    chatPalette: '1D100<=70 【探索】',
    hideStatus: false,
    placedAsPiece: false,
  };

  beforeEach(() => {
    clearAssets();
  });

  // spec: alhalte-argenterie/scene-settings メンバー移動UIのインライン埋め込みと移動実行 (Issue #92)
  it('embeds "メンバーを移動" UI directly into SceneSettingsTab without modal overlay, and moves member on click', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let movedCharId: string | undefined;
    let movedTargetScene: string | undefined;

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene1,
        allScenes: [scene1, scene2],
        characters: [testChar],
        isAdmin: true,
        onMoveMember: (charId, targetScene) => {
          movedCharId = charId;
          movedTargetScene = targetScene;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Verify embedded "メンバーを移動" section and title exist inline in the tab
    const moveSection = target.querySelector('[data-testid="move-members-section"]');
    expect(moveSection).toBeTruthy();
    expect(target.querySelector('[data-testid="move-members-title"]')?.textContent).toContain('メンバーを移動');

    // Dialog overlay is NOT rendered or used
    expect(target.querySelector('[data-testid="move-members-overlay"]')).toBeNull();

    // Verify members list and member row are rendered directly in tab as M3 Segmented List
    const moveList = target.querySelector('[data-testid="members-move-list"]');
    expect(moveList).toBeTruthy();
    expect(moveList?.tagName.toLowerCase()).toBe('ul');
    expect(moveList?.classList.contains('members-segmented-list')).toBe(true);

    const moveRow = target.querySelector(`[data-testid="move-row-${testChar.id}"]`);
    expect(moveRow).toBeTruthy();
    expect(moveRow?.classList.contains('np-item')).toBe(true);
    expect(moveRow?.classList.contains('member-segmented-item')).toBe(true);

    // Verify M3 Menu toggle works
    const splitToggle = target.querySelector<HTMLButtonElement>(`[data-testid="member-split-toggle-${testChar.id}"]`);
    expect(splitToggle).toBeTruthy();
    expect(splitToggle?.classList.contains('np-icon-button')).toBe(true);
    splitToggle?.click();
    await new Promise((r) => setTimeout(r, 20));

    const menu = target.querySelector(`[data-testid="member-scene-menu-${testChar.id}"]`);
    expect(menu).toBeTruthy();

    // Select scene from Menu
    const sceneItem = target.querySelector<HTMLElement>(`[data-testid="member-scene-item-${testChar.id}-${scene2.id}"]`);
    expect(sceneItem).toBeTruthy();
    sceneItem?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Verify target scene is set
    const targetInput = target.querySelector<HTMLInputElement>(`[data-testid="member-target-input-${testChar.id}"]`);
    expect(targetInput).toBeTruthy();
    expect(targetInput?.value).toBe('ダンジョンシーン');

    const applyBtn = target.querySelector<HTMLButtonElement>(`[data-testid="apply-move-btn-${testChar.id}"]`);
    expect(applyBtn).toBeTruthy();
    expect(applyBtn?.classList.contains('np-button')).toBe(true);
    applyBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(movedCharId).toBe(testChar.id);
    expect(movedTargetScene).toBe('ダンジョンシーン');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/scene-settings 3つのスクエアMアクションボタン群の並び、初期比率、ホバー展開 (Issue #57)
  it('renders 3 Square M action buttons side-by-side with left-larger initial ratio and smooth hover expansion showing text labels', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scene1,
        allScenes: [scene1, scene2],
        characters: [testChar],
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // 1. Verify 3 action buttons row exists
    const actionsRow = target.querySelector<HTMLElement>('[data-testid="scene-actions-row"]');
    expect(actionsRow).toBeTruthy();

    const colCreate = target.querySelector<HTMLElement>('[data-testid="action-col-create"]');
    const colDeleteLogs = target.querySelector<HTMLElement>('[data-testid="action-col-delete-logs"]');
    const colDeleteScene = target.querySelector<HTMLElement>('[data-testid="action-col-delete-scene"]');

    expect(colCreate).toBeTruthy();
    expect(colDeleteLogs).toBeTruthy();
    expect(colDeleteScene).toBeTruthy();

    // 2. Verify all buttons are M3 Button with shape="square" and size="m"
    const createBtn = target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]');
    const deleteLogsBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-logs-btn"]');
    const deleteSceneBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]');

    expect(createBtn?.classList.contains('np-button')).toBe(true);
    expect(createBtn?.classList.contains('m')).toBe(true);
    expect(createBtn?.classList.contains('square')).toBe(true);

    expect(deleteLogsBtn?.classList.contains('np-button')).toBe(true);
    expect(deleteLogsBtn?.classList.contains('m')).toBe(true);
    expect(deleteLogsBtn?.classList.contains('square')).toBe(true);

    expect(deleteSceneBtn?.classList.contains('np-button')).toBe(true);
    expect(deleteSceneBtn?.classList.contains('m')).toBe(true);
    expect(deleteSceneBtn?.classList.contains('square')).toBe(true);

    // 3. Verify initial ratios: left is largest, middle is medium, right is smallest
    const flexCreate = parseFloat(colCreate!.style.flex);
    const flexLogs = parseFloat(colDeleteLogs!.style.flex);
    const flexScene = parseFloat(colDeleteScene!.style.flex);

    expect(flexCreate).toBeGreaterThan(flexLogs);
    expect(flexLogs).toBeGreaterThan(flexScene);

    // 4. Verify destructive styling on deletion buttons
    expect(deleteLogsBtn?.classList.contains('destructive-tonal-btn')).toBe(true);
    expect(deleteSceneBtn?.classList.contains('destructive-tonal-btn')).toBe(true);

    // 5. Verify label elements exist with action-btn-label class and expected text
    const labelCreate = target.querySelector('[data-testid="create-scene-label"]');
    const labelDeleteLogs = target.querySelector('[data-testid="delete-logs-label"]');
    const labelDeleteScene = target.querySelector('[data-testid="delete-scene-label"]');

    expect(labelCreate?.textContent).toContain('シーンを新規作成');
    expect(labelDeleteLogs?.textContent).toContain('ログを削除');
    expect(labelDeleteScene?.textContent).toContain('シーンを削除');

    expect(labelCreate?.classList.contains('action-btn-label')).toBe(true);
    expect(labelDeleteLogs?.classList.contains('action-btn-label')).toBe(true);
    expect(labelDeleteScene?.classList.contains('action-btn-label')).toBe(true);

    // 6. Test hover on "シーンを新規作成": expands own occupancy ratio
    colCreate?.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    expect(colCreate?.classList.contains('hovered')).toBe(true);
    expect(parseFloat(colCreate!.style.flex)).toBe(10);
    expect(parseFloat(colDeleteLogs!.style.flex)).toBeLessThan(flexLogs);
    expect(parseFloat(colDeleteScene!.style.flex)).toBeLessThan(flexScene);

    colCreate?.dispatchEvent(new MouseEvent('mouseleave', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
    expect(colCreate?.classList.contains('hovered')).toBe(false);

    // 7. Test hover on "シーンのログを削除": expands own occupancy ratio
    colDeleteLogs?.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    expect(colDeleteLogs?.classList.contains('hovered')).toBe(true);
    expect(parseFloat(colDeleteLogs!.style.flex)).toBe(10);
    expect(parseFloat(colCreate!.style.flex)).toBeLessThan(flexCreate);
    expect(parseFloat(colDeleteScene!.style.flex)).toBeLessThan(flexScene);

    colDeleteLogs?.dispatchEvent(new MouseEvent('mouseleave', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
    expect(colDeleteLogs?.classList.contains('hovered')).toBe(false);

    // 8. Test hover on "シーンを削除": expands own occupancy ratio
    colDeleteScene?.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    expect(colDeleteScene?.classList.contains('hovered')).toBe(true);
    expect(parseFloat(colDeleteScene!.style.flex)).toBe(10);
    expect(parseFloat(colCreate!.style.flex)).toBeLessThan(flexCreate);
    expect(parseFloat(colDeleteLogs!.style.flex)).toBeLessThan(flexLogs);

    colDeleteScene?.dispatchEvent(new MouseEvent('mouseleave', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
    expect(colDeleteScene?.classList.contains('hovered')).toBe(false);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/scene-settings 「シーンのログを削除」および「シーンを削除」押下時の全画面警告ダイアログ (Issue #57, Issue #60)
  it('triggers root fullscreen warning dialog for both "シーンのログを削除" and "シーンを削除" with viewport frontmost scrim', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [scene1, scene2],
        initialCharacters: [testChar],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Send a message so there is a log record for scene1
    const footerInput = target.querySelector<HTMLInputElement>('[data-testid="chat-input-field"]');
    const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
    expect(footerInput).toBeTruthy();
    expect(sendBtn).toBeTruthy();
    if (footerInput) {
      footerInput.value = '消去予定のテストメッセージ';
      footerInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    sendBtn?.click();
    await new Promise((r) => setTimeout(r, 40));

    // Switch to Scene Settings tab
    const settingsTabBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]');
    expect(settingsTabBtn).toBeTruthy();
    settingsTabBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // 1. Test "シーンのログを削除"
    const deleteLogsBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-logs-btn"]');
    expect(deleteLogsBtn).toBeTruthy();
    deleteLogsBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Viewport-level fullscreen dialog should appear at root outside control panel
    const rootLogsDialog = target.querySelector('[data-testid="root-delete-logs-dialog"]');
    expect(rootLogsDialog).toBeTruthy();
    expect(rootLogsDialog?.classList.contains('fullscreen-dialog-overlay')).toBe(true);

    const logsDialogTitle = target.querySelector('[data-testid="delete-logs-dialog-title"]');
    expect(logsDialogTitle?.textContent).toContain('シーン「広場シーン」のログを削除しますか？');

    // Cancel deletion
    const cancelLogsBtn = target.querySelector<HTMLButtonElement>('[data-testid="cancel-delete-logs-btn"]');
    expect(cancelLogsBtn).toBeTruthy();
    cancelLogsBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="root-delete-logs-dialog"]')).toBeNull();

    // Open again and confirm deletion
    deleteLogsBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    const confirmLogsBtn = target.querySelector<HTMLButtonElement>('[data-testid="confirm-delete-logs-button"]');
    expect(confirmLogsBtn).toBeTruthy();
    confirmLogsBtn?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="root-delete-logs-dialog"]')).toBeNull();

    // Verify logs were cleared by switching to LogTab
    const logTabBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-log"]');
    logTabBtn?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.textContent).not.toContain('消去予定のテストメッセージ');

    // 2. Switch back to Scene Settings tab to test "シーンを削除"
    settingsTabBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    const deleteSceneBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]');
    expect(deleteSceneBtn).toBeTruthy();
    deleteSceneBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    const rootSceneDialog = target.querySelector('[data-testid="root-delete-scene-dialog"]');
    expect(rootSceneDialog).toBeTruthy();
    expect(rootSceneDialog?.classList.contains('fullscreen-dialog-overlay')).toBe(true);

    const sceneDialogTitle = target.querySelector('[data-testid="delete-dialog-title"]');
    expect(sceneDialogTitle?.textContent).toContain('シーン「広場シーン」を削除しますか？');

    // Confirm scene deletion
    const confirmSceneBtn = target.querySelector<HTMLButtonElement>('[data-testid="graph-confirm-delete-btn"]');
    expect(confirmSceneBtn).toBeTruthy();
    confirmSceneBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    expect(target.querySelector('[data-testid="root-delete-scene-dialog"]')).toBeNull();

    unmount(instance);
    target.remove();
  });
});

/**
 * Scene management actions against a connected socket (Issue #109 / #143).
 *
 * The contract under test:
 * - the request frame is sent to banquette (`request*` API of `RoomSocket`),
 * - the pressed control is replaced by a wavy `CircularProgress` while pending,
 * - the local scenes / members / chunk cache change only after the ack,
 * - a timeout shows 「サーバーと通信できませんでした」 under the management
 *   controls and leaves the local state untouched (no optimistic update).
 */
describe('Scene management actions — server acknowledgement (Issue #109 / #143)', () => {
  const ERROR_MESSAGE = 'サーバーと通信できませんでした';

  function settle(ms = 20): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  async function waitFor(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
    const startedAt = Date.now();
    while (!predicate()) {
      if (Date.now() - startedAt > timeoutMs) return;
      await settle(10);
    }
  }

  function makeRoom(id = 'room-scene-action-143'): VisitedRoom {
    return {
      id,
      name: 'シーン管理ackテスト部屋',
      url: `https://alhalte.example/rooms/${id}#secret`,
      lastVisited: Date.now(),
      isAdmin: true,
    };
  }

  function makeScenes(): Scene[] {
    return [
      {
        id: 'scene-a',
        name: '広場',
        juices: createDefaultJuices(),
        viewTransform: { x: 0, y: 0, scale: 1 },
        members: ['42'],
      },
      {
        id: 'scene-b',
        name: '森',
        juices: createDefaultJuices(),
        viewTransform: { x: 0, y: 0, scale: 1 },
        members: [],
      },
    ];
  }

  function makeCharacter(): Character {
    return {
      id: '42',
      name: '旅人',
      initiative: 10,
      description: '移動テスト用キャラクター',
      reference: '',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
      placedAsPiece: false,
    };
  }

  function mountAppWithSocket(socket: FakeRoomSocket) {
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(App, {
      target,
      props: {
        initialRoom: makeRoom(),
        initialScenes: makeScenes(),
        initialCharacters: [makeCharacter()],
        isAdmin: true,
        roomSocket: asRoomSocket(socket),
      },
    });
    return { target, instance };
  }

  function openSettingsTab(target: HTMLElement): void {
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
  }

  function sceneNameValue(target: HTMLElement): string {
    return target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]')?.value ?? '';
  }

  function memberSceneText(target: HTMLElement): string {
    return (
      target.querySelector('[data-testid="member-current-scene-42"]')?.textContent ?? ''
    );
  }

  function openSceneSwitcher(target: HTMLElement): void {
    target.querySelector<HTMLInputElement>('[data-testid="scene-search-input"]')?.focus();
  }

  beforeEach(() => {
    clearAssets();
    localStorage.clear();
  });

  // spec: argenterie/scene-settings シーン新規作成はack後にのみ一覧と現在シーンを更新する (Issue #109 / #143)
  it('sends CreateScene, shows the wavy spinner while pending, and adds/selects the scene only after the ack', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    const { target, instance } = mountAppWithSocket(socket);
    await settle();
    openSettingsTab(target);
    await settle(30);

    const createBtn = target.querySelector<HTMLButtonElement>(
      '[data-testid="open-create-scene-btn"]',
    )!;
    createBtn.click();
    await settle();

    const nameInput = target.querySelector<HTMLInputElement>(
      '[data-testid="new-scene-name-input"]',
    )!;
    nameInput.value = '新シーン';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    target.querySelector<HTMLButtonElement>('[data-testid="submit-create-scene-btn"]')!.click();
    await settle(0);

    // The request reached banquette, the pressed icon became the wavy spinner.
    expect(socket.requestCreateSceneCalls).toHaveLength(1);
    expect(socket.requestCreateSceneCalls[0].inheritJuices).toBe(true);
    const newSceneId = socket.requestCreateSceneCalls[0].sceneId;
    expect(newSceneId.startsWith('scene-')).toBe(true);
    const pending = target.querySelector<HTMLElement>('[data-testid="create-action-pending"]');
    expect(pending).toBeTruthy();
    // M3 wavy indeterminate CircularProgress, not the non-wavy `loading` prop.
    expect(pending?.classList.contains('progress')).toBe(true);
    expect(pending?.classList.contains('indeterminate')).toBe(true);
    expect(pending?.classList.contains('wavy')).toBe(true);
    // The dialog closed immediately so the spinner is on the management icon.
    expect(target.querySelector('[data-testid="create-scene-overlay"]')).toBeNull();
    expect(createBtn.disabled).toBe(true);
    // No optimistic update: the current scene is still the previous one.
    expect(sceneNameValue(target)).toBe('広場');

    socket.resolveSceneAction('create');
    await settle();

    expect(target.querySelector('[data-testid="create-action-pending"]')).toBeNull();
    expect(createBtn.disabled).toBe(false);
    expect(sceneNameValue(target)).toBe('新シーン');
    expect(target.querySelector('[data-testid="scene-action-error"]')).toBeNull();

    // A stale SceneList that predates the `SceneCreated` ack must not drop it.
    socket.emitFrame({ SceneList: { scenes: ['scene-a', 'scene-b'] } });
    await settle();
    expect(sceneNameValue(target)).toBe('新シーン');

    // The list that includes the new id keeps it and its display name.
    socket.emitFrame({ SceneList: { scenes: ['scene-a', 'scene-b', newSceneId] } });
    await settle();
    expect(sceneNameValue(target)).toBe('新シーン');

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings ログ削除はrequestRewindLogのack後にのみキャッシュを消す (Issue #109 / #143)
  it('sends RewindLog for ログを削除 and clears the cached logs only after the ack', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    const { target, instance } = mountAppWithSocket(socket);
    await settle();

    // Render one log record in scene-a through the send + echo path.
    const inputEl = target.querySelector<HTMLInputElement>('[data-testid="chat-input-field"]')!;
    inputEl.value = '削除対象のメッセージ 143';
    inputEl.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    target.querySelector<HTMLElement>('[data-testid="send-message-button"]')!.click();
    await settle(0);
    socket.deliverEcho();
    await waitFor(() =>
      !!target.querySelector('[data-testid="messages-list"]')?.textContent?.includes('削除対象のメッセージ 143'),
    );
    expect(target.querySelector('[data-testid="messages-list"]')?.textContent).toContain(
      '削除対象のメッセージ 143',
    );

    openSettingsTab(target);
    await settle(30);

    target.querySelector<HTMLButtonElement>('[data-testid="delete-logs-btn"]')!.click();
    await settle(30);
    expect(target.querySelector('[data-testid="root-delete-logs-dialog"]')).toBeTruthy();

    target.querySelector<HTMLButtonElement>('[data-testid="confirm-delete-logs-button"]')!.click();
    await settle(0);

    expect(socket.requestRewindLogCalls).toEqual([
      { sceneId: 'scene-a', chunkIndex: 0, position: 0 },
    ]);
    expect(target.querySelector('[data-testid="delete-logs-action-pending"]')).toBeTruthy();
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-logs-btn"]')!.disabled,
    ).toBe(true);

    // Still cached while the ack is outstanding.
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-log"]')!.click();
    await settle(30);
    expect(target.querySelector('[data-testid="messages-list"]')?.textContent).toContain(
      '削除対象のメッセージ 143',
    );

    socket.resolveSceneAction('rewind');
    await waitFor(
      () =>
        !target
          .querySelector('[data-testid="messages-list"]')
          ?.textContent?.includes('削除対象のメッセージ 143'),
    );
    expect(target.querySelector('[data-testid="messages-list"]')?.textContent).not.toContain(
      '削除対象のメッセージ 143',
    );

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings シーン削除はSceneDeletedのack後にのみ一覧から除去する (Issue #109 / #143)
  it('sends DeleteScene for シーンを削除 and removes/selects only after the ack', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    const { target, instance } = mountAppWithSocket(socket);
    await settle();
    openSettingsTab(target);
    await settle(30);

    target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')!.click();
    await settle(30);
    expect(target.querySelector('[data-testid="root-delete-scene-dialog"]')).toBeTruthy();

    target.querySelector<HTMLButtonElement>('[data-testid="graph-confirm-delete-btn"]')!.click();
    await settle(0);

    // The overlay closes immediately, the spinner stays on the management icon.
    expect(target.querySelector('[data-testid="root-delete-scene-dialog"]')).toBeNull();
    expect(socket.requestDeleteSceneCalls).toEqual([{ sceneId: 'scene-a' }]);
    expect(target.querySelector('[data-testid="delete-scene-action-pending"]')).toBeTruthy();
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')!.disabled,
    ).toBe(true);
    expect(sceneNameValue(target)).toBe('広場');

    socket.resolveSceneAction('delete');
    await settle();

    expect(target.querySelector('[data-testid="delete-scene-action-pending"]')).toBeNull();
    // The removed scene was current, so the selection fell back to scene-b.
    expect(sceneNameValue(target)).toBe('森');
    expect(target.querySelector('[data-testid="scene-action-error"]')).toBeNull();

    openSceneSwitcher(target);
    await settle(30);
    expect(target.querySelector('[data-testid="scene-search-item-scene-a"]')).toBeNull();
    expect(target.querySelector('[data-testid="scene-search-item-scene-b"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings シーン削除の失敗はローカル状態を変えず中央寄せエラーを出す (Issue #109 / #143)
  it('shows the centered error and keeps the scene when シーンを削除 times out', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { target, instance } = mountAppWithSocket(socket);
    await settle();
    openSettingsTab(target);
    await settle(30);

    target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')!.click();
    await settle(30);
    target.querySelector<HTMLButtonElement>('[data-testid="graph-confirm-delete-btn"]')!.click();
    await settle(0);

    socket.rejectSceneAction(new OperationTimeoutError('deleteScene', 8000), 'delete');
    await settle();

    const error = target.querySelector<HTMLElement>('[data-testid="scene-action-error"]');
    expect(error).toBeTruthy();
    expect(error?.getAttribute('role')).toBe('status');
    expect(error?.textContent?.trim()).toBe(ERROR_MESSAGE);
    expect(target.querySelector('[data-testid="delete-scene-action-pending"]')).toBeNull();
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')!.disabled,
    ).toBe(false);
    expect(sceneNameValue(target)).toBe('広場');
    openSceneSwitcher(target);
    await settle(30);
    expect(target.querySelector('[data-testid="scene-search-item-scene-a"]')).toBeTruthy();

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings シーン新規作成の失敗はシーンを追加せず中央寄せエラーを出す (Issue #109 / #143)
  it('shows the centered error and adds no scene when シーンを新規作成 fails', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { target, instance } = mountAppWithSocket(socket);
    await settle();
    openSettingsTab(target);
    await settle(30);

    target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')!.click();
    await settle();
    const nameInput = target.querySelector<HTMLInputElement>(
      '[data-testid="new-scene-name-input"]',
    )!;
    nameInput.value = '失敗するシーン';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    target.querySelector<HTMLButtonElement>('[data-testid="submit-create-scene-btn"]')!.click();
    await settle(0);

    const requestedId = socket.requestCreateSceneCalls[0].sceneId;
    socket.rejectSceneAction(new ServerOperationError('CreateSceneFailed', 'boom'), 'create');
    await settle();

    expect(target.querySelector('[data-testid="scene-action-error"]')?.textContent?.trim()).toBe(
      ERROR_MESSAGE,
    );
    expect(target.querySelector('[data-testid="create-action-pending"]')).toBeNull();
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')!.disabled,
    ).toBe(false);
    // No local scene was added and the selection did not move.
    expect(sceneNameValue(target)).toBe('広場');
    openSceneSwitcher(target);
    await settle(30);
    expect(target.querySelector(`[data-testid="scene-search-item-${requestedId}"]`)).toBeNull();
    expect(target.querySelector('[data-testid="scene-search-item-scene-b"]')).toBeTruthy();

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings ログ削除の失敗はキャッシュを消さず中央寄せエラーを出す (Issue #109 / #143)
  it('shows the centered error and keeps the cached logs when ログを削除 fails', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { target, instance } = mountAppWithSocket(socket);
    await settle();

    const inputEl = target.querySelector<HTMLInputElement>('[data-testid="chat-input-field"]')!;
    inputEl.value = '残るはずのメッセージ 143';
    inputEl.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    target.querySelector<HTMLElement>('[data-testid="send-message-button"]')!.click();
    await settle(0);
    socket.deliverEcho();
    await waitFor(() =>
      !!target.querySelector('[data-testid="messages-list"]')?.textContent?.includes('残るはずのメッセージ 143'),
    );

    openSettingsTab(target);
    await settle(30);
    target.querySelector<HTMLButtonElement>('[data-testid="delete-logs-btn"]')!.click();
    await settle(30);
    target.querySelector<HTMLButtonElement>('[data-testid="confirm-delete-logs-button"]')!.click();
    await settle(0);

    socket.rejectSceneAction(new ServerOperationError('RewindLogFailed', 'boom'), 'rewind');
    await settle();

    expect(target.querySelector('[data-testid="scene-action-error"]')?.textContent?.trim()).toBe(
      ERROR_MESSAGE,
    );
    expect(target.querySelector('[data-testid="delete-logs-action-pending"]')).toBeNull();

    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-log"]')!.click();
    await settle(30);
    expect(target.querySelector('[data-testid="messages-list"]')?.textContent).toContain(
      '残るはずのメッセージ 143',
    );

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings メンバー移動はCharacterMovedのack後にのみscenes/membersを更新する (Issue #109 / #143)
  it('sends MoveCharacter for 移動, blocks double activation, and updates members only after the ack', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    const { target, instance } = mountAppWithSocket(socket);
    await settle();
    openSettingsTab(target);
    await settle(30);

    const targetInput = target.querySelector<HTMLInputElement>(
      '[data-testid="member-target-input-42"]',
    )!;
    expect(targetInput.value).toBe('広場');
    targetInput.value = '森';
    targetInput.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();

    const moveBtn = target.querySelector<HTMLButtonElement>('[data-testid="apply-move-btn-42"]')!;
    moveBtn.click();
    await settle(0);

    expect(socket.requestMoveCharacterCalls).toEqual([{ characterId: 42, toSceneId: 'scene-b' }]);
    expect(target.querySelector('[data-testid="move-action-pending-42"]')).toBeTruthy();
    expect(moveBtn.disabled).toBe(true);
    // No optimistic mutation yet.
    expect(memberSceneText(target)).toContain('現在: 広場');
    expect(socket.requestUpdateStateCalls).toHaveLength(0);

    // Duplicate activation while pending is a no-op.
    moveBtn.click();
    await settle(0);
    expect(socket.requestMoveCharacterCalls).toHaveLength(1);

    socket.resolveSceneAction('move');
    await settle();

    expect(target.querySelector('[data-testid="move-action-pending-42"]')).toBeNull();
    expect(memberSceneText(target)).toContain('現在: 森');
    expect(sceneNameValue(target)).toBe('森');
    // Scene membership is server-authoritative (`SceneInfo`); since Issue #160 no
    // `character::<id>::scene` state mirror is written after the move.
    expect(socket.requestUpdateStateCalls).toHaveLength(0);
    expect(target.querySelector('[data-testid="scene-action-error"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings メンバー移動の失敗は members を変えず中央寄せエラーを出す (Issue #109 / #143)
  it('shows the centered error and keeps the members when 移動 times out', async () => {
    const socket = new FakeRoomSocket();
    socket.autoAnswerSceneActions = false;
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { target, instance } = mountAppWithSocket(socket);
    await settle();
    openSettingsTab(target);
    await settle(30);

    const targetInput = target.querySelector<HTMLInputElement>(
      '[data-testid="member-target-input-42"]',
    )!;
    targetInput.value = '森';
    targetInput.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();

    target.querySelector<HTMLButtonElement>('[data-testid="apply-move-btn-42"]')!.click();
    await settle(0);

    socket.rejectSceneAction(new OperationTimeoutError('moveCharacter', 8000), 'move');
    await settle();

    expect(target.querySelector('[data-testid="scene-action-error"]')?.textContent?.trim()).toBe(
      ERROR_MESSAGE,
    );
    expect(target.querySelector('[data-testid="move-action-pending-42"]')).toBeNull();
    expect(memberSceneText(target)).toContain('現在: 広場');
    expect(sceneNameValue(target)).toBe('広場');
    // The state mirror is only sent after the move was acknowledged.
    expect(socket.requestUpdateStateCalls).toHaveLength(0);

    warn.mockRestore();
    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings タブ単体でも非同期コールバックのfalseを失敗として扱う (Issue #109 / #143)
  it('treats a callback returning false as a failure and shows the centered error (standalone tab)', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const scenes = makeScenes();
    let finishAction: (() => void) | null = null;
    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: scenes[0],
        allScenes: scenes,
        characters: [makeCharacter()],
        onDeleteScene: () =>
          new Promise<boolean>((resolve) => {
            finishAction = () => resolve(false);
          }),
      },
    });
    await settle();

    // Without `onRequestDeleteScene` the tab owns the confirmation dialog.
    target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')!.click();
    await settle();
    expect(target.querySelector('[data-testid="delete-confirm-dialog"]')).toBeTruthy();
    target.querySelector<HTMLButtonElement>('[data-testid="confirm-delete-button"]')!.click();
    await settle(0);

    // The tab mirrors the in-flight state locally while the callback runs.
    expect(target.querySelector('[data-testid="delete-scene-action-pending"]')).toBeTruthy();
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')!.disabled,
    ).toBe(true);

    finishAction!();
    await settle();

    expect(target.querySelector('[data-testid="delete-scene-action-pending"]')).toBeNull();
    const error = target.querySelector<HTMLElement>('[data-testid="scene-action-error"]');
    expect(error?.getAttribute('role')).toBe('status');
    expect(error?.textContent?.trim()).toBe(ERROR_MESSAGE);

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings SceneListは消えたシーンを除去し現在シーンを付け替える (Issue #109 / #143)
  it('removes scenes absent from SceneList, never removes "/", and re-points the current scene', async () => {
    const socket = new FakeRoomSocket();
    const { target, instance } = mountAppWithSocket(socket);
    await settle();
    openSettingsTab(target);
    await settle(30);

    // Select scene-b through the CommonHeader scene switcher.
    openSceneSwitcher(target);
    await settle(30);
    target.querySelector<HTMLElement>('[data-testid="scene-search-item-scene-b"]')?.click();
    await settle();
    expect(sceneNameValue(target)).toBe('森');

    // scene-b vanished server-side: the list is authoritative both ways.
    socket.emitFrame({ SceneList: { scenes: ['/', 'scene-a'] } });
    await settle();
    expect(sceneNameValue(target)).toBe('メインシーン');
    openSceneSwitcher(target);
    await settle(30);
    expect(target.querySelector('[data-testid="scene-search-item-scene-b"]')).toBeNull();
    expect(target.querySelector('[data-testid="scene-search-item-/"]')).toBeTruthy();

    // An empty list never wipes the room, and the root scene is never removed.
    socket.emitFrame({ SceneList: { scenes: [] } });
    await settle();
    expect(sceneNameValue(target)).toBe('メインシーン');
    socket.emitFrame({ SceneList: { scenes: ['scene-a'] } });
    await settle();
    openSceneSwitcher(target);
    await settle(30);
    expect(target.querySelector('[data-testid="scene-search-item-/"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="scene-search-item-scene-a"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: argenterie/scene-settings SceneDeletedブロードキャストも一覧から除去する (Issue #109 / #143)
  it('removes a scene announced by a SceneDeleted broadcast as a second safeguard', async () => {
    const socket = new FakeRoomSocket();
    const { target, instance } = mountAppWithSocket(socket);
    await settle();
    openSettingsTab(target);
    await settle(30);

    socket.emitFrame({ SceneDeleted: { scene_id: 'scene-b' } });
    await settle();

    openSceneSwitcher(target);
    await settle(30);
    expect(target.querySelector('[data-testid="scene-search-item-scene-b"]')).toBeNull();
    // The unrelated, still listed scene is untouched.
    expect(target.querySelector('[data-testid="scene-search-item-scene-a"]')).toBeTruthy();
    expect(sceneNameValue(target)).toBe('広場');

    unmount(instance);
    target.remove();
  });
});
