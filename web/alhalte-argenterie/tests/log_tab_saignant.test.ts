import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import { labelHash } from '@alhalte/pantry';
import {
  CURRENT_ALHALTE_ABI_VERSION,
  buildCharacterNameMap,
  characterNamesSignature,
  initSaignant,
  loadSaignantWasm,
  getSaignantCss,
  injectSaignantCss,
  renderGenerast,
  renderGenerastSync,
  encodeStringAsGenerastTin,
  encodeGenerastAstToTin,
  clearSaignantCache,
  setSaignantInstance,
} from '../src/utils/saignant.js';
import LogTab from '../src/components/Control/tabs/LogTab.svelte';
import App from '../src/components/App.svelte';
import { characterToTinMap } from '../src/utils/roomSocket.js';
import { ChunkLogManager } from '../src/utils/log.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { getThemeSettings, setThemePalette } from '../src/utils/theme.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Character, Scene, VisitedRoom } from '../src/types/index.js';

// ---------------------------------------------------------------------------
// Raw Generast postcard encoders for the real-wasm ABI tests (Issue #163).
//
// `Generast::Element` is postcard variant 0 and wraps a `Tin` as
// `label varint || payload length varint || payload`; the outer `Tin` uses the
// `alhalte/generast` label and the same payload framing.
// ---------------------------------------------------------------------------

const LABEL_ALHALTE_GENERAST = 0xd38f9ce233c2315an;
const LABEL_ALHALTE_GENERAST_CHARACTER_ID = labelHash('alhalte/generast/character-id');
const LABEL_ALHALTE_GENERAST_FLUCT = labelHash('alhalte/generast/fluct');
const LABEL_ALHALTE_GENERAST_ASSIGN = labelHash('alhalte/generast/assign');

function encodeVarint(value: bigint | number): number[] {
  let v = BigInt(value);
  const out: number[] = [];
  while (v >= 0x80n) {
    out.push(Number((v & 0x7fn) | 0x80n));
    v >>= 7n;
  }
  out.push(Number(v));
  return out;
}

function encodePostcardString(value: string): number[] {
  const bytes = new TextEncoder().encode(value);
  return [...encodeVarint(bytes.length), ...bytes];
}

/** Postcard zigzag encoding for a signed 32-bit integer. */
function encodeZigZag32(value: number): number[] {
  return encodeVarint((value << 1) ^ (value >> 31));
}

function u64LeBytes(value: bigint): number[] {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, value, true);
  return [...bytes];
}

function encodeElementTin(label: bigint, payload: number[]): Uint8Array {
  const body = [0, ...encodeVarint(label), ...encodeVarint(payload.length), ...payload];
  const tin = new Uint8Array(8 + body.length);
  new DataView(tin.buffer).setBigUint64(0, LABEL_ALHALTE_GENERAST, true);
  tin.set(body, 8);
  return tin;
}

function encodeMention(id: bigint): Uint8Array {
  return encodeElementTin(LABEL_ALHALTE_GENERAST_CHARACTER_ID, u64LeBytes(id));
}

const ALICE: Character = {
  id: '12',
  name: 'Alice',
  initiative: 0,
  description: '',
  statuses: [],
  parameters: [],
  chatPalette: '',
  hideStatus: false,
};

function settle(ms = 20): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await settle(10);
  }
}

describe('Saignant WASM & LogTab M3 Expressive Integration (Issue #55)', () => {
  beforeEach(() => {
    localStorage.clear();
    clearSaignantCache();
    document.head.querySelectorAll('#alhalte-saignant-stylesheet').forEach((el) => el.remove());
  });

  // spec: alhalte-saignant/abi 実 WASM による HTML レンダリングと CSS 取得
  it('executes real alhalte_saignant.wasm ABI to render Generast HTML and CSS without TS reimplementation', async () => {
    const wasmInstance = await loadSaignantWasm();
    expect(wasmInstance).toBeTruthy();
    expect(wasmInstance.exports.alhalte_abi_version()).toBe(CURRENT_ALHALTE_ABI_VERSION);
    expect(CURRENT_ALHALTE_ABI_VERSION).toBe(2);
    expect(typeof wasmInstance.exports.render_generast_abi_v2).toBe('function');

    // CSS extraction & injection
    const css = getSaignantCss();
    expect(css.length).toBeGreaterThan(1000);
    expect(css).toContain('.saignant-bold');
    expect(css).toContain('.saignant-italic');
    expect(css).toContain('.saignant-log-entry');

    injectSaignantCss();
    const styleEl = document.getElementById('alhalte-saignant-stylesheet');
    expect(styleEl).toBeTruthy();
    expect(styleEl?.textContent).toContain('.saignant-bold');

    // Plain text rendering via Saignant WASM
    const plainHtml = await renderGenerast('こんにちは世界');
    expect(plainHtml).toBe('こんにちは世界');

    // Rich AST bold rendering via Saignant WASM
    const boldAstBytes = encodeGenerastAstToTin({
      type: 'nested',
      meta: 'bold',
      contents: [{ type: 'string', value: 'クリティカル！' }],
    });
    const boldHtml = await renderGenerast(boldAstBytes);
    expect(boldHtml).toContain('<strong class="saignant-bold">クリティカル！</strong>');

    // Rich AST italic rendering via Saignant WASM
    const italicAstBytes = encodeGenerastAstToTin({
      type: 'nested',
      meta: 'italic',
      contents: [{ type: 'string', value: 'ファンブル' }],
    });
    const italicHtml = await renderGenerast(italicAstBytes);
    expect(italicHtml).toContain('<em class="saignant-italic">ファンブル</em>');
  });

  // spec: alhalte-argenterie/log-tab ログタブ共通ヘッダー（閉じる、Search シーン切り替え、歯車 Menu）
  it('renders LogTab header with close button, search scene selector, and gear settings menu', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const sceneMain: Scene = {
      id: 'scene-main',
      name: '本堂',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
    const sceneSub: Scene = {
      id: 'scene-sub',
      name: '奥の院',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let collapsed = false;
    let selectedSceneId: string | null = null;

    const instance = mount(LogTab, {
      target,
      props: {
        scene: sceneMain,
        allScenes: [sceneMain, sceneSub],
        role: 'admin',
        onCollapse: () => {
          collapsed = true;
        },
        onSelectScene: (id) => {
          selectedSceneId = id;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // 1. Collapse button
    const collapseBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(collapseBtn).toBeTruthy();
    expect(collapseBtn?.getAttribute('aria-label')).toBe('パネルを閉じる');
    collapseBtn?.click();
    expect(collapsed).toBe(true);

    // 2. Search scene selector
    const searchAnchor = target.querySelector('[data-testid="scene-search-anchor"]');
    const searchInput = target.querySelector<HTMLInputElement>('[data-testid="scene-search-input"]');
    expect(searchAnchor).toBeTruthy();
    expect(searchInput).toBeTruthy();
    expect(searchInput?.placeholder).toBe('本堂');

    // Focus input to open scene menu
    searchInput?.focus();
    await new Promise((r) => setTimeout(r, 30));

    const sceneMenu = target.querySelector('[data-testid="scene-search-menu"]');
    expect(sceneMenu).toBeTruthy();

    const subSceneItem = target.querySelector<HTMLElement>('[data-testid="scene-search-item-scene-sub"]');
    expect(subSceneItem).toBeTruthy();
    expect(subSceneItem?.textContent).toContain('奥の院');

    subSceneItem?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(selectedSceneId).toBe('scene-sub');

    // 3. Gear menu
    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    expect(gearBtn).toBeTruthy();
    expect(gearBtn?.getAttribute('aria-label')).toBe('ログ・部屋設定');

    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    const gearMenu = target.querySelector('[data-testid="gear-dropdown-menu"]');
    expect(gearMenu).toBeTruthy();
    expect(target.querySelector('[data-testid="zip-export-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="export-zip-logs-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="manage-permissions-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="default-permission-selector"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="theme-section-header"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab 歯車メニューの ZIP 出力、PL 公開警告ダイアログ、テーマ切り替え
  it('triggers ZIP exports, setDone warning overlay, and theme palette switching from gear menu', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-dungeon',
      name: '地下牢',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let exportedType: string | null = null;
    let doneDefaultPermission: string | null = null;

    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        roomId: 'room-xyz-123',
        role: 'admin',
        onExportZip: (type) => {
          exportedType = type;
        },
        onSetDefaultPermission: async (permission) => {
          doneDefaultPermission = permission;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Open gear menu
    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Test 1: Full ZIP export
    const fullZipItem = target.querySelector<HTMLElement>('[data-testid="zip-export-item"]');
    expect(fullZipItem).toBeTruthy();
    fullZipItem?.click();
    expect(exportedType).toBe('full');

    // Reopen menu for logs export
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Test 2: Logs ZIP export
    const logsZipItem = target.querySelector<HTMLElement>('[data-testid="export-zip-logs-item"]');
    expect(logsZipItem).toBeTruthy();
    logsZipItem?.click();
    expect(exportedType).toBe('logs');

    // Test 3: default permission selector (Issue #201, replaces the old
    // 「部屋の全てをPLに公開」 toggle)
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const selector = target.querySelector('[data-testid="default-permission-selector"]');
    expect(selector).toBeTruthy();
    expect(target.querySelector('[data-testid="set-done-item"]')).toBeNull();

    const adminBtn = target.querySelector<HTMLButtonElement>(
      '[data-testid="default-permission-btn-admin"]',
    );
    expect(adminBtn).toBeTruthy();
    adminBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(doneDefaultPermission).toBe('admin');

    // The component never commits on its own: the App applies the ack.
    expect(
      target.querySelector('[data-testid="default-permission-btn-admin"]')?.className,
    ).not.toContain('filled');

    // Test 4: Theme Palette selection (Azure, Violet, Emerald, Rose, Amber).
    // The default permission selector keeps the gear menu open.
    if (!target.querySelector('[data-testid="gear-dropdown-menu"]')) {
      gearBtn?.click();
      await new Promise((r) => setTimeout(r, 20));
    }

    const emeraldItem = target.querySelector<HTMLElement>('[data-testid="theme-item-emerald"]');
    expect(emeraldItem).toBeTruthy();
    emeraldItem?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(getThemeSettings().palette).toBe('emerald');
    expect(document.documentElement.dataset.themePalette).toBe('emerald');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab メッセージボックス（キャラアイコン、キャラカラー Pill、タイムスタンプ、Saignant WASM 描画）
  it('renders message box with character avatar icon, character color Pill badge, timestamp, and Saignant HTML content', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-hall',
      name: '大広間',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: ['char-1', 'char-2'],
    };

    const testCharacters: Character[] = [
      {
        id: 'char-1',
        name: 'ルシア',
        initiative: 15,
        iconUrl: 'https://alhalte.example/avatars/lucia.png',
        color: '#e91e63',
        description: '魔術師',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
      {
        id: 'char-2',
        name: 'ベルナルド',
        initiative: 10,
        color: '#2e7d32',
        description: '騎士',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];

    const chunkManager = new ChunkLogManager();
    chunkManager.appendRecord({
      id: 'msg-lucia-1',
      sceneId: 'scene-hall',
      content: '私の番ね。炎の矢を放ちます！',
      speaker: 'ルシア',
      speakerId: 'char-1',
      timestamp: 1700000000,
    });
    chunkManager.appendRecord({
      id: 'msg-bernardo-1',
      sceneId: 'scene-hall',
      content: '援護する、任せろ！',
      speaker: 'ベルナルド',
      speakerId: 'char-2',
      timestamp: 1700000060,
    });

    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        characters: testCharacters,
        chunkManager,
      },
    });

    await new Promise((r) => setTimeout(r, 50));

    // 1. First record: Lucia (with avatar image & pink color)
    const recordLucia = target.querySelector('[data-testid="log-record-msg-lucia-1"]');
    expect(recordLucia).toBeTruthy();

    const avatarLucia = target.querySelector('[data-testid="msg-avatar-msg-lucia-1"]');
    expect(avatarLucia).toBeTruthy();

    const speakerPillLucia = target.querySelector<HTMLElement>(
      '[data-testid="msg-speaker-msg-lucia-1"]',
    );
    expect(speakerPillLucia).toBeTruthy();
    expect(speakerPillLucia?.textContent?.trim()).toBe('ルシア');
    expect(speakerPillLucia?.classList.contains('msg-speaker-pill')).toBe(true);
    expect(speakerPillLucia?.getAttribute('style')).toContain('--char-color: #e91e63');

    const timestampLucia = target.querySelector('[data-testid="msg-timestamp-msg-lucia-1"]');
    expect(timestampLucia).toBeTruthy();
    expect(timestampLucia?.textContent?.trim()).not.toBe('');

    const contentLucia = target.querySelector('[data-testid="msg-content-msg-lucia-1"]');
    expect(contentLucia).toBeTruthy();
    expect(contentLucia?.textContent).toContain('私の番ね。炎の矢を放ちます！');

    // 2. Second record: Bernardo (no avatar image, initials with green color)
    const recordBernardo = target.querySelector('[data-testid="log-record-msg-bernardo-1"]');
    expect(recordBernardo).toBeTruthy();

    const avatarBernardo = target.querySelector('[data-testid="msg-avatar-msg-bernardo-1"]');
    expect(avatarBernardo).toBeTruthy();
    expect(avatarBernardo?.textContent?.trim()).toBe('ベル');

    const speakerPillBernardo = target.querySelector<HTMLElement>(
      '[data-testid="msg-speaker-msg-bernardo-1"]',
    );
    expect(speakerPillBernardo).toBeTruthy();
    expect(speakerPillBernardo?.textContent?.trim()).toBe('ベルナルド');
    expect(speakerPillBernardo?.getAttribute('style')).toContain('--char-color: #2e7d32');

    const contentBernardo = target.querySelector('[data-testid="msg-content-msg-bernardo-1"]');
    expect(contentBernardo?.textContent).toContain('援護する、任せろ！');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-saignant/abi v2 実 wasm で @12 の mention と fluct / assign が
  // キャラクター名で描画される (Issue #163)
  it('resolves @12 mention and fluct / assign names through the real wasm ABI v2', async () => {
    await loadSaignantWasm();
    const characters = [{ id: '12', name: 'Alice' }];

    const mentionHtml = await renderGenerast(encodeMention(12n), characters);
    expect(mentionHtml).toContain('@Alice');
    expect(mentionHtml).not.toContain('Anonymous');

    const fluctHtml = await renderGenerast(
      encodeElementTin(LABEL_ALHALTE_GENERAST_FLUCT, [
        ...encodeVarint(12n),
        ...encodePostcardString('HP'),
        ...encodeZigZag32(-5),
      ]),
      characters,
    );
    expect(fluctHtml).toContain('Alice.HP-5');
    expect(fluctHtml).not.toContain('Anonymous');

    const assignHtml = await renderGenerast(
      encodeElementTin(LABEL_ALHALTE_GENERAST_ASSIGN, [
        ...encodeVarint(12n),
        ...encodePostcardString('SAN'),
        ...encodeZigZag32(42),
      ]),
      characters,
    );
    expect(assignHtml).toContain('Alice.SAN=42');
    expect(assignHtml).not.toContain('Anonymous');

    // Without the names map the same v2 artifact keeps the v1 fallback.
    const anonymous = await renderGenerast(encodeMention(12n));
    expect(anonymous).toContain('@Anonymous');
  });

  // spec: alhalte-argenterie/wasm v1 のみの wasm でもクラッシュしない (Issue #163)
  it('falls back to render_generast_abi when the wasm does not expose ABI v2', () => {
    clearSaignantCache();
    const memory = new WebAssembly.Memory({ initial: 1 });
    let next = 1024;
    const fakeExports = {
      memory,
      alhalte_abi_version: () => 1,
      alloc: (len: number) => {
        const ptr = next;
        next += len + 8;
        return ptr;
      },
      dealloc: () => {},
      render_generast_abi: () => {
        const bytes = new TextEncoder().encode(
          '<span class="saignant-v1-fallback">v1</span>',
        );
        new Uint8Array(memory.buffer).set(bytes, 4096);
        return (BigInt(4096) << 32n) | BigInt(bytes.length);
      },
      get_full_css_abi: () => 0n,
    };
    setSaignantInstance({ exports: fakeExports } as unknown as WebAssembly.Instance);

    try {
      const html = renderGenerastSync('hello', [{ id: '12', name: 'Alice' }]);
      expect(html).toContain('saignant-v1-fallback');
      // The v1 path has no names concept; it must not throw or emit the map.
      expect(html).not.toContain('Alice');
    } finally {
      setSaignantInstance(null);
      clearSaignantCache();
    }
  });

  // spec: alhalte-argenterie/log-tab render cache key は characters signature を含む (Issue #163)
  it('includes the character signature in the render cache key and re-renders on a name update', () => {
    clearSaignantCache();
    const memory = new WebAssembly.Memory({ initial: 1 });
    let next = 1024;
    let renderCalls = 0;
    const fakeExports = {
      memory,
      alhalte_abi_version: () => 2,
      alloc: (len: number) => {
        const ptr = next;
        next += len + 8;
        return ptr;
      },
      dealloc: () => {},
      render_generast_abi: () => 0n,
      render_generast_abi_v2: (
        _ptr: number,
        _len: number,
        namesPtr: number,
        namesLen: number,
      ) => {
        renderCalls += 1;
        const names = new TextDecoder().decode(
          new Uint8Array(memory.buffer, namesPtr, namesLen),
        );
        const bytes = new TextEncoder().encode(`<span>${names}</span>`);
        new Uint8Array(memory.buffer).set(bytes, 8192);
        return (BigInt(8192) << 32n) | BigInt(bytes.length);
      },
      get_full_css_abi: () => 0n,
    };
    setSaignantInstance({ exports: fakeExports } as unknown as WebAssembly.Instance);

    try {
      const alice = renderGenerastSync('same text', [{ id: '12', name: 'Alice' }]);
      expect(alice).toContain('"12":"Alice"');
      expect(renderCalls).toBe(1);

      // Same input, same names: served from the render cache.
      expect(renderGenerastSync('same text', [{ id: '12', name: 'Alice' }])).toBe(alice);
      expect(renderCalls).toBe(1);

      // Same input, updated name: the signature changes, so the cache misses.
      const bob = renderGenerastSync('same text', [{ id: '12', name: 'Bob' }]);
      expect(bob).toContain('"12":"Bob"');
      expect(bob).not.toContain('Alice');
      expect(renderCalls).toBe(2);

      expect(characterNamesSignature([{ id: '12', name: 'Bob' }])).not.toBe(
        characterNamesSignature([{ id: '12', name: 'Alice' }]),
      );
      expect(buildCharacterNameMap([{ id: 12, name: 'Bob' }])).toEqual({ '12': 'Bob' });
    } finally {
      setSaignantInstance(null);
      clearSaignantCache();
    }
  });

  // spec: alhalte-argenterie/log-tab CharacterInfo 更新で mention の描画が更新される (Issue #163)
  it('re-renders a mention when a CharacterInfo broadcast updates the character name', async () => {
    Object.defineProperty(window, 'innerWidth', {
      writable: true,
      configurable: true,
      value: 1280,
    });
    Object.defineProperty(window, 'innerHeight', {
      writable: true,
      configurable: true,
      value: 800,
    });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const room: VisitedRoom = {
      id: 'room-163-mention',
      name: 'Issue 163 部屋',
      url: 'https://alhalte.example/rooms/room-163-mention#key',
      lastVisited: Date.now(),
      isAdmin: true,
    };
    const scene: Scene = {
      id: '/',
      name: 'メインシーン',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
    const socket = new FakeRoomSocket();
    const instance = mount(App, {
      target,
      props: {
        initialRoom: room,
        initialScenes: [scene],
        initialCharacters: [{ ...ALICE, id: '42', name: '勇者ロラン' }],
        isAdmin: true,
        roomSocket: asRoomSocket(socket),
      },
    });

    await settle(30);
    expect(socket.chunkManager).not.toBeNull();
    socket.chunkManager!.appendRecord({
      id: 'msg-mention-163',
      sceneId: '/',
      content: '@42',
      speaker: '勇者ロラン',
      speakerId: '42',
      timestamp: 1700000000,
      contentTin: [...encodeMention(42n)],
    });

    await waitFor(() =>
      (target.querySelector('[data-testid="msg-content-msg-mention-163"]')?.textContent ?? '').includes(
        '@勇者ロラン',
      ),
    );
    const content = target.querySelector('[data-testid="msg-content-msg-mention-163"]');
    expect(content?.textContent).toContain('@勇者ロラン');
    expect(content?.textContent).not.toContain('Anonymous');

    // CharacterInfo replaces the name in App's character state; LogTab must drop
    // the old rendered HTML and resolve the new name.
    socket.emitFrame({
      CharacterInfo: {
        character_id: 42,
        data: characterToTinMap({ ...ALICE, id: '42', name: '勇者ロラン改' }),
      },
    });

    await waitFor(() =>
      (target.querySelector('[data-testid="msg-content-msg-mention-163"]')?.textContent ?? '').includes(
        '@勇者ロラン改',
      ),
    );
    const updated = target.querySelector('[data-testid="msg-content-msg-mention-163"]');
    expect(updated?.textContent).toContain('@勇者ロラン改');
    expect(updated?.textContent).not.toContain('@勇者ロランAnonymous');
    expect(updated?.textContent).not.toContain('@Anonymous');

    unmount(instance);
    target.remove();
  });
});
