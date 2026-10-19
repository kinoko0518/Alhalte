<script lang="ts">
  /**
   * Asset Picker Overlay Modal Component.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-pantry`, `## alhalte-argenterie -> ### アセット`
   * - Issue #28:
   *   ［
   *     （（自分のアセット｜部屋のアセット※部屋の管理者のみ選択可能。選択不能時はグレーアウト）（画像｜音声） 右寄せボタン”アップロード”）
   *     （アセットがカード式で並ぶ。削除ボタンを備える。0番目には常に「NONE」があり、ユーザがこれを選択するとNoneが返る）
   *     ...
   *     （右寄せボタン”キャンセル”）
   *   ］
   * - Issue #109 / #141: アップロード中はカード内に波線 `LinearProgress`、
   *   失敗時はカード内に失敗文言と再試行、モーダルを開くたびに部屋アセットを同期。
   */
  import { untrack } from 'svelte';
  import { Button, Card, IconButton, LinearProgress, Search, SegmentedButton } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import {
    ASSET_STATE_KEY_PREFIX,
    ASSET_STATE_SCENE_ID,
    formatFileSize,
    loadAssets,
    removeAsset,
    retryAssetUpload,
    subscribeAssets,
    syncRoomAssets,
    uploadAssetFromFile,
    type AssetItem,
    type AssetMediaType,
    type AssetScope,
  } from '../../utils/assets.js';
  import type { RoomSocket } from '../../utils/roomSocket.js';

  let {
    open = true,
    initialMediaType = 'image',
    initialScope = 'personal',
    isAdmin = false,
    roomSocket = null,
    roomKey,
    roomId,
    onSelect,
    onClose,
  }: {
    open?: boolean;
    initialMediaType?: AssetMediaType;
    initialScope?: AssetScope;
    isAdmin?: boolean;
    /** Room socket used to sync / publish room asset metadata (Issue #141). */
    roomSocket?: RoomSocket | null;
    /** 32-byte room key; defaults to the URL fragment (`#<64 hex>`). */
    roomKey?: Uint8Array | null;
    /** Room id, only used for diagnostic logging. */
    roomId?: string;
    onSelect?: (dataUrl: string | undefined, asset?: AssetItem) => void;
    onClose?: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let scope = $state<AssetScope>(initialScope === 'room' && isAdmin ? 'room' : 'personal');
  // svelte-ignore state_referenced_locally
  let mediaType = $state<AssetMediaType>(initialMediaType);
  let allAssets = $state<AssetItem[]>(loadAssets());
  let fileInputEl = $state<HTMLInputElement | null>(null);
  let searchQuery = $state('');
  /** Latest room-scope delete error; the card stays until the retry succeeds. */
  let deleteError = $state<string | undefined>(undefined);
  // Sync runs once per open transition (not on every reactive tick).
  let lastSyncOpen = false;
  let syncInFlight = false;

  // Sync initialMediaType when modal opens or prop changes
  $effect(() => {
    mediaType = initialMediaType;
  });

  // Ensure non-admins cannot stay on 'room' scope
  $effect(() => {
    if (!isAdmin && scope === 'room') {
      scope = 'personal';
    }
  });

  // Subscribe to asset store updates
  $effect(() => {
    allAssets = loadAssets();
    const unsubscribe = subscribeAssets((updated) => {
      allAssets = updated;
    });
    return unsubscribe;
  });

  // Sync room assets every time the picker opens (Issue #141). A failure is
  // intentionally quiet: the local cache stays usable (the report records it).
  $effect(() => {
    if (!open) {
      lastSyncOpen = false;
      return;
    }
    if (lastSyncOpen) return;
    lastSyncOpen = true;
    untrack(() => {
      void runRoomAssetSync();
    });
  });

  async function runRoomAssetSync(): Promise<void> {
    if (!roomSocket || syncInFlight) return;
    syncInFlight = true;
    try {
      const result = await syncRoomAssets(roomSocket, roomKey);
      if (result.syncFailed) {
        console.warn(
          `Asset sync failed${roomId ? ` (room ${roomId})` : ''}: ${result.error ?? 'unknown error'}`,
        );
      }
    } catch (error) {
      // `syncRoomAssets` reports failures in its result; this is a safety net so
      // opening the picker can never break the UI.
      console.warn('Asset sync failed unexpectedly:', error);
    } finally {
      syncInFlight = false;
    }
  }

  let filteredAssets = $derived(
    allAssets.filter((item) => {
      if (item.scope !== scope || item.mediaType !== mediaType) return false;
      if (searchQuery.trim() && !item.name.toLowerCase().includes(searchQuery.trim().toLowerCase())) {
        return false;
      }
      return true;
    }),
  );

  let scopeOptions = $derived([
    {
      value: 'personal',
      label: '自分のアセット',
      selected: scope === 'personal',
      onclick: () => {
        scope = 'personal';
      },
    },
    {
      value: 'room',
      label: '部屋のアセット',
      disabled: !isAdmin,
      selected: scope === 'room',
      onclick: () => {
        if (isAdmin) {
          scope = 'room';
        }
      },
    },
  ]);

  let mediaTypeOptions = $derived([
    {
      value: 'image',
      label: '画像',
      selected: mediaType === 'image',
      onclick: () => {
        mediaType = 'image';
      },
    },
    {
      value: 'audio',
      label: '音声',
      selected: mediaType === 'audio',
      onclick: () => {
        mediaType = 'audio';
      },
    },
  ]);

  function handleSelectNone() {
    onSelect?.(undefined, undefined);
    onClose?.();
  }

  function handleSelectAsset(asset: AssetItem) {
    // A transfer in flight owns the card; a failed download may have no preview.
    if (asset.uploadState === 'uploading') return;
    if (!asset.dataUrl) return;
    onSelect?.(asset.dataUrl, asset);
    onClose?.();
  }

  async function handleDeleteAsset(e: MouseEvent, asset: AssetItem) {
    e.stopPropagation();
    if (asset.uploadState === 'uploading') return;
    if (asset.scope === 'room' && !isAdmin) return;
    deleteError = undefined;
    if (
      asset.scope === 'room' &&
      roomSocket &&
      typeof roomSocket.requestDeleteState === 'function'
    ) {
      // Publish the removal first: a failed delete keeps the card so the user
      // can retry instead of silently resurrecting the asset on the next sync.
      try {
        await roomSocket.requestDeleteState(
          ASSET_STATE_SCENE_ID,
          `${ASSET_STATE_KEY_PREFIX}${asset.id}`,
        );
      } catch (error) {
        console.warn('Failed to delete the room asset metadata:', error);
        deleteError = 'アセットの削除に失敗しました';
        return;
      }
    }
    removeAsset(asset.id);
  }

  async function handleRetryUpload(e: MouseEvent, asset: AssetItem) {
    e.stopPropagation();
    if (asset.uploadState === 'uploading') return;
    try {
      await retryAssetUpload(asset, { roomKey, roomSocket });
    } catch (error) {
      console.warn('Asset retry failed unexpectedly:', error);
    }
  }

  async function handleFileInputChange(e: Event) {
    const input =
      (e.currentTarget as HTMLInputElement) ??
      (e.target as HTMLInputElement) ??
      fileInputEl;
    const fileList = input?.files ?? fileInputEl?.files;
    if (!fileList || fileList.length === 0) return;
    const files = Array.from(fileList);
    // Reset before uploading so picking the same file again still fires `change`.
    try {
      if (input) input.value = '';
    } catch {
      // Ignore read-only files property in test environments
    }

    const targetScope = scope;
    const targetMediaType = mediaType;
    // Sequential: each card appears immediately (uploading state) and later
    // files never compete with an in-flight one for the single progress slot.
    for (const file of files) {
      try {
        await uploadAssetFromFile(file, targetScope, targetMediaType, {
          roomKey,
          roomSocket,
        });
      } catch (error) {
        // `uploadAssetFromFile` reports failures on the card; this only guards
        // against unexpected programming errors.
        console.warn('Asset upload failed unexpectedly:', error);
      }
    }
  }

  $effect(() => {
    const el = fileInputEl;
    if (!el) return;
    el.addEventListener('change', handleFileInputChange);
    return () => {
      el.removeEventListener('change', handleFileInputChange);
    };
  });
</script>

{#if open}
  <div
    class="asset-picker-overlay"
    role="dialog"
    aria-modal="true"
    aria-label="アセット選択"
    data-testid="asset-picker-modal"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose?.();
    }}
  >
    <div class="asset-picker-surface">
      <!-- Top Bar:
        （（自分のアセット｜部屋のアセット※部屋の管理者のみ選択可能。選択不能時はグレーアウト）（画像｜音声） 右寄せボタン”アップロード”）
      -->
      <div class="asset-picker-topbar" data-testid="asset-picker-topbar">
        <div class="segmented-group">
          <SegmentedButton
            name="asset-scope"
            options={scopeOptions}
            bind:group={scope}
            data-testid="asset-scope-segmented"
          />
          <SegmentedButton
            name="asset-media-type"
            options={mediaTypeOptions}
            bind:group={mediaType}
            data-testid="asset-type-segmented"
          />
        </div>

        <div class="upload-action">
          {#snippet uploadIcon()}<Icon>upload</Icon>{/snippet}
          <Button
            variant="filled"
            start={uploadIcon}
            disabled={scope === 'room' && !isAdmin}
            onclick={() => fileInputEl?.click()}
            data-testid="asset-upload-btn"
          >
            アップロード
          </Button>
          <input
            bind:this={fileInputEl}
            type="file"
            class="hidden-file-input"
            accept={mediaType === 'image' ? 'image/*' : 'audio/*'}
            multiple
            data-testid="asset-file-input"
          />
        </div>
      </div>

      <!-- Search Row: Material 3 Expressive Search -->
      <div class="asset-picker-search-row" data-testid="asset-search-row">
        <Search
          variant="contained"
          view="docked"
          label="アセット検索"
          placeholder="アセット名で検索"
          bind:value={searchQuery}
          inputAttributes={{ 'data-testid': 'asset-search-input' }}
          class="asset-search-bar"
          data-testid="asset-search-container"
        />
      </div>

      <!-- Middle Grid:
        （アセットがカード式で並ぶ。削除ボタンを備える。0番目には常に「NONE」があり、ユーザがこれを選択するとNoneが返る）
      -->
      <div class="asset-cards-grid" data-testid="asset-cards-grid">
        <!-- Index 0: Always "NONE" -->
        <div
          class="asset-card-wrapper none-card-wrapper"
          role="button"
          tabindex="0"
          onclick={handleSelectNone}
          onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && handleSelectNone()}
          data-testid="asset-card-none"
        >
          <Card variant="filled">
            <div class="asset-card-body none-card-body">
              <div class="asset-preview none-preview">
                <Icon>block</Icon>
              </div>
              <div class="asset-card-footer">
                <div class="asset-card-text">
                  <span class="asset-name-label">NONE</span>
                  <span class="asset-size-label">-</span>
                </div>
              </div>
            </div>
          </Card>
        </div>

        <!-- Subsequent Asset Cards -->
        {#each filteredAssets as asset (asset.id)}
          <div
            class="asset-card-wrapper"
            class:asset-card-uploading={asset.uploadState === 'uploading'}
            role="button"
            tabindex={asset.uploadState === 'uploading' ? -1 : 0}
            aria-busy={asset.uploadState === 'uploading'}
            aria-disabled={asset.uploadState === 'uploading'}
            onclick={() => handleSelectAsset(asset)}
            onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && handleSelectAsset(asset)}
            data-testid="asset-card-{asset.id}"
          >
            <Card variant="filled">
              <div class="asset-card-body">
                <div class="asset-preview">
                  {#if asset.mediaType === 'image'}
                    {#if asset.dataUrl}
                      <img
                        class="asset-thumb-img"
                        src={asset.dataUrl}
                        alt={asset.name}
                        data-testid="asset-thumb-{asset.id}"
                      />
                    {:else}
                      <div class="audio-preview-icon" data-testid="asset-image-fallback-{asset.id}">
                        <Icon>image_not_supported</Icon>
                      </div>
                    {/if}
                  {:else}
                    <div class="audio-preview-icon" data-testid="asset-audio-icon-{asset.id}">
                      <Icon>music_note</Icon>
                    </div>
                  {/if}

                  {#if asset.uploadState === 'uploading'}
                    {@const progressKnown =
                      typeof asset.uploadProgress === 'number' &&
                      Number.isFinite(asset.uploadProgress)}
                    <!-- Real XHR progress only: determinate when reported, wavy
                      indeterminate until the first upload event. -->
                    <div
                      class="asset-transfer asset-transfer-progress"
                      data-testid="asset-upload-progress-{asset.id}"
                    >
                      <LinearProgress
                        wavy
                        track={false}
                        indeterminate={!progressKnown}
                        value={asset.uploadProgress ?? 0}
                        max={1}
                        aria-label="{asset.name}をアップロード中"
                      />
                    </div>
                  {:else if asset.uploadState === 'failed'}
                    <div
                      class="asset-transfer asset-transfer-failed"
                      data-testid="asset-upload-failed-{asset.id}"
                    >
                      <span class="asset-transfer-message" title={asset.uploadError ?? ''}>
                        アップロードに失敗しました
                      </span>
                      <Button
                        variant="text"
                        size="xs"
                        onclick={(e: MouseEvent) => handleRetryUpload(e, asset)}
                        data-testid="asset-retry-btn-{asset.id}"
                      >
                        再試行
                      </Button>
                    </div>
                  {/if}
                </div>
                <div class="asset-card-footer">
                  <div class="asset-card-text">
                    <span class="asset-name-label" title={asset.name}>{asset.name}</span>
                    <span class="asset-size-label">{formatFileSize(asset.size)}</span>
                  </div>
                  <IconButton
                    variant="text"
                    size="xs"
                    aria-label="{asset.name}を削除"
                    disabled={(scope === 'room' && !isAdmin) ||
                      asset.uploadState === 'uploading'}
                    onclick={(e: MouseEvent) => handleDeleteAsset(e, asset)}
                    data-testid="asset-delete-btn-{asset.id}"
                  >
                    <Icon>delete</Icon>
                  </IconButton>
                </div>
              </div>
            </Card>
          </div>
        {/each}
      </div>

      <!-- Bottom Bar:
        （右寄せボタン”キャンセル”）
      -->
      <!-- Room-scope delete failure: the card stays so the user can retry. -->
      {#if deleteError}
        <div class="asset-picker-notice" role="alert" data-testid="asset-picker-notice">
          {deleteError}
        </div>
      {/if}

      <div class="asset-picker-bottombar" data-testid="asset-picker-bottombar">
        <Button
          variant="text"
          onclick={() => onClose?.()}
          data-testid="asset-cancel-btn"
        >
          キャンセル
        </Button>
      </div>
    </div>
  </div>
{/if}

<style>
  .asset-picker-overlay {
    position: fixed;
    inset: 0;
    background: var(--np-color-scrim, rgba(0, 0, 0, 0.32));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10000;
    padding: 16px;
    box-sizing: border-box;
  }

  .asset-picker-surface {
    background: var(--np-color-surface-container-high);
    color: var(--np-color-on-surface);
    border-radius: var(--np-shape-corner-extra-large, 28px);
    box-shadow: var(--np-elevation-3);
    padding: 24px;
    width: 100%;
    max-width: 720px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    gap: 20px;
    box-sizing: border-box;
  }

  .asset-picker-topbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }

  .segmented-group {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }

  .upload-action {
    margin-left: auto;
    display: flex;
    align-items: center;
  }

  .hidden-file-input {
    display: none;
  }

  .asset-picker-search-row {
    width: 100%;
  }

  :global(.asset-search-bar) {
    --np-search-pane-margin: 0px;
    --np-search-width: 100%;
    width: 100%;
  }

  .asset-cards-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 12px;
    overflow-y: auto;
    min-height: 180px;
    max-height: 50vh;
    padding: 4px;
  }

  .asset-card-wrapper {
    display: flex;
    flex-direction: column;
    width: 100%;
    cursor: pointer;
    border-radius: var(--np-shape-corner-medium, 12px);
    transition: transform var(--np-motion-expressive-fast-spatial, 150ms);
    outline: none;
  }

  .asset-card-wrapper:focus-visible {
    outline: 2px solid var(--np-color-primary);
    outline-offset: 2px;
  }

  .asset-card-wrapper :global(.np-card-container) {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .asset-card-wrapper :global(.np-card-content) {
    margin: 0;
    padding: 0;
    min-width: 100%;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .asset-card-body {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 154px;
    box-sizing: border-box;
    overflow: hidden;
  }

  .asset-preview {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--np-color-surface-container-lowest);
    color: var(--np-color-on-surface-variant);
    overflow: hidden;
    min-height: 96px;
    width: 100%;
  }

  .asset-card-uploading {
    cursor: progress;
  }

  /* Per-card transfer affordances (Issue #141): a real-progress bar while
     uploading, and a failure strip with a retry action. */
  .asset-transfer {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    box-sizing: border-box;
    background: var(--np-color-surface-container);
    color: var(--np-color-on-surface);
  }

  .asset-transfer-progress {
    bottom: 0;
    padding: 8px 10px;
    border-top: 1px solid var(--np-color-outline-variant);
  }

  .asset-transfer-failed {
    inset: 0;
    align-items: center;
    justify-content: center;
    padding: 8px;
    text-align: center;
    background: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
  }

  .asset-transfer-message {
    font-size: 0.7rem;
    font-weight: 600;
    line-height: 1.3;
  }

  .asset-picker-notice {
    padding: 8px 12px;
    border-radius: var(--np-shape-corner-small, 8px);
    background: var(--np-color-error-container);
    color: var(--np-color-on-error-container);
    font-size: 0.8rem;
    font-weight: 600;
  }

  .none-preview {
    background: var(--np-color-surface-container-low);
    color: var(--np-color-on-surface-variant);
  }

  .asset-thumb-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .audio-preview-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--np-color-primary);
    --np-icon-size: 2.25rem;
  }

  .asset-card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 6px 10px;
    min-height: 48px;
    background: var(--np-color-surface-container);
    border-top: 1px solid var(--np-color-outline-variant);
    box-sizing: border-box;
  }

  .asset-card-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
    gap: 2px;
  }

  .asset-name-label {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .asset-size-label {
    font-size: 0.75rem;
    font-weight: 400;
    color: var(--np-color-on-surface-variant);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .asset-picker-bottombar {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    padding-top: 4px;
  }
</style>
