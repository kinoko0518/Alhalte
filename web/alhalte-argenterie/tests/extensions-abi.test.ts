import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import { hexToBytes } from '@alhalte/pantry';
import {
  WasmExtensionInstance,
  ExtensionPipeline,
  canManageExtensions,
  reorderExtensions,
  toggleExtension,
  deleteExtension,
} from '../src/wasm/runner.js';
import ExtensionManagementTab from '../src/components/Control/tabs/ExtensionManagementTab.svelte';
import type { ExtensionItem } from '../src/types/index.js';

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

    const delBtn = target.querySelector<HTMLButtonElement>('[data-testid="ext-delete-btn-ext-1"]');
    expect(delBtn?.disabled).toBe(true);

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
});
