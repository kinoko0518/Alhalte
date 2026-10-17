/**
 * Field panel utilities: pan/zoom transformations, initiative sorting, and status displays.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### フィールド画面`
 */

import type { Character, CharacterStatus, ViewTransform } from '../types/index.js';

export interface ZoomBounds {
  min: number;
  max: number;
}

const DEFAULT_ZOOM_BOUNDS: ZoomBounds = {
  min: 0.1,
  max: 10.0,
};

/**
 * Applies a pan displacement (delta X, delta Y) to the current view transform.
 */
export function applyPan(
  current: ViewTransform,
  dx: number,
  dy: number,
): ViewTransform {
  return {
    x: current.x + dx,
    y: current.y + dy,
    scale: current.scale,
  };
}

/**
 * Applies a zoom scaling to the current view transform, optionally around a focal center point.
 */
export function applyZoom(
  current: ViewTransform,
  factor: number,
  center?: { x: number; y: number },
  bounds: ZoomBounds = DEFAULT_ZOOM_BOUNDS,
): ViewTransform {
  if (factor <= 0 || !Number.isFinite(factor)) {
    return current;
  }

  const rawScale = current.scale * factor;
  const clampedScale = Math.min(bounds.max, Math.max(bounds.min, rawScale));
  const effectiveFactor = clampedScale / current.scale;

  if (center) {
    return {
      x: center.x - (center.x - current.x) * effectiveFactor,
      y: center.y - (center.y - current.y) * effectiveFactor,
      scale: clampedScale,
    };
  }

  return {
    x: current.x,
    y: current.y,
    scale: clampedScale,
  };
}

/**
 * Applies a pinch gesture zoom calculation from two touch points.
 */
export function applyPinch(
  current: ViewTransform,
  initialDistance: number,
  currentDistance: number,
  center?: { x: number; y: number },
  bounds: ZoomBounds = DEFAULT_ZOOM_BOUNDS,
): ViewTransform {
  if (initialDistance <= 0 || currentDistance <= 0) {
    return current;
  }
  const factor = currentDistance / initialDistance;
  return applyZoom(current, factor, center, bounds);
}

/**
 * Sorts characters in descending order of initiative.
 *
 * ARCHITECTURE.md: "キャラクターはイニシアチブ順に並んでいる"
 */
export function sortCharactersByInitiative(characters: Character[]): Character[] {
  return [...characters].sort((a, b) => b.initiative - a.initiative);
}

/**
 * Formats a character status display element.
 *
 * Format: `ラベル名 最大値/現在地` with progress percentage (current / max).
 */
export function formatStatusItem(status: CharacterStatus): {
  label: string;
  current: number;
  max: number;
  text: string;
  ratio: number;
  percentage: number;
} {
  const text = `${status.label} ${status.max}/${status.current}`;
  const ratio = status.max > 0 ? status.current / status.max : 0;
  const percentage = Math.min(100, Math.max(0, ratio * 100));

  return {
    label: status.label,
    current: status.current,
    max: status.max,
    text,
    ratio,
    percentage,
  };
}

/**
 * Chunks character statuses into rows of 2 items per row.
 *
 * ARCHITECTURE.md: "※2つで1行"
 */
export function chunkStatusesInPairs(statuses: CharacterStatus[]): CharacterStatus[][] {
  const rows: CharacterStatus[][] = [];
  for (let i = 0; i < statuses.length; i += 2) {
    rows.push(statuses.slice(i, i + 2));
  }
  return rows;
}
