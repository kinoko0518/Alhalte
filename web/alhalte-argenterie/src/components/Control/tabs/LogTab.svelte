<script lang="ts">
  /**
   * Log Tab with header, messages list, and chunk lazy loading on edge scroll.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> ログタブ`
   */
  import { Button } from 'noph-ui';
  import type { LogRecord, Scene } from '../../../types/index.js';
  import { ChunkLogManager } from '../../../utils/log.js';

  let {
    scene,
    isAdmin = false,
    chunkManager = new ChunkLogManager(),
    onCollapse,
    onExportZip,
    onRenameScene,
    onDeleteScene,
    onManageMembers,
  }: {
    scene: Scene;
    isAdmin?: boolean;
    chunkManager?: ChunkLogManager;
    onCollapse?: () => void;
    onExportZip?: () => void;
    onRenameScene?: () => void;
    onDeleteScene?: () => void;
    onManageMembers?: () => void;
  } = $props();

  let isMenuOpen = $state(false);
  let messages = $state<LogRecord[]>([]);
  let listElement: HTMLDivElement | null = $state(null);

  function refreshLogs() {
    messages = chunkManager.getLoadedRecords(scene.id);
    queueMicrotask(() => {
      if (listElement) {
        listElement.scrollTop = listElement.scrollHeight;
      }
    });
  }

  $effect(() => {
    const activeSceneId = scene.id;
    messages = chunkManager.getLoadedRecords(activeSceneId);
    return chunkManager.subscribe(() => {
      refreshLogs();
    });
  });

  async function handleScroll(e: Event) {
    const el = e.currentTarget as HTMLDivElement;
    if (el.scrollTop <= 5) {
      // Reached top edge
      const newRecords = await chunkManager.handleScrollEdge(scene.id, 'top');
      if (newRecords) refreshLogs();
    } else if (el.scrollHeight - el.scrollTop - el.clientHeight <= 5) {
      // Reached bottom edge
      const newRecords = await chunkManager.handleScrollEdge(scene.id, 'bottom');
      if (newRecords) refreshLogs();
    }
  }

  function formatTime(timestamp: number): string {
    const d = new Date(timestamp * 1000);
    return isNaN(d.getTime()) ? '1970/1/1 00:00:00' : d.toLocaleString();
  }
</script>

<div class="log-tab-container" data-testid="log-tab">
  <!-- Header:
    （左寄せしまうボタン, 中央寄せシーン名, 右寄せ歯車アイコン！［
      ZIPログ出力 ［
        管理者専用
        シーン名変更 シーン削除 シーンメンバー管理
      ］
    ］）
  -->
  <header class="log-tab-header" data-testid="log-tab-header">
    <button
      type="button"
      class="header-btn collapse-btn"
      onclick={onCollapse}
      data-testid="collapse-button"
      title="しまう"
    >
      ◀
    </button>

    <h2 class="scene-title-centered" data-testid="scene-name-header">
      {scene.name}
    </h2>

    <div class="gear-menu-wrapper">
      <button
        type="button"
        class="header-btn gear-btn"
        onclick={() => (isMenuOpen = !isMenuOpen)}
        data-testid="gear-menu-button"
        title="設定"
      >
        ⚙
      </button>

      {#if isMenuOpen}
        <div class="gear-dropdown" data-testid="gear-dropdown-menu">
          <button
            type="button"
            class="dropdown-item"
            onclick={() => {
              isMenuOpen = false;
              onExportZip?.();
            }}
            data-testid="zip-export-item"
          >
            ZIPログ出力
          </button>

          {#if isAdmin}
            <div class="dropdown-divider"></div>
            <div class="admin-badge" data-testid="admin-section-label">管理者専用</div>
            <button
              type="button"
              class="dropdown-item"
              onclick={() => {
                isMenuOpen = false;
                onRenameScene?.();
              }}
              data-testid="rename-scene-item"
            >
              シーン名変更
            </button>
            <button
              type="button"
              class="dropdown-item"
              onclick={() => {
                isMenuOpen = false;
                onDeleteScene?.();
              }}
              data-testid="delete-scene-item"
            >
              シーン削除
            </button>
            <button
              type="button"
              class="dropdown-item"
              onclick={() => {
                isMenuOpen = false;
                onManageMembers?.();
              }}
              data-testid="manage-members-item"
            >
              シーンメンバー管理
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </header>

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
        <div class="message-entry" data-testid="log-record-{record.id}">
          <div class="msg-icon-col">
            <div class="msg-avatar-placeholder">👤</div>
          </div>
          <div class="msg-body">
            <div class="msg-header">
              <span class="msg-speaker" data-testid="msg-speaker-{record.id}">{record.speaker}</span>
              <span class="msg-timestamp" data-testid="msg-timestamp-{record.id}">
                {formatTime(record.timestamp)}
              </span>
            </div>
            <div class="msg-content" data-testid="msg-content-{record.id}">
              {record.content}
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
    background: var(--md-sys-color-surface, #fbf8fd);
  }
  .log-tab-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    background: var(--md-sys-color-surface-container, #f3edf7);
    border-bottom: 1px solid var(--md-sys-color-outline-variant, #cac4d0);
    position: relative;
  }
  .header-btn {
    border: none;
    background: transparent;
    cursor: pointer;
    font-size: 1.1rem;
    padding: 6px 10px;
    border-radius: 8px;
    color: var(--md-sys-color-on-surface, #1b1b1f);
  }
  .header-btn:hover {
    background: rgba(103, 80, 164, 0.08);
  }
  .scene-title-centered {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
    text-align: center;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 8px;
  }
  .gear-menu-wrapper {
    position: relative;
  }
  .gear-dropdown {
    position: absolute;
    top: 100%;
    right: 0;
    margin-top: 6px;
    background: var(--md-sys-color-surface-container-high, #ece6f0);
    border: 1px solid var(--md-sys-color-outline-variant, #cac4d0);
    border-radius: 12px;
    padding: 6px 0;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
    z-index: 200;
    min-width: 170px;
  }
  .dropdown-item {
    display: block;
    width: 100%;
    padding: 8px 16px;
    border: none;
    background: none;
    text-align: left;
    font-size: 0.9rem;
    cursor: pointer;
    color: var(--md-sys-color-on-surface, #1b1b1f);
  }
  .dropdown-item:hover {
    background: rgba(103, 80, 164, 0.08);
  }
  .dropdown-divider {
    height: 1px;
    background: var(--md-sys-color-outline-variant, #cac4d0);
    margin: 4px 0;
  }
  .admin-badge {
    padding: 4px 16px;
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--md-sys-color-primary, #6750a4);
  }
  .messages-list {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .empty-log-notice {
    padding: 32px;
    text-align: center;
    color: var(--md-sys-color-on-surface-variant, #49454f);
    font-size: 0.9rem;
  }
  .message-entry {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }
  .msg-avatar-placeholder {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: #e0e0e0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1.1rem;
  }
  .msg-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .msg-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .msg-speaker {
    font-weight: 600;
    font-size: 0.88rem;
    color: var(--md-sys-color-on-surface, #1b1b1f);
  }
  .msg-timestamp {
    font-size: 0.75rem;
    color: var(--md-sys-color-on-surface-variant, #49454f);
  }
  .msg-content {
    font-size: 0.92rem;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>
