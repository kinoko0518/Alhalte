/**
 * Character ownership (`std::owner_account`) decoding & backfill (Issue #228).
 *
 * Banquette stores the owner as an 8-byte little-endian `u64` with the
 * `alhalte/maitre-d/account-id` label (`primitive/u64` is also accepted). The
 * client keeps the exact decimal string, never re-emits the attribute and
 * backfills a freshly created character from the saved account until the
 * server ack / broadcast carries it.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### キャラクター同期`
 * - GitHub Issue #228
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import { labelHash, LABEL_PRIMITIVE_U64 } from '@alhalte/pantry';
import App from '../src/components/App.svelte';
import {
  characterToTinMap,
  KEY_OWNER_ACCOUNT,
  LABEL_OWNER_ACCOUNT,
  tinMapToCharacter,
} from '../src/utils/roomSocket.js';
import { clearAssets } from '../src/utils/assets.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { resetOutbox } from '../src/utils/outbox.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

const ACCOUNT = '1001';

function tick(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Little-endian 8-byte payload of a `u64`. */
function leBytes(value: bigint): number[] {
  const bytes: number[] = [];
  let rest = value;
  for (let i = 0; i < 8; i++) {
    bytes.push(Number(rest & 0xffn));
    rest >>= 8n;
  }
  return bytes;
}

function ownerTin(value: bigint, label = LABEL_OWNER_ACCOUNT): { label: number; payload: number[] } {
  return { label, payload: leBytes(value) };
}

describe('Character owner account decoding (Issue #228)', () => {
  it('uses the maitre-d account-id label', () => {
    expect(LABEL_OWNER_ACCOUNT).toBe(Number(labelHash('alhalte/maitre-d/account-id')));
    expect(KEY_OWNER_ACCOUNT).toBe('std::owner_account');
  });

  it('decodes the maitre-d label and primitive/u64 as an exact decimal string', () => {
    const fromLabel = tinMapToCharacter('42', {
      'std::name': { label: Number(labelHash('primitive/string')), payload: [0x41] },
      'std::owner_account': ownerTin(123n),
    });
    expect(fromLabel.ownerAccountId).toBe('123');

    const fromPrimitive = tinMapToCharacter('42', {
      'std::owner_account': ownerTin(123n, Number(LABEL_PRIMITIVE_U64)),
    });
    expect(fromPrimitive.ownerAccountId).toBe('123');
  });

  it('keeps IDs above 2^53 exact', () => {
    const id = 9007199254740993n; // 2^53 + 1, unrepresentable as a JS number.
    const character = tinMapToCharacter('42', {
      'std::owner_account': ownerTin(id),
    });
    expect(character.ownerAccountId).toBe('9007199254740993');
  });

  it('rejects a 4-byte payload and an unknown label', () => {
    const fourBytes = tinMapToCharacter('42', {
      'std::owner_account': { label: LABEL_OWNER_ACCOUNT, payload: [1, 0, 0, 0] },
    });
    expect(fourBytes.ownerAccountId).toBeUndefined();

    const unknown = tinMapToCharacter('42', {
      'std::owner_account': { label: 1, payload: leBytes(7n) },
    });
    expect(unknown.ownerAccountId).toBeUndefined();
  });

  it('treats a missing owner as undefined', () => {
    expect(tinMapToCharacter('42', {}).ownerAccountId).toBeUndefined();
    expect(tinMapToCharacter('42', null).ownerAccountId).toBeUndefined();
  });

  it('never re-emits the owner in characterToTinMap', () => {
    const character: Character = {
      id: '42',
      name: '勇者ロラン',
      initiative: 10,
      description: '',
      statuses: [],
      parameters: [],
      chatPalette: '',
      hideStatus: false,
      ownerAccountId: '9007199254740993',
    };
    const map = characterToTinMap(character);
    expect(map[KEY_OWNER_ACCOUNT]).toBeUndefined();
    expect(Object.keys(map)).not.toContain(KEY_OWNER_ACCOUNT);
  });
});

describe('Created character ownership backfill (Issue #228)', () => {
  const ROOM: VisitedRoom = {
    id: 'aabbccddeeff0229',
    name: '所有権テスト部屋',
    url: 'https://alhalte.example/rooms/aabbccddeeff0229#key',
    lastVisited: Date.now(),
  };

  let target: HTMLDivElement;
  let app: unknown = null;

  beforeEach(() => {
    clearAssets();
    resetOutbox();
    document.body.innerHTML = '';
    localStorage.clear();
    localStorage.setItem('alhalte:account-id', ACCOUNT);
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    if (app) {
      unmount(app as never);
      app = null;
    }
    target.remove();
    document.body.innerHTML = '';
    localStorage.clear();
  });

  function mountApp(socket: FakeRoomSocket): void {
    const scene: Scene = {
      id: '/',
      name: 'メインシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
    app = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        initialScenes: [scene],
        initialCharacters: [],
        userRole: 'participant',
        roomSocket: asRoomSocket(socket),
      },
    });
  }

  // spec: Issue #228 作成 ack が owner を持たなくても保存済みアカウントで補完する
  it('backfills ownerAccountId from the saved account so a created character is immediately mine', async () => {
    const socket = new FakeRoomSocket();
    socket.characterListSnapshot = [];
    mountApp(socket);
    await tick(30);

    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-characters"]')?.click();
    await tick(40);

    target.querySelector<HTMLButtonElement>('[data-testid="create-character-btn"]')?.click();
    await tick(30);

    const nameInput = target.querySelector<HTMLInputElement>('[data-testid="edit-name-input"]');
    expect(nameInput).toBeTruthy();
    nameInput!.value = '新しい勇者';
    nameInput!.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    target.querySelector<HTMLButtonElement>('[data-testid="save-character-btn"]')?.click();
    await tick(60);

    expect(socket.requestCreateCharacterCalls).toHaveLength(1);
    // The client never sends the ownership attribute.
    expect(socket.requestCreateCharacterCalls[0].character.ownerAccountId).toBeUndefined();

    const serverId = String(socket.requestCreateCharacterCalls[0].allocatedId);
    const card = target.querySelector(`[data-testid="character-card-${serverId}"]`);
    expect(card).toBeTruthy();
    // Owned: the participant gets the piece toggle and その他.
    expect(target.querySelector(`[data-testid="piece-char-btn-${serverId}"]`)).toBeTruthy();
    expect(target.querySelector(`[data-testid="more-char-btn-${serverId}"]`)).toBeTruthy();
    // And the character appears in the "mine" view.
    target
      .querySelector<HTMLInputElement>('[data-testid="character-view-segmented"] input[value="mine"]')
      ?.click();
    await tick();
    expect(target.querySelector(`[data-testid="character-card-${serverId}"]`)).toBeTruthy();
  });
});
