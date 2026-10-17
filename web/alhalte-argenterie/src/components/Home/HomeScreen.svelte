<script lang="ts">
  /**
   * Home screen with Material 3 Expressive Top App Bar, theme selector,
   * hero banner, visited room cards, ExtendedFAB, and room creation overlay.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### ホーム画面`
   * - GitHub Issue #32: Rich Material 3 Expressive HomeScreen & theme selection
   */
  import { AppBar, Button, Card, ExtendedFab, IconButton } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { VisitedRoom } from '../../types/index.js';
  import {
    generateRoomUrl,
    getVisitedRooms,
    removeVisitedRoom,
    saveVisitedRoom,
  } from '../../utils/home.js';
  import {
    applyThemeToDocument,
    getThemeSettings,
    setThemeMode,
    setThemePalette,
    THEME_PALETTES,
    type ThemeMode,
    type ThemePaletteId,
  } from '../../utils/theme.js';

  let {
    baseUrl = typeof window !== 'undefined' ? window.location.origin : 'https://example.com',
    onEnterRoom,
  }: {
    baseUrl?: string;
    onEnterRoom?: (room: VisitedRoom) => void;
  } = $props();

  let visitedRooms = $state<VisitedRoom[]>(getVisitedRooms());
  let isCreateDialogOpen = $state(false);
  let newRoomName = $state('');
  let generatedUrlData = $state<{ roomId: string; roomKeyHex: string; roomUrl: string } | null>(null);
  let copyNotice = $state('');

  let themeSettings = $state(getThemeSettings());

  function loadRooms() {
    visitedRooms = getVisitedRooms();
  }

  $effect(() => {
    applyThemeToDocument(themeSettings);
  });

  function handleCycleThemeMode() {
    const order: ThemeMode[] = ['system', 'light', 'dark'];
    const nextIdx = (order.indexOf(themeSettings.mode) + 1) % order.length;
    themeSettings = setThemeMode(order[nextIdx]);
  }

  function handleSelectPalette(paletteId: ThemePaletteId) {
    themeSettings = setThemePalette(paletteId);
  }

  function openCreateDialog() {
    newRoomName = '';
    copyNotice = '';
    generatedUrlData = generateRoomUrl(baseUrl);
    isCreateDialogOpen = true;
  }

  function closeCreateDialog() {
    isCreateDialogOpen = false;
  }

  async function handleCopyLink() {
    if (!generatedUrlData) return;
    try {
      if (typeof navigator !== 'undefined' && navigator.clipboard) {
        await navigator.clipboard.writeText(generatedUrlData.roomUrl);
        copyNotice = 'コピーしました';
      } else {
        copyNotice = 'URL: ' + generatedUrlData.roomUrl;
      }
    } catch {
      copyNotice = 'コピーに失敗しました';
    }
  }

  function pushRoomToHistory(room: VisitedRoom) {
    if (typeof window === 'undefined' || !window.history?.pushState) return;
    try {
      const parsed = new URL(room.url, window.location.origin);
      const targetUrl =
        parsed.origin === window.location.origin
          ? parsed.href
          : `${parsed.pathname}${parsed.search}${parsed.hash}`;
      window.history.pushState({ roomId: room.id }, '', targetUrl);
    } catch {
      // Ignore history errors in restricted environments
    }
  }

  function handleJoinRoom() {
    if (!generatedUrlData) return;
    const room: VisitedRoom = {
      id: generatedUrlData.roomId,
      name: newRoomName.trim() || '名称未設定',
      url: generatedUrlData.roomUrl,
      lastVisited: Date.now(),
      isAdmin: true, // The creator is the initial admin
    };
    saveVisitedRoom(room);
    loadRooms();
    isCreateDialogOpen = false;
    pushRoomToHistory(room);
    onEnterRoom?.(room);
  }

  function handleSelectRoom(room: VisitedRoom) {
    saveVisitedRoom(room);
    pushRoomToHistory(room);
    onEnterRoom?.(room);
  }

  function handleRemoveRoom(roomId: string) {
    removeVisitedRoom(roomId);
    loadRooms();
  }
</script>

{#snippet appBarLeading()}
  <div class="brand-emblem" aria-hidden="true">
    <Icon>casino</Icon>
  </div>
{/snippet}

{#snippet appBarTrailing()}
  <div class="theme-controls" data-testid="theme-controls">
    <div class="palette-swatches" role="group" aria-label="カラーテーマ選択">
      {#each THEME_PALETTES as palette (palette.id)}
        <button
          type="button"
          class="palette-swatch-btn"
          class:active={themeSettings.palette === palette.id}
          style="--_swatch-color: {palette.swatchColor};"
          onclick={() => handleSelectPalette(palette.id)}
          aria-label="テーマ: {palette.label}"
          aria-pressed={themeSettings.palette === palette.id}
          data-testid="theme-palette-{palette.id}"
        ></button>
      {/each}
    </div>

    <IconButton
      variant="tonal"
      size="s"
      onclick={handleCycleThemeMode}
      data-testid="theme-mode-toggle"
      aria-label="表示モード切替 ({themeSettings.mode})"
    >
      {#if themeSettings.mode === 'light'}
        <Icon>light_mode</Icon>
      {:else if themeSettings.mode === 'dark'}
        <Icon>dark_mode</Icon>
      {:else}
        <Icon>contrast</Icon>
      {/if}
    </IconButton>
  </div>
{/snippet}

{#snippet fabAddIcon()}
  <Icon>add</Icon>
{/snippet}

<div class="home-container" data-testid="home-screen">
  <AppBar
    variant="small"
    headline="Alhalte"
    subtitle="Self-Hosted TRPG Virtual Tabletop"
    leading={appBarLeading}
    trailing={appBarTrailing}
  />

  <div class="home-content-wrapper">
    <!-- Expressive Hero Banner -->
    <section class="hero-banner" data-testid="home-hero-banner">
      <div class="hero-text-col">
        <div class="hero-badge">
          <Icon>encrypted</Icon>
          <span>End-to-End URL Fragment Encryption</span>
        </div>
        <h1 class="home-title">テーブルを囲み、物語のシーンを紡ぐ</h1>
        <p class="home-subtitle">
          シーンを第一級概念として備えたセルフホストVTT。部屋の暗号鍵はURLフラグメントにのみ保持され、サーバーに知られることなく安全にセッションを進行できます。
        </p>
        <div class="hero-stats-row">
          <div class="stat-pill">
            <Icon>history</Icon>
            <span>訪れた部屋: <strong>{visitedRooms.length}</strong> 件</span>
          </div>
          <div class="stat-pill">
            <Icon>palette</Icon>
            <span>Material 3 Expressive ({themeSettings.palette})</span>
          </div>
        </div>
      </div>

      <div class="hero-action-col">
        <Button variant="filled" size="m" start={fabAddIcon} onclick={openCreateDialog}>
          新しい部屋を作成する
        </Button>
      </div>
    </section>

    <div class="section-header-row">
      <h2 class="section-heading">訪れたことのある部屋</h2>
      <span class="section-caption">ブラウザの履歴に保存されたプレイヤー参加ルーム</span>
    </div>

    <main class="rooms-grid" data-testid="visited-rooms-list">
      {#if visitedRooms.length === 0}
        <div class="no-rooms-card" data-testid="no-rooms-message">
          <div class="empty-icon-circle">
            <Icon>meeting_room</Icon>
          </div>
          <p class="empty-title">訪れた部屋の履歴はありません</p>
          <p class="empty-subtext">
            招待リンクから部屋にアクセスするか、右下の「部屋の新規作成」ボタンから新しいセッションを開始してください。
          </p>
        </div>
      {:else}
        {#each visitedRooms as room (room.id)}
          <div class="room-card-wrapper" data-testid="room-card-{room.id}">
            <Card variant="elevated">
              <div class="room-card-body">
                <div class="room-card-top">
                  <div class="room-avatar" aria-hidden="true">
                    <Icon>fort</Icon>
                  </div>
                  <div class="room-title-group">
                    <h3 class="room-card-name">{room.name}</h3>
                    <span class="room-id-chip">ID: {room.id.slice(0, 8)}</span>
                  </div>
                  <IconButton
                    variant="text"
                    size="xs"
                    aria-label="履歴から削除"
                    onclick={() => handleRemoveRoom(room.id)}
                    data-testid="remove-room-{room.id}"
                  >
                    <Icon>delete</Icon>
                  </IconButton>
                </div>

                <div class="room-meta-row">
                  <Icon>schedule</Icon>
                  <p class="room-card-date">
                    最終アクセス: {new Date(room.lastVisited).toLocaleString()}
                  </p>
                </div>

                <div class="room-card-actions">
                  <Button variant="filled" size="s" onclick={() => handleSelectRoom(room)}>
                    入室する
                  </Button>
                </div>
              </div>
            </Card>
          </div>
        {/each}
      {/if}
    </main>
  </div>

  <div class="fab-container">
    <ExtendedFab
      variant="primary"
      size="m"
      icon={fabAddIcon}
      label="部屋の新規作成"
      onclick={openCreateDialog}
      data-testid="create-room-button"
    />
  </div>

  {#if isCreateDialogOpen}
    <div class="dialog-overlay" data-testid="create-room-overlay">
      <div class="dialog-content">
        <h2 class="dialog-header-centered" data-testid="dialog-title">部屋の作成</h2>

        <div class="dialog-field">
          <label for="room-name-input" class="field-label">ルーム名</label>
          <input
            id="room-name-input"
            class="single-line-input"
            type="text"
            placeholder="ルーム名"
            bind:value={newRoomName}
            data-testid="room-name-input"
          />
        </div>

        <div class="dialog-field url-copy-row">
          <input
            class="single-line-input readonly-input"
            type="text"
            readonly
            value={generatedUrlData?.roomUrl ?? ''}
            data-testid="room-url-input"
          />
          <Button variant="tonal" onclick={handleCopyLink} data-testid="copy-link-button">
            リンクをコピー
          </Button>
        </div>
        {#if copyNotice}
          <div class="copy-notice" data-testid="copy-notice">{copyNotice}</div>
        {/if}

        <p class="dialog-warning-centered" data-testid="dialog-warning">
          このリンクを紛失した場合、部屋へのアクセスは永久に失われます！
        </p>

        <div class="dialog-actions">
          <Button variant="text" onclick={closeCreateDialog}>キャンセル</Button>
          <Button variant="filled" onclick={handleJoinRoom} data-testid="join-created-room-button">
            入室する
          </Button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .home-container {
    position: relative;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    background-color: var(--np-color-background);
    color: var(--np-color-on-background);
    font-family: Roboto, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    overflow-y: auto;
  }
  .brand-emblem {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    margin-inline-start: 8px;
    border-radius: var(--np-shape-corner-medium);
    background: var(--np-color-primary-container);
    color: var(--np-color-on-primary-container);
  }
  .theme-controls {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-inline-end: 8px;
  }
  .palette-swatches {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-radius: var(--np-shape-corner-full);
    background: var(--np-color-surface-container);
    border: 1px solid var(--np-color-outline-variant);
  }
  .palette-swatch-btn {
    width: 22px;
    height: 22px;
    border-radius: var(--np-shape-corner-full);
    border: 2px solid transparent;
    background-color: var(--_swatch-color);
    cursor: pointer;
    padding: 0;
    transition: transform var(--np-motion-expressive-fast-spatial);
  }
  .palette-swatch-btn:hover {
    transform: scale(1.15);
  }
  .palette-swatch-btn.active {
    border-color: var(--np-color-on-surface);
    transform: scale(1.2);
  }
  .home-content-wrapper {
    width: 100%;
    max-width: 1120px;
    margin: 0 auto;
    padding: 20px 24px 112px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }
  .hero-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 28px 32px;
    border-radius: var(--np-shape-corner-extra-large);
    background: linear-gradient(
      135deg,
      var(--np-color-primary-container) 0%,
      var(--np-color-secondary-container) 60%,
      var(--np-color-tertiary-container) 100%
    );
    color: var(--np-color-on-primary-container);
    box-shadow: var(--np-elevation-1);
  }
  .hero-text-col {
    flex: 1 1 480px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .hero-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    width: fit-content;
    padding: 4px 12px;
    border-radius: var(--np-shape-corner-full);
    background: var(--np-color-surface-container-lowest);
    color: var(--np-color-primary);
    font-size: 0.78rem;
    font-weight: 600;
  }
  .home-title {
    margin: 0;
    font-size: 1.85rem;
    font-weight: 800;
    letter-spacing: -0.01em;
    color: var(--np-color-on-primary-container);
  }
  .home-subtitle {
    margin: 0;
    font-size: 0.95rem;
    line-height: 1.5;
    color: var(--np-color-on-secondary-container);
  }
  .hero-stats-row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 4px;
  }
  .stat-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--np-shape-corner-medium);
    background: color-mix(in srgb, var(--np-color-surface-container-lowest) 75%, transparent);
    color: var(--np-color-on-surface);
    font-size: 0.84rem;
  }
  .hero-action-col {
    display: flex;
    align-items: center;
  }
  .section-header-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 8px;
  }
  .section-heading {
    margin: 0;
    font-size: 1.3rem;
    font-weight: 700;
    color: var(--np-color-on-surface);
  }
  .section-caption {
    font-size: 0.85rem;
    color: var(--np-color-on-surface-variant);
  }
  .rooms-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 16px;
  }
  .no-rooms-card {
    grid-column: 1 / -1;
    padding: 48px 24px;
    border-radius: var(--np-shape-corner-extra-large);
    background: var(--np-color-surface-container-low);
    border: 1px dashed var(--np-color-outline-variant);
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 10px;
    color: var(--np-color-on-surface-variant);
  }
  .empty-icon-circle {
    width: 56px;
    height: 56px;
    border-radius: var(--np-shape-corner-full);
    background: var(--np-color-secondary-container);
    color: var(--np-color-on-secondary-container);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .empty-title {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
  }
  .empty-subtext {
    margin: 0;
    max-width: 460px;
    font-size: 0.88rem;
    line-height: 1.5;
  }
  .room-card-body {
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .room-card-top {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .room-avatar {
    width: 44px;
    height: 44px;
    border-radius: var(--np-shape-corner-medium);
    background: var(--np-color-primary-container);
    color: var(--np-color-on-primary-container);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .room-title-group {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .room-card-name {
    margin: 0;
    font-size: 1.15rem;
    font-weight: 700;
    color: var(--np-color-on-surface);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .room-id-chip {
    font-size: 0.75rem;
    color: var(--np-color-on-surface-variant);
    font-family: monospace;
  }
  .room-meta-row {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--np-color-on-surface-variant);
    font-size: 0.84rem;
  }
  .room-card-date {
    margin: 0;
  }
  .room-card-actions {
    margin-top: 4px;
    display: flex;
    justify-content: flex-end;
  }
  .fab-container {
    position: fixed;
    right: 32px;
    bottom: 32px;
    z-index: 100;
  }
  .dialog-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: color-mix(in srgb, var(--np-color-scrim) 45%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .dialog-content {
    background: var(--np-color-surface-container-high);
    color: var(--np-color-on-surface);
    padding: 28px;
    border-radius: var(--np-shape-corner-extra-large);
    width: 90%;
    max-width: 520px;
    box-shadow: var(--np-elevation-3);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .dialog-header-centered {
    margin: 0;
    text-align: center;
    font-size: 1.5rem;
    font-weight: 700;
    color: var(--np-color-on-surface);
  }
  .dialog-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field-label {
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--np-color-on-surface-variant);
  }
  .single-line-input {
    width: 100%;
    padding: 12px 16px;
    border: 1px solid var(--np-color-outline);
    border-radius: var(--np-shape-corner-medium);
    font-size: 1rem;
    background: var(--np-color-surface-container-lowest);
    color: var(--np-color-on-surface);
    box-sizing: border-box;
  }
  .readonly-input {
    background: var(--np-color-surface-container);
    color: var(--np-color-on-surface-variant);
    cursor: default;
  }
  .url-copy-row {
    display: flex;
    flex-direction: row;
    gap: 8px;
    align-items: center;
  }
  .url-copy-row input {
    flex: 1;
  }
  .copy-notice {
    font-size: 0.82rem;
    color: var(--np-color-primary);
    text-align: right;
    font-weight: 600;
  }
  .dialog-warning-centered {
    margin: 8px 0;
    padding: 10px 14px;
    border-radius: var(--np-shape-corner-medium);
    background: var(--np-color-tertiary-container);
    color: var(--np-color-on-tertiary-container);
    text-align: center;
    font-weight: 600;
    font-size: 0.92rem;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
</style>
