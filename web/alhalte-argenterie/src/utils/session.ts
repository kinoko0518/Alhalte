/**
 * Account session and room permission utilities for @alhalte/argenterie.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-maitre-d -> ### セッション管理 / アカウント管理`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
 * - ARCHITECTURE.md: `## alhalte-banquette -> ### protocol`
 * - GitHub Issue #94: アカウントの概念をArgenterieと接続
 * - GitHub Issue #201: 4 段階権限 (admin / participant / viewer / denied)
 */

/**
 * Role of the authenticated caller inside one room.
 *
 * The four levels mirror banquette's `RoomPermissionLevel` and the `role` field
 * of `GET /api/rooms/{room_id}` (Issue #200). `admin` may do everything,
 * `participant` may write to the scenes the server moved it into, `viewer` is
 * read-only, and `denied` is refused entry (see `GateScreen`).
 */
export type RoomRole = 'admin' | 'participant' | 'viewer' | 'denied';

/**
 * Wire form of banquette's `RoomPermissionLevel`.
 *
 * `ClientFrame::SetMemberPermission` / `SetDefaultPermission` and
 * `ServerFrame::PermissionsSnapshot` / `PermissionsUpdated` serialize the level
 * with Serde's default (PascalCase) representation, while the HTTP preflight
 * role is lowercase. {@link roleToPermissionLevel} /
 * {@link permissionLevelToRole} convert between the two.
 */
export type RoomPermissionLevel = 'Admin' | 'Participant' | 'Viewer' | 'Denied';

/** Every role, weakest first (used by the permission selectors). */
export const ROOM_ROLES: readonly RoomRole[] = ['denied', 'viewer', 'participant', 'admin'];

/** Japanese labels for the four permission levels (UI). */
export const ROOM_ROLE_LABELS: Readonly<Record<RoomRole, string>> = {
  denied: '拒否',
  viewer: '閲覧者',
  participant: '参加者',
  admin: '管理者',
};

const LEVEL_TO_ROLE: Readonly<Record<RoomPermissionLevel, RoomRole>> = {
  Denied: 'denied',
  Viewer: 'viewer',
  Participant: 'participant',
  Admin: 'admin',
};

const ROLE_TO_LEVEL: Readonly<Record<RoomRole, RoomPermissionLevel>> = {
  denied: 'Denied',
  viewer: 'Viewer',
  participant: 'Participant',
  admin: 'Admin',
};

/**
 * Returns true when `value` is one of the four room roles.
 *
 * The legacy `gm` / `pl` roles are intentionally not accepted (Issue #201).
 */
export function isRoomRole(value: unknown): value is RoomRole {
  return value === 'admin' || value === 'participant' || value === 'viewer' || value === 'denied';
}

/** Returns true when `value` is a wire permission level. */
export function isRoomPermissionLevel(value: unknown): value is RoomPermissionLevel {
  return value === 'Admin' || value === 'Participant' || value === 'Viewer' || value === 'Denied';
}

/**
 * Converts a wire permission level into the lowercase UI role.
 *
 * Unknown values fall back to `denied`: an unparsable level must never grant
 * more than no access.
 */
export function permissionLevelToRole(level: unknown): RoomRole {
  return isRoomPermissionLevel(level) ? LEVEL_TO_ROLE[level] : 'denied';
}

/**
 * Converts a UI role into the wire permission level.
 *
 * Both the lowercase (`admin`) and PascalCase (`Admin`) spellings are accepted
 * so a snapshot value can be fed back into a set frame unchanged. Unknown
 * values map to `Denied`, never to a wider level.
 */
export function roleToPermissionLevel(role: unknown): RoomPermissionLevel {
  if (isRoomPermissionLevel(role)) return role;
  if (typeof role === 'string' && isRoomRole(role.toLowerCase())) {
    return ROLE_TO_LEVEL[role.toLowerCase() as RoomRole];
  }
  return 'Denied';
}

/**
 * UI capabilities derived from a {@link RoomRole}.
 *
 * `canSend` covers log / state writes to the scenes the server authorizes (the
 * server remains the final authority). `canManageScenes` covers scene creation,
 * deletion and member moves; `canManageExtensions` the extension tab;
 * `canManageRoom` the permission table and the default permission selector.
 */
export interface RoomRoleCapabilities {
  /** The role these capabilities were derived from. */
  role: RoomRole;
  /** `role === 'admin'`. */
  isAdmin: boolean;
  /** `role === 'participant'`. */
  isParticipant: boolean;
  /** `role === 'viewer'` only; a participant is never treated as a viewer. */
  isViewer: boolean;
  /** `role === 'denied'` (the room is replaced by `GateScreen`). */
  isDenied: boolean;
  /** May send log / state writes to accessible scenes and speak in chat. */
  canSend: boolean;
  /** May create / update / delete characters. */
  canManageCharacters: boolean;
  /** May create or delete scenes and move members between scenes. */
  canManageScenes: boolean;
  /** May add / remove / reorder / toggle extensions. */
  canManageExtensions: boolean;
  /** May change the room permission table (members and the default level). */
  canManageRoom: boolean;
}

/**
 * Derives the UI capabilities of a role.
 *
 * `null` / `undefined` (before the preflight answers) behaves like `denied`:
 * nothing is enabled until the server says otherwise.
 */
export function roomRoleCapabilities(role: RoomRole | null | undefined): RoomRoleCapabilities {
  const resolved: RoomRole = role ?? 'denied';
  const isAdmin = resolved === 'admin';
  const isParticipant = resolved === 'participant';
  const isViewer = resolved === 'viewer';
  const isDenied = resolved === 'denied';
  return {
    role: resolved,
    isAdmin,
    isParticipant,
    isViewer,
    isDenied,
    canSend: isAdmin || isParticipant,
    canManageCharacters: isAdmin || isParticipant,
    canManageScenes: isAdmin,
    canManageExtensions: isAdmin,
    canManageRoom: isAdmin,
  };
}

export interface AccountSearchItem {
  account_id: number;
  username: string;
  icon_url?: string | null;
}

/**
 * One row of `RoomPermissionModal`.
 *
 * `username` is resolved from `/api/accounts/search` because the permission
 * snapshot only carries account IDs; an unresolved name falls back to
 * `#(account_id)` in the UI. `role` is the lowercase UI role, not the wire
 * level.
 */
export interface RoomPermissionItem {
  account_id: number;
  username?: string | null;
  icon_url?: string | null;
  role: RoomRole;
}

/** Decoded permission table of the room. */
export interface RoomPermissionsSnapshot {
  /** Level applied to accounts without an explicit entry. */
  defaultPermission: RoomRole;
  /** Explicit per-account levels. */
  permissions: RoomPermissionItem[];
}

/**
 * Builds the permission table from a `PermissionsSnapshot` /
 * `PermissionsUpdated` payload.
 *
 * The wire map is keyed by account ID as a string (`BTreeMap<u64, _>` JSON),
 * and the levels use Serde's PascalCase enum names. Both are normalized here so
 * the UI never sees a wire-level spelling.
 */
export function permissionsSnapshotToTable(snapshot: {
  default_permission?: unknown;
  permissions?: Record<string, unknown> | null;
}): RoomPermissionsSnapshot {
  const entries = snapshot.permissions ?? {};
  const permissions: RoomPermissionItem[] = [];
  for (const [key, value] of Object.entries(entries)) {
    const accountId = Number(key);
    if (!Number.isFinite(accountId)) continue;
    permissions.push({ account_id: accountId, role: permissionLevelToRole(value) });
  }
  permissions.sort((a, b) => a.account_id - b.account_id);
  return {
    defaultPermission: permissionLevelToRole(snapshot.default_permission),
    permissions,
  };
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
 *
 * This is the only HTTP call of the permission UI: the permission table itself
 * is read and written over the room WebSocket (Issue #201). A numeric query
 * resolves the account by ID, which is how the modal turns a snapshot account
 * ID into a display name.
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
