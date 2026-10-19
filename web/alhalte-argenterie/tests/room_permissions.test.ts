/**
 * Tests for the four-level room permission UI (Issue #94 / #201).
 *
 * The permission table is read and written over the room WebSocket
 * (`QueryPermissions` / `SetMemberPermission` / `SetDefaultPermission`); only
 * the account search stays on HTTP. Every mutation is applied only after the
 * banquette ack.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
 * - ARCHITECTURE.md: `## alhalte-banquette -> ### protocol`
 * - GitHub Issue #94 / #201
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import RoomPermissionModal from '../src/components/Control/RoomPermissionModal.svelte';
import {
  permissionsSnapshotToTable,
  permissionLevelToRole,
  roleToPermissionLevel,
  roomRoleCapabilities,
  ROOM_ROLES,
} from '../src/utils/session.js';
import type { AccountSearchItem } from '../src/utils/session.js';
import { ServerOperationError } from '../src/utils/roomSocket.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';

async function tick(ms = 20): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

/** Search mock answering `#<id>` / name queries with the given accounts. */
function searchFetch(accounts: AccountSearchItem[]): typeof fetch {
  return vi.fn(async (url: string | URL | Request) => {
    const parsed = new URL(String(url), 'http://localhost');
    const q = parsed.searchParams.get('q') ?? '';
    const matches = accounts.filter(
      (account) => String(account.account_id) === q || account.username.includes(q),
    );
    return {
      ok: true,
      status: 200,
      json: async () => matches,
    } as Response;
  }) as unknown as typeof fetch;
}

describe('session permission helpers (Issue #201)', () => {
  // spec: alhalte-argenterie/permissions 0001 4 段階 role と wire 表記の変換
  it('converts between the lowercase UI role and the PascalCase wire level', () => {
    expect(roleToPermissionLevel('admin')).toBe('Admin');
    expect(roleToPermissionLevel('participant')).toBe('Participant');
    expect(roleToPermissionLevel('viewer')).toBe('Viewer');
    expect(roleToPermissionLevel('denied')).toBe('Denied');
    // A wire level can be fed back unchanged.
    expect(roleToPermissionLevel('Admin')).toBe('Admin');
    // Unknown input never widens the level.
    expect(roleToPermissionLevel('gm')).toBe('Denied');
    expect(roleToPermissionLevel(undefined)).toBe('Denied');

    expect(permissionLevelToRole('Admin')).toBe('admin');
    expect(permissionLevelToRole('Participant')).toBe('participant');
    expect(permissionLevelToRole('Viewer')).toBe('viewer');
    expect(permissionLevelToRole('Denied')).toBe('denied');
    expect(permissionLevelToRole('gm')).toBe('denied');
  });

  // spec: alhalte-argenterie/permissions 0002 PermissionsSnapshot の復号
  it('decodes a PermissionsSnapshot payload into account rows', () => {
    const table = permissionsSnapshotToTable({
      default_permission: 'Viewer',
      permissions: { '2002': 'Admin', '1001': 'Participant' },
    });

    expect(table.defaultPermission).toBe('viewer');
    // Sorted by account id so the list order is stable.
    expect(table.permissions).toEqual([
      { account_id: 1001, role: 'participant' },
      { account_id: 2002, role: 'admin' },
    ]);
  });

  // spec: alhalte-argenterie/permissions 0003 role ごとの操作権限
  it('derives the capability matrix from the four roles', () => {
    const admin = roomRoleCapabilities('admin');
    expect(admin).toMatchObject({
      isAdmin: true,
      isParticipant: false,
      isViewer: false,
      isDenied: false,
      canSend: true,
      canManageScenes: true,
      canManageCharacters: true,
      canManageExtensions: true,
      canManageRoom: true,
    });

    const participant = roomRoleCapabilities('participant');
    expect(participant).toMatchObject({
      isAdmin: false,
      isParticipant: true,
      isViewer: false,
      isDenied: false,
      canSend: true,
      canManageScenes: false,
      canManageCharacters: true,
      canManageExtensions: false,
      canManageRoom: false,
    });

    const viewer = roomRoleCapabilities('viewer');
    expect(viewer.isViewer).toBe(true);
    expect(viewer.canSend).toBe(false);
    expect(viewer.canManageScenes).toBe(false);
    expect(viewer.canManageCharacters).toBe(false);
    expect(viewer.canManageExtensions).toBe(false);
    expect(viewer.canManageRoom).toBe(false);

    const denied = roomRoleCapabilities('denied');
    expect(denied.isDenied).toBe(true);
    expect(denied.canSend).toBe(false);
    expect(denied.canManageRoom).toBe(false);

    // An unknown (pre-preflight) role grants nothing.
    expect(roomRoleCapabilities(null).canSend).toBe(false);
    expect(ROOM_ROLES).toEqual(['denied', 'viewer', 'participant', 'admin']);
  });
});

describe('RoomPermissionModal over the room WebSocket (Issue #201)', () => {
  let target: HTMLDivElement;
  let socket: FakeRoomSocket;

  const Alice: AccountSearchItem = { account_id: 1001, username: 'Alice', icon_url: null };
  const Bob: AccountSearchItem = { account_id: 2002, username: 'Bob', icon_url: null };

  beforeEach(() => {
    localStorage.clear();
    socket = new FakeRoomSocket();
    socket.permissionsSnapshot = {
      default_permission: 'Viewer',
      permissions: { '1001': 'Viewer' },
    };
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    target.remove();
    vi.restoreAllMocks();
  });

  function mountModal(onPermissionsChange?: (table: unknown) => void) {
    return mount(RoomPermissionModal, {
      target,
      props: {
        roomSocket: asRoomSocket(socket),
        fetchFn: searchFetch([Alice, Bob]),
        isOpen: true,
        onPermissionsChange,
      },
    });
  }

  // spec: alhalte-argenterie/permissions 0010 一覧は QueryPermissions の ack から構築する
  it('loads the list with QueryPermissions and resolves account names over HTTP search', async () => {
    const instance = mountModal();
    await tick(40);

    expect(socket.requestQueryPermissionsCalls).toBe(1);
    const row = target.querySelector('[data-testid="permission-row-1001"]');
    expect(row).toBeTruthy();
    expect(row?.getAttribute('data-role')).toBe('viewer');
    // The snapshot only carries the id; the name comes from the search API.
    expect(target.querySelector('[data-testid="user-name-1001"]')?.textContent?.trim()).toBe(
      'Alice',
    );
    expect(target.querySelector('[data-testid="role-viewer-btn-1001"]')?.className).toContain(
      'filled',
    );

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0020 role 変更は WS ack 後にのみ確定する
  it('commits a role change only after the SetMemberPermission ack', async () => {
    let published: any = null;
    const instance = mountModal((table) => {
      published = table;
    });
    await tick(40);

    target.querySelector<HTMLButtonElement>('[data-testid="role-participant-btn-1001"]')?.click();
    await tick(40);

    expect(socket.requestSetMemberPermissionCalls).toEqual([
      { accountId: 1001, permission: 'Participant' },
    ]);
    expect(
      target.querySelector('[data-testid="permission-row-1001"]')?.getAttribute('data-role'),
    ).toBe('participant');
    expect(published).toMatchObject({
      defaultPermission: 'viewer',
      permissions: [{ account_id: 1001, role: 'participant' }],
    });
    expect(target.querySelector('[data-testid="permission-error"]')).toBeNull();

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0030 ack 失敗時は一覧を変えずエラーを出す
  it('keeps the previous row and shows the error when the permission frame is rejected', async () => {
    const instance = mountModal();
    await tick(40);

    socket.permissionsError = new ServerOperationError(
      'SetMemberPermissionFailed',
      'not an admin',
    );
    target.querySelector<HTMLButtonElement>('[data-testid="role-participant-btn-1001"]')?.click();
    await tick(40);

    expect(
      target.querySelector('[data-testid="permission-row-1001"]')?.getAttribute('data-role'),
    ).toBe('viewer');
    const error = target.querySelector('[data-testid="permission-error"]');
    expect(error?.textContent).toContain('ロールの変更に失敗しました');
    expect(error?.textContent).toContain('not an admin');

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0040 警告ダイアログ（拒否 / 管理者）
  it('warns before denying or promoting an account and commits on confirm', async () => {
    const instance = mountModal();
    await tick(40);

    target.querySelector<HTMLButtonElement>('[data-testid="role-denied-btn-1001"]')?.click();
    await tick(20);
    let dialog = target.querySelector('[data-testid="permission-warning-dialog"]');
    expect(dialog?.textContent).toContain('拒否');
    // Nothing was sent before the confirmation.
    expect(socket.requestSetMemberPermissionCalls).toEqual([]);

    target.querySelector<HTMLButtonElement>('[data-testid="warning-confirm-role-btn"]')?.click();
    await tick(40);
    expect(socket.requestSetMemberPermissionCalls).toEqual([
      { accountId: 1001, permission: 'Denied' },
    ]);
    expect(
      target.querySelector('[data-testid="permission-row-1001"]')?.getAttribute('data-role'),
    ).toBe('denied');

    target.querySelector<HTMLButtonElement>('[data-testid="role-admin-btn-1001"]')?.click();
    await tick(20);
    dialog = target.querySelector('[data-testid="permission-warning-dialog"]');
    expect(dialog?.textContent).toContain('管理者');
    target.querySelector<HTMLButtonElement>('[data-testid="warning-confirm-role-btn"]')?.click();
    await tick(40);
    expect(socket.requestSetMemberPermissionCalls.at(-1)).toEqual({
      accountId: 1001,
      permission: 'Admin',
    });

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0050 検索したアカウントを参加者として追加する
  it('adds a searched account as a participant after the ack', async () => {
    const instance = mountModal();
    await tick(40);

    const input = target.querySelector<HTMLInputElement>('[data-testid="account-search-input"]');
    expect(input).toBeTruthy();
    input!.value = 'Bo';
    input!.dispatchEvent(new Event('input'));
    await tick(300); // search debounce

    target.querySelector<HTMLButtonElement>('[data-testid="search-result-item-2002"]')?.click();
    await tick(40);

    expect(socket.requestSetMemberPermissionCalls).toEqual([
      { accountId: 2002, permission: 'Participant' },
    ]);
    const row = target.querySelector('[data-testid="permission-row-2002"]');
    expect(row?.getAttribute('data-role')).toBe('participant');
    expect(target.querySelector('[data-testid="user-name-2002"]')?.textContent?.trim()).toBe('Bob');

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0060 既定に戻す（明示的な権限の削除）
  it('resets an explicit entry back to the room default after the ack', async () => {
    socket.permissionsSnapshot = {
      default_permission: 'Viewer',
      permissions: { '1001': 'Admin' },
    };
    const instance = mountModal();
    await tick(40);

    const resetButton = target.querySelector<HTMLButtonElement>(
      '[data-testid="delete-permission-btn-1001"]',
    );
    expect(resetButton?.disabled).toBe(false);
    resetButton?.click();
    await tick(20);
    expect(
      target.querySelector('[data-testid="permission-warning-dialog"]')?.textContent,
    ).toContain('既定の権限');
    target.querySelector<HTMLButtonElement>('[data-testid="warning-confirm-delete-btn"]')?.click();
    await tick(40);

    expect(socket.requestSetMemberPermissionCalls).toEqual([
      { accountId: 1001, permission: 'Viewer' },
    ]);
    // The entry is now equal to the default, so there is nothing left to reset.
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-permission-btn-1001"]')
        ?.disabled,
    ).toBe(true);

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0070 既定の権限 selector は WS で送る
  it('sends SetDefaultPermission from the modal selector and shows the acked level', async () => {
    const instance = mountModal();
    await tick(40);

    expect(
      target.querySelector('[data-testid="modal-default-permission-btn-viewer"]')?.className,
    ).toContain('filled');

    target
      .querySelector<HTMLButtonElement>('[data-testid="modal-default-permission-btn-participant"]')
      ?.click();
    await tick(40);

    expect(socket.requestSetDefaultPermissionCalls).toEqual(['Participant']);
    expect(
      target.querySelector('[data-testid="modal-default-permission-btn-participant"]')?.className,
    ).toContain('filled');
    expect(target.querySelector('[data-testid="default-permission-error"]')).toBeNull();

    unmount(instance);
  });

  // spec: alhalte-argenterie/permissions 0080 既定の権限変更の失敗を表示する
  it('shows the error when the default permission frame is rejected', async () => {
    const instance = mountModal();
    await tick(40);

    socket.permissionsError = new ServerOperationError(
      'SetDefaultPermissionFailed',
      'last admin',
    );
    target
      .querySelector<HTMLButtonElement>('[data-testid="modal-default-permission-btn-participant"]')
      ?.click();
    await tick(40);

    const error = target.querySelector('[data-testid="default-permission-error"]');
    expect(error?.textContent).toContain('既定の権限の変更に失敗しました');
    // Nothing was committed: the selector still shows the old level.
    expect(
      target.querySelector('[data-testid="modal-default-permission-btn-viewer"]')?.className,
    ).toContain('filled');

    unmount(instance);
  });
});
