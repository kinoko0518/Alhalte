<script lang="ts">
  /**
   * Field panel with pan/zoom canvas and initiative-ordered character status overlay.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### フィールド画面`
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
  import { calculateDimensionsAndCrop } from '../../utils/sceneSettings.js';

  let {
    roomName = 'Alhalte Room',
    scene,
    characters = [],
    onTransformChange,
  }: {
    roomName?: string;
    scene: Scene;
    characters?: Character[];
    onTransformChange?: (sceneId: string, transform: ViewTransform) => void;
  } = $props();

  let isDragging = $state(false);
  let dragStart = $state({ x: 0, y: 0 });
  let initialPinchDistance = $state<number | null>(null);

  // Characters in this scene, sorted by initiative descending
  let sceneCharacters = $derived.by(() => {
    const present = characters.filter((c) => scene.members.includes(c.id));
    return sortCharactersByInitiative(present);
  });

  // Background styling from Juices
  let bgStyle = $derived.by(() => {
    if (!scene.juices.bgImage) return '';
    const crop = calculateDimensionsAndCrop({
      naturalWidth: scene.juices.bgWidth || 1920,
      naturalHeight: scene.juices.bgHeight || 1080,
      width: scene.juices.bgWidth,
      height: scene.juices.bgHeight,
    });
    return `width: ${crop.width}px; height: ${crop.height}px; background-image: url('${scene.juices.bgImage}'); background-size: cover;`;
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
      <div class="field-background" style={bgStyle} data-testid="field-background"></div>
    {/if}

    <!-- Pieces placed on map -->
    {#each sceneCharacters as char (char.id)}
      {#if char.placedAsPiece}
        <div
          class="field-piece"
          data-testid="piece-{char.id}"
          style="left: {char.piecePosition?.x ?? 100}px; top: {char.piecePosition?.y ?? 100}px;"
        >
          <img
            class="piece-avatar"
            src={char.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
            alt={char.name}
          />
          <span class="piece-label">{char.name}</span>
        </div>
      {/if}
    {/each}
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
    background: #1e1e24;
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
    box-shadow: 0 0 20px rgba(0, 0, 0, 0.5);
  }
  .field-piece {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: center;
    cursor: grab;
  }
  .piece-avatar {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    border: 2px solid #fff;
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.4);
    background: #444;
  }
  .piece-label {
    font-size: 0.75rem;
    color: #fff;
    background: rgba(0, 0, 0, 0.7);
    padding: 2px 6px;
    border-radius: 4px;
    margin-top: 4px;
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
    border-radius: 12px;
    background: rgba(28, 27, 31, 0.85);
    color: #fff;
    backdrop-filter: blur(8px);
    width: fit-content;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
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
    border-radius: 12px;
    background: rgba(28, 27, 31, 0.85);
    backdrop-filter: blur(8px);
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
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
    border-radius: 8px;
    object-fit: cover;
    background: #555;
  }
  .char-initiative-badge {
    position: absolute;
    right: -4px;
    bottom: -4px;
    background: #6750a4;
    color: #fff;
    font-size: 0.7rem;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 6px;
    border: 1px solid #fff;
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
    color: #fff;
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
    background: rgba(255, 255, 255, 0.15);
    border-radius: 4px;
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
    background: #4caf50;
    opacity: 0.6;
    transition: width 0.2s ease;
  }
  .status-label-text {
    position: relative;
    z-index: 1;
    font-size: 0.68rem;
    font-weight: 600;
    color: #fff;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.8);
    white-space: nowrap;
    padding: 0 4px;
  }
</style>
