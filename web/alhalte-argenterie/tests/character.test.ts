import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import { truncateDescription, MAX_DESCRIPTION_LENGTH, DESCRIPTION_ELLIPSIS } from '../src/utils/character.js';
import { addAsset, clearAssets } from '../src/utils/assets.js';
import CharacterManagementTab from '../src/components/Control/tabs/CharacterManagementTab.svelte';
import type { Character } from '../src/types/index.js';

describe('Character Management & Edit Panel', () => {
  beforeEach(() => {
    clearAssets();
  });

  // spec: alhalte-argenterie/character 説明120文字……省略
  it('truncates descriptions exceeding 120 characters with …… and preserves <= 120 chars', () => {
    // 50 chars - no truncation
    const shortDesc = 'これは短いキャラクター説明文です。特に省略される必要はありません。';
    expect(truncateDescription(shortDesc)).toBe(shortDesc);

    // Exactly 120 chars - no truncation
    const exact120 = 'あ'.repeat(120);
    expect(truncateDescription(exact120)).toBe(exact120);

    // 121 chars - truncated to 120 chars + '……'
    const over120 = 'あ'.repeat(120) + 'い';
    const truncated = truncateDescription(over120);
    expect(truncated).toBe('あ'.repeat(120) + DESCRIPTION_ELLIPSIS);
    expect(truncated.endsWith(DESCRIPTION_ELLIPSIS)).toBe(true);
  });

  // spec: alhalte-argenterie/character CharacterManagementTab コンポーネント編集パネルと削除警告
  it('renders character list with 120-char ellipsis, edit panel, and full-screen delete warning', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const longDesc = '長大なバックストーリー。'.repeat(15); // > 120 characters
    const testChar: Character = {
      id: 'char-mage',
      name: '大魔道士',
      initiative: 14,
      description: longDesc,
      reference: 'https://alhalte.example/char/1',
      statuses: [
        { label: 'HP', current: 15, max: 15 },
        { label: 'MP', current: 30, max: 30 },
      ],
      parameters: [{ label: 'INT', value: 18 }],
      chatPalette: '1D100<=90 【魔術】',
      hideStatus: false,
      placedAsPiece: false,
    };

    let updatedChar: Character | null = null;
    let deletedId: string | null = null;
    let pieceToggledId: string | null = null;

    const instance = mount(CharacterManagementTab, {
      target,
      props: {
        characters: [testChar],
        onUpdateCharacter: (c) => {
          updatedChar = c;
        },
        onDeleteCharacter: (id) => {
          deletedId = id;
        },
        onTogglePiece: (id) => {
          pieceToggledId = id;
        },
      },
    });

    // Check character card
    expect(target.querySelector('[data-testid="char-name-char-mage"]')?.textContent).toBe('大魔道士');

    // Description must be truncated with ……
    const descEl = target.querySelector('[data-testid="char-desc-char-mage"]');
    expect(descEl?.textContent).toContain('……');
    expect(descEl?.textContent?.length).toBeLessThan(longDesc.length);

    // Check "新規作成" button is an M3 IconButton with person_add icon
    const createBtn = target.querySelector<HTMLButtonElement>('[data-testid="create-character-btn"]');
    expect(createBtn).toBeTruthy();
    expect(createBtn?.classList.contains('np-icon-button')).toBe(true);
    expect(createBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('person_add');

    // Test "駒として配置" and "削除" IconButtons are inside right-aligned .char-actions container (Issue #31)
    const actionsContainer = target.querySelector('[data-testid="character-card-char-mage"] .char-actions');
    expect(actionsContainer).toBeTruthy();
    const pieceBtn = target.querySelector<HTMLButtonElement>('[data-testid="piece-char-btn-char-mage"]');
    const deleteBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-char-btn-char-mage"]');
    expect(pieceBtn).toBeTruthy();
    expect(deleteBtn).toBeTruthy();
    expect(actionsContainer?.contains(pieceBtn)).toBe(true);
    expect(actionsContainer?.contains(deleteBtn)).toBe(true);
    expect(pieceBtn?.classList.contains('np-icon-button')).toBe(true);
    expect(pieceBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('person_pin_circle');
    pieceBtn?.click();
    expect(pieceToggledId).toBe('char-mage');

    // Open Edit panel by clicking avatar
    target.querySelector<HTMLButtonElement>('[data-testid="edit-char-btn-char-mage"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="edit-dialog-title"]')?.textContent?.trim()).toBe(
      'キャラクター編集',
    );

    // Verify fields in edit panel
    expect(target.querySelector('[data-testid="edit-name-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="edit-init-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="edit-description-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="edit-reference-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="edit-status-list"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="edit-param-list"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="edit-chat-palette-input"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="edit-hide-status-toggle"]')).toBeTruthy();

    // Modify initiative and save
    const initInput = target.querySelector<HTMLInputElement>('[data-testid="edit-init-input"]');
    if (initInput) {
      initInput.value = '18';
      initInput.dispatchEvent(new Event('input'));
    }

    target.querySelector<HTMLButtonElement>('[data-testid="save-character-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(updatedChar).toBeTruthy();
    expect(Number(updatedChar?.initiative)).toBe(18);

    // Test Delete IconButton -> opens full-screen warning dialog
    expect(deleteBtn).toBeTruthy();
    expect(deleteBtn?.classList.contains('np-icon-button')).toBe(true);
    expect(deleteBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('delete');
    deleteBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="char-delete-confirm-dialog"]')).toBeTruthy();
    expect(
      target.querySelector('[data-testid="char-delete-warning-title"]')?.textContent,
    ).toContain('大魔道士');

    // Confirm delete
    target.querySelector<HTMLButtonElement>('[data-testid="confirm-char-delete-button"]')?.click();
    expect(deletedId).toBe('char-mage');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/character 新規作成時の初期値（ステータス、パラメータ、チャットパレットは空） Issue #27
  it('creates new character with empty statuses, parameters, and chatPalette', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let createdChar: Character | null = null;

    const instance = mount(CharacterManagementTab, {
      target,
      props: {
        characters: [],
        onCreateCharacter: (c) => {
          createdChar = c;
        },
      },
    });

    const createBtn = target.querySelector<HTMLButtonElement>('[data-testid="create-character-btn"]');
    expect(createBtn).toBeTruthy();
    createBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(createdChar).toBeTruthy();
    expect(createdChar?.statuses).toEqual([]);
    expect(createdChar?.parameters).toEqual([]);
    expect(createdChar?.chatPalette).toBe('');
    expect(createdChar?.name).toBe('新規キャラクター');
    expect(createdChar?.initiative).toBe(0);

    // Edit panel should be open for the newly created character
    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeTruthy();
    const chatPaletteInput = target.querySelector<HTMLTextAreaElement>('[data-testid="edit-chat-palette-input"]');
    expect(chatPaletteInput?.value).toBe('');

    // Adding status row initializes with empty label and 0 values
    target.querySelector<HTMLButtonElement>('[data-testid="add-status-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    const statusLabel = target.querySelector<HTMLInputElement>('[data-testid="status-label-0"]');
    const statusCurrent = target.querySelector<HTMLInputElement>('[data-testid="status-current-0"]');
    const statusMax = target.querySelector<HTMLInputElement>('[data-testid="status-max-0"]');
    expect(statusLabel?.value).toBe('');
    expect(Number(statusCurrent?.value)).toBe(0);
    expect(Number(statusMax?.value)).toBe(0);

    // Adding param row initializes with empty label and 0 value
    target.querySelector<HTMLButtonElement>('[data-testid="add-param-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    const paramLabel = target.querySelector<HTMLInputElement>('[data-testid="param-label-0"]');
    const paramValue = target.querySelector<HTMLInputElement>('[data-testid="param-value-0"]');
    expect(paramLabel?.value).toBe('');
    expect(Number(paramValue?.value)).toBe(0);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/character アセット選択オーバーレイとカルーセルによる立ち絵・アイコン選択 (Issue #28)
  it('selects and clears character icon via AssetPickerModal and M3 Carousel', async () => {
    const heroIcon = addAsset({
      id: 'char-img-1',
      name: 'hero.webp',
      scope: 'personal',
      mediaType: 'image',
      dataUrl: 'data:image/webp;base64,aGVybw==',
      mimeType: 'image/webp',
      createdAt: 1000,
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const testChar: Character = {
      id: 'char-knight',
      name: '騎士',
      initiative: 10,
      description: '前衛',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
    };

    let savedChar: Character | null = null;

    const instance = mount(CharacterManagementTab, {
      target,
      props: {
        characters: [testChar],
        isAdmin: false,
        onUpdateCharacter: (c) => {
          savedChar = c;
        },
      },
    });

    // Open character edit panel
    target.querySelector<HTMLButtonElement>('[data-testid="edit-char-btn-char-knight"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    // spec: Issue #52 アイコン画像選択UIではCarouselは不要
    expect(target.querySelector('[data-testid="char-image-carousel"]')).toBeNull();

    // Open AssetPickerModal by clicking edit-avatar-picker
    target.querySelector<HTMLButtonElement>('[data-testid="edit-avatar-picker"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="asset-card-none"]')).toBeTruthy();
    expect(target.querySelector(`[data-testid="asset-card-${heroIcon.id}"]`)).toBeTruthy();

    // Select heroIcon card in modal
    target.querySelector<HTMLElement>(`[data-testid="asset-card-${heroIcon.id}"]`)?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="asset-picker-modal"]')).toBeNull();

    const avatarImg = target.querySelector<HTMLImageElement>('[data-testid="edit-avatar-img"]');
    expect(avatarImg?.getAttribute('src')).toBe(heroIcon.dataUrl);

    // Clear via NONE card in AssetPickerModal
    target.querySelector<HTMLButtonElement>('[data-testid="open-char-image-picker-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLElement>('[data-testid="asset-card-none"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(avatarImg?.getAttribute('src')).toContain('data:image/svg+xml');

    // Re-select via AssetPickerModal (Issue #52: icons use AssetPickerModal instead of Carousel)
    target.querySelector<HTMLButtonElement>('[data-testid="edit-avatar-picker"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLElement>(`[data-testid="asset-card-${heroIcon.id}"]`)?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(avatarImg?.getAttribute('src')).toBe(heroIcon.dataUrl);

    // Save character and verify iconUrl was updated
    target.querySelector<HTMLButtonElement>('[data-testid="save-character-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(savedChar?.iconUrl).toBe(heroIcon.dataUrl);

    unmount(instance);
    target.remove();
  });
});
