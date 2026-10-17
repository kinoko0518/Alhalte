<script lang="ts">
  /**
   * Extension Management Tab.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> 拡張機能タブ`
   */
  import { Button, Card } from 'noph-ui';
  import type { ExtensionItem } from '../../../types/index.js';
  import {
    canManageExtensions,
    deleteExtension,
    reorderExtensions,
    toggleExtension,
  } from '../../../wasm/runner.js';

  let {
    extensions = [],
    isAdmin = false,
    onUpdateExtensions,
    onUploadExtension,
  }: {
    extensions?: ExtensionItem[];
    isAdmin?: boolean;
    onUpdateExtensions?: (extensions: ExtensionItem[]) => void;
    onUploadExtension?: () => void;
  } = $props();

  let sortedExtensions = $derived.by(() => {
    return [...extensions].sort((a, b) => a.priority - b.priority);
  });

  function handleToggle(id: string) {
    if (!canManageExtensions(isAdmin)) return;
    try {
      const updated = toggleExtension(extensions, id, isAdmin);
      onUpdateExtensions?.(updated);
    } catch {
      // Non-admin blocked
    }
  }

  function handleDelete(id: string) {
    if (!canManageExtensions(isAdmin)) return;
    try {
      const updated = deleteExtension(extensions, id, isAdmin);
      onUpdateExtensions?.(updated);
    } catch {
      // Non-admin blocked
    }
  }

  function handleMoveUp(index: number) {
    if (!canManageExtensions(isAdmin) || index <= 0) return;
    try {
      const updated = reorderExtensions(extensions, index, index - 1, isAdmin);
      onUpdateExtensions?.(updated);
    } catch {
      // Non-admin blocked
    }
  }

  function handleMoveDown(index: number) {
    if (!canManageExtensions(isAdmin) || index >= sortedExtensions.length - 1) return;
    try {
      const updated = reorderExtensions(extensions, index, index + 1, isAdmin);
      onUpdateExtensions?.(updated);
    } catch {
      // Non-admin blocked
    }
  }
</script>

<div
  class="extension-tab-container"
  class:non-admin-locked={!isAdmin}
  data-testid="extension-management-tab"
>
  <div class="tab-header">
    <div class="title-with-lock">
      <h3 class="tab-title">拡張機能一覧</h3>
      {#if !isAdmin}
        <span class="lock-indicator" data-testid="lock-indicator">（管理者のみ操作可能）</span>
      {/if}
    </div>
    {#if isAdmin}
      <Button variant="filled" onclick={onUploadExtension} data-testid="upload-extension-btn">
        拡張機能を追加 (.wasm)
      </Button>
    {/if}
  </div>

  <!-- Reorderable list:
    ［
      （有効化チェックボックス 拡張機能名 右寄せ削除ボタン）
      ...
    ］※並べ替え可能リスト
  -->
  <div class="extension-list" data-testid="extension-list">
    {#if sortedExtensions.length === 0}
      <div class="empty-notice" data-testid="no-extensions-notice">
        インストールされた拡張機能はありません
      </div>
    {:else}
      {#each sortedExtensions as ext, idx (ext.id)}
        <div
          class="extension-card-wrapper"
          class:disabled-card={!isAdmin}
          data-testid="extension-item-{ext.id}"
        >
          <Card variant="outlined">
            <div class="ext-card-content">
              <!-- Reorder buttons (up/down) -->
              <div class="reorder-col">
                <button
                  type="button"
                  class="reorder-btn"
                  disabled={!isAdmin || idx === 0}
                  onclick={() => handleMoveUp(idx)}
                  data-testid="move-up-btn-{ext.id}"
                  title="優先度を上げる"
                >
                  ▲
                </button>
                <button
                  type="button"
                  class="reorder-btn"
                  disabled={!isAdmin || idx === sortedExtensions.length - 1}
                  onclick={() => handleMoveDown(idx)}
                  data-testid="move-down-btn-{ext.id}"
                  title="優先度を下げる"
                >
                  ▼
                </button>
              </div>

              <!-- Enable checkbox -->
              <input
                type="checkbox"
                class="enable-checkbox"
                checked={ext.enabled}
                disabled={!isAdmin}
                onchange={() => handleToggle(ext.id)}
                data-testid="ext-toggle-{ext.id}"
                title={isAdmin ? '有効/無効化' : '管理者のみ切り替え可能'}
              />

              <!-- Extension Icon (if provided by self_introduction) -->
              {#if ext.iconSvg}
                <span class="ext-icon" data-testid="ext-icon-{ext.id}" aria-hidden="true">
                  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                  {@html ext.iconSvg}
                </span>
              {/if}

              <!-- Extension Name & Description -->
              <div class="ext-info">
                <span class="ext-name" data-testid="ext-name-{ext.id}">
                  {ext.name}
                </span>
                {#if ext.description}
                  <span class="ext-description" data-testid="ext-description-{ext.id}">
                    {ext.description}
                  </span>
                {/if}
              </div>

              <span class="priority-badge">#{ext.priority + 1}</span>

              <!-- Right-aligned Delete button -->
              <div class="right-delete">
                <button
                  type="button"
                  class="delete-btn"
                  disabled={!isAdmin}
                  onclick={() => handleDelete(ext.id)}
                  data-testid="ext-delete-btn-{ext.id}"
                  title={isAdmin ? '削除' : '管理者のみ削除可能'}
                >
                  🗑
                </button>
              </div>
            </div>
          </Card>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .extension-tab-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 16px;
    box-sizing: border-box;
    overflow-y: auto;
    background: var(--np-color-surface);
    color: var(--np-color-on-surface);
    gap: 16px;
  }
  /*
   * ARCHITECTURE.md: "部屋の管理者以外にはグレーアウトして見え、触っても動かない。"
   */
  .extension-tab-container.non-admin-locked {
    opacity: 0.65;
    filter: grayscale(80%);
  }
  .extension-tab-container.non-admin-locked .extension-card-wrapper {
    pointer-events: none;
    cursor: not-allowed;
  }
  .tab-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .title-with-lock {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tab-title {
    margin: 0;
    font-size: 1.15rem;
    font-weight: 700;
  }
  .lock-indicator {
    font-size: 0.8rem;
    color: var(--np-color-error);
  }
  .extension-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .empty-notice {
    padding: 32px;
    text-align: center;
    color: var(--np-color-on-surface-variant);
    font-size: 0.9rem;
  }
  .ext-card-content {
    padding: 12px 16px;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .reorder-col {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .reorder-btn {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 0.65rem;
    padding: 2px 4px;
    color: var(--np-color-on-surface-variant);
  }
  .reorder-btn:disabled {
    color: var(--np-color-outline-variant);
    cursor: not-allowed;
  }
  .enable-checkbox {
    width: 18px;
    height: 18px;
    cursor: pointer;
  }
  .enable-checkbox:disabled {
    cursor: not-allowed;
  }
  .ext-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    flex-shrink: 0;
    color: var(--np-color-primary);
  }
  .ext-icon :global(svg) {
    width: 24px;
    height: 24px;
  }
  .ext-info {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    gap: 2px;
  }
  .ext-name {
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--np-color-on-surface);
  }
  .ext-description {
    font-size: 0.78rem;
    color: var(--np-color-on-surface-variant);
    line-height: 1.35;
  }
  .priority-badge {
    font-size: 0.75rem;
    background: var(--np-color-surface-container-highest);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--np-color-on-surface-variant);
  }
  .right-delete {
    margin-left: auto;
  }
  .delete-btn {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 1.1rem;
    color: var(--np-color-error);
    padding: 4px 6px;
  }
  .delete-btn:disabled {
    color: var(--np-color-outline-variant);
    cursor: not-allowed;
  }
</style>
