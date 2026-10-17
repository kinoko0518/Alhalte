import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import { truncateDescription, MAX_DESCRIPTION_LENGTH, DESCRIPTION_ELLIPSIS } from '../src/utils/character.js';
import CharacterManagementTab from '../src/components/Control/tabs/CharacterManagementTab.svelte';
import type { Character } from '../src/types/index.js';

describe('Character Management & Edit Panel', () => {
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

    // Test "駒として配置" button
    target.querySelector<HTMLButtonElement>('[data-testid="place-piece-btn-char-mage"]')?.click();
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

    // Test Delete button -> opens full-screen warning dialog
    target.querySelector<HTMLButtonElement>('[data-testid="delete-char-btn-char-mage"]')?.click();
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
});
