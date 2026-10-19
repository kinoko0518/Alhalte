/**
 * Tests for account UI, Google sign-in flow, and sign-out (Issue #116).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### アカウントUI / Googleサインイン`
 * - GitHub Issue #116: アカウントに関するUIを追加
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import AccountMenu from '../src/components/Control/AccountMenu.svelte';
import App from '../src/components/App.svelte';
import FieldPanel from '../src/components/Field/FieldPanel.svelte';
import {
  beginGoogleSignIn,
  completeGoogleSignIn,
  normalizeAccountProfile,
  signOut,
  OAUTH_REDIRECT_URI_STORAGE_KEY,
  OAUTH_RETURN_PATH_STORAGE_KEY,
  OAUTH_STATE_STORAGE_KEY,
} from '../src/utils/account.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { Scene, VisitedRoom } from '../src/types/index.js';



function jsonResponse(payload: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? 'OK' : 'Error',
    json: async () => payload,
  } as Response;
}

describe('Account UI & Google sign-in (Issue #116)', () => {
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

  // spec: alhalte-argenterie/account 0010 32byte Tin ID からアバターURLを解決
  it('normalizes account profiles and derives the avatar URL from a 32-byte Tin ID', () => {
    const profile = normalizeAccountProfile({
      account_id: 42,
      handle_name: 'Alice',
      icon_tin_id: Array.from({ length: 32 }, (_, i) => i),
    });
    expect(profile.account_id).toBe(42);
    expect(profile.handle_name).toBe('Alice');
    expect(profile.icon_url).toBe(
      '/api/cellar/tin/000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f',
    );

    expect(
      normalizeAccountProfile({ account_id: 7, handle_name: '', icon_tin_id: [1, 2] }).icon_url,
    ).toBeNull();
  });

  // spec: alhalte-argenterie/account 0020 OAuth開始時に戻り先URL・state・fragmentを保持
  it('starts Google OAuth, preserving the room fragment and adding select_account', async () => {
    window.history.replaceState(null, '', '/rooms/1122334455667788?invite=1#room-key');
    const fetchFn = vi.fn(async () =>
      jsonResponse({ url: 'https://accounts.google.com/o/oauth2/v2/auth?client_id=abc' }),
    );
    const navigate = vi.fn();

    const result = await beginGoogleSignIn({
      fetchFn: fetchFn as unknown as typeof fetch,
      navigate,
      prompt: 'select_account',
    });

    const requested = new URL(String(fetchFn.mock.calls[0][0]), window.location.origin);
    expect(requested.pathname).toBe('/auth/google/url');
    expect(requested.searchParams.get('redirect_uri')).toBe(
      `${window.location.origin}/rooms/1122334455667788`,
    );
    const state = requested.searchParams.get('state');
    expect(state).toBeTruthy();
    expect(sessionStorage.getItem(OAUTH_STATE_STORAGE_KEY)).toBe(state);
    expect(sessionStorage.getItem(OAUTH_RETURN_PATH_STORAGE_KEY)).toBe(
      '/rooms/1122334455667788?invite=1#room-key',
    );
    expect(sessionStorage.getItem(OAUTH_REDIRECT_URI_STORAGE_KEY)).toBe(
      `${window.location.origin}/rooms/1122334455667788`,
    );

    const navigated = new URL(navigate.mock.calls[0][0]);
    expect(navigated.searchParams.get('prompt')).toBe('select_account');
    expect(result).toContain('prompt=select_account');
  });

  // spec: alhalte-argenterie/account 0021 OAuthコールバックでaccount/session保存・部屋URL復元
  it('completes Google OAuth, stores account/session, and restores the room URL fragment', async () => {
    window.history.replaceState(null, '', '/rooms/1122334455667788?code=code-1&state=state-1');
    sessionStorage.setItem(OAUTH_STATE_STORAGE_KEY, 'state-1');
    sessionStorage.setItem(OAUTH_REDIRECT_URI_STORAGE_KEY, `${window.location.origin}/rooms/1122334455667788`);
    sessionStorage.setItem(OAUTH_RETURN_PATH_STORAGE_KEY, '/rooms/1122334455667788#room-key');

    const fetchFn = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      expect(JSON.parse(String(init?.body))).toEqual({
        code: 'code-1',
        redirect_uri: `${window.location.origin}/rooms/1122334455667788`,
      });
      return jsonResponse({
        account: { account_id: 42, handle_name: 'Alice', icon_tin_id: null },
        session_id: 123456789,
      });
    });

    const profile = await completeGoogleSignIn({ fetchFn: fetchFn as unknown as typeof fetch });

    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(profile?.account_id).toBe(42);
    expect(localStorage.getItem('alhalte:account-id')).toBe('42');
    expect(localStorage.getItem('alhalte:session-id')).toBe('123456789');
    expect(document.cookie).toContain('session_id=123456789');
    expect(`${window.location.pathname}${window.location.search}${window.location.hash}`).toBe(
      '/rooms/1122334455667788#room-key',
    );
    expect(sessionStorage.getItem(OAUTH_STATE_STORAGE_KEY)).toBeNull();
    expect(sessionStorage.getItem(OAUTH_REDIRECT_URI_STORAGE_KEY)).toBeNull();
    expect(sessionStorage.getItem(OAUTH_RETURN_PATH_STORAGE_KEY)).toBeNull();
  });

  // spec: alhalte-argenterie/account 0022 OAuth state不一致時は交換しない
  it('rejects a mismatched OAuth state without calling the callback endpoint', async () => {
    window.history.replaceState(null, '', '/?code=code-1&state=attacker');
    sessionStorage.setItem(OAUTH_STATE_STORAGE_KEY, 'expected');
    const fetchFn = vi.fn();

    await expect(
      completeGoogleSignIn({ fetchFn: fetchFn as unknown as typeof fetch }),
    ).rejects.toThrow('state mismatch');
    expect(fetchFn).not.toHaveBeenCalled();
  });

  // spec: alhalte-argenterie/account 0031 サインアウトでaccount/session/cookieを削除
  it('signs out by clearing the account, session, OAuth state, and cookie', async () => {
    localStorage.setItem('alhalte:account-id', '42');
    localStorage.setItem('alhalte:session-id', '99');
    document.cookie = 'session_id=99; path=/; SameSite=Lax';
    sessionStorage.setItem(OAUTH_STATE_STORAGE_KEY, 'state');

    signOut();

    expect(localStorage.getItem('alhalte:account-id')).toBeNull();
    expect(localStorage.getItem('alhalte:session-id')).toBeNull();
    expect(document.cookie).not.toContain('session_id=99');
    expect(sessionStorage.getItem(OAUTH_STATE_STORAGE_KEY)).toBeNull();
  });


  // spec: alhalte-argenterie/account 0032 未サインイン時はFilled tonal「サインイン」
  it('renders a Filled tonal sign-in button when signed out and invokes onSignIn', async () => {
    const onSignIn = vi.fn();
    const instance = mount(AccountMenu, {
      target,
      props: { signedIn: false, onSignIn },
    });
    await new Promise((r) => setTimeout(r, 20));

    const button = target.querySelector<HTMLButtonElement>('[data-testid="account-signin-button"]');
    expect(button).toBeTruthy();
    expect(button?.textContent?.trim()).toBe('サインイン');
    expect(button?.classList.contains('np-button')).toBe(true);
    expect(button?.classList.contains('tonal')).toBe(true);
    expect(target.querySelector('[data-testid="account-menu-button"]')).toBeNull();

    button?.click();
    expect(onSignIn).toHaveBeenCalledTimes(1);

    unmount(instance);
  });

  // spec: alhalte-argenterie/account 0033 サインイン時はアバターとM3 Menu（別アカウント/サインアウト）
  it('renders the account avatar and M3 menu with switch/sign-out actions when signed in', async () => {
    localStorage.setItem('alhalte:account-id', '42');
    localStorage.setItem('alhalte:session-id', '99');
    const onSignOut = vi.fn();
    const onSwitchAccount = vi.fn();

    const instance = mount(AccountMenu, {
      target,
      props: {
        signedIn: true,
        account: { account_id: 42, handle_name: 'Alice', icon_url: null },
        onSignOut,
        onSwitchAccount,
      },
    });
    await new Promise((r) => setTimeout(r, 20));

    const accountButton = target.querySelector<HTMLButtonElement>(
      '[data-testid="account-menu-button"]',
    );
    expect(accountButton).toBeTruthy();
    expect(accountButton?.textContent?.trim()).toBe('A');
    expect(accountButton?.getAttribute('aria-label')).toBe('アカウントメニュー');

    accountButton?.click();
    await new Promise((r) => setTimeout(r, 20));

    const menu = target.querySelector('[data-testid="account-menu"]');
    expect(menu).toBeTruthy();
    expect(target.querySelector('[data-testid="account-switch-item"]')?.textContent).toContain(
      '別のアカウントでログイン',
    );
    expect(target.querySelector('[data-testid="account-signout-item"]')?.textContent).toContain(
      'サインアウト',
    );

    target.querySelector<HTMLButtonElement>('[data-testid="account-switch-item"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(onSwitchAccount).toHaveBeenCalledTimes(1);
    expect(localStorage.getItem('alhalte:account-id')).toBeNull();

    // The control switches to the signed-out state after switching accounts.
    expect(target.querySelector('[data-testid="account-signin-button"]')).toBeTruthy();

    unmount(instance);
  });

  // spec: alhalte-argenterie/account 0034 フィールド画面右上にアカウントUI
  it('places the account control in the top-right layer of the field panel', async () => {
    const scene: Scene = {
      id: '/',
      name: 'メインシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    const instance = mount(FieldPanel, {
      target,
      props: { scene },
    });
    await new Promise((r) => setTimeout(r, 20));

    const layer = target.querySelector('[data-testid="field-account-layer"]');
    expect(layer).toBeTruthy();
    expect(layer?.querySelector('[data-testid="account-controls"]')).toBeTruthy();

    unmount(instance);
  });

  // spec: alhalte-argenterie/account 0035 門前払い画面にもアカウントUIを配置
  it('renders the account control on the gate screen', async () => {
    const room: VisitedRoom = {
      id: 'private-room-001',
      name: 'プライベートルーム',
      url: 'https://alhalte.example/rooms/private-room-001#key',
      lastVisited: Date.now(),
      isAdmin: false,
    };

    const instance = mount(App, {
      target,
      props: { initialRoom: room, isForbidden: true },
    });
    await new Promise((r) => setTimeout(r, 20));

    const gate = target.querySelector('[data-testid="forbidden-room-screen"]');
    expect(gate).toBeTruthy();
    expect(gate?.querySelector('[data-testid="account-controls"]')).toBeTruthy();
    expect(gate?.querySelector('[data-testid="account-signin-button"]')).toBeTruthy();

    unmount(instance);
  });
});
