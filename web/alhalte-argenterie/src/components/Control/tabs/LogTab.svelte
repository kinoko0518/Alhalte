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
  import { untrack } from 'svelte';
  import type { Character, LogRecord, Scene } from '../../../types/index.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../../../utils/assets.js';
  import { ChunkLogManager } from '../../../utils/log.js';
  import {
    initSaignant,
    renderGenerast,
    renderGenerastSync,
  } from '../../../utils/saignant.js';
  import CommonHeader from '../CommonHeader.svelte';
  import IconImagePicker from '../IconImagePicker.svelte';

  let {
    scene,
    allScenes = [],
    characters = [],
    roomId,
    isAdmin = false,
    chunkManager = new ChunkLogManager(),
    onCollapse,
    onSelectScene,
    onExportZip,
    onRenameScene,
    onDeleteScene,
    onManageMembers,
    onManagePermissions,
    onSetDone,
    onRequestAssetPicker,
    hideHeader = false,
  }: {
    scene: Scene;
    allScenes?: Scene[];
    characters?: Character[];
    roomId?: string;
    isAdmin?: boolean;
    chunkManager?: ChunkLogManager;
    onCollapse?: () => void;
    onSelectScene?: (sceneId: string) => void;
    onExportZip?: (type: 'full' | 'logs') => void;
    onRenameScene?: () => void;
    onDeleteScene?: () => void;
    onManageMembers?: () => void;
    onManagePermissions?: () => void;
    onSetDone?: (done: boolean) => void;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
    hideHeader?: boolean;
  } = $props();

  // Messages, animation tracking, & WASM rendering cache
  let initialRecordIds = new Set<string>(chunkManager.getLoadedRecords(scene.id).map((r) => r.id));
  let newRecordIds = $state(new Set<string>());
  let messages = $state<LogRecord[]>(chunkManager.getLoadedRecords(scene.id));
  let listElement = $state<HTMLDivElement | null>(null);
  let renderedContentMap = $state<Record<string, string>>({});

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

  void updateRenderedHtml(messages);

  let currentSceneId = scene.id;
  let currentManager = chunkManager;
  let currentUnsub = chunkManager.subscribe(() => {
    refreshLogs();
  });

  async function updateRenderedHtml(recs: LogRecord[]) {
    const nextMap = { ...renderedContentMap };
    let changed = false;
    for (const r of recs) {
      if (!nextMap[r.id]) {
        try {
          nextMap[r.id] = await renderGenerast(r.content);
        } catch {
          nextMap[r.id] = renderGenerastSync(r.content);
        }
        changed = true;
      }
    }
    if (changed) {
      renderedContentMap = nextMap;
    }
  }

  function refreshLogs() {
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
    void updateRenderedHtml(messages);
    queueMicrotask(() => {
      if (listElement) {
        listElement.scrollTop = listElement.scrollHeight;
      }
    });
  }

  $effect(() => {
    void initSaignant();
    const activeSceneId = scene.id;
    const activeManager = chunkManager;

    if (activeSceneId !== currentSceneId || activeManager !== currentManager) {
      currentSceneId = activeSceneId;
      if (activeManager !== currentManager) {
        currentUnsub();
        currentManager = activeManager;
        currentUnsub = currentManager.subscribe(() => {
          refreshLogs();
        });
      }
      untrack(() => {
        const records = currentManager.getLoadedRecords(activeSceneId);
        initialRecordIds = new Set(records.map((r) => r.id));
        newRecordIds = new Set();
        messages = records;
        void updateRenderedHtml(messages);
      });
    }

    return () => {
      currentUnsub();
    };
  });

  async function handleScroll(e: Event) {
    const el = e.currentTarget as HTMLDivElement;
    if (el.scrollTop <= 5) {
      const newRecords = await chunkManager.handleScrollEdge(scene.id, 'top');
      if (newRecords) {
        for (const r of newRecords) {
          initialRecordIds.add(r.id);
        }
        refreshLogs();
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
</script>

<div class="log-tab-container" data-testid="log-tab">
  {#if !hideHeader}
    <CommonHeader
      {scene}
      {allScenes}
      {characters}
      {roomId}
      {isAdmin}
      {onCollapse}
      {onSelectScene}
      {onExportZip}
      {onRenameScene}
      {onDeleteScene}
      {onManageMembers}
      {onManagePermissions}
      {onSetDone}
    />
  {/if}

  <!-- Messages List -->
  <div
    class="messages-list"
    bind:this={listElement}
    onscroll={handleScroll}
    data-testid="messages-list"
  >
    {#if messages.length === 0}
      <div class="empty-log-notice" data-testid="empty-log-notice">
        ログはありません
      </div>
    {:else}
      {#each messages as record (record.id)}
        {@const char = findCharacter(record.speakerId, record.speaker)}
        {@const charColor = char?.color}
        {@const isNew = newRecordIds.has(record.id)}
        <div
          class="message-entry"
          class:message-entry-new={isNew}
          data-testid="log-record-{record.id}"
          data-is-new={isNew ? 'true' : undefined}
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
              {@html renderedContentMap[record.id] ?? renderGenerastSync(record.content)}
            </div>
          </div>
        </div>
      {/each}
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
</style>
