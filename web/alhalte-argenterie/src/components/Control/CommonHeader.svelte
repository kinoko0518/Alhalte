<script lang="ts">
  /**
   * Common Header for Control Panel (Issue #89).
   *
   * Layout adheres to Material 3 Expressive:
   * - Left: Panel close IconButton (size="s", variant="tonal", shape="square", width="wide").
   *   The icon follows the hosting surface (`placement`, Issue #223):
   *   `right_panel_close` on the landscape side sheet and `bottom_panel_close`
   *   on the portrait bottom sheet.
   *   On the portrait bottom sheet only, a second IconButton toggles the sheet
   *   between half and full (Issue #242): `open_in_full` / `close_fullscreen`.
   *   noph-ui's own handle only steps when the content grows taller than half
   *   (it reads `scrollHeight - clientHeight`), so the full state needs an
   *   explicit, content-independent control.
   * - Center: Search in Docked layout, contained style, displaying scene candidates (existing scenes only)
   * - Right: Gear IconButton opening menu (room ZIP export, log ZIP export, admin actions, horizontal theme color circles)
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> コントロールパネル共通ヘッダー`
   * - GitHub Issue #89: コントロールパネルに関する複数の提言
   * - GitHub Issue #223: 縦画面のボトムシートは下向きの閉じるアイコンを使う
   * - GitHub Issue #242: 縦画面のボトムシートに全開（全画面近く）状態を追加する
   */
  import { Button, Divider, IconButton, Item, Menu, MenuItem, Search } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character, PanelPlacement, Scene } from '../../types/index.js';
  import {
    ROOM_ROLES,
    ROOM_ROLE_LABELS,
    roomRoleCapabilities,
    type RoomRole,
  } from '../../utils/session.js';
  import {
    THEME_PALETTES,
    getThemeSettings,
    setThemePalette,
    type ThemePaletteId,
  } from '../../utils/theme.js';
  import {
    applyMenuPosition,
    getMenuPositionStyle,
  } from '../../utils/menuPosition.js';

  let {
    scene,
    allScenes = [],
    characters = [],
    roomId,
    role = 'participant',
    defaultPermission = null,
    placement = 'side',
    sheetExpanded = false,
    onToggleSheetExpanded,
    onCollapse,
    onSelectScene,
    onExportZip,
    onRenameScene,
    onDeleteScene,
    onManageMembers,
    onManagePermissions,
    onSetDefaultPermission,
  }: {
    scene: Scene;
    allScenes?: Scene[];
    characters?: Character[];
    roomId?: string;
    /**
     * Role of the caller (Issue #201). The admin-only gear menu section is
     * derived from it; a participant never sees it.
     */
    role?: RoomRole;
    /** Current level for accounts without an explicit entry (`null` = unknown). */
    defaultPermission?: RoomRole | null;
    /**
     * Hosting surface of the header (Issue #223). `bottom` means the portrait
     * bottom sheet, so the close button points down; the landscape side sheet
     * keeps pointing right. Passed explicitly instead of reading the viewport.
     */
    placement?: PanelPlacement;
    /**
     * Whether the portrait bottom sheet is currently fully open (Issue #242).
     * Drives the icon, the label and `aria-expanded` of the expand toggle.
     */
    sheetExpanded?: boolean;
    /**
     * Toggles the portrait bottom sheet between half and full (Issue #242).
     * The toggle is only rendered for `placement === 'bottom'` and only when
     * this callback is provided, so the landscape side sheet is unchanged.
     */
    onToggleSheetExpanded?: () => void;
    onCollapse?: () => void;
    onSelectScene?: (sceneId: string) => void;
    onExportZip?: (type: 'full' | 'logs') => void;
    onRenameScene?: () => void;
    onDeleteScene?: () => void;
    onManageMembers?: () => void;
    onManagePermissions?: () => void;
    /**
     * Sends `SetDefaultPermission` and resolves only after the banquette ack
     * (Issue #201). Replaces the old 「部屋の全てをPLに公開」 toggle.
     */
    onSetDefaultPermission?: (permission: RoomRole) => void | Promise<unknown>;
  } = $props();

  let capabilities = $derived(roomRoleCapabilities(role));
  let isAdmin = $derived(capabilities.isAdmin);

  /**
   * Material Symbols glyph for the left close button (Issue #223).
   *
   * A bottom sheet closes downwards, so it uses `bottom_panel_close`; the
   * landscape side sheet keeps the right-facing `right_panel_close`.
   */
  let closeIcon = $derived(placement === 'bottom' ? 'bottom_panel_close' : 'right_panel_close');

  // Search state
  let sceneSearchQuery = $state('');
  let isSceneSearchExpanded = $state(false);

  // Gear menu state
  let isGearMenuOpen = $state(false);
  let gearButtonEl = $state<HTMLButtonElement | null>(null);
  let gearMenuEl = $state<HTMLDivElement | undefined>(undefined);

  $effect(() => {
    if (isGearMenuOpen && gearMenuEl && gearButtonEl) {
      return applyMenuPosition(gearMenuEl, gearButtonEl, {
        placement: 'bottom-end',
        anchorName: '--gear-menu-anchor',
      });
    }
  });

  // Theme settings
  let currentPaletteId = $state<ThemePaletteId>(getThemeSettings().palette);

  // Default permission selector state (Issue #201)
  /** Level whose `SetDefaultPermission` is in flight (spinner / disabled state). */
  let pendingDefaultPermission = $state<RoomRole | null>(null);
  let defaultPermissionError = $state<string | null>(null);

  /**
   * Sends `SetDefaultPermission` for the picked level.
   *
   * The selector reflects `defaultPermission`, which only changes after the
   * banquette ack (`PermissionsUpdated`), so a rejected request
   * (`SetDefaultPermissionFailed` / `PermissionDenied` / timeout) never looks
   * like success.
   */
  async function handleSelectDefaultPermission(permission: RoomRole) {
    if (pendingDefaultPermission !== null) return;
    if (defaultPermission === permission) return;
    pendingDefaultPermission = permission;
    defaultPermissionError = null;
    try {
      await onSetDefaultPermission?.(permission);
    } catch (err) {
      defaultPermissionError = err instanceof Error ? err.message : String(err);
    } finally {
      pendingDefaultPermission = null;
    }
  }

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
  <!-- Left: Panel Close Icon Button (Issue #223: downward on the bottom sheet) -->
  <div class="header-left">
    <IconButton
      variant="tonal"
      size="s"
      shape="square"
      width="wide"
      onclick={onCollapse}
      data-testid="collapse-button"
      data-panel-placement={placement}
      aria-label="パネルを閉じる"
      title="パネルを閉じる"
    >
      <Icon>{closeIcon}</Icon>
    </IconButton>

    <!-- Left: portrait-only half ⇄ full toggle (Issue #242). The portrait
         bottom sheet is sized by its state, so its content never grows past
         half and noph-ui's handle cannot reach the full state. -->
    {#if placement === 'bottom' && onToggleSheetExpanded}
      <IconButton
        variant="tonal"
        size="s"
        shape="square"
        width="wide"
        onclick={onToggleSheetExpanded}
        data-testid="expand-sheet-button"
        data-panel-placement={placement}
        aria-expanded={sheetExpanded ? 'true' : 'false'}
        aria-label={sheetExpanded ? 'ボトムシートを半開に戻す' : 'ボトムシートを全開にする'}
        title={sheetExpanded ? 'ボトムシートを半開に戻す' : 'ボトムシートを全開にする'}
      >
        <Icon>{sheetExpanded ? 'close_fullscreen' : 'open_in_full'}</Icon>
      </IconButton>
    {/if}
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
        bind:element={gearMenuEl}
        open={isGearMenuOpen}
        anchor={gearButtonEl}
        style={getMenuPositionStyle(gearButtonEl, {
          placement: 'bottom-end',
          anchorName: '--gear-menu-anchor',
        })}
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
              onManagePermissions?.();
            }}
            data-testid="manage-permissions-item"
          >
            {#snippet start()}<Icon>admin_panel_settings</Icon>{/snippet}
            部屋の権限管理
          </MenuItem>

          <!-- 既定の権限 selector (Issue #201): replaces the old
               「部屋の全てをPLに公開」 toggle. The ack is authoritative. -->
          <div class="default-permission-section" data-testid="default-permission-section">
            <div class="menu-section-header" data-testid="default-permission-label">既定の権限</div>
            <div
              class="default-permission-row"
              role="group"
              aria-label="既定の権限"
              data-testid="default-permission-selector"
            >
              {#each ROOM_ROLES as permission (permission)}
                <Button
                  size="s"
                  variant={defaultPermission === permission ? 'filled' : 'outlined'}
                  onclick={() => handleSelectDefaultPermission(permission)}
                  disabled={pendingDefaultPermission !== null}
                  data-testid={`default-permission-btn-${permission}`}
                  data-role={permission}
                >
                  {ROOM_ROLE_LABELS[permission]}
                </Button>
              {/each}
            </div>
            {#if pendingDefaultPermission}
              <div class="default-permission-pending" role="status" data-testid="default-permission-pending">
                既定の権限を「{ROOM_ROLE_LABELS[pendingDefaultPermission]}」に変更しています…
              </div>
            {/if}
            {#if defaultPermissionError}
              <p class="warning-error" data-testid="default-permission-error" role="alert">
                既定の権限の変更に失敗しました: {defaultPermissionError}
              </p>
            {/if}
          </div>
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
    gap: 4px;
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
  .default-permission-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 4px 16px 8px;
  }
  .default-permission-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .default-permission-pending {
    font-size: 0.78rem;
    color: var(--np-color-on-surface-variant);
    padding: 2px 0;
  }
  .warning-error {
    margin: 0;
    font-size: 0.82rem;
    color: var(--np-color-error, #b3261e);
    line-height: 1.4;
  }
</style>
