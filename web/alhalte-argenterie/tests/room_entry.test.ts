/**
 * App-level room entry tests for Issue #156.
 *
 * `GET /api/rooms/{room_id}` is the join preflight: 403 renders the gate screen,
 * 404 a not-found state, and the server role drives the GM-only UI. The room
 * WebSocket must not be opened before the preflight succeeds.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 入室`, `### 門前払い画面`
 * - GitHub Issue #156
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import type { VisitedRoom } from '../src/types/index.js';
import type { RoomRole } from '../src/utils/session.js';

const ROOM_KEY_HEX =
  'abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789';

function jsonResponse(payload: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? 'OK' : 'Error',
    json: async () => payload,
  } as Response;
}

function roomInfo(role: RoomRole, name = 'サーバー部屋') {
  return {
    room_id: '1122334455667788',
    name,
    role,
    done: false,
  };
}

describe('App room entry preflight & roles (Issue #156)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    localStorage.clear();
    document.cookie = 'session_id=; path=/; max-age=0; SameSite=Lax';
    window.history.replaceState(null, '', '/');
    // `App` loads the permission list separately; keep it off the network.
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => jsonResponse([])),
    );
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    unmountAll();
    target.remove();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    window.history.replaceState(null, '', '/');
  });

  const mounted: Array<Record<string, unknown>> = [];

  function mountApp(props: Record<string, unknown>) {
    const instance = mount(App, { target, props });
    mounted.push(instance as unknown as Record<string, unknown>);
    return instance;
  }

  function unmountAll() {
    for (const instance of mounted.splice(0)) {
      unmount(instance as never);
    }
  }

  function gearMenu(): void {
    target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]')?.click();
  }

  const baseRoom: VisitedRoom = {
    id: '1122334455667788',
    name: 'ローカル名',
    url: `https://alhalte.example/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    lastVisited: Date.now(),
  };

  it('derives isAdmin from the server role and shows the GM-only menu section', async () => {
    mountApp({ initialRoom: baseRoom, userRole: 'admin' });
    gearMenu();
    await new Promise((r) => setTimeout(r, 30));

    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="manage-permissions-item"]')).toBeTruthy();
  });

  it('does not treat VisitedRoom.isAdmin or localStorage as authority', async () => {
    localStorage.setItem(
      'alhalte:visited-rooms',
      JSON.stringify([{ ...baseRoom, isAdmin: true }]),
    );

    mountApp({ initialRoom: { ...baseRoom, isAdmin: true }, userRole: 'participant' });
    gearMenu();
    await new Promise((r) => setTimeout(r, 30));

    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeNull();
  });

  it('marks a viewer and disables message sending in the footer', async () => {
    mountApp({ initialRoom: baseRoom, userRole: 'viewer' });
    await new Promise((r) => setTimeout(r, 30));

    const textarea = target.querySelector<HTMLTextAreaElement>(
      '[data-testid="chat-input-field"]',
    );
    expect(textarea?.getAttribute('placeholder')).toContain('閲覧者モード');
    gearMenu();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeNull();
  });

  it('disables scene management and extension administration for viewers', async () => {
    mountApp({ initialRoom: baseRoom, userRole: 'viewer' });
    await new Promise((r) => setTimeout(r, 30));

    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-settings"]')?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')?.disabled,
    ).toBe(true);
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')?.disabled,
    ).toBe(true);

    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-extensions"]')?.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="lock-indicator"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="upload-extension-btn"]')).toBeNull();
  });

  it('honours the explicit isAdmin test override when no role is supplied', async () => {
    mountApp({ initialRoom: baseRoom, isAdmin: true });
    gearMenu();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeTruthy();
  });

  it('renders the gate screen on a 403 and never mounts the room layout', async () => {
    window.history.replaceState(
      null,
      '',
      `/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    );
    const fetchFn = vi.fn(async () => jsonResponse({ error: 'forbidden' }, 403));

    mountApp({ fetchFn });

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="forbidden-room-screen"]')).toBeTruthy(),
    );
    expect(target.querySelector('[data-testid="room-layout"]')).toBeNull();
    expect(target.querySelector('[data-testid="home-screen"]')).toBeNull();
    expect(target.querySelector('[data-testid="forbidden-title"]')?.textContent?.trim()).toBe(
      'アカウントに入室権限がありません',
    );
    // 403 is final: the client must not retry it forever.
    expect(fetchFn).toHaveBeenCalledTimes(1);
  });

  // spec: alhalte-argenterie/入室 4 段階 role の denied も GateScreen (#201)
  it('renders the gate screen when the preflight body reports the denied role', async () => {
    window.history.replaceState(
      null,
      '',
      `/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    );
    const fetchFn = vi.fn(async () => jsonResponse(roomInfo('denied')));

    mountApp({ fetchFn });

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="forbidden-room-screen"]')).toBeTruthy(),
    );
    expect(target.querySelector('[data-testid="room-layout"]')).toBeNull();
    expect(target.querySelector('[data-testid="chat-input-field"]')).toBeNull();
  });

  // spec: alhalte-argenterie/入室 done フラグは必須ではない (#200 / #201)
  it('accepts a preflight response without the legacy done flag', async () => {
    window.history.replaceState(
      null,
      '',
      `/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    );
    const fetchFn = vi.fn(async () =>
      jsonResponse({ room_id: '1122334455667788', name: 'サーバー部屋', role: 'participant' }),
    );

    mountApp({ fetchFn });

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="room-layout"]')).toBeTruthy(),
    );
    expect(target.textContent).toContain('サーバー部屋');
  });

  it('renders a clear not-found state on a 404 and does not enter', async () => {
    window.history.replaceState(
      null,
      '',
      `/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    );
    const fetchFn = vi.fn(async () => jsonResponse({ error: 'not found' }, 404));

    mountApp({ fetchFn });

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="room-not-found"]')).toBeTruthy(),
    );
    expect(target.textContent).toContain('部屋が見つかりません');
    expect(target.querySelector('[data-testid="room-layout"]')).toBeNull();
    expect(target.querySelector('[data-testid="forbidden-room-screen"]')).toBeNull();
    // 404 is final: no infinite reconnect loop.
    expect(fetchFn).toHaveBeenCalledTimes(1);
  });

  // spec: alhalte-argenterie/入室 サーバー不達では部屋を開かない (Issue #151)
  it('does not mount the room or open a WebSocket while the server is unreachable', async () => {
    window.history.replaceState(
      null,
      '',
      `/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    );
    // Server down: the preflight never produces a response.
    const fetchFn = vi.fn(async () => {
      throw new Error('network down');
    });
    const webSocketCtor = vi.fn();
    vi.stubGlobal('WebSocket', webSocketCtor);

    mountApp({ fetchFn });

    await vi.waitFor(() => expect(fetchFn).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 60));

    // The room is only entered after the server confirms it: no layout, no socket.
    expect(target.querySelector('[data-testid="room-layout"]')).toBeNull();
    expect(target.querySelector('[data-testid="room-preflight-loading"]')).toBeTruthy();
    expect(webSocketCtor).not.toHaveBeenCalled();
  });

  it('treats a 401 as sign-in guidance and shows the gate screen', async () => {
    window.history.replaceState(
      null,
      '',
      `/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    );
    const fetchFn = vi.fn(async () => jsonResponse({ error: 'unauthorized' }, 401));

    mountApp({ fetchFn });

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="forbidden-room-screen"]')).toBeTruthy(),
    );
    expect(target.querySelector('[data-testid="room-layout"]')).toBeNull();
  });

  it('sets currentRoom.name and userRole from the preflight response', async () => {
    window.history.replaceState(
      null,
      '',
      `/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    );
    const fetchFn = vi.fn(async () => jsonResponse(roomInfo('admin', 'サーバー側の部屋名')));

    mountApp({ fetchFn });

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="room-layout"]')).toBeTruthy(),
    );
    expect(target.textContent).toContain('サーバー側の部屋名');
    gearMenu();
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeTruthy();
  });
});
