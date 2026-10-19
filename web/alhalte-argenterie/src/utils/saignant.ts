/**
 * WebAssembly Loader and Runtime Wrapper for `alhalte-saignant`.
 *
 * Strictly executes `web/alhalte-argenterie/public/wasm/alhalte_saignant.wasm`
 * for Generast interpretation, HTML rendering, and CSS token injection.
 * (TS reimplementation of Saignant rendering is forbidden by specification).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-saignant`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> ログタブ`
 * - GitHub Issue #55: `feat(argenterie): ログタブの共通ヘッダー (Search)、ZIP 出力、キャラカラー Pill、Saignant WASM 描画`
 */

import type { GenerastAST } from './generast.js';

/**
 * ABI version this client expects from the committed
 * `public/wasm/alhalte_saignant.wasm` artifact.
 *
 * v2 adds `render_generast_abi_v2(ptr, len, names_ptr, names_len)` with a JSON
 * `{ "<id>": "<name>" }` character-name map (Issue #163). The loader below still
 * works with a v1-only artifact by falling back to `render_generast_abi`.
 */
export const CURRENT_ALHALTE_ABI_VERSION = 2;

/**
 * Minimal character shape needed to build the Saignant name map.
 *
 * `Character` satisfies this structurally, so callers do not need to import a
 * Saignant-specific type. IDs are stringified because the ABI keys are strings.
 */
export interface SaignantCharacter {
  id: string | number;
  name: string;
}

/**
 * 64-bit label hash for `alhalte/generast`:
 * `xxh64("alhalte/generast", 0) = 0xd38f9ce233c2315an`
 */
export const LABEL_ALHALTE_GENERAST = 0xd38f9ce233c2315an;

/**
 * 64-bit label hash for `alhalte/generast/style/bold`.
 */
export const LABEL_ALHALTE_GENERAST_STYLE_BOLD = 0x346cd94e0673773en;

/**
 * 64-bit label hash for `alhalte/generast/style/italic`.
 */
export const LABEL_ALHALTE_GENERAST_STYLE_ITALIC = 0xa8aced56b8362da0n;

export interface SaignantWasmExports {
  memory: WebAssembly.Memory;
  alhalte_abi_version: () => number;
  alloc: (len: number) => number;
  dealloc: (ptr: number, len: number) => void;
  render_generast_abi: (ptr: number, len: number) => bigint;
  /**
   * ABI v2: like `render_generast_abi` plus a JSON character-name map.
   * Optional so a v1-only artifact still loads (Issue #163).
   */
  render_generast_abi_v2?: (
    ptr: number,
    len: number,
    namesPtr: number,
    namesLen: number,
  ) => bigint;
  get_full_css_abi: () => bigint;
}

let cachedInstance: WebAssembly.Instance | null = null;
let instancePromise: Promise<WebAssembly.Instance> | null = null;
let cachedCss: string | null = null;
const renderCache = new Map<string, string>();
const MAX_RENDER_CACHE_SIZE = 1000;

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

/**
 * Encodes a plain text string into a Generast::String AST wrapped in a Tin.
 *
 * Wire layout:
 * - 8 bytes: Tin label `LABEL_ALHALTE_GENERAST` (u64 little-endian)
 * - Postcard payload:
 *   - Variant 1 (Generast::String)
 *   - Varint string length
 *   - UTF-8 string bytes
 */
export function encodeStringAsGenerastTin(text: string): Uint8Array {
  const encoder = new TextEncoder();
  const textBytes = encoder.encode(text);
  const payload = [
    1, // Variant 1: Generast::String
    ...encodeVarint(textBytes.length),
    ...textBytes,
  ];

  const tinBytes = new Uint8Array(8 + payload.length);
  const dv = new DataView(tinBytes.buffer);
  dv.setBigUint64(0, LABEL_ALHALTE_GENERAST, true);
  tinBytes.set(payload, 8);
  return tinBytes;
}

/**
 * Encodes a Generast AST node into Postcard bytes and wraps in a Tin.
 */
export function encodeGenerastAstToTin(ast: GenerastAST | any): Uint8Array {
  if (typeof ast === 'string') {
    return encodeStringAsGenerastTin(ast);
  }

  if (ast instanceof Uint8Array) {
    return ast;
  }

  if (ast && typeof ast === 'object') {
    if (ast.type === 'string' && typeof ast.value === 'string') {
      return encodeStringAsGenerastTin(ast.value);
    }

    if (ast.type === 'nested') {
      // Nested node
      const metaLabel =
        ast.meta === 'bold'
          ? LABEL_ALHALTE_GENERAST_STYLE_BOLD
          : ast.meta === 'italic'
            ? LABEL_ALHALTE_GENERAST_STYLE_ITALIC
            : typeof ast.meta === 'bigint'
              ? ast.meta
              : typeof ast.meta === 'number'
                ? BigInt(ast.meta)
                : LABEL_ALHALTE_GENERAST_STYLE_BOLD;

      const children = Array.isArray(ast.contents) ? ast.contents : [];
      const childrenPayloads: number[] = [];
      for (const child of children) {
        if (child.type === 'string' && typeof child.value === 'string') {
          const enc = new TextEncoder().encode(child.value);
          childrenPayloads.push(1, ...encodeVarint(enc.length), ...enc);
        }
      }

      const payload = [
        2, // Variant 2: Generast::Nested
        ...encodeVarint(metaLabel),
        0, // meta payload len = 0
        ...encodeVarint(children.length),
        ...childrenPayloads,
      ];

      const tinBytes = new Uint8Array(8 + payload.length);
      const dv = new DataView(tinBytes.buffer);
      dv.setBigUint64(0, LABEL_ALHALTE_GENERAST, true);
      tinBytes.set(payload, 8);
      return tinBytes;
    }
  }

  return encodeStringAsGenerastTin(String(ast ?? ''));
}

/**
 * Builds the `{ "<id>": "<name>" }` map passed to `render_generast_abi_v2`.
 *
 * Character IDs are stringified so the Rust side can parse the decimal `u64`
 * IDs it stores in `generast/character-id` mentions. Non-numeric local IDs are
 * still sent; the Rust ABI ignores keys it cannot parse.
 */
export function buildCharacterNameMap(
  characters?: readonly SaignantCharacter[] | null,
): Record<string, string> {
  const map: Record<string, string> = {};
  for (const character of characters ?? []) {
    if (!character) continue;
    const id = String(character.id ?? '');
    if (id.length === 0) continue;
    map[id] = typeof character.name === 'string' ? character.name : String(character.name ?? '');
  }
  return map;
}

/**
 * Deterministic signature of the names that affect Saignant rendering.
 *
 * Included in the render cache key so a `CharacterList` / `CharacterInfo`
 * update can never reuse HTML rendered with the old name. Entries are sorted so
 * a reordered character list is not treated as a change.
 */
export function characterNamesSignature(
  characters?: readonly SaignantCharacter[] | null,
): string {
  const entries = (characters ?? [])
    .filter(Boolean)
    .map((character) => [String(character.id ?? ''), String(character.name ?? '')] as const);
  entries.sort((left, right) => {
    if (left[0] !== right[0]) return left[0] < right[0] ? -1 : 1;
    if (left[1] !== right[1]) return left[1] < right[1] ? -1 : 1;
    return 0;
  });
  return JSON.stringify(entries);
}

/**
 * Loads and instantiates `alhalte_saignant.wasm`.
 *
 * In browser environments, fetches from `/wasm/alhalte_saignant.wasm`.
 * In Node.js / test environments, reads the wasm file via `fs.readFile`.
 */
export async function loadSaignantWasm(): Promise<WebAssembly.Instance> {
  if (cachedInstance) {
    return cachedInstance;
  }
  if (instancePromise) {
    const instance = await instancePromise;
    // `setSaignantInstance(null)` may have cleared `cachedInstance` while the
    // promise stayed resolved; restore it so sync rendering does not fall back.
    cachedInstance = instance;
    return instance;
  }

  instancePromise = (async () => {
    let wasmBuffer: ArrayBuffer | Uint8Array;

    const isNode =
      typeof process !== 'undefined' &&
      process.versions != null &&
      process.versions.node != null;

    if (isNode) {
      const fs = await import('node:fs');
      const path = await import('node:path');

      const candidates = [
        path.resolve(process.cwd(), 'web/alhalte-argenterie/public/wasm/alhalte_saignant.wasm'),
        path.resolve(process.cwd(), 'public/wasm/alhalte_saignant.wasm'),
        path.resolve(process.cwd(), 'alhalte-argenterie/public/wasm/alhalte_saignant.wasm'),
        path.resolve(__dirname, '../../public/wasm/alhalte_saignant.wasm'),
      ];

      let targetPath: string | null = null;
      for (const p of candidates) {
        if (fs.existsSync(p)) {
          targetPath = p;
          break;
        }
      }

      if (!targetPath) {
        throw new Error(
          `alhalte_saignant.wasm not found in candidate paths: ${candidates.join(', ')}`,
        );
      }

      wasmBuffer = fs.readFileSync(targetPath);
    } else {
      // Browser environment
      const wasmUrl = '/wasm/alhalte_saignant.wasm';
      const response = await fetch(wasmUrl);
      if (!response.ok) {
        throw new Error(`Failed to fetch ${wasmUrl}: HTTP ${response.status}`);
      }
      wasmBuffer = await response.arrayBuffer();
    }

    const result = await WebAssembly.instantiate(wasmBuffer as BufferSource);
    const instance: WebAssembly.Instance =
      'instance' in result ? (result as { instance: WebAssembly.Instance }).instance : result;
    cachedInstance = instance;
    return instance;
  })();

  return instancePromise;
}

/**
 * Sets a custom WebAssembly instance (useful for unit tests and mocks).
 *
 * Passing `null` also clears the in-flight instantiation promise so a later
 * `loadSaignantWasm` re-instantiates the real artifact instead of returning a
 * detached resolved promise.
 */
export function setSaignantInstance(instance: WebAssembly.Instance | null): void {
  cachedInstance = instance;
  instancePromise = instance ? Promise.resolve(instance) : null;
  cachedCss = null;
  renderCache.clear();
}

/**
 * Returns true if Saignant WASM has been instantiated.
 */
export function isSaignantInitialized(): boolean {
  return cachedInstance !== null;
}

/**
 * Initializes Saignant WASM and injects the Saignant CSS stylesheet.
 */
export async function initSaignant(): Promise<void> {
  await loadSaignantWasm();
  injectSaignantCss();
}

/**
 * Retrieves the full Saignant CSS stylesheet (Structural + Expressive) from WASM ABI.
 */
export function getSaignantCss(): string {
  if (cachedCss !== null) {
    return cachedCss;
  }
  if (!cachedInstance) {
    return '';
  }

  const exports = cachedInstance.exports as unknown as SaignantWasmExports;
  const packed = exports.get_full_css_abi();
  if (packed === 0n) {
    return '';
  }

  const ptr = Number(packed >> 32n);
  const len = Number(packed & 0xffffffffn);
  const bytes = new Uint8Array(exports.memory.buffer, ptr, len);
  const css = new TextDecoder().decode(bytes);
  exports.dealloc(ptr, len);

  cachedCss = css;
  return css;
}

/**
 * Injects the Saignant CSS stylesheet into the document `<head>`.
 */
export function injectSaignantCss(): void {
  if (typeof document === 'undefined') return;

  const styleId = 'alhalte-saignant-stylesheet';
  if (document.getElementById(styleId)) return;

  const css = getSaignantCss();
  if (!css) return;

  const styleEl = document.createElement('style');
  styleEl.id = styleId;
  styleEl.textContent = css;
  document.head.appendChild(styleEl);
}

/**
 * Clears the rendered HTML cache.
 */
export function clearSaignantCache(): void {
  renderCache.clear();
}

/**
 * Renders a Generast payload or log text into semantic HTML using Saignant WASM ABI.
 *
 * @param input Plain text string, GenerastAST object, or serialized Tin/Postcard byte array.
 * @param characters Current characters used to resolve mentions / status names
 *   through ABI v2. A v1-only artifact ignores the names.
 * @returns Rendered HTML string.
 */
export async function renderGenerast(
  input: string | Uint8Array | GenerastAST | any,
  characters?: readonly SaignantCharacter[] | null,
): Promise<string> {
  if (!cachedInstance) {
    await loadSaignantWasm();
  }
  return renderGenerastSync(input, characters);
}

/**
 * Synchronous Generast rendering using cached Saignant WASM instance.
 *
 * If WASM is not yet instantiated, triggers asynchronous initialization
 * and returns safe HTML-escaped text as an immediate fallback.
 *
 * @param input Generast payload to render.
 * @param characters Current characters used for ABI v2 name resolution.
 */
export function renderGenerastSync(
  input: string | Uint8Array | GenerastAST | any,
  characters?: readonly SaignantCharacter[] | null,
): string {
  if (input === null || input === undefined) {
    return '';
  }

  const namesSignature = characterNamesSignature(characters);
  let cacheKey: string | null = null;
  if (typeof input === 'string') {
    // Names affect the output, so they are part of the cache identity. The
    // caller (LogTab) also clears the cache when characters change; this makes
    // a stale hit impossible even for callers that do not.
    cacheKey = `${namesSignature}\u0000${input}`;
    if (renderCache.has(cacheKey)) {
      return renderCache.get(cacheKey)!;
    }
  }

  if (!cachedInstance) {
    // Initiate background load
    void loadSaignantWasm().then(() => injectSaignantCss());
    const fallbackText = typeof input === 'string' ? input : JSON.stringify(input);
    return fallbackText
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  }

  const exports = cachedInstance.exports as unknown as SaignantWasmExports;

  let inBytes: Uint8Array;
  if (input instanceof Uint8Array) {
    inBytes = input;
  } else if (typeof input === 'string') {
    inBytes = encodeStringAsGenerastTin(input);
  } else {
    inBytes = encodeGenerastAstToTin(input);
  }

  if (inBytes.length === 0) {
    return '';
  }

  // ABI v2: pass the current names. A v1-only artifact keeps the old path.
  const renderV2 = exports.render_generast_abi_v2;
  let namesPtr = 0;
  let namesLen = 0;
  if (renderV2) {
    const namesBytes = new TextEncoder().encode(
      JSON.stringify(buildCharacterNameMap(characters)),
    );
    namesLen = namesBytes.length;
    namesPtr = exports.alloc(namesLen);
    if (namesPtr !== 0) {
      new Uint8Array(exports.memory.buffer).set(namesBytes, namesPtr);
    }
  }

  const inPtr = exports.alloc(inBytes.length);
  if (inPtr === 0) {
    if (namesPtr !== 0) {
      exports.dealloc(namesPtr, namesLen);
    }
    return '';
  }

  new Uint8Array(exports.memory.buffer).set(inBytes, inPtr);
  const packed = renderV2
    ? renderV2(inPtr, inBytes.length, namesPtr, namesPtr === 0 ? 0 : namesLen)
    : exports.render_generast_abi(inPtr, inBytes.length);
  exports.dealloc(inPtr, inBytes.length);
  if (namesPtr !== 0) {
    exports.dealloc(namesPtr, namesLen);
  }

  if (packed === 0n) {
    return '';
  }

  const outPtr = Number(packed >> 32n);
  const outLen = Number(packed & 0xffffffffn);
  const outBytes = new Uint8Array(exports.memory.buffer, outPtr, outLen);
  const html = new TextDecoder().decode(outBytes);
  exports.dealloc(outPtr, outLen);

  if (cacheKey !== null) {
    if (renderCache.size >= MAX_RENDER_CACHE_SIZE) {
      renderCache.clear();
    }
    renderCache.set(cacheKey, html);
  }

  return html;
}
