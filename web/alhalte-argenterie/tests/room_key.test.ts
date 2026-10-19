/**
 * App-level room key resolution tests for Issue #156.
 *
 * The room key must come from `currentRoom.url` / an explicit prop, not only
 * from `window.location.hash`. Extension uploads with no key report an error
 * instead of attempting a doomed cellar transfer.
 *
 * `cellarTransfer` is mocked so the suite never touches the network.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 拡張機能`
 * - GitHub Issue #156
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import { bytesToHex } from '@alhalte/pantry';
import App from '../src/components/App.svelte';
import { resolveRoomKey, getRoomKeyFromUrl } from '../src/utils/cellarTransfer.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { ExtensionItem, Scene, VisitedRoom } from '../src/types/index.js';

const ROOM_KEY_HEX =
  'abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789';

const TIN_ID = 'ab'.repeat(32);

const cellar = vi.hoisted(() => ({
  encodeRoomTin: vi.fn(),
  uploadTin: vi.fn(),
}));

vi.mock('../src/utils/cellarTransfer.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../src/utils/cellarTransfer.js')>();
  return {
    ...actual,
    encodeRoomTin: cellar.encodeRoomTin,
    uploadTin: cellar.uploadTin,
  };
});

// Valid Alhalte ABI extension wasm (same fixture as extensions-abi.test.ts).
const VALID_WASM_HEX =
  '0061736d010000000115046000017f60017f017f60027f7f0060027f7f017e030504000102030503010001074505066d656d6f7279020013616c68616c74655f6162695f76657273696f6e000005616c6c6f630001076465616c6c6f6300021070726f636573735f67656e657261737400030a3404040041010b040041000b02000b250041800820002001fc0a000041800820016a41213a0000428008422086200141016aad840b';

const MAIN_SCENE: Scene = {
  id: '/',
  name: 'メインシーン',
  juices: createDefaultJuices(),
  viewTransform: { x: 0, y: 0, scale: 1 },
  members: [],
};

function wasmFile(): File {
  return new File([new Uint8Array(VALID_WASM_HEX.match(/../g)!.map((b) => parseInt(b, 16)))], 'ext.wasm', {
    type: 'application/wasm',
  });
}

function roomWithKey(): VisitedRoom {
  return {
    id: '1122334455667788',
    name: 'キー解決テスト部屋',
    url: `https://alhalte.example/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    lastVisited: Date.now(),
  };
}

function roomWithoutKey(): VisitedRoom {
  return {
    id: '1122334455667788',
    name: 'キーなし部屋',
    url: 'https://alhalte.example/rooms/1122334455667788',
    lastVisited: Date.now(),
  };
}

describe('Room key resolution (Issue #156)', () => {
  beforeEach(() => {
    localStorage.clear();
    window.history.replaceState(null, '', '/');
  });

  it('reads a 64-hex key from a room URL fragment and rejects malformed ones', () => {
    const fromUrl = getRoomKeyFromUrl(
      `https://alhalte.example/rooms/1122334455667788#${ROOM_KEY_HEX}`,
    );
    expect(fromUrl).toBeInstanceOf(Uint8Array);
    expect(fromUrl && bytesToHex(fromUrl)).toBe(ROOM_KEY_HEX);

    expect(getRoomKeyFromUrl('/rooms/1122334455667788#not-a-key')).toBeNull();
    expect(getRoomKeyFromUrl('/rooms/1122334455667788')).toBeNull();
    expect(getRoomKeyFromUrl(undefined)).toBeNull();
  });

  it('prefers an explicit prop, then the room URL, then the location fragment', () => {
    const explicit = new Uint8Array(32).fill(9);
    expect(
      bytesToHex(
        resolveRoomKey({
          roomKey: explicit,
          roomUrl: `https://alhalte.example/rooms/1122334455667788#${ROOM_KEY_HEX}`,
        })!,
      ),
    ).toBe(bytesToHex(explicit));

    expect(
      bytesToHex(
        resolveRoomKey({
          roomUrl: `https://alhalte.example/rooms/1122334455667788#${ROOM_KEY_HEX}`,
        })!,
      ),
    ).toBe(ROOM_KEY_HEX);

    expect(resolveRoomKey({ roomKey: null, roomUrl: undefined })).toBeNull();
  });
});

describe('App room key wiring (Issue #156)', () => {
  let target: HTMLDivElement;
  let instance: Record<string, unknown> | null = null;

  beforeEach(() => {
    localStorage.clear();
    window.history.replaceState(null, '', '/');
    cellar.encodeRoomTin.mockReset();
    cellar.uploadTin.mockReset();
    cellar.encodeRoomTin.mockResolvedValue({
      wireBytes: new Uint8Array([1, 2, 3]),
      tinIdHex: TIN_ID,
    });
    cellar.uploadTin.mockResolvedValue({ tinId: TIN_ID });
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    if (instance) {
      unmount(instance as never);
      instance = null;
    }
    target.remove();
    vi.restoreAllMocks();
  });

  function mountApp(room: VisitedRoom, extensions: ExtensionItem[] = []): FakeRoomSocket {
    const socket = new FakeRoomSocket();
    instance = mount(App, {
      target,
      props: {
        initialRoom: room,
        initialScenes: [MAIN_SCENE],
        initialExtensions: extensions,
        userRole: 'gm',
        roomSocket: asRoomSocket(socket),
      },
    }) as unknown as Record<string, unknown>;
    return socket;
  }

  async function uploadExtension(): Promise<void> {
    target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-extensions"]')?.click();
    await new Promise((r) => setTimeout(r, 30));
    const input = target.querySelector<HTMLInputElement>('[data-testid="extension-file-input"]');
    expect(input).toBeTruthy();
    Object.defineProperty(input!, 'files', { value: [wasmFile()], configurable: true });
    input!.dispatchEvent(new Event('change', { bubbles: true }));
  }

  it('uses the currentRoom.url fragment even when the location has no hash', async () => {
    mountApp(roomWithKey());
    await uploadExtension();

    await vi.waitFor(() => expect(cellar.encodeRoomTin).toHaveBeenCalled());
    const roomKey = cellar.encodeRoomTin.mock.calls[0]![2] as Uint8Array;
    expect(bytesToHex(roomKey)).toBe(ROOM_KEY_HEX);
  });

  it('reports an error and never uploads when the room key is missing', async () => {
    mountApp(roomWithoutKey());
    await uploadExtension();

    await vi.waitFor(() =>
      expect(target.querySelector('[data-testid="extension-upload-error"]')).toBeTruthy(),
    );
    expect(target.querySelector('[data-testid="extension-upload-error"]')?.textContent).toContain(
      '部屋鍵がない',
    );
    expect(cellar.encodeRoomTin).not.toHaveBeenCalled();
    expect(cellar.uploadTin).not.toHaveBeenCalled();
  });
});
