/**
 * Tests for account UI, Google sign-in flow, and sign-out (Issue #116), plus the
 * account control placements in the home app bar, the field screen, and the gate
 * screen (Issue #268).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### アカウントUI / Googleサインイン`
 * - GitHub Issue #116: アカウントに関するUIを追加
 * - GitHub Issue #268: フィールドパネルのアカウントアイコンからMenuが開かない
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
  OAUTH_RETURN_URL_STORAGE_KEY,
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

/** Waits for Svelte to flush the state changes triggered by a DOM event. */
function flush(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 20));
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
    vi.unstubAllGlobals();
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

  // spec: alhalte-argenterie/account 0020 OAuth開始時に戻り先URLとfragmentを保持
  it('starts Google OAuth through maitre-d, preserving the room fragment', async () => {
    window.history.replaceState(null, '', '/rooms/1122334455667788?invite=1#room-key');
    const navigate = vi.fn();

    const result = await beginGoogleSignIn({
      navigate,
      prompt: 'select_account',
    });

    const requested = new URL(navigate.mock.calls[0][0], window.location.origin);
    expect(requested.pathname).toBe('/auth/login');
    // maitre-d receives an absolute return_to without the fragment.
    expect(requested.searchParams.get('return_to')).toBe(
      `${window.location.origin}/rooms/1122334455667788?invite=1`,
    );
    expect(requested.searchParams.get('prompt')).toBe('select_account');
    // The full URL (room key included) is parked for the return trip.
    expect(sessionStorage.getItem(OAUTH_RETURN_URL_STORAGE_KEY)).toBe(
      `${window.location.origin}/rooms/1122334455667788?invite=1#room-key`,
    );
    expect(result).toContain('/auth/login?');
  });

  // spec: alhalte-argenterie/account 0021 OAuthコールバックでaccount/session保存・部屋URL復元
  it('completes Google OAuth via GET /api/session and restores the room URL fragment', async () => {
    window.history.replaceState(null, '', '/rooms/1122334455667788?invite=1');
    sessionStorage.setItem(
      OAUTH_RETURN_URL_STORAGE_KEY,
      `${window.location.origin}/rooms/1122334455667788?invite=1#room-key`,
    );

    const fetchFn = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      expect(init?.method).toBe('GET');
      // Cookie-only: a stale X-Alhalte-Session header must not pick the account.
      expect(init?.headers).toBeUndefined();
      return jsonResponse({
        account: { account_id: 42, handle_name: 'Alice', icon_tin_id: null },
        session_id: '123456789',
      });
    });

    const profile = await completeGoogleSignIn({ fetchFn: fetchFn as unknown as typeof fetch });

    expect(fetchFn).toHaveBeenCalledTimes(1);
    const requested = new URL(String(fetchFn.mock.calls[0][0]), window.location.origin);
    expect(requested.pathname).toBe('/api/session');

    expect(profile?.account_id).toBe(42);
    expect(localStorage.getItem('alhalte:account-id')).toBe('42');
    expect(localStorage.getItem('alhalte:session-id')).toBe('123456789');
    expect(document.cookie).toContain('session_id=123456789');
    expect(`${window.location.pathname}${window.location.search}${window.location.hash}`).toBe(
      '/rooms/1122334455667788?invite=1#room-key',
    );
    expect(sessionStorage.getItem(OAUTH_RETURN_URL_STORAGE_KEY)).toBeNull();
  });

  // spec: alhalte-argenterie/account 0024 64bit session ID を丸めずに保存
  it('keeps a 64-bit session ID exact instead of rounding it through a JS number', async () => {
    sessionStorage.setItem(OAUTH_RETURN_URL_STORAGE_KEY, `${window.location.origin}/`);
    // Above `Number.MAX_SAFE_INTEGER`: `JSON.parse` would round this.
    const exact = '18446744073709551614';
    const fetchFn = vi.fn(async () =>
      jsonResponse({
        account: { account_id: 42, handle_name: 'Alice', icon_tin_id: null },
        session_id: exact,
      }),
    );

    const profile = await completeGoogleSignIn({ fetchFn: fetchFn as unknown as typeof fetch });

    expect(profile?.account_id).toBe(42);
    expect(localStorage.getItem('alhalte:session-id')).toBe(exact);
    expect(document.cookie).toContain(`session_id=${exact}`);
  });

  // spec: alhalte-argenterie/account 0025 callback Cookie の正確な session ID を優先
  it('prefers the exact session cookie over a rounded legacy JSON number', async () => {
    const exact = '18446744073709551614';
    // The OAuth callback already set the cookie to the exact value.
    document.cookie = `session_id=${exact}; path=/; SameSite=Lax`;
    sessionStorage.setItem(OAUTH_RETURN_URL_STORAGE_KEY, `${window.location.origin}/`);
    const fetchFn = vi.fn(async () =>
      jsonResponse({
        account: { account_id: 42, handle_name: 'Alice', icon_tin_id: null },
        // Older maitre-d responses used a JSON number, which `JSON.parse`
        // rounds above 2^53.
        session_id: 18446744073709552000,
      }),
    );

    await completeGoogleSignIn({ fetchFn: fetchFn as unknown as typeof fetch });

    expect(localStorage.getItem('alhalte:session-id')).toBe(exact);
    expect(document.cookie).toContain(`session_id=${exact}`);
  });

  // spec: alhalte-argenterie/account 0022 未サインインへの戻りは交換しない
  it('does nothing when no sign-in is pending', async () => {
    window.history.replaceState(null, '', '/?code=code-1&state=state-1');
    const fetchFn = vi.fn();

    await expect(
      completeGoogleSignIn({ fetchFn: fetchFn as unknown as typeof fetch }),
    ).resolves.toBeNull();
    expect(fetchFn).not.toHaveBeenCalled();
  });

  // spec: alhalte-argenterie/account 0023 callback後にsessionが無ければ未サインインのまま
  it('clears the pending sign-in when /api/session returns 401', async () => {
    sessionStorage.setItem(
      OAUTH_RETURN_URL_STORAGE_KEY,
      `${window.location.origin}/rooms/1122334455667788`,
    );
    const fetchFn = vi.fn(async () => jsonResponse({ error: 'Invalid session' }, 401));

    const profile = await completeGoogleSignIn({
      fetchFn: fetchFn as unknown as typeof fetch,
    });

    expect(profile).toBeNull();
    expect(localStorage.getItem('alhalte:account-id')).toBeNull();
    expect(sessionStorage.getItem(OAUTH_RETURN_URL_STORAGE_KEY)).toBeNull();
  });

  // spec: alhalte-argenterie/account 0031 サインアウトでaccount/session/cookieを削除
  it('signs out by clearing the account, session, pending sign-in, and cookie', async () => {
    localStorage.setItem('alhalte:account-id', '42');
    localStorage.setItem('alhalte:session-id', '99');
    document.cookie = 'session_id=99; path=/; SameSite=Lax';
    sessionStorage.setItem(OAUTH_RETURN_URL_STORAGE_KEY, `${window.location.origin}/#key`);

    signOut();

    expect(localStorage.getItem('alhalte:account-id')).toBeNull();
    expect(localStorage.getItem('alhalte:session-id')).toBeNull();
    expect(document.cookie).not.toContain('session_id=99');
    expect(sessionStorage.getItem(OAUTH_RETURN_URL_STORAGE_KEY)).toBeNull();
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

  // spec: Issue #268 フィールドの右上アカウントUIからメニューを開いて項目を操作できる
  it('opens the account menu from the field top-right layer and signs out from a menu item', async () => {
    localStorage.setItem('alhalte:account-id', '42');
    localStorage.setItem('alhalte:session-id', '99');
    // `AccountMenu` reads `/api/account/{id}` on mount; tests never use the network.
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        jsonResponse({ account_id: 42, handle_name: 'Alice', icon_tin_id: null }),
      ),
    );
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
    await flush();

    const layer = target.querySelector('[data-testid="field-account-layer"]');
    expect(layer).toBeTruthy();
    expect(layer?.querySelector('[data-testid="account-controls"]')).toBeTruthy();

    const avatar = layer?.querySelector<HTMLButtonElement>('[data-testid="account-menu-button"]');
    expect(avatar).toBeTruthy();
    expect(avatar?.getAttribute('aria-expanded')).toBe('false');

    // Issue #268: a press on the avatar must reach the button instead of being
    // captured by the field container for board panning.
    avatar?.dispatchEvent(
      new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerId: 1 }),
    );
    avatar?.click();
    await flush();

    expect(avatar?.getAttribute('aria-expanded')).toBe('true');
    const menu = target.querySelector('[data-testid="account-menu"]');
    expect(menu).toBeTruthy();
    const switchItem = menu?.querySelector<HTMLElement>('[data-testid="account-switch-item"]');
    const signOutItem = menu?.querySelector<HTMLElement>('[data-testid="account-signout-item"]');
    expect(switchItem?.textContent).toContain('別のアカウントでログイン');
    expect(signOutItem?.textContent).toContain('サインアウト');

    // The menu items are reachable: sign-out clears the local session.
    signOutItem?.click();
    await flush();

    expect(localStorage.getItem('alhalte:account-id')).toBeNull();
    expect(localStorage.getItem('alhalte:session-id')).toBeNull();
    expect(layer?.querySelector('[data-testid="account-signin-button"]')).toBeTruthy();

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
