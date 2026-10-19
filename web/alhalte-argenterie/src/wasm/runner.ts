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
  self_introduction?: () => bigint | number;
}

function encodeVarint(value: number): number[] {
  const out: number[] = [];
  let v = value >>> 0;
  while (v >= 0x80) {
    out.push((v & 0x7f) | 0x80);
    v >>>= 7;
  }
  out.push(v);
  return out;
}

function decodeVarint(bytes: Uint8Array, offset: number): { value: number; nextOffset: number } {
  let result = 0;
  let shift = 0;
  let pos = offset;
  while (pos < bytes.length) {
    const byte = bytes[pos];
    pos += 1;
    result |= (byte & 0x7f) << shift;
    if ((byte & 0x80) === 0) {
      return { value: result >>> 0, nextOffset: pos };
    }
    shift += 7;
    if (shift > 35) {
      throw new Error('Postcard varint overflow');
    }
  }
  throw new Error('Unexpected EOF while decoding Postcard varint');
}

/**
 * Encodes `(String, String, String)` in Postcard binary format.
 */
export function encodePostcardStringTuple3(
  name: string,
  iconSvg: string,
  description: string,
): Uint8Array {
  const encoder = new TextEncoder();
  const parts = [name, iconSvg, description].map((s) => encoder.encode(s));
  const out: number[] = [];
  for (const part of parts) {
    out.push(...encodeVarint(part.length));
    for (let i = 0; i < part.length; i++) {
      out.push(part[i]);
    }
  }
  return new Uint8Array(out);
}

/**
 * Decodes `(String, String, String)` from Postcard binary format.
 */
export function decodePostcardStringTuple3(bytes: Uint8Array): {
  name: string;
  iconSvg: string;
  description: string;
} {
  const decoder = new TextDecoder();
  let offset = 0;
  const values: string[] = [];
  for (let i = 0; i < 3; i++) {
    const { value: len, nextOffset } = decodeVarint(bytes, offset);
    const end = nextOffset + len;
    if (end > bytes.length) {
      throw new Error('Unexpected EOF while decoding Postcard string');
    }
    values.push(decoder.decode(bytes.subarray(nextOffset, end)));
    offset = end;
  }
  return {
    name: values[0],
    iconSvg: values[1],
    description: values[2],
  };
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
   * Calls `self_introduction()` if exported by the extension module and decodes `(name, icon_svg, description)`.
   */
  public selfIntroduction(): { name: string; iconSvg: string; description: string } | null {
    if (typeof this.exports.self_introduction !== 'function') {
      return null;
    }

    const packedResult = this.exports.self_introduction();
    const resBig = BigInt(packedResult);
    if (resBig === 0n) {
      return null;
    }

    const outPtr = Number(resBig >> 32n);
    const outLen = Number(resBig & 0xffffffffn);
    if (outLen <= 0 || outPtr <= 0) {
      return null;
    }

    const outMemBuffer = new Uint8Array(this.exports.memory.buffer);
    const output = new Uint8Array(outMemBuffer.subarray(outPtr, outPtr + outLen));
    this.exports.dealloc(outPtr, outLen);

    return decodePostcardStringTuple3(output);
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
 * Severity-free reason why an enabled extension did not run (Issue #165).
 */
export type ExtensionPipelineWarningCode = 'missing-wasm';

/**
 * Non-fatal pipeline problem.
 *
 * The pipeline keeps the last run's warnings in {@link ExtensionPipeline.getLastWarnings}
 * and optionally reports them through {@link ExtensionPipelineRunOptions.onWarning}
 * so the UI can show `wasm がありません` instead of silently skipping the
 * extension (Issue #165).
 */
export interface ExtensionPipelineWarning {
  /** Machine readable reason. */
  code: ExtensionPipelineWarningCode;
  /** Id of the skipped extension. */
  extensionId: string;
  /** Display name of the skipped extension. */
  extensionName: string;
  /** Japanese message suitable for a status notice. */
  message: string;
}

/** Options for {@link ExtensionPipeline.executePipeline}. */
export interface ExtensionPipelineRunOptions {
  /** Called once per skipped extension (same objects as `getLastWarnings()`). */
  onWarning?: (warning: ExtensionPipelineWarning) => void;
}

/**
 * Formats pipeline warnings for the App-level `pipeline-warning` notice.
 *
 * @param warnings Warnings of the last pipeline run.
 * @returns `''` when nothing was skipped, otherwise a Japanese notice naming the
 * extensions whose wasm is missing.
 */
export function formatExtensionPipelineWarning(
  warnings: readonly ExtensionPipelineWarning[],
): string {
  if (warnings.length === 0) return '';
  const names = [...new Set(warnings.map((warning) => warning.extensionName).filter(Boolean))];
  const named = names.map((name) => `「${name}」`).join('、');
  return named.length > 0
    ? `拡張機能${named}の wasm がありません。適用せず原文を送信します。`
    : 'wasm がない拡張機能は適用せず原文を送信します。';
}

/**
 * Extension pipeline runner.
 *
 * Runs enabled extensions sequentially in priority order.
 */
export class ExtensionPipeline {
  private instances: Map<string, WasmExtensionInstance> = new Map();
  /** Warnings collected by the most recent {@link executePipeline} run. */
  private lastWarnings: ExtensionPipelineWarning[] = [];

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
   * Warnings collected by the most recent {@link executePipeline} run.
   *
   * An enabled extension without a wasm body is reported here instead of being
   * skipped silently (Issue #165).
   */
  public getLastWarnings(): ExtensionPipelineWarning[] {
    return [...this.lastWarnings];
  }

  /** Records a warning and forwards it to the caller's hook. */
  private warn(
    warning: ExtensionPipelineWarning,
    options: ExtensionPipelineRunOptions,
  ): void {
    this.lastWarnings.push(warning);
    options.onWarning?.(warning);
  }

  /**
   * Executes the extension pipeline on an initial Generast Tin payload.
   *
   * Enabled extensions without a wasm body (and without a registered instance)
   * are reported through {@link getLastWarnings} / `options.onWarning` and
   * skipped; the remaining extensions still run in priority order.
   *
   * @param extensions List of extensions in room
   * @param initialTin Initial plaintext Tin with label alhalte/generast
   * @param options Optional warning hook
   * @returns Transformed Generast Tin payload
   */
  public async executePipeline(
    extensions: ExtensionItem[],
    initialTin: Uint8Array,
    options: ExtensionPipelineRunOptions = {},
  ): Promise<Uint8Array> {
    this.lastWarnings = [];

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

      if (!instance) {
        // Issue #165: an enabled extension without wasm must stay visible as a
        // warning / UI state; silently dropping it hid broken deployments.
        this.warn(
          {
            code: 'missing-wasm',
            extensionId: ext.id,
            extensionName: ext.name,
            message: `拡張機能「${ext.name}」の wasm がありません。`,
          },
          options,
        );
        continue;
      }

      try {
        currentPayload = instance.processGenerast(currentPayload);
      } catch (err) {
        console.error(`Error executing extension "${ext.name}":`, err);
        // If extension execution fails, in Alhalte it may wrap in syntax-error
        // or propagate. We continue or rethrow depending on pipeline mode.
        throw err;
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

/**
 * Default official extensions configured in Bistrot.toml.
 *
 * Includes metadata matching `self_introduction_info()` in each official extension crate.
 */
export const DEFAULT_OFFICIAL_EXTENSIONS: ExtensionItem[] = [
  {
    id: 'alhalte-dsl',
    name: 'Alhalte DSL',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM7.5 18c-.83 0-1.5-.67-1.5-1.5S6.67 15 7.5 15s1.5.67 1.5 1.5S8.33 18 7.5 18zm0-9C6.67 9 6 8.33 6 7.5S6.67 6 7.5 6 9 6.67 9 7.5 8.33 9 7.5 9zm4.5 4.5c-.83 0-1.5-.67-1.5-1.5s.67-1.5 1.5-1.5 1.5.67 1.5 1.5-.67 1.5-1.5 1.5zm4.5 4.5c-.83 0-1.5-.67-1.5-1.5s.67-1.5 1.5-1.5 1.5.67 1.5 1.5-.67 1.5-1.5 1.5zm0-9c-.83 0-1.5-.67-1.5-1.5S15.67 6 16.5 6s1.5.67 1.5 1.5-.67 1.5-1.5 1.5z"/></svg>',
    description:
      'ダイスロール（xDy）・四則演算・比較判定・シークレットダイス・変数展開を処理する公式DSL拡張機能',
    enabled: true,
    priority: 0,
  },
  {
    id: 'markdown',
    name: 'Markdown スタイル',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M2.5 4v16h19V4H2.5zm2 2h15v12h-15V6zm2 2v8h2.5V11l2 2.5 2-2.5V16h2.5V8h-2.5l-2 2.5-2-2.5H6.5zm11 0l-2.5 3h1.5v5h2v-5h1.5l-2.5-3z"/></svg>',
    description:
      '太字（**）・斜体（*）・打ち消し線（~~）などのMarkdown記法を装飾ノードに変換する公式拡張機能',
    enabled: true,
    priority: 1,
  },
  {
    id: 'aozora',
    name: '青空文庫ルビ・傍点',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M18 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zM6 4h5v8l-2.5-1.5L6 12V4z"/></svg>',
    description:
      '青空文庫形式のルビ（｜親文字《ルビ》）と傍点（［＃「…」に傍点］）を変換する公式拡張機能',
    enabled: true,
    priority: 2,
  },
  {
    id: 'coc-6',
    name: 'クトゥルフ神話TRPG (第6版)',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M12 2L4.5 6.5v11L12 22l7.5-4.5v-11L12 2zm0 2.3l5.5 3.3v8.8L12 19.7l-5.5-3.3V7.6L12 4.3zm0 3.7a4 4 0 100 8 4 4 0 000-8zm0 2a2 2 0 110 4 2 2 0 010-4z"/></svg>',
    description:
      'CoC第6版のCCB/CC判定・クリティカル/ファンブル・組み合わせロール・RESB対抗ロールを提供する公式拡張機能',
    enabled: false,
    priority: 3,
  },
  {
    id: 'coc-7',
    name: '新クトゥルフ神話TRPG (第7版)',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M12 2L4.5 6.5v11L12 22l7.5-4.5v-11L12 2zm0 2.3l5.5 3.3v8.8L12 19.7l-5.5-3.3V7.6L12 4.3zM9 9v2h3.5L9 16h2.2l3.5-5V9H9z"/></svg>',
    description:
      'CoC第7版のCC判定（レギュラー/ハード/イクストリーム）・ボーナス/ペナルティダイスを提供する公式拡張機能',
    enabled: false,
    priority: 4,
  },
  {
    id: 'emoklore',
    name: 'エモクロアTRPG',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/></svg>',
    description:
      'エモクロアTRPGのxDM<=y判定（共鳴判定・成功数カウント・ダブル/トリプル等）を提供する公式拡張機能',
    enabled: false,
    priority: 5,
  },
];

export interface ExtensionManifestItem {
  id: string;
  name?: string;
  url: string;
  enabled?: boolean;
  priority?: number;
}

/**
 * Loads default official extensions from `/extensions/manifest.json` (staged by `alhalte-bistrot`),
 * instantiating each `.wasm` binary to read `self_introduction()` metadata, and falling back
 * gracefully to `DEFAULT_OFFICIAL_EXTENSIONS` when offline or in test environments.
 *
 * A manifest entry whose `.wasm` cannot be fetched / instantiated is returned with
 * `wasmMissing: true` (Issue #165) so the UI can show `wasm がありません` instead of
 * listing a metadata-only extension as runnable. When the manifest itself is
 * unreachable the static defaults are returned unchanged: availability is
 * unknown there, so no row is marked.
 */
export async function loadDefaultExtensions(
  fetchFn: typeof fetch | undefined = typeof globalThis.fetch === 'function'
    ? globalThis.fetch.bind(globalThis)
    : undefined,
): Promise<ExtensionItem[]> {
  if (!fetchFn) {
    return DEFAULT_OFFICIAL_EXTENSIONS.map((ext) => ({ ...ext }));
  }

  try {
    const res = await fetchFn('/extensions/manifest.json');
    if (!res.ok) {
      return DEFAULT_OFFICIAL_EXTENSIONS.map((ext) => ({ ...ext }));
    }

    const manifest = (await res.json()) as ExtensionManifestItem[];
    if (!Array.isArray(manifest) || manifest.length === 0) {
      return DEFAULT_OFFICIAL_EXTENSIONS.map((ext) => ({ ...ext }));
    }

    const items: ExtensionItem[] = [];
    for (let idx = 0; idx < manifest.length; idx++) {
      const entry = manifest[idx];
      const fallback = DEFAULT_OFFICIAL_EXTENSIONS.find((d) => d.id === entry.id);

      let wasmBytes: Uint8Array | undefined;
      let intro: { name: string; iconSvg: string; description: string } | null = null;

      if (entry.url) {
        try {
          const wasmRes = await fetchFn(entry.url);
          if (wasmRes.ok) {
            const bytes = new Uint8Array(await wasmRes.arrayBuffer());
            const instance = await WasmExtensionInstance.instantiate(bytes);
            wasmBytes = bytes;
            intro = instance.selfIntroduction();
          }
        } catch {
          // Missing / unreadable / ABI-incompatible wasm: keep the metadata but
          // surface the row as unavailable (Issue #165).
          wasmBytes = undefined;
        }
      }

      const item: ExtensionItem = {
        id: entry.id,
        name: intro?.name || entry.name || fallback?.name || entry.id,
        iconSvg: intro?.iconSvg || fallback?.iconSvg,
        description: intro?.description || fallback?.description,
        enabled: entry.enabled ?? fallback?.enabled ?? true,
        priority: entry.priority ?? idx,
        wasmBytes,
      };
      if (!wasmBytes) item.wasmMissing = true;
      items.push(item);
    }

    return items.sort((a, b) => a.priority - b.priority);
  } catch {
    return DEFAULT_OFFICIAL_EXTENSIONS.map((ext) => ({ ...ext }));
  }
}
