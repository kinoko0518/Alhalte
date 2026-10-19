import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  generateRoomUrl,
  getVisitedRooms,
  saveVisitedRoom,
  removeVisitedRoom,
  parseRoomUrl,
  resolveRoomFromLocation,
} from '../src/utils/home.js';
import { getRecentRooms } from '../src/utils/recentRooms.js';
import HomeScreen from '../src/components/Home/HomeScreen.svelte';
import App from '../src/components/App.svelte';
import type { VisitedRoom } from '../src/types/index.js';

/** Minimal `Response`-shaped object for mocked fetch calls. */
function jsonResponse(payload: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? 'OK' : 'Error',
    json: async () => payload,
  } as Response;
}

/** Signs the test session in through the same storage the app reads. */
function signIn(): void {
  localStorage.setItem('alhalte:account-id', '42');
  localStorage.setItem('alhalte:session-id', '99');
}

describe('Home Screen & Room URL', () => {
  beforeEach(() => {
    localStorage.clear();
    document.cookie = 'session_id=; path=/; max-age=0; SameSite=Lax';
    window.history.replaceState(null, '', '/');
  });

  afterEach(() => {
    window.history.replaceState(null, '', '/');
    vi.restoreAllMocks();
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

    let posted: { name: string; room_id?: string } | null = null;
    const fetchFn = vi.fn(async (url: string | URL | Request, init?: RequestInit) => {
      // The home screen also reads `GET /api/server-info` on mount for the
      // debug-login warning (Issue #150); it is off here.
      if (String(url).endsWith('/api/server-info')) {
        return jsonResponse({ debug_login: false });
      }
      expect(String(url)).toBe('https://alhalte.example/api/rooms');
      expect(init?.method).toBe('POST');
      posted = JSON.parse(String(init?.body)) as { name: string; room_id?: string };
      return jsonResponse({
        room_id: posted.room_id,
        name: posted.name,
        role: 'admin',
        done: false,
      });
    });

    let enteredRoom: VisitedRoom | null = null;
    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        fetchFn: fetchFn as unknown as typeof fetch,
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
    // Creation requires an authenticated session (Issue #156).
    signIn();
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
    expect(joinBtn?.disabled).toBe(false);
    joinBtn?.click();

    await vi.waitFor(() => expect(enteredRoom).toBeTruthy());
    // Only the room creation is a POST; `/api/server-info` is a GET on mount.
    const roomCalls = fetchFn.mock.calls.filter(([url]) =>
      String(url).endsWith('/api/rooms'),
    );
    expect(roomCalls).toHaveLength(1);
    expect(posted?.name).toBe('新セッション部屋');
    expect(posted?.room_id).toMatch(/^[0-9a-f]{16}$/);
    expect(enteredRoom?.name).toBe('新セッション部屋');
    // The creator is `gm` according to the server response.
    expect(enteredRoom?.isAdmin).toBe(true);
    expect(enteredRoom?.url).toMatch(
      new RegExp(`^https://alhalte\\.example/rooms/${posted?.room_id}#[0-9a-f]{64}$`),
    );
    expect(getRecentRooms().some((room) => room.roomId === posted?.room_id)).toBe(true);
    expect(window.location.pathname).toContain(`/rooms/${posted?.room_id}`);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/home 未サインインでは部屋を作成できない (Issue #156)
  it('does not create a room while signed out and shows the sign-in guidance', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const fetchFn = vi.fn();
    let enteredRoom: VisitedRoom | null = null;
    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        fetchFn: fetchFn as unknown as typeof fetch,
        onEnterRoom: (r) => {
          enteredRoom = r;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="create-room-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(target.querySelector('[data-testid="create-room-signin-guidance"]')).toBeTruthy();
    const joinBtn = target.querySelector<HTMLButtonElement>(
      '[data-testid="join-created-room-button"]',
    );
    expect(joinBtn?.disabled).toBe(true);

    joinBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Nothing is registered on the server while signed out. (The mount still
    // reads `GET /api/server-info` for the debug-login warning, Issue #150.)
    const roomCalls = fetchFn.mock.calls.filter(([url]) =>
      String(url).endsWith('/api/rooms'),
    );
    expect(roomCalls).toHaveLength(0);
    expect(enteredRoom).toBeNull();
    expect(getRecentRooms()).toHaveLength(0);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/home 401 はサインイン導線を出す (Issue #156)
  it('shows sign-in guidance on a 401 and never enters or saves a local room', async () => {
    signIn();
    const target = document.createElement('div');
    document.body.appendChild(target);

    const fetchFn = vi.fn(async () => jsonResponse({ error: 'Unauthorized' }, 401));
    let enteredRoom: VisitedRoom | null = null;
    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        fetchFn: fetchFn as unknown as typeof fetch,
        onEnterRoom: (r) => {
          enteredRoom = r;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="create-room-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLButtonElement>('[data-testid="join-created-room-button"]')?.click();

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="create-room-error"]')).toBeTruthy(),
    );
    expect(target.querySelector('[data-testid="create-room-signin-guidance"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="create-room-error"]')?.textContent).toContain(
      'サインインが必要です',
    );
    expect(enteredRoom).toBeNull();
    expect(getRecentRooms()).toHaveLength(0);
    expect(window.location.pathname).toBe('/');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/home 通信/HTTP 失敗では入室しない (Issue #156)
  it('stays on Home with an error on a network failure and does not enter', async () => {
    signIn();
    const target = document.createElement('div');
    document.body.appendChild(target);

    const fetchFn = vi.fn(async () => {
      throw new Error('network down');
    });
    let enteredRoom: VisitedRoom | null = null;
    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        fetchFn: fetchFn as unknown as typeof fetch,
        onEnterRoom: (r) => {
          enteredRoom = r;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="create-room-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLButtonElement>('[data-testid="join-created-room-button"]')?.click();

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="create-room-error"]')?.textContent).toContain(
        '通信できませんでした',
      ),
    );
    expect(enteredRoom).toBeNull();
    expect(getRecentRooms()).toHaveLength(0);

    unmount(instance);
    target.remove();
  });

  it('stays on Home with an error on an HTTP failure and does not enter', async () => {
    signIn();
    const target = document.createElement('div');
    document.body.appendChild(target);

    const fetchFn = vi.fn(async () => jsonResponse({ error: 'boom' }, 500));
    let enteredRoom: VisitedRoom | null = null;
    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        fetchFn: fetchFn as unknown as typeof fetch,
        onEnterRoom: (r) => {
          enteredRoom = r;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="create-room-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLButtonElement>('[data-testid="join-created-room-button"]')?.click();

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="create-room-error"]')?.textContent).toContain(
        'HTTP 500',
      ),
    );
    expect(enteredRoom).toBeNull();
    expect(getRecentRooms()).toHaveLength(0);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/home サーバーが採番した room_id が正 (Issue #151)
  it('uses the server-issued room_id instead of the client-generated preview', async () => {
    signIn();
    const target = document.createElement('div');
    document.body.appendChild(target);

    const SERVER_ROOM_ID = 'ffffffffffffffff';
    const requestedRoomIds: Array<string | undefined> = [];
    const fetchFn = vi.fn(async (_url: string | URL | Request, init?: RequestInit) => {
      requestedRoomIds.push(
        (JSON.parse(String(init?.body)) as { room_id?: string }).room_id,
      );
      // The registry answers with a different canonical ID than the preview.
      return jsonResponse({
        room_id: SERVER_ROOM_ID,
        name: 'サーバー命名の部屋',
        role: 'admin',
        done: false,
      });
    });

    let enteredRoom: VisitedRoom | null = null;
    const instance = mount(HomeScreen, {
      target,
      props: {
        baseUrl: 'https://alhalte.example',
        fetchFn: fetchFn as unknown as typeof fetch,
        onEnterRoom: (r) => {
          enteredRoom = r;
        },
      },
    });

    target.querySelector<HTMLButtonElement>('[data-testid="create-room-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    // The dialog previews a client-generated 16-hex ID before the server answers.
    const previewUrl = target.querySelector<HTMLInputElement>('[data-testid="room-url-input"]')?.value;
    expect(previewUrl).toMatch(/^https:\/\/alhalte\.example\/rooms\/[0-9a-f]{16}#[0-9a-f]{64}$/);
    const previewRoomId = previewUrl?.match(/\/rooms\/([0-9a-f]{16})#/)?.[1];
    expect(previewRoomId).not.toBe(SERVER_ROOM_ID);

    target.querySelector<HTMLButtonElement>('[data-testid="join-created-room-button"]')?.click();

    await vi.waitFor(() => expect(enteredRoom).toBeTruthy());
    expect(requestedRoomIds).toEqual([previewRoomId]);
    expect(enteredRoom?.id).toBe(SERVER_ROOM_ID);
    expect(enteredRoom?.url).toMatch(
      new RegExp(`^https://alhalte\\.example/rooms/${SERVER_ROOM_ID}#[0-9a-f]{64}$`),
    );
    expect(getRecentRooms().some((room) => room.roomId === SERVER_ROOM_ID)).toBe(true);
    expect(getRecentRooms().some((room) => room.roomId === previewRoomId)).toBe(false);
    expect(window.location.pathname).toContain(`/rooms/${SERVER_ROOM_ID}`);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/home 招待リンク直接アクセスで部屋へ直接入室 (#34 / #156 preflight)
  it('directly enters the room after the server preflight when App is mounted at a room invitation URL', async () => {
    const key = 'abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789';
    window.history.replaceState(null, '', `/rooms/1122334455667788#${key}`);

    const target = document.createElement('div');
    document.body.appendChild(target);

    const fetchFn = vi.fn(async (url: string | URL | Request) => {
      const parsed = new URL(String(url), window.location.origin);
      expect(parsed.pathname).toBe('/api/rooms/1122334455667788');
      return jsonResponse({
        room_id: '1122334455667788',
        name: 'サーバー部屋',
        role: 'participant',
        done: false,
      });
    });
    const instance = mount(App, {
      target,
      props: { fetchFn: fetchFn as unknown as typeof fetch },
    });

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="room-layout"]')).toBeTruthy(),
    );
    expect(target.querySelector('[data-testid="home-screen"]')).toBeNull();
    expect(target.textContent).toContain('サーバー部屋');

    unmount(instance);
    target.remove();
  });
});
