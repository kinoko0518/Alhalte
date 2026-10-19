/**
 * Home screen and room URL utilities.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### ホーム画面`
 * - ARCHITECTURE.md: `## tin-rs -> ### opener (0010)`
 * - ARCHITECTURE.md: `## alhalte-maitre-d`
 */

import { bytesToHex } from '@alhalte/pantry';
import type { VisitedRoom } from '../types/index.js';
import { removeRecentRoom, saveRecentRoom } from './recentRooms.js';
import { buildAuthHeaders, type RoomRole } from './session.js';

export const LOCAL_STORAGE_VISITED_ROOMS_KEY = 'alhalte:visited-rooms';

/**
 * Room metadata returned by `POST /api/rooms` and `GET /api/rooms/{room_id}`.
 *
 * Reference: `crates/alhalte-maitre-d/ARCHITECTURE.md` (`RoomInfoResponse`).
 */
export interface RoomInfo {
  /** Canonical 16-hex room ID. */
  room_id: string;
  /** Human-readable room name. */
  name: string;
  /** Role of the authenticated caller in the room. */
  role: RoomRole;
  /** Whether the room is marked done (fully public). */
  done: boolean;
}

/**
 * Failure classification for the room registry API.
 *
 * `unauthorized` / `forbidden` / `not-found` mirror the HTTP statuses the
 * server uses; `network` means the request never produced a response and may be
 * retried, while `http` / `invalid-response` are protocol failures.
 */
export type RoomApiErrorKind =
  | 'unauthorized'
  | 'forbidden'
  | 'not-found'
  | 'http'
  | 'network'
  | 'invalid-response';

/**
 * Typed error raised by {@link createRoom} / {@link fetchRoomInfo}.
 *
 * The status is `null` for network failures (no response was received).
 */
export class RoomApiError extends Error {
  readonly kind: RoomApiErrorKind;
  readonly status: number | null;

  constructor(kind: RoomApiErrorKind, message: string, status: number | null = null) {
    super(message);
    this.name = 'RoomApiError';
    this.kind = kind;
    this.status = status;
  }
}

/** Options shared by the room registry helpers. */
export interface RoomApiOptions {
  /** Optional API base URL (tests / reverse-proxy prefixes). */
  baseUrl?: string;
  /** Injectable fetch implementation for tests. */
  fetchFn?: typeof fetch;
  /** Account ID override (defaults to the saved account). */
  accountId?: string | number | null;
  /** Session ID override (defaults to the saved session). */
  sessionId?: string | number | null;
}

/** Options for {@link createRoom}. */
export interface CreateRoomOptions extends RoomApiOptions {
  /**
   * Optional client-generated 16-hex room ID.
   *
   * The Home screen generates one so the invitation URL can be previewed before
   * the server registers the room; the server always returns the canonical ID.
   */
  roomId?: string;
}

function resolveFetch(fetchFn?: typeof fetch): typeof fetch {
  const resolved = fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!resolved) {
    throw new RoomApiError('network', 'fetch is not available in this environment');
  }
  return resolved;
}

function statusErrorKind(status: number): RoomApiErrorKind {
  if (status === 401) return 'unauthorized';
  if (status === 403) return 'forbidden';
  if (status === 404) return 'not-found';
  return 'http';
}

function isRoomRole(value: unknown): value is RoomRole {
  return value === 'gm' || value === 'pl' || value === 'viewer';
}

/** Validates the room registry response before it reaches the UI. */
function parseRoomInfo(payload: unknown): RoomInfo {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) {
    throw new RoomApiError('invalid-response', 'Room response must be a JSON object');
  }
  const record = payload as Record<string, unknown>;
  const roomId = record.room_id;
  const name = record.name;
  const role = record.role;
  const done = record.done;
  if (typeof roomId !== 'string' || !isValidRoomId(roomId)) {
    throw new RoomApiError('invalid-response', 'Room response is missing a valid room_id');
  }
  if (typeof name !== 'string') {
    throw new RoomApiError('invalid-response', 'Room response is missing a room name');
  }
  if (!isRoomRole(role)) {
    throw new RoomApiError('invalid-response', 'Room response is missing a valid role');
  }
  if (typeof done !== 'boolean') {
    throw new RoomApiError('invalid-response', 'Room response is missing the done flag');
  }
  return { room_id: roomId, name, role, done };
}

async function requestRoomJson(
  url: string,
  init: RequestInit,
  fetchFn: typeof fetch,
): Promise<RoomInfo> {
  let response: Response;
  try {
    response = await fetchFn(url, init);
  } catch (error) {
    throw new RoomApiError(
      'network',
      `Room request failed: ${error instanceof Error ? error.message : String(error)}`,
    );
  }

  if (!response.ok) {
    throw new RoomApiError(
      statusErrorKind(response.status),
      `Room request failed with HTTP ${response.status} ${response.statusText}`,
      response.status,
    );
  }

  let payload: unknown;
  try {
    payload = await response.json();
  } catch (error) {
    throw new RoomApiError(
      'invalid-response',
      `Room response is not valid JSON: ${error instanceof Error ? error.message : String(error)}`,
      response.status,
    );
  }
  return parseRoomInfo(payload);
}

/**
 * Registers a room on `alhalte-maitre-d` (`POST /api/rooms`).
 *
 * Requires an authenticated session: the server derives the creator and its
 * initial `gm` role from the session, never from the request body.
 *
 * @throws {RoomApiError} with `unauthorized`, `forbidden`, `http`, `network`, or
 * `invalid-response`.
 */
export async function createRoom(
  name: string,
  options: CreateRoomOptions = {},
): Promise<RoomInfo> {
  const fetchFn = resolveFetch(options.fetchFn);
  const base = (options.baseUrl || '').replace(/\/+$/, '');
  const body: { name: string; room_id?: string } = { name };
  if (options.roomId) {
    body.room_id = options.roomId;
  }

  return requestRoomJson(
    `${base}/api/rooms`,
    {
      method: 'POST',
      headers: buildAuthHeaders(options.accountId, options.sessionId),
      credentials: 'include',
      body: JSON.stringify(body),
    },
    fetchFn,
  );
}

/**
 * Fetches room metadata (`GET /api/rooms/{room_id}`).
 *
 * Used as the join preflight so a 403 (`forbidden`) or 404 (`not-found`) is
 * known before the WebSocket is opened.
 *
 * @throws {RoomApiError} with the same classifications as {@link createRoom}.
 */
export async function fetchRoomInfo(
  roomId: string,
  options: RoomApiOptions = {},
): Promise<RoomInfo> {
  const fetchFn = resolveFetch(options.fetchFn);
  const base = (options.baseUrl || '').replace(/\/+$/, '');
  return requestRoomJson(
    `${base}/api/rooms/${encodeURIComponent(String(roomId))}`,
    {
      method: 'GET',
      headers: buildAuthHeaders(options.accountId, options.sessionId),
      credentials: 'include',
    },
    fetchFn,
  );
}

/**
 * Generates a client-side room invitation URL with a 256-bit random room key in the URL fragment.
 *
 * @param baseUrl Base URL (e.g. window.location.origin)
 * @param customRoomId Optional custom 64-bit room ID (hex or numeric string)
 * @param customRoomKey Optional custom 32-byte (256-bit) room key
 */
export function generateRoomUrl(
  baseUrl: string,
  customRoomId?: string,
  customRoomKey?: Uint8Array,
): {
  roomId: string;
  roomKeyHex: string;
  roomUrl: string;
} {
  // Generate 64-bit (8 bytes = 16 hex chars) room ID if not supplied
  const roomId =
    customRoomId ??
    (() => {
      const idBytes = new Uint8Array(8);
      crypto.getRandomValues(idBytes);
      return bytesToHex(idBytes);
    })();

  // Generate 256-bit (32 bytes = 64 hex chars) room key if not supplied
  const keyBytes = customRoomKey ?? new Uint8Array(32);
  if (!customRoomKey) {
    crypto.getRandomValues(keyBytes);
  }
  const roomKeyHex = bytesToHex(keyBytes);

  // Clean base URL trailing slash
  const cleanBase = baseUrl.replace(/\/+$/, '');
  const roomUrl = `${cleanBase}/rooms/${roomId}#${roomKeyHex}`;

  return {
    roomId,
    roomKeyHex,
    roomUrl,
  };
}

/**
 * Loads visited rooms from localStorage.
 *
 * Excludes rooms where the user is an admin (`isAdmin === true`). This is a
 * legacy display preference kept for the stored room list; it is not an
 * authority for room access or the GM UI (Issue #156). The server role from
 * `GET /api/rooms/{id}` is authoritative.
 *
 * ARCHITECTURE.md: "自身が管理者である部屋のリストの表示は行わず、localStorageに保存された、訪れたことのある部屋のリストをカードとして表示"
 */
export function getVisitedRooms(storage?: Storage): VisitedRoom[] {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return [];

  const raw = store.getItem(LOCAL_STORAGE_VISITED_ROOMS_KEY);
  if (!raw) return [];

  try {
    const parsed = JSON.parse(raw) as VisitedRoom[];
    if (!Array.isArray(parsed)) return [];

    // Filter out rooms where user is an admin
    return parsed
      .filter((r) => !r.isAdmin)
      .sort((a, b) => b.lastVisited - a.lastVisited);
  } catch {
    return [];
  }
}

/**
 * Saves or updates a visited room in localStorage.
 */
export function saveVisitedRoom(room: VisitedRoom, storage?: Storage): void {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return;

  const raw = store.getItem(LOCAL_STORAGE_VISITED_ROOMS_KEY);
  let list: VisitedRoom[] = [];
  if (raw) {
    try {
      list = JSON.parse(raw);
      if (!Array.isArray(list)) list = [];
    } catch {
      list = [];
    }
  }

  const existingIdx = list.findIndex((item) => item.id === room.id);
  if (existingIdx !== -1) {
    list[existingIdx] = { ...list[existingIdx], ...room, lastVisited: Date.now() };
  } else {
    list.unshift({ ...room, lastVisited: Date.now() });
  }

  store.setItem(LOCAL_STORAGE_VISITED_ROOMS_KEY, JSON.stringify(list));

  saveRecentRoom(
    {
      roomId: room.id,
      roomName: room.name,
      url: room.url,
      visitedAt: room.lastVisited || Date.now(),
      isAdmin: room.isAdmin,
    },
    store,
  );
}

/**
 * Removes a visited room from localStorage.
 */
export function removeVisitedRoom(roomId: string, storage?: Storage): void {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return;

  const raw = store.getItem(LOCAL_STORAGE_VISITED_ROOMS_KEY);
  if (raw) {
    try {
      const list = JSON.parse(raw) as VisitedRoom[];
      if (Array.isArray(list)) {
        const filtered = list.filter((item) => item.id !== roomId);
        store.setItem(LOCAL_STORAGE_VISITED_ROOMS_KEY, JSON.stringify(filtered));
      }
    } catch {
      // Ignore corrupt store
    }
  }

  removeRecentRoom(roomId, store);
}

/**
 * Canonical room ID form: 16 lowercase/uppercase hexadecimal characters.
 *
 * Reference: `crates/alhalte-maitre-d/ARCHITECTURE.md` (room ID contract).
 */
export const ROOM_ID_HEX_PATTERN = /^[0-9a-f]{16}$/i;

/**
 * Returns true when `value` is either a canonical 16-hex room ID or a legacy
 * decimal room ID that fits in a `u64`.
 *
 * A 16-character all-digit string is treated as canonical hex (the same rule
 * the Rust `RoomId` parser uses).
 */
export function isValidRoomId(value: string): boolean {
  const roomId = value.trim();
  if (ROOM_ID_HEX_PATTERN.test(roomId)) return true;
  if (!/^[0-9]{1,20}$/.test(roomId)) return false;
  try {
    return BigInt(roomId) <= 0xffffffffffffffffn;
  } catch {
    return false;
  }
}

/**
 * Parses a room invitation URL or path and extracts `roomId`, `roomKeyHex`, and `roomUrl`.
 *
 * Supports:
 * - `/rooms/<roomId>#<roomKeyHex>`
 * - `/room/<roomId>#<roomKeyHex>`
 * - `/?room=<roomId>#<roomKeyHex>`
 *
 * `<roomId>` must be a canonical 16-hex room ID (or a legacy decimal ID);
 * arbitrary strings are rejected by returning `null`.
 */
export function parseRoomUrl(urlStr: string): {
  roomId: string;
  roomKeyHex: string;
  roomUrl: string;
} | null {
  if (!urlStr || typeof urlStr !== 'string') return null;

  try {
    const url = new URL(urlStr, 'https://alhalte.local');
    let rawRoomId: string | null = null;

    const pathMatch = url.pathname.match(/^\/rooms?\/([^/?#]+)\/?$/);
    if (pathMatch && pathMatch[1]) {
      rawRoomId = decodeURIComponent(pathMatch[1]);
    } else {
      const queryRoom = url.searchParams.get('room');
      if (queryRoom && queryRoom.trim().length > 0) {
        rawRoomId = queryRoom.trim();
      }
    }

    if (!rawRoomId) return null;
    const roomId = rawRoomId.trim();
    if (!isValidRoomId(roomId)) return null;

    const roomKeyHex = url.hash.startsWith('#') ? url.hash.slice(1) : url.hash;
    const isAbsolute = /^https?:\/\//i.test(urlStr);

    return {
      roomId,
      roomKeyHex,
      roomUrl: isAbsolute ? url.href : `${url.pathname}${url.search}${url.hash}`,
    };
  } catch {
    return null;
  }
}

/**
 * Inspects the current browser Location (or supplied Location), parses any room URL,
 * restores or registers the VisitedRoom entry in localStorage, and returns it.
 */
export function resolveRoomFromLocation(
  loc?: Pick<Location, 'href'>,
  storage?: Storage,
): VisitedRoom | null {
  const targetLoc = loc ?? (typeof window !== 'undefined' ? window.location : undefined);
  if (!targetLoc || !targetLoc.href) return null;

  const parsed = parseRoomUrl(targetLoc.href);
  if (!parsed) return null;

  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  let existing: VisitedRoom | undefined;

  if (store) {
    const raw = store.getItem(LOCAL_STORAGE_VISITED_ROOMS_KEY);
    if (raw) {
      try {
        const list = JSON.parse(raw) as VisitedRoom[];
        if (Array.isArray(list)) {
          existing = list.find((item) => item.id === parsed.roomId);
        }
      } catch {
        // Ignore corrupt store
      }
    }
  }

  const resolvedRoom: VisitedRoom = existing
    ? {
        ...existing,
        url: targetLoc.href,
        lastVisited: Date.now(),
      }
    : {
        id: parsed.roomId,
        name: `Room ${parsed.roomId.slice(0, 8)}`,
        url: targetLoc.href,
        lastVisited: Date.now(),
        isAdmin: false,
      };

  saveVisitedRoom(resolvedRoom, store);
  return resolvedRoom;
}

