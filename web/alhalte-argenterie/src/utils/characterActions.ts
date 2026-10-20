/**
 * Pure role / ownership / view matrix for the character list (Issue #228).
 *
 * The character tab is the single place where a viewer, a participant and an
 * admin see different row actions, and the differences depend on three
 * independent axes:
 *
 * - the active view (`scene` / `mine` / `all`),
 * - the caller's role (`admin` / `participant` / `viewer` / `denied`),
 * - whether the row belongs to the caller (`std::owner_account`).
 *
 * Keeping the matrix here (instead of inline `{#if}` chains in the component)
 * makes it table-testable and keeps the component free of role logic.
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
 * - GitHub Issue #228
 */

import type { Character } from '../types/index.js';
import type { RoomRole } from './session.js';

/** Character list view selected by the M3 segmented button. */
export type CharacterView = 'scene' | 'mine' | 'all';

/** Row action kinds, in their required left-to-right order. */
export type CharacterRowAction = 'move' | 'focus' | 'piece' | 'more';

/** Labels of the segmented button options (Issue #228). */
export const CHARACTER_VIEW_LABELS: Readonly<Record<CharacterView, string>> = {
  scene: 'シーン内のキャラクター',
  mine: '自分のキャラクター',
  all: 'すべてのキャラクター',
};

/**
 * Normalizes an account ID to its exact decimal string form.
 *
 * `std::owner_account` is a `u64`, so IDs may exceed `Number.MAX_SAFE_INTEGER`.
 * The decoder keeps the exact decimal string and this helper normalizes the
 * forms a caller may compare (`'0123'`, `123`, `123n` -> `'123'`). Values that
 * cannot be represented exactly are rejected instead of being rounded.
 */
export function normalizeAccountId(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === 'bigint') {
    return value >= 0n ? value.toString() : null;
  }
  if (typeof value === 'number') {
    if (!Number.isInteger(value) || value < 0 || !Number.isSafeInteger(value)) return null;
    return String(value);
  }
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!/^\d+$/.test(trimmed)) return null;
    // BigInt strips leading zeros without losing precision for long IDs.
    return BigInt(trimmed).toString();
  }
  return null;
}

/**
 * Returns true when `character` is owned by `accountId`.
 *
 * A missing / malformed owner is treated as "not owned" (Issue #228): the
 * server is the only one allowed to assign ownership, and a character without
 * an owner must not accidentally become manageable by everyone.
 */
export function isCharacterOwnedBy(
  character: Pick<Character, 'ownerAccountId'> | null | undefined,
  accountId: unknown,
): boolean {
  const owner = normalizeAccountId(character?.ownerAccountId);
  const caller = normalizeAccountId(accountId);
  return owner !== null && caller !== null && owner === caller;
}

/**
 * Views available to a role.
 *
 * - viewer: only the current scene (no segmented button at all),
 * - participant: the current scene and their own characters,
 * - admin: all three views,
 * - denied: never rendered (GateScreen), but behave like a viewer defensively.
 */
export function availableCharacterViews(role: RoomRole): CharacterView[] {
  if (role === 'admin') return ['scene', 'mine', 'all'];
  if (role === 'participant') return ['scene', 'mine'];
  return ['scene'];
}

/** Inputs of {@link visibleRowActions}. */
export interface VisibleRowActionsOptions {
  role: RoomRole;
  view: CharacterView;
  character: Pick<Character, 'id' | 'ownerAccountId'>;
  /** Account ID of the caller (`getSavedAccountId()`), or `null` when signed out. */
  currentAccountId?: string | null;
  /** True when the character is currently a member of the active scene. */
  inCurrentScene: boolean;
  /**
   * True when the character belongs to some scene, i.e. there is a scene to
   * switch to with 「視点を合わせる」. A character in no scene has no target and
   * must not get an inert focus action.
   */
  hasCharacterScene: boolean;
}

/**
 * Returns the row actions of one character, in required left-to-right order:
 * `move` (別シーンに配置), `focus` (視点を合わせる), `piece` (駒として配置 /
 * 盤面から削除), `more` (その他).
 *
 * Matrix (Issue #228):
 *
 * | view  | viewer | participant            | admin                                   |
 * | ----- | ------ | ---------------------- | --------------------------------------- |
 * | scene | none   | own: piece, more       | every row: move, piece, more            |
 * | mine  | n/a    | own: more (+focus)     | own: more (+focus)                      |
 * | all   | n/a    | n/a                    | every row: more (+focus)                |
 *
 * `(+focus)` means the character is not in the current scene **and** belongs to
 * a scene (a 未所属 character has no scene to switch to). Admin is never
 * ownership-gated; a missing owner only hides a participant's actions.
 */
export function visibleRowActions({
  role,
  view,
  character,
  currentAccountId,
  inCurrentScene,
  hasCharacterScene,
}: VisibleRowActionsOptions): CharacterRowAction[] {
  if (role === 'denied' || role === 'viewer') return [];

  const owned = isCharacterOwnedBy(character, currentAccountId);
  const focus: CharacterRowAction[] =
    !inCurrentScene && hasCharacterScene ? ['focus'] : [];

  if (view === 'scene') {
    if (role === 'admin') return ['move', 'piece', 'more'];
    return owned ? ['piece', 'more'] : [];
  }

  if (view === 'mine') {
    return owned ? [...focus, 'more'] : [];
  }

  // `all` is admin-only; participant / viewer never reach this tab.
  if (role === 'admin') return [...focus, 'more'];
  return [];
}
