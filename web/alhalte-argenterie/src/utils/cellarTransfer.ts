/**
 * Authenticated browser → maitre-d → cellar transfer utilities.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-cellar`, `## alhalte-pantry`, `## alhalte-argenterie -> ### アセット`
 * - GitHub Issue #109 / #138: 部屋アセット・拡張機能 wasm の実転送路
 *
 * The browser never reaches `alhalte-cellar` directly. Every request goes to
 * `/api/cellar/*` on `alhalte-maitre-d`, which resolves the caller from the
 * `session_id` cookie and injects `X-Alhalte-Owner` / `X-Alhalte-Session-Id`
 * itself. This module therefore authenticates with the cookie only (no
 * `X-Alhalte-Account` claim), so a stale `localStorage` account can never make
 * the proxy reject a request.
 *
 * Transfer flow (2-step, see the cellar status codes):
 * 1. `POST /api/cellar/store` with a metadata handshake.
 * 2. `210` → `PUT /api/cellar/store/{tinId}` sends the body.
 *    `211` / `212` → the body already exists; the upload is skipped.
 *
 * The room key is the 64-hex URL fragment of a room URL (`#<64hex>`); it is
 * only ever sent to the cellar wrapped inside the tin, never on the wire.
 */

import {
  Compression,
  Encryption,
  bytesToHex,
  computeBlake3Hash,
  decodeTin,
  encodeTin,
  hexToBytes,
  type DecryptedTin,
  type EncodeTinOptions,
  type EncodedTinResult,
} from '@alhalte/pantry';

/** Path prefix of the maitre-d cellar proxy (Vite dev proxy forwards `/api`). */
export const CELLAR_PROXY_BASE_PATH = '/api/cellar';

/**
 * Default timeout for a single transfer step, in milliseconds.
 *
 * Mirrors the server-side backstop so a stalled handshake or upload fails
 * observably instead of hanging the UI.
 */
export const DEFAULT_OPERATION_TIMEOUT_MS = 8000;

/** Cellar status codes returned by the metadata handshake. */
export const CELLAR_STATUS_UPLOAD_REQUIRED = 210;
/** The entity already existed; a reference row was added. */
export const CELLAR_STATUS_DEDUPLICATED = 211;
/** The caller already owns the entity. */
export const CELLAR_STATUS_ALREADY_OWNED = 212;

/** 64 hex characters: a room key or a 256-bit tin id. */
const HEX_64_PATTERN = /^[0-9a-fA-F]{64}$/;
const HEX_PATTERN = /^[0-9a-fA-F]+$/;

/**
 * Machine readable reason for a failed cellar transfer.
 *
 * Downstream UI switches on this instead of parsing messages:
 * - `invalid-room-key`: the supplied room key is not 32 bytes.
 * - `invalid-tin-id`: the tin id is not 64 hex characters.
 * - `invalid-argument`: a non-byte payload was passed to {@link uploadTin}.
 * - `encode-failed`: pantry's `encodeTin` rejected the payload.
 * - `decode-failed`: pantry's `decodeTin` rejected the downloaded bytes.
 * - `http-error`: maitre-d / cellar answered with an unexpected status.
 * - `network-error`: the request never completed (connection reset, CORS, ...).
 * - `timeout`: `timeoutMs` elapsed and the request was aborted.
 * - `aborted`: the caller's `AbortSignal` fired.
 * - `invalid-response`: the response body was not the expected shape.
 */
export type CellarTransferErrorCode =
  | 'invalid-room-key'
  | 'invalid-tin-id'
  | 'invalid-argument'
  | 'encode-failed'
  | 'decode-failed'
  | 'http-error'
  | 'network-error'
  | 'timeout'
  | 'aborted'
  | 'invalid-response';

/**
 * Typed error raised by every function in this module.
 *
 * The HTTP status (when any) is exposed as `status` so callers can distinguish
 * e.g. `403` (spoofed owner / no session) from `502` (cellar unreachable).
 */
export class CellarTransferError extends Error {
  /** Machine readable failure reason. */
  readonly code: CellarTransferErrorCode;

  /** HTTP status of the failing response, when the failure had one. */
  readonly status?: number;

  /**
   * Creates a transfer error.
   *
   * @param code Machine readable failure reason.
   * @param message Human readable description.
   * @param options Optional HTTP status and underlying cause.
   */
  constructor(
    code: CellarTransferErrorCode,
    message: string,
    options?: { status?: number; cause?: unknown },
  ) {
    super(message, options && options.cause !== undefined ? { cause: options.cause } : undefined);
    this.name = 'CellarTransferError';
    this.code = code;
    this.status = options?.status;
  }
}

/**
 * Type guard for {@link CellarTransferError}.
 */
export function isCellarTransferError(error: unknown): error is CellarTransferError {
  return error instanceof CellarTransferError;
}

/**
 * Minimal structural subset of `XMLHttpRequest` used by {@link uploadTin}.
 *
 * Declaring it structurally keeps the upload testable without a real browser
 * (tests inject a fake). A real `XMLHttpRequest` satisfies it at runtime; the
 * default factory bridges the two with a documented cast because the DOM typings
 * narrow `send` to `BufferSource<ArrayBuffer>`, while tin bytes are
 * `Uint8Array<ArrayBufferLike>`.
 */
export interface CellarXhr {
  /** Opens a request. */
  open(method: string, url: string): void;
  /** Adds a request header. */
  setRequestHeader(name: string, value: string): void;
  /** Sends the request body (UTF-8 string or raw bytes). */
  send(body?: Uint8Array | string | null): void;
  /** Aborts the request. */
  abort(): void;
  /** Final HTTP status (0 until the request completes). */
  readonly status: number;
  /** Text response body. */
  readonly responseText: string;
  /** Upload progress source (`xhr.upload`). */
  readonly upload: { onprogress: ((event: ProgressEvent) => void) | null };
  /** Completion handler (also fires for 4xx / 5xx). */
  onload: ((event: ProgressEvent) => void) | null;
  /** Transport failure handler. */
  onerror: ((event: ProgressEvent) => void) | null;
  /** Abort handler (timeout or caller cancellation). */
  onabort: ((event: ProgressEvent) => void) | null;
}

/**
 * Factory returning the `XMLHttpRequest`-compatible object used for uploads.
 *
 * Inject a fake in tests; production uses `new XMLHttpRequest()`.
 *
 * @see {@link CellarXhr} for why the default implementation is cast.
 */
export type CellarXhrFactory = () => CellarXhr;

/** Default factory backed by the browser `XMLHttpRequest`. */
const defaultXhrFactory: CellarXhrFactory = () =>
  new XMLHttpRequest() as unknown as CellarXhr;

/**
 * Pantry encoder signature accepted by {@link encodeRoomTin}.
 *
 * Defaults to pantry's `encodeTin`; inject a deterministic fake in tests.
 */
export type TinEncoder = (
  tin: { label: bigint; payload: Uint8Array },
  encryption: Encryption,
  compression: Compression,
  options?: EncodeTinOptions,
) => Promise<EncodedTinResult>;

/**
 * Options for {@link encodeRoomTin}.
 */
export interface EncodeRoomTinOptions {
  /** Encoder override (defaults to pantry's `encodeTin`). */
  encoder?: TinEncoder;
  /**
   * Compression for the tin body. Defaults to {@link Compression.Lz4}; pass
   * {@link Compression.None} for very small wasm payloads.
   */
  compression?: Compression;
  /** Deterministic 12-byte nonce (defaults to random) — tests only. */
  nonce?: Uint8Array;
}

/**
 * Result of {@link encodeRoomTin}.
 */
export interface EncodedRoomTin {
  /** Wire-format bytes to hand to {@link uploadTin}. */
  wireBytes: Uint8Array;
  /** 64-character lowercase hex tin id. */
  tinIdHex: string;
}

/**
 * Options for {@link uploadTin}.
 */
export interface UploadTinOptions {
  /** Upload progress callback receiving `0..1`. */
  onProgress?: (progress: number) => void;
  /** Caller cancellation. */
  signal?: AbortSignal;
  /** Timeout for each step in milliseconds (defaults to {@link DEFAULT_OPERATION_TIMEOUT_MS}). */
  timeoutMs?: number;
  /**
   * Pre-computed tin id (from {@link encodeRoomTin}). When omitted it is
   * derived from `wireBytes` with BLAKE3.
   */
  tinId?: string;
  /** Optional wrapped key (`12B nonce || ciphertext`) forwarded to cellar. */
  wrappedKey?: Uint8Array;
  /** XHR factory override (tests). */
  xhrFactory?: CellarXhrFactory;
  /** Proxy base path (defaults to {@link CELLAR_PROXY_BASE_PATH}). */
  baseUrl?: string;
}

/**
 * Result of {@link uploadTin}.
 */
export interface UploadTinResult {
  /** 64-character lowercase hex tin id stored by cellar. */
  tinId: string;
}

/**
 * Options for {@link downloadTin}.
 */
export interface DownloadTinOptions {
  /** Caller cancellation. */
  signal?: AbortSignal;
  /** Timeout in milliseconds (defaults to {@link DEFAULT_OPERATION_TIMEOUT_MS}). */
  timeoutMs?: number;
  /** Fetch implementation override (tests). */
  fetchFn?: typeof fetch;
  /** Proxy base path (defaults to {@link CELLAR_PROXY_BASE_PATH}). */
  baseUrl?: string;
}

/**
 * Reads the room key from a URL fragment (`#<64 hex>`).
 *
 * The fragment never leaves the browser, which is why it is a safe place for
 * the room key.
 *
 * @param location Location-like object (defaults to `globalThis.location`).
 * @returns The 32-byte room key, or `null` when the fragment is absent or not
 * a 64-character hex string.
 */
export function getRoomKeyFromLocation(
  location?: { hash: string } | null,
): Uint8Array | null {
  const target =
    location ?? (typeof globalThis !== 'undefined' ? globalThis.location : undefined) ?? null;
  if (!target || typeof target.hash !== 'string' || target.hash.length === 0) {
    return null;
  }

  const fragment = target.hash.startsWith('#') ? target.hash.slice(1) : target.hash;
  if (!HEX_64_PATTERN.test(fragment)) {
    return null;
  }
  return hexToBytes(fragment.toLowerCase());
}

/**
 * Encodes a room payload into a tin encrypted with the room key.
 *
 * Uses `Encryption.RoomKey` (random 96-bit nonce, AES-256-GCM-SIV) and, by
 * default, `Compression.Lz4`.
 *
 * @param payload Raw bytes (image, audio, wasm, ...).
 * @param label 64-bit tin label identifying the payload format.
 * @param roomKey 32-byte room key (see {@link getRoomKeyFromLocation}).
 * @param options Encoder / compression overrides (tests).
 * @returns The wire bytes and the resulting tin id.
 * @throws {CellarTransferError} `invalid-room-key` or `encode-failed`.
 */
export async function encodeRoomTin(
  payload: Uint8Array,
  label: bigint,
  roomKey: Uint8Array,
  options?: EncodeRoomTinOptions,
): Promise<EncodedRoomTin> {
  if (!(roomKey instanceof Uint8Array) || roomKey.length !== 32) {
    throw new CellarTransferError(
      'invalid-room-key',
      `Room key must be 32 bytes (256 bits); received ${roomKey?.length ?? 0}`,
    );
  }

  const encoder = options?.encoder ?? encodeTin;
  const compression = options?.compression ?? Compression.Lz4;

  try {
    const encoded = await encoder(
      { label, payload },
      Encryption.RoomKey,
      compression,
      options?.nonce ? { roomKey, nonce: options.nonce } : { roomKey },
    );
    return { wireBytes: encoded.wireBytes, tinIdHex: encoded.tinIdHex.toLowerCase() };
  } catch (error) {
    throw new CellarTransferError('encode-failed', `Failed to encode room tin: ${String(error)}`, {
      cause: error,
    });
  }
}

/** Parsed result of a single XHR call. */
interface XhrResult {
  /** Final HTTP status. */
  status: number;
  /** Text response body. */
  responseText: string;
}

/** Internal XHR call options. */
interface XhrCallOptions {
  /** HTTP method. */
  method: 'POST' | 'PUT';
  /** Absolute or root-relative URL. */
  url: string;
  /** Request headers. */
  headers: Record<string, string>;
  /** Request body. */
  body?: Uint8Array | string | null;
  /** Caller cancellation. */
  signal?: AbortSignal;
  /** Per-request timeout. */
  timeoutMs: number;
  /** Upload progress sink. */
  onProgress?: (progress: number) => void;
}

/**
 * Runs one XHR call, translating timeout / abort / transport failures into
 * {@link CellarTransferError}.
 */
function performXhr(
  factory: CellarXhrFactory,
  options: XhrCallOptions,
): Promise<XhrResult> {
  return new Promise<XhrResult>((resolve, reject) => {
    // Nothing is allocated when the caller already cancelled.
    if (options.signal?.aborted) {
      reject(new CellarTransferError('aborted', 'Cellar transfer was aborted by the caller'));
      return;
    }

    let settled = false;
    let timedOut = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const xhr = factory();

    const cleanup = (): void => {
      if (timer !== undefined) {
        clearTimeout(timer);
        timer = undefined;
      }
      options.signal?.removeEventListener('abort', handleSignalAbort);
      xhr.onload = null;
      xhr.onerror = null;
      xhr.onabort = null;
      xhr.upload.onprogress = null;
    };

    function fail(error: CellarTransferError): void {
      if (settled) return;
      settled = true;
      cleanup();
      reject(error);
    }

    function succeed(result: XhrResult): void {
      if (settled) return;
      settled = true;
      cleanup();
      resolve(result);
    }

    function handleSignalAbort(): void {
      xhr.abort();
    }

    xhr.onload = () => {
      succeed({ status: xhr.status, responseText: xhr.responseText ?? '' });
    };
    xhr.onerror = () => {
      fail(
        new CellarTransferError(
          'network-error',
          `Cellar request to ${options.url} failed before a response was received`,
        ),
      );
    };
    xhr.onabort = () => {
      if (timedOut) {
        fail(
          new CellarTransferError(
            'timeout',
            `Cellar request to ${options.url} exceeded ${options.timeoutMs} ms`,
          ),
        );
      } else {
        fail(new CellarTransferError('aborted', 'Cellar transfer was aborted by the caller'));
      }
    };

    if (options.onProgress) {
      const onProgress = options.onProgress;
      xhr.upload.onprogress = (event: ProgressEvent) => {
        const total = event.total;
        const ratio = total > 0 ? event.loaded / total : 0;
        onProgress(Math.min(1, Math.max(0, ratio)));
      };
    }

    try {
      xhr.open(options.method, options.url);
      for (const [name, value] of Object.entries(options.headers)) {
        xhr.setRequestHeader(name, value);
      }
      options.signal?.addEventListener('abort', handleSignalAbort);
      timer = setTimeout(() => {
        timedOut = true;
        xhr.abort();
      }, options.timeoutMs);
      xhr.send(options.body ?? null);
    } catch (error) {
      fail(
        new CellarTransferError(
          'network-error',
          `Failed to start cellar request to ${options.url}: ${String(error)}`,
          { cause: error },
        ),
      );
    }
  });
}

/**
 * Builds the headers shared by every proxy request.
 *
 * Ownership headers are intentionally absent: maitre-d derives the account
 * from the `session_id` cookie and rejects any client-supplied claim.
 */
function baseHeaders(): Record<string, string> {
  return { Accept: 'application/json' };
}

/** Converts a wrapped key to the lowercase hex header value cellar expects. */
function wrappedKeyToHex(wrappedKey: Uint8Array | undefined): string | null {
  if (!wrappedKey || wrappedKey.length === 0) return null;
  return bytesToHex(wrappedKey).toLowerCase();
}

/**
 * Normalizes and validates a tin id.
 *
 * @throws {CellarTransferError} `invalid-tin-id`.
 */
function requireTinId(tinId: string): string {
  if (typeof tinId !== 'string' || !HEX_64_PATTERN.test(tinId.trim())) {
    throw new CellarTransferError(
      'invalid-tin-id',
      `Tin id must be 64 hex characters; received ${JSON.stringify(tinId)}`,
    );
  }
  return tinId.trim().toLowerCase();
}

/** Extracts a short, safe error description from a response body. */
function describeBody(body: string): string {
  const trimmed = body.trim();
  if (trimmed.length === 0) return 'empty response';
  return trimmed.length > 200 ? `${trimmed.slice(0, 200)}…` : trimmed;
}

/**
 * Uploads an already encoded tin to cellar through the maitre-d proxy.
 *
 * Performs the 2-step handshake:
 * 1. `POST /api/cellar/store` with `{ tin_id, size, wrapped_key }`.
 * 2. `210` → `PUT /api/cellar/store/{tinId}` with the wire bytes.
 *    `211` / `212` → resolves without uploading.
 *
 * `onProgress` reports the upload of step 2 as `0..1`; step 1 is a small
 * metadata request that reports no progress. Both steps are bounded by
 * `timeoutMs` and by `signal`.
 *
 * @param wireBytes Encoded tin bytes (see {@link encodeRoomTin}).
 * @param options Progress, cancellation, timeout, and injection hooks.
 * @returns The tin id cellar stored the bytes under.
 * @throws {CellarTransferError} with `http-error`, `timeout`, `aborted`, or
 * `network-error`.
 */
export async function uploadTin(
  wireBytes: Uint8Array,
  options?: UploadTinOptions,
): Promise<UploadTinResult> {
  if (!(wireBytes instanceof Uint8Array) || wireBytes.length === 0) {
    throw new CellarTransferError(
      'invalid-argument',
      'wireBytes must be a non-empty Uint8Array',
    );
  }

  const baseUrl = (options?.baseUrl ?? CELLAR_PROXY_BASE_PATH).replace(/\/+$/, '');
  const timeoutMs = options?.timeoutMs ?? DEFAULT_OPERATION_TIMEOUT_MS;
  const xhrFactory: CellarXhrFactory = options?.xhrFactory ?? defaultXhrFactory;
  const tinId = options?.tinId
    ? requireTinId(options.tinId)
    : bytesToHex(computeBlake3Hash(wireBytes)).toLowerCase();
  const wrappedKeyHex = wrappedKeyToHex(options?.wrappedKey);

  // Step 1: metadata handshake. `owner` / `session_id` are placeholders — the
  // maitre-d proxy overwrites both from the authenticated session.
  const handshake = await performXhr(xhrFactory, {
    method: 'POST',
    url: `${baseUrl}/store`,
    headers: { ...baseHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({
      owner: 0,
      session_id: 0,
      tin_id: tinId,
      size: wireBytes.byteLength,
      wrapped_key: wrappedKeyHex,
    }),
    signal: options?.signal,
    timeoutMs,
  });

  if (
    handshake.status !== CELLAR_STATUS_UPLOAD_REQUIRED &&
    handshake.status !== CELLAR_STATUS_DEDUPLICATED &&
    handshake.status !== CELLAR_STATUS_ALREADY_OWNED
  ) {
    throw new CellarTransferError(
      'http-error',
      `Cellar store handshake failed with HTTP ${handshake.status}: ${describeBody(handshake.responseText)}`,
      { status: handshake.status },
    );
  }

  if (handshake.status !== CELLAR_STATUS_UPLOAD_REQUIRED) {
    // 211 / 212: the entity already exists, the reference is registered.
    options?.onProgress?.(1);
    return { tinId };
  }

  // Step 2: upload the body.
  const uploadHeaders: Record<string, string> = {
    ...baseHeaders(),
    'Content-Type': 'application/octet-stream',
  };
  if (wrappedKeyHex) {
    uploadHeaders['X-Alhalte-Wrapped-Key'] = wrappedKeyHex;
  }

  const upload = await performXhr(xhrFactory, {
    method: 'PUT',
    url: `${baseUrl}/store/${encodeURIComponent(tinId)}`,
    headers: uploadHeaders,
    body: wireBytes,
    signal: options?.signal,
    timeoutMs,
    ...(options?.onProgress ? { onProgress: options.onProgress } : {}),
  });

  if (upload.status < 200 || upload.status >= 300) {
    throw new CellarTransferError(
      'http-error',
      `Cellar upload failed with HTTP ${upload.status}: ${describeBody(upload.responseText)}`,
      { status: upload.status },
    );
  }

  options?.onProgress?.(1);
  return { tinId };
}

/**
 * Downloads and decrypts a room tin through the maitre-d proxy.
 *
 * Reads `X-Alhalte-Wrapped-Key` (hex) when present and delegates decryption to
 * pantry's `decodeTin`, verifying the result against `tinId`.
 *
 * @param tinId 64-character hex tin id.
 * @param roomKey 32-byte room key (see {@link getRoomKeyFromLocation}).
 * @param options Cancellation, timeout, and injection hooks.
 * @returns The decrypted tin (payload, label, opener, tin id).
 * @throws {CellarTransferError} with `invalid-tin-id`, `http-error`,
 * `timeout`, `aborted`, `network-error`, or `decode-failed`.
 */
export async function downloadTin(
  tinId: string,
  roomKey: Uint8Array,
  options?: DownloadTinOptions,
): Promise<DecryptedTin> {
  const normalizedId = requireTinId(tinId);
  if (!(roomKey instanceof Uint8Array) || roomKey.length !== 32) {
    throw new CellarTransferError(
      'invalid-room-key',
      `Room key must be 32 bytes (256 bits); received ${roomKey?.length ?? 0}`,
    );
  }

  const baseUrl = (options?.baseUrl ?? CELLAR_PROXY_BASE_PATH).replace(/\/+$/, '');
  const timeoutMs = options?.timeoutMs ?? DEFAULT_OPERATION_TIMEOUT_MS;
  const fetchFn = options?.fetchFn ?? (typeof fetch !== 'undefined' ? fetch : undefined);
  if (!fetchFn) {
    throw new CellarTransferError('network-error', 'fetch is not available in this environment');
  }

  if (options?.signal?.aborted) {
    throw new CellarTransferError('aborted', 'Cellar transfer was aborted by the caller');
  }

  const url = `${baseUrl}/tin/${encodeURIComponent(normalizedId)}`;
  const controller = new AbortController();
  let timedOut = false;
  const onCallerAbort = (): void => controller.abort();
  options?.signal?.addEventListener('abort', onCallerAbort);
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, timeoutMs);

  let bytes = new Uint8Array(0);
  let wrappedKey: Uint8Array | null = null;
  try {
    const response = await fetchFn(url, {
      method: 'GET',
      headers: { Accept: 'application/octet-stream' },
      credentials: 'include',
      signal: controller.signal,
    });
    if (!response.ok) {
      const body = await response.text().catch(() => '');
      throw new CellarTransferError(
        'http-error',
        `Cellar download failed with HTTP ${response.status}: ${describeBody(body)}`,
        { status: response.status },
      );
    }
    bytes = new Uint8Array(await response.arrayBuffer());
    wrappedKey = parseWrappedKeyHeader(response.headers);
  } catch (error) {
    if (error instanceof CellarTransferError) throw error;
    if (timedOut) {
      throw new CellarTransferError(
        'timeout',
        `Cellar download of ${normalizedId} exceeded ${timeoutMs} ms`,
        { cause: error },
      );
    }
    if (options?.signal?.aborted) {
      throw new CellarTransferError('aborted', 'Cellar transfer was aborted by the caller', {
        cause: error,
      });
    }
    throw new CellarTransferError(
      'network-error',
      `Cellar download of ${normalizedId} failed: ${String(error)}`,
      { cause: error },
    );
  } finally {
    clearTimeout(timer);
    options?.signal?.removeEventListener('abort', onCallerAbort);
  }

  try {
    return decodeTin(bytes, {
      roomKey,
      wrappedKey,
      expectedTinId: normalizedId,
    });
  } catch (error) {
    throw new CellarTransferError(
      'decode-failed',
      `Failed to decode tin ${normalizedId}: ${String(error)}`,
      { cause: error },
    );
  }
}

/**
 * Reads the optional `X-Alhalte-Wrapped-Key` header (lowercase hex).
 *
 * @throws {CellarTransferError} `invalid-response` when the header is present
 * but is not hex.
 */
function parseWrappedKeyHeader(headers: Headers): Uint8Array | null {
  const raw =
    headers.get('X-Alhalte-Wrapped-Key') ?? headers.get('x-alhalte-wrapped-key');
  if (!raw) return null;
  const trimmed = raw.trim();
  if (!HEX_PATTERN.test(trimmed) || trimmed.length % 2 !== 0) {
    throw new CellarTransferError(
      'invalid-response',
      'X-Alhalte-Wrapped-Key is not a hex string',
    );
  }
  return hexToBytes(trimmed);
}
