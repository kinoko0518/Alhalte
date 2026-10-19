/**
 * Tests for Room Permission Management, Viewer Restrictions, and Account Auth (Issue #94).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-maitre-d -> ### 部屋権限管理API`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> コントロールパネル共通ヘッダー`
 * - GitHub Issue #94: アカウントの概念をArgenterieと接続
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import RoomPermissionModal from '../src/components/Control/RoomPermissionModal.svelte';
import CommonHeader from '../src/components/Control/CommonHeader.svelte';
import CommonFooter from '../src/components/Control/CommonFooter.svelte';
import App from '../src/components/App.svelte';
import type { Scene, Character, VisitedRoom } from '../src/types/index.js';
import type { AccountSearchItem, RoomPermissionItem } from '../src/utils/session.js';
import { LOCAL_STORAGE_ACCOUNT_ID_KEY, setRoomPermission } from '../src/utils/session.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';

describe('Room Permission Management & Viewer Restrictions (Issue #94)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    localStorage.clear();
    document.cookie = '';
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    target.remove();
    vi.restoreAllMocks();
  });

  // spec: alhalte-argenterie/permissions 0010 タイトル「部屋の権限管理」、M3 Search アカウント検索、Divider 区切りリスト追加
  it('renders RoomPermissionModal, searches accounts with icon, name, #(account_id) and adds to Divider list', async () => {
    const mockAccounts: AccountSearchItem[] = [
      { account_id: 1001, username: 'Alice', icon_url: null },
      { account_id: 2002, username: 'Bob', icon_url: 'http://example.com/bob.png' },
    ];

    const mockFetch = vi.fn(async (url: string | URL | Request, init?: RequestInit) => {
      const urlStr = url.toString();
      if (urlStr.includes('/api/accounts/search')) {
        return {
          ok: true,
          status: 200,
          json: async () => mockAccounts,
        } as Response;
      }
      if (urlStr.includes('/api/room/0123456789abcdef/permissions') && init?.method === 'POST') {
        return {
          ok: true,
          status: 200,
          json: async () => ({ status: 'ok' }),
        } as Response;
      }
      return {
        ok: true,
        status: 200,
        json: async () => [],
      } as Response;
    });

    let permissionsResult: RoomPermissionItem[] = [];
    const instance = mount(RoomPermissionModal, {
      target,
      props: {
        roomId: '0123456789abcdef',
        initialPermissions: [],
        isOpen: true,
        fetchFn: mockFetch as unknown as typeof fetch,
        onPermissionsChange: (perms) => {
          permissionsResult = perms;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Verify Title
    const title = target.querySelector('[data-testid="room-permission-title"]');
    expect(title?.textContent?.trim()).toBe('部屋の権限管理');

    // Search accounts
    const searchInput = target.querySelector<HTMLInputElement>('[data-testid="account-search-input"]');
    expect(searchInput).toBeTruthy();
    if (searchInput) {
      searchInput.value = 'Bo';
      searchInput.dispatchEvent(new Event('input'));
    }

    // Wait for search debounce
    await new Promise((r) => setTimeout(r, 400));

    // Candidate item check
    const bobItem = target.querySelector<HTMLButtonElement>('[data-testid="search-result-item-2002"]');
    expect(bobItem).toBeTruthy();
    expect(bobItem?.textContent).toContain('Bob');
    expect(bobItem?.textContent).toContain('#2002');

    // Click candidate to add to list
    bobItem?.click();
    await new Promise((r) => setTimeout(r, 50));

    // Check Bob is added to Divider list with default 'pl' role
    const bobRow = target.querySelector('[data-testid="permission-row-2002"]');
    expect(bobRow).toBeTruthy();
    expect(bobRow?.textContent).toContain('Bob');
    expect(bobRow?.textContent).toContain('#2002');
    expect(target.querySelector('[data-testid="role-pl-btn-2002"]')?.classList.contains('np-button')).toBe(true);

    expect(permissionsResult.some((p) => p.account_id === 2002 && p.role === 'pl')).toBe(true);

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0020 connected button group (size="s") でのロール切り替え (閲覧者/PL/GM)
  it('toggles role via connected button group, switching directly to PL without warning', async () => {
    const initialPerms: RoomPermissionItem[] = [
      { account_id: 1001, username: 'Alice', role: 'viewer' },
    ];

    const mockFetch = vi.fn(async () => {
      return { ok: true, status: 200, json: async () => ({ status: 'ok' }) } as Response;
    });

    let updatedList: RoomPermissionItem[] = [];
    const instance = mount(RoomPermissionModal, {
      target,
      props: {
        roomId: 'fedcba9876543210',
        initialPermissions: initialPerms,
        isOpen: true,
        fetchFn: mockFetch as unknown as typeof fetch,
        onPermissionsChange: (perms) => {
          updatedList = perms;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    const roleGroup = target.querySelector('[data-testid="role-button-group-1001"]');
    expect(roleGroup).toBeTruthy();
    expect(roleGroup?.classList.contains('np-button-group')).toBe(true);

    // Switch to PL (no warning required)
    const plBtn = target.querySelector<HTMLButtonElement>('[data-testid="role-pl-btn-1001"]');
    plBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    // No warning dialog should appear
    expect(target.querySelector('[data-testid="permission-warning-overlay"]')).toBeNull();
    expect(updatedList.find((p) => p.account_id === 1001)?.role).toBe('pl');

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0030 閲覧者への切り替え時の全画面警告
  it('shows full-screen warning when switching to 閲覧者 (Viewer), with cancel and confirm support', async () => {
    const initialPerms: RoomPermissionItem[] = [
      { account_id: 1001, username: 'Alice', role: 'pl' },
    ];

    const instance = mount(RoomPermissionModal, {
      target,
      props: {
        roomId: 'fedcba9876543210',
        initialPermissions: initialPerms,
        isOpen: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Click 閲覧者 button
    const viewerBtn = target.querySelector<HTMLButtonElement>('[data-testid="role-viewer-btn-1001"]');
    viewerBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Fullscreen warning dialog must be displayed
    const warning = target.querySelector('[data-testid="permission-warning-dialog"]');
    expect(warning).toBeTruthy();
    expect(warning?.textContent).toContain('ロールを「閲覧者」に切り替えますか？');
    expect(warning?.textContent).toContain('メッセージの送信や部屋の状態変更');

    // Click Cancel -> dialog closes and role remains PL
    const cancelBtn = target.querySelector<HTMLButtonElement>('[data-testid="warning-cancel-btn"]');
    cancelBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="permission-warning-dialog"]')).toBeNull();

    // Click 閲覧者 button again and confirm
    viewerBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    const confirmBtn = target.querySelector<HTMLButtonElement>('[data-testid="warning-confirm-role-btn"]');
    confirmBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="permission-warning-dialog"]')).toBeNull();
    expect(target.querySelector('[data-testid="role-viewer-btn-1001"]')?.getAttribute('value')).toBe('viewer');

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0040 GM への切り替え時の全画面警告
  it('shows full-screen warning when switching to GM, with cancel and confirm support', async () => {
    const initialPerms: RoomPermissionItem[] = [
      { account_id: 1001, username: 'Alice', role: 'pl' },
    ];

    const instance = mount(RoomPermissionModal, {
      target,
      props: {
        roomId: 'fedcba9876543210',
        initialPermissions: initialPerms,
        isOpen: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Click GM button
    const gmBtn = target.querySelector<HTMLButtonElement>('[data-testid="role-gm-btn-1001"]');
    gmBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const warning = target.querySelector('[data-testid="permission-warning-dialog"]');
    expect(warning).toBeTruthy();
    expect(warning?.textContent).toContain('ロールを「GM（管理者）」に切り替えますか？');

    // Click Confirm
    const confirmBtn = target.querySelector<HTMLButtonElement>('[data-testid="warning-confirm-role-btn"]');
    confirmBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="permission-warning-dialog"]')).toBeNull();

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0050 削除ボタン押下時の全画面警告とリストからの削除
  it('shows full-screen warning on delete button click, and deletes account from permissions upon confirm', async () => {
    const initialPerms: RoomPermissionItem[] = [
      { account_id: 1001, username: 'Alice', role: 'pl' },
      { account_id: 2002, username: 'Bob', role: 'viewer' },
    ];
    const mockFetch = vi.fn(async () => {
      return { ok: true, status: 200, json: async () => ({ status: 'ok' }) } as Response;
    });

    let currentList: RoomPermissionItem[] = [...initialPerms];
    const instance = mount(RoomPermissionModal, {
      target,
      props: {
        roomId: 'fedcba9876543210',
        initialPermissions: initialPerms,
        isOpen: true,
        fetchFn: mockFetch as unknown as typeof fetch,
        onPermissionsChange: (perms) => {
          currentList = perms;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Click delete button for Bob
    const deleteBobBtn = target.querySelector<HTMLButtonElement>('[data-testid="delete-permission-btn-2002"]');
    deleteBobBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Full-screen warning alert dialog appears
    const warning = target.querySelector('[data-testid="permission-warning-dialog"]');
    expect(warning).toBeTruthy();
    expect(warning?.textContent).toContain('アカウント権限の削除');
    expect(warning?.textContent).toContain('Bob (#2002)');

    // Click confirm delete button
    const confirmDeleteBtn = target.querySelector<HTMLButtonElement>('[data-testid="warning-confirm-delete-btn"]');
    confirmDeleteBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Bob is removed from list
    expect(target.querySelector('[data-testid="permission-row-2002"]')).toBeNull();
    expect(target.querySelector('[data-testid="permission-row-1001"]')).toBeTruthy();
    expect(currentList.some((p) => p.account_id === 2002)).toBe(false);

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0060 権限変更はサーバー成功後にのみ確定する
  it('commits a GM role change only after setRoomPermission succeeds', async () => {
    const initialPerms: RoomPermissionItem[] = [
      { account_id: 1001, username: 'Alice', role: 'pl' },
    ];
    const mockFetch = vi.fn(async () => {
      return { ok: true, status: 200, json: async () => ({ status: 'ok' }) } as Response;
    });
    let updated: RoomPermissionItem[] | null = null;

    const instance = mount(RoomPermissionModal, {
      target,
      props: {
        roomId: 'fedcba9876543210',
        initialPermissions: initialPerms,
        isOpen: true,
        fetchFn: mockFetch as unknown as typeof fetch,
        onPermissionsChange: (perms) => {
          updated = perms;
        },
      },
    });
    await new Promise((r) => setTimeout(r, 20));

    target.querySelector<HTMLButtonElement>('[data-testid="role-gm-btn-1001"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLButtonElement>('[data-testid="warning-confirm-role-btn"]')?.click();
    // Wait for the awaited POST to settle.
    for (let i = 0; i < 20 && updated === null; i++) {
      await new Promise((r) => setTimeout(r, 10));
    }

    expect(mockFetch).toHaveBeenCalled();
    expect(updated).not.toBeNull();
    expect(updated!.find((p) => p.account_id === 1001)?.role).toBe('gm');
    expect(target.querySelector('[data-testid="permission-row-1001"]')?.getAttribute('data-role')).toBe(
      'gm',
    );
    expect(target.querySelector('[data-testid="permission-error"]')).toBeNull();

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0061 権限変更の認証は caller account を使う
  it('sends the caller account, not the permission target, as the auth identity', async () => {
    localStorage.setItem(LOCAL_STORAGE_ACCOUNT_ID_KEY, '9001');
    const mockFetch = vi.fn(
      async (_url: string | URL | Request, _init?: RequestInit) =>
        ({ ok: true, status: 200, json: async () => ({ status: 'ok' }) }) as Response,
    );

    await setRoomPermission('fedcba9876543210', 1001, 'gm', {
      fetchFn: mockFetch as unknown as typeof fetch,
    });

    const init = mockFetch.mock.calls[0][1] as RequestInit;
    const headers = init.headers as Record<string, string>;
    expect(headers['X-Alhalte-Account']).toBe('9001');
    expect(headers['X-Alhalte-Account']).not.toBe('1001');
  });

  // spec: alhalte-argenterie/permissions 0070 権限変更失敗時のロールバックとエラー表示
  it('rolls back a denied role change and shows an error banner', async () => {
    const initialPerms: RoomPermissionItem[] = [
      { account_id: 1001, username: 'Alice', role: 'pl' },
    ];
    const mockFetch = vi.fn(async () => {
      return {
        ok: false,
        status: 403,
        statusText: 'Forbidden',
        json: async () => ({ error: 'permission denied' }),
      } as Response;
    });
    let updated: RoomPermissionItem[] | null = null;

    const instance = mount(RoomPermissionModal, {
      target,
      props: {
        roomId: 'fedcba9876543210',
        initialPermissions: initialPerms,
        isOpen: true,
        fetchFn: mockFetch as unknown as typeof fetch,
        onPermissionsChange: (perms) => {
          updated = perms;
        },
      },
    });
    await new Promise((r) => setTimeout(r, 20));

    target.querySelector<HTMLButtonElement>('[data-testid="role-gm-btn-1001"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLButtonElement>('[data-testid="warning-confirm-role-btn"]')?.click();

    let errorBanner: Element | null = null;
    for (let i = 0; i < 20 && errorBanner === null; i++) {
      await new Promise((r) => setTimeout(r, 10));
      errorBanner = target.querySelector('[data-testid="permission-error"]');
    }

    expect(errorBanner).toBeTruthy();
    expect(errorBanner?.textContent).toContain('失敗');
    // Rolled back: the role is still PL and no optimistic list was published.
    expect(target.querySelector('[data-testid="permission-row-1001"]')?.getAttribute('data-role')).toBe(
      'pl',
    );
    expect(updated).toBeNull();

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0080 権限削除失敗時のロールバックとエラー表示
  it('rolls back a failed permission delete and shows an error banner', async () => {
    const initialPerms: RoomPermissionItem[] = [
      { account_id: 1001, username: 'Alice', role: 'pl' },
      { account_id: 2002, username: 'Bob', role: 'viewer' },
    ];
    const mockFetch = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      if (init?.method === 'DELETE') {
        return {
          ok: false,
          status: 403,
          statusText: 'Forbidden',
          json: async () => ({ error: 'permission denied' }),
        } as Response;
      }
      return { ok: true, status: 200, json: async () => initialPerms } as Response;
    });
    let updated: RoomPermissionItem[] | null = null;

    const instance = mount(RoomPermissionModal, {
      target,
      props: {
        roomId: 'fedcba9876543210',
        initialPermissions: initialPerms,
        isOpen: true,
        fetchFn: mockFetch as unknown as typeof fetch,
        onPermissionsChange: (perms) => {
          updated = perms;
        },
      },
    });
    await new Promise((r) => setTimeout(r, 20));

    target.querySelector<HTMLButtonElement>('[data-testid="delete-permission-btn-2002"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    target
      .querySelector<HTMLButtonElement>('[data-testid="warning-confirm-delete-btn"]')
      ?.click();

    let errorBanner: Element | null = null;
    for (let i = 0; i < 20 && errorBanner === null; i++) {
      await new Promise((r) => setTimeout(r, 10));
      errorBanner = target.querySelector('[data-testid="permission-error"]');
    }

    expect(errorBanner).toBeTruthy();
    // Rolled back: the row is still present and nothing was published.
    expect(target.querySelector('[data-testid="permission-row-2002"]')).toBeTruthy();
    expect(updated).toBeNull();

    unmount(instance);
  });

  // spec: alhalte-argenterie/header 歯車メニューの管理者専用から「シーン名変更」「シーン削除」「シーンメンバー管理」削除と「部屋の権限管理」追加
  it('replaces scene rename/delete/members with 部屋の権限管理 in CommonHeader gear menu and triggers callback', async () => {
    const testScene: Scene = {
      id: '/',
      name: 'メインシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let openedPermissions = false;
    const instance = mount(CommonHeader, {
      target,
      props: {
        scene: testScene,
        isAdmin: true,
        onManagePermissions: () => {
          openedPermissions = true;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Click gear menu button
    target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Admin section must NOT contain old scene items
    expect(target.querySelector('[data-testid="rename-scene-item"]')).toBeNull();
    expect(target.querySelector('[data-testid="delete-scene-item"]')).toBeNull();
    expect(target.querySelector('[data-testid="manage-members-item"]')).toBeNull();

    // Must contain 部屋の権限管理 item
    const managePermsItem = target.querySelector<HTMLButtonElement>('[data-testid="manage-permissions-item"]');
    expect(managePermsItem).toBeTruthy();
    expect(managePermsItem?.textContent).toContain('部屋の権限管理');

    managePermsItem?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(openedPermissions).toBe(true);

    unmount(instance);
  });

  // spec: alhalte-argenterie/viewer 閲覧者のメッセージ送信および入力欄の無効化 (mutate restriction)
  it('disables message input, send button, and dice picker when isViewer is true in CommonFooter', async () => {
    let sentText = '';
    const instance = mount(CommonFooter, {
      target,
      props: {
        isViewer: true,
        characters: [],
        onSendMessage: (text) => {
          sentText = text;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Send button must be disabled
    const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
    expect(sendBtn?.disabled).toBe(true);

    // Chat input textarea must be disabled with viewer placeholder
    const chatInput = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(chatInput?.disabled).toBe(true);
    expect(chatInput?.placeholder).toContain('閲覧者モードのためメッセージは送信できません');

    // Dice buttons must be disabled
    const d100Btn = target.querySelector<HTMLButtonElement>('[data-testid="dice-btn-1d100"]');
    expect(d100Btn?.disabled).toBe(true);

    // Attempting send programmatically does nothing
    sendBtn?.click();
    expect(sentText).toBe('');

    unmount(instance);
  });

  // spec: alhalte-argenterie/gate 0010 maitre-d の入室拒否時に門前払い画面を表示 (Issue #115)
  it('displays the gate screen and blocks room operations when maitre-d denies entry', async () => {
    const testRoom: VisitedRoom = {
      id: 'private-room-001',
      name: 'プライベートルーム',
      url: 'https://alhalte.example/rooms/private-room-001#key',
      lastVisited: Date.now(),
      isAdmin: false,
    };

    const instance = mount(App, {
      target,
      props: {
        initialRoom: testRoom,
        isForbidden: true,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    const forbiddenScreen = target.querySelector('[data-testid="forbidden-room-screen"]');
    expect(forbiddenScreen).toBeTruthy();

    const title = target.querySelector('[data-testid="forbidden-title"]');
    expect(title?.textContent?.trim()).toBe('アカウントに入室権限がありません');

    const message = target.querySelector('[data-testid="forbidden-message"]');
    expect(message?.textContent?.trim()).toBe(
      '部屋の管理者に権限をリクエストしてください。正しいアカウントを使っていますか？',
    );

    const signInSwitchBtn = target.querySelector('[data-testid="forbidden-signin-button"]');
    expect(signInSwitchBtn?.textContent?.trim()).toBe('サインイン・アカウント切り替え');

    // Room layout should NOT be displayed
    expect(target.querySelector('[data-testid="room-layout"]')).toBeNull();

    // Clicking return home button clears room and shows home screen
    const returnHomeBtn = target.querySelector<HTMLButtonElement>('[data-testid="forbidden-return-home-btn"]');
    expect(returnHomeBtn).toBeTruthy();
    returnHomeBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="forbidden-room-screen"]')).toBeNull();
    expect(target.querySelector('[data-testid="home-screen"]')).toBeTruthy();

    unmount(instance);
  });
});
