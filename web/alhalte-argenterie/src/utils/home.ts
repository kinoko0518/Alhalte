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

export const LOCAL_STORAGE_VISITED_ROOMS_KEY = 'alhalte:visited-rooms';

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
  let roomId = customRoomId;
  if (!roomId) {
    const idBytes = new Uint8Array(8);
    crypto.getRandomValues(idBytes);
    roomId = bytesToHex(idBytes);
  }

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
 * Excludes rooms where the user is an admin (`isAdmin === true`).
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

