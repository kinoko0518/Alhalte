/**
 * Account profile retrieval, Google sign-in flow, and sign-out helpers for
 * `@alhalte/argenterie`.
 *
 * Authentication is owned by `alhalte-maitre-d`. The browser navigates to
 * `GET /auth/login?return_to=<absolute-url>`; maitre-d issues a single-use
 * state, sends Google the fixed `{public_url}/auth/google/callback` redirect
 * URI (never the argenterie origin / port), and redirects back to `return_to`
 * with a `session_id` cookie. The app then calls `GET /api/session` to learn the
 * account and stores it in the same places the room socket reads.
 *
 * Browsers cannot send a URL fragment to the server, so the complete return URL
 * (room key fragment included) is parked in `sessionStorage` before leaving and
 * restored with `history.replaceState` after the redirect, so signing in never
 * loses an invitation URL.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### アカウントUI / Googleサインイン`
 * - crates/alhalte-maitre-d/ARCHITECTURE.md: `### Google 認証`
 * - GitHub Issue #116: アカウントに関するUIを追加
 * - GitHub Issue #188: Google サインインを maitre-d 経由にする
 */

import {
  buildAuthHeaders,
  getSavedAccountId,
  getSavedSessionId,
  saveAccountId,
  saveSessionId,
} from './session.js';

/** sessionStorage key holding the full URL (fragment included) to restore after OAuth. */
export const OAUTH_RETURN_URL_STORAGE_KEY = 'alhalte:oauth-return-url';

/** Account profile returned by `GET /api/account/{account_id}`. */
export interface AccountProfile {
  /** 64-bit account ID. */
  account_id: number;
  /** Handle name displayed in the account menu. */
  handle_name: string;
  /** Optional 32-byte avatar Tin ID, serialized as an array of bytes. */
  icon_tin_id?: number[] | string | null;
  /** Resolved avatar URL (derived from `icon_tin_id` when absent). */
  icon_url?: string | null;
}

/** Response payload of `GET /api/session`. */
export interface SessionResponse {
  /** Public account profile for the session owner. */
  account: AccountProfile;
  /** Session ID resolved from the `session_id` cookie. */
  session_id: number;
}

/** Options shared by the account API helpers. */
export interface AccountRequestOptions {
  /** Optional base URL (useful for tests / reverse-proxy prefixes). */
  baseUrl?: string;
  /** Injectable fetch implementation for tests. */
  fetchFn?: typeof fetch;
  /** Account ID override (defaults to the saved account). */
  accountId?: string | number | null;
  /** Session ID override (defaults to the saved session). */
  sessionId?: string | number | null;
}

/**
 * Returns true when both an account ID and a session ID are stored.
 */
export function isSignedIn(): boolean {
  return Boolean(getSavedAccountId() && getSavedSessionId());
}

/**
 * Converts a serialized 32-byte Tin ID into lowercase hexadecimal.
 *
 * `alhalte-maitre-d` serializes `[u8; 32]` as a JSON number array, while some
 * callers may already hold the hexadecimal representation. Invalid input
 * yields `null` instead of throwing so a malformed avatar never breaks the UI.
 */
export function tinIdToHex(value: unknown): string | null {
  if (typeof value === 'string') {
    return /^[0-9a-fA-F]{64}$/.test(value) ? value.toLowerCase() : null;
  }
  if (!Array.isArray(value) || value.length !== 32) {
    return null;
  }

  const hexBytes: string[] = [];
  for (const byte of value) {
    const n = Number(byte);
    if (!Number.isInteger(n) || n < 0 || n > 255) {
      return null;
    }
    hexBytes.push(n.toString(16).padStart(2, '0'));
  }
  return hexBytes.join('');
}

/**
 * Normalizes an account API payload into [`AccountProfile`].
 */
export function normalizeAccountProfile(raw: {
  account_id: number;
  handle_name?: string;
  icon_tin_id?: number[] | string | null;
  icon_url?: string | null;
}): AccountProfile {
  const iconHex = tinIdToHex(raw.icon_tin_id);
  const iconUrl = raw.icon_url ?? (iconHex ? `/api/cellar/tin/${iconHex}` : null);

  return {
    account_id: raw.account_id,
    handle_name: raw.handle_name || 'Anonymous',
    icon_tin_id: raw.icon_tin_id ?? null,
    icon_url: iconUrl,
  };
}

/**
 * Fetches the profile of an account.
 */
export async function getAccountProfile(
  accountId: string | number,
  options: AccountRequestOptions = {},
): Promise<AccountProfile> {
  const fetchFn = options.fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!fetchFn) {
    throw new Error('fetch is not available');
  }

  const base = (options.baseUrl || '').replace(/\/+$/, '');
  const url = `${base}/api/account/${encodeURIComponent(String(accountId))}`;
  const res = await fetchFn(url, {
    method: 'GET',
    headers: buildAuthHeaders(options.accountId ?? accountId, options.sessionId),
    credentials: 'include',
  });

  if (!res.ok) {
    throw new Error(`Failed to get account profile: ${res.status} ${res.statusText}`);
  }

  return normalizeAccountProfile((await res.json()) as AccountProfile);
}

/** Options for starting Google OAuth. */
export interface BeginGoogleSignInOptions {
  /** Optional API base URL (the origin that proxies `/auth` to maitre-d). */
  baseUrl?: string;
  /** Injectable navigation function for tests. */
  navigate?: (url: string) => void;
  /** Google `prompt` parameter. `select_account` is used when switching. */
  prompt?: 'select_account' | 'consent' | 'none' | null;
  /** Injectable storage for tests. */
  storage?: Storage;
}

function getSessionStorage(storage?: Storage): Storage | null {
  if (storage) return storage;
  if (typeof sessionStorage === 'undefined') return null;
  return sessionStorage;
}

function safeSessionSet(storage: Storage | null, key: string, value: string): void {
  try {
    storage?.setItem(key, value);
  } catch {
    // Storage may be unavailable in private browsing; OAuth still works
    // without the return-URL restore, which is best-effort.
  }
}

function safeSessionRemove(storage: Storage | null, key: string): void {
  try {
    storage?.removeItem(key);
  } catch {
    // Ignore storage errors.
  }
}

/**
 * Returns the current application URL (origin, query and fragment included)
 * without OAuth callback parameters.
 *
 * The fragment is included because the room key lives there; maitre-d only
 * receives the fragment-free part as `return_to`.
 */
export function currentReturnUrl(
  location: Pick<Location, 'origin' | 'pathname' | 'search' | 'hash'>,
): string {
  const params = new URLSearchParams(location.search);
  params.delete('code');
  params.delete('state');
  params.delete('error');
  const query = params.toString();
  return `${location.origin}${location.pathname}${query ? `?${query}` : ''}${location.hash}`;
}

/** Removes the URL fragment before handing a URL to the server. */
function withoutFragment(url: string): string {
  const index = url.indexOf('#');
  return index === -1 ? url : url.slice(0, index);
}

/**
 * Starts the Google OAuth redirect flow through `maitre-d`.
 *
 * The complete return URL (fragment included) is stored in `sessionStorage`,
 * then the browser navigates to `GET /auth/login?return_to=<fragment-free URL>`.
 * maitre-d owns the OAuth state and sends Google its fixed callback URL, so the
 * argenterie origin / port is never part of the redirect URI.
 *
 * @returns The maitre-d login URL that was navigated to.
 */
export async function beginGoogleSignIn(
  options: BeginGoogleSignInOptions = {},
): Promise<string> {
  if (typeof window === 'undefined') {
    throw new Error('window is not available');
  }

  const storage = getSessionStorage(options.storage);
  const returnUrl = currentReturnUrl(window.location);
  safeSessionSet(storage, OAUTH_RETURN_URL_STORAGE_KEY, returnUrl);

  const params = new URLSearchParams({ return_to: withoutFragment(returnUrl) });
  if (options.prompt) {
    params.set('prompt', options.prompt);
  }

  const base = (options.baseUrl || '').replace(/\/+$/, '');
  const loginUrl = `${base}/auth/login?${params.toString()}`;

  const navigate =
    options.navigate ??
    ((url: string) => {
      window.location.assign(url);
    });
  navigate(loginUrl);
  return loginUrl;
}

/** Options for completing Google OAuth. */
export interface CompleteGoogleSignInOptions {
  /** Optional API base URL. */
  baseUrl?: string;
  /** Injectable fetch implementation for tests. */
  fetchFn?: typeof fetch;
  /** Injectable storage for tests. */
  storage?: Storage;
}

/** Converts a stored absolute URL into a same-origin path for `replaceState`. */
function restoreTarget(stored: string, origin: string): string | null {
  try {
    const url = new URL(stored, origin);
    if (url.origin !== origin) {
      return null;
    }
    return `${url.pathname}${url.search}${url.hash}`;
  } catch {
    return null;
  }
}

/**
 * Completes Google OAuth after maitre-d redirects back to the app.
 *
 * The callback sets the `session_id` cookie, so this calls `GET /api/session`
 * (cookie only; a stale `X-Alhalte-Session` header must not win) to resolve the
 * account. It then stores the account and session in the same locations used by
 * the room socket, restores the pre-OAuth URL (including the room key fragment)
 * and returns the account profile.
 *
 * Returns `null` when no sign-in is pending or the callback left no valid
 * session. Never throws for an unsigned user.
 *
 * # Errors
 *
 * Throws when `/api/session` fails for a reason other than 401, or when the
 * response is missing the account/session.
 */
export async function completeGoogleSignIn(
  options: CompleteGoogleSignInOptions = {},
): Promise<AccountProfile | null> {
  if (typeof window === 'undefined') {
    return null;
  }

  const storage = getSessionStorage(options.storage);
  let returnUrl: string | null = null;
  try {
    returnUrl = storage?.getItem(OAUTH_RETURN_URL_STORAGE_KEY) ?? null;
  } catch {
    // Ignore storage read errors; without a pending sign-in there is nothing
    // to complete.
  }
  if (!returnUrl) {
    return null;
  }

  const fetchFn = options.fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!fetchFn) {
    throw new Error('fetch is not available');
  }

  const base = (options.baseUrl || '').replace(/\/+$/, '');
  const res = await fetchFn(`${base}/api/session`, {
    method: 'GET',
    credentials: 'include',
  });

  if (res.status === 401) {
    safeSessionRemove(storage, OAUTH_RETURN_URL_STORAGE_KEY);
    return null;
  }
  if (!res.ok) {
    throw new Error(`Failed to complete Google sign-in: ${res.status} ${res.statusText}`);
  }

  const data = (await res.json()) as SessionResponse;
  if (!data.account || data.session_id === undefined || data.session_id === null) {
    throw new Error('Google sign-in response is missing account or session');
  }

  saveAccountId(data.account.account_id);
  saveSessionId(data.session_id);

  const target = restoreTarget(returnUrl, window.location.origin);
  if (target) {
    try {
      window.history.replaceState(window.history.state, '', target);
    } catch {
      // Ignore history errors in restricted environments.
    }
  }

  safeSessionRemove(storage, OAUTH_RETURN_URL_STORAGE_KEY);

  return normalizeAccountProfile(data.account);
}

/**
 * Removes the stored account, session, and pending OAuth return URL.
 *
 * The room key fragment is intentionally untouched; signing out must not lose
 * an invitation URL.
 */
export function signOut(): void {
  if (typeof localStorage !== 'undefined') {
    try {
      localStorage.removeItem('alhalte:account-id');
      localStorage.removeItem('alhalte:session-id');
    } catch {
      // Ignore storage errors.
    }
  }
  if (typeof document !== 'undefined') {
    document.cookie = 'session_id=; path=/; max-age=0; SameSite=Lax';
  }
  if (typeof sessionStorage !== 'undefined') {
    safeSessionRemove(sessionStorage, OAUTH_RETURN_URL_STORAGE_KEY);
  }
}
