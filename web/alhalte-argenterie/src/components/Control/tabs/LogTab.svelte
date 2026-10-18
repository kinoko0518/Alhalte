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
  import { Button, Divider, IconButton, Menu, MenuItem, TextField } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { Character, LogRecord, Scene } from '../../../types/index.js';
  import type { AssetItem, AssetMediaType, AssetScope } from '../../../utils/assets.js';
  import { ChunkLogManager } from '../../../utils/log.js';
  import {
    initSaignant,
    renderGenerast,
    renderGenerastSync,
  } from '../../../utils/saignant.js';
  import {
    THEME_PALETTES,
    getThemeSettings,
    setThemePalette,
    type ThemePaletteId,
  } from '../../../utils/theme.js';
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
    onSetDone,
    onRequestAssetPicker,
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
    onSetDone?: (done: boolean) => void;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
  } = $props();

  // Search & Gear menu state
  let isGearMenuOpen = $state(false);
  let isSceneMenuOpen = $state(false);
  let sceneSearchQuery = $state('');
  let gearButtonEl = $state<HTMLButtonElement | null>(null);
  let gearMenuContainerEl = $state<HTMLDivElement | null>(null);
  let sceneSearchAnchorEl = $state<HTMLDivElement | null>(null);

  // Fullscreen Dialogs state
  let isDoneWarningOpen = $state(false);
  let isMemberManagementOpen = $state(false);

  // Theme settings
  let currentPaletteId = $state<ThemePaletteId>(getThemeSettings().palette);

  // Messages, animation tracking, & WASM rendering cache
  let initialRecordIds = new Set<string>(chunkManager.getLoadedRecords(scene.id).map((r) => r.id));
  let newRecordIds = $state(new Set<string>());
  let messages = $state<LogRecord[]>(chunkManager.getLoadedRecords(scene.id));
  let listElement = $state<HTMLDivElement | null>(null);
  let renderedContentMap = $state<Record<string, string>>({});

  void updateRenderedHtml(messages);

  let currentSceneId = scene.id;
  let currentManager = chunkManager;
  let currentUnsub = chunkManager.subscribe(() => {
    refreshLogs();
  });

  let availableScenes = $derived(allScenes.length > 0 ? allScenes : [scene]);

  let filteredScenes = $derived.by(() => {
    const q = sceneSearchQuery.trim().toLowerCase();
    if (!q) return availableScenes;
    return availableScenes.filter((s) => s.name.toLowerCase().includes(q));
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

  function handleSelectTheme(palId: ThemePaletteId) {
    currentPaletteId = palId;
    setThemePalette(palId);
    isGearMenuOpen = false;
  }

  function handleConfirmSetDone() {
    onSetDone?.(true);
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

  function handleWindowClick(e: MouseEvent) {
    const target = e.target as Node;
    if (
      isGearMenuOpen &&
      gearMenuContainerEl &&
      !gearMenuContainerEl.contains(target)
    ) {
      isGearMenuOpen = false;
    }
    if (
      isSceneMenuOpen &&
      sceneSearchAnchorEl &&
      !sceneSearchAnchorEl.contains(target)
    ) {
      isSceneMenuOpen = false;
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div class="log-tab-container" data-testid="log-tab">
  <!-- Common Header:
    - 左端: サイドシート閉じる IconButton (close)
    - 中央寄せ: シーン切り替え・選択 M3 Search (TextField type="search" & Menu)
    - 右端: 歯車設定 M3 IconButton & M3 Menu
  -->
  <header class="log-tab-header" data-testid="log-tab-header">
    <div class="header-left">
      <IconButton
        variant="text"
        size="s"
        onclick={onCollapse}
        data-testid="collapse-button"
        aria-label="パネルを閉じる"
      >
        <Icon>close</Icon>
      </IconButton>
    </div>

    <div class="header-center" bind:this={sceneSearchAnchorEl} data-testid="scene-search-anchor">
      <span class="visually-hidden-label" data-testid="scene-name-header">{scene.name}</span>
      <TextField
        variant="outlined"
        type="search"
        placeholder={scene.name}
        bind:value={sceneSearchQuery}
        onfocus={() => (isSceneMenuOpen = true)}
        aria-label="シーンを検索・切り替え"
        data-testid="scene-search-input"
        class="scene-search-field"
      >
        {#snippet leading()}<Icon>search</Icon>{/snippet}
      </TextField>

      {#if isSceneMenuOpen}
        <Menu
          open={isSceneMenuOpen}
          anchor={sceneSearchAnchorEl}
          ontoggle={(e) => {
            if (!e.newState || e.newState === 'closed') {
              isSceneMenuOpen = false;
            }
          }}
          data-testid="scene-search-menu"
        >
          {#each filteredScenes as s (s.id)}
            <MenuItem
              selected={s.id === scene.id}
              onclick={() => {
                isSceneMenuOpen = false;
                sceneSearchQuery = '';
                onSelectScene?.(s.id);
              }}
              data-testid="scene-search-item-{s.id}"
            >
              {#snippet start()}
                <Icon>{s.id === scene.id ? 'check' : 'account_tree'}</Icon>
              {/snippet}
              {s.name}
            </MenuItem>
          {/each}
          {#if filteredScenes.length === 0}
            <div class="menu-empty-notice" data-testid="empty-search-notice">
              該当するシーンがありません
            </div>
          {/if}
        </Menu>
      {/if}
    </div>

    <div class="header-right" bind:this={gearMenuContainerEl}>
      <IconButton
        variant="text"
        size="s"
        bind:element={gearButtonEl}
        onclick={() => (isGearMenuOpen = !isGearMenuOpen)}
        data-testid="gear-menu-button"
        aria-label="ログ・部屋設定"
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
          {#each THEME_PALETTES as pal}
            <MenuItem
              selected={currentPaletteId === pal.id}
              onclick={() => handleSelectTheme(pal.id)}
              data-testid="theme-item-{pal.id}"
            >
              {#snippet start()}
                <span
                  class="theme-color-dot"
                  style="background-color: {pal.swatchColor};"
                  data-testid="theme-dot-{pal.id}"
                ></span>
              {/snippet}
              {pal.label}
            </MenuItem>
          {/each}
        </Menu>
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
        {@const char = findCharacter(record.speakerId, record.speaker)}
        {@const charColor = char?.color}
        {@const isNew = newRecordIds.has(record.id)}
        <div
          class="message-entry"
          class:message-entry-new={isNew}
          data-testid="log-record-{record.id}"
          data-is-new={isNew ? 'true' : undefined}
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

  <!-- Fullscreen Warning Dialog: 部屋の全てをPLに公開 (setDone: true) -->
  {#if isDoneWarningOpen}
    <div
      class="fullscreen-dialog-overlay"
      data-testid="set-done-warning-dialog"
      onclick={(e) => {
        if (e.target === e.currentTarget) isDoneWarningOpen = false;
      }}
    >
      <div
        class="dialog-box warning-box"
        data-testid="set-done-dialog-box"
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
                handleConfirmSetDone();
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
            variant="tonal"
            onclick={() => (isMemberManagementOpen = false)}
            data-testid="close-member-management-btn"
          >
            閉じる
          </Button>
        </div>
      </div>
    </div>
  {/if}
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

  .log-tab-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 12px;
    background: var(--np-color-surface-container, #f3edf7);
    border-bottom: 1px solid var(--np-color-outline-variant, #cac4d0);
    position: relative;
    z-index: 10;
  }

  .header-left,
  .header-right {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .header-right {
    position: relative;
  }

  .header-center {
    flex: 1;
    min-width: 0;
    display: flex;
    justify-content: center;
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

  :global(.scene-search-field) {
    width: 100%;
    max-width: 280px;
    --np-text-field-container-color: var(--np-color-surface-container-high);
  }

  .admin-badge {
    padding: 4px 16px;
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--np-color-primary, #6750a4);
  }

  .menu-empty-notice {
    padding: 12px 16px;
    font-size: 0.85rem;
    color: var(--np-color-on-surface-variant);
  }

  .menu-section-header {
    padding: 8px 16px 4px;
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--np-color-on-surface-variant);
    text-transform: uppercase;
  }

  .theme-color-dot {
    display: inline-block;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1px solid var(--np-color-outline);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.2);
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

  /* Full-screen Overlay Styles */
  .fullscreen-dialog-overlay {
    position: fixed;
    inset: 0;
    background-color: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1200;
    backdrop-filter: blur(2px);
    animation: np-overlay-fade-in var(--np-motion-expressive-fast-effects, 200ms ease);
  }

  .dialog-box {
    background-color: var(--np-color-surface-container-high, #ece6f0);
    border-radius: var(--np-shape-corner-extra-large, 28px);
    box-shadow: var(--np-elevation-3);
    padding: 24px;
    width: 90%;
    max-width: 480px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 16px;
    animation: np-dialog-scale-in var(--np-motion-expressive-fast-spatial, 200ms ease);
  }

  .warning-box {
    border: 1px solid var(--np-color-error-container, #ffdad6);
  }

  .dialog-title-centered {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 700;
    text-align: center;
    color: var(--np-color-on-surface);
  }

  .warning-text {
    color: var(--np-color-error, #ba1a1a);
  }

  .warning-body {
    margin: 0;
    font-size: 0.95rem;
    line-height: 1.5;
    color: var(--np-color-on-surface-variant);
    text-align: center;
  }

  .dialog-footer-right {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 12px;
    margin-top: 8px;
  }

  :global(.confirm-done-btn) {
    --np-button-container-color: var(--np-color-error);
    --np-button-label-text-color: var(--np-color-on-error);
  }

  .members-list-preview {
    max-height: 240px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .member-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: var(--np-color-surface-container);
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
    background-color: var(--np-color-primary);
  }

  .member-name {
    font-size: 0.9rem;
    font-weight: 600;
  }

  .member-role-badge {
    font-size: 0.75rem;
    padding: 2px 8px;
    border-radius: var(--np-shape-corner-full, 9999px);
    background: var(--np-color-surface-container-highest);
    color: var(--np-color-on-surface-variant);
  }

  .empty-notice {
    text-align: center;
    color: var(--np-color-on-surface-variant);
    font-size: 0.85rem;
  }

  @keyframes np-overlay-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes np-dialog-scale-in {
    from {
      opacity: 0;
      transform: scale(0.95);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
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
