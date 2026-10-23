/**
 * WebAssembly ABI Host Runner for Alhalte Extensions.
 *
 * References:
 * - ARCHITECTURE.md: `# 拡張機能 -> ## 拡張機能ABI`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> 拡張機能タブ`
 */

import type { ExtensionItem } from '../types/index.js';

/**
 * ABI version implemented by the official extensions (`(id, name, icon_svg, description)`).
 *
 * ABI 1 is still accepted for reading room state / older wasm bodies; see
 * {@link MIN_SUPPORTED_ALHALTE_ABI_VERSION}.
 */
export const CURRENT_ALHALTE_ABI_VERSION = 2;

/**
 * Oldest extension ABI the host still runs.
 *
 * ABI 1 `self_introduction` returns `(name, icon_svg, description)`; the host
 * uses the manifest id for those extensions (Issue #274).
 */
export const MIN_SUPPORTED_ALHALTE_ABI_VERSION = 1;

/**
 * Decoded `self_introduction` metadata.
 *
 * `id` is only present for ABI 2+ wasm. ABI 1 has no id, so the manifest id is
 * authoritative there.
 */
export interface SelfIntroduction {
  /** Extension id declared by the wasm (ABI 2+ only). */
  id?: string;
  /** Display name. */
  name: string;
  /** Inline SVG icon markup. */
  iconSvg: string;
  /** Human readable description. */
  description: string;
}

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
 * Encodes a tuple of strings in Postcard binary format.
 */
function encodePostcardStringTuple(values: readonly string[]): Uint8Array {
  const encoder = new TextEncoder();
  const parts = values.map((s) => encoder.encode(s));
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
 * Encodes `(String, String, String)` in Postcard binary format.
 */
export function encodePostcardStringTuple3(
  name: string,
  iconSvg: string,
  description: string,
): Uint8Array {
  return encodePostcardStringTuple([name, iconSvg, description]);
}

/**
 * Encodes `(String, String, String, String)` in Postcard binary format.
 *
 * Used by tests (and hosts) to build ABI 2 `self_introduction` payloads.
 */
export function encodePostcardStringTuple4(
  id: string,
  name: string,
  iconSvg: string,
  description: string,
): Uint8Array {
  return encodePostcardStringTuple([id, name, iconSvg, description]);
}

/**
 * Decodes `count` consecutive Postcard strings from binary format.
 */
function decodePostcardStrings(bytes: Uint8Array, count: number): string[] {
  const decoder = new TextDecoder();
  let offset = 0;
  const values: string[] = [];
  for (let i = 0; i < count; i++) {
    const { value: len, nextOffset } = decodeVarint(bytes, offset);
    const end = nextOffset + len;
    if (end > bytes.length) {
      throw new Error('Unexpected EOF while decoding Postcard string');
    }
    values.push(decoder.decode(bytes.subarray(nextOffset, end)));
    offset = end;
  }
  return values;
}

/**
 * Decodes `(String, String, String)` from Postcard binary format (ABI 1).
 */
export function decodePostcardStringTuple3(bytes: Uint8Array): {
  name: string;
  iconSvg: string;
  description: string;
} {
  const values = decodePostcardStrings(bytes, 3);
  return {
    name: values[0],
    iconSvg: values[1],
    description: values[2],
  };
}

/**
 * Decodes `(String, String, String, String)` from Postcard binary format (ABI 2).
 */
export function decodePostcardStringTuple4(bytes: Uint8Array): {
  id: string;
  name: string;
  iconSvg: string;
  description: string;
} {
  const values = decodePostcardStrings(bytes, 4);
  return {
    id: values[0],
    name: values[1],
    iconSvg: values[2],
    description: values[3],
  };
}

/**
 * WebAssembly instance wrapper for an Alhalte extension module.
 */
export class WasmExtensionInstance {
  private instance: WebAssembly.Instance;
  private exports: WasmExtensionExports;
  private abiVersion: number;

  private constructor(instance: WebAssembly.Instance) {
    this.instance = instance;
    const exp = instance.exports as unknown as WasmExtensionExports;

    if (typeof exp.alhalte_abi_version !== 'function') {
      throw new Error('Extension does not export alhalte_abi_version');
    }
    const version = exp.alhalte_abi_version();
    if (
      !Number.isInteger(version) ||
      version < MIN_SUPPORTED_ALHALTE_ABI_VERSION ||
      version > CURRENT_ALHALTE_ABI_VERSION
    ) {
      throw new Error(
        `Unsupported extension ABI version: ${version} (supported ${MIN_SUPPORTED_ALHALTE_ABI_VERSION}..${CURRENT_ALHALTE_ABI_VERSION})`,
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
    this.abiVersion = version;
  }

  /**
   * ABI version reported by the wasm (`alhalte_abi_version`).
   */
  public getAbiVersion(): number {
    return this.abiVersion;
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
   * Calls `self_introduction()` if exported by the extension module and decodes it.
   *
   * ABI 1 returns `(name, icon_svg, description)` and the result has no `id`.
   * ABI 2 returns `(id, name, icon_svg, description)`. The optional export means
   * a v2 wasm may return `null`; the caller then falls back to the manifest id.
   */
  public selfIntroduction(): SelfIntroduction | null {
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

    if (this.abiVersion >= 2) {
      return decodePostcardStringTuple4(output);
    }
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
 * Default official extensions configured in Restaurant.toml.
 *
 * Offline / test fallback only: the display metadata of the real rows comes
 * from each staged wasm (`self_introduction`, ABI 2). The values here must stay
 * in sync with the official wasm; `extensions-abi.test.ts` compares them with
 * the staged `public/extensions/*.wasm` (Issue #274).
 */
export const DEFAULT_OFFICIAL_EXTENSIONS: ExtensionItem[] = [
  {
    id: 'alhalte-dsl',
    name: 'Alhalte DSL',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM7.5 18c-.83 0-1.5-.67-1.5-1.5S6.67 15 7.5 15s1.5.67 1.5 1.5S8.33 18 7.5 18zm0-9C6.67 9 6 8.33 6 7.5S6.67 6 7.5 6 9 6.67 9 7.5 8.33 9 7.5 9zm4.5 4.5c-.83 0-1.5-.67-1.5-1.5s.67-1.5 1.5-1.5 1.5.67 1.5 1.5-.67 1.5-1.5 1.5zm4.5 4.5c-.83 0-1.5-.67-1.5-1.5s.67-1.5 1.5-1.5 1.5.67 1.5 1.5-.67 1.5-1.5 1.5zm0-9c-.83 0-1.5-.67-1.5-1.5S15.67 6 16.5 6s1.5.67 1.5 1.5-.67 1.5-1.5 1.5z"/></svg>',
    description:
      'ダイスの記法などを処理します。ほとんどのルール拡張の動作のために必要です。',
    enabled: true,
    priority: 0,
  },
  {
    id: 'markdown',
    name: 'Markdown',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M2.5 4v16h19V4H2.5zm2 2h15v12h-15V6zm2 2v8h2.5V11l2 2.5 2-2.5V16h2.5V8h-2.5l-2 2.5-2-2.5H6.5zm11 0l-2.5 3h1.5v5h2v-5h1.5l-2.5-3z"/></svg>',
    description: '太字（**）・斜体（*）・打ち消し線（~~）などのMarkdown記法を処理します。',
    enabled: true,
    priority: 1,
  },
  {
    id: 'aozora',
    name: '青空文庫書式',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M18 2H6c-1.1 0-2 .9-2 2v16c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zM6 4h5v8l-2.5-1.5L6 12V4z"/></svg>',
    description: '青空文庫書式を変換する公式拡張機能',
    enabled: true,
    priority: 2,
  },
  {
    id: 'coc-6',
    name: 'クトゥルフ神話TRPG (第6版)',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M12 2L4.5 6.5v11L12 22l7.5-4.5v-11L12 2zm0 2.3l5.5 3.3v8.8L12 19.7l-5.5-3.3V7.6L12 4.3zm0 3.7a4 4 0 100 8 4 4 0 000-8zm0 2a2 2 0 110 4 2 2 0 010-4z"/></svg>',
    description: 'クトゥルフ神話TRPG第6版の記法や処理のための公式拡張機能です。',
    enabled: false,
    priority: 3,
  },
  {
    id: 'coc-7',
    name: '新クトゥルフ神話TRPG (第7版)',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M12 2L4.5 6.5v11L12 22l7.5-4.5v-11L12 2zm0 2.3l5.5 3.3v8.8L12 19.7l-5.5-3.3V7.6L12 4.3zM9 9v2h3.5L9 16h2.2l3.5-5V9H9z"/></svg>',
    description: '新クトゥルフ神話TRPG第7版の記法や処理のための公式拡張機能です。',
    enabled: false,
    priority: 4,
  },
  {
    id: 'emoklore',
    name: 'エモクロアTRPG',
    iconSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="currentColor"><path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/></svg>',
    description: 'エモクロアTRPGの記法や処理のための公式拡張機能です。',
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
 *
 * ABI 2 extensions declare their own id in `self_introduction`. When it differs
 * from the manifest id the wasm is not trusted (fail-closed, Issue #274): the
 * row keeps the manifest metadata, is marked `wasmMissing` and never runs.
 * ABI 1 wasm has no declared id, so the manifest id is used.
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
      let intro: SelfIntroduction | null = null;

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
          intro = null;
        }
      }

      // Issue #274: a wasm that introduces itself under a different id is not
      // the extension the manifest asked for. Fail closed instead of running
      // foreign code under the manifest id.
      if (wasmBytes && intro?.id && intro.id !== entry.id) {
        console.warn(
          `[extensions] self_introduction id mismatch: manifest id "${entry.id}" but wasm declares "${intro.id}"; the extension is disabled`,
        );
        wasmBytes = undefined;
        intro = null;
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
