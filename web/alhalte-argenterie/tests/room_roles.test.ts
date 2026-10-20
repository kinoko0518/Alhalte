/**
 * Role-based UI control tests for the four-level room permission model
 * (Issue #201).
 *
 * Admin: everything. Participant: log / state writes to accessible scenes and
 * character CRUD, but no scene management, extension management or room
 * settings. Viewer: read-only. Denied: `GateScreen`.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 門前払い画面`
 * - GitHub Issue #201
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import App from '../src/components/App.svelte';
import type { RoomRole } from '../src/utils/session.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';

async function tick(ms = 20): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

const ROOM: VisitedRoom = {
  id: 'aabbccddeeff0011',
  name: '権限テスト部屋',
  url: 'https://alhalte.example/rooms/aabbccddeeff0011#secret',
  lastVisited: Date.now(),
};

function makeScene(overrides: Partial<Scene> = {}): Scene {
  return {
    id: '/',
    name: 'メインシーン',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    // The character tab's default view lists the current scene's members.
    members: ['1'],
    ...overrides,
  };
}

function makeCharacter(): Character {
  return {
    id: '1',
    name: 'アリス',
    initiative: 10,
    description: '',
    statuses: [],
    parameters: [],
    chatPalette: '',
    hideStatus: false,
    ownerAccountId: '1001',
  };
}

describe('Four-level room role UI (Issue #201)', () => {
  let target: HTMLDivElement;
  let socket: FakeRoomSocket;

  beforeEach(() => {
    document.body.innerHTML = '';
    localStorage.clear();
    localStorage.setItem('alhalte:account-id', '1001');
    socket = new FakeRoomSocket();
    // The character tab re-queries the list on mount; keep the fixture row.
    socket.characterListSnapshot = [makeCharacter()];
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    unmountAll();
    target.remove();
    vi.restoreAllMocks();
  });

  const mounted: unknown[] = [];

  function mountApp(role: RoomRole) {
    const instance = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [makeScene()],
        initialCharacters: [makeCharacter()],
        userRole: role,
        roomSocket: asRoomSocket(socket),
      },
    });
    mounted.push(instance);
    return instance;
  }

  function unmountAll() {
    while (mounted.length > 0) {
      unmount(mounted.pop() as never);
    }
  }

  function gearMenu(): void {
    target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]')?.click();
  }

  async function openTab(tab: 'log' | 'graph' | 'settings' | 'characters' | 'extensions') {
    target.querySelector<HTMLButtonElement>(`[data-testid="tab-btn-${tab}"]`)?.click();
    await tick(30);
  }

  function sendButton(): HTMLButtonElement | null {
    return target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
  }

  async function typeAndSend(text: string): Promise<void> {
    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea).toBeTruthy();
    textarea!.value = text;
    textarea!.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    sendButton()?.click();
    await tick();
  }

  // spec: alhalte-argenterie/permissions 0100 admin は全操作できる
  it('gives an admin every control: room settings, scene management, characters and speech', async () => {
    mountApp('admin');
    await tick(40);

    gearMenu();
    await tick();
    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="manage-permissions-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="default-permission-selector"]')).toBeTruthy();
    target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]')?.click();
    await tick();

    await openTab('settings');
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')?.disabled,
    ).toBe(false);
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')?.disabled,
    ).toBe(false);

    await openTab('characters');
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="create-character-btn"]')?.disabled,
    ).toBe(false);
    // Issue #228: an admin gets every view and every scene-view row action.
    expect(target.querySelector('[data-testid="character-view-tabs"]')).toBeTruthy();
    expect(
      target.querySelector('[data-testid="character-view-segmented"] input[value="all"]'),
    ).toBeTruthy();
    expect(target.querySelector('[data-testid="move-char-btn-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="piece-char-btn-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="more-char-btn-1"]')).toBeTruthy();

    await openTab('extensions');
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="upload-extension-btn"]'),
    ).toBeTruthy();

    expect(sendButton()?.disabled).toBe(false);

    // The admin can speak in the current scene.
    await typeAndSend('管理者の発言');
    expect(socket.requestAppendLogCalls).toHaveLength(1);
    expect(socket.requestAppendLogCalls[0].sceneId).toBe('/');
  });

  // spec: alhalte-argenterie/permissions 0110 participant は scene 管理以外を操作できる
  it('lets a participant speak and manage characters but not scenes, extensions or room settings', async () => {
    mountApp('participant');
    await tick(40);

    // No admin section, no permission management, no default permission selector.
    gearMenu();
    await tick();
    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeNull();
    expect(target.querySelector('[data-testid="manage-permissions-item"]')).toBeNull();
    expect(target.querySelector('[data-testid="default-permission-selector"]')).toBeNull();
    target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]')?.click();
    await tick();

    // Scene management is admin-only.
    await openTab('settings');
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')?.disabled,
    ).toBe(true);
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')?.disabled,
    ).toBe(true);
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-logs-btn"]')?.disabled,
    ).toBe(true);

    // Character CRUD is allowed.
    await openTab('characters');
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="create-character-btn"]')?.disabled,
    ).toBe(false);
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="edit-char-btn-1"]')?.disabled,
    ).toBe(false);

    // Issue #228: a participant gets the scene / mine segmented views and the
    // own character's piece toggle + その他 (but neither 別シーン nor 視点).
    expect(target.querySelector('[data-testid="character-view-tabs"]')).toBeTruthy();
    const roleSegmented = target.querySelector('[data-testid="character-view-segmented"]');
    expect(roleSegmented).toBeTruthy();
    expect(roleSegmented?.querySelector('input[value="scene"]')).toBeTruthy();
    expect(roleSegmented?.querySelector('input[value="mine"]')).toBeTruthy();
    expect(roleSegmented?.querySelector('input[value="all"]')).toBeNull();
    expect(target.querySelector('[data-testid="move-char-btn-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="focus-char-btn-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="piece-char-btn-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="more-char-btn-1"]')).toBeTruthy();

    // Extension management is admin-only.
    await openTab('extensions');
    expect(target.querySelector('[data-testid="lock-indicator"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="upload-extension-btn"]')).toBeNull();

    // Speech is allowed, and the participant is not treated as a viewer.
    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea?.disabled).toBe(false);
    expect(textarea?.placeholder).not.toContain('閲覧者モード');
    await typeAndSend('参加者の発言');
    expect(socket.requestAppendLogCalls).toHaveLength(1);
    expect(socket.requestAppendLogCalls[0].sceneId).toBe('/');
  });

  // spec: alhalte-argenterie/permissions 0120 viewer は読み取り専用
  it('makes a viewer read-only: no speech, no scene management, no character CRUD, no extensions', async () => {
    mountApp('viewer');
    await tick(40);

    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea?.disabled).toBe(true);
    expect(textarea?.placeholder).toContain('閲覧者モードのためメッセージは送信できません');
    expect(sendButton()?.disabled).toBe(true);
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="dice-btn-1d100"]')?.disabled,
    ).toBe(true);
    await typeAndSend('送れない発言');
    expect(socket.requestAppendLogCalls).toHaveLength(0);

    gearMenu();
    await tick();
    expect(target.querySelector('[data-testid="admin-section-label"]')).toBeNull();
    expect(target.querySelector('[data-testid="default-permission-selector"]')).toBeNull();
    target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]')?.click();
    await tick();

    await openTab('settings');
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="open-create-scene-btn"]')?.disabled,
    ).toBe(true);
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="delete-scene-btn"]')?.disabled,
    ).toBe(true);

    await openTab('characters');
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="create-character-btn"]')?.disabled,
    ).toBe(true);
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="edit-char-btn-1"]')?.disabled,
    ).toBe(true);
    // Issue #228: a viewer only has the scene view (no segmented button) and no
    // row actions at all, but the rows themselves are still rendered.
    expect(target.querySelector('[data-testid="character-view-tabs"]')).toBeNull();
    expect(target.querySelector('[data-testid="character-card-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="piece-char-btn-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="more-char-btn-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="move-char-btn-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="focus-char-btn-1"]')).toBeNull();
    expect(target.querySelector('[data-testid="delete-char-btn-1"]')).toBeNull();

    await openTab('extensions');
    expect(target.querySelector('[data-testid="lock-indicator"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="upload-extension-btn"]')).toBeNull();
  });

  // spec: alhalte-argenterie/permissions 0130 denied は GateScreen
  it('replaces the whole room with the gate screen for a denied role', async () => {
    mountApp('denied');
    await tick(40);

    expect(target.querySelector('[data-testid="forbidden-room-screen"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="forbidden-title"]')?.textContent?.trim()).toBe(
      'アカウントに入室権限がありません',
    );
    expect(target.querySelector('[data-testid="room-layout"]')).toBeNull();
    expect(target.querySelector('[data-testid="chat-input-field"]')).toBeNull();
  });

  // spec: alhalte-argenterie/permissions 0140 旧 role / old frame は残っていない
  it('has no legacy gm / pl role or SetDone / DoneUpdated frame left in the source', () => {
    const root = join(__dirname, '..', 'src');
    const files: string[] = [];
    const walk = (dir: string): void => {
      for (const entry of readdirSync(dir)) {
        const path = join(dir, entry);
        if (statSync(path).isDirectory()) walk(path);
        else if (/\.(ts|svelte)$/.test(entry)) files.push(path);
      }
    };
    walk(root);
    expect(files.length).toBeGreaterThan(10);

    const legacy = /SetDone|DoneUpdated|SetDoneFailed|DoneUpdatedAck|'gm'|"gm"|'pl'|"pl"/;
    const offenders = files.filter((file) => legacy.test(readFileSync(file, 'utf8')));
    expect(offenders).toEqual([]);
  });
});
