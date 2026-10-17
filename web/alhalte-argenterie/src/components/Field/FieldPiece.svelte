<script lang="ts">
  /**
   * Field piece component representing a character on the field canvas.
   *
   * Uses Pointer Events and Pointer Capture for smooth dragging without browser interference.
   * Completely suppresses browser image ghost drag-and-drop.
   *
   * References:
   * - ARCHITECTURE.md: `## alhalte-argenterie -> ### フィールド画面`
   * - GitHub Issue #61: 盤面駒のドラッグ操作性改善とゴースト抑制
   */
  import type { Character } from '../../types/index.js';

  let {
    character,
    scale = 1.0,
    onMove,
    onDragStart,
    onDragEnd,
  }: {
    character: Character;
    scale?: number;
    onMove?: (position: { x: number; y: number }) => void;
    onDragStart?: () => void;
    onDragEnd?: () => void;
  } = $props();

  let isDragging = $state(false);
  let dragPointerId = $state<number | null>(null);
  let startPointerPos = $state({ x: 0, y: 0 });
  let startPiecePos = $state({ x: 0, y: 0 });
  let currentPos = $state<{ x: number; y: number } | null>(null);

  let displayX = $derived(
    isDragging && currentPos ? currentPos.x : (character.piecePosition?.x ?? 100)
  );
  let displayY = $derived(
    isDragging && currentPos ? currentPos.y : (character.piecePosition?.y ?? 100)
  );

  function handlePointerDown(e: PointerEvent) {
    if (e.button !== 0) return; // Only primary button (left click / touch)
    e.stopPropagation();

    isDragging = true;
    dragPointerId = e.pointerId;
    startPointerPos = { x: e.clientX, y: e.clientY };

    const initialX = character.piecePosition?.x ?? 100;
    const initialY = character.piecePosition?.y ?? 100;
    startPiecePos = { x: initialX, y: initialY };
    currentPos = { x: initialX, y: initialY };

    const target = e.currentTarget as HTMLElement | null;
    if (target && typeof target.setPointerCapture === 'function') {
      try {
        target.setPointerCapture(e.pointerId);
      } catch {
        // Ignore if pointer capture fails in special environments
      }
    }

    onDragStart?.();
  }

  function handlePointerMove(e: PointerEvent) {
    if (!isDragging || (dragPointerId !== null && e.pointerId !== dragPointerId)) return;
    e.stopPropagation();

    const currentScale = scale > 0 ? scale : 1.0;
    const dx = (e.clientX - startPointerPos.x) / currentScale;
    const dy = (e.clientY - startPointerPos.y) / currentScale;

    currentPos = {
      x: Math.round(startPiecePos.x + dx),
      y: Math.round(startPiecePos.y + dy),
    };
  }

  function handlePointerUp(e: PointerEvent) {
    if (!isDragging || (dragPointerId !== null && e.pointerId !== dragPointerId)) {
      return;
    }
    e.stopPropagation();

    const target = e.currentTarget as HTMLElement | null;
    if (target && typeof target.releasePointerCapture === 'function') {
      try {
        target.releasePointerCapture(e.pointerId);
      } catch {
        // Ignore
      }
    }

    const finalPos = currentPos ?? {
      x: character.piecePosition?.x ?? 100,
      y: character.piecePosition?.y ?? 100,
    };

    if (!character.piecePosition) {
      character.piecePosition = { x: finalPos.x, y: finalPos.y };
    } else {
      character.piecePosition.x = finalPos.x;
      character.piecePosition.y = finalPos.y;
    }

    isDragging = false;
    dragPointerId = null;
    currentPos = null;

    onMove?.(finalPos);
    onDragEnd?.();
  }

  function handlePointerCancel(e: PointerEvent) {
    handlePointerUp(e);
  }
</script>

<div
  class="field-piece"
  class:is-dragging={isDragging}
  data-testid="piece-{character.id}"
  style="left: {displayX}px; top: {displayY}px;"
  onpointerdown={handlePointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onpointercancel={handlePointerCancel}
  ondragstart={(e) => e.preventDefault()}
  draggable="false"
  role="presentation"
>
  <img
    class="piece-avatar"
    src={character.iconUrl || 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"/>'}
    alt={character.name}
    draggable="false"
    ondragstart={(e) => e.preventDefault()}
  />
  <span class="piece-label">{character.name}</span>
</div>

<style>
  .field-piece {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: center;
    cursor: grab;
    user-select: none;
    -webkit-user-select: none;
    touch-action: none;
    z-index: 10;
  }

  .field-piece.is-dragging {
    cursor: grabbing;
    z-index: 20;
    filter: drop-shadow(0 8px 16px rgba(0, 0, 0, 0.25));
    transform: scale(1.05);
    transition: transform var(--np-motion-expressive-fast-effects, 150ms cubic-bezier(0.2, 0, 0, 1));
  }

  .piece-avatar {
    width: 48px;
    height: 48px;
    border-radius: var(--np-shape-corner-full, 9999px);
    border: 2px solid var(--np-color-primary, #6750a4);
    box-shadow: var(--np-elevation-2, 0 1px 3px rgba(0, 0, 0, 0.2));
    background: var(--np-color-surface-container-highest, #e6e0e9);
    user-select: none;
    -webkit-user-select: none;
    -webkit-user-drag: none;
    pointer-events: none;
  }

  .piece-label {
    font-size: 0.75rem;
    color: var(--np-color-on-surface, #1d1b20);
    background: color-mix(in srgb, var(--np-color-surface-container-high, #ece6f0) 90%, transparent);
    padding: 2px 8px;
    border-radius: var(--np-shape-corner-extra-small, 4px);
    margin-top: 4px;
    box-shadow: var(--np-elevation-1, 0 1px 2px rgba(0, 0, 0, 0.1));
    user-select: none;
    -webkit-user-select: none;
    pointer-events: none;
    white-space: nowrap;
  }
</style>
