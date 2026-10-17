/**
 * Chat palette partial match suggestions and replacement logic.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 部屋内 -> コントロールパネル共通フッター`
 */

/**
 * Parses raw multiline chat palette text into distinct, non-empty lines.
 */
export function parseChatPaletteLines(paletteText: string): string[] {
  if (!paletteText) return [];
  return paletteText
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/**
 * Finds all lines in the chat palette that partially match the input text query.
 *
 * @param query The text typed into the input field
 * @param paletteText The full multiline chat palette of the selected character
 * @returns Array of matching palette lines
 */
export function findChatPaletteMatches(query: string, paletteText: string): string[] {
  const trimmed = query.trim();
  if (trimmed.length === 0) {
    return [];
  }

  const lines = parseChatPaletteLines(paletteText);
  const lowerQuery = trimmed.toLowerCase();

  return lines.filter((line) => line.toLowerCase().includes(lowerQuery));
}

/**
 * Replaces the partially matched portion in `input` with the chosen `selectedLine`.
 *
 * ARCHITECTURE.md: "サジェストはタップすれば部分一致している部分をその全体に置換"
 *
 * @param input The current value of the input field
 * @param matchedPart The substring that matched (e.g. the query or token)
 * @param selectedLine The entire line from the chat palette
 */
export function replaceChatPaletteMatch(
  input: string,
  matchedPart: string,
  selectedLine: string,
): string {
  const trimmedMatched = matchedPart.trim();
  if (!trimmedMatched) {
    return selectedLine;
  }

  // Find position of matchedPart (case-insensitive search)
  const lowerInput = input.toLowerCase();
  const lowerMatched = trimmedMatched.toLowerCase();
  const index = lowerInput.indexOf(lowerMatched);

  if (index !== -1) {
    return (
      input.slice(0, index) +
      selectedLine +
      input.slice(index + trimmedMatched.length)
    );
  }

  // Fallback: if not directly substring, replace whole input
  return selectedLine;
}
