<script lang="ts">
  /**
   * Asset Picker Overlay Modal Component.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-pantry`, `## alhalte-argenterie`
   * - Issue #28:
   *   ［
   *     （（自分のアセット｜部屋のアセット※部屋の管理者のみ選択可能。選択不能時はグレーアウト）（画像｜音声） 右寄せボタン”アップロード”）
   *     （アセットがカード式で並ぶ。削除ボタンを備える。0番目には常に「NONE」があり、ユーザがこれを選択するとNoneが返る）
   *     ...
   *     （右寄せボタン”キャンセル”）
   *   ］
   */
  import { Button, Card, IconButton, SegmentedButton, TextField } from 'noph-ui';
  import { Icon } from 'noph-ui/icons';
  import {
    addAssetFromFile,
    loadAssets,
    removeAsset,
    subscribeAssets,
    type AssetItem,
    type AssetMediaType,
    type AssetScope,
  } from '../../utils/assets.js';

  let {
    open = true,
    initialMediaType = 'image',
    initialScope = 'personal',
    isAdmin = false,
    onSelect,
    onClose,
  }: {
    open?: boolean;
    initialMediaType?: AssetMediaType;
    initialScope?: AssetScope;
    isAdmin?: boolean;
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
  let directUrl = $state('');

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
    onSelect?.(asset.dataUrl, asset);
    onClose?.();
  }

  function handleSelectDirectUrl() {
    if (!directUrl.trim()) return;
    onSelect?.(directUrl.trim(), undefined);
    onClose?.();
  }

  function handleDeleteAsset(e: MouseEvent, id: string) {
    e.stopPropagation();
    if (scope === 'room' && !isAdmin) return;
    removeAsset(id);
  }

  async function handleFileInputChange(e: Event) {
    const input =
      (e.currentTarget as HTMLInputElement) ??
      (e.target as HTMLInputElement) ??
      fileInputEl;
    const fileList = input?.files ?? fileInputEl?.files;
    if (!fileList || fileList.length === 0) return;
    const files = Array.from(fileList);
    for (const file of files) {
      await addAssetFromFile(file, scope, mediaType);
    }
    try {
      if (input) input.value = '';
    } catch {
      // Ignore read-only files property in test environments
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

      <!-- Search & Direct URL Row -->
      <div class="asset-picker-inputs-row" data-testid="asset-inputs-row">
        <TextField
          variant="outlined"
          type="search"
          label="アセット検索"
          placeholder="アセット名で検索"
          bind:value={searchQuery}
          data-testid="asset-search-input"
          class="asset-search-field"
        />
        <div class="direct-url-group">
          <TextField
            variant="outlined"
            type="url"
            label="URL直接指定"
            placeholder="https://... または data:..."
            bind:value={directUrl}
            data-testid="asset-url-input"
            class="asset-url-field"
          />
          <Button
            variant="tonal"
            disabled={!directUrl.trim()}
            onclick={handleSelectDirectUrl}
            data-testid="asset-apply-url-btn"
          >
            URLを適用
          </Button>
        </div>
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
          <Card variant="outlined">
            <div class="asset-card-body none-card-body">
              <div class="asset-preview none-preview">
                <Icon>block</Icon>
              </div>
              <div class="asset-card-footer">
                <span class="asset-name-label">NONE</span>
              </div>
            </div>
          </Card>
        </div>

        <!-- Subsequent Asset Cards -->
        {#each filteredAssets as asset (asset.id)}
          <div
            class="asset-card-wrapper"
            role="button"
            tabindex="0"
            onclick={() => handleSelectAsset(asset)}
            onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && handleSelectAsset(asset)}
            data-testid="asset-card-{asset.id}"
          >
            <Card variant="outlined">
              <div class="asset-card-body">
                <div class="asset-preview">
                  {#if asset.mediaType === 'image'}
                    <img
                      class="asset-thumb-img"
                      src={asset.dataUrl}
                      alt={asset.name}
                      data-testid="asset-thumb-{asset.id}"
                    />
                  {:else}
                    <div class="audio-preview-icon" data-testid="asset-audio-icon-{asset.id}">
                      <Icon>music_note</Icon>
                    </div>
                  {/if}
                </div>
                <div class="asset-card-footer">
                  <span class="asset-name-label" title={asset.name}>{asset.name}</span>
                  <IconButton
                    variant="text"
                    size="xs"
                    aria-label="{asset.name}を削除"
                    disabled={scope === 'room' && !isAdmin}
                    onclick={(e: MouseEvent) => handleDeleteAsset(e, asset.id)}
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

  .asset-picker-inputs-row {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }

  .asset-picker-inputs-row :global(.asset-search-field) {
    flex: 1;
    min-width: 200px;
  }

  .direct-url-group {
    display: flex;
    gap: 8px;
    align-items: center;
    flex: 2;
    min-width: 280px;
  }

  .direct-url-group :global(.asset-url-field) {
    flex: 1;
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
    cursor: pointer;
    border-radius: var(--np-shape-corner-medium, 12px);
    transition: transform var(--np-motion-expressive-fast-spatial, 150ms);
    outline: none;
  }

  .asset-card-wrapper:focus-visible {
    outline: 2px solid var(--np-color-primary);
    outline-offset: 2px;
  }

  .asset-card-body {
    display: flex;
    flex-direction: column;
    height: 148px;
    box-sizing: border-box;
    overflow: hidden;
  }

  .asset-preview {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--np-color-surface-container-lowest);
    color: var(--np-color-on-surface-variant);
    overflow: hidden;
    min-height: 92px;
  }

  .none-preview {
    background: var(--np-color-surface-container);
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
    gap: 4px;
    padding: 6px 10px;
    min-height: 40px;
    background: var(--np-color-surface-container);
    border-top: 1px solid var(--np-color-outline-variant);
    box-sizing: border-box;
  }

  .asset-name-label {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .asset-picker-bottombar {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    padding-top: 4px;
  }
</style>
