<script lang="ts">
  /**
   * Room Permission Management Modal (Issue #94 / #201).
   *
   * Features:
   * - Title: 「部屋の権限管理」
   * - The table itself is read and written over the room WebSocket
   *   (`QueryPermissions` / `SetMemberPermission` / `SetDefaultPermission`,
   *   Issue #201). Only account search stays on HTTP (`/api/accounts/search`).
   * - M3 Search to find accounts by handle name or numeric account ID.
   * - Search results display icon, name, and #(account_id).
   * - Selecting an account adds it to the Divider-separated list as
   *   参加者 (participant).
   * - Each row has a size "s" connected button group with the four levels
   *   (拒否 / 閲覧者 / 参加者 / 管理者) and a "既定に戻す" button.
   * - Default permission selector for accounts without an explicit entry.
   * - Every mutation is applied only after the banquette ack (never
   *   optimistically): the row list is replaced by the acknowledged table.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
   * - GitHub Issue #94: アカウントの概念をArgenterieと接続
   * - GitHub Issue #201: 4 段階権限を WebSocket で変更する
   */
  import { Button, ButtonGroup, Divider, IconButton, Item, Search } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type {
    AccountSearchItem,
    RoomPermissionItem,
    RoomPermissionsSnapshot,
  } from '../../types/index.js';
  import {
    ROOM_ROLES,
    ROOM_ROLE_LABELS,
    permissionsSnapshotToTable,
    roleToPermissionLevel,
    searchAccounts,
    type RoomRole,
  } from '../../utils/session.js';
  import type { PermissionsAck, RoomSocket } from '../../utils/roomSocket.js';

  let {
    roomSocket,
    isOpen = true,
    fetchFn,
    baseUrl = '',
    onClose,
    onPermissionsChange,
  }: {
    roomSocket?: RoomSocket | null;
    isOpen?: boolean;
    fetchFn?: typeof fetch;
    baseUrl?: string;
    onClose?: () => void;
    /**
     * Called with the acknowledged table after every query / mutation so `App`
     * can follow a change of its own role and the header selector.
     */
    onPermissionsChange?: (table: RoomPermissionsSnapshot) => void;
  } = $props();

  /** Timeout for a permission frame (matches `RoomSocket`'s default). */
  const PERMISSION_TIMEOUT_MS = 8000;

  let permissions = $state<RoomPermissionItem[]>([]);
  let defaultPermission = $state<RoomRole>('viewer');
  let isLoading = $state(false);
  let isMutating = $state(false);
  let pendingAction = $state<string | null>(null);
  let permissionError = $state<string | null>(null);
  let defaultPermissionError = $state<string | null>(null);

  let searchQuery = $state('');
  let isSearchExpanded = $state(false);
  let searchResults = $state<AccountSearchItem[]>([]);
  let isSearching = $state(false);
  let searchDebounceTimer: any = null;

  /** Formats a failed permission mutation for the error banner. */
  function permissionErrorMessage(action: string, err: unknown): string {
    const detail = err instanceof Error ? err.message : String(err);
    return `${action}に失敗しました: ${detail}`;
  }

  /** Resolves one account ID to its display name through the search API. */
  async function lookupAccount(accountId: number): Promise<AccountSearchItem | null> {
    try {
      const results = await searchAccounts(String(accountId), { baseUrl, fetchFn });
      return results.find((item) => item.account_id === accountId) ?? null;
    } catch (err) {
      // The permission table only carries IDs; an unresolved name falls back to
      // `#(account_id)` instead of failing the whole table.
      console.warn('Failed to resolve the account name:', err);
      return null;
    }
  }

  /**
   * Fills in the display name of every row.
   *
   * `PermissionsSnapshot` only carries account IDs, so names come from
   * `/api/accounts/search?q=<id>` (and from the previous list when known).
   */
  async function resolveNames(items: RoomPermissionItem[]): Promise<RoomPermissionItem[]> {
    const known = new Map(permissions.map((item) => [item.account_id, item]));
    return Promise.all(
      items.map(async (item) => {
        if (item.username) return item;
        const cached = known.get(item.account_id);
        if (cached?.username) {
          return { ...item, username: cached.username, icon_url: cached.icon_url };
        }
        const account = await lookupAccount(item.account_id);
        return account
          ? { ...item, username: account.username, icon_url: account.icon_url }
          : item;
      }),
    );
  }

  /** Replaces the local table with an acknowledged one (names resolved). */
  async function applyTable(table: RoomPermissionsSnapshot): Promise<void> {
    const resolved = await resolveNames(table.permissions);
    permissions = resolved;
    defaultPermission = table.defaultPermission;
    onPermissionsChange?.({ defaultPermission: table.defaultPermission, permissions: resolved });
  }

  // Query the table every time the modal opens (or the socket is replaced).
  $effect(() => {
    if (isOpen && roomSocket) {
      void loadPermissions();
    }
  });

  async function loadPermissions(): Promise<void> {
    if (!roomSocket) {
      permissionError = 'ルームに接続していません';
      return;
    }
    isLoading = true;
    permissionError = null;
    try {
      const ack = await roomSocket.requestQueryPermissions(PERMISSION_TIMEOUT_MS);
      await applyTable(permissionsSnapshotToTable(ack));
    } catch (err) {
      permissionError = permissionErrorMessage('権限一覧の取得', err);
    } finally {
      isLoading = false;
    }
  }

  /**
   * Sends one permission frame and applies the acknowledged table.
   *
   * The local list is never changed before the ack: a rejection (non-admin,
   * last-admin demotion, timeout) leaves the previous table in place and shows
   * the error banner.
   */
  async function mutate(
    key: string,
    action: string,
    send: () => Promise<PermissionsAck>,
  ): Promise<boolean> {
    if (!roomSocket) {
      permissionError = 'ルームに接続していません';
      return false;
    }
    if (isMutating) return false;
    isMutating = true;
    pendingAction = key;
    permissionError = null;
    defaultPermissionError = null;
    try {
      const ack = await send();
      await applyTable(permissionsSnapshotToTable(ack));
      return true;
    } catch (err) {
      permissionError = permissionErrorMessage(action, err);
      console.warn('Failed to update the room permission table:', err);
      return false;
    } finally {
      isMutating = false;
      pendingAction = null;
    }
  }

  // Reactively search accounts when searchQuery changes
  $effect(() => {
    const q = searchQuery.trim();
    if (searchDebounceTimer) {
      clearTimeout(searchDebounceTimer);
    }
    if (!q) {
      searchResults = [];
      isSearchExpanded = false;
      return;
    }
    isSearching = true;
    searchDebounceTimer = setTimeout(async () => {
      try {
        const results = await searchAccounts(q, { baseUrl, fetchFn });
        searchResults = results;
        isSearchExpanded = results.length > 0;
      } catch (err) {
        console.warn('Failed to search accounts:', err);
        searchResults = [];
      } finally {
        isSearching = false;
      }
    }, 150);

    return () => {
      if (searchDebounceTimer) {
        clearTimeout(searchDebounceTimer);
      }
    };
  });

  // Active warning dialog state
  type PendingWarning =
    | { type: 'reset'; account: RoomPermissionItem }
    | { type: 'role-change'; account: RoomPermissionItem; targetRole: RoomRole }
    | { type: 'default-change'; targetRole: RoomRole }
    | null;

  let activeWarning = $state<PendingWarning>(null);

  /** True when a level change is loud enough to need a confirmation dialog. */
  function needsWarning(role: RoomRole): boolean {
    return role === 'admin' || role === 'denied' || role === 'viewer';
  }

  function roleWarningCopy(role: RoomRole): { title: string; message: string; icon: string } {
    if (role === 'admin') {
      return {
        title: 'ロールを「管理者」に切り替えますか？',
        message:
          '管理者は部屋の権限管理・シーン管理・拡張機能管理など、すべての運営操作を行えます。',
        icon: 'security',
      };
    }
    if (role === 'denied') {
      return {
        title: 'このアカウントを「拒否」にしますか？',
        message:
          '拒否されたアカウントは部屋に入室できなくなります。ログや状態の受信も行えません。',
        icon: 'block',
      };
    }
    return {
      title: 'ロールを「閲覧者」に切り替えますか？',
      message:
        '閲覧者は部屋の情報やログを閲覧できますが、メッセージの送信や部屋の状態変更（コマ移動・シーン更新など）は行えなくなります。',
      icon: 'visibility',
    };
  }

  // Select account from search candidate to add to permissions.
  async function handleSelectAccount(account: AccountSearchItem) {
    if (permissions.some((p) => p.account_id === account.account_id)) {
      searchQuery = '';
      isSearchExpanded = false;
      return;
    }

    const ok = await mutate(
      `add:${account.account_id}`,
      '権限の追加',
      () => roomSocket!.requestSetMemberPermission(
        account.account_id,
        roleToPermissionLevel('participant'),
        PERMISSION_TIMEOUT_MS,
      ),
    );
    if (!ok) return;

    // The snapshot only carries IDs; keep the name the search already resolved.
    permissions = permissions.map((item) =>
      item.account_id === account.account_id
        ? { ...item, username: account.username, icon_url: account.icon_url }
        : item,
    );
    searchQuery = '';
    isSearchExpanded = false;
  }

  // Role button clicked in connected button group
  function handleRequestRoleChange(item: RoomPermissionItem, newRole: RoomRole) {
    if (item.role === newRole) return;
    if (needsWarning(newRole)) {
      activeWarning = { type: 'role-change', account: item, targetRole: newRole };
      return;
    }
    void applyRoleChange(item.account_id, newRole);
  }

  async function applyRoleChange(accountId: number, role: RoomRole): Promise<void> {
    await mutate(`role:${accountId}`, 'ロールの変更', () =>
      roomSocket!.requestSetMemberPermission(
        accountId,
        roleToPermissionLevel(role),
        PERMISSION_TIMEOUT_MS,
      ),
    );
  }

  // Reset button clicked (drops the explicit entry back to the room default)
  function handleRequestDelete(item: RoomPermissionItem) {
    activeWarning = { type: 'reset', account: item };
  }

  function handleRequestDefaultPermission(role: RoomRole) {
    if (role === defaultPermission) return;
    if (role === 'admin' || role === 'denied') {
      activeWarning = { type: 'default-change', targetRole: role };
      return;
    }
    void applyDefaultPermission(role);
  }

  async function applyDefaultPermission(role: RoomRole): Promise<void> {
    if (!roomSocket) {
      defaultPermissionError = 'ルームに接続していません';
      return;
    }
    if (isMutating) return;
    isMutating = true;
    pendingAction = `default:${role}`;
    defaultPermissionError = null;
    try {
      const ack = await roomSocket.requestSetDefaultPermission(
        roleToPermissionLevel(role),
        PERMISSION_TIMEOUT_MS,
      );
      await applyTable(permissionsSnapshotToTable(ack));
    } catch (err) {
      defaultPermissionError = permissionErrorMessage('既定の権限の変更', err);
      console.warn('Failed to update the default room permission:', err);
    } finally {
      isMutating = false;
      pendingAction = null;
    }
  }

  /** Resets an account to the room default permission (the ack is authoritative). */
  async function applyReset(item: RoomPermissionItem): Promise<void> {
    await mutate(`reset:${item.account_id}`, '権限の削除', () =>
      roomSocket!.requestSetMemberPermission(
        item.account_id,
        roleToPermissionLevel(defaultPermission),
        PERMISSION_TIMEOUT_MS,
      ),
    );
  }

  async function confirmWarning() {
    if (!activeWarning) return;
    const warning = activeWarning;
    activeWarning = null;
    if (warning.type === 'reset') {
      await applyReset(warning.account);
    } else if (warning.type === 'role-change') {
      await applyRoleChange(warning.account.account_id, warning.targetRole);
    } else {
      await applyDefaultPermission(warning.targetRole);
    }
  }

  function cancelWarning() {
    activeWarning = null;
  }

  /** Display name of an account with the `#id` fallback. */
  function displayName(item: RoomPermissionItem): string {
    return item.username && item.username.length > 0 ? item.username : `#${item.account_id}`;
  }
</script>

{#if isOpen}
  <div
    class="room-permission-overlay"
    data-testid="room-permission-overlay"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose?.();
    }}
  >
    <div
      class="room-permission-dialog"
      data-testid="room-permission-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="room-permission-title"
    >
      <!-- Header -->
      <div class="permission-dialog-header">
        <div class="header-title-row">
          <div class="header-icon-circle">
            <Icon>admin_panel_settings</Icon>
          </div>
          <h2 id="room-permission-title" class="dialog-title" data-testid="room-permission-title">
            部屋の権限管理
          </h2>
        </div>
        <IconButton
          variant="text"
          size="s"
          onclick={onClose}
          aria-label="閉じる"
          data-testid="close-permission-dialog-btn"
        >
          <Icon>close</Icon>
        </IconButton>
      </div>

      <!-- Account Search using M3 Search -->
      <div class="search-section" data-testid="account-search-section">
        <Search
          variant="contained"
          view="docked"
          placeholder="アカウントを検索 (名前またはID)"
          bind:value={searchQuery}
          bind:expanded={isSearchExpanded}
          class="account-search-bar"
          inputAttributes={{
            'data-testid': 'account-search-input',
            'aria-label': 'アカウントを検索',
            oninput: (e) => {
              searchQuery = (e.target as HTMLInputElement).value;
            },
          }}
          resultsAttributes={{
            'data-testid': 'account-search-results',
            class: 'account-search-results',
          }}
        >
          {#snippet leading()}
            <Icon>search</Icon>
          {/snippet}

          {#if searchResults.length > 0}
            {#each searchResults as item (item.account_id)}
              <Item
                type="button"
                role="option"
                onclick={() => handleSelectAccount(item)}
                data-testid={`search-result-item-${item.account_id}`}
                class="search-candidate-item"
              >
                {#snippet start()}
                  <div class="account-avatar-small" aria-hidden="true">
                    {#if item.icon_url}
                      <img src={item.icon_url} alt="" class="avatar-img" />
                    {:else}
                      <Icon>account_circle</Icon>
                    {/if}
                  </div>
                {/snippet}
                <div class="account-candidate-info">
                  <span class="account-candidate-name">{item.username}</span>
                  <span class="account-candidate-id" data-testid={`candidate-id-${item.account_id}`}>
                    #{item.account_id}
                  </span>
                </div>
              </Item>
            {/each}
          {:else if isSearching}
            <div class="search-notice" data-testid="search-loading-notice">検索中...</div>
          {:else if searchQuery.trim().length > 0}
            <div class="search-notice" data-testid="search-empty-notice">該当するアカウントが見つかりません</div>
          {/if}
        </Search>
      </div>

      <!-- Default permission selector (accounts without an explicit entry) -->
      <div class="default-permission-section" data-testid="default-permission-section">
        <div class="default-permission-label">
          <span>既定の権限</span>
          <span class="default-permission-hint">明示的に設定されていないアカウントに適用されます</span>
        </div>
        <ButtonGroup
          variant="connected"
          size="s"
          data-testid="modal-default-permission-group"
          aria-label="既定の権限"
        >
          {#each ROOM_ROLES as role (role)}
            <Button
              size="s"
              variant={defaultPermission === role ? 'filled' : 'outlined'}
              onclick={() => handleRequestDefaultPermission(role)}
              disabled={isMutating}
              data-testid={`modal-default-permission-btn-${role}`}
              data-role={role}
            >
              {ROOM_ROLE_LABELS[role]}
            </Button>
          {/each}
        </ButtonGroup>
      </div>

      {#if defaultPermissionError}
        <div
          class="permission-error-banner"
          data-testid="default-permission-error"
          role="alert"
        >
          <Icon>error</Icon>
          <span>{defaultPermissionError}</span>
        </div>
      {/if}

      <!-- Persistence failure banner (the list is unchanged on failure) -->
      {#if permissionError}
        <div
          class="permission-error-banner"
          data-testid="permission-error"
          role="alert"
        >
          <Icon>error</Icon>
          <span>{permissionError}</span>
        </div>
      {/if}

      <!-- Divider-separated Permissions List -->
      <div class="permissions-list-container" data-testid="permissions-list">
        {#if isLoading && permissions.length === 0}
          <div class="empty-permissions" data-testid="permission-loading" role="status">
            <Icon class="empty-icon">hourglass_top</Icon>
            <p>権限一覧を読み込んでいます…</p>
          </div>
        {:else if permissions.length === 0}
          <div class="empty-permissions" data-testid="empty-permissions-notice">
            <Icon class="empty-icon">group_off</Icon>
            <p>権限が設定されているアカウントはありません</p>
            <span class="empty-subtext">上の検索バーからアカウントを追加してください</span>
          </div>
        {:else}
          {#each permissions as perm, index (perm.account_id)}
            {#if index > 0}
              <Divider />
            {/if}
            <div
              class="permission-row"
              data-testid={`permission-row-${perm.account_id}`}
              data-role={perm.role}
              data-pending={pendingAction === `role:${perm.account_id}` ? 'true' : 'false'}
            >
              <!-- Left: Avatar, Username, #(account_id) -->
              <div class="user-info-col">
                <div class="user-avatar" aria-hidden="true">
                  {#if perm.icon_url}
                    <img src={perm.icon_url} alt="" class="avatar-img" />
                  {:else}
                    <Icon>person</Icon>
                  {/if}
                </div>
                <div class="user-name-group">
                  <span class="user-name" data-testid={`user-name-${perm.account_id}`}>
                    {displayName(perm)}
                  </span>
                  <span class="user-id" data-testid={`user-id-${perm.account_id}`}>
                    #{perm.account_id}
                  </span>
                </div>
              </div>

              <!-- Right: Connected Button Group (size="s") & Reset Button -->
              <div class="actions-col">
                <ButtonGroup
                  variant="connected"
                  size="s"
                  data-testid={`role-button-group-${perm.account_id}`}
                  aria-label={`${displayName(perm)} の権限`}
                >
                  {#each ROOM_ROLES as role (role)}
                    <Button
                      size="s"
                      variant={perm.role === role ? 'filled' : 'outlined'}
                      onclick={() => handleRequestRoleChange(perm, role)}
                      disabled={isMutating}
                      data-testid={`role-${role}-btn-${perm.account_id}`}
                      data-role={role}
                    >
                      {ROOM_ROLE_LABELS[role]}
                    </Button>
                  {/each}
                </ButtonGroup>

                <IconButton
                  variant="tonal"
                  size="s"
                  onclick={() => handleRequestDelete(perm)}
                  disabled={isMutating || perm.role === defaultPermission}
                  aria-label={`アカウント ${displayName(perm)} の権限を既定に戻す`}
                  title="既定の権限に戻す"
                  data-testid={`delete-permission-btn-${perm.account_id}`}
                  class="permission-delete-btn"
                >
                  <Icon>delete</Icon>
                </IconButton>
              </div>
            </div>
          {/each}
        {/if}
      </div>

      <!-- Footer Action -->
      <div class="permission-dialog-footer">
        <Button variant="text" onclick={onClose} data-testid="done-permission-dialog-btn">
          閉じる
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- Fullscreen Warning Alert Dialog (reset / viewer / denied / admin) -->
{#if activeWarning}
  <div
    class="fullscreen-warning-overlay"
    data-testid="permission-warning-overlay"
    onclick={(e) => {
      if (e.target === e.currentTarget) cancelWarning();
    }}
  >
    <div
      class="warning-dialog-box"
      data-testid="permission-warning-dialog"
      role="alertdialog"
      aria-modal="true"
    >
      {#if activeWarning.type === 'reset'}
        <div class="warning-header-icon error-icon" aria-hidden="true">
          <Icon>warning</Icon>
        </div>
        <h3 class="warning-title error-text" data-testid="warning-title">
          アカウント権限の削除
        </h3>
        <p class="warning-message" data-testid="warning-message">
          「{displayName(activeWarning.account)} (#{activeWarning.account.account_id})」の明示的な権限を削除し、既定の権限「{ROOM_ROLE_LABELS[defaultPermission]}」に戻しますか？
        </p>
        <div class="warning-actions">
          <Button variant="text" onclick={cancelWarning} data-testid="warning-cancel-btn">
            キャンセル
          </Button>
          <div class="warning-destructive-wrapper">
            <Button
              variant="filled"
              onclick={confirmWarning}
              data-testid="warning-confirm-delete-btn"
            >
              削除する
            </Button>
          </div>
        </div>
      {:else if activeWarning.type === 'role-change'}
        <div
          class="warning-header-icon {activeWarning.targetRole === 'admin'
            ? 'primary-icon'
            : activeWarning.targetRole === 'denied'
              ? 'error-icon'
              : 'tertiary-icon'}"
          aria-hidden="true"
        >
          <Icon>{roleWarningCopy(activeWarning.targetRole).icon}</Icon>
        </div>
        <h3 class="warning-title" data-testid="warning-title">
          {roleWarningCopy(activeWarning.targetRole).title}
        </h3>
        <p class="warning-message" data-testid="warning-message">
          「{displayName(activeWarning.account)}」の権限を「{ROOM_ROLE_LABELS[activeWarning.targetRole]}」に変更します。<br />
          {roleWarningCopy(activeWarning.targetRole).message}
        </p>
        <div class="warning-actions">
          <Button variant="text" onclick={cancelWarning} data-testid="warning-cancel-btn">
            キャンセル
          </Button>
          <Button variant="filled" onclick={confirmWarning} data-testid="warning-confirm-role-btn">
            {ROOM_ROLE_LABELS[activeWarning.targetRole]}に変更する
          </Button>
        </div>
      {:else if activeWarning.type === 'default-change'}
        <div
          class="warning-header-icon {activeWarning.targetRole === 'admin' ? 'primary-icon' : 'error-icon'}"
          aria-hidden="true"
        >
          <Icon>{activeWarning.targetRole === 'admin' ? 'security' : 'block'}</Icon>
        </div>
        <h3 class="warning-title" data-testid="warning-title">
          既定の権限を「{ROOM_ROLE_LABELS[activeWarning.targetRole]}」に切り替えますか？
        </h3>
        <p class="warning-message" data-testid="warning-message">
          明示的に設定されていないすべてのアカウント（未サインインを含む）に適用されます。<br />
          {roleWarningCopy(activeWarning.targetRole).message}
        </p>
        <div class="warning-actions">
          <Button variant="text" onclick={cancelWarning} data-testid="warning-cancel-btn">
            キャンセル
          </Button>
          <Button variant="filled" onclick={confirmWarning} data-testid="warning-confirm-default-btn">
            {ROOM_ROLE_LABELS[activeWarning.targetRole]}に変更する
          </Button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .room-permission-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.4));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 9500;
  }
  .room-permission-dialog {
    background: var(--np-color-surface-container-high);
    color: var(--np-color-on-surface);
    border-radius: var(--np-shape-corner-extra-large, 28px);
    width: 92%;
    max-width: 720px;
    max-height: 85vh;
    box-shadow: var(--np-elevation-3);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-sizing: border-box;
  }
  .permission-dialog-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 24px 16px;
    border-bottom: 1px solid var(--np-color-outline-variant);
  }
  .header-title-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .header-icon-circle {
    width: 36px;
    height: 36px;
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-primary-container);
    color: var(--np-color-on-primary-container);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .dialog-title {
    margin: 0;
    font-size: 1.35rem;
    font-weight: 700;
    color: var(--np-color-on-surface);
  }
  .search-section {
    padding: 16px 24px 8px;
    position: relative;
    z-index: 10;
  }
  :global(.account-search-bar) {
    width: 100%;
  }
  :global(.account-search-results) {
    max-height: 220px;
    overflow-y: auto;
    border: 1px solid var(--np-color-outline-variant);
    box-shadow: var(--np-elevation-2);
  }
  .search-candidate-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 16px;
  }
  .account-avatar-small {
    width: 28px;
    height: 28px;
    border-radius: var(--np-shape-corner-full, 50%);
    background: var(--np-color-secondary-container);
    color: var(--np-color-on-secondary-container);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .avatar-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .account-candidate-info {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .account-candidate-name {
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--np-color-on-surface);
  }
  .account-candidate-id {
    font-size: 0.8rem;
    color: var(--np-color-on-surface-variant);
    font-family: monospace;
  }
  .search-notice {
    padding: 12px 16px;
    text-align: center;
    font-size: 0.85rem;
    color: var(--np-color-on-surface-variant);
  }
  .default-permission-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 4px 24px 12px;
    border-bottom: 1px solid var(--np-color-outline-variant);
  }
  .default-permission-label {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
  }
  .default-permission-hint {
    font-size: 0.75rem;
    font-weight: 400;
    color: var(--np-color-on-surface-variant);
  }
  .permission-error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px 24px 0;
    padding: 10px 12px;
    border-radius: var(--np-shape-corner-small, 8px);
    background: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
    font-size: 0.85rem;
    line-height: 1.4;
  }
  .permissions-list-container {
    flex: 1;
    overflow-y: auto;
    padding: 8px 24px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-height: 160px;
  }
  .empty-permissions {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 36px 16px;
    text-align: center;
    color: var(--np-color-on-surface-variant);
    gap: 8px;
  }
  :global(.empty-icon) {
    font-size: 40px !important;
    opacity: 0.7;
    margin-bottom: 4px;
  }
  .empty-subtext {
    font-size: 0.82rem;
    opacity: 0.8;
  }
  .permission-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 0;
    gap: 12px;
  }
  .user-info-col {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    flex: 1;
  }
  .user-avatar {
    width: 36px;
    height: 36px;
    border-radius: var(--np-shape-corner-full, 50%);
    background: var(--np-color-primary-container);
    color: var(--np-color-on-primary-container);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    overflow: hidden;
  }
  .user-name-group {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .user-name {
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--np-color-on-surface);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .user-id {
    font-size: 0.8rem;
    color: var(--np-color-on-surface-variant);
    font-family: monospace;
    flex-shrink: 0;
  }
  .actions-col {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  :global(.permission-delete-btn) {
    color: var(--np-color-error) !important;
  }
  .permission-dialog-footer {
    display: flex;
    justify-content: flex-end;
    padding: 12px 24px 16px;
    border-top: 1px solid var(--np-color-outline-variant);
  }

  /* Fullscreen Warning Overlay */
  .fullscreen-warning-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.45));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10000;
  }
  .warning-dialog-box {
    background: var(--np-color-surface-container-high);
    color: var(--np-color-on-surface);
    border-radius: var(--np-shape-corner-extra-large, 28px);
    padding: 28px;
    width: 90%;
    max-width: 480px;
    box-shadow: var(--np-elevation-3);
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 16px;
    box-sizing: border-box;
  }
  .warning-header-icon {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .error-icon {
    background: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
  }
  .tertiary-icon {
    background: var(--np-color-tertiary-container);
    color: var(--np-color-on-tertiary-container);
  }
  .primary-icon {
    background: var(--np-color-primary-container);
    color: var(--np-color-on-primary-container);
  }
  .warning-title {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 700;
    color: var(--np-color-on-surface);
  }
  .error-text {
    color: var(--np-color-error);
  }
  .warning-message {
    margin: 0;
    font-size: 0.92rem;
    line-height: 1.5;
    color: var(--np-color-on-surface-variant);
  }
  .warning-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    width: 100%;
    margin-top: 8px;
  }
  .warning-destructive-wrapper {
    --np-button-container-color: var(--np-color-error);
    --np-button-label-color: var(--np-color-on-error);
  }
</style>
