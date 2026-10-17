/**
 * WebAssembly ABI Host Runner for Alhalte Extensions.
 *
 * References:
 * - ARCHITECTURE.md: `# 拡張機能 -> ## 拡張機能ABI`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> 拡張機能タブ`
 */

import type { ExtensionItem } from '../types/index.js';

export const CURRENT_ALHALTE_ABI_VERSION = 1;

export interface WasmExtensionExports {
  memory: WebAssembly.Memory;
  alhalte_abi_version: () => number;
  alloc: (len: number) => number;
  dealloc: (ptr: number, len: number) => void;
  process_generast: (ptr: number, len: number) => bigint | number;
}

/**
 * WebAssembly instance wrapper for an Alhalte extension module.
 */
export class WasmExtensionInstance {
  private instance: WebAssembly.Instance;
  private exports: WasmExtensionExports;

  private constructor(instance: WebAssembly.Instance) {
    this.instance = instance;
    const exp = instance.exports as unknown as WasmExtensionExports;

    if (typeof exp.alhalte_abi_version !== 'function') {
      throw new Error('Extension does not export alhalte_abi_version');
    }
    const version = exp.alhalte_abi_version();
    if (version !== CURRENT_ALHALTE_ABI_VERSION) {
      throw new Error(
        `Unsupported extension ABI version: ${version} (expected ${CURRENT_ALHALTE_ABI_VERSION})`,
      );
    }

    if (
      !(exp.memory instanceof WebAssembly.Memory) ||
      typeof exp.alloc !== 'function' ||
      typeof exp.dealloc !== 'function' ||
      typeof exp.process_generast !== 'function'
    ) {
      throw new Error(
        'Extension missing required exports (memory, alloc, dealloc, process_generast)',
      );
    }

    this.exports = exp;
  }

  /**
   * Instantiates an extension from raw wasm binary bytes.
   */
  public static async instantiate(wasmBytes: Uint8Array): Promise<WasmExtensionInstance> {
    const compiled = await WebAssembly.compile(wasmBytes as unknown as BufferSource);
    const instance = await WebAssembly.instantiate(compiled, {});
    return new WasmExtensionInstance(instance);
  }

  /**
   * Executes process_generast on input Tin bytes.
   */
  public processGenerast(inputBytes: Uint8Array): Uint8Array {
    const len = inputBytes.length;
    const inPtr = this.exports.alloc(len);

    try {
      const memBuffer = new Uint8Array(this.exports.memory.buffer);
      memBuffer.set(inputBytes, inPtr);

      const packedResult = this.exports.process_generast(inPtr, len);
      const resBig = BigInt(packedResult);
      const outPtr = Number(resBig >> 32n);
      const outLen = Number(resBig & 0xffffffffn);

      if (outLen < 0 || outPtr < 0) {
        throw new Error(`Invalid output pointer/length: ptr=${outPtr}, len=${outLen}`);
      }

      // Copy output bytes before deallocation
      const outMemBuffer = new Uint8Array(this.exports.memory.buffer);
      const output = new Uint8Array(outMemBuffer.subarray(outPtr, outPtr + outLen));

      // Deallocate output memory
      this.exports.dealloc(outPtr, outLen);

      return output;
    } finally {
      // Deallocate input memory
      this.exports.dealloc(inPtr, len);
    }
  }
}

/**
 * Extension pipeline runner.
 *
 * Runs enabled extensions sequentially in priority order.
 */
export class ExtensionPipeline {
  private instances: Map<string, WasmExtensionInstance> = new Map();

  /**
   * Registers or updates a loaded wasm instance for an extension item.
   */
  public registerInstance(id: string, instance: WasmExtensionInstance): void {
    this.instances.set(id, instance);
  }

  /**
   * Unregisters an instance.
   */
  public unregisterInstance(id: string): void {
    this.instances.delete(id);
  }

  /**
   * Executes the extension pipeline on an initial Generast Tin payload.
   *
   * @param extensions List of extensions in room
   * @param initialTin Initial plaintext Tin with label alhalte/generast
   * @returns Transformed Generast Tin payload
   */
  public async executePipeline(
    extensions: ExtensionItem[],
    initialTin: Uint8Array,
  ): Promise<Uint8Array> {
    // Filter active and sort by priority ascending
    const active = extensions
      .filter((ext) => ext.enabled)
      .sort((a, b) => a.priority - b.priority);

    let currentPayload = initialTin;

    for (const ext of active) {
      let instance = this.instances.get(ext.id);

      if (!instance && ext.wasmBytes) {
        instance = await WasmExtensionInstance.instantiate(ext.wasmBytes);
        this.instances.set(ext.id, instance);
      }

      if (instance) {
        try {
          currentPayload = instance.processGenerast(currentPayload);
        } catch (err) {
          console.error(`Error executing extension "${ext.name}":`, err);
          // If extension execution fails, in Alhalte it may wrap in syntax-error
          // or propagate. We continue or rethrow depending on pipeline mode.
          throw err;
        }
      }
    }

    return currentPayload;
  }
}

/**
 * Validates whether the current user is permitted to modify extensions.
 *
 * ARCHITECTURE.md: "拡張機能の追加や優先度の変更、削除、無効化ができるのは部屋の管理者のみ。部屋の管理者以外にはグレーアウトして見え、触っても動かない。"
 */
export function canManageExtensions(isAdmin: boolean): boolean {
  return isAdmin === true;
}

/**
 * Reorders extensions array by moving an item from `sourceIndex` to `targetIndex`.
 * Automatically updates `priority` values.
 */
export function reorderExtensions(
  extensions: ExtensionItem[],
  sourceIndex: number,
  targetIndex: number,
  isAdmin: boolean,
): ExtensionItem[] {
  if (!canManageExtensions(isAdmin)) {
    throw new Error('Permission denied: only room admins can reorder extensions');
  }

  const items = [...extensions].sort((a, b) => a.priority - b.priority);
  if (
    sourceIndex < 0 ||
    sourceIndex >= items.length ||
    targetIndex < 0 ||
    targetIndex >= items.length
  ) {
    return items;
  }

  const [moved] = items.splice(sourceIndex, 1);
  items.splice(targetIndex, 0, moved);

  return items.map((item, index) => ({
    ...item,
    priority: index,
  }));
}

/**
 * Toggles extension enabled status.
 */
export function toggleExtension(
  extensions: ExtensionItem[],
  extensionId: string,
  isAdmin: boolean,
): ExtensionItem[] {
  if (!canManageExtensions(isAdmin)) {
    throw new Error('Permission denied: only room admins can toggle extensions');
  }

  return extensions.map((ext) =>
    ext.id === extensionId ? { ...ext, enabled: !ext.enabled } : ext,
  );
}

/**
 * Deletes an extension.
 */
export function deleteExtension(
  extensions: ExtensionItem[],
  extensionId: string,
  isAdmin: boolean,
): ExtensionItem[] {
  if (!canManageExtensions(isAdmin)) {
    throw new Error('Permission denied: only room admins can delete extensions');
  }

  return extensions
    .filter((ext) => ext.id !== extensionId)
    .map((ext, idx) => ({ ...ext, priority: idx }));
}
