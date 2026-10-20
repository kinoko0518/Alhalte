/**
 * Persisted visibility preference for the footer dice picker.
 *
 * The dice picker is hidden from the "その他" menu of `CommonFooter`. The
 * choice used to live only in the component instance, so it was lost on every
 * site reload (and on a tab switch that remounts the footer). This module
 * stores the preference in `localStorage`, the established mechanism for UI
 * preferences (`alhalte:theme`), so it survives reloads and sign-out.
 *
 * The preference is intentionally *not* room-scoped: it describes how the
 * user likes the footer, not a property of a room.
 *
 * Storage is untrusted: it may be unavailable (SSR), throw (`SecurityError`
 * in private mode, quota errors) or contain a corrupt value. Every failure
 * path falls back to the default (visible) so the footer never breaks.
 *
 * References:
 * - GitHub Issue #246: ダイスピッカーの非表示がサイトリロードでリセットされる
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### ローカル保存`
 */

/** `localStorage` key holding the dice picker visibility (`'true'` / `'false'`). */
export const DICE_PICKER_VISIBLE_STORAGE_KEY = 'alhalte:dice-picker-visible';

/** The dice picker is shown unless the user explicitly hid it. */
export const DEFAULT_DICE_PICKER_VISIBLE = true;

const STORAGE_VISIBLE = 'true';
const STORAGE_HIDDEN = 'false';

/**
 * Reads the persisted dice picker visibility.
 *
 * Returns the default (visible) when nothing is stored, when the stored value
 * is not one this module wrote, or when storage access throws.
 *
 * @param storage Storage override (tests); defaults to `localStorage`.
 */
export function getDicePickerVisible(storage?: Storage): boolean {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return DEFAULT_DICE_PICKER_VISIBLE;

  try {
    const raw = store.getItem(DICE_PICKER_VISIBLE_STORAGE_KEY);
    if (raw === STORAGE_HIDDEN) return false;
    if (raw === STORAGE_VISIBLE) return true;
    // `null` (nothing stored) and any corrupt value fall back to visible.
    return DEFAULT_DICE_PICKER_VISIBLE;
  } catch {
    return DEFAULT_DICE_PICKER_VISIBLE;
  }
}

/**
 * Persists the dice picker visibility. Write failures (private mode, quota)
 * are swallowed: the in-memory toggle already happened, only the next reload
 * would lose it.
 *
 * @param visible Whether the dice picker should be shown.
 * @param storage Storage override (tests); defaults to `localStorage`.
 */
export function setDicePickerVisible(visible: boolean, storage?: Storage): void {
  const store = storage ?? (typeof localStorage !== 'undefined' ? localStorage : undefined);
  if (!store) return;

  try {
    store.setItem(
      DICE_PICKER_VISIBLE_STORAGE_KEY,
      visible ? STORAGE_VISIBLE : STORAGE_HIDDEN,
    );
  } catch {
    // Ignore storage write errors; the preference must not break the footer.
  }
}
