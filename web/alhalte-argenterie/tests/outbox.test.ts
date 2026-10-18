/**
 * Unit tests for the durable offline outbox (Issue #109 / sub-issue #137).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### ローカル保存`
 * - GitHub Issue #109: `オフライン動作`
 * - GitHub Issue #137: `オフラインメッセージ・キャラクターの永続キュー`
 *
 * The shared suite runs twice, once against MemoryOutboxStorage and once
 * against IndexedDbOutboxStorage (via fake-indexeddb), so both adapters have to
 * satisfy the same contract.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { indexedDB as fakeIndexedDb } from 'fake-indexeddb';
import {
  OUTBOX_DB_NAME,
  OUTBOX_DB_VERSION,
  OUTBOX_KEY_PATH,
  OUTBOX_KINDS,
  OUTBOX_STORE_NAME,
  OUTBOX_TTL_MS,
  IndexedDbOutboxStorage,
  MemoryOutboxStorage,
  Outbox,
  createDefaultOutboxStorage,
  createOutbox,
  getOutbox,
  isIndexedDbAvailable,
  isOutboxKind,
  resetOutbox,
  type OutboxEntry,
  type OutboxStorage,
} from '../src/utils/outbox.js';

const BASE_TIME = 1_700_000_000_000;
const DAY_MS = 24 * 60 * 60 * 1000;

let dbCounter = 0;

/** Resolves once a fake IndexedDB delete request settles. */
function deleteFakeDatabase(name: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = fakeIndexedDb.deleteDatabase(name);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error ?? new Error('failed to delete test database'));
    // A still-open connection blocks deletion; the unique test name makes leaking harmless.
    request.onblocked = () => resolve();
  });
}

interface StorageHarness {
  storage: OutboxStorage;
  dispose: () => Promise<void>;
}

const storageCases: Array<{ label: string; create: () => StorageHarness }> = [
  {
    label: 'memory',
    create: () => ({
      storage: new MemoryOutboxStorage(),
      dispose: async () => {},
    }),
  },
  {
    label: 'indexeddb',
    create: () => {
      const dbName = `outbox-contract-${++dbCounter}`;
      const storage = new IndexedDbOutboxStorage(fakeIndexedDb, dbName);
      return {
        storage,
        dispose: async () => {
          storage.close();
          // Let the memoized connection close before deleting.
          await new Promise((resolve) => setTimeout(resolve, 0));
          await deleteFakeDatabase(dbName);
        },
      };
    },
  },
];

describe('Outbox constants and guards (Issue #137)', () => {
  // spec: alhalte-argenterie/outbox 30 日 TTL
  it('exposes a 30-day TTL', () => {
    expect(OUTBOX_TTL_MS).toBe(30 * DAY_MS);
    expect(OUTBOX_TTL_MS).toBe(2_592_000_000);
  });

  // spec: alhalte-argenterie/outbox DB 名・store 名・keyPath の定数化
  it('exposes the documented IndexedDB schema constants', () => {
    expect(OUTBOX_DB_NAME).toBe('alhalte-argenterie-outbox');
    expect(OUTBOX_STORE_NAME).toBe('outbox');
    expect(OUTBOX_KEY_PATH).toBe('id');
    expect(OUTBOX_DB_VERSION).toBe(1);
  });

  // spec: alhalte-argenterie/outbox kind は message / character / state / asset
  it('supports message, character, state, and asset kinds', () => {
    expect(OUTBOX_KINDS).toEqual(['message', 'character', 'state', 'asset']);
    for (const kind of ['message', 'character', 'state', 'asset']) {
      expect(isOutboxKind(kind)).toBe(true);
    }
    expect(isOutboxKind('message ')).toBe(false);
    expect(isOutboxKind('')).toBe(false);
    expect(isOutboxKind(undefined)).toBe(false);
    expect(isOutboxKind(42)).toBe(false);
  });
});

describe('Outbox default storage in jsdom (Issue #137)', () => {
  afterEach(() => {
    resetOutbox();
  });

  // spec: alhalte-argenterie/outbox indexedDB が無い環境はメモリ実装へフォールバック
  it('falls back to memory storage when indexedDB is unavailable', async () => {
    expect(isIndexedDbAvailable()).toBe(false);
    expect(createDefaultOutboxStorage().kind).toBe('memory');

    const outbox = getOutbox();
    expect(outbox.storageKind).toBe('memory');
    expect(outbox.usingMemoryFallback).toBe(true);
    // The singleton is stable until reset.
    expect(getOutbox()).toBe(outbox);

    const entry = await outbox.enqueue({ roomId: 'room-jsdom', kind: 'message', payload: 'hi' });
    expect(entry.createdAt).toBeGreaterThan(0);
    await expect(outbox.list('room-jsdom')).resolves.toHaveLength(1);

    resetOutbox();
    expect(getOutbox()).not.toBe(outbox);
  });

  it('throws a TypeError for malformed entries', async () => {
    const outbox = createOutbox({ storage: new MemoryOutboxStorage() });
    await expect(
      outbox.enqueue({ roomId: '', kind: 'message', payload: null }),
    ).rejects.toBeInstanceOf(TypeError);
    await expect(
      outbox.enqueue({ roomId: 'r', kind: 'bogus' as never, payload: null }),
    ).rejects.toBeInstanceOf(TypeError);
    await expect(
      outbox.enqueue({ roomId: 'r', kind: 'message', payload: null, createdAt: Number.NaN }),
    ).rejects.toBeInstanceOf(TypeError);
    await expect(
      outbox.enqueue({ roomId: 'r', kind: 'message', payload: null, id: '' }),
    ).rejects.toBeInstanceOf(TypeError);
    await expect(outbox.list('r')).resolves.toEqual([]);
  });
});

describe.each(storageCases)('Outbox contract with $label storage (Issue #137)', ({ create }) => {
  let harness: StorageHarness;
  let outbox: Outbox;
  let clock: number;

  beforeEach(() => {
    clock = BASE_TIME;
    harness = create();
    outbox = createOutbox({ storage: harness.storage, now: () => clock });
  });

  afterEach(async () => {
    outbox.close();
    await harness.dispose();
  });

  // spec: alhalte-argenterie/outbox enqueue は id / createdAt を補完して保存する
  it('enqueues an entry, generating id and createdAt when omitted', async () => {
    const payload = { text: 'オフライン中の発言', speaker: 'hero' };
    const stored = await outbox.enqueue({ roomId: 'room-a', kind: 'message', payload });

    expect(stored.id).toBeTruthy();
    expect(typeof stored.id).toBe('string');
    expect(stored.roomId).toBe('room-a');
    expect(stored.kind).toBe('message');
    expect(stored.createdAt).toBe(clock);
    expect(stored.payload).toEqual(payload);

    await expect(outbox.list('room-a')).resolves.toEqual([stored]);
    await expect(outbox.list('room-a', 'message')).resolves.toHaveLength(1);
    await expect(outbox.list('room-a', 'character')).resolves.toEqual([]);
    await expect(outbox.list('room-b')).resolves.toEqual([]);
  });

  // spec: alhalte-argenterie/outbox 呼び出し側 id / createdAt は維持し、id で upsert する
  it('honors caller supplied id and createdAt and upserts by id', async () => {
    await outbox.enqueue({
      id: 'entry-fixed',
      roomId: 'room-a',
      kind: 'message',
      createdAt: clock - 500,
      payload: { revision: 1 },
    });
    const replaced = await outbox.enqueue({
      id: 'entry-fixed',
      roomId: 'room-a',
      kind: 'message',
      createdAt: clock - 500,
      payload: { revision: 2 },
    });

    expect(replaced.id).toBe('entry-fixed');
    const entries = await outbox.list('room-a');
    expect(entries).toHaveLength(1);
    expect(entries[0]?.payload).toEqual({ revision: 2 });
  });

  // spec: alhalte-argenterie/outbox list は createdAt 昇順・room / kind で絞り込む
  it('lists per room and kind in ascending createdAt order', async () => {
    const make = (createdAt: number, roomId: string, kind: OutboxEntry['kind']) =>
      outbox.enqueue({ roomId, kind, createdAt, payload: { createdAt } });

    await make(clock + 30, 'room-a', 'message');
    await make(clock + 10, 'room-a', 'message');
    await make(clock + 20, 'room-a', 'character');
    await make(clock + 15, 'room-b', 'message');

    const allA = await outbox.list('room-a');
    expect(allA.map((entry) => entry.createdAt)).toEqual([clock + 10, clock + 20, clock + 30]);

    const messagesA = await outbox.list('room-a', 'message');
    expect(messagesA.map((entry) => entry.createdAt)).toEqual([clock + 10, clock + 30]);
    expect(messagesA.every((entry) => entry.kind === 'message')).toBe(true);

    const roomB = await outbox.list('room-b');
    expect(roomB.map((entry) => entry.createdAt)).toEqual([clock + 15]);
  });

  // spec: alhalte-argenterie/outbox remove / clear
  it('removes a single entry and ignores unknown ids', async () => {
    const keep = await outbox.enqueue({ roomId: 'room-a', kind: 'message', payload: 'keep' });
    const drop = await outbox.enqueue({ roomId: 'room-a', kind: 'message', payload: 'drop' });

    await outbox.remove(drop.id);
    await expect(outbox.remove('does-not-exist')).resolves.toBeUndefined();

    const remaining = await outbox.list('room-a');
    expect(remaining.map((entry) => entry.id)).toEqual([keep.id]);
  });

  it('clears one room without touching other rooms, then clears everything', async () => {
    await outbox.enqueue({ id: 'a1', roomId: 'room-a', kind: 'message', createdAt: clock, payload: 1 });
    await outbox.enqueue({ id: 'a2', roomId: 'room-a', kind: 'character', createdAt: clock, payload: 2 });
    await outbox.enqueue({ id: 'b1', roomId: 'room-b', kind: 'message', createdAt: clock, payload: 3 });

    await outbox.clear('room-a');
    expect(await outbox.list('room-a')).toEqual([]);
    expect((await outbox.list('room-b')).map((entry) => entry.id)).toEqual(['b1']);

    await outbox.clear();
    expect(await outbox.list('room-b')).toEqual([]);
  });

  // spec: alhalte-argenterie/outbox 30 日を超えたエントリは読み出し時に prune する
  it('prunes entries older than 30 days on read and reports how many were dropped', async () => {
    await outbox.enqueue({
      id: 'expired',
      roomId: 'room-a',
      kind: 'message',
      createdAt: clock - OUTBOX_TTL_MS - 1,
      payload: 'expired',
    });
    await outbox.enqueue({
      id: 'boundary',
      roomId: 'room-a',
      kind: 'message',
      createdAt: clock - OUTBOX_TTL_MS,
      payload: 'boundary',
    });
    await outbox.enqueue({
      id: 'fresh',
      roomId: 'room-a',
      kind: 'message',
      createdAt: clock - 1000,
      payload: 'fresh',
    });

    const listed = await outbox.list('room-a');
    expect(listed.map((entry) => entry.id)).toEqual(['boundary', 'fresh']);
    expect(outbox.lastPrunedCount).toBe(1);

    // No second prune on the next read.
    await outbox.list('room-a');
    expect(outbox.lastPrunedCount).toBe(0);
  });

  it('pruneExpired accepts an explicit clock and returns the removed count', async () => {
    await outbox.enqueue({ id: 'old', roomId: 'room-a', kind: 'message', createdAt: clock - 10, payload: 1 });
    await outbox.enqueue({ id: 'new', roomId: 'room-a', kind: 'message', createdAt: clock, payload: 2 });

    await expect(outbox.pruneExpired(clock + 10)).resolves.toBe(0);
    await expect(outbox.pruneExpired(clock + OUTBOX_TTL_MS + 1)).resolves.toBe(2);
    await expect(outbox.pruneExpired(clock + OUTBOX_TTL_MS + 1)).resolves.toBe(0);
    await expect(outbox.list('room-a')).resolves.toEqual([]);
  });

  // spec: alhalte-argenterie/outbox flush は成功したものだけ削除し、失敗は残す
  it('flushes in createdAt order and deletes only successful entries', async () => {
    await outbox.enqueue({ id: 'm1', roomId: 'room-a', kind: 'message', createdAt: clock + 1, payload: 'm1' });
    await outbox.enqueue({ id: 'm2', roomId: 'room-a', kind: 'message', createdAt: clock + 2, payload: 'm2' });
    await outbox.enqueue({ id: 'm3', roomId: 'room-a', kind: 'message', createdAt: clock + 3, payload: 'm3' });
    await outbox.enqueue({ id: 'other', roomId: 'room-b', kind: 'message', createdAt: clock + 4, payload: 'other' });

    const seen: string[] = [];
    const result = await outbox.flush('room-a', async (entry) => {
      seen.push(entry.id);
      if (entry.id === 'm2') throw new Error('server rejected');
      if (entry.id === 'm3') return false;
      return true;
    });

    expect(seen).toEqual(['m1', 'm2', 'm3']);
    expect(result).toEqual({ attempted: 3, succeeded: 1, failed: 2, expired: 0 });
    expect((await outbox.list('room-a')).map((entry) => entry.id)).toEqual(['m2', 'm3']);
    // Other rooms are untouched.
    expect((await outbox.list('room-b')).map((entry) => entry.id)).toEqual(['other']);
  });

  it('retries failed entries on the next flush and stops when the queue drains', async () => {
    await outbox.enqueue({ id: 'm1', roomId: 'room-a', kind: 'message', createdAt: clock + 1, payload: 'm1' });
    await outbox.enqueue({ id: 'm2', roomId: 'room-a', kind: 'character', createdAt: clock + 2, payload: 'm2' });

    const first = await outbox.flush('room-a', (entry) => (entry.id === 'm1' ? false : undefined));
    expect(first).toEqual({ attempted: 2, succeeded: 1, failed: 1, expired: 0 });

    const delivered: string[] = [];
    const second = await outbox.flush('room-a', (entry) => {
      delivered.push(entry.id);
    });
    expect(delivered).toEqual(['m1']);
    expect(second).toEqual({ attempted: 1, succeeded: 1, failed: 0, expired: 0 });

    const third = await outbox.flush('room-a', () => {
      throw new Error('must not be called');
    });
    expect(third).toEqual({ attempted: 0, succeeded: 0, failed: 0, expired: 0 });
  });

  // spec: alhalte-argenterie/outbox 期限切れは flush 対象にならない
  it('never offers expired entries to the flush handler', async () => {
    await outbox.enqueue({
      id: 'stale',
      roomId: 'room-a',
      kind: 'message',
      createdAt: clock - OUTBOX_TTL_MS - 1,
      payload: 'stale',
    });
    await outbox.enqueue({
      id: 'fresh',
      roomId: 'room-a',
      kind: 'message',
      createdAt: clock - OUTBOX_TTL_MS,
      payload: 'fresh',
    });

    const delivered: string[] = [];
    const result = await outbox.flush('room-a', (entry) => {
      delivered.push(entry.id);
    });

    expect(delivered).toEqual(['fresh']);
    expect(result).toEqual({ attempted: 1, succeeded: 1, failed: 0, expired: 1 });
    expect(outbox.lastPrunedCount).toBe(1);
    await expect(outbox.list('room-a')).resolves.toEqual([]);
  });

  it('keeps flushing later entries when the handler throws for an early one', async () => {
    for (const id of ['m1', 'm2', 'm3']) {
      await outbox.enqueue({
        id,
        roomId: 'room-a',
        kind: 'message',
        createdAt: clock + Number(id.slice(1)),
        payload: id,
      });
    }

    const result = await outbox.flush('room-a', (entry) => {
      if (entry.id === 'm1') throw new Error('boom');
    });

    expect(result).toEqual({ attempted: 3, succeeded: 2, failed: 1, expired: 0 });
    expect((await outbox.list('room-a')).map((entry) => entry.id)).toEqual(['m1']);
  });

  it('shares queued entries between outboxes using the same storage', async () => {
    const writer = createOutbox({ storage: harness.storage, now: () => clock });
    const reader = createOutbox({ storage: harness.storage, now: () => clock + 1 });

    await writer.enqueue({ id: 'shared', roomId: 'room-a', kind: 'message', payload: 'x' });
    const entries = await reader.list('room-a');
    expect(entries.map((entry) => entry.id)).toEqual(['shared']);
    expect(entries[0]?.payload).toBe('x');
  });
});

describe('IndexedDbOutboxStorage specifics (Issue #137)', () => {
  const usedDatabases: string[] = [];

  afterEach(async () => {
    while (usedDatabases.length > 0) {
      const name = usedDatabases.pop()!;
      await new Promise((resolve) => setTimeout(resolve, 0));
      await deleteFakeDatabase(name);
    }
  });

  function uniqueDbName(prefix: string): string {
    const name = `${prefix}-${++dbCounter}`;
    usedDatabases.push(name);
    return name;
  }

  // spec: alhalte-argenterie/outbox IndexedDB 実装は DB 名 / store 名 / keyPath を使う
  it('creates the outbox object store with the id key path', async () => {
    const dbName = uniqueDbName('outbox-schema');
    const storage = new IndexedDbOutboxStorage(fakeIndexedDb, dbName);
    try {
      await storage.put({
        id: 'schema-entry',
        roomId: 'room-a',
        kind: 'message',
        createdAt: BASE_TIME,
        payload: null,
      });

      const db = await new Promise<IDBDatabase>((resolve, reject) => {
        const request = fakeIndexedDb.open(dbName, OUTBOX_DB_VERSION);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      try {
        expect(db.objectStoreNames.contains(OUTBOX_STORE_NAME)).toBe(true);
        const store = db.transaction(OUTBOX_STORE_NAME, 'readonly').objectStore(OUTBOX_STORE_NAME);
        expect(store.keyPath).toBe(OUTBOX_KEY_PATH);
        expect(store.indexNames.contains('roomId')).toBe(true);
      } finally {
        db.close();
      }
    } finally {
      storage.close();
    }
  });

  // spec: alhalte-argenterie/outbox IndexedDB はリロード（再接続）をまたいで永続する
  it('persists entries across closed and reopened connections', async () => {
    const dbName = uniqueDbName('outbox-persist');

    const first = new IndexedDbOutboxStorage(fakeIndexedDb, dbName);
    const firstOutbox = createOutbox({ storage: first, now: () => BASE_TIME });
    await firstOutbox.enqueue({ id: 'persisted', roomId: 'room-a', kind: 'character', payload: { hp: 7 } });
    firstOutbox.close();
    await new Promise((resolve) => setTimeout(resolve, 0));

    const second = new IndexedDbOutboxStorage(fakeIndexedDb, dbName);
    const secondOutbox = createOutbox({ storage: second, now: () => BASE_TIME });
    try {
      const entries = await secondOutbox.list('room-a');
      expect(entries).toHaveLength(1);
      expect(entries[0]?.payload).toEqual({ hp: 7 });
    } finally {
      secondOutbox.close();
    }
  });

  // spec: alhalte-argenterie/outbox 複数タブからの同時書き込みを壊さない
  it('merges writes from two independent connections (multi-tab simulation)', async () => {
    const dbName = uniqueDbName('outbox-multitab');
    const tabA = new IndexedDbOutboxStorage(fakeIndexedDb, dbName);
    const tabB = new IndexedDbOutboxStorage(fakeIndexedDb, dbName);
    const outboxA = createOutbox({ storage: tabA, now: () => BASE_TIME });
    const outboxB = createOutbox({ storage: tabB, now: () => BASE_TIME });

    try {
      await outboxA.enqueue({ id: 'from-a', roomId: 'room-a', kind: 'message', payload: 'a' });
      await outboxB.enqueue({ id: 'from-b', roomId: 'room-a', kind: 'message', payload: 'b' });
      await outboxB.enqueue({ id: 'from-b-other-room', roomId: 'room-b', kind: 'message', payload: 'b2' });

      const fromA = await outboxA.list('room-a');
      const fromB = await outboxB.list('room-a');
      expect(fromA.map((entry) => entry.id).sort()).toEqual(['from-a', 'from-b']);
      expect(fromB.map((entry) => entry.id).sort()).toEqual(['from-a', 'from-b']);

      // Deletes from one tab are visible to the other.
      await outboxA.remove('from-b');
      expect((await outboxB.list('room-a')).map((entry) => entry.id)).toEqual(['from-a']);
      expect(await outboxB.list('room-b')).toHaveLength(1);
    } finally {
      outboxA.close();
      outboxB.close();
    }
  });
});

describe('IndexedDB capability detection (Issue #137)', () => {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'indexedDB');

  afterEach(() => {
    if (descriptor) {
      Object.defineProperty(globalThis, 'indexedDB', descriptor);
    } else {
      delete (globalThis as { indexedDB?: IDBFactory }).indexedDB;
    }
    resetOutbox();
  });

  // spec: alhalte-argenterie/outbox indexedDB が有ると IndexedDB 実装を選ぶ
  it('selects IndexedDB storage when indexedDB exists and falls back again when it disappears', async () => {
    expect(isIndexedDbAvailable()).toBe(false);

    Object.defineProperty(globalThis, 'indexedDB', {
      configurable: true,
      writable: true,
      value: fakeIndexedDb,
    });
    expect(isIndexedDbAvailable()).toBe(true);

    const storage = createDefaultOutboxStorage();
    expect(storage.kind).toBe('indexeddb');
    storage.close();

    const outbox = getOutbox();
    expect(outbox.storageKind).toBe('indexeddb');
    expect(outbox.usingMemoryFallback).toBe(false);
    // Use a separate, isolated database for the round-trip so the default DB is not polluted.
    outbox.close();
    const dbName = `outbox-default-${++dbCounter}`;
    const isolated = createOutbox({ storage: new IndexedDbOutboxStorage(fakeIndexedDb, dbName) });
    await isolated.enqueue({ roomId: 'room-a', kind: 'message', payload: 1 });
    expect(await isolated.list('room-a')).toHaveLength(1);
    isolated.close();
    resetOutbox();
    await new Promise((resolve) => setTimeout(resolve, 0));
    await deleteFakeDatabase(dbName);

    delete (globalThis as { indexedDB?: IDBFactory }).indexedDB;
    expect(isIndexedDbAvailable()).toBe(false);
    expect(createDefaultOutboxStorage().kind).toBe('memory');
  });
});
