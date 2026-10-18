<script lang="ts">
  /**
   * Common Header for Control Panel (Issue #89).
   *
   * Layout adheres to Material 3 Expressive:
   * - Left: Panel close IconButton (size="s", variant="tonal", shape="square", width="wide", icon="right_panel_close")
   * - Center: Search in Docked layout, contained style, displaying scene candidates (existing scenes only)
   * - Right: Gear IconButton opening menu (room ZIP export, log ZIP export, admin actions, horizontal theme color circles)
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> コントロールパネル共通ヘッダー`
   * - GitHub Issue #89: コントロールパネルに関する複数の提言
   */
  import { Button, Divider, IconButton, Item, Menu, MenuItem, Search } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character, Scene } from '../../types/index.js';
  import {
    THEME_PALETTES,
    getThemeSettings,
    setThemePalette,
    type ThemePaletteId,
  } from '../../utils/theme.js';

  let {
    scene,
    allScenes = [],
    characters = [],
    roomId,
    isAdmin = false,
    onCollapse,
    onSelectScene,
    onExportZip,
    onRenameScene,
    onDeleteScene,
    onManageMembers,
    onSetDone,
  }: {
    scene: Scene;
    allScenes?: Scene[];
    characters?: Character[];
    roomId?: string;
    isAdmin?: boolean;
    onCollapse?: () => void;
    onSelectScene?: (sceneId: string) => void;
    onExportZip?: (type: 'full' | 'logs') => void;
    onRenameScene?: () => void;
    onDeleteScene?: () => void;
    onManageMembers?: () => void;
    onSetDone?: (done: boolean) => void;
  } = $props();

  // Search state
  let sceneSearchQuery = $state('');
  let isSceneSearchExpanded = $state(false);

  // Gear menu state
  let isGearMenuOpen = $state(false);
  let gearButtonEl = $state<HTMLButtonElement | null>(null);

  // Theme settings
  let currentPaletteId = $state<ThemePaletteId>(getThemeSettings().palette);

  // Fullscreen Dialogs state
  let isDoneWarningOpen = $state(false);
  let isMemberManagementOpen = $state(false);

  let availableScenes = $derived(allScenes.length > 0 ? allScenes : [scene]);

  let filteredScenes = $derived.by(() => {
    const q = sceneSearchQuery.trim().toLowerCase();
    if (!q) return availableScenes;
    return availableScenes.filter((s) => s.name.toLowerCase().includes(q));
  });

  function handleSelectScene(sceneId: string) {
    isSceneSearchExpanded = false;
    sceneSearchQuery = '';
    onSelectScene?.(sceneId);
  }

  function handleSelectTheme(palId: ThemePaletteId) {
    currentPaletteId = palId;
    setThemePalette(palId);
    isGearMenuOpen = false;
  }

  async function handleExportZip(type: 'full' | 'logs') {
    onExportZip?.(type);
    const targetRoomId = roomId || scene.id;
    if (!targetRoomId) return;

    const origin =
      typeof window !== 'undefined' && window.location?.origin && window.location.origin !== 'null'
        ? window.location.origin
        : 'http://localhost';
    const exportUrl = `${origin}/api/room/${encodeURIComponent(targetRoomId)}/export/zip?type=${type}`;
    if (typeof window !== 'undefined' && typeof window.fetch === 'function') {
      try {
        const res = await fetch(exportUrl);
        if (res.ok) {
          const blob = await res.blob();
          const downloadUrl = URL.createObjectURL(blob);
          const a = document.createElement('a');
          a.href = downloadUrl;
          a.download = `room_${targetRoomId}_${type}.zip`;
          document.body.appendChild(a);
          a.click();
          document.body.removeChild(a);
          URL.revokeObjectURL(downloadUrl);
        }
      } catch (err) {
        if (typeof process === 'undefined' || process.env?.NODE_ENV !== 'test') {
          console.error('Failed to download room zip archive:', err);
        }
      }
    }
  }

  function handleWindowClick(e: MouseEvent) {
    const target = e.target as Node;
    if (
      isGearMenuOpen &&
      gearButtonEl &&
      !gearButtonEl.contains(target) &&
      !(e.target as Element).closest?.('[data-testid="gear-dropdown-menu"]')
    ) {
      isGearMenuOpen = false;
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

<header class="common-header" data-testid="common-header">
  <!-- Left: Panel Close Icon Button -->
  <div class="header-left">
    <IconButton
      variant="tonal"
      size="s"
      shape="square"
      width="wide"
      onclick={onCollapse}
      data-testid="collapse-button"
      aria-label="パネルを閉じる"
      title="パネルを閉じる"
    >
      <Icon>right_panel_close</Icon>
    </IconButton>
  </div>

  <!-- Center: Search in Docked layout, contained style -->
  <div class="header-center" data-testid="scene-search-anchor">
    <span class="visually-hidden-label" data-testid="scene-name-header">{scene.name}</span>
    <Search
      variant="contained"
      view="docked"
      placeholder={scene.name || 'シーンを検索'}
      bind:value={sceneSearchQuery}
      bind:expanded={isSceneSearchExpanded}
      class="common-scene-search"
      inputAttributes={{
        'data-testid': 'scene-search-input',
        'aria-label': 'シーンを検索・切り替え',
      }}
      resultsAttributes={{
        'data-testid': 'scene-search-menu',
        class: 'scene-search-docked-results',
      }}
    >
      {#each filteredScenes as s (s.id)}
        <Item
          type="button"
          role="option"
          selected={s.id === scene.id}
          onclick={() => handleSelectScene(s.id)}
          data-testid="scene-search-item-{s.id}"
        >
          {#snippet start()}
            <Icon>{s.id === scene.id ? 'check' : 'account_tree'}</Icon>
          {/snippet}
          {s.name}
        </Item>
      {/each}
      {#if filteredScenes.length === 0}
        <div class="menu-empty-notice" data-testid="empty-search-notice">
          該当するシーンがありません
        </div>
      {/if}
    </Search>
  </div>

  <!-- Right: Gear Icon & Dropdown Menu -->
  <div class="header-right">
    <IconButton
      variant="text"
      size="s"
      bind:element={gearButtonEl}
      onclick={() => (isGearMenuOpen = !isGearMenuOpen)}
      data-testid="gear-menu-button"
      aria-label="ログ・部屋設定"
      title="設定"
    >
      <Icon>settings</Icon>
    </IconButton>

    {#if isGearMenuOpen}
      <Menu
        open={isGearMenuOpen}
        anchor={gearButtonEl}
        ontoggle={(e) => {
          if (!e.newState || e.newState === 'closed') {
            isGearMenuOpen = false;
          }
        }}
        data-testid="gear-dropdown-menu"
        class="common-header-gear-menu"
      >
        <MenuItem
          onclick={() => {
            isGearMenuOpen = false;
            handleExportZip('full');
          }}
          data-testid="zip-export-item"
        >
          {#snippet start()}<Icon>inventory_2</Icon>{/snippet}
          部屋をZIP出力
        </MenuItem>

        <MenuItem
          onclick={() => {
            isGearMenuOpen = false;
            handleExportZip('logs');
          }}
          data-testid="export-zip-logs-item"
        >
          {#snippet start()}<Icon>description</Icon>{/snippet}
          ログをZIP出力
        </MenuItem>

        {#if isAdmin}
          <Divider />
          <div class="admin-badge" data-testid="admin-section-label">管理者専用</div>
          <MenuItem
            onclick={() => {
              isGearMenuOpen = false;
              onRenameScene?.();
            }}
            data-testid="rename-scene-item"
          >
            {#snippet start()}<Icon>edit</Icon>{/snippet}
            シーン名変更
          </MenuItem>
          <MenuItem
            onclick={() => {
              isGearMenuOpen = false;
              onDeleteScene?.();
            }}
            data-testid="delete-scene-item"
          >
            {#snippet start()}<Icon>delete</Icon>{/snippet}
            シーン削除
          </MenuItem>
          <MenuItem
            onclick={() => {
              isGearMenuOpen = false;
              isMemberManagementOpen = true;
              onManageMembers?.();
            }}
            data-testid="manage-members-item"
          >
            {#snippet start()}<Icon>admin_panel_settings</Icon>{/snippet}
            シーンメンバー管理
          </MenuItem>
          <MenuItem
            onclick={() => {
              isGearMenuOpen = false;
              isDoneWarningOpen = true;
            }}
            data-testid="set-done-item"
          >
            {#snippet start()}<Icon>public</Icon>{/snippet}
            部屋の全てをPLに公開
          </MenuItem>
        {/if}

        <Divider />

        <div class="menu-section-header" data-testid="theme-section-header">テーマ選択</div>
        <div
          class="theme-circles-row"
          data-testid="theme-circles-row"
          role="radiogroup"
          aria-label="テーマ選択"
        >
          {#each THEME_PALETTES as pal}
            <button
              type="button"
              role="radio"
              aria-checked={currentPaletteId === pal.id}
              class="theme-color-circle"
              class:selected={currentPaletteId === pal.id}
              style="background-color: {pal.swatchColor};"
              onclick={() => handleSelectTheme(pal.id)}
              title={pal.label}
              aria-label={pal.label}
              data-testid="theme-item-{pal.id}"
              data-circle-id={pal.id}
            >
              {#if currentPaletteId === pal.id}
                <Icon class="theme-check-icon">check</Icon>
              {/if}
              <span class="theme-color-dot visually-hidden-label" data-testid="theme-dot-{pal.id}"></span>
            </button>
          {/each}
        </div>
      </Menu>
    {/if}
  </div>
</header>

<!-- PL 公開確認 全画面警告ダイアログ -->
{#if isDoneWarningOpen}
  <div
    class="fullscreen-dialog-overlay"
    data-testid="set-done-warning-dialog"
    onclick={(e) => {
      if (e.target === e.currentTarget) isDoneWarningOpen = false;
    }}
  >
    <div
      class="dialog-box"
      data-testid="done-warning-box"
      role="alertdialog"
      aria-modal="true"
    >
      <h2 class="dialog-title-centered warning-text" data-testid="done-dialog-title">
        部屋の全てをPLに公開しますか？
      </h2>
      <p class="warning-body" data-testid="done-dialog-warning">
        部屋を「公開（done）」状態にします。
        非公開シーンを含むすべてのシーン、キャラクター、ログが参加者全員に閲覧可能になります。
        この操作を実行してよろしいですか？
      </p>

      <div class="dialog-footer-right">
        <Button
          variant="text"
          onclick={() => (isDoneWarningOpen = false)}
          data-testid="cancel-done-btn"
        >
          キャンセル
        </Button>
        <div class="confirm-done-wrapper">
          <Button
            variant="filled"
            class="confirm-done-btn"
            onclick={() => {
              isDoneWarningOpen = false;
              onSetDone?.(true);
            }}
            data-testid="confirm-done-button"
          >
            公開する
          </Button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- Member Management Dialog -->
{#if isMemberManagementOpen}
  <div
    class="fullscreen-dialog-overlay"
    data-testid="member-management-dialog"
    onclick={(e) => {
      if (e.target === e.currentTarget) isMemberManagementOpen = false;
    }}
  >
    <div
      class="dialog-box"
      data-testid="member-management-box"
      role="dialog"
      aria-modal="true"
    >
      <h2 class="dialog-title-centered" data-testid="member-management-title">
        メンバー権限を管理
      </h2>
      <div class="members-list-preview" data-testid="member-management-content">
        {#if characters.length === 0}
          <p class="empty-notice">キャラクターはいません</p>
        {:else}
          {#each characters as char (char.id)}
            <div class="member-row" data-testid="member-row-{char.id}">
              <div class="member-name-group">
                <span
                  class="member-dot"
                  style={char.color ? `background-color: ${char.color};` : ''}
                ></span>
                <span class="member-name">{char.name}</span>
              </div>
              <span class="member-role-badge">{isAdmin ? '管理者' : 'メンバー'}</span>
            </div>
          {/each}
        {/if}
      </div>
      <div class="dialog-footer-right">
        <Button
          variant="text"
          onclick={() => (isMemberManagementOpen = false)}
          data-testid="close-members-dialog-btn"
        >
          閉じる
        </Button>
      </div>
    </div>
  </div>
{/if}

<style>
  .common-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--np-color-outline-variant);
    background-color: var(--np-color-surface);
    gap: 8px;
    flex-shrink: 0;
    min-height: 52px;
    position: relative;
    z-index: 50;
    box-sizing: border-box;
  }
  .header-left {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }
  .header-center {
    flex: 1;
    display: flex;
    justify-content: center;
    min-width: 0;
    position: relative;
  }
  .header-right {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    position: relative;
  }
  .visually-hidden-label {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  :global(.common-scene-search) {
    --_bar-height: 2.5rem;
    --_pane-margin: 0px;
    width: 100%;
    max-width: 260px;
  }
  :global(.common-scene-search .np-search-bar) {
    height: 2.5rem;
    padding-inline: 0.5rem;
  }
  :global(.common-scene-search .np-search-docked .np-search-container) {
    max-height: 300px;
  }
  :global(.common-scene-search .np-search-results) {
    max-height: 240px;
    box-shadow: var(--np-elevation-2);
    border: 1px solid var(--np-color-outline-variant);
  }
  .menu-empty-notice {
    padding: 12px 16px;
    font-size: 0.85rem;
    color: var(--np-color-on-surface-variant);
    text-align: center;
  }
  .admin-badge {
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--np-color-primary);
    padding: 6px 16px 2px;
  }
  .menu-section-header {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--np-color-on-surface-variant);
    padding: 8px 16px 4px;
    letter-spacing: 0.4px;
  }
  .theme-circles-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: space-around;
    gap: 8px;
    padding: 8px 16px;
    box-sizing: border-box;
  }
  .theme-color-circle {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: none;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    outline: none;
    box-shadow: var(--np-elevation-1);
    transition: transform var(--np-motion-expressive-fast-spatial, 150ms ease),
                box-shadow var(--np-motion-expressive-fast-effects, 150ms ease);
  }
  .theme-color-circle:hover {
    transform: scale(1.18);
  }
  .theme-color-circle.selected {
    outline: 2px solid var(--np-color-primary);
    outline-offset: 2px;
    transform: scale(1.1);
  }
  :global(.theme-check-icon) {
    font-size: 16px !important;
    color: #ffffff;
    filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.7));
  }
  .fullscreen-dialog-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 9000;
  }
  .dialog-box {
    background: var(--np-color-surface-container-high, #ece6f0);
    border-radius: var(--np-shape-corner-extra-large, 28px);
    padding: 24px;
    width: 90%;
    max-width: 480px;
    box-shadow: var(--np-elevation-3, 0 8px 24px rgba(0, 0, 0, 0.2));
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .dialog-title-centered {
    margin: 0;
    text-align: center;
    font-size: 1.35rem;
    font-weight: 600;
    color: var(--np-color-on-surface, #1d1b20);
  }
  .warning-text {
    color: var(--np-color-error, #b3261e);
  }
  .warning-body {
    margin: 0;
    text-align: center;
    font-size: 0.92rem;
    color: var(--np-color-on-surface-variant, #49454f);
    line-height: 1.5;
  }
  .dialog-footer-right {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
  .confirm-done-wrapper {
    --np-button-container-color: var(--np-color-error, #b3261e);
    --np-button-label-text-color: var(--np-color-on-error, #ffffff);
  }
  .members-list-preview {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 240px;
    overflow-y: auto;
  }
  .member-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: var(--np-color-surface);
    border-radius: var(--np-shape-corner-small, 8px);
  }
  .member-name-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .member-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--np-color-primary);
  }
  .member-name {
    font-size: 0.9rem;
    font-weight: 500;
  }
  .member-role-badge {
    font-size: 0.75rem;
    padding: 2px 6px;
    border-radius: var(--np-shape-corner-extra-small, 4px);
    background: var(--np-color-surface-container);
    color: var(--np-color-on-surface-variant);
  }
  .empty-notice {
    text-align: center;
    color: var(--np-color-on-surface-variant);
    font-size: 0.85rem;
    margin: 12px 0;
  }
</style>
