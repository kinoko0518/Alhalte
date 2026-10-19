/**
 * Tests for rendering durable offline messages inline in the log (Issue #148).
 *
 * The queue lives in `App`'s outbox; `LogTab` receives the room's
 * `kind: 'message'` entries and renders the current scene's records below a
 * wavy divider, through the same Saignant / `contentTin` path as server-echoed
 * messages.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`
 * - GitHub Issue #148: `「サーバーと通信できませんでした」UIがイメージと異なる`
 */
import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import LogTab from '../src/components/Control/tabs/LogTab.svelte';
import { buildContentTinFromText } from '../src/utils/generast.js';
import { ChunkLogManager } from '../src/utils/log.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import type { LogRecord, QueuedLogMessage, Scene } from '../src/types/index.js';

async function waitFor(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

function createScene(id: string, name = id): Scene {
  return {
    id,
    name,
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
  };
}

function createRecord(overrides: Partial<LogRecord> & { id: string; sceneId: string }): LogRecord {
  return {
    content: '',
    speaker: 'アルハルテ太郎',
    timestamp: 1000,
    kind: 'chat',
    ...overrides,
  };
}

describe('Queued log messages (Issue #148)', () => {
  let target: HTMLDivElement;

  beforeEach(() => {
    localStorage.clear();
    document.body.innerHTML = '';
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  // spec: alhalte-argenterie/発言とログ表示 キュー済みメッセージを波線区切り付きでログ内に描画する
  it('renders only the current scene queue below the wavy divider with the canonical copy and count', async () => {
    const scene = createScene('/');
    const contentTin = await buildContentTinFromText('contentTin の本文');
    expect(contentTin).not.toBeNull();

    const queuedMessages: QueuedLogMessage[] = [
      {
        entryId: 'outbox-1',
        sceneId: '/',
        record: createRecord({
          id: 'queued-current',
          sceneId: '/',
          // The raw fallback differs from the persisted `contentTin`, so the
          // assertion proves the queued row renders through the same path as a
          // normal message (Issue #161 / #148).
          content: '生のフォールバック本文',
          contentTin: Array.from(contentTin!),
        }),
      },
      {
        entryId: 'outbox-2',
        sceneId: '/other',
        record: createRecord({
          id: 'queued-other',
          sceneId: '/other',
          content: '別シーンのキュー本文',
        }),
      },
    ];

    const instance = mount(LogTab, {
      target,
      props: {
        scene,
        chunkManager: new ChunkLogManager(),
        queuedMessages,
      },
    });

    await waitFor(() => {
      const content = target.querySelector('[data-testid="msg-content-queued-current"]');
      return content?.textContent?.includes('contentTin の本文') ?? false;
    });

    // The divider and the notice live inside the scrollable message list, after
    // the loaded log (here: no loaded records).
    const messagesList = target.querySelector<HTMLElement>('[data-testid="messages-list"]');
    expect(messagesList).toBeTruthy();
    const divider = messagesList?.querySelector<HTMLElement>(
      '[data-testid="queued-log-divider"]',
    );
    expect(divider).toBeTruthy();

    const notice = divider?.querySelector<HTMLElement>('[data-testid="offline-queue-notice"]');
    expect(notice).toBeTruthy();
    expect(notice?.getAttribute('data-kind')).toBe('message');
    expect(notice?.getAttribute('role')).toBe('status');
    expect(notice?.getAttribute('aria-live')).toBe('polite');
    expect(
      notice?.querySelector('[data-testid="offline-queue-notice-title"]')?.textContent?.trim(),
    ).toBe('サーバーと通信できませんでした');
    expect(
      notice?.querySelector('[data-testid="offline-queue-notice-description"]')?.textContent?.trim(),
    ).toBe('メッセージは次回の接続時に送信されます');
    expect(
      notice?.querySelector('[data-testid="offline-queue-notice-count"]')?.textContent?.trim(),
    ).toBe('1 件を蓄積中');
    // Two CSS wavy lines frame the notice (no image assets).
    expect(notice?.querySelectorAll('.wavy-divider').length).toBe(2);

    // Only the current scene's queued record is rendered, and it is marked as
    // queued. The record text comes from `contentTin`, not `content`.
    expect(target.querySelectorAll('[data-queued="true"]')).toHaveLength(1);
    const queuedRow = target.querySelector<HTMLElement>(
      '[data-testid="log-record-queued-current"]',
    );
    expect(queuedRow?.getAttribute('data-queued')).toBe('true');
    expect(queuedRow?.textContent).toContain('contentTin の本文');
    expect(queuedRow?.textContent).not.toContain('生のフォールバック本文');
    expect(target.querySelector('[data-testid="log-record-queued-other"]')).toBeNull();
    expect(target.textContent).not.toContain('別シーンのキュー本文');

    unmount(instance);
  });

  // spec: alhalte-argenterie/発言とログ表示 flush 後の echo と LogRecord.id で重複排除する
  it('dedupes a queued record by LogRecord.id once the loaded log already contains it', async () => {
    const scene = createScene('/');
    const chunkManager = new ChunkLogManager();
    chunkManager.appendRecord(
      createRecord({
        id: 'echoed-1',
        sceneId: '/',
        content: '読み込み済みの本文',
      }),
    );

    const queuedMessages: QueuedLogMessage[] = [
      {
        entryId: 'outbox-echoed',
        sceneId: '/',
        record: createRecord({
          id: 'echoed-1',
          sceneId: '/',
          content: 'キューの本文',
        }),
      },
    ];

    const instance = mount(LogTab, {
      target,
      props: {
        scene,
        chunkManager,
        queuedMessages,
      },
    });

    await waitFor(() => {
      const content = target.querySelector('[data-testid="msg-content-echoed-1"]');
      return content?.textContent?.includes('読み込み済みの本文') ?? false;
    });

    // Exactly one row: the echo wins and the queued copy is suppressed.
    expect(target.querySelectorAll('[data-testid="log-record-echoed-1"]')).toHaveLength(1);
    expect(target.querySelector('[data-queued="true"]')).toBeNull();
    expect(target.querySelector('[data-testid="queued-log-divider"]')).toBeNull();
    expect(target.querySelector('[data-testid="offline-queue-notice"]')).toBeNull();
    expect(target.textContent).not.toContain('キューの本文');

    unmount(instance);
  });

  it('shows the queue count for several current-scene records', async () => {
    const scene = createScene('/');
    const queuedMessages: QueuedLogMessage[] = [
      {
        entryId: 'outbox-a',
        sceneId: '/',
        record: createRecord({ id: 'queued-a', sceneId: '/', content: 'キューA' }),
      },
      {
        entryId: 'outbox-b',
        sceneId: '/',
        record: createRecord({ id: 'queued-b', sceneId: '/', content: 'キューB' }),
      },
    ];

    const instance = mount(LogTab, {
      target,
      props: {
        scene,
        chunkManager: new ChunkLogManager(),
        queuedMessages,
      },
    });

    await waitFor(() => target.querySelectorAll('[data-queued="true"]').length === 2);

    expect(target.querySelectorAll('[data-queued="true"]')).toHaveLength(2);
    expect(
      target
        .querySelector('[data-testid="offline-queue-notice-count"]')
        ?.textContent?.trim(),
    ).toBe('2 件を蓄積中');
    expect(target.querySelectorAll('[data-testid="queued-log-divider"]')).toHaveLength(1);
    expect(target.textContent).toContain('キューA');
    expect(target.textContent).toContain('キューB');

    unmount(instance);
  });
});
