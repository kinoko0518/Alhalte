import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  computeMenuPosition,
  getMenuPositionStyle,
  applyMenuPosition,
  menuPosition,
} from '../src/utils/menuPosition.js';

describe('menuPosition utility & actions (Issue #124)', () => {
  // jsdom has no real UA stylesheet, so emulate the popover rule that leaks `0`
  // into every side without an author declaration. Longhands are used because
  // jsdom/cssstyle does not expand the `inset` shorthand (Issue #241).
  const UA_POPOVER_CSS = '[popover] { top: 0; right: 0; bottom: 0; left: 0; }';
  let uaPopoverSheet: HTMLStyleElement | null = null;

  beforeEach(() => {
    document.body.innerHTML = '';
    // Mock viewport size
    window.innerWidth = 1000;
    window.innerHeight = 800;

    uaPopoverSheet = document.createElement('style');
    uaPopoverSheet.textContent = UA_POPOVER_CSS;
    document.head.appendChild(uaPopoverSheet);
  });

  afterEach(() => {
    uaPopoverSheet?.remove();
    uaPopoverSheet = null;
  });

  // spec: alhalte-argenterie/menu-position anchor が無い場合のフォールバックスタイル
  it('returns safe fallback styles when anchor is not provided', () => {
    const result = computeMenuPosition(null, null, { anchorName: '--test-anchor' });
    expect(result.styleString).toContain('position: fixed !important');
    expect(result.styleString).toContain('inset: unset !important');
    expect(result.styleString).toContain('margin: 0 !important');
    expect(result.styleString).toContain('position-area: none !important');
    expect(result.styleString).toContain('position-try-fallbacks: none !important');
    expect(result.styleString).toContain('align-self: unset !important');
    expect(result.styleString).toContain('justify-self: unset !important');
    expect(result.styleString).toContain('position-anchor: --test-anchor');
  });

  // spec: alhalte-argenterie/menu-position 直下配置（bottom-start, bottom-end）の計算
  it('computes bottom placement directly below anchor with offset', () => {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);

    // Anchor at top: 100, left: 200, width: 50, height: 40 -> bottom: 140, right: 250
    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      bottom: 140,
      left: 200,
      right: 250,
      width: 50,
      height: 40,
      x: 200,
      y: 100,
      toJSON: () => {},
    });

    const result = computeMenuPosition(anchor, null, {
      placement: 'bottom-start',
      offset: 4,
    });

    // top = anchor.bottom + offset = 140 + 4 = 144
    expect(result.top).toBe(144);
    expect(result.left).toBe(200);
    expect(result.isTop).toBe(false);
    expect(result.styleString).toContain('position: fixed !important');
    expect(result.styleString).toContain('inset: unset !important');
    expect(result.styleString).toContain('margin: 0 !important');
    expect(result.styleString).toContain('position-area: none !important');
    expect(result.styleString).toContain('position-try-fallbacks: none !important');
    expect(result.styleString).toContain('align-self: unset !important');
    expect(result.styleString).toContain('justify-self: unset !important');
    expect(result.styleString).toContain('top: 144px !important');
    expect(result.styleString).toContain('left: 200px !important');
  });

  // spec: alhalte-argenterie/menu-position bottom-end の右寄せ計算（アンカー右端へ直接バインド）
  it('computes bottom-end placement aligning menu directly to right side of anchor', () => {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);

    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      bottom: 140,
      left: 700,
      right: 750,
      width: 50,
      height: 40,
      x: 700,
      y: 100,
      toJSON: () => {},
    });

    const result = computeMenuPosition(anchor, null, {
      placement: 'bottom-end',
      estimatedWidth: 150,
      offset: 6,
    });

    // top = 140 + 6 = 146
    // right = vw (1000) - anchor.right (750) = 250
    expect(result.top).toBe(146);
    expect(result.right).toBe(250);
    expect(result.left).toBeUndefined();
    expect(result.styleString).toContain('top: 146px !important');
    expect(result.styleString).toContain('right: 250px !important');
    expect(result.styleString).toContain('position-area: none !important');
  });

  // spec: alhalte-argenterie/menu-position 直上配置（top-start, top-end）の計算
  it('computes top placement directly above anchor with bottom offset property', () => {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);

    // Anchor near bottom: top: 720, bottom: 760, left: 100, right: 150
    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 720,
      bottom: 760,
      left: 100,
      right: 150,
      width: 50,
      height: 40,
      x: 100,
      y: 720,
      toJSON: () => {},
    });

    const result = computeMenuPosition(anchor, null, {
      placement: 'top-start',
      offset: 4,
      useBottomForTopPlacement: true,
    });

    // bottom = vh (800) - anchor.top (720) + offset (4) = 84
    expect(result.isTop).toBe(true);
    expect(result.bottom).toBe(84);
    expect(result.left).toBe(100);
    expect(result.styleString).toContain('bottom: 84px !important');
    expect(result.styleString).toContain('left: 100px !important');
    expect(result.styleString).toContain('position-area: none !important');
  });

  // spec: alhalte-argenterie/menu-position auto 配置での下部スペース不足時の自動反転とmax-height
  it('flips to top and applies max-height when bottom space is insufficient', () => {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);

    // Anchor near bottom: top: 700, bottom: 740, left: 100, right: 150
    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 700,
      bottom: 740,
      left: 100,
      right: 150,
      width: 50,
      height: 40,
      x: 100,
      y: 700,
      toJSON: () => {},
    });

    // estimatedHeight: 200. Space below = 800 - 740 - 4 - 8 = 48 < 200.
    // Space above = 700 - 4 - 8 = 688 >= 200.
    const result = computeMenuPosition(anchor, null, {
      placement: 'auto',
      estimatedHeight: 200,
      offset: 4,
      margin: 8,
    });

    expect(result.isTop).toBe(true);
    // bottom = 800 - 700 + 4 = 104
    expect(result.bottom).toBe(104);
    expect(result.maxHeight).toBe(688);
    expect(result.styleString).toContain('bottom: 104px !important');
    expect(result.styleString).toContain('max-height: 688px !important');
    expect(result.styleString).toContain('overflow-y: auto !important');
  });

  // spec: alhalte-argenterie/menu-position CSS Anchor Positioning プロパティの付与
  it('sets anchor-name on anchor element and position-anchor in styles', () => {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);

    const style = getMenuPositionStyle(anchor, {
      anchorName: '--gear-menu-anchor',
      placement: 'bottom-end',
    });

    expect(anchor.style.getPropertyValue('anchor-name')).toBe('--gear-menu-anchor');
    expect(style).toContain('position-anchor: --gear-menu-anchor');
    expect(style).toContain('--np-menu-position-area: bottom span-right');
    expect(style).toContain('position-area: none !important');
    expect(style).toContain('position-try-fallbacks: none !important');
  });

  // spec: alhalte-argenterie/menu-position applyMenuPosition による要素への直接インラインスタイル適用
  it('applies styles directly to menu element and updates on window resize', () => {
    const anchor = document.createElement('button');
    const menu = document.createElement('div');
    document.body.appendChild(anchor);
    document.body.appendChild(menu);

    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 50,
      bottom: 90,
      left: 300,
      right: 350,
      width: 50,
      height: 40,
      x: 300,
      y: 50,
      toJSON: () => {},
    });

    const cleanup = applyMenuPosition(menu, anchor, {
      placement: 'bottom-start',
      offset: 5,
    });

    expect(menu.style.position).toBe('fixed');
    expect(menu.style.top).toBe('95px');
    expect(menu.style.left).toBe('300px');
    expect(menu.getAttribute('style')).toContain('position-area: none !important');
    expect(menu.getAttribute('style')).toContain('position-try-fallbacks: none !important');

    // Simulate window resize
    window.dispatchEvent(new Event('resize'));
    expect(menu.style.top).toBe('95px');

    cleanup();
  });

  // spec: alhalte-argenterie/menu-position menuPosition Svelte action の動作
  it('supports Svelte action binding', () => {
    const anchor = document.createElement('button');
    const menu = document.createElement('div');
    document.body.appendChild(anchor);
    document.body.appendChild(menu);

    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 80,
      bottom: 110,
      left: 40,
      right: 90,
      width: 50,
      height: 30,
      x: 40,
      y: 80,
      toJSON: () => {},
    });

    const actionReturn = menuPosition(menu, { anchor, options: { placement: 'bottom-start', offset: 10 } });
    expect(menu.style.top).toBe('120px');
    expect(menu.style.left).toBe('40px');

    actionReturn.destroy();
  });

  // spec: Issue #241 — web/alhalte-argenterie/ARCHITECTURE.md にはメニュー配置の節が無く、
  // 「UA popover の inset:0 に勝つには四辺すべてを longhand で pin する」という要件は
  // ここで固定する。文字列パス (styleString) と DOM パス (applyMenuPosition) の両方が対象。
  it('pins the opposite inset to auto so the UA popover inset:0 cannot leak (Issue #241)', () => {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);

    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      bottom: 140,
      left: 700,
      right: 750,
      width: 50,
      height: 40,
      x: 700,
      y: 100,
      toJSON: () => {},
    });

    const result = computeMenuPosition(anchor, null, {
      placement: 'bottom-end',
      offset: 6,
      estimatedWidth: 150,
    });

    // vw = 1000 -> right = 1000 - 750 = 250, top = 140 + 6 = 146.
    expect(result.styleString).toContain('right: 250px !important');
    expect(result.styleString).toContain('left: auto !important');
    expect(result.styleString).toContain('top: 146px !important');
    expect(result.styleString).toContain('bottom: auto !important');
    // The unused side is a longhand pin, never an `inset: 0` shorthand.
    expect(result.styleString).not.toMatch(/(^|;\s*)inset:\s*0/);
  });

  // spec: Issue #241 — UA popover の inset:0 が漏れないよう、各軸の使わない側も
  // 明示的に auto !important で pin される（ARCHITECTURE 未記載のためここで固定）。
  it('bottom-end leaves no axis unpinned: left is auto and right is set (Issue #241)', () => {
    const anchor = document.createElement('button');
    const menu = document.createElement('div');
    menu.setAttribute('popover', 'manual');
    document.body.append(anchor, menu);

    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      bottom: 140,
      left: 700,
      right: 750,
      width: 50,
      height: 40,
      x: 700,
      y: 100,
      toJSON: () => {},
    });

    // Sanity check: the injected UA rule alone really would put the menu at x=0.
    expect(window.getComputedStyle(menu).left).toBe('0px');

    const cleanup = applyMenuPosition(menu, anchor, {
      placement: 'bottom-end',
      offset: 6,
      estimatedWidth: 150,
      estimatedHeight: 120,
    });

    // The used sides keep their pixel values.
    expect(menu.style.getPropertyValue('top')).toBe('146px');
    expect(menu.style.getPropertyPriority('top')).toBe('important');
    expect(menu.style.getPropertyValue('right')).toBe('250px');
    expect(menu.style.getPropertyPriority('right')).toBe('important');
    // The unused sides are still author declarations, pinned to auto.
    expect(menu.style.getPropertyValue('left')).toBe('auto');
    expect(menu.style.getPropertyPriority('left')).toBe('important');
    expect(menu.style.getPropertyValue('bottom')).toBe('auto');
    expect(menu.style.getPropertyPriority('bottom')).toBe('important');

    // The cascade actually resolves the way Chrome does: no x=0 collapse.
    expect(window.getComputedStyle(menu).left).toBe('auto');
    expect(window.getComputedStyle(menu).top).toBe('146px');
    expect(window.getComputedStyle(menu).right).toBe('250px');

    cleanup();
  });

  // spec: Issue #241 — top 配置でも使わない top を auto !important で pin する。
  it('top placement pins top:auto and sets bottom (Issue #241)', () => {
    const anchor = document.createElement('button');
    document.body.appendChild(anchor);

    // Anchor near the bottom of the 800px viewport.
    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 720,
      bottom: 760,
      left: 100,
      right: 150,
      width: 50,
      height: 40,
      x: 100,
      y: 720,
      toJSON: () => {},
    });

    const topStart = computeMenuPosition(anchor, null, {
      placement: 'top-start',
      offset: 4,
      estimatedWidth: 100,
      estimatedHeight: 120,
    });
    // bottom = 800 - 720 + 4 = 84, left = 100.
    expect(topStart.isTop).toBe(true);
    expect(topStart.styleString).toContain('bottom: 84px !important');
    expect(topStart.styleString).toContain('top: auto !important');
    expect(topStart.styleString).toContain('left: 100px !important');
    expect(topStart.styleString).toContain('right: auto !important');

    const topEnd = computeMenuPosition(anchor, null, {
      placement: 'top-end',
      offset: 4,
      estimatedWidth: 100,
      estimatedHeight: 120,
    });
    // right = 1000 - 150 = 850, left = auto.
    expect(topEnd.isTop).toBe(true);
    expect(topEnd.styleString).toContain('bottom: 84px !important');
    expect(topEnd.styleString).toContain('top: auto !important');
    expect(topEnd.styleString).toContain('right: 850px !important');
    expect(topEnd.styleString).toContain('left: auto !important');

    const menu = document.createElement('div');
    menu.setAttribute('popover', 'manual');
    document.body.appendChild(menu);

    const cleanup = applyMenuPosition(menu, anchor, {
      placement: 'top-start',
      offset: 4,
      estimatedWidth: 100,
      estimatedHeight: 120,
    });

    expect(menu.style.getPropertyValue('top')).toBe('auto');
    expect(menu.style.getPropertyPriority('top')).toBe('important');
    expect(menu.style.getPropertyValue('bottom')).toBe('84px');
    expect(menu.style.getPropertyPriority('bottom')).toBe('important');
    expect(window.getComputedStyle(menu).top).toBe('auto');
    expect(window.getComputedStyle(menu).bottom).toBe('84px');

    cleanup();
  });

  // spec: Issue #241 — resize / scroll で再適用しても auto pin が維持される。
  it('keeps the auto pins after resize and scroll (Issue #241)', () => {
    const anchor = document.createElement('button');
    const menu = document.createElement('div');
    menu.setAttribute('popover', 'manual');
    document.body.append(anchor, menu);

    const anchorRectSpy = vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      bottom: 140,
      left: 700,
      right: 750,
      width: 50,
      height: 40,
      x: 700,
      y: 100,
      toJSON: () => {},
    });

    const cleanup = applyMenuPosition(menu, anchor, {
      placement: 'bottom-end',
      offset: 6,
      estimatedWidth: 150,
      estimatedHeight: 120,
    });

    const expectPins = (top: string, right: string) => {
      expect(menu.style.getPropertyValue('top')).toBe(top);
      expect(menu.style.getPropertyPriority('top')).toBe('important');
      expect(menu.style.getPropertyValue('right')).toBe(right);
      expect(menu.style.getPropertyPriority('right')).toBe('important');
      expect(menu.style.getPropertyValue('left')).toBe('auto');
      expect(menu.style.getPropertyPriority('left')).toBe('important');
      expect(menu.style.getPropertyValue('bottom')).toBe('auto');
      expect(menu.style.getPropertyPriority('bottom')).toBe('important');
      expect(window.getComputedStyle(menu).left).toBe('auto');
    };

    expectPins('146px', '250px');

    // window resize listener.
    window.dispatchEvent(new Event('resize'));
    expectPins('146px', '250px');

    // Captured scroll listener (`window.addEventListener('scroll', update, true)`).
    menu.dispatchEvent(new Event('scroll'));
    expectPins('146px', '250px');

    // The anchor moved: the next update must still pin all four sides.
    anchorRectSpy.mockReturnValue({
      top: 200,
      bottom: 240,
      left: 600,
      right: 650,
      width: 50,
      height: 40,
      x: 600,
      y: 200,
      toJSON: () => {},
    });
    window.dispatchEvent(new Event('resize'));
    expectPins('246px', '350px');

    cleanup();
  });
});
