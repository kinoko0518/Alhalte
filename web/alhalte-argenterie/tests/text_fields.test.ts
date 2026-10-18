import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import CommonFooter from '../src/components/Control/CommonFooter.svelte';
import HomeScreen from '../src/components/Home/HomeScreen.svelte';
import CharacterManagementTab from '../src/components/Control/tabs/CharacterManagementTab.svelte';
import SceneSettingsTab from '../src/components/Control/tabs/SceneSettingsTab.svelte';
import AssetPickerModal from '../src/components/Control/AssetPickerModal.svelte';
import { clearAssets, addAsset } from '../src/utils/assets.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

describe('Material 3 Expressive Outlined TextField Migration (Issue #62)', () => {
  beforeEach(() => {
    localStorage.clear();
    clearAssets();
  });

  // spec: alhalte-argenterie/common-footer チャット入力欄 M3 Outlined TextField (textarea)
  it('renders M3 Outlined TextField textarea in CommonFooter with minLines, maxLines, and two-way binding', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testChar: Character = {
      id: 'char-1',
      name: '探索者',
      initiative: 10,
      description: '',
      statuses: [],
      parameters: [],
      chatPalette: '1D100<=50 【目星】',
      hideStatus: false,
    };

    let sentText: string | null = null;
    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: [testChar],
        selectedCharacterId: 'char-1',
        onSendMessage: (msg) => {
          sentText = msg;
        },
      },
    });

    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea).toBeTruthy();
    expect(textarea?.tagName.toLowerCase()).toBe('textarea');

    // Verify it is wrapped in an M3 Outlined Field
    const npField = textarea?.closest('.np-field');
    expect(npField).toBeTruthy();
    expect(npField?.classList.contains('np-field-outlined')).toBe(true);

    // Verify typing updates message input and Enter sends message
    if (textarea) {
      textarea.value = 'こんにちは、M3 TextField!';
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    // Press Enter to submit
    textarea?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
    expect(sentText).toBe('こんにちは、M3 TextField!');
    expect(textarea?.value).toBe('');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/home HomeScreen ルーム名・ニックネーム・部屋URL M3 Outlined TextField
  it('renders M3 Outlined TextFields in HomeScreen creation dialog with label and readonly state', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let enteredRoom: VisitedRoom | null = null;
    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        onEnterRoom: (r) => {
          enteredRoom = r;
        },
      },
    });

    // Open creation dialog
    target.querySelector<HTMLButtonElement>('[data-testid="create-room-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    const roomNameInput = target.querySelector<HTMLInputElement>('[data-testid="room-name-input"]');
    const nicknameInput = target.querySelector<HTMLInputElement>('[data-testid="user-nickname-input"]');
    const roomUrlInput = target.querySelector<HTMLInputElement>('[data-testid="room-url-input"]');

    expect(roomNameInput).toBeTruthy();
    // Issue #94: 部屋作成時のニックネーム不要化
    expect(nicknameInput).toBeNull();
    expect(roomUrlInput).toBeTruthy();

    // Verify all are M3 Outlined TextFields
    expect(roomNameInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(roomUrlInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);

    // Verify roomUrlInput is readonly
    expect(roomUrlInput?.readOnly).toBe(true);
    expect(roomUrlInput?.value).toContain('https://alhalte.example/rooms/');

    // Type room name
    if (roomNameInput) {
      roomNameInput.value = 'アーカム図書館';
      roomNameInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    if (nicknameInput) {
      nicknameInput.value = 'ヘンリー・アーミティッジ';
      nicknameInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    target.querySelector<HTMLButtonElement>('[data-testid="join-created-room-button"]')?.click();
    expect(enteredRoom).toBeTruthy();
    expect(enteredRoom?.name).toBe('アーカム図書館');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/character CharacterManagementTab 編集入力欄 M3 Outlined TextField
  it('renders M3 Outlined TextFields in CharacterManagementTab edit panel for all character properties', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testChar: Character = {
      id: 'char-investigator',
      name: '探偵',
      initiative: 12,
      description: '敏腕私立探偵',
      reference: 'https://alhalte.example/investigator',
      statuses: [{ label: 'SAN', current: 60, max: 80 }],
      parameters: [{ label: 'CON', value: 14 }],
      chatPalette: '1D100<=70 【聞き耳】',
      hideStatus: false,
    };

    let updatedChar: Character | null = null;
    const instance = mount(CharacterManagementTab, {
      target,
      props: {
        characters: [testChar],
        onUpdateCharacter: (c) => {
          updatedChar = c;
        },
      },
    });

    // Open edit panel
    target.querySelector<HTMLButtonElement>('[data-testid="edit-char-btn-char-investigator"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Verify M3 Outlined TextFields exist
    const nameInput = target.querySelector<HTMLInputElement>('[data-testid="edit-name-input"]');
    const initInput = target.querySelector<HTMLInputElement>('[data-testid="edit-init-input"]');
    const descInput = target.querySelector<HTMLTextAreaElement>('[data-testid="edit-description-input"]');
    const refInput = target.querySelector<HTMLInputElement>('[data-testid="edit-reference-input"]');
    const paletteInput = target.querySelector<HTMLTextAreaElement>('[data-testid="edit-chat-palette-input"]');
    const statusLabelInput = target.querySelector<HTMLInputElement>('[data-testid="status-label-0"]');
    const statusCurrentInput = target.querySelector<HTMLInputElement>('[data-testid="status-current-0"]');
    const statusMaxInput = target.querySelector<HTMLInputElement>('[data-testid="status-max-0"]');
    const paramLabelInput = target.querySelector<HTMLInputElement>('[data-testid="param-label-0"]');
    const paramValueInput = target.querySelector<HTMLInputElement>('[data-testid="param-value-0"]');

    expect(nameInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(initInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(descInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(refInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(paletteInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(statusLabelInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(statusCurrentInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(statusMaxInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(paramLabelInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(paramValueInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);

    // Modify values and save
    if (nameInput) {
      nameInput.value = '凄腕探偵';
      nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    if (initInput) {
      initInput.value = '15';
      initInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    if (statusCurrentInput) {
      statusCurrentInput.value = '55';
      statusCurrentInput.dispatchEvent(new Event('input', { bubbles: true }));
    }

    target.querySelector<HTMLButtonElement>('[data-testid="save-character-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(updatedChar).toBeTruthy();
    expect(updatedChar?.name).toBe('凄腕探偵');
    expect(Number(updatedChar?.initiative)).toBe(15);
    expect(Number(updatedChar?.statuses[0]?.current)).toBe(55);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/scene-settings SceneSettingsTab 各種入力欄 M3 Outlined TextField
  it('renders M3 Outlined TextFields in SceneSettingsTab for scene name, dimensions with px suffix, and creation dialog', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-field-test',
      name: '洋館エントランス',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: ['char-1'],
    };

    const testCharacters: Character[] = [
      {
        id: 'char-1',
        name: '助手',
        initiative: 8,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];

    let updatedScene: Scene | null = null;
    let createdSceneData: { name: string; inheritJuice: boolean } | null = null;

    const instance = mount(SceneSettingsTab, {
      target,
      props: {
        currentScene: testScene,
        allScenes: [testScene],
        characters: testCharacters,
        onUpdateScene: (sc) => {
          updatedScene = sc;
        },
        onCreateScene: (name, inheritJuice) => {
          createdSceneData = { name, inheritJuice };
        },
      },
    });

    // 1. Scene name input
    const sceneNameInput = target.querySelector<HTMLInputElement>('[data-testid="scene-name-input"]');
    expect(sceneNameInput).toBeTruthy();
    expect(sceneNameInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(sceneNameInput?.value).toBe('洋館エントランス');

    if (sceneNameInput) {
      sceneNameInput.value = '洋館ホール';
      sceneNameInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    expect(updatedScene?.name).toBe('洋館ホール');

    // 2. Width and Height inputs with suffixText="px"
    const widthInput = target.querySelector<HTMLInputElement>('[data-testid="bg-width-input"]');
    const heightInput = target.querySelector<HTMLInputElement>('[data-testid="bg-height-input"]');
    expect(widthInput).toBeTruthy();
    expect(heightInput).toBeTruthy();
    expect(widthInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(heightInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);

    const widthSuffix = widthInput?.closest('.np-field')?.querySelector('.suffix');
    const heightSuffix = heightInput?.closest('.np-field')?.querySelector('.suffix');
    expect(widthSuffix?.textContent?.trim()).toBe('px');
    expect(heightSuffix?.textContent?.trim()).toBe('px');

    if (widthInput) {
      widthInput.value = '1920';
      widthInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    expect(updatedScene?.juices.bgWidth).toBe(1920);

    // 3. New Scene creation overlay
    target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    const newSceneInput = target.querySelector<HTMLInputElement>('[data-testid="new-scene-name-input"]');
    expect(newSceneInput).toBeTruthy();
    expect(newSceneInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);

    if (newSceneInput) {
      newSceneInput.value = '地下通路';
      newSceneInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    target.querySelector<HTMLButtonElement>('[data-testid="submit-create-scene-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(createdSceneData).toEqual({ name: '地下通路', inheritJuice: true });

    // 4. Move members embedded in tab (Issue #92)
    const memberTargetInput = target.querySelector<HTMLInputElement>('[data-testid="member-target-input-char-1"]');
    expect(memberTargetInput).toBeTruthy();
    expect(memberTargetInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/asset-picker AssetPickerModal 検索およびURL直接入力 M3 Outlined TextField
  it('renders M3 Outlined TextFields in AssetPickerModal for searching assets and entering direct URLs', async () => {
    const asset1 = addAsset({
      id: 'ast-castle',
      name: '古城.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,Y2FzdGxl',
      mimeType: 'image/webp',
      createdAt: 1000,
    });
    const asset2 = addAsset({
      id: 'ast-forest',
      name: '深い森.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,Zm9yZXN0',
      mimeType: 'image/webp',
      createdAt: 2000,
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    let selectedUrl: string | undefined = undefined;
    let modalClosed = false;

    const instance = mount(AssetPickerModal, {
      target,
      props: {
        open: true,
        initialMediaType: 'image',
        initialScope: 'personal',
        onSelect: (url) => {
          selectedUrl = url;
        },
        onClose: () => {
          modalClosed = true;
        },
      },
    });

    const searchInput = target.querySelector<HTMLInputElement>('[data-testid="asset-search-input"]');
    const urlInput = target.querySelector<HTMLInputElement>('[data-testid="asset-url-input"]');
    expect(searchInput).toBeTruthy();
    expect(urlInput).toBeTruthy();

    expect(searchInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);
    expect(urlInput?.closest('.np-field')?.classList.contains('np-field-outlined')).toBe(true);

    // Both assets are visible initially
    expect(target.querySelector(`[data-testid="asset-card-${asset1.id}"]`)).toBeTruthy();
    expect(target.querySelector(`[data-testid="asset-card-${asset2.id}"]`)).toBeTruthy();

    // Type "森" in search input -> filters down to asset2
    if (searchInput) {
      searchInput.value = '森';
      searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector(`[data-testid="asset-card-${asset1.id}"]`)).toBeNull();
    expect(target.querySelector(`[data-testid="asset-card-${asset2.id}"]`)).toBeTruthy();

    // Direct URL input
    const applyUrlBtn = target.querySelector<HTMLButtonElement>('[data-testid="asset-apply-url-btn"]');
    expect(applyUrlBtn?.disabled).toBe(true);

    if (urlInput) {
      urlInput.value = 'https://images.example/custom-map.png';
      urlInput.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    expect(applyUrlBtn?.disabled).toBe(false);
    applyUrlBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(selectedUrl).toBe('https://images.example/custom-map.png');
    expect(modalClosed).toBe(true);

    unmount(instance);
    target.remove();
  });
});
