/**
 * Tests for the maitre-d gate screen (Issue #115).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 門前払い画面`
 * - GitHub Issue #115: 門前払い画面の実装
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import GateScreen from '../src/components/Gate/GateScreen.svelte';

function jsonResponse(payload: unknown): Response {
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    json: async () => payload,
  } as Response;
}

describe('Maître-d gate screen (Issue #115)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    document.cookie = 'session_id=; path=/; max-age=0; SameSite=Lax';
    window.history.replaceState(null, '', '/');
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    target.remove();
    vi.restoreAllMocks();
  });

  // spec: alhalte-argenterie/gate 0020 中央大文字・説明・サインイン/アカウント切替
  it('renders the required centered copy, account control, and home action', async () => {
    const onReturnHome = vi.fn();
    const instance = mount(GateScreen, {
      target,
      props: { onReturnHome },
    });
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="forbidden-room-screen"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="forbidden-title"]')?.textContent?.trim()).toBe(
      'アカウントに入室権限がありません',
    );
    expect(target.querySelector('[data-testid="forbidden-message"]')?.textContent?.trim()).toBe(
      '部屋の管理者に権限をリクエストしてください。正しいアカウントを使っていますか？',
    );
    expect(
      target.querySelector('[data-testid="forbidden-signin-button"]')?.textContent?.trim(),
    ).toBe('サインイン・アカウント切り替え');

    expect(target.querySelector('[data-testid="account-controls"]')).toBeTruthy();

    target.querySelector<HTMLButtonElement>('[data-testid="forbidden-return-home-btn"]')?.click();
    expect(onReturnHome).toHaveBeenCalledTimes(1);

    unmount(instance);
  });

  // spec: alhalte-argenterie/gate 0021 サインイン・アカウント切替操作
  it('signs out the current account before starting account switching', async () => {
    localStorage.setItem('alhalte:account-id', '42');
    localStorage.setItem('alhalte:session-id', '99');

    const fetchFn = vi.fn(async () =>
      jsonResponse({ url: 'https://accounts.google.com/o/oauth2/v2/auth?client_id=abc' }),
    );
    const navigate = vi.fn();

    const instance = mount(GateScreen, {
      target,
      props: {
        fetchFn: fetchFn as unknown as typeof fetch,
        navigate,
      },
    });
    await new Promise((r) => setTimeout(r, 20));

    target.querySelector<HTMLButtonElement>('[data-testid="forbidden-signin-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(localStorage.getItem('alhalte:account-id')).toBeNull();
    expect(localStorage.getItem('alhalte:session-id')).toBeNull();

    const authCall = fetchFn.mock.calls.find((call) =>
      String(call[0]).includes('/auth/google/url'),
    );
    expect(authCall).toBeTruthy();
    const requested = new URL(String(authCall?.[0]), window.location.origin);
    expect(requested.pathname).toBe('/auth/google/url');
    expect(navigate).toHaveBeenCalledTimes(1);
    expect(new URL(navigate.mock.calls[0][0]).searchParams.get('prompt')).toBe('select_account');

    unmount(instance);
  });

  // spec: alhalte-argenterie/gate 0022 onSignIn 注入時は外部ハンドラに委譲
  it('delegates to an injected onSignIn handler', async () => {
    const onSignIn = vi.fn();
    const instance = mount(GateScreen, {
      target,
      props: { onSignIn },
    });
    await new Promise((r) => setTimeout(r, 20));

    target.querySelector<HTMLButtonElement>('[data-testid="forbidden-signin-button"]')?.click();
    expect(onSignIn).toHaveBeenCalledTimes(1);

    unmount(instance);
  });
});
