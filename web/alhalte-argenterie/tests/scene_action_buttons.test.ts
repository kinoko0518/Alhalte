import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { clearAssets } from '../src/utils/assets.js';
import { ChunkLogManager, parseLogRecord } from '../src/utils/log.js';
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

  // spec: alhalte-argenterie/scene-settings メンバー移動ボタン M3 Button size="m" とダイアログ展開 (Issue #57)
  it('renders "メンバーを移動" button as M3 Button with size="m", and opens move members dialog on click', async () => {
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

    // Verify "メンバーを移動" button exists and is an M3 Button with size="m"
    const moveBtn = target.querySelector<HTMLButtonElement>('[data-testid="open-move-members-btn"]');
    expect(moveBtn).toBeTruthy();
    expect(moveBtn?.classList.contains('np-button')).toBe(true);
    expect(moveBtn?.classList.contains('m')).toBe(true);
    expect(moveBtn?.textContent).toContain('メンバーを移動');

    // Dialog is not open initially
    expect(target.querySelector('[data-testid="move-members-overlay"]')).toBeNull();

    // Click button to open dialog
    moveBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const moveOverlay = target.querySelector('[data-testid="move-members-overlay"]');
    expect(moveOverlay).toBeTruthy();
    expect(target.querySelector('[data-testid="move-members-title"]')?.textContent).toContain('メンバーを移動');

    // Change target scene and click apply
    const targetInput = target.querySelector<HTMLInputElement>(`[data-testid="member-target-input-${testChar.id}"]`);
    expect(targetInput).toBeTruthy();
    if (targetInput) {
      targetInput.value = 'ダンジョンシーン';
      targetInput.dispatchEvent(new Event('input', { bubbles: true }));
    }

    const applyBtn = target.querySelector<HTMLButtonElement>(`[data-testid="apply-move-btn-${testChar.id}"]`);
    expect(applyBtn).toBeTruthy();
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
