/**
 * Table-driven tests for the pure character role / view / ownership matrix
 * (Issue #228).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
 * - GitHub Issue #228
 */
import { describe, it, expect } from 'vitest';
import {
  availableCharacterViews,
  CHARACTER_VIEW_LABELS,
  isCharacterOwnedBy,
  normalizeAccountId,
  visibleRowActions,
  type CharacterRowAction,
  type CharacterView,
} from '../src/utils/characterActions.js';
import type { RoomRole } from '../src/utils/session.js';

describe('characterActions (Issue #228)', () => {
  describe('normalizeAccountId', () => {
    it.each([
      ['1001', '1001'],
      [' 1001 ', '1001'],
      ['0001001', '1001'],
      ['9007199254740993', '9007199254740993'],
      [1001, '1001'],
      [1001n, '1001'],
      [0, '0'],
      [0n, '0'],
    ])('normalizes %s to %s', (value, expected) => {
      expect(normalizeAccountId(value)).toBe(expected);
    });

    it.each([null, undefined, '', '   ', 'abc', '-1', '1.5', 1.5, -1, NaN, Infinity, {}, []])(
      'rejects %s',
      (value) => {
        expect(normalizeAccountId(value)).toBeNull();
      },
    );

    it('rejects numbers above MAX_SAFE_INTEGER instead of rounding them', () => {
      expect(normalizeAccountId(Number.MAX_SAFE_INTEGER + 2)).toBeNull();
    });
  });

  describe('isCharacterOwnedBy', () => {
    it('compares the exact decimal owner', () => {
      expect(isCharacterOwnedBy({ ownerAccountId: '1001' }, '1001')).toBe(true);
      expect(isCharacterOwnedBy({ ownerAccountId: '1001' }, 1001)).toBe(true);
      expect(isCharacterOwnedBy({ ownerAccountId: '01001' }, '1001')).toBe(true);
      expect(isCharacterOwnedBy({ ownerAccountId: '1001' }, '1002')).toBe(false);
    });

    it('treats a missing owner or missing account as not owned', () => {
      expect(isCharacterOwnedBy({}, '1001')).toBe(false);
      expect(isCharacterOwnedBy({ ownerAccountId: '1001' }, null)).toBe(false);
      expect(isCharacterOwnedBy(null, '1001')).toBe(false);
    });

    it('keeps IDs above 2^53 exact', () => {
      expect(isCharacterOwnedBy({ ownerAccountId: '9007199254740993' }, '9007199254740993')).toBe(
        true,
      );
      // A rounded JS number must not match the exact owner.
      expect(
        isCharacterOwnedBy({ ownerAccountId: '9007199254740993' }, 9007199254740993 as never),
      ).toBe(false);
    });
  });

  describe('availableCharacterViews', () => {
    it.each([
      ['viewer', ['scene']],
      ['participant', ['scene', 'mine']],
      ['admin', ['scene', 'mine', 'all']],
      ['denied', ['scene']],
    ] as Array<[RoomRole, CharacterView[]]>)('%s -> %j', (role, expected) => {
      expect(availableCharacterViews(role)).toEqual(expected);
    });

    it('has a label for every view', () => {
      expect(CHARACTER_VIEW_LABELS).toEqual({
        scene: 'シーン内のキャラクター',
        mine: '自分のキャラクター',
        all: 'すべてのキャラクター',
      });
    });
  });

  describe('visibleRowActions', () => {
    const owned = { id: '42', ownerAccountId: '1001' };
    const other = { id: '43', ownerAccountId: '2002' };
    const unowned = { id: '44' };

    function actions(
      role: RoomRole,
      view: CharacterView,
      character: { id: string; ownerAccountId?: string },
      inCurrentScene: boolean,
      hasCharacterScene = true,
    ): CharacterRowAction[] {
      return visibleRowActions({
        role,
        view,
        character,
        currentAccountId: '1001',
        inCurrentScene,
        hasCharacterScene,
      });
    }

    type MatrixRow = [
      RoomRole,
      CharacterView,
      typeof owned,
      boolean,
      boolean,
      CharacterRowAction[],
    ];

    // spec: Issue #228 権限 × ビュー × 所有の行アクション表
    it.each([
      // viewer: rows exist, no actions.
      ['viewer', 'scene', owned, true, true, []],
      ['viewer', 'scene', other, true, true, []],
      // participant, scene view: own characters only piece + more.
      ['participant', 'scene', owned, true, true, ['piece', 'more']],
      ['participant', 'scene', other, true, true, []],
      ['participant', 'scene', unowned, true, true, []],
      // admin, scene view: move + piece + more for every row.
      ['admin', 'scene', owned, true, true, ['move', 'piece', 'more']],
      ['admin', 'scene', other, true, true, ['move', 'piece', 'more']],
      ['admin', 'scene', unowned, true, true, ['move', 'piece', 'more']],
      // participant, mine view: own characters always more, focus when elsewhere.
      ['participant', 'mine', owned, true, true, ['more']],
      ['participant', 'mine', owned, false, true, ['focus', 'more']],
      ['participant', 'mine', owned, false, false, ['more']],
      ['participant', 'mine', other, true, true, []],
      // admin, mine view: same as participant (mine = own characters).
      ['admin', 'mine', owned, true, true, ['more']],
      ['admin', 'mine', owned, false, true, ['focus', 'more']],
      ['admin', 'mine', owned, false, false, ['more']],
      ['admin', 'mine', other, true, true, []],
      // admin, all view: every row more, focus when not in a scene-attached row.
      ['admin', 'all', owned, true, true, ['more']],
      ['admin', 'all', other, true, true, ['more']],
      ['admin', 'all', unowned, false, true, ['focus', 'more']],
      ['admin', 'all', unowned, false, false, ['more']],
      // all is admin-only; participant / viewer get nothing defensively.
      ['participant', 'all', owned, true, true, []],
      ['viewer', 'all', owned, true, true, []],
      ['denied', 'scene', owned, true, true, []],
    ] as MatrixRow[])(
      '%s / %s / %s / inScene=%s hasScene=%s -> %j',
      (role, view, character, inCurrentScene, hasCharacterScene, expected) => {
        expect(actions(role, view, character, inCurrentScene, hasCharacterScene)).toEqual(
          expected,
        );
      },
    );

    it('keeps the required left-to-right order for a row with every action', () => {
      // `scene` has no focus; `all` has no move / piece. The combined order is
      // encoded in the union of the matrix.
      expect(actions('admin', 'scene', owned, false)).toEqual(['move', 'piece', 'more']);
      expect(actions('admin', 'all', owned, false)).toEqual(['focus', 'more']);
    });
  });
});
