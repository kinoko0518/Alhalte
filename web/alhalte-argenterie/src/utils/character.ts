/**
 * Character data helpers.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> キャラクタータブ`
 */

import type { Character } from '../types/index.js';

export const MAX_DESCRIPTION_LENGTH = 120;
export const DESCRIPTION_ELLIPSIS = '……';

/**
 * Creates a default Character object with empty statuses, parameters, and isNarrative: false.
 */
export function createDefaultCharacter(overrides?: Partial<Character>): Character {
  return {
    id: `char-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    name: '新規キャラクター',
    initiative: 0,
    description: '',
    reference: '',
    statuses: [],
    parameters: [],
    chatPalette: '',
    hideStatus: false,
    isNarrative: false,
    placedAsPiece: false,
    ...overrides,
  };
}

/**
 * Truncates a character description to 120 characters, appending "……" if longer.
 *
 * ARCHITECTURE.md: "説明※120文字以降は……として省略"
 */
export function truncateDescription(
  description: string,
  maxLength = MAX_DESCRIPTION_LENGTH,
  ellipsis = DESCRIPTION_ELLIPSIS,
): string {
  if (!description) return '';
  if (description.length <= maxLength) {
    return description;
  }
  return description.slice(0, maxLength) + ellipsis;
}
