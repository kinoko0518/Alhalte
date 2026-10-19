import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  generateRoomUrl,
  getVisitedRooms,
  saveVisitedRoom,
  removeVisitedRoom,
  parseRoomUrl,
  resolveRoomFromLocation,
} from '../src/utils/home.js';
import HomeScreen from '../src/components/Home/HomeScreen.svelte';
import App from '../src/components/App.svelte';
import type { VisitedRoom } from '../src/types/index.js';

describe('Home Screen & Room URL', () => {
  beforeEach(() => {
    localStorage.clear();
    window.history.replaceState(null, '', '/');
  });

  afterEach(() => {
    window.history.replaceState(null, '', '/');
  });

  // spec: alhalte-argenterie/home 招待URL生成・URLフラグメント
  it('generates room URL with 64-bit ID and 256-bit key in URL fragment', () => {
    const origin = 'https://alhalte.example';
    const result = generateRoomUrl(origin);

    expect(result.roomId).toMatch(/^[0-9a-f]{16}$/);
    expect(result.roomKeyHex).toMatch(/^[0-9a-f]{64}$/);
    expect(result.roomUrl).toBe(`${origin}/rooms/${result.roomId}#${result.roomKeyHex}`);
  });

  // spec: alhalte-argenterie/home 招待URL解析 (parseRoomUrl)
  it('parses room URLs in /rooms/<id>#<key>, /room/<id>#<key>, and /?room=<id>#<key> formats', () => {
    const key = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';

    const p1 = parseRoomUrl(`https://alhalte.example/rooms/0102030405060708#${key}`);
    expect(p1).toEqual({
      roomId: '0102030405060708',
      roomKeyHex: key,
      roomUrl: `https://alhalte.example/rooms/0102030405060708#${key}`,
    });

    const p2 = parseRoomUrl(`/room/abcdef0123456789#${key}`);
    expect(p2).toEqual({
      roomId: 'abcdef0123456789',
      roomKeyHex: key,
      roomUrl: `/room/abcdef0123456789#${key}`,
    });

    const p3 = parseRoomUrl(`http://127.0.0.1:5173/?room=feedfacecafebeef#${key}`);
    expect(p3).toEqual({
      roomId: 'feedfacecafebeef',
      roomKeyHex: key,
      roomUrl: `http://127.0.0.1:5173/?room=feedfacecafebeef#${key}`,
    });

    // Legacy decimal room IDs are still accepted.
    const p4 = parseRoomUrl(`/rooms/777#${key}`);
    expect(p4).toEqual({
      roomId: '777',
      roomKeyHex: key,
      roomUrl: `/rooms/777#${key}`,
    });

    // Invalid / decorative room IDs are rejected explicitly.
    expect(parseRoomUrl(`https://alhalte.example/rooms/room-1#${key}`)).toBeNull();
    expect(parseRoomUrl(`/room/not-a-room#${key}`)).toBeNull();
    expect(parseRoomUrl(`/?room=visited-room-1#${key}`)).toBeNull();
    expect(parseRoomUrl('https://alhalte.example/')).toBeNull();
  });

  // spec: alhalte-argenterie/home URLからの部屋解決とlocalStorage連携 (resolveRoomFromLocation)
  it('resolves room from location for new links and preserves existing admin room metadata', () => {
    const key = 'fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210';
    const newUrl = `http://localhost:3000/rooms/1122334455667788#${key}`;

    const resolvedNew = resolveRoomFromLocation({ href: newUrl });
    expect(resolvedNew).toBeTruthy();
    expect(resolvedNew?.id).toBe('1122334455667788');
    expect(resolvedNew?.name).toBe('Room 11223344');
    expect(resolvedNew?.isAdmin).toBe(false);
    expect(getVisitedRooms().map((r) => r.id)).toContain('1122334455667788');

    // Existing admin room in localStorage should preserve name and isAdmin: true
    const adminRoom: VisitedRoom = {
      id: 'aabbccddeeff0011',
      name: 'GM Room',
      url: `http://localhost:3000/rooms/aabbccddeeff0011#${key}`,
      lastVisited: 1000,
      isAdmin: true,
    };
    saveVisitedRoom(adminRoom);

    const resolvedAdmin = resolveRoomFromLocation({
      href: `http://localhost:3000/rooms/aabbccddeeff0011#${key}`,
    });
    expect(resolvedAdmin?.id).toBe('aabbccddeeff0011');
    expect(resolvedAdmin?.name).toBe('GM Room');
    expect(resolvedAdmin?.isAdmin).toBe(true);
  });

  // spec: alhalte-argenterie/home localStorage保存・取得・管理者除外
  it('saves visited rooms and excludes rooms where user is admin from display', () => {
    const room1: VisitedRoom = {
      id: '1111111111111111',
      name: 'Player Room',
      url: 'https://alhalte.example/rooms/1111111111111111#key1',
      lastVisited: 1000,
      isAdmin: false,
    };
    const room2: VisitedRoom = {
      id: '2222222222222222',
      name: 'Admin Room',
      url: 'https://alhalte.example/rooms/2222222222222222#key2',
      lastVisited: 2000,
      isAdmin: true, // Should be excluded from visited list display
    };
    const room3: VisitedRoom = {
      id: '3333333333333333',
      name: 'Another Player Room',
      url: 'https://alhalte.example/rooms/3333333333333333#key3',
      lastVisited: 3000,
      isAdmin: false,
    };

    saveVisitedRoom(room1);
    saveVisitedRoom(room2);
    saveVisitedRoom(room3);

    const visited = getVisitedRooms();
    expect(visited).toHaveLength(2);
    expect(visited.map((r) => r.id)).toEqual(['3333333333333333', '1111111111111111']);
    expect(visited.some((r) => r.isAdmin)).toBe(false);

    removeVisitedRoom('1111111111111111');
    expect(getVisitedRooms().map((r) => r.id)).toEqual(['3333333333333333']);
  });

  // spec: alhalte-argenterie/account 0030 ホーム画面のテーマ設定削除とアカウントUI配置 (Issue #116)
  it('renders rich M3 Expressive HomeScreen with account controls in place of theme settings, displays visited cards, and opens creation dialog', async () => {
    const testRoom: VisitedRoom = {
      id: 'aabbccddeeff0011',
      name: '冒険者ギルド',
      url: 'https://alhalte.example/rooms/aabbccddeeff0011#abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890',
      lastVisited: Date.now(),
      isAdmin: false,
    };
    saveVisitedRoom(testRoom);

    const target = document.createElement('div');
    document.body.appendChild(target);

    let enteredRoom: VisitedRoom | null = null;
    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        onEnterRoom: (r) => {
          enteredRoom = r;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Check M3 AppBar, Hero Banner, and ExtendedFab are rendered
    expect(target.querySelector('.np-app-bar')).toBeTruthy();
    expect(target.querySelector('[data-testid="home-hero-banner"]')).toBeTruthy();

    // Theme settings were removed from the Home screen (Issue #116).
    expect(target.querySelector('[data-testid="theme-controls"]')).toBeNull();
    expect(target.querySelector('[data-testid="theme-palette-violet"]')).toBeNull();
    expect(target.querySelector('[data-testid="theme-mode-toggle"]')).toBeNull();

    // Signed-out account control is a Filled tonal 「サインイン」 button.
    const accountControls = target.querySelector('[data-testid="home-account-controls"]');
    expect(accountControls).toBeTruthy();
    const signInBtn = accountControls?.querySelector<HTMLButtonElement>(
      '[data-testid="account-signin-button"]',
    );
    expect(signInBtn?.textContent?.trim()).toBe('サインイン');
    expect(signInBtn?.classList.contains('np-button')).toBe(true);
    expect(signInBtn?.classList.contains('tonal')).toBe(true);

    // Check visited room card is rendered
    expect(target.querySelector('[data-testid="room-card-aabbccddeeff0011"]')).toBeTruthy();
    expect(target.textContent).toContain('冒険者ギルド');

    // Click "部屋の新規作成" ExtendedFab button
    const createBtn = target.querySelector<HTMLButtonElement>('[data-testid="create-room-button"]');
    expect(createBtn).toBeTruthy();
    expect(createBtn?.classList.contains('np-extended-fab')).toBe(true);
    createBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Verify overlay appears with required elements
    const overlay = target.querySelector('[data-testid="create-room-overlay"]');
    expect(overlay).toBeTruthy();

    const title = target.querySelector('[data-testid="dialog-title"]');
    expect(title?.textContent?.trim()).toBe('部屋の作成');

    const warning = target.querySelector('[data-testid="dialog-warning"]');
    expect(warning?.textContent?.trim()).toBe(
      'このリンクを紛失した場合、部屋へのアクセスは永久に失われます！',
    );

    const urlInput = target.querySelector<HTMLInputElement>('[data-testid="room-url-input"]');
    expect(urlInput?.readOnly).toBe(true);
    expect(urlInput?.value).toContain('https://alhalte.example/rooms/');
    expect(urlInput?.value).toContain('#');

    // spec: alhalte-argenterie/home 部屋作成時のニックネーム不要化 (#94)
    expect(target.querySelector('[data-testid="user-nickname-input"]')).toBeNull();

    // Enter room name and click "入室する"
    const nameInput = target.querySelector<HTMLInputElement>('[data-testid="room-name-input"]');
    if (nameInput) {
      nameInput.value = '新セッション部屋';
      nameInput.dispatchEvent(new Event('input'));
    }

    const joinBtn = target.querySelector<HTMLButtonElement>('[data-testid="join-created-room-button"]');
    joinBtn?.click();

    expect(enteredRoom).toBeTruthy();
    expect(enteredRoom?.name).toBe('新セッション部屋');
    expect(enteredRoom?.isAdmin).toBe(true);
    expect(window.location.pathname).toContain('/rooms/');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/home 招待リンク直接アクセスで部屋へ直接入室 (#34)
  it('directly enters the room when App is mounted at a room invitation URL', () => {
    const key = 'abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789';
    window.history.replaceState(null, '', `/rooms/1122334455667788#${key}`);

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
    });

    expect(target.querySelector('[data-testid="room-layout"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="home-screen"]')).toBeNull();

    unmount(instance);
    target.remove();
  });
});
