<script lang="ts">
  /**
   * Room Permission Management Modal (Issue #94).
   *
   * Features:
   * - Title: 「部屋の権限管理」
   * - M3 Search to query accounts by handle name or numeric account ID.
   * - Search results display icon, name, and #(account_id).
   * - Selecting an account adds it to the Divider-separated list.
   * - Each row has:
   *   - Avatar icon, username, #(account_id)
   *   - Size "s" connected button group (ButtonGroup variant="connected") toggling "閲覧者", "PL", "GM"
   *   - Remove button (Button or IconButton with delete icon)
   * - Full-screen warning dialog on:
   *   - Remove button click
   *   - Switch to "閲覧者" (Viewer)
   *   - Switch to "GM"
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-maitre-d -> ### 部屋権限管理API`
   * - GitHub Issue #94: アカウントの概念をArgenterieと接続
   */
  import { Button, ButtonGroup, Divider, IconButton, Item, Search } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { AccountSearchItem, RoomPermissionItem, RoomRole } from '../../types/index.js';
  import {
    deleteRoomPermission,
    getRoomPermissions,
    searchAccounts,
    setRoomPermission,
  } from '../../utils/session.js';

  let {
    roomId,
    initialPermissions = [],
    isOpen = true,
    fetchFn,
    baseUrl = '',
    onClose,
    onPermissionsChange,
  }: {
    roomId: string | number;
    initialPermissions?: RoomPermissionItem[];
    isOpen?: boolean;
    fetchFn?: typeof fetch;
    baseUrl?: string;
    onClose?: () => void;
    onPermissionsChange?: (permissions: RoomPermissionItem[]) => void;
  } = $props();

  let permissions = $state<RoomPermissionItem[]>([...initialPermissions]);
  let searchQuery = $state('');
  let isSearchExpanded = $state(false);
  let searchResults = $state<AccountSearchItem[]>([]);
  let isSearching = $state(false);
  let searchDebounceTimer: any = null;

  // Active warning dialog state
  type PendingWarning =
    | {
        type: 'delete';
        account: RoomPermissionItem;
      }
    | {
        type: 'role-change';
        account: RoomPermissionItem;
        targetRole: 'viewer' | 'gm';
        previousRole: RoomRole;
      }
    | null;

  let activeWarning = $state<PendingWarning>(null);

  // Load existing permissions on mount
  $effect(() => {
    if (initialPermissions.length === 0 && roomId) {
      loadPermissions();
    }
  });

  async function loadPermissions() {
    try {
      const list = await getRoomPermissions(roomId, { baseUrl, fetchFn });
      permissions = list;
      onPermissionsChange?.(list);
    } catch (err) {
      // If error or unauthenticated, maintain current list
      console.warn('Failed to load room permissions:', err);
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

  // Select account from search candidate to add to permissions
  async function handleSelectAccount(account: AccountSearchItem) {
    // If account already exists in permissions, ignore or focus
    if (permissions.some((p) => p.account_id === account.account_id)) {
      searchQuery = '';
      isSearchExpanded = false;
      return;
    }

    const defaultRole: RoomRole = 'pl';
    const newItem: RoomPermissionItem = {
      account_id: account.account_id,
      username: account.username,
      icon_url: account.icon_url,
      role: defaultRole,
    };

    permissions = [...permissions, newItem];
    searchQuery = '';
    isSearchExpanded = false;

    try {
      await setRoomPermission(roomId, account.account_id, defaultRole, { baseUrl, fetchFn });
    } catch (err) {
      console.warn('Failed to persist room permission:', err);
    }
    onPermissionsChange?.(permissions);
  }

  // Role button clicked in connected button group
  function handleRequestRoleChange(item: RoomPermissionItem, newRole: RoomRole) {
    if (item.role === newRole) return;

    if (newRole === 'viewer' || newRole === 'gm') {
      // Issue warning dialog
      activeWarning = {
        type: 'role-change',
        account: item,
        targetRole: newRole,
        previousRole: item.role,
      };
    } else {
      // Direct change to PL
      applyRoleChange(item.account_id, newRole);
    }
  }

  async function applyRoleChange(accountId: number, role: RoomRole) {
    permissions = permissions.map((p) => (p.account_id === accountId ? { ...p, role } : p));
    try {
      await setRoomPermission(roomId, accountId, role, { baseUrl, fetchFn });
    } catch (err) {
      console.warn('Failed to update room permission:', err);
    }
    onPermissionsChange?.(permissions);
  }

  // Delete button clicked
  function handleRequestDelete(item: RoomPermissionItem) {
    activeWarning = {
      type: 'delete',
      account: item,
    };
  }

  async function confirmWarning() {
    if (!activeWarning) return;
    if (activeWarning.type === 'delete') {
      const aid = activeWarning.account.account_id;
      permissions = permissions.filter((p) => p.account_id !== aid);
      try {
        await deleteRoomPermission(roomId, aid, { baseUrl, fetchFn });
      } catch (err) {
        console.warn('Failed to delete room permission:', err);
      }
      onPermissionsChange?.(permissions);
    } else if (activeWarning.type === 'role-change') {
      await applyRoleChange(activeWarning.account.account_id, activeWarning.targetRole);
    }
    activeWarning = null;
  }

  function cancelWarning() {
    activeWarning = null;
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

      <!-- Divider-separated Permissions List -->
      <div class="permissions-list-container" data-testid="permissions-list">
        {#if permissions.length === 0}
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
            <div class="permission-row" data-testid={`permission-row-${perm.account_id}`}>
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
                    {perm.username}
                  </span>
                  <span class="user-id" data-testid={`user-id-${perm.account_id}`}>
                    #{perm.account_id}
                  </span>
                </div>
              </div>

              <!-- Right: Connected Button Group (size="s") & Delete Button -->
              <div class="actions-col">
                <ButtonGroup
                  variant="connected"
                  size="s"
                  selection="single"
                  value={perm.role}
                  data-testid={`role-button-group-${perm.account_id}`}
                >
                  <Button
                    size="s"
                    variant={perm.role === 'viewer' ? 'filled' : 'outlined'}
                    value="viewer"
                    onclick={() => handleRequestRoleChange(perm, 'viewer')}
                    data-testid={`role-viewer-btn-${perm.account_id}`}
                  >
                    閲覧者
                  </Button>
                  <Button
                    size="s"
                    variant={perm.role === 'pl' ? 'filled' : 'outlined'}
                    value="pl"
                    onclick={() => handleRequestRoleChange(perm, 'pl')}
                    data-testid={`role-pl-btn-${perm.account_id}`}
                  >
                    PL
                  </Button>
                  <Button
                    size="s"
                    variant={perm.role === 'gm' ? 'filled' : 'outlined'}
                    value="gm"
                    onclick={() => handleRequestRoleChange(perm, 'gm')}
                    data-testid={`role-gm-btn-${perm.account_id}`}
                  >
                    GM
                  </Button>
                </ButtonGroup>

                <IconButton
                  variant="tonal"
                  size="s"
                  onclick={() => handleRequestDelete(perm)}
                  aria-label={`アカウント ${perm.username} を権限から削除`}
                  title="削除"
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

<!-- Fullscreen Warning Alert Dialog (for Delete, Viewer, GM confirmation) -->
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
      {#if activeWarning.type === 'delete'}
        <div class="warning-header-icon error-icon" aria-hidden="true">
          <Icon>warning</Icon>
        </div>
        <h3 class="warning-title error-text" data-testid="warning-title">
          アカウント権限の削除
        </h3>
        <p class="warning-message" data-testid="warning-message">
          「{activeWarning.account.username} (#{activeWarning.account.account_id})」の権限をリストから削除しますか？<br />
          全体公開されていない部屋の場合、このアカウントは入室できなくなります。
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
      {:else if activeWarning.type === 'role-change' && activeWarning.targetRole === 'viewer'}
        <div class="warning-header-icon tertiary-icon" aria-hidden="true">
          <Icon>visibility</Icon>
        </div>
        <h3 class="warning-title" data-testid="warning-title">
          ロールを「閲覧者」に切り替えますか？
        </h3>
        <p class="warning-message" data-testid="warning-message">
          「{activeWarning.account.username}」のロールを「閲覧者」に変更します。<br />
          閲覧者は部屋の情報やログを受信・閲覧できますが、<strong>メッセージの送信や部屋の状態変更（コマ移動・シーン更新等）を行うことはできなくなります</strong>。
        </p>
        <div class="warning-actions">
          <Button variant="text" onclick={cancelWarning} data-testid="warning-cancel-btn">
            キャンセル
          </Button>
          <Button variant="filled" onclick={confirmWarning} data-testid="warning-confirm-role-btn">
            閲覧者に変更する
          </Button>
        </div>
      {:else if activeWarning.type === 'role-change' && activeWarning.targetRole === 'gm'}
        <div class="warning-header-icon primary-icon" aria-hidden="true">
          <Icon>security</Icon>
        </div>
        <h3 class="warning-title" data-testid="warning-title">
          ロールを「GM（管理者）」に切り替えますか？
        </h3>
        <p class="warning-message" data-testid="warning-message">
          「{activeWarning.account.username}」にGM権限を付与します。<br />
          GMは<strong>部屋の権限管理、シーン削除、部屋の全体公開など、すべての管理者機能へのアクセスが可能になります</strong>。
        </p>
        <div class="warning-actions">
          <Button variant="text" onclick={cancelWarning} data-testid="warning-cancel-btn">
            キャンセル
          </Button>
          <Button variant="filled" onclick={confirmWarning} data-testid="warning-confirm-role-btn">
            GMに変更する
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
    max-width: 640px;
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
    --np-button-label-text-color: var(--np-color-on-error);
  }
</style>
