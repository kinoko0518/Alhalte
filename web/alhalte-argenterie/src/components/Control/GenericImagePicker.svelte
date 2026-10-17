<script lang="ts">
  /**
   * Generic Image Picker Component (Material 3 Expressive Carousel + Asset Picker Trigger).
   *
   * References:
   * - GitHub Issue #52: 汎用画像ピッカーの分離とアイコン拡大表示の実装
   * - M3 Expressive Carousel: https://m3.material.io/components/carousel/overview
   */
  import { Carousel, CarouselItem } from 'noph-ui';
  import {
    loadAssets,
    subscribeAssets,
    type AssetItem,
    type AssetMediaType,
    type AssetScope,
  } from '../../utils/assets.js';
  import AssetPickerModal from './AssetPickerModal.svelte';

  let {
    value,
    label = '画像アセット',
    aspectRatio = 16 / 9,
    showNone = true,
    isAdmin = false,
    imageAssets,
    onSelect,
    onOpenPicker,
    onRequestAssetPicker,
    dataTestId = 'generic-image-picker',
    dataCarouselTestId = 'generic-image-carousel',
    itemTestIdPrefix = 'carousel-item',
    noneTestId = 'carousel-item-none',
    moreButtonTestId = 'more-images-btn',
    cardTestId = 'generic-image-card',
    legacyOpenBtnTestId = 'open-bg-picker-btn',
    class: customClass = '',
  }: {
    value?: string;
    label?: string;
    aspectRatio?: number;
    showNone?: boolean;
    isAdmin?: boolean;
    imageAssets?: AssetItem[];
    onSelect?: (dataUrl?: string, asset?: AssetItem) => void;
    onOpenPicker?: () => void;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
    dataTestId?: string;
    dataCarouselTestId?: string;
    itemTestIdPrefix?: string;
    noneTestId?: string;
    moreButtonTestId?: string;
    cardTestId?: string;
    legacyOpenBtnTestId?: string;
    class?: string;
  } = $props();

  let isAssetPickerOpen = $state(false);
  let internalAssets = $state<AssetItem[]>(loadAssets());

  $effect(() => {
    internalAssets = loadAssets();
    const unsubscribe = subscribeAssets((updated) => {
      internalAssets = updated;
    });
    return unsubscribe;
  });

  let effectiveAssets = $derived(
    imageAssets ??
      internalAssets.filter(
        (a) => a.mediaType === 'image' && (isAdmin || a.scope === 'personal'),
      ),
  );

  function handleSelect(dataUrl?: string, asset?: AssetItem) {
    onSelect?.(dataUrl, asset);
  }

  function handleOpenPickerClick(e?: MouseEvent) {
    e?.stopPropagation();
    if (onOpenPicker) {
      onOpenPicker();
      return;
    }
    if (onRequestAssetPicker) {
      onRequestAssetPicker({
        mediaType: 'image',
        initialScope: 'personal',
        onSelect: (dataUrl, asset) => {
          handleSelect(dataUrl, asset);
        },
      });
    } else {
      isAssetPickerOpen = true;
    }
  }

  function handleAssetPicked(dataUrl?: string, asset?: AssetItem) {
    handleSelect(dataUrl, asset);
  }
</script>

<div class="generic-image-picker {customClass}" data-testid={dataTestId}>
  <!-- M3 Expressive Multi-Browse Carousel -->
  <div class="generic-carousel-wrapper" data-testid={dataCarouselTestId}>
    <Carousel variant="multi-browse" {label}>
      {#if showNone}
        <CarouselItem
          type="button"
          label="NONE"
          {aspectRatio}
          onclick={() => handleSelect(undefined)}
          data-testid={noneTestId}
        />
      {/if}
      {#each effectiveAssets as asset (asset.id)}
        <CarouselItem
          type="button"
          image={asset.dataUrl}
          label={asset.name}
          {aspectRatio}
          onclick={() => handleSelect(asset.dataUrl, asset)}
          data-testid="{itemTestIdPrefix}-{asset.id}"
        />
      {/each}
    </Carousel>
  </div>

  <!-- Bottom-right action: primary color clickable text/button "+ その他の画像" -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="picker-footer-row"
    data-testid={cardTestId}
    onclick={handleOpenPickerClick}
  >
    <button
      type="button"
      class="more-images-btn"
      onclick={handleOpenPickerClick}
      data-testid={moreButtonTestId}
    >
      + その他の画像
    </button>
    <!-- Hidden alias button for backward compatibility with existing tests expecting open-bg-picker-btn -->
    {#if legacyOpenBtnTestId}
      <button
        type="button"
        class="legacy-hidden-btn"
        onclick={handleOpenPickerClick}
        data-testid={legacyOpenBtnTestId}
        aria-hidden="true"
        tabindex="-1"
      >
        + その他の画像
      </button>
    {/if}
  </div>

  {#if isAssetPickerOpen}
    <AssetPickerModal
      open={isAssetPickerOpen}
      initialMediaType="image"
      {isAdmin}
      onSelect={handleAssetPicked}
      onClose={() => (isAssetPickerOpen = false)}
    />
  {/if}
</div>

<style>
  .generic-image-picker {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
    position: relative;
  }
  .generic-carousel-wrapper {
    --np-carousel-item-height: 96px;
    --np-carousel-item-width: 140px;
    --np-carousel-item-container-color: var(--np-color-surface-container-high);
    --np-carousel-item-outline-width: 1px;
    --np-carousel-item-outline-color: var(--np-color-outline-variant);
    width: 100%;
  }
  .picker-footer-row {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    padding: 2px 4px 0 0;
  }
  .more-images-btn {
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--np-color-primary);
    font-size: 0.875rem;
    font-weight: 500;
    padding: 4px 8px;
    border-radius: var(--np-shape-corner-small, 8px);
    transition: background-color var(--np-motion-expressive-fast-effects, 150ms ease),
                color var(--np-motion-expressive-fast-effects, 150ms ease);
    text-decoration: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .more-images-btn:hover {
    background: var(--np-color-primary-container);
    color: var(--np-color-on-primary-container);
  }
  .more-images-btn:focus-visible {
    outline: 2px solid var(--np-color-primary);
    outline-offset: 2px;
  }
  .legacy-hidden-btn {
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
</style>
