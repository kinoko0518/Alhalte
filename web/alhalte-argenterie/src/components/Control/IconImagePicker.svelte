<script lang="ts">
  /**
   * Icon Image Picker Component (Material 3 Expressive Hover Zoom + Asset Picker Trigger).
   *
   * References:
   * - GitHub Issue #52: 汎用画像ピッカーの分離とアイコン拡大表示の実装
   * - Material 3 Expressive Motion tokens: `--np-motion-expressive-fast-spatial`
   */
  import type { AssetItem, AssetMediaType, AssetScope } from '../../utils/assets.js';
  import AssetPickerModal from './AssetPickerModal.svelte';

  let {
    src,
    alt = '',
    size = 64,
    shape = 'rounded',
    title = 'クリックでファイル選択',
    dataTestId = 'icon-image-picker',
    imgTestId,
    isAdmin = false,
    onRequestAssetPicker,
    onSelect,
    onclick,
    class: customClass = '',
  }: {
    src?: string;
    alt?: string;
    size?: number | string;
    shape?: 'circle' | 'rounded';
    title?: string;
    dataTestId?: string;
    imgTestId?: string;
    isAdmin?: boolean;
    onRequestAssetPicker?: (options: {
      mediaType: AssetMediaType;
      initialScope?: AssetScope;
      onSelect: (dataUrl?: string, asset?: AssetItem) => void;
    }) => void;
    onSelect?: (dataUrl?: string, asset?: AssetItem) => void;
    onclick?: (e: MouseEvent) => void;
    class?: string;
  } = $props();

  let isAssetPickerOpen = $state(false);
  let isHovered = $state(false);

  function handleClick(e: MouseEvent) {
    if (onclick) {
      onclick(e);
      return;
    }
    if (onRequestAssetPicker) {
      onRequestAssetPicker({
        mediaType: 'image',
        initialScope: 'personal',
        onSelect: (dataUrl, asset) => {
          handleAssetPicked(dataUrl, asset);
        },
      });
    } else {
      isAssetPickerOpen = true;
    }
  }

  function handleAssetPicked(dataUrl?: string, asset?: AssetItem) {
    onSelect?.(dataUrl, asset);
  }

  let sizePx = $derived(typeof size === 'number' ? `${size}px` : size);
  let resolvedImgTestId = $derived(imgTestId ?? `${dataTestId}-img`);
</script>

<button
  type="button"
  class="icon-image-picker-btn {shape} {customClass}"
  class:is-hovered={isHovered}
  style="--icon-picker-size: {sizePx};"
  onclick={handleClick}
  onmouseenter={() => (isHovered = true)}
  onmouseleave={() => (isHovered = false)}
  onfocus={() => (isHovered = true)}
  onblur={() => (isHovered = false)}
  {title}
  data-testid={dataTestId}
>
  <img
    class="icon-image-picker-img"
    class:hovered={isHovered}
    style="transform: {isHovered ? 'scale(1.1)' : 'scale(1)'}; transition: transform var(--np-motion-expressive-fast-spatial, 200ms ease);"
    src={src || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
    {alt}
    data-testid={resolvedImgTestId}
  />
</button>

{#if isAssetPickerOpen}
  <AssetPickerModal
    open={isAssetPickerOpen}
    initialMediaType="image"
    {isAdmin}
    onSelect={handleAssetPicked}
    onClose={() => (isAssetPickerOpen = false)}
  />
{/if}

<style>
  .icon-image-picker-btn {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    margin: 0;
    outline: none;
    border-radius: var(--np-shape-corner-medium, 12px);
    vertical-align: middle;
  }
  .icon-image-picker-btn.circle {
    border-radius: 50%;
  }
  .icon-image-picker-img {
    width: var(--icon-picker-size, 64px);
    height: var(--icon-picker-size, 64px);
    border-radius: var(--np-shape-corner-medium, 12px);
    object-fit: cover;
    background: var(--np-color-surface-container-highest);
    transition: transform var(--np-motion-expressive-fast-spatial, 200ms ease),
                box-shadow var(--np-motion-expressive-fast-spatial, 200ms ease);
    transform: scale(1);
    will-change: transform;
    display: block;
  }
  .icon-image-picker-btn.circle .icon-image-picker-img {
    border-radius: 50%;
  }
  .icon-image-picker-btn:hover .icon-image-picker-img,
  .icon-image-picker-btn:focus-visible .icon-image-picker-img,
  .icon-image-picker-img.hovered {
    transform: scale(1.1);
    box-shadow: var(--np-elevation-2);
  }
  .icon-image-picker-btn:active .icon-image-picker-img {
    transform: scale(1.04);
  }
</style>
