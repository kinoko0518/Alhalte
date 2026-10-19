<script lang="ts">
  /**
   * Home screen with Material 3 Expressive Top App Bar, theme selector,
   * hero banner, visited room cards, ExtendedFAB, and room creation overlay.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### ホーム画面`
   * - GitHub Issue #32: Rich Material 3 Expressive HomeScreen & theme selection
   */
  import { hexToBytes } from '@alhalte/pantry';
  import { AppBar, Button, Card, ExtendedFab, IconButton, TextField } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import type { VisitedRoom } from '../../types/index.js';
  import type { RecentRoom } from '../../utils/recentRooms.js';
  import {
    getRecentRooms,
    recordRoomVisit,
    removeRecentRoom,
    saveRecentRoom,
  } from '../../utils/recentRooms.js';
  import {
    createRoom,
    generateRoomUrl,
    getVisitedRooms,
    removeVisitedRoom,
    RoomApiError,
    saveVisitedRoom,
  } from '../../utils/home.js';
  import { beginGoogleSignIn, isSignedIn } from '../../utils/account.js';
  import AccountMenu from '../Control/AccountMenu.svelte';

  let {
    baseUrl = typeof window !== 'undefined' ? window.location.origin : 'https://example.com',
    fetchFn,
    onEnterRoom,
    onSelectRoom,
  }: {
    baseUrl?: string;
    /** Injectable fetch implementation for tests / embedders. */
    fetchFn?: typeof fetch;
    onEnterRoom?: (room: RecentRoom | VisitedRoom) => void;
    onSelectRoom?: (room: RecentRoom | VisitedRoom) => void;
  } = $props();

  let recentRooms = $state<RecentRoom[]>(getRecentRooms());
  let visitedRooms = $derived(recentRooms);
  let isCreateDialogOpen = $state(false);
  let newRoomName = $state('');
  let generatedUrlData = $state<{ roomId: string; roomKeyHex: string; roomUrl: string } | null>(null);
  let copyNotice = $state('');
  /** True while `POST /api/rooms` is in flight; blocks double submits. */
  let isCreating = $state(false);
  /** Non-retryable / guidance error shown inside the create dialog. */
  let createError = $state('');
  /** True when the user must sign in before a room can be created. */
  let createNeedsSignIn = $state(false);
  /** True while the OAuth redirect is being started from the dialog. */
  let isStartingSignIn = $state(false);

  function loadRooms() {
    recentRooms = getRecentRooms();
  }

  function openCreateDialog() {
    newRoomName = '';
    copyNotice = '';
    createError = '';
    createNeedsSignIn = !isSignedIn();
    generatedUrlData = generateRoomUrl(baseUrl);
    isCreateDialogOpen = true;
  }

  function closeCreateDialog() {
    if (isCreating) return;
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

  function pushRoomToHistory(room: RecentRoom | VisitedRoom) {
    if (typeof window === 'undefined' || !window.history?.pushState) return;
    try {
      const urlStr = room.url || (room as any).roomUrl;
      if (!urlStr) return;
      const parsed = new URL(urlStr, window.location.origin);
      const targetUrl =
        parsed.origin === window.location.origin
          ? parsed.href
          : `${parsed.pathname}${parsed.search}${parsed.hash}`;
      window.history.pushState({ roomId: room.roomId || room.id }, '', targetUrl);
    } catch {
      // Ignore history errors in restricted environments
    }
  }

  /** Starts (or restarts) Google OAuth from the create dialog. */
  async function handleCreateSignIn() {
    if (isStartingSignIn) return;
    isStartingSignIn = true;
    createError = '';
    try {
      await beginGoogleSignIn({ baseUrl, ...(fetchFn ? { fetchFn } : {}) });
    } catch {
      createError = 'サインインを開始できませんでした。右上のアカウントメニューから再試行してください。';
      isStartingSignIn = false;
    }
  }

  /**
   * Registers the room on the server, then enters it.
   *
   * The invitation URL is only built after the server confirms the room; a
   * failure leaves the user on the Home screen and never saves or enters a
   * local-only room (Issue #156).
   */
  async function handleJoinRoom() {
    if (!generatedUrlData || isCreating) return;

    if (!isSignedIn()) {
      createNeedsSignIn = true;
      createError = '';
      return;
    }

    const name = newRoomName.trim() || '名称未設定';
    isCreating = true;
    createError = '';
    createNeedsSignIn = false;

    try {
      const created = await createRoom(name, {
        baseUrl,
        ...(fetchFn ? { fetchFn } : {}),
        roomId: generatedUrlData.roomId,
      });
      const roomKeyBytes = hexToBytes(generatedUrlData.roomKeyHex);
      const invitation = generateRoomUrl(baseUrl, created.room_id, roomKeyBytes);
      const now = Date.now();
      const room: RecentRoom = {
        roomId: created.room_id,
        roomName: created.name || name,
        url: invitation.roomUrl,
        roomUrl: invitation.roomUrl,
        visitedAt: now,
        // Kept for backwards-compatible local storage only; the server role is
        // the authority and `App` re-fetches it on entry.
        isAdmin: created.role === 'gm',
        id: created.room_id,
        name: created.name || name,
        lastVisited: now,
      };
      saveRecentRoom(room);
      saveVisitedRoom(room as any);
      loadRooms();
      isCreateDialogOpen = false;
      pushRoomToHistory(room);
      onSelectRoom?.(room);
      onEnterRoom?.(room);
    } catch (error) {
      if (error instanceof RoomApiError) {
        if (error.kind === 'unauthorized') {
          createNeedsSignIn = true;
          createError = 'サインインが必要です。右上のアカウントメニューからサインインしてください。';
        } else if (error.kind === 'forbidden') {
          createError = '部屋を作成する権限がありません。';
        } else if (error.kind === 'network') {
          createError = 'サーバーと通信できませんでした。時間をおいて再試行してください。';
        } else {
          createError = `部屋を作成できませんでした（HTTP ${error.status ?? 'error'}）。`;
        }
      } else {
        createError = '部屋を作成できませんでした。時間をおいて再試行してください。';
      }
    } finally {
      isCreating = false;
    }
  }

  function handleSelectRoom(room: RecentRoom | VisitedRoom) {
    const updated = recordRoomVisit(room as RecentRoom);
    saveVisitedRoom(room as any);
    loadRooms();
    pushRoomToHistory(updated);
    onSelectRoom?.(updated);
    onEnterRoom?.(updated);
  }

  function handleRemoveRoom(roomId: string) {
    removeRecentRoom(roomId);
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
  <div class="home-account-controls" data-testid="home-account-controls">
    <AccountMenu />
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
      {#if recentRooms.length === 0}
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
        {#each recentRooms as room (room.roomId || room.id)}
          <div
            class="room-card-wrapper"
            data-testid="room-card-{room.roomId || room.id}"
            role="button"
            tabindex="0"
            onclick={() => handleSelectRoom(room)}
            onkeydown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                handleSelectRoom(room);
              }
            }}
          >
            <Card variant="elevated">
              <div class="room-card-body">
                <div class="room-card-top">
                  <div class="room-avatar" aria-hidden="true">
                    {#if room.thumbnailUrl}
                      <img src={room.thumbnailUrl} alt="" class="room-avatar-img" />
                    {:else}
                      <Icon>fort</Icon>
                    {/if}
                  </div>
                  <div class="room-title-group">
                    <h3 class="room-card-name">{room.roomName || room.name}</h3>
                    <div class="room-meta-chips">
                      <span class="room-id-chip">ID: {(room.roomId || room.id || '').slice(0, 8)}</span>
                      {#if room.ownerAccountId}
                        <span class="room-owner-chip">Owner: {room.ownerAccountId}</span>
                      {/if}
                    </div>
                  </div>
                  <IconButton
                    variant="text"
                    size="xs"
                    aria-label="履歴から削除"
                    onclick={(e) => {
                      e.stopPropagation();
                      handleRemoveRoom(room.roomId || room.id || '');
                    }}
                    data-testid="remove-room-{room.roomId || room.id}"
                  >
                    <Icon>delete</Icon>
                  </IconButton>
                </div>

                <div class="room-meta-row">
                  <Icon>schedule</Icon>
                  <p class="room-card-date">
                    最終アクセス: {new Date(room.visitedAt || room.lastVisited || Date.now()).toLocaleString()}
                  </p>
                </div>

                <div class="room-card-actions">
                  <Button
                    variant="filled"
                    size="s"
                    data-testid="enter-room-{room.roomId || room.id}"
                    onclick={(e) => {
                      e.stopPropagation();
                      handleSelectRoom(room);
                    }}
                  >
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
    <div
      class="dialog-overlay"
      data-testid="create-room-overlay"
      onclick={(e) => {
        if (e.target === e.currentTarget) closeCreateDialog();
      }}
    >
      <div class="dialog-content">
        <h2 class="dialog-header-centered" data-testid="dialog-title">部屋の作成</h2>

        <TextField
          id="room-name-input"
          variant="outlined"
          label="ルーム名"
          placeholder="ルーム名"
          bind:value={newRoomName}
          data-testid="room-name-input"
          class="dialog-text-field"
        />

        <div class="url-copy-row">
          <TextField
            variant="outlined"
            label="部屋URL"
            readonly
            value={generatedUrlData?.roomUrl ?? ''}
            data-testid="room-url-input"
            class="url-text-field"
          />
          <Button variant="tonal" onclick={handleCopyLink} data-testid="copy-link-button">
            リンクをコピー
          </Button>
        </div>
        {#if copyNotice}
          <div class="copy-notice" data-testid="copy-notice">{copyNotice}</div>
        {/if}

        {#if createNeedsSignIn}
          <div class="create-signin-guidance" data-testid="create-room-signin-guidance">
            <p class="create-signin-text">
              部屋を作成するにはサインインが必要です。
            </p>
            <Button
              variant="tonal"
              onclick={handleCreateSignIn}
              disabled={isStartingSignIn}
              data-testid="create-room-signin-button"
            >
              サインイン
            </Button>
          </div>
        {/if}

        {#if createError}
          <div class="create-error" role="alert" data-testid="create-room-error">
            {createError}
          </div>
        {/if}

        <p class="dialog-warning-centered" data-testid="dialog-warning">
          このリンクを紛失した場合、部屋へのアクセスは永久に失われます！
        </p>

        <div class="dialog-actions">
          <Button variant="text" onclick={closeCreateDialog} disabled={isCreating}>
            キャンセル
          </Button>
          <Button
            variant="filled"
            onclick={handleJoinRoom}
            disabled={isCreating || createNeedsSignIn}
            aria-busy={isCreating}
            data-testid="join-created-room-button"
          >
            {isCreating ? '作成中…' : '入室する'}
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
  .home-account-controls {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding-inline-end: 8px;
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
  .room-card-wrapper {
    cursor: pointer;
    transition: transform var(--np-motion-expressive-fast-spatial, 200ms ease);
    outline: none;
    border-radius: var(--np-shape-corner-large, 16px);
  }
  .room-card-wrapper:hover {
    transform: translateY(-2px);
  }
  .room-card-wrapper:focus-visible {
    box-shadow: 0 0 0 3px var(--np-color-primary);
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
    overflow: hidden;
  }
  .room-avatar-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .room-meta-chips {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .room-owner-chip {
    font-size: 0.72rem;
    padding: 1px 6px;
    border-radius: var(--np-shape-corner-full);
    background: var(--np-color-surface-container-high);
    color: var(--np-color-on-surface-variant);
    font-family: monospace;
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
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
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
  .dialog-content :global(.dialog-text-field) {
    width: 100%;
  }
  .url-copy-row {
    display: flex;
    flex-direction: row;
    gap: 8px;
    align-items: center;
    width: 100%;
  }
  .url-copy-row :global(.url-text-field) {
    flex: 1;
  }
  .copy-notice {
    font-size: 0.82rem;
    color: var(--np-color-primary);
    text-align: right;
    font-weight: 600;
  }
  .create-signin-guidance {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    border-radius: var(--np-shape-corner-medium);
    background: var(--np-color-surface-container-highest);
    color: var(--np-color-on-surface);
  }
  .create-signin-text {
    margin: 0;
    font-size: 0.86rem;
    line-height: 1.4;
  }
  .create-error {
    padding: 10px 14px;
    border-radius: var(--np-shape-corner-medium);
    background: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
    font-size: 0.86rem;
    font-weight: 600;
    line-height: 1.4;
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
