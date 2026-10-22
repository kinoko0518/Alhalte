<script lang="ts">
  /**
   * Account control for the Home app bar, the field top-right corner, and the
   * gate screen (Issue #116).
   *
   * - Signed in: shows the account avatar as an IconButton. The M3 Menu offers
   *   「別のアカウントでログイン」 and 「サインアウト」.
   * - Signed out: shows a Filled tonal 「サインイン」 button.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### アカウントUI`
   * - GitHub Issue #116: アカウントに関するUIを追加
   */
  import { Button, IconButton, Menu, MenuItem } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import {
    beginGoogleSignIn,
    getAccountProfile,
    isSignedIn,
    signOut as clearSession,
    type AccountProfile,
  } from '../../utils/account.js';
  import { getSavedAccountId } from '../../utils/session.js';
  import {
    applyMenuPosition,
    getMenuPositionStyle,
  } from '../../utils/menuPosition.js';

  let {
    baseUrl = '',
    account: externalAccount = null,
    signedIn: externalSignedIn,
    fetchFn,
    onSignIn,
    onSignOut,
    onSwitchAccount,
  }: {
    baseUrl?: string;
    account?: AccountProfile | null;
    signedIn?: boolean;
    fetchFn?: typeof fetch;
    onSignIn?: () => void;
    onSignOut?: () => void;
    onSwitchAccount?: () => void;
  } = $props();

  let signedIn = $state<boolean>(isSignedIn());
  let account = $state<AccountProfile | null>(null);
  let isMenuOpen = $state(false);
  let isBusy = $state(false);
  let authError = $state('');
  let menuButtonEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | undefined>(undefined);

  let accountInitial = $derived.by(() => {
    const name = account?.handle_name?.trim() ?? '';
    return name ? name.charAt(0).toUpperCase() : '?';
  });

  $effect(() => {
    if (externalSignedIn !== undefined) {
      signedIn = externalSignedIn;
    }
    if (externalAccount) {
      account = externalAccount;
    }
  });

  $effect(() => {
    if (!signedIn || account || externalAccount) return;
    const accountId = getSavedAccountId();
    if (!accountId) {
      signedIn = false;
      return;
    }

    let cancelled = false;
    getAccountProfile(accountId, { baseUrl, fetchFn })
      .then((profile) => {
        if (!cancelled) account = profile;
      })
      .catch(() => {
        if (!cancelled) authError = 'アカウント情報を取得できませんでした';
      });

    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    if (isMenuOpen && menuEl && menuButtonEl) {
      return applyMenuPosition(menuEl, menuButtonEl, {
        placement: 'bottom-end',
        anchorName: '--account-menu-anchor',
      });
    }
  });

  async function handleSignIn() {
    authError = '';
    if (onSignIn) {
      onSignIn();
      return;
    }

    isBusy = true;
    try {
      await beginGoogleSignIn({ baseUrl, prompt: 'select_account' });
    } catch {
      authError = 'サインインを開始できませんでした';
      isBusy = false;
    }
  }

  async function handleSwitchAccount() {
    isMenuOpen = false;
    authError = '';
    clearSession();
    signedIn = false;
    account = null;

    if (onSwitchAccount) {
      onSwitchAccount();
      return;
    }

    isBusy = true;
    try {
      await beginGoogleSignIn({ baseUrl, prompt: 'select_account' });
    } catch {
      authError = 'アカウントを切り替えできませんでした';
      isBusy = false;
    }
  }

  /**
   * Keeps presses inside the account control from reaching a pan/capture
   * ancestor (the field container captures the pointer for board panning, which
   * retargets `pointerup`/`click` away from the avatar). Mirrors
   * `FieldPiece.svelte`; harmless on the home app bar and the gate screen.
   *
   * References:
   * - GitHub Issue #268: フィールドパネルのアカウントアイコンからMenuが開かない
   */
  function handlePointerDown(e: PointerEvent) {
    e.stopPropagation();
  }

  function handleSignOut() {
    isMenuOpen = false;
    clearSession();
    signedIn = false;
    account = null;

    if (onSignOut) {
      onSignOut();
      return;
    }
    if (typeof window !== 'undefined') {
      window.location.reload();
    }
  }
</script>

<div
  class="account-menu-root"
  data-testid="account-controls"
  role="presentation"
  onpointerdown={handlePointerDown}
>
  {#if signedIn}
    <IconButton
      variant="text"
      shape="round"
      size="s"
      bind:element={menuButtonEl}
      onclick={() => (isMenuOpen = !isMenuOpen)}
      aria-label="アカウントメニュー"
      aria-haspopup="menu"
      aria-expanded={isMenuOpen}
      data-testid="account-menu-button"
    >
      {#if account?.icon_url}
        <img class="account-avatar" src={account.icon_url} alt="" />
      {:else if account}
        <span class="account-avatar account-avatar-initial" aria-hidden="true">
          {accountInitial}
        </span>
      {:else}
        <Icon>account_circle</Icon>
      {/if}
    </IconButton>

    {#if isMenuOpen}
      <Menu
        bind:element={menuEl}
        open={isMenuOpen}
        anchor={menuButtonEl}
        style={getMenuPositionStyle(menuButtonEl, {
          placement: 'bottom-end',
          anchorName: '--account-menu-anchor',
        })}
        ontoggle={(e) => {
          if (!e.newState || e.newState === 'closed') {
            isMenuOpen = false;
          }
        }}
        data-testid="account-menu"
        class="account-dropdown-menu"
      >
        <MenuItem onclick={handleSwitchAccount} data-testid="account-switch-item">
          {#snippet start()}<Icon>switch_account</Icon>{/snippet}
          別のアカウントでログイン
        </MenuItem>
        <MenuItem onclick={handleSignOut} data-testid="account-signout-item">
          {#snippet start()}<Icon>logout</Icon>{/snippet}
          サインアウト
        </MenuItem>
      </Menu>
    {/if}
  {:else}
    <Button
      variant="tonal"
      size="s"
      onclick={handleSignIn}
      disabled={isBusy}
      data-testid="account-signin-button"
    >
      サインイン
    </Button>
  {/if}

  {#if authError}
    <span class="account-error" role="alert">{authError}</span>
  {/if}
</div>

<style>
  .account-menu-root {
    position: relative;
    display: inline-flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    pointer-events: auto;
  }
  .account-avatar {
    width: 28px;
    height: 28px;
    border-radius: var(--np-shape-corner-full);
    object-fit: cover;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .account-avatar-initial {
    background: var(--np-color-primary-container);
    color: var(--np-color-on-primary-container);
    font-size: 0.85rem;
    font-weight: 700;
    line-height: 1;
  }
  .account-error {
    max-width: 220px;
    font-size: 0.72rem;
    line-height: 1.3;
    color: var(--np-color-error);
    text-align: right;
  }
  :global(.account-dropdown-menu) {
    min-width: 220px;
  }
</style>
