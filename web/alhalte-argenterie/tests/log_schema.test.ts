/**
 * Issue #161: shared log schema, system-event rendering and send-time pipeline.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 発言とログ表示`,
 *   `## alhalte-banquette -> #### log`, `## alhalte-kitchen -> ### イベント解析`
 * - GitHub Issue #161
 */
import fs from 'node:fs';
import path from 'node:path';
import { describe, it, expect, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import type { Character, ExtensionItem, Scene, VisitedRoom } from '../src/types/index.js';
import LogTab from '../src/components/Control/tabs/LogTab.svelte';
import App from '../src/components/App.svelte';
import { ChunkLogManager, contentTinToBytes } from '../src/utils/log.js';
import { encodeExtIoTin, LABEL_ALHALTE_GENERAST } from '../src/utils/generast.js';
import { encodeGenerastAstToTin } from '../src/utils/saignant.js';
import {
  LABEL_BANQUETTE_LOG_CHARACTER_CAME,
  LABEL_BANQUETTE_LOG_CHARACTER_GONE,
  entryToLogRecord,
  logRecordToTin,
  tinToLogRecord,
} from '../src/utils/roomSocket.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';

const markdownWasm = fs.readFileSync(
  path.resolve(__dirname, '../public/extensions/markdown.wasm'),
);

async function waitFor(predicate: () => boolean, timeoutMs = 4000): Promise<void> {
  const startedAt = Date.now();
  while (!predicate()) {
    if (Date.now() - startedAt > timeoutMs) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

function testScene(id: string): Scene {
  return {
    id,
    name: 'ログ schema テスト',
    juices: createDefaultJuices(),
    viewTransform: { x: 0, y: 0, scale: 1 },
    members: [],
  };
}

let systemRecordPosition = 0;

/** Encodes a non-negative integer as a postcard `u64` varint (test helper). */
function postcardVarint(value: number): number[] {
  const bytes: number[] = [];
  let remaining = BigInt(value);
  do {
    const byte = Number(remaining & 0x7fn);
    remaining >>= 7n;
    bytes.push(remaining > 0n ? byte | 0x80 : byte);
  } while (remaining > 0n);
  return bytes;
}

function systemRecord(label: number, payload: number[], id: string) {
  const record = entryToLogRecord('scene-schema', { label, payload }, 0, systemRecordPosition++);
  expect(record).not.toBeNull();
  return { ...record!, id };
}

function markdownExtension(): ExtensionItem[] {
  return [
    {
      id: 'markdown',
      name: 'Markdown スタイル',
      enabled: true,
      priority: 0,
      wasmBytes: markdownWasm,
    },
  ];
}

describe('shared log schema (Issue #161)', () => {
  beforeEach(() => {
    localStorage.clear();
    document.body.innerHTML = '';
  });

  // spec: alhalte-argenterie/内部フロー system event decode by label
  it('decodes banquette character event labels without producing chat garbage', () => {
    const came = systemRecord(LABEL_BANQUETTE_LOG_CHARACTER_CAME, [7], 'came');
    expect(came.kind).toBe('system');
    expect(came.event).toEqual({ type: 'character-came', characterId: '7' });
    expect(came.content).toBe('');
    // Legacy id-only payload: no append time on the wire.
    expect(came.timestamp).toBe(0);

    const gone = systemRecord(LABEL_BANQUETTE_LOG_CHARACTER_GONE, [9], 'gone');
    expect(gone.kind).toBe('system');
    expect(gone.event).toEqual({ type: 'character-gone', characterId: '9' });
    expect(gone.content).toBe('');
    expect(gone.timestamp).toBe(0);

    // Unknown labels must still fall back to text, never throw.
    const unknown = entryToLogRecord(
      'scene-schema',
      { label: 12345, payload: [0xff, 0xfe] },
      0,
      1,
    );
    expect(unknown).not.toBeNull();
    expect(unknown!.kind).toBe('chat');

    // A known label with a truncated payload still becomes a system row
    // instead of leaking postcard bytes as chat text.
    const malformed = entryToLogRecord(
      'scene-schema',
      { label: LABEL_BANQUETTE_LOG_CHARACTER_CAME, payload: [] },
      0,
      2,
    );
    expect(malformed?.kind).toBe('system');
    expect(malformed?.content).toBe('');
    expect(malformed?.timestamp).toBe(0);

    // `tinToLogRecord` uses the same label-first decode.
    const viaTin = tinToLogRecord(
      { label: LABEL_BANQUETTE_LOG_CHARACTER_CAME, payload: [7] },
      'scene-schema',
    );
    expect(viaTin?.kind).toBe('system');
    expect(viaTin?.event).toEqual({ type: 'character-came', characterId: '7' });
    expect(viaTin?.timestamp).toBe(0);
  });

  // spec: alhalte-argenterie/内部フロー system event append timestamp (#227)
  it('uses the banquette came/gone append timestamp for the system log record', () => {
    const timestamp = 1_700_000_123;
    const came = systemRecord(
      LABEL_BANQUETTE_LOG_CHARACTER_CAME,
      [...postcardVarint(7), ...postcardVarint(timestamp)],
      'came-timestamped',
    );
    expect(came.kind).toBe('system');
    expect(came.timestamp).toBe(timestamp);
    expect(came.event).toEqual({ type: 'character-came', characterId: '7' });

    const gone = systemRecord(
      LABEL_BANQUETTE_LOG_CHARACTER_GONE,
      [...postcardVarint(9), ...postcardVarint(timestamp + 1)],
      'gone-timestamped',
    );
    expect(gone.kind).toBe('system');
    expect(gone.timestamp).toBe(timestamp + 1);
    expect(gone.event).toEqual({ type: 'character-gone', characterId: '9' });

    // The timestamp is also decoded through the Tin wrapper path.
    const viaTin = tinToLogRecord(
      {
        label: LABEL_BANQUETTE_LOG_CHARACTER_CAME,
        payload: [...postcardVarint(7), ...postcardVarint(timestamp)],
      },
      'scene-schema',
    );
    expect(viaTin?.timestamp).toBe(timestamp);

    // A truncated timestamp varint must neither throw nor drop the event.
    const truncated = entryToLogRecord(
      'scene-schema',
      {
        label: LABEL_BANQUETTE_LOG_CHARACTER_GONE,
        payload: [...postcardVarint(9), 0x80],
      },
      0,
      3,
    );
    expect(truncated?.kind).toBe('system');
    expect(truncated?.timestamp).toBe(0);
    expect(truncated?.event).toEqual({ type: 'character-gone', characterId: '9' });
  });

  // spec: alhalte-argenterie/内部フロー log-record contentTin round trip
  it('round-trips contentTin and speakerId through the log-record Tin', () => {
    const contentTin = Array.from(encodeExtIoTin('保存する本文', 42));
    const tin = logRecordToTin(
      {
        id: 'round-trip-1',
        sceneId: 'scene-schema',
        content: '保存する本文',
        speaker: '探索者',
        speakerId: '42',
        timestamp: 1234,
        contentTin,
      },
      'op-161',
    );
    const record = tinToLogRecord(tin, 'scene-schema', 0, 0);
    expect(record).not.toBeNull();
    expect(record!.content).toBe('保存する本文');
    expect(record!.speakerId).toBe('42');
    expect(record!.timestamp).toBe(1234);
    expect(record!.contentTin).toEqual(contentTin);
    expect(record!.kind).toBe('chat');
  });

  // spec: alhalte-argenterie/log-tab system rows render M3 status + timestamp
  it('renders character system events as status rows without binary garbage', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const scene = testScene('scene-schema');
    const characters: Character[] = [
      {
        id: '7',
        name: '入退場テスト',
        initiative: 0,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];
    const chunkManager = new ChunkLogManager();
    const instance = mount(LogTab, {
      target,
      props: { scene, characters, chunkManager, hideHeader: true },
    });

    const records = [
      systemRecord(LABEL_BANQUETTE_LOG_CHARACTER_CAME, [7], 'sys-came'),
      systemRecord(LABEL_BANQUETTE_LOG_CHARACTER_GONE, [7], 'sys-gone'),
    ];
    for (const record of records) {
      chunkManager.appendRecord(record);
    }

    await waitFor(
      () => target.querySelectorAll('[data-testid^="log-system-text-"]').length === 2,
    );

    const list = target.querySelector('[data-testid="messages-list"]');
    expect(list).toBeTruthy();
    const text = list!.textContent ?? '';
    expect(text).toContain('入退場テスト が入室しました');
    expect(text).toContain('入退場テスト が退室しました');
    // No event payload bytes may be decoded as chat text.
    expect(text).not.toContain('\uFFFD');
    expect(text).not.toContain('[object');
    expect(target.querySelectorAll('[data-testid^="msg-content-"]')).toHaveLength(0);
    expect(target.querySelectorAll('[data-testid^="log-system-text-"]')).toHaveLength(2);
    for (const record of records) {
      expect(
        target.querySelector(`[data-testid="msg-timestamp-${record.id}"]`),
      ).toBeTruthy();
    }

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab legacy record without contentTin
  it('keeps rendering legacy records through the extension pipeline', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const scene = testScene('scene-legacy');
    const chunkManager = new ChunkLogManager();
    const instance = mount(LogTab, {
      target,
      props: {
        scene,
        chunkManager,
        extensions: markdownExtension(),
        hideHeader: true,
      },
    });

    chunkManager.appendRecord({
      id: 'legacy-chat',
      sceneId: 'scene-legacy',
      content: '**旧ログ**',
      speaker: '探索者',
      timestamp: 100,
    });

    await waitFor(
      () =>
        !!target
          .querySelector('[data-testid="msg-content-legacy-chat"]')
          ?.innerHTML?.includes('saignant-bold'),
    );
    const content = target.querySelector('[data-testid="msg-content-legacy-chat"]');
    expect(content?.innerHTML).toContain('saignant-bold');
    expect(content?.textContent?.trim()).toBe('旧ログ');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab contentTin preferred over the local pipeline
  it('renders contentTin without re-running extensions, independent of extension setup', async () => {
    const contentTin = Array.from(
      encodeGenerastAstToTin({
        type: 'nested',
        meta: 'bold',
        contents: [{ type: 'string', value: '保存済みの表示' }],
      }),
    );

    const mountWithExtensions = async (extensions: ExtensionItem[]) => {
      const target = document.createElement('div');
      document.body.appendChild(target);
      const chunkManager = new ChunkLogManager();
      const instance = mount(LogTab, {
        target,
        props: {
          scene: testScene('scene-content-tin'),
          chunkManager,
          extensions,
          hideHeader: true,
        },
      });
      chunkManager.appendRecord({
        id: 'content-tin-chat',
        sceneId: 'scene-content-tin',
        content: '**原文**',
        speaker: '探索者',
        timestamp: 100,
        contentTin,
      });
      await waitFor(
        () =>
          !!target
            .querySelector('[data-testid="msg-content-content-tin-chat"]')
            ?.innerHTML?.includes('saignant-bold'),
      );
      const html =
        target.querySelector('[data-testid="msg-content-content-tin-chat"]')?.innerHTML ?? '';
      const text =
        target.querySelector('[data-testid="msg-content-content-tin-chat"]')?.textContent ?? '';
      unmount(instance);
      target.remove();
      return { html, text };
    };

    // One client has markdown enabled, the other has no extensions at all.
    const withMarkdown = await mountWithExtensions(markdownExtension());
    const withoutExtensions = await mountWithExtensions([]);

    for (const rendered of [withMarkdown, withoutExtensions]) {
      expect(rendered.html).toContain('saignant-bold');
      expect(rendered.text.trim()).toBe('保存済みの表示');
      expect(rendered.text).not.toContain('原文');
    }
  });

  // spec: alhalte-argenterie/内部フロー 送信時 pipeline 結果を contentTin に保存する
  it('persists the send-time extension result as contentTin and renders it after the echo', async () => {
    Object.defineProperty(window, 'innerWidth', {
      writable: true,
      configurable: true,
      value: 1280,
    });
    Object.defineProperty(window, 'innerHeight', {
      writable: true,
      configurable: true,
      value: 800,
    });

    const target = document.createElement('div');
    document.body.appendChild(target);
    const room: VisitedRoom = {
      id: 'room-log-schema-test',
      name: 'ログ schema テスト部屋',
      url: 'https://alhalte.example/rooms/room-log-schema-test#key',
      lastVisited: Date.now(),
      isAdmin: true,
    };
    const characters: Character[] = [
      {
        id: 'char-1',
        name: '探索者',
        initiative: 0,
        description: '',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];
    const socket = new FakeRoomSocket();
    const app = mount(App, {
      target,
      props: {
        initialRoom: room,
        initialCharacters: characters,
        initialExtensions: markdownExtension(),
        isAdmin: true,
        roomSocket: asRoomSocket(socket),
      },
    });

    const input = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(input).toBeTruthy();
    input!.value = '**強調**';
    input!.dispatchEvent(new Event('input', { bubbles: true }));
    await waitFor(
      () =>
        target
          .querySelector('[data-testid="common-footer"]')
          ?.getAttribute('aria-busy') !== 'true',
    );
    target.querySelector<HTMLElement>('[data-testid="send-message-button"]')!.click();

    await waitFor(() => socket.requestAppendLogCalls.length === 1);
    expect(socket.requestAppendLogCalls).toHaveLength(1);
    const sent = socket.requestAppendLogCalls[0].record;
    // The raw text is still the fallback field.
    expect(sent.content).toBe('**強調**');
    // The pipeline output is persisted as a byte array.
    expect(Array.isArray(sent.contentTin)).toBe(true);
    const bytes = contentTinToBytes(sent.contentTin);
    expect(bytes).not.toBeNull();
    expect(
      new DataView(bytes!.buffer, bytes!.byteOffset, 8).getBigUint64(0, true),
    ).toBe(LABEL_ALHALTE_GENERAST);

    socket.deliverEcho();
    await waitFor(
      () =>
        !!target
          .querySelector('[data-testid^="msg-content-"]')
          ?.innerHTML?.includes('saignant-bold'),
    );
    const rendered = target.querySelector('[data-testid^="msg-content-"]');
    expect(rendered).toBeTruthy();
    expect(rendered?.innerHTML).toContain('saignant-bold');
    expect(rendered?.textContent?.trim()).toBe('強調');

    unmount(app);
    target.remove();
  });
});
