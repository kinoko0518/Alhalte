<script lang="ts">
  /**
   * Full-screen gate shown when alhalte-maitre-d refuses entry to a room.
   *
   * Required copy (Issue #115):
   * - 画面中央大文字「アカウントに入室権限がありません」
   * - その下「部屋の管理者に権限をリクエストしてください。正しいアカウントを使っていますか？」
   * - その下「サインイン・アカウント切り替え」
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 門前払い画面`
   * - GitHub Issue #115: 門前払い画面の実装
   */
  import { Button } from 'noph-ui';
  import AccountMenu from '../Control/AccountMenu.svelte';
  import { beginGoogleSignIn, isSignedIn, signOut } from '../../utils/account.js';

  let {
    baseUrl = '',
    fetchFn,
    navigate,
    onSignIn,
    onReturnHome,
  }: {
    baseUrl?: string;
    fetchFn?: typeof fetch;
    navigate?: (url: string) => void;
    onSignIn?: () => void;
    onReturnHome?: () => void;
  } = $props();

  let isBusy = $state(false);
  let authError = $state('');

  async function handleSignInOrSwitch() {
    authError = '';
    if (onSignIn) {
      onSignIn();
      return;
    }

    // Switching from an existing account must discard the old session before
    // Google can choose another account.
    const switching = isSignedIn();
    if (switching) {
      signOut();
    }

    isBusy = true;
    try {
      await beginGoogleSignIn({ baseUrl, fetchFn, navigate, prompt: 'select_account' });
    } catch {
      authError = 'サインインを開始できませんでした';
      isBusy = false;
    }
  }
</script>

<div class="gate-screen" data-testid="forbidden-room-screen">
  <div class="gate-account-slot">
    <AccountMenu {baseUrl} {fetchFn} />
  </div>

  <div class="gate-content">
    <h1 class="gate-title" data-testid="forbidden-title">
      アカウントに入室権限がありません
    </h1>
    <p class="gate-message" data-testid="forbidden-message">
      部屋の管理者に権限をリクエストしてください。正しいアカウントを使っていますか？
    </p>

    <div class="gate-actions">
      <Button
        variant="filled"
        size="m"
        onclick={handleSignInOrSwitch}
        disabled={isBusy}
        data-testid="forbidden-signin-button"
      >
        サインイン・アカウント切り替え
      </Button>
      {#if onReturnHome}
        <Button variant="text" size="s" onclick={onReturnHome} data-testid="forbidden-return-home-btn">
          ホームに戻る
        </Button>
      {/if}
    </div>

    {#if authError}
      <p class="gate-error" role="alert">{authError}</p>
    {/if}
  </div>
</div>

<style>
  .gate-screen {
    width: 100%;
    flex: 1;
    min-height: 100vh;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    background-color: var(--np-color-background);
    color: var(--np-color-on-background);
    padding: 24px;
    box-sizing: border-box;
  }
  .gate-account-slot {
    position: absolute;
    top: 16px;
    right: 16px;
    z-index: 10;
  }
  .gate-content {
    max-width: 720px;
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 20px;
  }
  .gate-title {
    margin: 0;
    font-size: clamp(1.75rem, 4vw, 2.75rem);
    font-weight: 800;
    line-height: 1.25;
    letter-spacing: -0.01em;
    color: var(--np-color-on-surface);
  }
  .gate-message {
    margin: 0;
    max-width: 560px;
    font-size: 1rem;
    line-height: 1.7;
    color: var(--np-color-on-surface-variant);
  }
  .gate-actions {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .gate-error {
    margin: 0;
    font-size: 0.85rem;
    color: var(--np-color-error);
  }
</style>
