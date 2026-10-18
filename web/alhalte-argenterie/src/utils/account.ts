/**
 * Account profile retrieval, Google sign-in flow, and sign-out helpers for
 * `@alhalte/argenterie`.
 *
 * The browser starts Google OAuth through `alhalte-maitre-d`'s
 * `GET /auth/google/url` endpoint. The authorization code is returned to the
 * current application URL (without its query string or fragment); the app then
 * exchanges it with `POST /auth/google/callback`. The room key fragment and any
 * room query are parked in `sessionStorage` while Google is open and restored
 * after the exchange, so signing in never loses an invitation URL.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### アカウントUI / Googleサインイン`
 * - crates/alhalte-maitre-d/ARCHITECTURE.md: `### Google 認証`
 * - GitHub Issue #116: アカウントに関するUIを追加
 */

import {
  buildAuthHeaders,
  getSavedAccountId,
  getSavedSessionId,
  saveAccountId,
  saveSessionId,
} from './session.js';

/** sessionStorage key holding the OAuth CSRF state. */
export const OAUTH_STATE_STORAGE_KEY = 'alhalte:oauth-state';

/** sessionStorage key holding the application path to restore after OAuth. */
export const OAUTH_RETURN_PATH_STORAGE_KEY = 'alhalte:oauth-return-path';

/** sessionStorage key holding the exact redirect URI used for OAuth. */
export const OAUTH_REDIRECT_URI_STORAGE_KEY = 'alhalte:oauth-redirect-uri';

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

/** Response payload of `POST /auth/google/callback`. */
export interface AuthResponse {
  /** Created or existing account. */
  account: AccountProfile;
  /** Newly issued 64-bit session ID. */
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
  /** Optional API base URL. */
  baseUrl?: string;
  /** Injectable fetch implementation for tests. */
  fetchFn?: typeof fetch;
  /** Injectable navigation function for tests. */
  navigate?: (url: string) => void;
  /** Google `prompt` parameter. `select_account` is used when switching. */
  prompt?: 'select_account' | 'consent' | 'none' | null;
  /** Injectable storage for tests. */
  storage?: Storage;
  /** Injectable redirect URI for tests. */
  redirectUri?: string;
}

/**
 * Creates a random, URL-safe OAuth state value.
 */
export function createOAuthState(): string {
  const cryptoObj = typeof globalThis !== 'undefined' ? globalThis.crypto : undefined;
  if (cryptoObj && typeof cryptoObj.randomUUID === 'function') {
    return cryptoObj.randomUUID();
  }
  if (cryptoObj && typeof cryptoObj.getRandomValues === 'function') {
    const bytes = new Uint8Array(16);
    cryptoObj.getRandomValues(bytes);
    return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
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
    // without state persistence, but restore-path support is best-effort.
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
 * Returns the current application path (query and fragment included) without
 * OAuth callback parameters.
 */
export function currentReturnPath(location: Pick<Location, 'pathname' | 'search' | 'hash'>): string {
  const params = new URLSearchParams(location.search);
  params.delete('code');
  params.delete('state');
  const query = params.toString();
  return `${location.pathname}${query ? `?${query}` : ''}${location.hash}`;
}


/**
 * Starts the Google OAuth redirect flow.
 *
 * The state, return path, and redirect URI are stored in `sessionStorage`
 * before navigating to Google. The redirect URI deliberately excludes the
 * query string and fragment; [`completeGoogleSignIn`] restores them.
 *
 * @returns The Google authorization URL that was navigated to.
 */
export async function beginGoogleSignIn(
  options: BeginGoogleSignInOptions = {},
): Promise<string> {
  const fetchFn = options.fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!fetchFn) {
    throw new Error('fetch is not available');
  }
  if (typeof window === 'undefined') {
    throw new Error('window is not available');
  }

  const storage = getSessionStorage(options.storage);
  const location = window.location;
  const returnPath = currentReturnPath(location);
  const redirectUri = options.redirectUri ?? `${location.origin}${location.pathname}`;
  const state = createOAuthState();

  safeSessionSet(storage, OAUTH_STATE_STORAGE_KEY, state);
  safeSessionSet(storage, OAUTH_RETURN_PATH_STORAGE_KEY, returnPath);
  safeSessionSet(storage, OAUTH_REDIRECT_URI_STORAGE_KEY, redirectUri);

  const base = (options.baseUrl || '').replace(/\/+$/, '');
  const query = new URLSearchParams({ redirect_uri: redirectUri, state });
  const res = await fetchFn(`${base}/auth/google/url?${query.toString()}`, {
    method: 'GET',
    credentials: 'include',
  });

  if (!res.ok) {
    safeSessionRemove(storage, OAUTH_STATE_STORAGE_KEY);
    safeSessionRemove(storage, OAUTH_RETURN_PATH_STORAGE_KEY);
    safeSessionRemove(storage, OAUTH_REDIRECT_URI_STORAGE_KEY);
    throw new Error(`Failed to start Google sign-in: ${res.status} ${res.statusText}`);
  }

  const data = (await res.json()) as { url?: string };
  if (!data.url) {
    throw new Error('Google authorization URL was not returned');
  }

  const authUrl = new URL(data.url);
  if (options.prompt) {
    authUrl.searchParams.set('prompt', options.prompt);
  }

  const navigate =
    options.navigate ??
    ((url: string) => {
      window.location.assign(url);
    });
  navigate(authUrl.toString());
  return authUrl.toString();
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

/**
 * Completes Google OAuth when the current URL carries `code` and `state`.
 *
 * Stores the returned account and session in the same locations used by the
 * room socket, restores the pre-OAuth path (including the room key fragment),
 * and returns the account profile. Returns `null` when the URL has no callback
 * parameters.
 *
 * # Errors
 *
 * Throws when the state does not match, the exchange fails, or the response is
 * missing the account/session.
 */
export async function completeGoogleSignIn(
  options: CompleteGoogleSignInOptions = {},
): Promise<AccountProfile | null> {
  if (typeof window === 'undefined') {
    return null;
  }

  const location = window.location;
  const params = new URLSearchParams(location.search);
  const code = params.get('code');
  if (!code) {
    return null;
  }

  const storage = getSessionStorage(options.storage);
  const state = params.get('state');
  let expectedState: string | null = null;
  let storedRedirectUri: string | null = null;
  let returnPath: string | null = null;
  try {
    expectedState = storage?.getItem(OAUTH_STATE_STORAGE_KEY) ?? null;
    storedRedirectUri = storage?.getItem(OAUTH_REDIRECT_URI_STORAGE_KEY) ?? null;
    returnPath = storage?.getItem(OAUTH_RETURN_PATH_STORAGE_KEY) ?? null;
  } catch {
    // Ignore storage read errors; validation below still prevents an
    // unchecked exchange when a state was expected.
  }

  if (expectedState && state !== expectedState) {
    safeSessionRemove(storage, OAUTH_STATE_STORAGE_KEY);
    safeSessionRemove(storage, OAUTH_RETURN_PATH_STORAGE_KEY);
    safeSessionRemove(storage, OAUTH_REDIRECT_URI_STORAGE_KEY);
    throw new Error('Google sign-in state mismatch');
  }

  const fetchFn = options.fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!fetchFn) {
    throw new Error('fetch is not available');
  }

  const base = (options.baseUrl || '').replace(/\/+$/, '');
  const redirectUri = storedRedirectUri ?? `${location.origin}${location.pathname}`;

  const res = await fetchFn(`${base}/auth/google/callback`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify({ code, redirect_uri: redirectUri }),
  });

  if (!res.ok) {
    throw new Error(`Failed to complete Google sign-in: ${res.status} ${res.statusText}`);
  }

  const data = (await res.json()) as AuthResponse;
  if (!data.account || data.session_id === undefined || data.session_id === null) {
    throw new Error('Google sign-in response is missing account or session');
  }

  saveAccountId(data.account.account_id);
  saveSessionId(data.session_id);

  const restoredPath =
    returnPath && returnPath.startsWith('/') ? returnPath : currentReturnPath(location);
  try {
    window.history.replaceState(window.history.state, '', restoredPath);
  } catch {
    // Ignore history errors in restricted environments.
  }

  safeSessionRemove(storage, OAUTH_STATE_STORAGE_KEY);
  safeSessionRemove(storage, OAUTH_RETURN_PATH_STORAGE_KEY);
  safeSessionRemove(storage, OAUTH_REDIRECT_URI_STORAGE_KEY);

  return normalizeAccountProfile(data.account);
}

/**
 * Removes the stored account, session, and OAuth state.
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
    safeSessionRemove(sessionStorage, OAUTH_STATE_STORAGE_KEY);
    safeSessionRemove(sessionStorage, OAUTH_RETURN_PATH_STORAGE_KEY);
    safeSessionRemove(sessionStorage, OAUTH_REDIRECT_URI_STORAGE_KEY);
  }
}

