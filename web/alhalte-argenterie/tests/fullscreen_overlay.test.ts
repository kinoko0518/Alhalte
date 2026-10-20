import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { clearAssets, addAsset } from '../src/utils/assets.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

describe('Fullscreen Warnings & Overlays Viewport Hoisting (Issue #60)', () => {
  const dummyRoom: VisitedRoom = {
    id: '0000000000000060',
    name: '全画面オーバーレイ検証部屋',
    url: 'https://alhalte.example/rooms/0000000000000060#secret',
    lastVisited: Date.now(),
    isAdmin: true,
  };

  const scene1: Scene = {
    id: 'scene-alpha',
    name: '始まりの広場',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: ['char-1'],
  };

  const scene2: Scene = {
    id: 'scene-beta',
    name: '深緑の森',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
  };

  const testChar: Character = {
    id: 'char-1',
    name: '調査員アルト',
    initiative: 12,
    description: 'オーバーレイ検証用キャラクター',
    reference: '',
    statuses: [{ label: 'HP', current: 20, max: 20 }],
    parameters: [{ label: 'STR', value: 10 }],
    chatPalette: '1D100<=70 【調査】',
    hideStatus: false,
    placedAsPiece: false,
  };

  beforeEach(() => {
    clearAssets();
  });

  // spec: alhalte-argenterie/fullscreen-overlay シーングラフ削除警告のビューポート最前面化と開閉 (Issue #60)
  it('renders SceneGraphTab deletion confirmation dialog at root viewport level outside control panel, covering full screen with fixed scrim, and supports outside click / cancel / confirm', async () => {
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
        initialMovements: [
          { characterId: 'char-1', characterName: '調査員アルト', toSceneId: 'scene-alpha', timestamp: 100 },
          { characterId: 'char-1', characterName: '調査員アルト', fromSceneId: 'scene-alpha', toSceneId: 'scene-beta', timestamp: 200 },
        ],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Switch to Scene Tab (graph tab)
    const graphTabBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-graph"]');
    expect(graphTabBtn).toBeTruthy();
    graphTabBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Target a scene node in the graph and open action buttons
    const nodeLabels = target.querySelectorAll<HTMLButtonElement>('[data-testid^="node-label-"]');
    const betaNodeLabel = Array.from(nodeLabels).find((el) => el.textContent?.includes('深緑の森'));
    expect(betaNodeLabel).toBeTruthy();
    betaNodeLabel?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    const deleteBtn = target.querySelector<HTMLButtonElement>('[data-testid^="node-delete-btn-"]');
    expect(deleteBtn).toBeTruthy();

    // Click delete -> opens root-level full-screen confirmation dialog
    deleteBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // 1. Viewport最前面化の検証: コントロールパネルおよびSide Sheetの内部ではなく、Appルート直下に存在すること
    const controlPanel = target.querySelector('[data-testid="control-panel"]');
    const sideSheet = target.querySelector('[data-testid="landscape-side-sheet"]');
    const rootOverlay = target.querySelector('[data-testid="root-delete-scene-dialog"]');

    expect(rootOverlay).toBeTruthy();
    expect(controlPanel).toBeTruthy();
    expect(controlPanel?.contains(rootOverlay)).toBe(false);
    expect(sideSheet?.contains(rootOverlay)).toBe(false);

    // 2. スタイル検証: position: fixed; inset: 0; z-index: 1000; と scrim
    expect(rootOverlay?.classList.contains('fullscreen-dialog-overlay')).toBe(true);
    expect(rootOverlay?.querySelector('[data-testid="graph-delete-dialog-title"]')?.textContent).toContain(
      'シーン「深緑の森」を削除しますか？',
    );

    // 3. スクリム外側クリックによるクローズ検証
    rootOverlay?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="root-delete-scene-dialog"]')).toBeNull();

    // 4. 再度開いて「キャンセル」ボタンによるクローズ検証
    betaNodeLabel?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLButtonElement>('[data-testid^="node-delete-btn-"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    const cancelBtn = target.querySelector<HTMLButtonElement>('[data-testid="graph-cancel-delete-btn"]');
    expect(cancelBtn).toBeTruthy();
    cancelBtn?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="root-delete-scene-dialog"]')).toBeNull();

    // 5. 再度開いて「削除する」確認操作によるシーン削除検証
    betaNodeLabel?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLButtonElement>('[data-testid^="node-delete-btn-"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    const confirmBtn = target.querySelector<HTMLButtonElement>('[data-testid="graph-confirm-delete-btn"]');
    expect(confirmBtn).toBeTruthy();
    confirmBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    expect(target.querySelector('[data-testid="root-delete-scene-dialog"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/fullscreen-overlay シーン設定タブの削除ダイアログのビューポート最前面化 (Issue #60)
  it('renders SceneSettingsTab scene deletion confirmation dialog at root viewport level outside control panel', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [scene1, scene2],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Switch to Scene Settings Tab
    const settingsTabBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]');
    settingsTabBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Click "シーンを削除"
    const deleteSceneBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]');
    expect(deleteSceneBtn).toBeTruthy();
    deleteSceneBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Verify it is hoisted to root level
    const controlPanel = target.querySelector('[data-testid="control-panel"]');
    const rootOverlay = target.querySelector('[data-testid="root-delete-scene-dialog"]');
    expect(rootOverlay).toBeTruthy();
    expect(controlPanel?.contains(rootOverlay)).toBe(false);

    // Cancel closes dialog
    const cancelBtn = target.querySelector<HTMLButtonElement>('[data-testid="graph-cancel-delete-btn"]');
    cancelBtn?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="root-delete-scene-dialog"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/fullscreen-overlay アセットピッカーのビューポート最前面化と全画面スクリム (Issue #60)
  it('renders AssetPickerModal at root viewport level outside control panel with scrim, closing on outside click and cancel', async () => {
    addAsset({
      name: 'bg-forest.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,AAA',
    });

    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [scene1],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Switch to Scene Settings Tab
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Click "画像選択" button
    const openPickerBtn = target.querySelector<HTMLButtonElement>('[data-testid="open-bg-picker-btn"]');
    expect(openPickerBtn).toBeTruthy();
    openPickerBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Verify AssetPickerModal is hoisted to root level outside control panel
    const controlPanel = target.querySelector('[data-testid="control-panel"]');
    const assetModal = target.querySelector('[data-testid="asset-picker-modal"]');
    const rootWrapper = target.querySelector('[data-testid="root-asset-picker-wrapper"]');

    expect(assetModal).toBeTruthy();
    expect(rootWrapper).toBeTruthy();
    expect(controlPanel?.contains(assetModal)).toBe(false);
    expect(assetModal?.classList.contains('asset-picker-overlay')).toBe(true);

    // Clicking scrim outside surface closes the modal
    assetModal?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeNull();

    // Reopen and test Cancel button
    openPickerBtn?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeTruthy();

    const cancelBtn = target.querySelector<HTMLButtonElement>('[data-testid="asset-cancel-btn"]');
    cancelBtn?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/fullscreen-overlay キャラクター削除警告のビューポート最前面化 (Issue #60)
  it('renders character deletion confirmation at root viewport level outside control panel, closing on outside click', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [scene1],
        initialCharacters: [testChar],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Switch to Characters Tab
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-characters"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Open その他 and click 削除 (Issue #228)
    target.querySelector<HTMLButtonElement>('[data-testid="more-char-btn-char-1"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    const deleteCharBtn = target.querySelector<HTMLElement>('[data-testid="delete-char-btn-char-1"]');
    expect(deleteCharBtn).toBeTruthy();
    deleteCharBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Verify root dialog is outside control panel
    const controlPanel = target.querySelector('[data-testid="control-panel"]');
    const rootCharDialog = target.querySelector('[data-testid="root-delete-char-dialog"]');
    expect(rootCharDialog).toBeTruthy();
    expect(controlPanel?.contains(rootCharDialog)).toBe(false);

    // Click outside to close
    rootCharDialog?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="root-delete-char-dialog"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/fullscreen-overlay 縦長画面（portrait）Bottom Sheetでのビューポート最前面化 (Issue #60)
  it('renders full-screen overlays at root viewport level in portrait layout without being trapped in Bottom Sheet', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [scene1, scene2],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Verify portrait layout & bottom sheet exist
    const bottomSheet = target.querySelector('[data-testid="portrait-bottom-sheet"]');
    expect(bottomSheet).toBeTruthy();

    // Switch to Scene Settings Tab inside Bottom Sheet
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Click "シーンを削除"
    target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Verify dialog is NOT inside bottomSheet
    const rootOverlay = target.querySelector('[data-testid="root-delete-scene-dialog"]');
    expect(rootOverlay).toBeTruthy();
    expect(bottomSheet?.contains(rootOverlay)).toBe(false);

    // Click scrim to close
    rootOverlay?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="root-delete-scene-dialog"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/fullscreen-overlay キャラクター編集モーダルのビューポート最前面化と全画面スクリム (Issue #60)
  it('renders CharacterEditModal at root viewport level outside control panel with dark scrim, closing on outside click and cancel', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [scene1],
        initialCharacters: [testChar],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Switch to Characters Tab
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-characters"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Click character item to edit
    const charCard = target.querySelector<HTMLElement>('[data-testid="character-card-char-1"]');
    expect(charCard).toBeTruthy();
    charCard?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Verify CharacterEditModal is hoisted to root level outside control panel and outside side sheet
    const controlPanel = target.querySelector('[data-testid="control-panel"]');
    const sideSheet = target.querySelector('[data-testid="landscape-side-sheet"]');
    const rootEditWrapper = target.querySelector('[data-testid="root-character-edit-wrapper"]');
    const editPanel = target.querySelector('[data-testid="character-edit-panel"]');

    expect(rootEditWrapper).toBeTruthy();
    expect(editPanel).toBeTruthy();
    expect(controlPanel?.contains(editPanel)).toBe(false);
    expect(sideSheet?.contains(editPanel)).toBe(false);

    // Verify dark scrim overlay style
    expect(editPanel?.classList.contains('character-edit-overlay')).toBe(true);

    // Verify Issue #129 enhancements: M3 Switches, Dividers, descriptions, and scroll container
    const hideStatusInput = editPanel?.querySelector<HTMLInputElement>('[data-testid="edit-hide-status-toggle"]');
    expect(hideStatusInput?.getAttribute('role')).toBe('switch');
    expect(hideStatusInput?.closest('.np-switch')).toBeTruthy();

    const narrativeInput = editPanel?.querySelector<HTMLInputElement>('[data-testid="edit-narrative-toggle"]');
    expect(narrativeInput?.getAttribute('role')).toBe('switch');
    expect(narrativeInput?.closest('.np-switch')).toBeTruthy();

    expect(editPanel?.querySelector('[data-testid="status-section-desc"]')?.textContent).toBe(
      '体力などのゲーム中に頻繁に更新される値です。',
    );
    expect(editPanel?.querySelector('[data-testid="param-section-desc"]')?.textContent).toBe(
      '筋力、知力などゲーム中めったに更新されない値です。',
    );
    expect(editPanel?.querySelectorAll('.np-divider').length).toBeGreaterThanOrEqual(6);
    expect(editPanel?.style.overflowY).toBe('auto');
    expect(editPanel?.querySelector<HTMLElement>('.edit-dialog-content')?.style.maxHeight).toBe('85vh');

    // Click outside on scrim to close
    editPanel?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeNull();

    // Reopen and test Cancel button
    charCard?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeTruthy();

    const cancelBtn = target.querySelector<HTMLButtonElement>('[data-testid="cancel-character-btn"]');
    cancelBtn?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/fullscreen-overlay キャラクター編集中のアセットピッカー重畳表示 (Issue #52, #60)
  it('opens AssetPickerModal on top of hoisted CharacterEditModal without dismissing edit modal', async () => {
    const avatarAsset = addAsset({
      id: 'hoisted-avatar-1',
      name: 'wizard.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,d2l6YXJk',
      mimeType: 'image/webp',
    });

    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [scene1],
        initialCharacters: [testChar],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Switch to Characters Tab
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-characters"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Open character edit
    target.querySelector<HTMLElement>('[data-testid="character-card-char-1"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Edit panel is open
    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeTruthy();

    // Click "画像選択" in CharacterEditModal
    target.querySelector<HTMLButtonElement>('[data-testid="open-char-image-picker-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Both CharacterEditModal and AssetPickerModal exist at root level
    const editPanel = target.querySelector('[data-testid="character-edit-panel"]');
    const assetModal = target.querySelector('[data-testid="asset-picker-modal"]');
    expect(editPanel).toBeTruthy();
    expect(assetModal).toBeTruthy();

    // Pick asset in modal
    target.querySelector<HTMLElement>(`[data-testid="asset-card-${avatarAsset.id}"]`)?.click();
    await new Promise((r) => setTimeout(r, 30));

    // AssetPickerModal closed, CharacterEditModal still open with updated image
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeNull();
    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeTruthy();

    const avatarImg = target.querySelector<HTMLImageElement>('[data-testid="edit-avatar-img"]');
    expect(avatarImg?.getAttribute('src')).toBe(avatarAsset.dataUrl);

    // Save character
    target.querySelector<HTMLButtonElement>('[data-testid="save-character-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/fullscreen-overlay 縦長画面（portrait）Bottom Sheetでのキャラクター編集のビューポート最前面化 (Issue #60)
  it('renders CharacterEditModal at root viewport level in portrait layout without being trapped in Bottom Sheet', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
        initialScenes: [scene1],
        initialCharacters: [testChar],
        isAdmin: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    const bottomSheet = target.querySelector('[data-testid="portrait-bottom-sheet"]');
    expect(bottomSheet).toBeTruthy();

    // Switch to Characters Tab inside Bottom Sheet
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-characters"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Open character edit
    target.querySelector<HTMLElement>('[data-testid="character-card-char-1"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    // Verify CharacterEditModal is NOT inside bottomSheet
    const editPanel = target.querySelector('[data-testid="character-edit-panel"]');
    expect(editPanel).toBeTruthy();
    expect(bottomSheet?.contains(editPanel)).toBe(false);

    // Click cancel to close
    target.querySelector<HTMLButtonElement>('[data-testid="cancel-character-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeNull();

    unmount(instance);
    target.remove();
  });
});
