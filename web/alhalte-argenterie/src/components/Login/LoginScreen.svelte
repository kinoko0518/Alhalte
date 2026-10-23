<script lang="ts">
  /**
   * Local password login screen.
   *
   * `GET /auth/login` redirects here (`/login?return_to=...`) when maitre-d runs
   * with `--local-login`. The form posts JSON to `/api/auth/local/login`, which
   * sets the `session_id` cookie. After success the browser navigates back to
   * the app URL; `completeGoogleSignIn()` on the app mount resolves
   * `/api/session` and restores the URL fragment (room key).
   *
   * All interactive controls are noph-ui components (Material 3 Expressive).
   */
  import { Button, Card, TextField } from 'noph-ui';
  import { OAUTH_RETURN_URL_STORAGE_KEY } from '../../utils/account.js';

  const MIN_PASSWORD_LEN = 8;

  let {
    fetchFn = typeof fetch !== 'undefined' ? fetch : undefined,
    navigate = (url: string) => {
      if (typeof window !== 'undefined') window.location.assign(url);
    },
  }: {
    /** Injectable fetch implementation for tests / embedders. */
    fetchFn?: typeof fetch;
    /** Injectable navigation implementation for tests. */
    navigate?: (url: string) => void;
  } = $props();

  let accountName = $state('');
  let password = $state('');
  let isSubmitting = $state(false);
  let errorMessage = $state('');

  /**
   * Resolves `return_to` only when it stays on this origin.
   *
   * The server validated the origin before redirecting here, but the login page
   * is also reachable directly, so the same-origin check is repeated.
   */
  function safeReturnTo(): string {
    if (typeof window === 'undefined') return '/';
    const raw = new URLSearchParams(window.location.search).get('return_to');
    if (!raw) return '/';
    try {
      const url = new URL(raw, window.location.origin);
      if (url.origin !== window.location.origin) return '/';
      return `${url.pathname}${url.search}${url.hash}`;
    } catch {
      return '/';
    }
  }

  async function handleSubmit(event: SubmitEvent) {
    event.preventDefault();
    if (isSubmitting) return;

    errorMessage = '';
    if (accountName.trim() === '') {
      errorMessage = 'アカウント名を入力してください';
      return;
    }
    if (password.length < MIN_PASSWORD_LEN) {
      errorMessage = `パスワードは${MIN_PASSWORD_LEN}文字以上で入力してください`;
      return;
    }

    isSubmitting = true;
    try {
      if (!fetchFn) {
        errorMessage = 'サーバーに接続できませんでした';
        return;
      }
      const response = await fetchFn('/api/auth/local/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify({ account_name: accountName, password }),
      });

      if (response.ok) {
        const returnTo = safeReturnTo();
        // The normal `beginGoogleSignIn` path already stored the full URL
        // (including the room-key fragment). A direct visit to /login has no
        // pending URL, so store the safe return target here; otherwise the app
        // mount would not call /api/session and would look signed out.
        try {
          if (
            typeof sessionStorage !== 'undefined' &&
            !sessionStorage.getItem(OAUTH_RETURN_URL_STORAGE_KEY)
          ) {
            sessionStorage.setItem(OAUTH_RETURN_URL_STORAGE_KEY, returnTo);
          }
        } catch {
          // Ignore storage failures; navigation still proceeds.
        }
        navigate(returnTo);
        return;
      }

      if (response.status === 401) {
        errorMessage = 'アカウント名またはパスワードが違います';
      } else if (response.status === 404) {
        errorMessage = 'このサーバーではローカルログインが無効です';
      } else if (response.status === 400) {
        const payload = (await response.json().catch(() => null)) as { error?: unknown } | null;
        errorMessage =
          typeof payload?.error === 'string' ? payload.error : '入力内容を確認してください';
      } else {
        errorMessage = 'ログインに失敗しました';
      }
    } catch {
      errorMessage = 'サーバーに接続できませんでした';
    } finally {
      isSubmitting = false;
    }
  }
</script>

<div class="login-page" data-testid="login-screen">
  <div class="login-card">
    <Card variant="elevated">
      <form class="login-form" onsubmit={handleSubmit} novalidate>
      <div class="login-heading">
        <p class="login-eyebrow">Alhalte</p>
        <h1 class="login-title">サインイン</h1>
        <p class="login-description">
          アカウント名とパスワードを入力してください。初めての名前は新しいアカウントとして登録されます。
        </p>
      </div>

      <TextField
        id="login-account-name"
        name="account_name"
        label="アカウント名"
        variant="outlined"
        autocomplete="username"
        required
        bind:value={accountName}
        data-testid="login-account-name"
      />

      <TextField
        id="login-password"
        name="password"
        label="パスワード"
        variant="outlined"
        type="password"
        autocomplete="current-password"
        supportingText={`${MIN_PASSWORD_LEN}文字以上`}
        required
        bind:value={password}
        data-testid="login-password"
      />

      {#if errorMessage}
        <p class="login-error" role="alert" aria-live="polite" data-testid="login-error">
          {errorMessage}
        </p>
      {/if}

      <Button
        variant="filled"
        size="m"
        type="submit"
        loading={isSubmitting}
        disabled={isSubmitting}
        data-testid="login-submit"
      >
        サインイン
      </Button>
    </form>
  </Card>
  </div>
</div>

<style>
  .login-page {
    min-height: 100dvh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    box-sizing: border-box;
    background: var(--np-color-surface, #fef7ff);
    color: var(--np-color-on-surface, #1d1b20);
  }

  .login-card {
    width: min(28rem, 100%);
  }

  .login-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 8px;
  }

  .login-heading {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 8px;
  }

  .login-eyebrow {
    margin: 0;
    font: var(--np-text-label-large, 500 0.875rem/1.25rem Roboto, sans-serif);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--np-color-primary, #6750a4);
  }

  .login-title {
    margin: 0;
    font: var(--np-text-headline-medium, 400 1.75rem/2.25rem Roboto, sans-serif);
  }

  .login-description {
    margin: 0;
    font: var(--np-text-body-medium, 400 0.875rem/1.25rem Roboto, sans-serif);
    color: var(--np-color-on-surface-variant, #49454f);
    line-height: 1.6;
  }

  .login-error {
    margin: 0;
    padding: 12px 16px;
    border-radius: 12px;
    background: var(--np-color-error-container, #f9dedc);
    color: var(--np-color-on-error-container, #410e0b);
    font: var(--np-text-body-medium, 400 0.875rem/1.25rem Roboto, sans-serif);
  }
</style>
