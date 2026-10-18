<script lang="ts">
  /**
   * Field panel with pan/zoom canvas and initiative-ordered character status overlay.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### フィールド画面`
   * - GitHub Issue #32: Material 3 semantic color tokens (`--np-color-*`)
   */
  import type { Character, Scene, ViewTransform } from '../../types/index.js';
  import {
    applyPan,
    applyPinch,
    applyZoom,
    chunkStatusesInPairs,
    formatStatusItem,
    sortCharactersByInitiative,
  } from '../../utils/field.js';
  import { calculateBackgroundDimensions } from '../../utils/backgroundDimension.js';
  import AccountMenu from '../Control/AccountMenu.svelte';
  import FieldPiece from './FieldPiece.svelte';

  let {
    roomName = 'Alhalte Room',
    scene,
    characters = [],
    origW: propOrigW,
    origH: propOrigH,
    onTransformChange,
    onPieceMove,
  }: {
    roomName?: string;
    scene: Scene;
    characters?: Character[];
    origW?: number;
    origH?: number;
    onTransformChange?: (sceneId: string, transform: ViewTransform) => void;
    onPieceMove?: (characterId: string, position: { x: number; y: number }) => void;
  } = $props();

  function handlePieceMove(characterId: string, position: { x: number; y: number }) {
    const target = characters.find((c) => c.id === characterId);
    if (target) {
      if (!target.piecePosition) {
        target.piecePosition = { ...position };
      } else {
        target.piecePosition.x = position.x;
        target.piecePosition.y = position.y;
      }
    }
    onPieceMove?.(characterId, position);
  }

  let isDragging = $state(false);
  let dragStart = $state({ x: 0, y: 0 });
  let initialPinchDistance = $state<number | null>(null);

  // Original image dimensions (origW, origH)
  let origW = $state<number>(1920);
  let origH = $state<number>(1080);

  $effect(() => {
    if (propOrigW && propOrigW > 0) origW = propOrigW;
    if (propOrigH && propOrigH > 0) origH = propOrigH;
  });

  $effect(() => {
    const src = scene.juices.bgImage;
    if (!src) return;

    if (typeof Image !== 'undefined') {
      const img = new Image();
      img.onload = () => {
        if (img.naturalWidth > 0 && img.naturalHeight > 0) {
          origW = img.naturalWidth;
          origH = img.naturalHeight;
        }
      };
      img.src = src;
    }
  });

  // Characters in this scene, sorted by initiative descending
  let sceneCharacters = $derived.by(() => {
    const present = characters.filter((c) => scene.members.includes(c.id));
    return sortCharactersByInitiative(present);
  });

  // Background dimension and crop calculation from Juices
  let bgDimensions = $derived.by(() => {
    if (!scene.juices.bgImage) return null;
    return calculateBackgroundDimensions({
      origW,
      origH,
      width: scene.juices.bgWidth,
      height: scene.juices.bgHeight,
    });
  });

  let bgStyle = $derived.by(() => {
    if (!scene.juices.bgImage || !bgDimensions) return '';
    return `width: ${bgDimensions.width}px; height: ${bgDimensions.height}px; background-image: url('${scene.juices.bgImage}'); background-size: cover; background-position: center;`;
  });


  function handleWheel(e: WheelEvent) {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.1 : 0.9;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const center = {
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
    };
    const updated = applyZoom(scene.viewTransform, factor, center);
    scene.viewTransform = updated;
    onTransformChange?.(scene.id, updated);
  }

  function handlePointerDown(e: PointerEvent) {
    if (e.button !== 0) return; // Primary left button or touch
    isDragging = true;
    dragStart = { x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function handlePointerMove(e: PointerEvent) {
    if (!isDragging) return;
    const dx = e.clientX - dragStart.x;
    const dy = e.clientY - dragStart.y;
    dragStart = { x: e.clientX, y: e.clientY };
    const updated = applyPan(scene.viewTransform, dx, dy);
    scene.viewTransform = updated;
    onTransformChange?.(scene.id, updated);
  }

  function handlePointerUp(e: PointerEvent) {
    isDragging = false;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      // Ignore
    }
  }

  // Touch pinch handlers
  function handleTouchStart(e: TouchEvent) {
    if (e.touches.length === 2) {
      const dx = e.touches[0].clientX - e.touches[1].clientX;
      const dy = e.touches[0].clientY - e.touches[1].clientY;
      initialPinchDistance = Math.hypot(dx, dy);
    }
  }

  function handleTouchMove(e: TouchEvent) {
    if (e.touches.length === 2 && initialPinchDistance !== null) {
      e.preventDefault();
      const dx = e.touches[0].clientX - e.touches[1].clientX;
      const dy = e.touches[0].clientY - e.touches[1].clientY;
      const currentDist = Math.hypot(dx, dy);
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const center = {
        x: (e.touches[0].clientX + e.touches[1].clientX) / 2 - rect.left,
        y: (e.touches[0].clientY + e.touches[1].clientY) / 2 - rect.top,
      };
      const updated = applyPinch(scene.viewTransform, initialPinchDistance, currentDist, center);
      initialPinchDistance = currentDist;
      scene.viewTransform = updated;
      onTransformChange?.(scene.id, updated);
    }
  }

  function handleTouchEnd(e: TouchEvent) {
    if (e.touches.length < 2) {
      initialPinchDistance = null;
    }
  }
</script>

<div
  class="field-container"
  data-testid="field-panel"
  onwheel={handleWheel}
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onpointercancel={handlePointerUp}
  ontouchstart={handleTouchStart}
  ontouchmove={handleTouchMove}
  ontouchend={handleTouchEnd}
  role="region"
  aria-label="Field Board"
>
  <!-- Interactive transformed canvas layer -->
  <div
    class="field-canvas"
    data-testid="field-canvas"
    style="transform: translate({scene.viewTransform.x}px, {scene.viewTransform.y}px) scale({scene.viewTransform.scale});"
  >
    {#if scene.juices.bgImage}
      <div
        class="field-background"
        style={bgStyle}
        data-testid="field-background"
        data-bg-width={bgDimensions?.width}
        data-bg-height={bgDimensions?.height}
      >
        <img
          src={scene.juices.bgImage}
          alt=""
          class="field-bg-loader"
          onload={(e) => {
            const el = e.currentTarget as HTMLImageElement;
            if (el.naturalWidth > 0 && el.naturalHeight > 0) {
              origW = el.naturalWidth;
              origH = el.naturalHeight;
            }
          }}
        />
      </div>
    {/if}

    <!-- Pieces placed on map -->
    {#each sceneCharacters as char (char.id)}
      {#if char.placedAsPiece}
        <FieldPiece
          character={char}
          scale={scene.viewTransform.scale}
          onMove={(pos) => handlePieceMove(char.id, pos)}
        />
      {/if}
    {/each}
  </div>

  <!-- Top-right overlay: account control (Issue #116) -->
  <div class="field-top-right-layer" data-testid="field-account-layer">
    <AccountMenu />
  </div>

  <!-- Top-left overlay: Room name and initiative-ordered character list -->
  <aside class="field-top-left-layer" data-testid="field-overlay-layer">
    <div class="room-name-badge" data-testid="room-name-badge">
      {roomName}
    </div>

    <div class="initiative-character-list" data-testid="initiative-character-list">
      {#each sceneCharacters as char (char.id)}
        <div class="character-status-entry" data-testid="char-entry-{char.id}">
          <div class="char-icon-wrapper">
            <img
              class="char-icon"
              src={char.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
              alt={char.name}
            />
            <span class="char-initiative-badge" data-testid="initiative-badge-{char.id}">
              {char.initiative}
            </span>
          </div>

          <div class="char-info-and-statuses">
            <span class="char-name">{char.name}</span>

            {#if !char.hideStatus}
              <div class="status-rows">
                {#each chunkStatusesInPairs(char.statuses) as pair, rIdx (rIdx)}
                  <div class="status-pair-row">
                    {#each pair as status (status.label)}
                      {@const formatted = formatStatusItem(status)}
                      <div class="status-item-bar" data-testid="status-item-{char.id}-{status.label}">
                        <!-- Background progress bar -->
                        <div
                          class="status-progress-fill"
                          style="width: {formatted.percentage}%;"
                        ></div>
                        <!-- Foreground label text -->
                        <span class="status-label-text">{formatted.text}</span>
                      </div>
                    {/each}
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  </aside>
</div>

<style>
  .field-container {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--np-color-surface-dim);
    user-select: none;
    touch-action: none;
  }
  .field-canvas {
    position: absolute;
    top: 0;
    left: 0;
    transform-origin: 0 0;
    width: 100%;
    height: 100%;
  }
  .field-background {
    position: absolute;
    top: 0;
    left: 0;
    background-repeat: no-repeat;
    box-shadow: var(--np-elevation-3);
  }
  .field-bg-loader {
    display: none;
  }
  .field-top-right-layer {
    position: absolute;
    top: 16px;
    right: 16px;
    z-index: 60;
    pointer-events: auto;
  }
  .field-top-left-layer {
    position: absolute;
    top: 16px;
    left: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 320px;
    z-index: 50;
    pointer-events: none;
  }
  .room-name-badge {
    pointer-events: auto;
    font-size: 1.1rem;
    font-weight: 700;
    padding: 8px 16px;
    border-radius: var(--np-shape-corner-medium);
    background: color-mix(in srgb, var(--np-color-surface-container-high) 88%, transparent);
    color: var(--np-color-on-surface);
    backdrop-filter: blur(8px);
    width: fit-content;
    box-shadow: var(--np-elevation-2);
    border: 1px solid var(--np-color-outline-variant);
  }
  .initiative-character-list {
    pointer-events: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .character-status-entry {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: var(--np-shape-corner-medium);
    background: color-mix(in srgb, var(--np-color-surface-container-high) 88%, transparent);
    backdrop-filter: blur(8px);
    box-shadow: var(--np-elevation-1);
    border: 1px solid var(--np-color-outline-variant);
  }
  .char-icon-wrapper {
    position: relative;
    width: 40px;
    height: 40px;
    flex-shrink: 0;
  }
  .char-icon {
    width: 40px;
    height: 40px;
    border-radius: var(--np-shape-corner-small);
    object-fit: cover;
    background: var(--np-color-surface-container-highest);
  }
  .char-initiative-badge {
    position: absolute;
    right: -4px;
    bottom: -4px;
    background: var(--np-color-primary);
    color: var(--np-color-on-primary);
    font-size: 0.7rem;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: var(--np-shape-corner-extra-small);
    border: 1px solid var(--np-color-surface);
  }
  .char-info-and-statuses {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .char-name {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status-rows {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .status-pair-row {
    display: flex;
    gap: 6px;
  }
  .status-item-bar {
    position: relative;
    flex: 1;
    height: 18px;
    background: var(--np-color-surface-container-highest);
    border-radius: var(--np-shape-corner-extra-small);
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .status-progress-fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: var(--np-color-primary-container);
    transition: width var(--np-motion-expressive-fast-effects);
  }
  .status-label-text {
    position: relative;
    z-index: 1;
    font-size: 0.68rem;
    font-weight: 600;
    color: var(--np-color-on-surface);
    white-space: nowrap;
    padding: 0 4px;
  }
</style>
