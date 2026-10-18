<script lang="ts">
  /**
   * Extension Management Tab.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> 拡張機能タブ`
   */
  import { Button, IconButton, List, ListItem, Menu, MenuItem, Switch } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { ExtensionItem } from '../../../types/index.js';
  import {
    canManageExtensions,
    deleteExtension,
    reorderExtensions,
    toggleExtension,
  } from '../../../wasm/runner.js';
  import {
    applyMenuPosition,
    getMenuPositionStyle,
  } from '../../../utils/menuPosition.js';

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

  let activeMenuExtId = $state<string | null>(null);
  let moreButtonElements = $state<Record<string, HTMLElement | undefined>>({});
  let menuElements = $state<Record<string, HTMLDivElement | undefined>>({});

  $effect(() => {
    if (activeMenuExtId && menuElements[activeMenuExtId] && moreButtonElements[activeMenuExtId]) {
      return applyMenuPosition(
        menuElements[activeMenuExtId],
        moreButtonElements[activeMenuExtId],
        {
          placement: 'bottom-end',
          anchorName: `--ext-menu-anchor-${activeMenuExtId}`,
        }
      );
    }
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
      （有効化Switch 拡張機能名 右寄せ削除メニュー）
      ...
    ］※並べ替え可能リスト
  -->
  <div class="extension-list-wrapper">
    {#if sortedExtensions.length === 0}
      <div class="empty-notice" data-testid="no-extensions-notice">
        インストールされた拡張機能はありません
      </div>
    {:else}
      <List class="extension-list" data-testid="extension-list">
        {#each sortedExtensions as ext, idx (ext.id)}
          <ListItem
            type="text"
            class="extension-item-row"
            data-testid="extension-item-{ext.id}"
          >
            {#snippet start()}
              <div class="ext-start-group">
                {#if isAdmin}
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
                {/if}
                <div
                  class="ext-icon-themed"
                  data-testid="ext-icon-{ext.id}"
                  aria-hidden="true"
                >
                  {#if ext.iconSvg}
                    <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                    {@html ext.iconSvg}
                  {:else}
                    <Icon>extension</Icon>
                  {/if}
                </div>
              </div>
            {/snippet}

            <span class="ext-name" data-testid="ext-name-{ext.id}">
              {ext.name}
            </span>

            {#snippet supportingText()}
              {#if ext.description}
                <span class="ext-description" data-testid="ext-description-{ext.id}">
                  {ext.description}
                </span>
              {/if}
            {/snippet}

            {#snippet end()}
              <div class="ext-end-actions" onclick={(e) => e.stopPropagation()}>
                <span class="priority-badge">#{ext.priority + 1}</span>
                <Switch
                  selected={ext.enabled}
                  disabled={!isAdmin}
                  onchange={() => handleToggle(ext.id)}
                  data-testid="ext-toggle-{ext.id}"
                  aria-label="{ext.name}の有効/無効化"
                />
                <div class="more-menu-wrapper">
                  <IconButton
                    variant="text"
                    disabled={!isAdmin}
                    onclick={(e) => {
                      e.stopPropagation();
                      activeMenuExtId = activeMenuExtId === ext.id ? null : ext.id;
                    }}
                    bind:element={moreButtonElements[ext.id]}
                    aria-label="その他"
                    title="その他"
                    data-testid="ext-more-btn-{ext.id}"
                  >
                    <Icon>more_vert</Icon>
                  </IconButton>
                  <Menu
                    bind:element={menuElements[ext.id]}
                    open={activeMenuExtId === ext.id}
                    anchor={moreButtonElements[ext.id]}
                    style={getMenuPositionStyle(moreButtonElements[ext.id], {
                      placement: 'bottom-end',
                      anchorName: `--ext-menu-anchor-${ext.id}`,
                    })}
                    data-testid="ext-menu-{ext.id}"
                    ontoggle={(e) => {
                      if (!e.newState || e.newState === 'closed') {
                        if (activeMenuExtId === ext.id) activeMenuExtId = null;
                      }
                    }}
                  >
                    <MenuItem
                      disabled={!isAdmin}
                      onclick={() => {
                        activeMenuExtId = null;
                        handleDelete(ext.id);
                      }}
                      data-testid="ext-delete-btn-{ext.id}"
                    >
                      {#snippet start()}<Icon>delete</Icon>{/snippet}
                      削除
                    </MenuItem>
                  </Menu>
                </div>
              </div>
            {/snippet}
          </ListItem>
        {/each}
      </List>
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
   * ARCHITECTURE.md & Issue #59:
   * 管理者（admin）以外のユーザーが開いた場合、タブ全体が暗くなりアクションを受け付けない制御（opacity: 0.5, pointer-events: none）を確実に適用
   */
  .extension-tab-container.non-admin-locked {
    opacity: 0.5;
    pointer-events: none;
    user-select: none;
    filter: grayscale(80%);
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
  .extension-list-wrapper {
    display: flex;
    flex-direction: column;
    width: 100%;
  }
  .extension-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0;
    margin: 0;
    list-style: none;
    width: 100%;
  }
  :global(.extension-item-row.np-item) {
    width: 100%;
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-surface-container-low);
    box-sizing: border-box;
  }
  .ext-start-group {
    display: flex;
    align-items: center;
    gap: 8px;
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
  /*
   * Issue #59 要件:
   * 拡張機能アイコン: SVG の色を無視してテーマの色（var(--np-color-primary)）で塗りつぶし、
   * テーマのより暗い色（var(--np-color-primary-container) または --np-color-surface-container-high）の
   * 広めの padding 付き角丸背景（--np-shape-corner-medium）を付与。
   */
  .ext-icon-themed {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    padding: 10px;
    box-sizing: border-box;
    border-radius: var(--np-shape-corner-medium, 12px);
    background: var(--np-color-primary-container, var(--np-color-surface-container-high));
    color: var(--np-color-primary);
    flex-shrink: 0;
  }
  .ext-icon-themed :global(svg) {
    width: 100%;
    height: 100%;
    fill: var(--np-color-primary) !important;
    color: var(--np-color-primary) !important;
  }
  .ext-icon-themed :global(svg *) {
    fill: var(--np-color-primary) !important;
  }
  .ext-name {
    font-weight: 600;
    font-size: 0.95rem;
    color: var(--np-color-on-surface);
  }
  .ext-description {
    font-size: 0.8rem;
    color: var(--np-color-on-surface-variant);
    line-height: 1.35;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ext-end-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .priority-badge {
    font-size: 0.75rem;
    background: var(--np-color-surface-container-highest);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--np-color-on-surface-variant);
  }
  .more-menu-wrapper {
    position: relative;
    display: inline-flex;
  }
  .empty-notice {
    padding: 32px;
    text-align: center;
    color: var(--np-color-on-surface-variant);
    font-size: 0.9rem;
  }
</style>
