import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import CharacterManagementTab from '../src/components/Control/tabs/CharacterManagementTab.svelte';
import ExtensionManagementTab from '../src/components/Control/tabs/ExtensionManagementTab.svelte';
import type { Character, ExtensionItem } from '../src/types/index.js';
import { clearAssets } from '../src/utils/assets.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';

/** Lets the tab-open `QueryState('/')` (a microtask) settle. */
function tick(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

describe('Lists Migration to noph-ui List and ListItem (Issue #59)', () => {
  beforeEach(() => {
    clearAssets();
  });

  // spec: alhalte-argenterie/character M3 List/ListItem 移行と駒配置・削除アイコンボタン
  it('renders character list with noph-ui List/ListItem, full width, person_pin_circle piece button, and delete button', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testChar: Character = {
      id: 'char-warrior',
      name: '戦士アルベルト',
      initiative: 12,
      description: '前衛で戦う屈強な戦士。大剣を振るう。',
      reference: 'https://alhalte.example/char/warrior',
      statuses: [{ label: 'HP', current: 28, max: 28 }],
      parameters: [{ label: 'STR', value: 16 }],
      chatPalette: '2D6+4 【大剣攻撃】',
      hideStatus: false,
      placedAsPiece: false,
    };

    let toggledPieceId: string | null = null;
    let deleteConfirmChar: { id: string; name: string } | null = null;

    const instance = mount(CharacterManagementTab, {
      target,
      props: {
        characters: [testChar],
        isAdmin: true,
        onTogglePiece: (id) => {
          toggledPieceId = id;
        },
        onRequestDeleteCharacter: (c) => {
          deleteConfirmChar = c;
        },
      },
    });

    // Verify List container is rendered
    const listEl = target.querySelector<HTMLUListElement>('[data-testid="character-list"]');
    expect(listEl).toBeTruthy();
    expect(listEl?.tagName.toLowerCase()).toBe('ul');

    // Verify ListItem item wrapper
    const itemEl = target.querySelector('[data-testid="character-card-char-warrior"]');
    expect(itemEl).toBeTruthy();
    expect(itemEl?.classList.contains('np-item')).toBe(true);

    // Verify avatar button / image
    const avatarBtn = target.querySelector<HTMLButtonElement>('[data-testid="edit-char-btn-char-warrior"]');
    expect(avatarBtn).toBeTruthy();

    // Verify name and supporting text (description)
    const nameEl = target.querySelector('[data-testid="char-name-char-warrior"]');
    expect(nameEl?.textContent).toBe('戦士アルベルト');
    const descEl = target.querySelector('[data-testid="char-desc-char-warrior"]');
    expect(descEl?.textContent).toContain('前衛で戦う屈強な戦士');

    // Verify "駒として配置" IconButton
    const pieceBtn = target.querySelector<HTMLButtonElement>('[data-testid="piece-char-btn-char-warrior"]');
    expect(pieceBtn).toBeTruthy();
    expect(pieceBtn?.classList.contains('np-icon-button')).toBe(true);
    expect(pieceBtn?.getAttribute('aria-label')).toBe('駒として配置');
    expect(pieceBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('person_pin_circle');

    pieceBtn?.click();
    expect(toggledPieceId).toBe('char-warrior');

    // Verify "キャラクターを削除" IconButton
    const deleteBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-char-btn-char-warrior"]');
    expect(deleteBtn).toBeTruthy();
    expect(deleteBtn?.classList.contains('np-icon-button')).toBe(true);
    expect(deleteBtn?.getAttribute('aria-label')).toBe('キャラクターを削除');
    expect(deleteBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('delete');

    deleteBtn?.click();
    expect(deleteConfirmChar).toEqual({ id: 'char-warrior', name: '戦士アルベルト' });

    // Item click opens edit panel
    itemEl?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="character-edit-panel"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/extensions-abi M3 List/ListItem 移行、テーマアイコン、Switch、三点リーダメニュー
  it('renders extension list with noph-ui List/ListItem, themed icon, Switch, and more_vert menu with delete action', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testExt: ExtensionItem = {
      id: 'ext-dice',
      name: 'カスタムダイス',
      iconSvg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2z"/></svg>',
      description: '独自のダイスボット計算式を提供する拡張機能',
      enabled: true,
      priority: 0,
    };

    let updatedExtensions: ExtensionItem[] | null = null;

    // The tab only mutates after the server acknowledges (Issue #109 / #145), so
    // mount it on a connected socket whose state writes answer immediately.
    const fakeSocket = new FakeRoomSocket();

    const instance = mount(ExtensionManagementTab, {
      target,
      props: {
        extensions: [testExt],
        isAdmin: true,
        roomSocket: asRoomSocket(fakeSocket),
        onUpdateExtensions: (exts) => {
          updatedExtensions = exts;
        },
      },
    });

    // Let the tab-open `QueryState('/')` merge settle (empty remote state keeps
    // the local row because the list is only used for rendering here).
    await tick();

    // Verify List container
    const listEl = target.querySelector<HTMLUListElement>('[data-testid="extension-list"]');
    expect(listEl).toBeTruthy();
    expect(listEl?.tagName.toLowerCase()).toBe('ul');

    // Verify ListItem row
    const itemEl = target.querySelector('[data-testid="extension-item-ext-dice"]');
    expect(itemEl).toBeTruthy();
    expect(itemEl?.classList.contains('np-item')).toBe(true);

    // Verify themed icon container
    const iconContainer = target.querySelector('[data-testid="ext-icon-ext-dice"]');
    expect(iconContainer).toBeTruthy();
    expect(iconContainer?.classList.contains('ext-icon-themed')).toBe(true);
    expect(iconContainer?.querySelector('svg')).toBeTruthy();

    // Verify name & description
    expect(target.querySelector('[data-testid="ext-name-ext-dice"]')?.textContent?.trim()).toBe('カスタムダイス');
    expect(target.querySelector('[data-testid="ext-description-ext-dice"]')?.textContent?.trim()).toBe(
      '独自のダイスボット計算式を提供する拡張機能',
    );

    // Verify Switch
    const switchInput = target.querySelector<HTMLInputElement>('[data-testid="ext-toggle-ext-dice"]');
    expect(switchInput).toBeTruthy();
    expect(switchInput?.checked).toBe(true);
    switchInput?.click();
    await tick();
    expect(updatedExtensions).toBeTruthy();
    expect(updatedExtensions?.find((e) => e.id === 'ext-dice')?.enabled).toBe(false);

    // Verify "その他" IconButton (more_vert)
    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="ext-more-btn-ext-dice"]');
    expect(moreBtn).toBeTruthy();
    expect(moreBtn?.classList.contains('np-icon-button')).toBe(true);
    expect(moreBtn?.getAttribute('aria-label')).toBe('その他');
    expect(moreBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('more_vert');

    vi.spyOn(moreBtn!, 'getBoundingClientRect').mockReturnValue({
      top: 150,
      bottom: 186,
      left: 500,
      right: 536,
      width: 36,
      height: 36,
      x: 500,
      y: 150,
      toJSON: () => {},
    });

    // Click "その他" to open Menu
    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const extMenu = target.querySelector<HTMLElement>('[data-testid="ext-menu-ext-dice"]');
    expect(extMenu).toBeTruthy();
    expect(extMenu?.style.position).toBe('fixed');
    expect(extMenu?.getAttribute('style')).toContain('position: fixed !important');
    expect(extMenu?.getAttribute('style')).toContain('position-area: none !important');
    expect(extMenu?.getAttribute('style')).toContain('position-try-fallbacks: none !important');
    expect(extMenu?.style.top).toBe('190px');

    // Delete MenuItem
    const deleteMenuItem = target.querySelector<HTMLElement>('[data-testid="ext-delete-btn-ext-dice"]');
    expect(deleteMenuItem).toBeTruthy();
    deleteMenuItem?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(updatedExtensions).toEqual([]);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/extensions-abi 非管理者時の無効化・暗転制御
  it('locks and grays out extension management tab for non-admin users with opacity and pointer-events', () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testExt: ExtensionItem = {
      id: 'ext-calc',
      name: '計算拡張',
      description: '計算機能を提供',
      enabled: false,
      priority: 0,
    };

    const instance = mount(ExtensionManagementTab, {
      target,
      props: {
        extensions: [testExt],
        isAdmin: false,
      },
    });

    const rootEl = target.querySelector<HTMLDivElement>('[data-testid="extension-management-tab"]');
    expect(rootEl).toBeTruthy();
    expect(rootEl?.classList.contains('non-admin-locked')).toBe(true);

    // Lock indicator
    expect(target.querySelector('[data-testid="lock-indicator"]')).toBeTruthy();

    // Admin-only upload button must not exist
    expect(target.querySelector('[data-testid="upload-extension-btn"]')).toBeNull();

    // Switch and More button must be disabled
    const switchInput = target.querySelector<HTMLInputElement>('[data-testid="ext-toggle-ext-calc"]');
    expect(switchInput?.disabled).toBe(true);

    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="ext-more-btn-ext-calc"]');
    expect(moreBtn?.disabled).toBe(true);

    unmount(instance);
    target.remove();
  });
});
