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
}

/**
 * Removes a visited room from localStorage.
 */
export function removeVisitedRoom(roomId: string, storage?: Storage): void {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return;

  const raw = store.getItem(LOCAL_STORAGE_VISITED_ROOMS_KEY);
  if (!raw) return;

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
