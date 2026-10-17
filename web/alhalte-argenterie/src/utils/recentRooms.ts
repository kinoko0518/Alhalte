/**
 * Room visit history management utilities.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### ホーム画面`
 * - GitHub Issue #58: fix(argenterie): 最近訪れた部屋の機能化と再入室連携
 */

export const RECENT_ROOMS_STORAGE_KEY = 'alhalte_recent_rooms';
export const MAX_RECENT_ROOMS = 20;

export interface RecentRoom {
  roomId: string;
  roomName: string;
  visitedAt: number;
  thumbnailUrl?: string;
  ownerAccountId?: string;
  url?: string;
  roomUrl?: string;
  isAdmin?: boolean;
  // Aliases for compatibility with VisitedRoom
  id?: string;
  name?: string;
  lastVisited?: number;
}

/**
 * Normalizes a recent room record to ensure all expected properties and aliases exist.
 */
export function normalizeRecentRoom(
  data: Partial<RecentRoom> & { roomId?: string; id?: string },
): RecentRoom {
  const roomId = String(data.roomId ?? data.id ?? '').trim();
  const roomName = String(data.roomName ?? data.name ?? '名称未設定').trim() || '名称未設定';
  const visitedAt =
    typeof data.visitedAt === 'number'
      ? data.visitedAt
      : typeof data.lastVisited === 'number'
        ? data.lastVisited
        : Date.now();
  const url = data.url ?? data.roomUrl ?? '';

  return {
    roomId,
    roomName,
    visitedAt,
    thumbnailUrl: data.thumbnailUrl,
    ownerAccountId: data.ownerAccountId,
    url,
    roomUrl: url,
    isAdmin: Boolean(data.isAdmin),
    // Aliases
    id: roomId,
    name: roomName,
    lastVisited: visitedAt,
  };
}

/**
 * Loads recent rooms from localStorage, ordered by visitedAt descending (latest first).
 */
export function getRecentRooms(storage?: Storage): RecentRoom[] {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return [];

  const raw = store.getItem(RECENT_ROOMS_STORAGE_KEY);
  let parsed: any[] = [];
  if (raw) {
    try {
      const json = JSON.parse(raw);
      if (Array.isArray(json)) {
        parsed = json;
      }
    } catch {
      parsed = [];
    }
  }

  // Fallback: If no recent rooms in new key, check legacy visited rooms key
  if (parsed.length === 0) {
    const legacyRaw = store.getItem('alhalte:visited-rooms');
    if (legacyRaw) {
      try {
        const legacyJson = JSON.parse(legacyRaw);
        if (Array.isArray(legacyJson)) {
          parsed = legacyJson;
        }
      } catch {
        // ignore
      }
    }
  }

  return parsed
    .filter((item) => item && (item.roomId || item.id))
    .map(normalizeRecentRoom)
    .sort((a, b) => b.visitedAt - a.visitedAt)
    .slice(0, MAX_RECENT_ROOMS);
}

/**
 * Saves or updates a room in the recent rooms list in localStorage.
 * Updates visitedAt timestamp, moves/sorts to top, and limits to MAX_RECENT_ROOMS (20).
 */
export function saveRecentRoom(
  room: Partial<RecentRoom> & { roomId?: string; id?: string },
  storage?: Storage,
): RecentRoom[] {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return [];

  const normalized = normalizeRecentRoom(room);
  if (!normalized.roomId) return getRecentRooms(store);

  const current = getRecentRooms(store);
  const existingIdx = current.findIndex(
    (item) => item.roomId === normalized.roomId || item.id === normalized.roomId,
  );

  let updated: RecentRoom[];
  if (existingIdx !== -1) {
    // Merge existing fields with new updates, preserving updated visitedAt
    const existing = current[existingIdx];
    const merged: RecentRoom = normalizeRecentRoom({
      ...existing,
      ...normalized,
      visitedAt: room.visitedAt !== undefined ? room.visitedAt : Date.now(),
    });
    // Remove existing and prepend
    const withoutExisting = current.filter((_, idx) => idx !== existingIdx);
    updated = [merged, ...withoutExisting];
  } else {
    updated = [normalized, ...current];
  }

  // Sort descending by visitedAt and truncate to MAX_RECENT_ROOMS
  updated.sort((a, b) => b.visitedAt - a.visitedAt);
  const truncated = updated.slice(0, MAX_RECENT_ROOMS);

  try {
    store.setItem(RECENT_ROOMS_STORAGE_KEY, JSON.stringify(truncated));
  } catch {
    // Ignore storage quota errors
  }

  return truncated;
}

/**
 * Records a room visit with current timestamp and saves to localStorage.
 */
export function recordRoomVisit(
  room: Partial<RecentRoom> & { roomId?: string; id?: string },
  storage?: Storage,
): RecentRoom {
  const normalized = normalizeRecentRoom({
    ...room,
    visitedAt: room.visitedAt ?? Date.now(),
  });
  saveRecentRoom(normalized, storage);
  return normalized;
}

/**
 * Removes a room from the recent rooms list in localStorage.
 */
export function removeRecentRoom(roomId: string, storage?: Storage): RecentRoom[] {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return [];

  const current = getRecentRooms(store);
  const filtered = current.filter((item) => item.roomId !== roomId && item.id !== roomId);

  try {
    store.setItem(RECENT_ROOMS_STORAGE_KEY, JSON.stringify(filtered));
  } catch {
    // Ignore storage errors
  }

  return filtered;
}

/**
 * Clears all recent rooms history from localStorage.
 */
export function clearRecentRooms(storage?: Storage): void {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return;
  try {
    store.removeItem(RECENT_ROOMS_STORAGE_KEY);
  } catch {
    // Ignore
  }
}
