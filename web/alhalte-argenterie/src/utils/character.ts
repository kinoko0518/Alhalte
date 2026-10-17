/**
 * Character data helpers.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> キャラクタータブ`
 */

export const MAX_DESCRIPTION_LENGTH = 120;
export const DESCRIPTION_ELLIPSIS = '……';

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
