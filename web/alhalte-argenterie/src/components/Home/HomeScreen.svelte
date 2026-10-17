<script lang="ts">
  /**
   * Home screen with visited room cards and room creation overlay.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### ホーム画面`
   */
  import { Button, Card, Dialog, TextField } from 'noph-ui';
  import type { VisitedRoom } from '../../types/index.js';
  import {
    generateRoomUrl,
    getVisitedRooms,
    saveVisitedRoom,
  } from '../../utils/home.js';

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

  function loadRooms() {
    visitedRooms = getVisitedRooms();
  }

  $effect(() => {
    loadRooms();
  });

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
</script>

<div class="home-container" data-testid="home-screen">
  <header class="home-header">
    <h1 class="home-title">Alhalte</h1>
    <p class="home-subtitle">訪れたことのある部屋</p>
  </header>

  <main class="rooms-grid" data-testid="visited-rooms-list">
    {#if visitedRooms.length === 0}
      <div class="no-rooms-message" data-testid="no-rooms-message">
        訪れた部屋の履歴はありません
      </div>
    {:else}
      {#each visitedRooms as room (room.id)}
        <div class="room-card-wrapper" data-testid="room-card-{room.id}">
          <Card variant="outlined">
            <div class="room-card-body">
              <h2 class="room-card-name">{room.name}</h2>
              <p class="room-card-date">
                最終アクセス: {new Date(room.lastVisited).toLocaleString()}
              </p>
              <div class="room-card-actions">
                <Button variant="filled" onclick={() => handleSelectRoom(room)}>
                  入室する
                </Button>
              </div>
            </div>
          </Card>
        </div>
      {/each}
    {/if}
  </main>

  <div class="fab-container">
    <Button
      variant="filled"
      onclick={openCreateDialog}
      data-testid="create-room-button"
    >
      部屋の新規作成
    </Button>
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
    padding: 24px;
    box-sizing: border-box;
    background-color: var(--md-sys-color-surface, #fbf8fd);
    color: var(--md-sys-color-on-surface, #1b1b1f);
    font-family: Roboto, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  }
  .home-header {
    margin-bottom: 24px;
  }
  .home-title {
    margin: 0;
    font-size: 2rem;
    font-weight: 700;
  }
  .home-subtitle {
    margin: 4px 0 0;
    color: var(--md-sys-color-on-surface-variant, #45464f);
  }
  .rooms-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 16px;
  }
  .no-rooms-message {
    padding: 48px;
    text-align: center;
    color: var(--md-sys-color-on-surface-variant, #45464f);
    grid-column: 1 / -1;
  }
  .room-card-body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .room-card-name {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 600;
  }
  .room-card-date {
    margin: 0;
    font-size: 0.85rem;
    color: var(--md-sys-color-on-surface-variant, #45464f);
  }
  .room-card-actions {
    margin-top: 12px;
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
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .dialog-content {
    background: var(--md-sys-color-surface-container-high, #ece6f0);
    color: var(--md-sys-color-on-surface, #1b1b1f);
    padding: 24px;
    border-radius: 28px;
    width: 90%;
    max-width: 520px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .dialog-header-centered {
    margin: 0;
    text-align: center;
    font-size: 1.5rem;
    font-weight: 600;
  }
  .dialog-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field-label {
    font-size: 0.9rem;
    font-weight: 500;
  }
  .single-line-input {
    width: 100%;
    padding: 12px 16px;
    border: 1px solid var(--md-sys-color-outline, #757780);
    border-radius: 12px;
    font-size: 1rem;
    background: var(--md-sys-color-surface, #fff);
    box-sizing: border-box;
  }
  .readonly-input {
    background: var(--md-sys-color-surface-container, #f3edf7);
    color: var(--md-sys-color-on-surface-variant, #45464f);
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
    font-size: 0.8rem;
    color: var(--md-sys-color-primary, #6750a4);
    text-align: right;
  }
  .dialog-warning-centered {
    margin: 8px 0;
    text-align: center;
    color: var(--md-sys-color-error, #b3261e);
    font-weight: 600;
    font-size: 0.95rem;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
</style>
