import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import fs from 'node:fs';
import path from 'node:path';
import { hexToBytes } from '@alhalte/pantry';
import {
  WasmExtensionInstance,
  ExtensionPipeline,
  canManageExtensions,
  reorderExtensions,
  toggleExtension,
  deleteExtension,
  encodePostcardStringTuple3,
  decodePostcardStringTuple3,
  formatExtensionPipelineWarning,
  loadDefaultExtensions,
} from '../src/wasm/runner.js';
import ExtensionManagementTab from '../src/components/Control/tabs/ExtensionManagementTab.svelte';
import type { ExtensionItem } from '../src/types/index.js';
import { buildGenerastFromText } from '../src/utils/generast.js';
import { renderGenerast } from '../src/utils/saignant.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';

// Valid WebAssembly binary implementing:
// - memory (1 page)
// - alhalte_abi_version() -> 1
// - alloc(len) -> 0
// - dealloc(ptr, len) -> void
// - process_generast(ptr, len) -> copies input to 1024, appends '!' (0x21), returns (1024n << 32) | (len + 1)
const VALID_WASM_HEX =
  '0061736d010000000115046000017f60017f017f60027f7f0060027f7f017e030504000102030503010001074505066d656d6f7279020013616c68616c74655f6162695f76657273696f6e000005616c6c6f630001076465616c6c6f6300021070726f636573735f67656e657261737400030a3404040041010b040041000b02000b250041800820002001fc0a000041800820016a41213a0000428008422086200141016aad840b';

// Invalid version WebAssembly binary (alhalte_abi_version returns 2)
const INVALID_VERSION_WASM_HEX =
  '0061736d010000000115046000017f60017f017f60027f7f0060027f7f017e030504000102030503010001074505066d656d6f7279020013616c68616c74655f6162695f76657273696f6e000005616c6c6f630001076465616c6c6f6300021070726f636573735f67656e657261737400030a1304040041020b040041000b02000b0400422a0b';

function uleb128(value: number): number[] {
  const out: number[] = [];
  let v = value >>> 0;
  while (v >= 0x80) {
    out.push((v & 0x7f) | 0x80);
    v >>>= 7;
  }
  out.push(v);
  return out;
}

function sleb128(value: number): number[] {
  const out: number[] = [];
  let v = value | 0;
  while (true) {
    const byte = v & 0x7f;
    v >>= 7;
    if ((v === 0 && (byte & 0x40) === 0) || (v === -1 && (byte & 0x40) !== 0)) {
      out.push(byte);
      return out;
    }
    out.push(byte | 0x80);
  }
}

function section(id: number, payload: number[]): number[] {
  return [id, ...uleb128(payload.length), ...payload];
}

function exportEntry(name: string, kind: number, index: number): number[] {
  const nameBytes = Array.from(new TextEncoder().encode(name));
  return [...uleb128(nameBytes.length), ...nameBytes, kind, ...uleb128(index)];
}

function buildWasmWithSelfIntro(introBytes: Uint8Array): Uint8Array {
  const typeSec = section(1, [
    5,
    0x60, 0x00, 0x01, 0x7f, // () -> i32
    0x60, 0x01, 0x7f, 0x01, 0x7f, // (i32) -> i32
    0x60, 0x02, 0x7f, 0x7f, 0x00, // (i32, i32) -> ()
    0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7e, // (i32, i32) -> i64
    0x60, 0x00, 0x01, 0x7e, // () -> i64
  ]);
  const funcSec = section(3, [5, 0, 1, 2, 3, 4]);
  const memSec = section(5, [1, 0x00, 1]);
  const expPayload = [
    6,
    ...exportEntry('memory', 2, 0),
    ...exportEntry('alhalte_abi_version', 0, 0),
    ...exportEntry('alloc', 0, 1),
    ...exportEntry('dealloc', 0, 2),
    ...exportEntry('process_generast', 0, 3),
    ...exportEntry('self_introduction', 0, 4),
  ];
  const expSec = section(7, expPayload);

  const fn0 = [0x00, 0x41, 0x01, 0x0b];
  const fn1 = [0x00, 0x41, 0x00, 0x0b];
  const fn2 = [0x00, 0x0b];
  const fn3 = [0x00, 0x42, 0x00, 0x0b];
  // (2048n << 32n) | BigInt(introBytes.length)
  const fn4 = [
    0x00,
    0x42, ...sleb128(2048),
    0x42, 0x20,
    0x86, // i64.shl
    0x41, ...sleb128(introBytes.length),
    0xad, // i64.extend_i32_u
    0x84, // i64.or
    0x0b,
  ];
  const bodies = [fn0, fn1, fn2, fn3, fn4].flatMap((b) => [...uleb128(b.length), ...b]);
  const codeSec = section(10, [5, ...bodies]);

  const dataSeg = [
    0x00,
    0x41, ...sleb128(2048), 0x0b,
    ...uleb128(introBytes.length),
    ...Array.from(introBytes),
  ];
  const dataSec = section(11, [1, ...dataSeg]);

  return new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    ...typeSec,
    ...funcSec,
    ...memSec,
    ...expSec,
    ...codeSec,
    ...dataSec,
  ]);
}

describe('Extensions Tab & WASM ABI Host Runner', () => {
  // spec: alhalte-argenterie/extensions-abi 管理者権限ロック（非管理者グレーアウト）
  it('locks extension management for non-admins (grayed out & disabled) and permits admin edits', async () => {
    const extensions: ExtensionItem[] = [
      { id: 'ext-1', name: 'alhalte-dsl', enabled: true, priority: 0 },
      { id: 'ext-2', name: 'markdown', enabled: false, priority: 1 },
    ];

    // Non-admin permission checks
    expect(canManageExtensions(false)).toBe(false);
    expect(() => reorderExtensions(extensions, 0, 1, false)).toThrow(/Permission denied/);
    expect(() => toggleExtension(extensions, 'ext-1', false)).toThrow(/Permission denied/);
    expect(() => deleteExtension(extensions, 'ext-1', false)).toThrow(/Permission denied/);

    // Admin permission checks
    expect(canManageExtensions(true)).toBe(true);
    const toggled = toggleExtension(extensions, 'ext-1', true);
    expect(toggled.find((e) => e.id === 'ext-1')?.enabled).toBe(false);

    const reordered = reorderExtensions(extensions, 0, 1, true);
    expect(reordered[0].id).toBe('ext-2');
    expect(reordered[1].id).toBe('ext-1');

    const deleted = deleteExtension(extensions, 'ext-2', true);
    expect(deleted).toHaveLength(1);
    expect(deleted[0].id).toBe('ext-1');

    // UI test for non-admin locking
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instanceNonAdmin = mount(ExtensionManagementTab, {
      target,
      props: {
        extensions,
        isAdmin: false,
      },
    });

    const rootEl = target.querySelector('[data-testid="extension-management-tab"]');
    expect(rootEl?.classList.contains('non-admin-locked')).toBe(true);
    expect(target.querySelector('[data-testid="lock-indicator"]')).toBeTruthy();

    const checkbox = target.querySelector<HTMLInputElement>('[data-testid="ext-toggle-ext-1"]');
    expect(checkbox?.disabled).toBe(true);

    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="ext-more-btn-ext-1"]');
    expect(moreBtn?.disabled).toBe(true);

    const delBtn = target.querySelector('[data-testid="ext-delete-btn-ext-1"]');
    expect(delBtn?.getAttribute('aria-disabled') === 'true' || delBtn?.classList.contains('disabled')).toBe(true);

    unmount(instanceNonAdmin);
    target.remove();
  });

  // spec: alhalte-argenterie/extensions-abi WASM ABI バージョン確認
  it('validates alhalte_abi_version is 1 and rejects incompatible versions', async () => {
    // Valid module
    const validBytes = hexToBytes(VALID_WASM_HEX);
    const instance = await WasmExtensionInstance.instantiate(validBytes);
    expect(instance).toBeDefined();

    // Invalid module with version 2
    const invalidBytes = hexToBytes(INVALID_VERSION_WASM_HEX);
    await expect(WasmExtensionInstance.instantiate(invalidBytes)).rejects.toThrow(
      /Unsupported extension ABI version: 2/,
    );
  });

  // spec: alhalte-argenterie/extensions-abi WASM process_generast 実行
  it('executes process_generast via ABI memory alloc/dealloc and unpacks result', async () => {
    const validBytes = hexToBytes(VALID_WASM_HEX);
    const instance = await WasmExtensionInstance.instantiate(validBytes);

    const inputMsg = new TextEncoder().encode('Hello Alhalte');
    const outputBytes = instance.processGenerast(inputMsg);

    const outputText = new TextDecoder().decode(outputBytes);
    expect(outputText).toBe('Hello Alhalte!');
  });

  // spec: alhalte-argenterie/extensions-abi 優先度順パイプライン実行
  it('runs multiple enabled extensions sequentially in priority order through the pipeline', async () => {
    const pipeline = new ExtensionPipeline();
    const validBytes = hexToBytes(VALID_WASM_HEX);

    const extensions: ExtensionItem[] = [
      {
        id: 'ext-disabled',
        name: 'disabled-ext',
        enabled: false, // Skipped
        priority: 0,
        wasmBytes: validBytes,
      },
      {
        id: 'ext-second',
        name: 'second-ext',
        enabled: true,
        priority: 2,
        wasmBytes: validBytes,
      },
      {
        id: 'ext-first',
        name: 'first-ext',
        enabled: true,
        priority: 1,
        wasmBytes: validBytes,
      },
    ];

    const inputMsg = new TextEncoder().encode('Dice');
    const resultBytes = await pipeline.executePipeline(extensions, inputMsg);

    // Each active extension appends '!'
    // Two active extensions -> 'Dice!!'
    const resultText = new TextDecoder().decode(resultBytes);
    expect(resultText).toBe('Dice!!');
  });

  // spec: extensions/self_introduction Postcard (String, String, String) デコードと UI 表示
  it('decodes self_introduction Postcard metadata from WASM and renders iconSvg & description in UI', async () => {
    const name = 'クトゥルフ神話TRPG (第6版)';
    const iconSvg =
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24"><circle cx="12" cy="12" r="10"/></svg>';
    const description =
      'CoC第6版のCCB/CC判定・クリティカル/ファンブル・組み合わせロール・RESB対抗ロールを提供する公式拡張機能';

    const encoded = encodePostcardStringTuple3(name, iconSvg, description);
    const decoded = decodePostcardStringTuple3(encoded);
    expect(decoded).toEqual({ name, iconSvg, description });

    // Module without self_introduction returns null
    const legacyInstance = await WasmExtensionInstance.instantiate(hexToBytes(VALID_WASM_HEX));
    expect(legacyInstance.selfIntroduction()).toBeNull();

    // Module with self_introduction returns decoded metadata
    const introWasmBytes = buildWasmWithSelfIntro(encoded);
    const introInstance = await WasmExtensionInstance.instantiate(introWasmBytes);
    const intro = introInstance.selfIntroduction();
    expect(intro).toEqual({ name, iconSvg, description });

    // UI renders iconSvg, name, and description
    const target = document.createElement('div');
    document.body.appendChild(target);

    const extensions: ExtensionItem[] = [
      {
        id: 'coc-6',
        name: intro!.name,
        iconSvg: intro!.iconSvg,
        description: intro!.description,
        enabled: true,
        priority: 0,
      },
    ];

    const mounted = mount(ExtensionManagementTab, {
      target,
      props: {
        extensions,
        isAdmin: true,
      },
    });

    const iconEl = target.querySelector('[data-testid="ext-icon-coc-6"]');
    expect(iconEl).toBeTruthy();
    expect(iconEl?.querySelector('svg')).toBeTruthy();

    const nameEl = target.querySelector('[data-testid="ext-name-coc-6"]');
    expect(nameEl?.textContent?.trim()).toBe(name);

    const descEl = target.querySelector('[data-testid="ext-description-coc-6"]');
    expect(descEl?.textContent?.trim()).toBe(description);

    unmount(mounted);
    target.remove();
  });

  // spec: alhalte-argenterie/extensions-abi ログタブ連携による左畳み込みGenerast変換とSaignant HTML生成 (Issue #117, Issue #127)
  it('integrates extension pipeline left fold with Generast AST and Saignant HTML rendering for log messages', async () => {
    const pipeline = new ExtensionPipeline();
    const dslBytes = fs.readFileSync(path.resolve(__dirname, '../public/extensions/alhalte-dsl.wasm'));
    const markdownBytes = fs.readFileSync(path.resolve(__dirname, '../public/extensions/markdown.wasm'));

    const extensions: ExtensionItem[] = [
      {
        id: 'alhalte-dsl',
        name: 'Alhalte DSL',
        enabled: true,
        priority: 0,
        wasmBytes: dslBytes,
      },
      {
        id: 'markdown',
        name: 'Markdown スタイル',
        enabled: true,
        priority: 1,
        wasmBytes: markdownBytes,
      },
    ];

    // 1. Japanese message "ほげほげ" should not produce label mismatch error (Issue #127)
    const astHoge = await buildGenerastFromText('ほげほげ', extensions, pipeline, 'char-1');
    expect(astHoge instanceof Uint8Array).toBe(true);
    const htmlHoge = await renderGenerast(astHoge);
    expect(htmlHoge).toBe('ほげほげ');
    expect(htmlHoge).not.toContain('label mismatch');
    expect(htmlHoge).not.toContain('0xd38f9ce233c2315a');

    // 2. Dice roll expression '1D100<=50' processed through DSL and rendered by Saignant
    const astDice = await buildGenerastFromText('1D100<=50', extensions, pipeline, 'char-1');
    expect(astDice instanceof Uint8Array).toBe(true);
    const htmlDice = await renderGenerast(astDice);
    expect(htmlDice).toContain('saignant-diceroll');
    expect(htmlDice).toContain('100');

    // 3. Markdown styled expression '**重要**' processed through Markdown and rendered by Saignant
    const astMd = await buildGenerastFromText('**重要**', extensions, pipeline, 'char-1');
    expect(astMd instanceof Uint8Array).toBe(true);
    const htmlMd = await renderGenerast(astMd);
    expect(htmlMd).toContain('<strong class="saignant-bold">重要</strong>');
  });

  // spec: alhalte-argenterie/extensions-abi 拡張機能アクションメニューのアンカーボタン隣接配置 (Issue #124)
  it('positions extension action menu directly adjacent to more button with position-anchor', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const extensions: ExtensionItem[] = [
      {
        id: 'ext-test',
        name: 'Test Extension',
        enabled: true,
        priority: 0,
      },
    ];

    const instance = mount(ExtensionManagementTab, {
      target,
      props: {
        extensions,
        isAdmin: true,
        // The tab is inert when it has no socket (Issue #109 / #145), so drive
        // the menu on a connected one.
        roomSocket: asRoomSocket(new FakeRoomSocket()),
      },
    });

    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="ext-more-btn-ext-test"]');
    expect(moreBtn).toBeTruthy();

    vi.spyOn(moreBtn!, 'getBoundingClientRect').mockReturnValue({
      top: 150,
      bottom: 186,
      left: 600,
      right: 636,
      width: 36,
      height: 36,
      x: 600,
      y: 150,
      toJSON: () => {},
    });

    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    const menu = target.querySelector<HTMLElement>('[data-testid="ext-menu-ext-test"]');
    expect(menu).toBeTruthy();
    expect(menu?.style.position).toBe('fixed');
    expect(menu?.getAttribute('style')).toContain('inset: unset');
    expect(menu?.style.margin).toBe('0px');
    // top = bottom (186) + offset (4) = 190px
    expect(menu?.style.top).toBe('190px');
    expect(moreBtn?.style.getPropertyValue('anchor-name')).toBe('--ext-menu-anchor-ext-test');
    expect(menu?.style.getPropertyValue('position-anchor')).toBe('--ext-menu-anchor-ext-test');

    unmount(instance);
    target.remove();
  });

  // spec: Issue #165 executePipeline は wasm の無い有効拡張を警告として残す
  it('reports enabled extensions without wasm instead of skipping them silently', async () => {
    const pipeline = new ExtensionPipeline();
    const validBytes = hexToBytes(VALID_WASM_HEX);
    const warnings: Array<{ extensionId: string; extensionName: string }> = [];

    const extensions: ExtensionItem[] = [
      { id: 'missing', name: 'Missing Ext', enabled: true, priority: 0 },
      { id: 'present', name: 'Present Ext', enabled: true, priority: 1, wasmBytes: validBytes },
      { id: 'disabled-missing', name: 'Disabled Missing', enabled: false, priority: 2 },
    ];

    const input = new TextEncoder().encode('Dice');
    const output = await pipeline.executePipeline(extensions, input, {
      onWarning: (warning) => warnings.push(warning),
    });

    // The runnable extension still ran; only the wasm-less one was skipped.
    expect(new TextDecoder().decode(output)).toBe('Dice!');
    expect(warnings).toEqual([
      expect.objectContaining({
        code: 'missing-wasm',
        extensionId: 'missing',
        extensionName: 'Missing Ext',
      }),
    ]);
    const lastWarnings = pipeline.getLastWarnings();
    expect(lastWarnings).toHaveLength(1);
    expect(lastWarnings[0].message).toContain('wasm がありません');
    expect(formatExtensionPipelineWarning(lastWarnings)).toContain('「Missing Ext」');
    expect(formatExtensionPipelineWarning([])).toBe('');

    // A later successful run clears the previous warnings.
    await pipeline.executePipeline(
      [{ id: 'present', name: 'Present Ext', enabled: true, priority: 0, wasmBytes: validBytes }],
      input,
    );
    expect(pipeline.getLastWarnings()).toEqual([]);
  });

  // spec: Issue #165 loadDefaultExtensions は wasm 取得失敗を wasmMissing として公開する
  it('exposes manifest entries whose wasm is missing as wasmMissing', async () => {
    const validBytes = hexToBytes(VALID_WASM_HEX);
    const fetchFn = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url === '/extensions/manifest.json') {
        return new Response(
          JSON.stringify([
            {
              id: 'markdown',
              name: 'Markdown スタイル',
              url: '/extensions/markdown.wasm',
              enabled: true,
              priority: 0,
            },
            {
              id: 'aozora',
              name: '青空文庫',
              url: '/extensions/aozora.wasm',
              enabled: false,
              priority: 1,
            },
            { id: 'no-url', name: 'No URL', enabled: true, priority: 2 },
          ]),
          { status: 200, headers: { 'content-type': 'application/json' } },
        );
      }
      if (url === '/extensions/markdown.wasm') {
        return new Response(validBytes, { status: 200 });
      }
      return new Response('not found', { status: 404 });
    });

    const items = await loadDefaultExtensions(fetchFn as unknown as typeof fetch);

    const markdown = items.find((item) => item.id === 'markdown');
    expect(markdown?.wasmBytes).toEqual(validBytes);
    expect(markdown?.wasmMissing).toBeUndefined();
    // Issue #165: a manifest entry without a fetchable wasm must be visible as
    // `wasm がありません`, not silently listed as if it were runnable.
    expect(items.find((item) => item.id === 'aozora')?.wasmMissing).toBe(true);
    expect(items.find((item) => item.id === 'no-url')?.wasmMissing).toBe(true);
  });

  // spec: Issue #165 拡張機能タブは wasm 欠落行を可視化し有効化を無効にする
  it('shows wasm がありません and disables enabling for an unavailable row', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const socket = new FakeRoomSocket();
    socket.stateSnapshot = {
      'extension::__seeded': true,
      'extension::broken': {
        id: 'broken',
        name: 'Broken Extension',
        priority: 0,
        enabled: true,
      },
      'extension::ok': {
        id: 'ok',
        name: 'OK Extension',
        priority: 1,
        enabled: true,
      },
    };

    const mounted = mount(ExtensionManagementTab, {
      target,
      props: {
        extensions: [
          { id: 'broken', name: 'Broken Extension', enabled: true, priority: 0, wasmMissing: true },
          {
            id: 'ok',
            name: 'OK Extension',
            enabled: true,
            priority: 1,
            wasmBytes: hexToBytes(VALID_WASM_HEX),
          },
        ],
        isAdmin: true,
        roomSocket: asRoomSocket(socket),
      },
    });
    await new Promise((r) => setTimeout(r, 40));

    const missing = target.querySelector<HTMLElement>('[data-testid="ext-wasm-missing-broken"]');
    expect(missing?.textContent?.trim()).toBe('wasm がありません');
    expect(
      target.querySelector('[data-testid="extension-item-broken"]')?.getAttribute('data-wasm-missing'),
    ).toBe('true');
    expect(
      target.querySelector<HTMLInputElement>('[data-testid="ext-toggle-broken"]')?.disabled,
    ).toBe(true);
    expect(
      target.querySelector<HTMLInputElement>('[data-testid="ext-toggle-ok"]')?.disabled,
    ).toBe(false);
    // The row can still be deleted, so the admin can clean up the broken entry.
    expect(
      target.querySelector<HTMLButtonElement>('[data-testid="ext-more-btn-broken"]')?.disabled,
    ).toBe(false);

    unmount(mounted);
    target.remove();
  });
});

