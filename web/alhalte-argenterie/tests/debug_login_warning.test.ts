import { describe, it, expect, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import HomeScreen from '../src/components/Home/HomeScreen.svelte';
import { fetchServerInfo } from '../src/utils/account.js';

/** Minimal `Response`-shaped object for mocked fetch calls. */
function jsonResponse(payload: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? 'OK' : 'Error',
    json: async () => payload,
  } as Response;
}

/** Mounts the home screen with a `fetch` that answers `/api/server-info`. */
function mountHome(fetchFn: (url: string | URL | Request, init?: RequestInit) => Promise<Response>) {
  const target = document.createElement('div');
  document.body.appendChild(target);
  const instance = mount(HomeScreen, {
    target,
    props: {
      baseUrl: 'https://alhalte.example',
      fetchFn: fetchFn as unknown as typeof fetch,
    },
  });
  return { target, instance };
}

describe('Debug login warning (Issue #150)', () => {
  afterEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  // spec: alhalte-argenterie/account 0040 GET /api/server-info を読む
  it('reads GET /api/server-info and normalizes the debug_login flag', async () => {
    const on = vi.fn(async (url: string | URL | Request, init?: RequestInit) => {
      expect(String(url)).toBe('https://alhalte.example/api/server-info');
      expect(init?.method).toBe('GET');
      return jsonResponse({ debug_login: true });
    });
    await expect(
      fetchServerInfo({ baseUrl: 'https://alhalte.example', fetchFn: on as unknown as typeof fetch }),
    ).resolves.toEqual({ debug_login: true });

    const off = vi.fn(async () => jsonResponse({ debug_login: false }));
    await expect(
      fetchServerInfo({ fetchFn: off as unknown as typeof fetch }),
    ).resolves.toEqual({ debug_login: false });
  });

  // spec: alhalte-argenterie/account 0041 server-info の失敗は警告なしに落とす
  it('never throws when the endpoint is missing, unreachable, or malformed', async () => {
    // An older maitre-d has no /api/server-info.
    await expect(
      fetchServerInfo({
        fetchFn: (async () => jsonResponse({ error: 'not found' }, 404)) as unknown as typeof fetch,
      }),
    ).resolves.toEqual({ debug_login: false });

    // A network failure must not break the home screen.
    await expect(
      fetchServerInfo({
        fetchFn: (async () => {
          throw new Error('offline');
        }) as unknown as typeof fetch,
      }),
    ).resolves.toEqual({ debug_login: false });

    // A non-boolean flag is not trusted.
    await expect(
      fetchServerInfo({
        fetchFn: (async () => jsonResponse({ debug_login: 'yes' })) as unknown as typeof fetch,
      }),
    ).resolves.toEqual({ debug_login: false });
  });

  // spec: alhalte-argenterie/home 0042 デバッグログイン中の全画面警告
  it('shows the full-screen warning when the server reports debug_login', async () => {
    const { target, instance } = mountHome(async () => jsonResponse({ debug_login: true }));

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="debug-login-warning"]')).toBeTruthy(),
    );
    const warning = target.querySelector<HTMLElement>('[data-testid="debug-login-warning"]')!;

    // 中央大文字の警告 and the fixed sentence.
    expect(warning.getAttribute('role')).toBe('alert');
    expect(warning.textContent).toContain('管理者向けの警告');
    expect(warning.textContent).toContain(
      '現在デバッグログインが有効になっています。公開されるインスタンスでこの機能を利用するのは推奨されません。',
    );

    // 全画面: the overlay is a fixed, full-viewport layer above the home content.
    expect(warning.classList.contains('debug-login-warning')).toBe(true);
    const card = warning.querySelector('.debug-login-warning-card');
    expect(card).toBeTruthy();
    expect(target.querySelector('[data-testid="home-screen"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/home 0043 通常時は警告を出さない
  it('renders no warning when debug_login is false', async () => {
    const { target, instance } = mountHome(async () => jsonResponse({ debug_login: false }));

    // Give the mount-time request a chance to resolve.
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="debug-login-warning"]')).toBeNull();
    expect(target.querySelector('[data-testid="home-screen"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/home 0044 server-info 失敗時もホーム画面は動く
  it('renders no warning when the server info cannot be read', async () => {
    const { target, instance } = mountHome(async () => {
      throw new Error('offline');
    });

    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="debug-login-warning"]')).toBeNull();
    // The home screen is still usable (the creation FAB is present).
    expect(target.querySelector('[data-testid="create-room-button"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });
});
