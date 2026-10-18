/**
 * Tests for the authenticated cellar transfer utilities (Issue #109 / #138).
 *
 * No external network is used: uploads run against an injected scripted
 * `XMLHttpRequest`, downloads against an injected `fetch`.
 */

import { describe, expect, it, vi } from 'vitest';
import {
  CELLAR_PROXY_BASE_PATH,
  CELLAR_STATUS_ALREADY_OWNED,
  CELLAR_STATUS_DEDUPLICATED,
  CELLAR_STATUS_UPLOAD_REQUIRED,
  CellarTransferError,
  DEFAULT_OPERATION_TIMEOUT_MS,
  downloadTin,
  encodeRoomTin,
  getRoomKeyFromLocation,
  isCellarTransferError,
  uploadTin,
  type CellarXhr,
  type CellarXhrFactory,
  type TinEncoder,
} from '../src/utils/cellarTransfer.js';
import {
  Compression,
  Encryption,
  bytesToHex,
  computeBlake3Hash,
  decodeTin,
  encodeTin,
  hexToBytes,
} from '@alhalte/pantry';

const ROOM_KEY_HEX =
  '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';

const ROOM_KEY = hexToBytes(ROOM_KEY_HEX);

/** Scripted response for one fake XHR. */
interface XhrStep {
  /** Final HTTP status. */
  status: number;
  /** Response text body. */
  responseText?: string;
  /** `[loaded, total]` upload progress events emitted before completion. */
  progress?: Array<[number, number]>;
  /** Never complete (lets the timeout / abort path run). */
  silent?: boolean;
  /** Fail the transport with `onerror`. */
  networkError?: boolean;
}

/** Fake `XMLHttpRequest` used to drive {@link uploadTin}. */
class ScriptedXhr implements CellarXhr {
  method = '';
  url = '';
  headers: Record<string, string> = {};
  body: Uint8Array | string | null = null;
  status = 0;
  responseText = '';
  upload: { onprogress: ((event: ProgressEvent) => void) | null } = { onprogress: null };
  onload: ((event: ProgressEvent) => void) | null = null;
  onerror: ((event: ProgressEvent) => void) | null = null;
  onabort: ((event: ProgressEvent) => void) | null = null;
  aborted = false;

  constructor(private readonly step: XhrStep) {}

  open(method: string, url: string): void {
    this.method = method;
    this.url = url;
  }

  setRequestHeader(name: string, value: string): void {
    this.headers[name] = value;
  }

  send(body?: Uint8Array | string | null): void {
    this.body = body ?? null;
    if (this.step.silent) return;
    setTimeout(() => {
      if (this.aborted) return;
      if (this.step.networkError) {
        this.onerror?.(new ProgressEvent('error'));
        return;
      }
      for (const [loaded, total] of this.step.progress ?? []) {
        this.upload.onprogress?.(new ProgressEvent('progress', { loaded, total }));
      }
      this.status = this.step.status;
      this.responseText = this.step.responseText ?? '';
      this.onload?.(new ProgressEvent('load'));
    }, 0);
  }

  abort(): void {
    if (this.aborted) return;
    this.aborted = true;
    this.onabort?.(new ProgressEvent('abort'));
  }
}

/** Builds a factory that hands out {@link ScriptedXhr} instances in order. */
function scriptedFactory(steps: XhrStep[]): {
  factory: CellarXhrFactory;
  xhrs: ScriptedXhr[];
} {
  const xhrs: ScriptedXhr[] = [];
  const factory: CellarXhrFactory = () => {
    const xhr = new ScriptedXhr(steps[xhrs.length] ?? { status: 0 });
    xhrs.push(xhr);
    return xhr;
  };
  return { factory, xhrs };
}

/** Minimal `Response`-shaped object for the download tests. */
function fakeResponse(options: {
  status?: number;
  body?: Uint8Array;
  text?: string;
  wrappedKey?: string;
}): Response {
  const status = options.status ?? 200;
  const body = options.body ?? new Uint8Array(0);
  const headers = new Map<string, string>();
  if (options.wrappedKey !== undefined) {
    headers.set('x-alhalte-wrapped-key', options.wrappedKey);
  }
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: '',
    headers: {
      get: (name: string) => headers.get(name.toLowerCase()) ?? null,
    },
    text: async () => options.text ?? '',
    arrayBuffer: async () =>
      body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength) as ArrayBuffer,
  } as unknown as Response;
}

/** Fetch stub returning one canned response. */
function stubFetch(response: Response): typeof fetch {
  return (async () => response) as unknown as typeof fetch;
}

// spec: alhalte-argenterie/cellar-transfer room key from URL fragment
describe('getRoomKeyFromLocation', () => {
  it('reads the 64-hex room key from the URL fragment', () => {
    const key = getRoomKeyFromLocation({ hash: `#${ROOM_KEY_HEX}` });
    expect(key).not.toBeNull();
    expect(key).toBeInstanceOf(Uint8Array);
    expect(key?.length).toBe(32);
    expect(key && bytesToHex(key)).toBe(ROOM_KEY_HEX);
  });

  it('accepts an uppercase fragment and a hash without the leading #', () => {
    const upper = getRoomKeyFromLocation({ hash: `#${ROOM_KEY_HEX.toUpperCase()}` });
    expect(upper && bytesToHex(upper)).toBe(ROOM_KEY_HEX);
    const bare = getRoomKeyFromLocation({ hash: ROOM_KEY_HEX });
    expect(bare && bytesToHex(bare)).toBe(ROOM_KEY_HEX);
  });

  it('returns null when the fragment is missing or malformed', () => {
    expect(getRoomKeyFromLocation({ hash: '' })).toBeNull();
    expect(getRoomKeyFromLocation({ hash: '#' })).toBeNull();
    expect(getRoomKeyFromLocation({ hash: `#${ROOM_KEY_HEX.slice(0, 62)}` })).toBeNull();
    expect(getRoomKeyFromLocation({ hash: '#not-a-key' })).toBeNull();
    expect(getRoomKeyFromLocation(null)).toBeNull();
  });

  it('defaults to the current window.location', () => {
    window.history.replaceState(null, '', `/rooms/0102030405060708#${ROOM_KEY_HEX}`);
    const key = getRoomKeyFromLocation();
    expect(key && bytesToHex(key)).toBe(ROOM_KEY_HEX);
    window.history.replaceState(null, '', '/');
  });
});

// spec: alhalte-argenterie/cellar-transfer encodeRoomTin (RoomKey + Lz4)
describe('encodeRoomTin', () => {
  it('encodes with Encryption.RoomKey and Compression.Lz4 by default', async () => {
    const calls: Array<{
      label: bigint;
      payload: Uint8Array;
      encryption: Encryption;
      compression: Compression;
      roomKey: unknown;
    }> = [];
    const encoder: TinEncoder = async (tin, encryption, compression, options) => {
      calls.push({
        label: tin.label,
        payload: tin.payload,
        encryption,
        compression,
        roomKey: options?.roomKey,
      });
      return {
        wireBytes: new Uint8Array([1, 2, 3]),
        tinId: new Uint8Array(32),
        tinIdHex: 'AA'.repeat(32),
      };
    };

    const payload = new Uint8Array([9, 9, 9]);
    const result = await encodeRoomTin(payload, 42n, ROOM_KEY, { encoder });

    expect(calls).toHaveLength(1);
    expect(calls[0].encryption).toBe(Encryption.RoomKey);
    expect(calls[0].compression).toBe(Compression.Lz4);
    expect(calls[0].roomKey).toBe(ROOM_KEY);
    expect(calls[0].label).toBe(42n);
    expect(Array.from(calls[0].payload)).toEqual([9, 9, 9]);
    expect(Array.from(result.wireBytes)).toEqual([1, 2, 3]);
    expect(result.tinIdHex).toBe('aa'.repeat(32));
  });

  it('roundtrips through pantry encodeTin / decodeTin with the room key', async () => {
    const payload = new TextEncoder().encode('room asset payload');
    const encoded = await encodeRoomTin(payload, 7n, ROOM_KEY);

    expect(encoded.tinIdHex).toMatch(/^[0-9a-f]{64}$/);
    expect(bytesToHex(computeBlake3Hash(encoded.wireBytes))).toBe(encoded.tinIdHex);

    const decoded = decodeTin(encoded.wireBytes, {
      roomKey: ROOM_KEY,
      expectedTinId: encoded.tinIdHex,
    });
    expect(decoded.label).toBe(7n);
    expect(new TextDecoder().decode(decoded.payload)).toBe('room asset payload');
    expect(decoded.opener.encryption).toBe(Encryption.RoomKey);
    expect(decoded.opener.compression).toBe(Compression.Lz4);
  });

  it('rejects a room key that is not 32 bytes', async () => {
    await expect(
      encodeRoomTin(new Uint8Array([1]), 1n, new Uint8Array(16)),
    ).rejects.toMatchObject({ code: 'invalid-room-key' });
  });

  it('wraps encoder failures as CellarTransferError', async () => {
    const encoder: TinEncoder = async () => {
      throw new Error('boom');
    };
    await expect(
      encodeRoomTin(new Uint8Array([1]), 1n, ROOM_KEY, { encoder }),
    ).rejects.toMatchObject({ code: 'encode-failed' });
  });
});

// spec: alhalte-argenterie/cellar-transfer uploadTin (210 → PUT, 211/212 skip)
describe('uploadTin', () => {
  it('POSTs the handshake then PUTs the body on 210', async () => {
    const { factory, xhrs } = scriptedFactory([
      { status: CELLAR_STATUS_UPLOAD_REQUIRED, responseText: '{"code":210}' },
      { status: 201, progress: [[0, 4], [2, 4], [4, 4]] },
    ]);
    const progress: number[] = [];
    const wireBytes = new Uint8Array([1, 2, 3, 4]);

    const result = await uploadTin(wireBytes, {
      xhrFactory: factory,
      onProgress: (p) => progress.push(p),
    });

    expect(xhrs).toHaveLength(2);
    // Step 1: metadata handshake.
    expect(xhrs[0].method).toBe('POST');
    expect(xhrs[0].url).toBe(`${CELLAR_PROXY_BASE_PATH}/store`);
    const handshake = JSON.parse(xhrs[0].body as string);
    expect(handshake).toMatchObject({ owner: 0, session_id: 0, size: 4 });
    expect(handshake.tin_id).toBe(result.tinId);
    expect(handshake.wrapped_key).toBeNull();
    // Step 2: body upload.
    expect(xhrs[1].method).toBe('PUT');
    expect(xhrs[1].url).toBe(`${CELLAR_PROXY_BASE_PATH}/store/${result.tinId}`);
    expect(Array.from(xhrs[1].body as Uint8Array)).toEqual([1, 2, 3, 4]);
    expect(xhrs[1].headers['Content-Type']).toBe('application/octet-stream');
    // Progress is monotonic, bounded, and ends at 1.
    expect(progress[progress.length - 1]).toBe(1);
    expect(progress).toContain(0.5);
    expect(progress.every((p) => p >= 0 && p <= 1)).toBe(true);
  });

  it('skips the PUT for 211 and 212', async () => {
    for (const status of [CELLAR_STATUS_DEDUPLICATED, CELLAR_STATUS_ALREADY_OWNED]) {
      const { factory, xhrs } = scriptedFactory([{ status, responseText: '{}' }]);
      const progress: number[] = [];
      const result = await uploadTin(new Uint8Array([1, 2]), {
        xhrFactory: factory,
        onProgress: (p) => progress.push(p),
      });
      expect(xhrs).toHaveLength(1);
      expect(result.tinId).toMatch(/^[0-9a-f]{64}$/);
      expect(progress).toEqual([1]);
    }
  });

  it('derives the tin id from the wire bytes with BLAKE3', async () => {
    const { factory } = scriptedFactory([{ status: CELLAR_STATUS_UPLOAD_REQUIRED }, { status: 201 }]);
    const wireBytes = new Uint8Array([5, 6, 7]);
    const result = await uploadTin(wireBytes, { xhrFactory: factory });
    expect(result.tinId).toBe(bytesToHex(computeBlake3Hash(wireBytes)));
  });

  it('accepts a pre-computed tin id and a wrapped key', async () => {
    const { factory, xhrs } = scriptedFactory([
      { status: CELLAR_STATUS_UPLOAD_REQUIRED },
      { status: 201 },
    ]);
    const tinId = 'ab'.repeat(32);
    const wrappedKey = new Uint8Array([0xde, 0xad, 0xbe, 0xef]);

    const result = await uploadTin(new Uint8Array([1]), {
      xhrFactory: factory,
      tinId: tinId.toUpperCase(),
      wrappedKey,
    });

    expect(result.tinId).toBe(tinId);
    const handshake = JSON.parse(xhrs[0].body as string);
    expect(handshake.tin_id).toBe(tinId);
    expect(handshake.wrapped_key).toBe('deadbeef');
    expect(xhrs[1].headers['X-Alhalte-Wrapped-Key']).toBe('deadbeef');
  });

  it('rejects a malformed pre-computed tin id', async () => {
    const { factory } = scriptedFactory([{ status: CELLAR_STATUS_UPLOAD_REQUIRED }]);
    await expect(
      uploadTin(new Uint8Array([1]), { xhrFactory: factory, tinId: 'nope' }),
    ).rejects.toMatchObject({ code: 'invalid-tin-id' });
  });

  it('surfaces handshake HTTP errors', async () => {
    const { factory, xhrs } = scriptedFactory([
      { status: 401, responseText: '{"error":"invalid session or account"}' },
    ]);
    await expect(uploadTin(new Uint8Array([1]), { xhrFactory: factory })).rejects.toMatchObject({
      code: 'http-error',
      status: 401,
    });
    expect(xhrs).toHaveLength(1);
  });

  it('surfaces upload HTTP errors', async () => {
    const { factory } = scriptedFactory([
      { status: CELLAR_STATUS_UPLOAD_REQUIRED },
      { status: 502, responseText: '{"error":"cellar upstream request failed"}' },
    ]);
    await expect(uploadTin(new Uint8Array([1]), { xhrFactory: factory })).rejects.toMatchObject({
      code: 'http-error',
      status: 502,
    });
  });

  it('times out and aborts the request', async () => {
    vi.useFakeTimers();
    try {
      const { factory, xhrs } = scriptedFactory([{ status: 0, silent: true }]);
      const promise = uploadTin(new Uint8Array([1]), {
        xhrFactory: factory,
        timeoutMs: 50,
      });
      const assertion = expect(promise).rejects.toMatchObject({ code: 'timeout' });
      await vi.advanceTimersByTimeAsync(60);
      await assertion;
      expect(xhrs[0].aborted).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('honours an AbortSignal', async () => {
    const { factory, xhrs } = scriptedFactory([{ status: 0, silent: true }]);
    const controller = new AbortController();
    const promise = uploadTin(new Uint8Array([1]), {
      xhrFactory: factory,
      signal: controller.signal,
    });
    const assertion = expect(promise).rejects.toMatchObject({ code: 'aborted' });
    controller.abort();
    await assertion;
    expect(xhrs[0].aborted).toBe(true);
  });

  it('rejects immediately when the signal is already aborted', async () => {
    const { factory, xhrs } = scriptedFactory([{ status: CELLAR_STATUS_UPLOAD_REQUIRED }]);
    const controller = new AbortController();
    controller.abort();
    await expect(
      uploadTin(new Uint8Array([1]), { xhrFactory: factory, signal: controller.signal }),
    ).rejects.toMatchObject({ code: 'aborted' });
    expect(xhrs).toHaveLength(0);
  });

  it('maps transport failures to network-error', async () => {
    const { factory } = scriptedFactory([{ status: 0, networkError: true }]);
    await expect(uploadTin(new Uint8Array([1]), { xhrFactory: factory })).rejects.toMatchObject({
      code: 'network-error',
    });
  });

  it('rejects empty wire bytes', async () => {
    const { factory } = scriptedFactory([{ status: CELLAR_STATUS_UPLOAD_REQUIRED }]);
    await expect(
      uploadTin(new Uint8Array(0), { xhrFactory: factory }),
    ).rejects.toMatchObject({ code: 'invalid-argument' });
  });
});

// spec: alhalte-argenterie/cellar-transfer downloadTin
describe('downloadTin', () => {
  it('downloads and decrypts a room tin', async () => {
    const payload = new TextEncoder().encode('wasm-extension-bytes');
    const encoded = await encodeRoomTin(payload, 11n, ROOM_KEY);
    const tinId = encoded.tinIdHex;

    const decoded = await downloadTin(tinId, ROOM_KEY, {
      fetchFn: stubFetch(fakeResponse({ body: encoded.wireBytes })),
    });

    expect(decoded.tinId).toBe(tinId);
    expect(decoded.label).toBe(11n);
    expect(new TextDecoder().decode(decoded.payload)).toBe('wasm-extension-bytes');
  });

  it('requests the proxy download route with the tin id', async () => {
    const encoded = await encodeRoomTin(new Uint8Array([1, 2, 3]), 1n, ROOM_KEY);
    const fetchFn = vi.fn(async () => fakeResponse({ body: encoded.wireBytes })) as unknown as typeof fetch;

    await downloadTin(encoded.tinIdHex.toUpperCase(), ROOM_KEY, { fetchFn });

    expect(fetchFn).toHaveBeenCalledTimes(1);
    const [url, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe(`${CELLAR_PROXY_BASE_PATH}/tin/${encoded.tinIdHex}`);
    expect(init.method).toBe('GET');
    expect(init.credentials).toBe('include');
  });

  it('rejects an invalid tin id before fetching', async () => {
    const fetchFn = vi.fn() as unknown as typeof fetch;
    await expect(downloadTin('xyz', ROOM_KEY, { fetchFn })).rejects.toMatchObject({
      code: 'invalid-tin-id',
    });
    expect(fetchFn).not.toHaveBeenCalled();
  });

  it('rejects a room key that is not 32 bytes', async () => {
    await expect(
      downloadTin('ab'.repeat(32), new Uint8Array(4), { fetchFn: stubFetch(fakeResponse({})) }),
    ).rejects.toMatchObject({ code: 'invalid-room-key' });
  });

  it('surfaces HTTP errors with the status', async () => {
    await expect(
      downloadTin('ab'.repeat(32), ROOM_KEY, {
        fetchFn: stubFetch(fakeResponse({ status: 403, text: '{"error":"forbidden"}' })),
      }),
    ).rejects.toMatchObject({ code: 'http-error', status: 403 });
  });

  it('maps transport failures to network-error', async () => {
    const fetchFn = (async () => {
      throw new TypeError('network down');
    }) as unknown as typeof fetch;
    await expect(downloadTin('ab'.repeat(32), ROOM_KEY, { fetchFn })).rejects.toMatchObject({
      code: 'network-error',
    });
  });

  it('times out an unresponsive download', async () => {
    vi.useFakeTimers();
    try {
      const fetchFn = ((_url: string, init?: RequestInit) =>
        new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener('abort', () =>
            reject(new DOMException('aborted', 'AbortError')),
          );
        })) as unknown as typeof fetch;

      const promise = downloadTin('ab'.repeat(32), ROOM_KEY, { fetchFn, timeoutMs: 50 });
      const assertion = expect(promise).rejects.toMatchObject({ code: 'timeout' });
      await vi.advanceTimersByTimeAsync(60);
      await assertion;
    } finally {
      vi.useRealTimers();
    }
  });

  it('honours an AbortSignal', async () => {
    const controller = new AbortController();
    const fetchFn = ((_url: string, init?: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener('abort', () =>
          reject(new DOMException('aborted', 'AbortError')),
        );
      })) as unknown as typeof fetch;

    const promise = downloadTin('ab'.repeat(32), ROOM_KEY, {
      fetchFn,
      signal: controller.signal,
    });
    const assertion = expect(promise).rejects.toMatchObject({ code: 'aborted' });
    controller.abort();
    await assertion;
  });

  it('rejects a non-hex wrapped key header', async () => {
    const encoded = await encodeRoomTin(new Uint8Array([1, 2, 3]), 1n, ROOM_KEY);
    await expect(
      downloadTin(encoded.tinIdHex, ROOM_KEY, {
        fetchFn: stubFetch(
          fakeResponse({ body: encoded.wireBytes, wrappedKey: 'not-hex!' }),
        ),
      }),
    ).rejects.toMatchObject({ code: 'invalid-response' });
  });

  it('fails with decode-failed when the room key is wrong', async () => {
    const encoded = await encodeRoomTin(new Uint8Array([1, 2, 3]), 1n, ROOM_KEY);
    const wrongKey = new Uint8Array(32).fill(7);
    await expect(
      downloadTin(encoded.tinIdHex, wrongKey, {
        fetchFn: stubFetch(fakeResponse({ body: encoded.wireBytes })),
      }),
    ).rejects.toMatchObject({ code: 'decode-failed' });
  });
});

// spec: alhalte-argenterie/cellar-transfer typed errors
describe('CellarTransferError', () => {
  it('exposes a code, an optional status, and a name', () => {
    const error = new CellarTransferError('http-error', 'boom', { status: 502 });
    expect(error).toBeInstanceOf(Error);
    expect(error.name).toBe('CellarTransferError');
    expect(error.code).toBe('http-error');
    expect(error.status).toBe(502);
    expect(error.message).toBe('boom');
    expect(isCellarTransferError(error)).toBe(true);
    expect(isCellarTransferError(new Error('boom'))).toBe(false);
  });

  it('keeps the configured default timeout at 8000 ms', () => {
    expect(DEFAULT_OPERATION_TIMEOUT_MS).toBe(8000);
  });

  it('preserves the underlying cause', () => {
    const cause = new Error('root cause');
    const error = new CellarTransferError('encode-failed', 'wrapped', { cause });
    expect((error as { cause?: unknown }).cause).toBe(cause);
  });
});

// Sanity check that the real pantry encoder is compatible with the defaults
// used above (guards against a silent Compression / Encryption mismatch).
it('uses the pantry Lz4 + RoomKey defaults end to end', async () => {
  const encoded = await encodeTin(
    { label: 3n, payload: new Uint8Array([1, 2, 3, 4]) },
    Encryption.RoomKey,
    Compression.Lz4,
    { roomKey: ROOM_KEY },
  );
  const viaHelper = await encodeRoomTin(new Uint8Array([1, 2, 3, 4]), 3n, ROOM_KEY);
  expect(viaHelper.wireBytes.length).toBeGreaterThan(0);
  expect(encoded.tinIdHex).not.toBe(viaHelper.tinIdHex); // different nonces
});
