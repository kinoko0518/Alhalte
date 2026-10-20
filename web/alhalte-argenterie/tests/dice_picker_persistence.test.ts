import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import CommonFooter from '../src/components/Control/CommonFooter.svelte';
import type { Character } from '../src/types/index.js';
import {
  DEFAULT_DICE_PICKER_VISIBLE,
  DICE_PICKER_VISIBLE_STORAGE_KEY,
  getDicePickerVisible,
  setDicePickerVisible,
} from '../src/utils/dicePickerPreference.js';

const sampleCharacters: Character[] = [
  {
    id: 'char-1',
    name: '探索者アルス',
    initiative: 12,
    description: '敏腕探偵',
    statuses: [{ label: 'HP', current: 12, max: 12 }],
    parameters: [{ label: 'STR', value: 14 }],
    chatPalette: '1D100<=50 【目星】',
    hideStatus: false,
  },
];

function mountFooter(target: HTMLElement) {
  return mount(CommonFooter, {
    target,
    props: {
      activeTab: 'log',
      characters: sampleCharacters,
      selectedCharacterId: 'char-1',
    },
  });
}

async function flush() {
  await new Promise((resolve) => setTimeout(resolve, 20));
}

/** Opens the "その他" menu and clicks the dice picker toggle item. */
async function toggleDicePicker(target: HTMLElement) {
  const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="footer-more-button"]');
  moreBtn?.click();
  await flush();

  const toggleItem = target.querySelector<HTMLElement>(
    '[data-testid="toggle-dice-picker-menu-item"]',
  );
  toggleItem?.click();
  await flush();
}

/** Opens the "その他" menu and returns the toggle item's label. */
async function dicePickerMenuLabel(target: HTMLElement): Promise<string | undefined> {
  const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="footer-more-button"]');
  moreBtn?.click();
  await flush();
  const toggleItem = target.querySelector<HTMLElement>(
    '[data-testid="toggle-dice-picker-menu-item"]',
  );
  const label = toggleItem?.textContent?.trim();
  document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  await flush();
  return label;
}

function isPickerVisible(target: HTMLElement): boolean {
  return target.querySelector('[data-testid="dice-buttons-row"]') !== null;
}

describe('Dice picker visibility persistence (Issue #246)', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
    document.body.innerHTML = '';
  });

  // spec: Issue #246 保存値が無いときはダイスピッカーを表示する（既定は表示）
  it('shows the dice picker by default when nothing is stored', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mountFooter(target);
    await flush();

    expect(localStorage.getItem(DICE_PICKER_VISIBLE_STORAGE_KEY)).toBeNull();
    expect(isPickerVisible(target)).toBe(true);

    unmount(instance);
    target.remove();
  });

  // spec: Issue #246 非表示にした状態を localStorage に保存し、リロード相当の再マウント後も非表示を維持する
  it('keeps the picker hidden after unmount and remount (simulated reload)', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mountFooter(target);
    await flush();
    expect(isPickerVisible(target)).toBe(true);
    expect(await dicePickerMenuLabel(target)).toContain('ダイスピッカーを非表示');

    await toggleDicePicker(target);
    expect(isPickerVisible(target)).toBe(false);
    expect(localStorage.getItem(DICE_PICKER_VISIBLE_STORAGE_KEY)).toBe('false');

    // Simulate a full page reload: a fresh component instance reads the
    // persisted preference instead of falling back to the default.
    unmount(instance);
    const remounted = mountFooter(target);
    await flush();

    expect(isPickerVisible(target)).toBe(false);
    expect(await dicePickerMenuLabel(target)).toContain('ダイスピッカーを表示');

    // Toggling back on persists the shown state as well.
    await toggleDicePicker(target);
    expect(isPickerVisible(target)).toBe(true);
    expect(localStorage.getItem(DICE_PICKER_VISIBLE_STORAGE_KEY)).toBe('true');

    unmount(remounted);
    const remountedAgain = mountFooter(target);
    await flush();

    expect(isPickerVisible(target)).toBe(true);
    expect(await dicePickerMenuLabel(target)).toContain('ダイスピッカーを非表示');

    unmount(remountedAgain);
    target.remove();
  });

  // spec: Issue #246 localStorage の getItem / setItem が例外を投げてもフッターは描画・トグルできる
  it('renders and toggles without throwing when storage access throws', async () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new DOMException('storage disabled', 'SecurityError');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('quota exceeded', 'QuotaExceededError');
    });

    expect(() => getDicePickerVisible()).not.toThrow();
    expect(getDicePickerVisible()).toBe(DEFAULT_DICE_PICKER_VISIBLE);
    expect(() => setDicePickerVisible(false)).not.toThrow();

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mountFooter(target);
    await flush();

    // Default is visible even though the stored value cannot be read.
    expect(isPickerVisible(target)).toBe(true);
    expect(await dicePickerMenuLabel(target)).toContain('ダイスピッカーを非表示');

    // Toggling still updates the in-memory state; the failed write is swallowed.
    await toggleDicePicker(target);
    expect(isPickerVisible(target)).toBe(false);
    expect(await dicePickerMenuLabel(target)).toContain('ダイスピッカーを表示');

    unmount(instance);
    target.remove();
  });

  // spec: Issue #246 壊れた保存値は表示（既定）にフォールバックする
  it('falls back to visible for a corrupt stored value', async () => {
    localStorage.setItem(DICE_PICKER_VISIBLE_STORAGE_KEY, 'garbage');

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mountFooter(target);
    await flush();

    expect(isPickerVisible(target)).toBe(true);

    unmount(instance);
    target.remove();
  });
});

describe('dicePickerPreference helper (Issue #246)', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  // spec: Issue #246 ダイスピッカー表示設定の読み書き（localStorage キー alhalte:dice-picker-visible）
  it('round-trips the persisted visibility', () => {
    expect(getDicePickerVisible()).toBe(true);

    setDicePickerVisible(false);
    expect(localStorage.getItem(DICE_PICKER_VISIBLE_STORAGE_KEY)).toBe('false');
    expect(getDicePickerVisible()).toBe(false);

    setDicePickerVisible(true);
    expect(localStorage.getItem(DICE_PICKER_VISIBLE_STORAGE_KEY)).toBe('true');
    expect(getDicePickerVisible()).toBe(true);
  });

  // spec: Issue #246 保存値なし・不正値・ストレージ例外は既定の表示にフォールバックする
  it('falls back to visible for missing, invalid, and throwing storage', () => {
    expect(getDicePickerVisible()).toBe(true);

    localStorage.setItem(DICE_PICKER_VISIBLE_STORAGE_KEY, '');
    expect(getDicePickerVisible()).toBe(true);

    localStorage.setItem(DICE_PICKER_VISIBLE_STORAGE_KEY, '{"visible":false}');
    expect(getDicePickerVisible()).toBe(true);

    localStorage.setItem(DICE_PICKER_VISIBLE_STORAGE_KEY, 'FALSE');
    expect(getDicePickerVisible()).toBe(true);

    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new DOMException('storage disabled', 'SecurityError');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('quota exceeded', 'QuotaExceededError');
    });

    expect(getDicePickerVisible()).toBe(true);
    expect(() => setDicePickerVisible(false)).not.toThrow();
  });
});
