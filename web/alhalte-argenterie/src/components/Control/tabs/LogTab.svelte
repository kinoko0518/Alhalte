<script lang="ts">
  /**
   * Log Tab with Material 3 Expressive Header (Search Scene Switcher, Gear Menu,
   * ZIP Export, PL Done Toggle, Theme Selector), Character Pill Badges,
   * IconImagePicker Avatars, and Saignant WASM HTML Rendering.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-saignant`, `## alhalte-argenterie -> ### 部屋内 -> ログタブ`
   * - GitHub Issue #55: `feat(argenterie): ログタブの共通ヘッダー (Search)、ZIP 出力、キャラカラー Pill、Saignant WASM 描画`
   */
  import { tick, untrack } from 'svelte';
  import { LoadingIndicator } from 'noph-ui';
  import type { Character, ExtensionItem, LogRecord, QueuedLogMessage, Scene } from '../../../types/index.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../../../utils/assets.js';
  import type { ConnectionState } from '../../../utils/connection.js';
  import {
    buildGenerastFromText,
    type GenerastAST,
  } from '../../../utils/generast.js';
  import { ChunkLogManager, logRecordContentTin, readLogBiblioHead } from '../../../utils/log.js';
  import { tinToLogRecord, type RoomSocket } from '../../../utils/roomSocket.js';
  import {
    characterNamesSignature,
    clearSaignantCache,
    initSaignant,
    renderGenerast,
    renderGenerastSync,
  } from '../../../utils/saignant.js';
  import { ExtensionPipeline } from '../../../wasm/runner.js';
  import OfflineQueueNotice from '../../OfflineQueueNotice.svelte';
  import CommonHeader from '../CommonHeader.svelte';
  import IconImagePicker from '../IconImagePicker.svelte';
  import type { RoomRole } from '../../../utils/session.js';

  let {
    scene,
    allScenes = [],
    characters = [],
    extensions = [],
    extensionPipeline = new ExtensionPipeline(),
    roomId,
    role = 'participant',
    defaultPermission = null,
    chunkManager = new ChunkLogManager(),
    onCollapse,
    onSelectScene,
    onExportZip,
    onRenameScene,
    onDeleteScene,
    onManageMembers,
    onManagePermissions,
    onSetDefaultPermission,
    onRequestAssetPicker,
    hideHeader = false,
    roomSocket = null,
    connectionState,
    queuedMessages = [],
  }: {
    scene: Scene;
    allScenes?: Scene[];
    characters?: Character[];
    extensions?: ExtensionItem[];
    extensionPipeline?: ExtensionPipeline;
    roomId?: string;
    /** Role of the caller, forwarded to the inline `CommonHeader` (Issue #201). */
    role?: RoomRole;
    /** Default permission shown by the header selector (`null` = unknown). */
    defaultPermission?: RoomRole | null;
    chunkManager?: ChunkLogManager;
    onCollapse?: () => void;
    onSelectScene?: (sceneId: string) => void;
    onExportZip?: (type: 'full' | 'logs') => void;
    onRenameScene?: () => void;
    onDeleteScene?: () => void;
    onManageMembers?: () => void;
    onManagePermissions?: () => void;
    onSetDefaultPermission?: (permission: RoomRole) => void | Promise<unknown>;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
    hideHeader?: boolean;
    /**
     * Room socket used for the eager "latest chunk" load (Issue #109 / #140).
     * When absent the tab never queries, which keeps component-level tests and
     * offline previews free of server assumptions.
     */
    roomSocket?: RoomSocket | null;
    /**
     * Connection state for retry triggering: a transition back to `connected`
     * retries a load that ended in the queueing (wave notice) state.
     */
    connectionState?: ConnectionState;
    /**
     * `kind: 'message'` outbox entries for the active room (Issue #148). Entries
     * for other scenes are ignored, and an entry whose `record.id` is already in
     * the loaded messages is dropped so an echo never renders twice.
     */
    queuedMessages?: QueuedLogMessage[];
  } = $props();

  // Messages, animation tracking, & WASM rendering cache
  let initialRecordIds = new Set<string>(chunkManager.getLoadedRecords(scene.id).map((r) => r.id));
  let newRecordIds = $state(new Set<string>());
  let messages = $state<LogRecord[]>(chunkManager.getLoadedRecords(scene.id));
  let listElement = $state<HTMLDivElement | null>(null);
  let renderedContentMap = $state<Record<string, string>>({});
  let renderVersion = 0;

  // ---------------------------------------------------------------------------
  // Eager latest-chunk loading (Issue #109 / sub-issue #140)
  // ---------------------------------------------------------------------------

  /**
   * Lifecycle of the eager "latest chunk" request:
   *
   * - `idle`: no socket (or the scene was just switched); nothing requested.
   * - `loading`: `QueryLogBiblio` → `QueryLogChunk` is in flight.
   * - `loaded`: the response arrived (possibly empty); the empty state may show.
   * - `queued`: the request timed out / the connection failed; keep cached
   *   records and show the wavy queue notice until a reconnect retries.
   */
  type LogLoadState = 'idle' | 'loading' | 'loaded' | 'queued';

  let loadState = $state<LogLoadState>('idle');
  /** Scene the current {@link loadState} belongs to (`null` before the first request). */
  let loadSceneId: string | null = null;
  /** Incremented on every request and scene switch so stale responses are ignored. */
  let loadGeneration = 0;

  let isLoading = $derived(loadState === 'loading');
  let isQueueing = $derived(loadState === 'queued');
  /** Offline cache rows are shown as a read-only view (Issue #162). */
  let isOffline = $derived(
    connectionState === 'disconnected' || connectionState === 'reconnecting',
  );
  let showsCachedRecords = $derived(messages.some((record) => record.cached === true));

  /**
   * Queued offline messages visible in this scene (Issue #148).
   *
   * Only the active scene is shown, and an entry is dropped as soon as a loaded
   * message carries the same `LogRecord.id` (the server echo, or a snapshot that
   * already contains it). The outbox entry may still exist for one flush tick,
   * so this is the primary duplicate guard.
   */
  let currentQueuedMessages = $derived.by(() => {
    const loadedIds = new Set(messages.map((record) => record.id));
    return queuedMessages.filter(
      (entry) => entry.sceneId === scene.id && !loadedIds.has(entry.record.id),
    );
  });

  /** Every record `updateRenderedHtml` must produce HTML for, loaded first. */
  function renderableRecords(): LogRecord[] {
    return [...messages, ...currentQueuedMessages.map((entry) => entry.record)];
  }

  /**
   * Loads the newest chunk of `targetSceneId`:
   * `queryLogBiblio` → head `(chunk_index, position)` → `queryLogChunk(head.chunk_index)`.
   *
   * `RoomSocket` appends the resulting `LogChunkSnapshot` entries to the chunk
   * manager itself (they arrive with positional IDs), so this function never
   * adds records — the existing manager subscription re-renders the list.
   *
   * Dedupe: an in-flight or already loaded request for the same scene is a
   * no-op unless `force` is set (used by the reconnect retry). A scene switch
   * bumps the generation and resets the state, so a late response for the
   * previous scene cannot overwrite the new one.
   */
  async function loadLatestChunk(targetSceneId: string, force = false): Promise<void> {
    const socket = roomSocket;
    if (!socket) return;
    if (
      !force &&
      loadSceneId === targetSceneId &&
      (loadState === 'loading' || loadState === 'loaded')
    ) {
      return;
    }

    const generation = ++loadGeneration;
    loadSceneId = targetSceneId;
    loadState = 'loading';

    const isStale = () =>
      generation !== loadGeneration || roomSocket !== socket || scene.id !== targetSceneId;

    try {
      const biblio = await socket.queryLogBiblio(targetSceneId);
      if (isStale()) return;

      // Register the server's chunk list (array index = chunk index) before
      // requesting the head chunk so scroll-edge loading knows every chunk that
      // exists (Issue #162). Applying the biblio also drops cache-only records:
      // the server is authoritative.
      chunkManager.applyServerBiblio(targetSceneId, biblio);

      const head = readLogBiblioHead(biblio);
      if (!head) {
        // A biblio without a head means the scene has no entries yet.
        loadState = 'loaded';
        return;
      }

      await socket.queryLogChunk(targetSceneId, head.chunkIndex);
      if (isStale()) return;

      // `RoomSocket` settles the pending operation inside
      // `dispatchPendingOperations`, *before* `handleServerFrame` appends the
      // `LogChunkSnapshot` entries to the chunk manager. The continuation of the
      // await above already runs after that synchronous handler, but yield one
      // extra microtask so any subscriber work queued by the append (and the
      // `messages` update it triggers) lands before the spinner is cleared.
      await Promise.resolve();
      if (isStale()) return;

      loadState = 'loaded';
    } catch {
      if (isStale()) return;
      // `OperationTimeoutError` or a connection failure: whatever is cached
      // stays visible and the queue notice replaces the empty state. The
      // connection-state effect below retries once on reconnect.
      loadState = 'queued';
    }
  }

  function handleAnimationEnd(recordId: string, event?: AnimationEvent) {
    // If multiple animations run (fade-in 150ms and spring-spatial 500ms),
    // wait until the spatial spring animation finishes before clearing.
    if (event && event.animationName && event.animationName !== 'np-message-spring-spatial') {
      return;
    }
    if (newRecordIds.has(recordId)) {
      const next = new Set(newRecordIds);
      next.delete(recordId);
      newRecordIds = next;
    }
  }

  void updateRenderedHtml(renderableRecords());

  // A queued entry appearing / disappearing and a newly echoed record both need
  // fresh HTML. The chunk-manager subscription covers echoes; this effect covers
  // the outbox side (Issue #148).
  $effect(() => {
    const queued = currentQueuedMessages;
    const loaded = messages;
    untrack(() => {
      void updateRenderedHtml([...loaded, ...queued.map((entry) => entry.record)]);
    });
  });

  let currentSceneId = scene.id;
  let currentManager = chunkManager;

  // The chunk-manager subscription is per manager, not per scene. Keeping it in
  // its own effect means a scene switch (which re-runs the scene effect below)
  // can never drop the subscription: otherwise `LogChunkSnapshot` / append
  // updates emitted after the switch would not reach the list.
  $effect(() => {
    const manager = chunkManager;
    const unsubscribe = manager.subscribe(() => {
      refreshLogs();
    });
    // Re-sync once: records may have been appended between component
    // initialization and this effect running. `untrack` keeps `refreshLogs`'
    // reactive reads from re-triggering this effect (it assigns `messages`).
    untrack(() => {
      refreshLogs();
    });
    return unsubscribe;
  });

  // Lazy older-chunk fetches go through the same socket and decoder as the
  // eager latest-chunk load (Issue #162). The manager dedupes / shares in-flight
  // requests, so repeated scroll events cannot issue duplicate queries.
  $effect(() => {
    const socket = roomSocket;
    const manager = chunkManager;
    if (!socket) return;
    const handler = async (chunkId: string, chunkIndex: number, sceneId: string) => {
      const entries = await socket.queryLogChunk(sceneId, chunkIndex);
      return entries
        .map((entry, position) => tinToLogRecord(entry, sceneId, chunkIndex, position))
        .filter((record): record is LogRecord => record !== null) as unknown as Record<
        string,
        unknown
      >[];
    };
    manager.setChunkFetchHandler(handler);
    return () => {
      manager.setChunkFetchHandler(undefined);
    };
  });

  async function updateRenderedHtml(recs: LogRecord[]) {
    const currentVersion = ++renderVersion;
    const nextMap = { ...renderedContentMap };
    let changed = false;
    for (const r of recs) {
      // System events are rendered as a dedicated status row, never through the
      // chat HTML pipeline.
      if (r.kind === 'system') continue;
      if (!nextMap[r.id]) {
        try {
          // `contentTin` is the extension result persisted at send time. Render
          // it as-is: re-running the pipeline here would make the display depend
          // on the local extension version / priority (Issue #161).
          const contentTin = logRecordContentTin(r);
          const ast = contentTin
            ? contentTin
            : await buildGenerastFromText(
                r.content,
                extensions,
                extensionPipeline,
                r.speakerId,
              );
          if (renderVersion !== currentVersion) {
            return;
          }
          nextMap[r.id] = await renderGenerast(ast, characters);
        } catch {
          nextMap[r.id] = renderGenerastSync(r.content, characters);
        }
        changed = true;
      }
    }
    if (renderVersion !== currentVersion) {
      return;
    }
    if (changed) {
      renderedContentMap = nextMap;
    }
  }

  let extensionsSignature = $derived(
    extensions
      .map(
        (e) =>
          `${e.id}:${e.enabled}:${e.priority}:${e.wasmBytes ? e.wasmBytes.length : 0}`,
      )
      .join(';'),
  );
  let lastExtensionsSignature = extensions
    .map(
      (e) =>
        `${e.id}:${e.enabled}:${e.priority}:${e.wasmBytes ? e.wasmBytes.length : 0}`,
    )
    .join(';');
  let lastPipeline = extensionPipeline;

  $effect(() => {
    const currentSig = extensionsSignature;
    const currentPipe = extensionPipeline;
    if (currentSig !== lastExtensionsSignature || currentPipe !== lastPipeline) {
      lastExtensionsSignature = currentSig;
      lastPipeline = currentPipe;
      untrack(() => {
        clearSaignantCache();
        renderedContentMap = {};
        void updateRenderedHtml(renderableRecords());
      });
    }
  });

  // Saignant ABI v2 resolves mentions / fluct / assign names from the current
  // character list (Issue #163). A CharacterList or CharacterInfo update changes
  // the signature; drop both the component-level HTML map and the WASM render
  // cache so no message keeps a name that no longer exists.
  let characterSignature = $derived(characterNamesSignature(characters));
  let lastCharacterSignature = characterNamesSignature(characters);

  $effect(() => {
    const currentSignature = characterSignature;
    if (currentSignature === lastCharacterSignature) return;
    lastCharacterSignature = currentSignature;
    untrack(() => {
      clearSaignantCache();
      renderedContentMap = {};
      void updateRenderedHtml(renderableRecords());
    });
  });

  function refreshLogs() {
    const stickToBottom = listElement
      ? listElement.scrollHeight - listElement.scrollTop - listElement.clientHeight <= 24
      : true;
    const currentRecords = chunkManager.getLoadedRecords(scene.id);
    if (currentRecords.length === 0) {
      initialRecordIds.clear();
      newRecordIds = new Set();
    } else {
      let hasNew = false;
      const nextNewIds = new Set(newRecordIds);
      for (const r of currentRecords) {
        if (!initialRecordIds.has(r.id) && !nextNewIds.has(r.id)) {
          nextNewIds.add(r.id);
          hasNew = true;
        }
      }
      if (hasNew) {
        newRecordIds = nextNewIds;
      }
    }
    messages = currentRecords;
    void updateRenderedHtml(renderableRecords());
    queueMicrotask(() => {
      // Prepending an older chunk must not jump the reader to the bottom.
      if (listElement && stickToBottom) {
        listElement.scrollTop = listElement.scrollHeight;
      }
    });
  }

  $effect(() => {
    void initSaignant();
    const activeSceneId = scene.id;
    const activeManager = chunkManager;

    untrack(() => {
      const sceneChanged = activeSceneId !== currentSceneId;
      const managerChanged = activeManager !== currentManager;
      if (!sceneChanged && !managerChanged) return;

      currentSceneId = activeSceneId;
      currentManager = activeManager;

      if (sceneChanged) {
        // Ignore in-flight responses for the previous scene. The loading
        // effect below starts a fresh request for the new one.
        loadGeneration += 1;
        loadSceneId = null;
        loadState = 'idle';
      }

      const records = currentManager.getLoadedRecords(activeSceneId);
      initialRecordIds = new Set(records.map((r) => r.id));
      newRecordIds = new Set();
      messages = records;
      void updateRenderedHtml(renderableRecords());
    });
  });

  // The log tab is mounted when it is shown, so mounting and scene switches are
  // the two eager-load triggers (Issue #109 / #140).
  $effect(() => {
    const socket = roomSocket;
    const targetSceneId = scene.id;
    if (!socket) return;
    untrack(() => {
      void loadLatestChunk(targetSceneId);
    });
    return () => {
      // Invalidate any response still in flight (unmount / socket swap).
      loadGeneration += 1;
      loadSceneId = null;
      loadState = 'idle';
    };
  });

  // Retry a queued load once the connection transitions back to `connected`.
  let previousConnectionState: ConnectionState | undefined = connectionState;
  $effect(() => {
    const state = connectionState;
    const previous = previousConnectionState;
    previousConnectionState = state;
    if (state !== 'connected' || previous === 'connected') return;
    if (loadState !== 'queued') return;
    const targetSceneId = scene.id;
    untrack(() => {
      void loadLatestChunk(targetSceneId, true);
    });
  });

  async function handleScroll(e: Event) {
    const el = e.currentTarget as HTMLDivElement;
    try {
      if (el.scrollTop <= 5) {
        const previousHeight = el.scrollHeight;
        const previousTop = el.scrollTop;
        const newRecords = await chunkManager.handleScrollEdge(scene.id, 'top');
        if (newRecords) {
          for (const r of newRecords) {
            initialRecordIds.add(r.id);
          }
          refreshLogs();
          // Keep the previously visible message at the same viewport position
          // after the older chunk was prepended.
          await tick();
          if (listElement) {
            listElement.scrollTop = listElement.scrollHeight - previousHeight + previousTop;
          }
        }
      } else if (el.scrollHeight - el.scrollTop - el.clientHeight <= 5) {
        const newRecords = await chunkManager.handleScrollEdge(scene.id, 'bottom');
        if (newRecords) {
          for (const r of newRecords) {
            initialRecordIds.add(r.id);
          }
          refreshLogs();
        }
      }
    } catch {
      // The older-chunk query timed out / the connection dropped: keep the
      // current records and show the same reconnect notice as the eager load.
      if (loadState !== 'loading') loadState = 'queued';
    }
  }

  function formatTime(timestamp: number): string {
    const d = new Date(timestamp * 1000);
    return isNaN(d.getTime()) ? '1970/1/1 00:00:00' : d.toLocaleString();
  }

  function findCharacter(speakerId?: string, speakerName?: string): Character | undefined {
    if (speakerId) {
      const found = characters.find((c) => c.id === speakerId);
      if (found) return found;
    }
    if (speakerName) {
      return characters.find((c) => c.name === speakerName);
    }
    return undefined;
  }

  /**
   * Human-readable label for a banquette system event.
   *
   * Only the decoded payload is used, so raw postcard bytes can never leak into
   * the log as text (Issue #161).
   */
  function systemEventLabel(record: LogRecord): string {
    const event = record.event;
    if (!event) return 'システムイベント';
    const characterName = (id: string): string =>
      characters.find((c) => c.id === id)?.name || `キャラクター ${id}`;
    switch (event.type) {
      case 'character-came':
        return `${characterName(event.characterId)} が入室しました`;
      case 'character-gone':
        return `${characterName(event.characterId)} が退室しました`;
      default:
        return 'システムイベント';
    }
  }

  /** Icon glyph shown at the start of a system row. */
  function systemEventGlyph(record: LogRecord): string {
    switch (record.event?.type) {
      case 'character-came':
        return '→';
      case 'character-gone':
        return '←';
      default:
        return '•';
    }
  }
</script>

{#snippet logEntry(record: LogRecord, isNew: boolean, queued: boolean)}
  {#if record.kind === 'system'}
    <!-- Banquette system event: a quiet M3 status row. The postcard
         payload is decoded into `record.event` upstream, so no binary
         bytes are ever rendered as chat text (Issue #161). -->
    <div
      class="system-log-entry"
      class:message-entry-new={isNew}
      data-testid="log-record-{record.id}"
      data-kind="system"
      data-is-new={isNew ? 'true' : undefined}
      data-queued={queued ? 'true' : undefined}
      onanimationend={(e) => handleAnimationEnd(record.id, e)}
    >
      <span class="system-log-glyph" aria-hidden="true">
        {systemEventGlyph(record)}
      </span>
      <div class="system-log-body">
        <span
          class="system-log-text"
          data-testid="log-system-text-{record.id}"
        >
          {systemEventLabel(record)}
        </span>
        <span class="msg-timestamp" data-testid="msg-timestamp-{record.id}">
          {formatTime(record.timestamp)}
        </span>
      </div>
    </div>
  {:else}
    {@const char = findCharacter(record.speakerId, record.speaker)}
    {@const charColor = char?.color}
    <div
      class="message-entry"
      class:message-entry-new={isNew}
      data-testid="log-record-{record.id}"
      data-kind="chat"
      data-is-new={isNew ? 'true' : undefined}
      data-queued={queued ? 'true' : undefined}
      onanimationend={(e) => handleAnimationEnd(record.id, e)}
    >
      <!-- Character Avatar / Icon -->
      <div class="msg-icon-col" data-testid="msg-avatar-col-{record.id}">
        {#if char?.iconUrl}
          <IconImagePicker
            src={char.iconUrl}
            alt={record.speaker}
            size={36}
            shape="circle"
            dataTestId="msg-avatar-{record.id}"
            {onRequestAssetPicker}
          />
        {:else}
          <div
            class="msg-avatar-placeholder"
            data-testid="msg-avatar-{record.id}"
            style={charColor
              ? `background-color: color-mix(in srgb, ${charColor} 18%, transparent); color: ${charColor}; border-color: ${charColor};`
              : ''}
          >
            {record.speaker ? record.speaker.slice(0, 2) : '👤'}
          </div>
        {/if}
      </div>

      <div class="msg-body">
        <!-- 1st Row: Character Color Pill Badge & Timestamp -->
        <div class="msg-header-row">
          <span
            class="msg-speaker-pill"
            style={charColor ? `--char-color: ${charColor};` : ''}
            data-testid="msg-speaker-{record.id}"
          >
            {record.speaker}
          </span>
          <span class="msg-timestamp" data-testid="msg-timestamp-{record.id}">
            {formatTime(record.timestamp)}
          </span>
        </div>

        <!-- 2nd Row: Generast rendered HTML via Saignant WASM -->
        <div class="msg-content saignant-content" data-testid="msg-content-{record.id}">
          {@html renderedContentMap[record.id] ?? renderGenerastSync(record.content, characters)}
        </div>
      </div>
    </div>
  {/if}
{/snippet}

<div class="log-tab-container" data-testid="log-tab">
  {#if !hideHeader}
    <CommonHeader
      {scene}
      {allScenes}
      {characters}
      {roomId}
      {role}
      {defaultPermission}
      {onCollapse}
      {onSelectScene}
      {onExportZip}
      {onRenameScene}
      {onDeleteScene}
      {onManageMembers}
      {onManagePermissions}
      {onSetDefaultPermission}
    />
  {/if}

  {#if isOffline && showsCachedRecords}
    <!-- Offline display: these records came from localStorage, not the server.
         They are replaced as soon as the authoritative snapshots arrive. -->
    <div class="log-cache-notice" data-testid="log-cache-indicator" role="status">
      キャッシュ表示中
    </div>
  {/if}

  {#if isQueueing}
    <!-- Download/interpretation did not finish: keep cached records and tell
         the reader that a reconnect will reload the latest chunk (Issue #109). -->
    <div class="log-queue-notice" data-testid="log-queue-notice">
      <OfflineQueueNotice
        kind="generic"
        description="接続が回復したらログを再読み込みします"
      />
    </div>
  {/if}

  <!-- Messages List -->
  <div class="messages-area">
    <div
      class="messages-list"
      bind:this={listElement}
      onscroll={handleScroll}
      data-testid="messages-list"
      aria-busy={isLoading}
    >
      {#if messages.length === 0 && currentQueuedMessages.length === 0 && !isLoading && !isQueueing}
        <div class="empty-log-notice" data-testid="empty-log-notice">
          ログはありません
        </div>
      {:else}
        {#each messages as record (record.id)}
          {@const isNew = newRecordIds.has(record.id)}
          {@render logEntry(record, isNew, false)}
        {/each}

        {#if currentQueuedMessages.length > 0}
          <!-- Offline queue (Issue #148): the wavy divider separates the loaded
               log from messages that will be sent on the next connection. Each
               record renders through the same Saignant / contentTin path. -->
          <div class="queued-log-divider" data-testid="queued-log-divider">
            <OfflineQueueNotice kind="message" count={currentQueuedMessages.length} />
          </div>
          {#each currentQueuedMessages as queued (queued.entryId)}
            {@render logEntry(queued.record, false, true)}
          {/each}
        {/if}
      {/if}
    </div>

    {#if isLoading}
      <!-- Contained M3 loading indicator over the (possibly cached) list. The
           empty state stays hidden until the request settles. -->
      <div
        class="log-loading-overlay"
        data-testid="log-loading-overlay"
        role="status"
        aria-label="ログを読み込み中"
        aria-live="polite"
      >
        <LoadingIndicator contained data-testid="log-loading-indicator" />
      </div>
    {/if}
  </div>
</div>

<style>
  .log-tab-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--np-color-surface, #fbf8fd);
    color: var(--np-color-on-surface, #1b1b1f);
  }

  .messages-area {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .messages-list {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .empty-log-notice {
    padding: 32px;
    text-align: center;
    color: var(--np-color-on-surface-variant, #49454f);
    font-size: 0.9rem;
  }

  /*
   * Inline offline queue (Issue #148). The divider and the queued records sit at
   * the end of the list, after the loaded log; `OfflineQueueNotice` supplies the
   * wavy lines, the canonical copy and the count. Queued records keep the normal
   * message styling so they read as the same conversation.
   */
  .queued-log-divider {
    flex: none;
    width: 100%;
  }

  /*
   * Queue state (timeout / connection failure): the wavy notice sits above the
   * list so it stays visible even when cached records fill the tab.
   */
  .log-queue-notice {
    flex: none;
    padding: 8px 16px 0;
  }

  .log-cache-notice {
    flex: none;
    margin: 8px 16px 0;
    padding: 6px 12px;
    border-radius: var(--np-shape-corner-full, 9999px);
    background-color: var(--np-color-surface-container-high, #ece6f0);
    color: var(--np-color-on-surface-variant, #49454f);
    border: 1px solid var(--np-color-outline-variant, #cac4d0);
    font-size: 0.78rem;
    text-align: center;
  }

  /*
   * Contained M3 LoadingIndicator floating over the message list while the
   * biblio/chunk round trip is in flight. A surface scrim keeps cached records
   * readable without making the spinner disappear into the background.
   */
  .log-loading-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    background-color: color-mix(in srgb, var(--np-color-surface) 72%, transparent);
  }

  .message-entry {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }

  .message-entry-new {
    animation:
      np-message-spring-spatial var(--np-motion-expressive-default-spatial, 500ms cubic-bezier(0.38, 1.21, 0.22, 1)) both,
      np-message-fade-in var(--np-motion-expressive-fast-effects, 150ms cubic-bezier(0.31, 0.94, 0.34, 1)) both;
    will-change: transform, opacity;
  }

  /*
   * Banquette system events (BGM / background / enter / leave). Rendered as a
   * quiet M3 status row instead of a chat bubble so postcard event payloads
   * never appear as message text (Issue #161).
   */
  .system-log-entry {
    display: flex;
    align-items: center;
    gap: 8px;
    align-self: center;
    max-width: min(100%, 560px);
    padding: 6px 12px;
    border-radius: var(--np-shape-corner-full, 9999px);
    background-color: var(--np-color-surface-container-low, #f5f2f7);
    color: var(--np-color-on-surface-variant, #49454f);
    border: 1px solid var(--np-color-outline-variant, #cac4d0);
    font-size: 0.8rem;
    line-height: 1.4;
  }

  .system-log-glyph {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background-color: color-mix(
      in srgb,
      var(--np-color-primary, #6750a4) 14%,
      transparent
    );
    color: var(--np-color-primary, #6750a4);
    font-size: 0.75rem;
    font-weight: 700;
    flex-shrink: 0;
  }

  .system-log-body {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .system-log-text {
    overflow-wrap: anywhere;
  }

  .msg-icon-col {
    flex-shrink: 0;
    display: flex;
    align-items: flex-start;
    padding-top: 2px;
  }

  .msg-avatar-placeholder {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background-color: var(--np-color-surface-container-high, #e0e0e0);
    color: var(--np-color-on-surface, #1b1b1f);
    border: 1px solid var(--np-color-outline-variant, #ccc);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.85rem;
    font-weight: 700;
    user-select: none;
  }

  .msg-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .msg-header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }

  .msg-speaker-pill {
    display: inline-flex;
    align-items: center;
    padding: 2px 10px;
    border-radius: var(--np-shape-corner-full, 9999px);
    font-weight: 600;
    font-size: 0.85rem;
    line-height: 1.4;
    background-color: color-mix(
      in srgb,
      var(--char-color, var(--np-color-primary)) 18%,
      transparent
    );
    color: var(--char-color, var(--np-color-primary));
    border: 1px solid
      color-mix(in srgb, var(--char-color, var(--np-color-primary)) 28%, transparent);
    word-break: break-all;
  }

  .msg-timestamp {
    font-size: 0.75rem;
    color: var(--np-color-on-surface-variant, #49454f);
    margin-left: auto;
  }

  .msg-content {
    font-size: 0.95rem;
    line-height: 1.5;
    color: var(--np-color-on-surface);
    word-break: break-word;
    white-space: pre-wrap;
  }

  @keyframes np-message-spring-spatial {
    from {
      transform: translateX(var(--msg-entry-offset-x, 28px));
    }
    to {
      transform: translateX(0);
    }
  }

  @keyframes np-message-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .message-entry-new {
      animation: none;
      transform: none;
      opacity: 1;
    }
  }

  @media (prefers-reduced-transparency: reduce) {
    .log-loading-overlay {
      background-color: var(--np-color-surface);
    }
  }
</style>
