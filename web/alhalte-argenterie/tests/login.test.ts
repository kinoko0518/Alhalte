/**
 * Local password login screen tests.
 *
 * The screen is a Svelte page on `/login`; `GET /auth/login` redirects here when
 * maitre-d runs with `--local-login`. It posts JSON to
 * `/api/auth/local/login` and navigates back to the app URL only after success.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import LoginScreen from '../src/components/Login/LoginScreen.svelte';
import { OAUTH_RETURN_URL_STORAGE_KEY } from '../src/utils/account.js';

type MockResponse = {
  ok: boolean;
  status: number;
  json: () => Promise<unknown>;
};

function response(status: number, body: unknown = {}): MockResponse {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  };
}

function setRoute(returnTo: string) {
  window.history.replaceState(
    {},
    '',
    `/login?return_to=${encodeURIComponent(returnTo)}`,
  );
}

function fillForm(target: HTMLElement, name: string, password: string) {
  const nameInput = target.querySelector<HTMLInputElement>(
    '[data-testid="login-account-name"]',
  );
  const passwordInput = target.querySelector<HTMLInputElement>(
    '[data-testid="login-password"]',
  );
  expect(nameInput).toBeTruthy();
  expect(passwordInput).toBeTruthy();
  nameInput!.value = name;
  nameInput!.dispatchEvent(new Event('input', { bubbles: true }));
  passwordInput!.value = password;
  passwordInput!.dispatchEvent(new Event('input', { bubbles: true }));
}

function submit(target: HTMLElement) {
  const form = target.querySelector('form');
  expect(form).toBeTruthy();
  form!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
}

describe('LoginScreen', () => {
  let instance: unknown;
  let target: HTMLDivElement;

  beforeEach(() => {
    sessionStorage.clear();
  });

  afterEach(() => {
    if (instance) unmount(instance as never);
    target?.remove();
    document.body.innerHTML = '';
  });

  // spec: alhalte-argenterie/local-login 0001 the form uses noph-ui controls
  it('renders noph-ui text fields, a password field, and a filled button', () => {
    setRoute('/');
    target = document.createElement('div');
    document.body.appendChild(target);

    instance = mount(LoginScreen, {
      target,
      props: {
        fetchFn: vi.fn() as unknown as typeof fetch,
        navigate: vi.fn(),
      },
    });

    expect(target.querySelector('[data-testid="login-screen"]')).toBeTruthy();
    const password = target.querySelector<HTMLInputElement>(
      '[data-testid="login-password"]',
    );
    expect(password?.type).toBe('password');
    expect(target.querySelector('[data-testid="login-submit"]')).toBeTruthy();
    expect(target.querySelector('.np-button')).toBeTruthy();
    expect(target.querySelector('.np-text-field')).toBeTruthy();
  });

  // spec: alhalte-argenterie/local-login 0002 a successful login returns to the app
  it('posts the account name and password, then navigates to a same-origin return_to', async () => {
    setRoute('/rooms/abc?x=1#room-key');
    target = document.createElement('div');
    document.body.appendChild(target);

    const fetchFn = vi.fn(async () => response(200));
    const navigate = vi.fn();
    instance = mount(LoginScreen, {
      target,
      props: { fetchFn: fetchFn as unknown as typeof fetch, navigate },
    });

    fillForm(target, 'Alice', 'correct horse');
    submit(target);

    await vi.waitFor(() => expect(navigate).toHaveBeenCalled());
    expect(navigate).toHaveBeenCalledWith('/rooms/abc?x=1#room-key');
    // A direct /login visit has no pending URL; the screen stores one so the
    // app mount calls /api/session and restores the session.
    expect(sessionStorage.getItem(OAUTH_RETURN_URL_STORAGE_KEY)).toBe(
      '/rooms/abc?x=1#room-key',
    );

    expect(fetchFn).toHaveBeenCalledTimes(1);
    const [url, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/auth/local/login');
    expect(init.method).toBe('POST');
    expect(JSON.parse(String(init.body))).toEqual({
      account_name: 'Alice',
      password: 'correct horse',
    });
  });

  // spec: alhalte-argenterie/local-login 0003 a wrong password stays on the form
  it('shows an error and does not navigate on 401', async () => {
    setRoute('/');
    target = document.createElement('div');
    document.body.appendChild(target);

    const fetchFn = vi.fn(async () => response(401));
    const navigate = vi.fn();
    instance = mount(LoginScreen, {
      target,
      props: { fetchFn: fetchFn as unknown as typeof fetch, navigate },
    });

    fillForm(target, 'Alice', 'wrong password');
    submit(target);

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="login-error"]')?.textContent).toContain(
        'アカウント名またはパスワードが違います',
      ),
    );
    expect(navigate).not.toHaveBeenCalled();
  });

  // spec: alhalte-argenterie/local-login 0004 cross-origin return_to is ignored
  it('never follows a cross-origin return_to', async () => {
    setRoute('https://evil.example/phish');
    target = document.createElement('div');
    document.body.appendChild(target);

    const navigate = vi.fn();
    instance = mount(LoginScreen, {
      target,
      props: {
        fetchFn: vi.fn(async () => response(200)) as unknown as typeof fetch,
        navigate,
      },
    });

    fillForm(target, 'Alice', 'correct horse');
    submit(target);

    await vi.waitFor(() => expect(navigate).toHaveBeenCalled());
    expect(navigate).toHaveBeenCalledWith('/');
  });

  // spec: alhalte-argenterie/local-login 0005 short passwords are rejected client-side
  it('does not call the API when the password is shorter than 8 characters', async () => {
    setRoute('/');
    target = document.createElement('div');
    document.body.appendChild(target);

    const fetchFn = vi.fn();
    const navigate = vi.fn();
    instance = mount(LoginScreen, {
      target,
      props: { fetchFn: fetchFn as unknown as typeof fetch, navigate },
    });

    fillForm(target, 'Alice', 'short');
    submit(target);

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="login-error"]')?.textContent).toContain(
        '8文字以上',
      ),
    );
    expect(fetchFn).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
  });
});
