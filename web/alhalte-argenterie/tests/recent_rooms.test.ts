/**
 * Unit tests for Recent Rooms visit history management and HomeScreen re-entry integration.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### ホーム画面`
 * - GitHub Issue #58: fix(argenterie): 最近訪れた部屋の機能化と再入室連携
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  getRecentRooms,
  saveRecentRoom,
  recordRoomVisit,
  removeRecentRoom,
  clearRecentRooms,
  RECENT_ROOMS_STORAGE_KEY,
  MAX_RECENT_ROOMS,
  type RecentRoom,
} from '../src/utils/recentRooms.js';
import HomeScreen from '../src/components/Home/HomeScreen.svelte';
import App from '../src/components/App.svelte';

describe('Recent Rooms visit history management (Issue #58)', () => {
  beforeEach(() => {
    localStorage.clear();
    window.history.replaceState(null, '', '/');
  });

  afterEach(() => {
    window.history.replaceState(null, '', '/');
  });

  // spec: alhalte-argenterie/recent-rooms 部屋入室時の履歴保存、重複時の更新、ソート順（最新順）
  it('saves room visit info to localStorage under alhalte_recent_rooms, updates duplicates, and sorts by latest visited', () => {
    const roomA: RecentRoom = {
      roomId: 'room-alpha',
      roomName: '深淵の迷宮',
      visitedAt: 1000,
      thumbnailUrl: 'https://example.com/thumb-a.png',
      ownerAccountId: 'owner-1',
      url: 'https://alhalte.example/rooms/room-alpha#key-a',
      isAdmin: false,
    };

    const roomB: RecentRoom = {
      roomId: 'room-beta',
      roomName: '陽光の都',
      visitedAt: 2000,
      thumbnailUrl: 'https://example.com/thumb-b.png',
      ownerAccountId: 'owner-2',
      url: 'https://alhalte.example/rooms/room-beta#key-b',
      isAdmin: true,
    };

    saveRecentRoom(roomA);
    saveRecentRoom(roomB);

    // Verify localStorage key and content
    const raw = localStorage.getItem(RECENT_ROOMS_STORAGE_KEY);
    expect(raw).toBeTruthy();
    const stored = JSON.parse(raw!);
    expect(stored).toHaveLength(2);

    // Should be sorted latest first: roomB (2000), roomA (1000)
    const list = getRecentRooms();
    expect(list[0].roomId).toBe('room-beta');
    expect(list[0].roomName).toBe('陽光の都');
    expect(list[0].thumbnailUrl).toBe('https://example.com/thumb-b.png');
    expect(list[0].ownerAccountId).toBe('owner-2');
    expect(list[1].roomId).toBe('room-alpha');

    // Duplicate update: visit roomA again with newer timestamp
    const updatedA = recordRoomVisit({
      roomId: 'room-alpha',
      roomName: '深淵の迷宮 (再訪)',
      visitedAt: 3000,
    });

    const updatedList = getRecentRooms();
    expect(updatedList).toHaveLength(2); // Still 2 items, no duplicate
    expect(updatedList[0].roomId).toBe('room-alpha');
    expect(updatedList[0].roomName).toBe('深淵の迷宮 (再訪)');
    expect(updatedList[0].visitedAt).toBe(3000);
    expect(updatedList[1].roomId).toBe('room-beta');
  });

  // spec: alhalte-argenterie/recent-rooms 最大保存件数（20件）の管理
  it('limits recent rooms to MAX_RECENT_ROOMS (20) and evicts oldest items', () => {
    for (let i = 1; i <= 25; i++) {
      saveRecentRoom({
        roomId: `room-${i}`,
        roomName: `セッション部屋 #${i}`,
        visitedAt: 1000 + i * 10,
        url: `https://alhalte.example/rooms/room-${i}#key-${i}`,
      });
    }

    const rooms = getRecentRooms();
    expect(rooms).toHaveLength(MAX_RECENT_ROOMS);
    expect(rooms.length).toBe(20);

    // Newest room (room-25, visitedAt: 1250) is first
    expect(rooms[0].roomId).toBe('room-25');
    // Oldest preserved room (room-6, visitedAt: 1060) is 20th
    expect(rooms[19].roomId).toBe('room-6');
    // Older rooms (room-1 to room-5) must be evicted
    expect(rooms.some((r) => r.roomId === 'room-1')).toBe(false);
    expect(rooms.some((r) => r.roomId === 'room-5')).toBe(false);
  });

  // spec: alhalte-argenterie/recent-rooms HomeScreenにおける最近訪れた部屋の一覧描画
  it('renders recent rooms list in HomeScreen with name, ID chip, thumbnail, and owner chip', async () => {
    saveRecentRoom({
      roomId: 'room-quest-01',
      roomName: '古城の探索',
      visitedAt: Date.now() - 5000,
      thumbnailUrl: 'https://example.com/castle.png',
      ownerAccountId: 'gm-user-01',
      url: 'https://alhalte.example/rooms/room-quest-01#secret-key',
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Card should be rendered
    const cardEl = target.querySelector('[data-testid="room-card-room-quest-01"]');
    expect(cardEl).toBeTruthy();
    expect(cardEl?.textContent).toContain('古城の探索');
    expect(cardEl?.textContent).toContain('ID: room-que');
    expect(cardEl?.textContent).toContain('Owner: gm-user-01');

    // Thumbnail image should be rendered
    const imgEl = cardEl?.querySelector<HTMLImageElement>('img.room-avatar-img');
    expect(imgEl).toBeTruthy();
    expect(imgEl?.src).toBe('https://example.com/castle.png');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/recent-rooms 履歴アイテムのクリックで該当 roomId による入室コールバックが発火
  it('triggers onSelectRoom and onEnterRoom callback with matching roomId on item click', async () => {
    saveRecentRoom({
      roomId: 'room-dungeon-99',
      roomName: '地下迷宮B3F',
      visitedAt: Date.now(),
      url: 'https://alhalte.example/rooms/room-dungeon-99#dungeon-key',
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    let selectedRoom: RecentRoom | null = null;
    let enteredRoom: RecentRoom | null = null;

    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        onSelectRoom: (r) => {
          selectedRoom = r as RecentRoom;
        },
        onEnterRoom: (r) => {
          enteredRoom = r as RecentRoom;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Click "入室する" button or card
    const enterBtn = target.querySelector<HTMLButtonElement>(
      '[data-testid="enter-room-room-dungeon-99"]',
    );
    expect(enterBtn).toBeTruthy();
    enterBtn?.click();

    await new Promise((r) => setTimeout(r, 20));

    expect(selectedRoom).toBeTruthy();
    expect(selectedRoom?.roomId).toBe('room-dungeon-99');
    expect(selectedRoom?.roomName).toBe('地下迷宮B3F');

    expect(enteredRoom).toBeTruthy();
    expect(enteredRoom?.roomId).toBe('room-dungeon-99');
    expect(enteredRoom?.roomName).toBe('地下迷宮B3F');

    // Also verify clicking the card directly invokes the callbacks
    selectedRoom = null;
    enteredRoom = null;
    const cardEl = target.querySelector<HTMLElement>('[data-testid="room-card-room-dungeon-99"]');
    cardEl?.click();

    expect(selectedRoom?.roomId).toBe('room-dungeon-99');
    expect(enteredRoom?.roomId).toBe('room-dungeon-99');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/recent-rooms 個別削除ボタンで該当履歴が消去されること
  it('deletes specific room history from storage and UI when delete IconButton is clicked without triggering re-entry', async () => {
    saveRecentRoom({
      roomId: 'room-to-delete',
      roomName: '消去対象ルーム',
      visitedAt: Date.now() - 10000,
      url: 'https://alhalte.example/rooms/room-to-delete#key',
    });
    saveRecentRoom({
      roomId: 'room-to-keep',
      roomName: '保持ルーム',
      visitedAt: Date.now(),
      url: 'https://alhalte.example/rooms/room-to-keep#key',
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    let reEntryCalled = false;
    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        onEnterRoom: () => {
          reEntryCalled = true;
        },
        onSelectRoom: () => {
          reEntryCalled = true;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="room-card-room-to-delete"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="room-card-room-to-keep"]')).toBeTruthy();

    // Click delete IconButton on 'room-to-delete'
    const deleteBtn = target.querySelector<HTMLButtonElement>(
      '[data-testid="remove-room-room-to-delete"]',
    );
    expect(deleteBtn).toBeTruthy();
    deleteBtn?.click();

    await new Promise((r) => setTimeout(r, 20));

    // Ensure re-entry callback was NOT triggered
    expect(reEntryCalled).toBe(false);

    // Ensure room is removed from localStorage
    const remainingRooms = getRecentRooms();
    expect(remainingRooms.some((r) => r.roomId === 'room-to-delete')).toBe(false);
    expect(remainingRooms.some((r) => r.roomId === 'room-to-keep')).toBe(true);

    // Ensure room is no longer in DOM
    expect(target.querySelector('[data-testid="room-card-room-to-delete"]')).toBeNull();
    expect(target.querySelector('[data-testid="room-card-room-to-keep"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/recent-rooms App連携: 部屋入室時の自動履歴保存
  it('automatically registers and updates recent room history in App.svelte when entering a room', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testRoom = {
      id: 'app-auto-room-01',
      roomId: 'app-auto-room-01',
      name: 'セッション本会場',
      roomName: 'セッション本会場',
      url: 'https://alhalte.example/rooms/app-auto-room-01#token123',
      lastVisited: Date.now(),
      visitedAt: Date.now(),
      isAdmin: false,
    };

    const instance = mount(App, {
      target,
      props: {
        initialRoom: testRoom,
      },
    });

    await new Promise((r) => setTimeout(r, 50));

    // Verify room was saved to alhalte_recent_rooms
    const recent = getRecentRooms();
    expect(recent.some((r) => r.roomId === 'app-auto-room-01')).toBe(true);
    const item = recent.find((r) => r.roomId === 'app-auto-room-01');
    expect(item?.roomName).toBe('セッション本会場');

    unmount(instance);
    target.remove();
  });
});
