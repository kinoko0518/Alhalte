/**
 * Account session and room permission utilities for @alhalte/argenterie.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-maitre-d -> ### セッション管理 / アカウント管理`
 * - GitHub Issue #94: アカウントの概念をArgenterieと接続
 */

export type RoomRole = 'gm' | 'pl' | 'viewer';

export interface AccountSearchItem {
  account_id: number;
  username: string;
  icon_url?: string | null;
}

export interface RoomPermissionItem {
  account_id: number;
  username: string;
  icon_url?: string | null;
  role: RoomRole;
}

export const LOCAL_STORAGE_ACCOUNT_ID_KEY = 'alhalte:account-id';
export const LOCAL_STORAGE_SESSION_ID_KEY = 'alhalte:session-id';

/**
 * Extracts session ID from document.cookie (session_id=<u64>).
 */
export function getSessionIdFromCookie(): string | null {
  if (typeof document === 'undefined' || !document.cookie) return null;
  const match = document.cookie.match(/(?:^|;\s*)session_id=([^;]+)/);
  if (match && match[1]) {
    return decodeURIComponent(match[1].trim());
  }
  return null;
}

/**
 * Sets session_id in document.cookie.
 */
export function setSessionIdCookie(sessionId: string | number, maxAgeSecs = 86400 * 30): void {
  if (typeof document === 'undefined') return;
  document.cookie = `session_id=${encodeURIComponent(String(sessionId))}; path=/; max-age=${maxAgeSecs}; SameSite=Lax`;
}

/**
 * Retrieves the currently saved account ID.
 */
export function getSavedAccountId(): string | null {
  if (typeof localStorage !== 'undefined') {
    const aid = localStorage.getItem(LOCAL_STORAGE_ACCOUNT_ID_KEY);
    if (aid) return aid;
  }
  return null;
}

/**
 * Saves the current account ID.
 */
export function saveAccountId(accountId: string | number): void {
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(LOCAL_STORAGE_ACCOUNT_ID_KEY, String(accountId));
  }
}

/**
 * Retrieves the current session ID (cookie first, then localStorage fallback).
 */
export function getSavedSessionId(): string | null {
  const fromCookie = getSessionIdFromCookie();
  if (fromCookie) return fromCookie;
  if (typeof localStorage !== 'undefined') {
    return localStorage.getItem(LOCAL_STORAGE_SESSION_ID_KEY);
  }
  return null;
}

/**
 * Saves the current session ID to both cookie and localStorage.
 */
export function saveSessionId(sessionId: string | number): void {
  setSessionIdCookie(sessionId);
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(LOCAL_STORAGE_SESSION_ID_KEY, String(sessionId));
  }
}

/**
 * Builds request headers including X-Alhalte-Session and X-Alhalte-Account if available.
 */
export function buildAuthHeaders(
  accountId?: string | number | null,
  sessionId?: string | number | null,
): Record<string, string> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  const aid = accountId ?? getSavedAccountId();
  const sid = sessionId ?? getSavedSessionId();
  if (aid) {
    headers['X-Alhalte-Account'] = String(aid);
  }
  if (sid) {
    headers['X-Alhalte-Session'] = String(sid);
  }
  return headers;
}

/**
 * Searches accounts by handle name or ID.
 */
export async function searchAccounts(
  query: string,
  options?: {
    baseUrl?: string;
    fetchFn?: typeof fetch;
    accountId?: string | number | null;
    sessionId?: string | number | null;
  },
): Promise<AccountSearchItem[]> {
  const q = query.trim();
  if (!q) return [];

  const fetchFn = options?.fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!fetchFn) return [];

  const base = (options?.baseUrl || '').replace(/\/+$/, '');
  const url = `${base}/api/accounts/search?q=${encodeURIComponent(q)}`;

  const res = await fetchFn(url, {
    method: 'GET',
    headers: buildAuthHeaders(options?.accountId, options?.sessionId),
    credentials: 'include',
  });

  if (!res.ok) {
    throw new Error(`Failed to search accounts: ${res.status} ${res.statusText}`);
  }

  return (await res.json()) as AccountSearchItem[];
}

/**
 * Fetches the room permission list.
 */
export async function getRoomPermissions(
  roomId: string | number,
  options?: {
    baseUrl?: string;
    fetchFn?: typeof fetch;
    accountId?: string | number | null;
    sessionId?: string | number | null;
  },
): Promise<RoomPermissionItem[]> {
  const fetchFn = options?.fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!fetchFn) return [];

  const base = (options?.baseUrl || '').replace(/\/+$/, '');
  const url = `${base}/api/room/${encodeURIComponent(String(roomId))}/permissions`;

  const res = await fetchFn(url, {
    method: 'GET',
    headers: buildAuthHeaders(options?.accountId, options?.sessionId),
    credentials: 'include',
  });

  if (!res.ok) {
    throw new Error(`Failed to get room permissions: ${res.status} ${res.statusText}`);
  }

  return (await res.json()) as RoomPermissionItem[];
}

/**
 * Sets (adds or updates) a room permission for an account.
 */
export async function setRoomPermission(
  roomId: string | number,
  accountId: number | string,
  role: RoomRole,
  options?: {
    baseUrl?: string;
    fetchFn?: typeof fetch;
    /** Caller account ID (defaults to the saved account, never the target). */
    callerAccountId?: string | number | null;
    sessionId?: string | number | null;
  },
): Promise<void> {
  const fetchFn = options?.fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!fetchFn) return;

  const base = (options?.baseUrl || '').replace(/\/+$/, '');
  const url = `${base}/api/room/${encodeURIComponent(String(roomId))}/permissions`;

  const res = await fetchFn(url, {
    method: 'POST',
    // `accountId` is the target of the permission change, not the caller.
    // Sending it as X-Alhalte-Account would make maitre-d verify the target's
    // session and reject a GM editing another account.
    headers: buildAuthHeaders(options?.callerAccountId, options?.sessionId),
    credentials: 'include',
    body: JSON.stringify({
      account_id: Number(accountId),
      role,
    }),
  });

  if (!res.ok) {
    throw new Error(`Failed to set room permission: ${res.status} ${res.statusText}`);
  }
}

/**
 * Deletes a room permission for an account.
 */
export async function deleteRoomPermission(
  roomId: string | number,
  accountId: number | string,
  options?: {
    baseUrl?: string;
    fetchFn?: typeof fetch;
    /** Caller account ID (defaults to the saved account, never the target). */
    callerAccountId?: string | number | null;
    sessionId?: string | number | null;
  },
): Promise<void> {
  const fetchFn = options?.fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!fetchFn) return;

  const base = (options?.baseUrl || '').replace(/\/+$/, '');
  const url = `${base}/api/room/${encodeURIComponent(String(roomId))}/permissions/${encodeURIComponent(String(accountId))}`;

  const res = await fetchFn(url, {
    method: 'DELETE',
    // See setRoomPermission: the target account is not the caller identity.
    headers: buildAuthHeaders(options?.callerAccountId, options?.sessionId),
    credentials: 'include',
  });

  if (!res.ok) {
    throw new Error(`Failed to delete room permission: ${res.status} ${res.statusText}`);
  }
}
