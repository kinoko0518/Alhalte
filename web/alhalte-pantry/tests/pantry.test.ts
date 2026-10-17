import { init as initZstd } from '@bokuweb/zstd-wasm';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';
import {
  CellarError,
  Compression,
  Encryption,
  InMemoryCellarClient,
  IntegrityError,
  LABEL_PRIMITIVE_STRING,
  MemoryPantryCacheAdapter,
  MissingKeyError,
  MissingWrappedKeyError,
  Pantry,
  bytesToHex,
  encodeTin,
  labelHash,
} from '../src/index.js';

describe('Pantry - Cache and Decryption Manager', () => {
  let cellar: InMemoryCellarClient;
  let cache: MemoryPantryCacheAdapter;
  let pantry: Pantry;

  const roomKey = new Uint8Array(32).fill(0x77);
  const roomKeyHex = bytesToHex(roomKey);

  beforeAll(async () => {
    await initZstd();
  });

  beforeEach(() => {
    cellar = new InMemoryCellarClient();
    cache = new MemoryPantryCacheAdapter();
    pantry = new Pantry({
      cellar,
      cache,
      roomKey,
    });
  });

  // spec: alhalte-pantry/cache-hit
  it('returns immediately from pantry cache when tin is already present', async () => {
    const fakeTinId = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
    const mockDecrypted = {
      tinId: fakeTinId,
      label: LABEL_PRIMITIVE_STRING,
      payload: new TextEncoder().encode('Cached in pantry'),
      opener: { encryption: Encryption.Plaintext, compression: Compression.None },
    };

    // Pre-populate cache
    await cache.set(fakeTinId, mockDecrypted);

    // Call pantry.getTin()
    const result = await pantry.getTin(fakeTinId);

    expect(result.tinId).toBe(fakeTinId);
    expect(result.payload).toEqual(mockDecrypted.payload);
    // Verified: Cellar was NOT requested
    expect(cellar.requestCount).toBe(0);
  });

  // spec: alhalte-pantry/flow 0000 plaintext
  it('fetches, decompresses, caches, and returns 0000 (Plaintext) tin with Zstd compression', async () => {
    const samplePayload = new TextEncoder().encode('Plaintext Zstd compressed Tin content');
    const { wireBytes, tinIdHex } = await encodeTin(
      { label: LABEL_PRIMITIVE_STRING, payload: samplePayload },
      Encryption.Plaintext,
      Compression.Zstd,
    );

    cellar.store(tinIdHex, wireBytes);

    // Initial cache miss -> downloads from cellar -> decodes -> stores in cache
    const firstGet = await pantry.getTin(tinIdHex);
    expect(cellar.requestCount).toBe(1);
    expect(firstGet.tinId).toBe(tinIdHex);
    expect(firstGet.label).toBe(LABEL_PRIMITIVE_STRING);
    expect(new TextDecoder().decode(firstGet.payload)).toBe('Plaintext Zstd compressed Tin content');
    expect(firstGet.opener.encryption).toBe(Encryption.Plaintext);
    expect(firstGet.opener.compression).toBe(Compression.Zstd);

    // Verify it is now saved in cache
    expect(await cache.has(tinIdHex)).toBe(true);

    // Second get: served directly from pantry cache
    const secondGet = await pantry.getTin(tinIdHex);
    expect(cellar.requestCount).toBe(1); // No additional cellar call!
    expect(secondGet.payload).toEqual(firstGet.payload);
  });

  // spec: alhalte-pantry/flow 0010 room-key with padme padding
  it('fetches, decrypts, strips PADMÉ padding, decompresses LZ4, and returns 0010 (RoomKey) tin', async () => {
    const rawText = 'Confidential message in Room 42 padded with ISO/IEC 7816-4 PADMÉ';
    const samplePayload = new TextEncoder().encode(rawText);

    // Encrypt with RoomKey, LZ4 compression, and PADMÉ padding target length
    const { wireBytes, tinIdHex } = await encodeTin(
      { label: labelHash('alhalte/chat-message'), payload: samplePayload },
      Encryption.RoomKey,
      Compression.Lz4,
      {
        roomKey,
        padmeTargetLength: 128, // PADMÉ padded to 128 bytes
      },
    );

    cellar.store(tinIdHex, wireBytes);

    const result = await pantry.getTin(tinIdHex);
    expect(cellar.requestCount).toBe(1);
    expect(result.tinId).toBe(tinIdHex);
    expect(result.label).toBe(labelHash('alhalte/chat-message'));
    expect(new TextDecoder().decode(result.payload)).toBe(rawText);
    expect(result.opener.encryption).toBe(Encryption.RoomKey);
    expect(result.opener.compression).toBe(Compression.Lz4);

    // Cache hit on re-fetch
    const cachedResult = await pantry.getTin(tinIdHex);
    expect(cellar.requestCount).toBe(1);
    expect(cachedResult.payload).toEqual(result.payload);
  });

  // spec: alhalte-pantry/flow 0010 room-key uncompressed with padme padding
  it('decrypts and unpads 0010 RoomKey uncompressed tin (encrypt-low-entropy)', async () => {
    const rawText = 'Low entropy utterance payload';
    const samplePayload = new TextEncoder().encode(rawText);

    const { wireBytes, tinIdHex } = await encodeTin(
      { label: LABEL_PRIMITIVE_STRING, payload: samplePayload },
      Encryption.RoomKey,
      Compression.None,
      {
        roomKey,
        padmeTargetLength: 64,
      },
    );

    cellar.store(tinIdHex, wireBytes);

    const result = await pantry.getTin(tinIdHex);
    expect(new TextDecoder().decode(result.payload)).toBe(rawText);
    expect(result.opener.compression).toBe(Compression.None);
  });

  // spec: alhalte-pantry/flow 0001 convergent with wrapped key
  it('fetches, unwraps key, decrypts zero-nonce tin, verifies BLAKE3 XOF, and returns 0001 (Convergent) tin', async () => {
    const highEntropyData = new Uint8Array(256);
    for (let i = 0; i < highEntropyData.length; i++) {
      highEntropyData[i] = (i * 37 + 13) & 0xff;
    }

    const { wireBytes, tinIdHex, wrappedKey } = await encodeTin(
      { label: labelHash('vendor/image/webp'), payload: highEntropyData },
      Encryption.Convergent,
      Compression.Brotli,
      { roomKey },
    );

    expect(wrappedKey).toBeDefined();
    expect(wrappedKey?.length).toBe(60); // 12B nonce + 48B ciphertext

    // Cellar stores tinBytes and wrappedKey
    cellar.store(tinIdHex, wireBytes, wrappedKey);

    const result = await pantry.getTin(tinIdHex);
    expect(cellar.requestCount).toBe(1);
    expect(result.tinId).toBe(tinIdHex);
    expect(result.label).toBe(labelHash('vendor/image/webp'));
    expect(result.payload).toEqual(highEntropyData);
    expect(result.opener.encryption).toBe(Encryption.Convergent);
    expect(result.opener.compression).toBe(Compression.Brotli);

    // Verify subsequent lookup uses pantry cache
    const second = await pantry.getTin(tinIdHex);
    expect(cellar.requestCount).toBe(1);
    expect(second.payload).toEqual(highEntropyData);
  });

  // spec: alhalte-pantry/deduplication in-flight request deduplication
  it('deduplicates concurrent requests for the same tin-id to cellar', async () => {
    const samplePayload = new TextEncoder().encode('Deduplication test data');
    const { wireBytes, tinIdHex } = await encodeTin(
      { label: LABEL_PRIMITIVE_STRING, payload: samplePayload },
      Encryption.Plaintext,
      Compression.None,
    );

    cellar.store(tinIdHex, wireBytes);

    // Launch 5 concurrent getTin requests for the same tin-id simultaneously
    const results = await Promise.all([
      pantry.getTin(tinIdHex),
      pantry.getTin(tinIdHex),
      pantry.getTin(tinIdHex),
      pantry.getTin(tinIdHex),
      pantry.getTin(tinIdHex),
    ]);

    // Exactly 1 cellar download request should have been made!
    expect(cellar.requestCount).toBe(1);

    // All 5 promises resolved to identical decrypted data
    for (const res of results) {
      expect(res.tinId).toBe(tinIdHex);
      expect(res.payload).toEqual(samplePayload);
    }

    // In-flight tracking map is cleaned up after completion
    expect(pantry.inFlightCount).toBe(0);
  });

  // spec: alhalte-pantry/integrity convergent key mismatch
  it('rejects convergent tin when wrapped key does not match plaintext BLAKE3 XOF', async () => {
    const payload = new TextEncoder().encode('Data with tampered wrapped key');
    const { wireBytes, tinIdHex } = await encodeTin(
      { label: LABEL_PRIMITIVE_STRING, payload },
      Encryption.Convergent,
      Compression.None,
      { roomKey },
    );

    // Create a bogus wrapped key with wrong convergent key
    const bogusTin = await encodeTin(
      { label: LABEL_PRIMITIVE_STRING, payload: new TextEncoder().encode('Different data') },
      Encryption.Convergent,
      Compression.None,
      { roomKey },
    );

    cellar.store(tinIdHex, wireBytes, bogusTin.wrappedKey);

    await expect(pantry.getTin(tinIdHex)).rejects.toThrow();
  });

  // spec: alhalte-pantry/errors missing room key
  it('throws MissingKeyError when room key is absent for RoomKey tin', async () => {
    const pantryNoKey = new Pantry({ cellar, cache });
    const { wireBytes, tinIdHex } = await encodeTin(
      { label: LABEL_PRIMITIVE_STRING, payload: new Uint8Array([1, 2, 3]) },
      Encryption.RoomKey,
      Compression.None,
      { roomKey },
    );

    cellar.store(tinIdHex, wireBytes);

    await expect(pantryNoKey.getTin(tinIdHex)).rejects.toThrow(MissingKeyError);
  });

  // spec: alhalte-pantry/errors missing wrapped key
  it('throws MissingWrappedKeyError when wrapped key is missing for Convergent tin', async () => {
    const { wireBytes, tinIdHex } = await encodeTin(
      { label: LABEL_PRIMITIVE_STRING, payload: new Uint8Array([1, 2, 3]) },
      Encryption.Convergent,
      Compression.None,
      { roomKey },
    );

    // Store tin in cellar WITHOUT wrapped key
    cellar.store(tinIdHex, wireBytes, null);

    await expect(pantry.getTin(tinIdHex)).rejects.toThrow(MissingWrappedKeyError);
  });

  // spec: alhalte-pantry/url-fragment dynamic room key
  it('resolves room key dynamically from URL fragment provider', async () => {
    let currentHash = `#${roomKeyHex}`;
    const dynamicPantry = new Pantry({
      cellar,
      cache,
      roomKey: () => currentHash,
    });

    const { wireBytes, tinIdHex } = await encodeTin(
      { label: LABEL_PRIMITIVE_STRING, payload: new TextEncoder().encode('Dynamic fragment test') },
      Encryption.RoomKey,
      Compression.None,
      { roomKey },
    );

    cellar.store(tinIdHex, wireBytes);

    const res = await dynamicPantry.getTin(tinIdHex);
    expect(new TextDecoder().decode(res.payload)).toBe('Dynamic fragment test');
  });

  // spec: alhalte-pantry/cellar 404 error
  it('propagates CellarError when tin is not found in cellar', async () => {
    const missingTinId = 'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff';
    await expect(pantry.getTin(missingTinId)).rejects.toThrow(CellarError);
  });

  // spec: alhalte-pantry/cache operations put, delete, clear
  it('supports direct putTin, deleteTin, and clearCache', async () => {
    const id = '1111111111111111111111111111111111111111111111111111111111111111';
    const mock = {
      tinId: id,
      label: 100n,
      payload: new Uint8Array([9, 8, 7]),
      opener: { encryption: Encryption.Plaintext, compression: Compression.None },
    };

    await pantry.putTin(mock);
    expect(await pantry.hasTin(id)).toBe(true);

    const retrieved = await pantry.getTin(id);
    expect(retrieved.payload).toEqual(mock.payload);

    await pantry.deleteTin(id);
    expect(await pantry.hasTin(id)).toBe(false);

    await pantry.putTin(mock);
    await pantry.clearCache();
    expect(await pantry.hasTin(id)).toBe(false);
  });
});
