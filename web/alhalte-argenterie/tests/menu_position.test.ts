import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  computeMenuPosition,
  getMenuPositionStyle,
  applyMenuPosition,
  menuPosition,
} from '../src/utils/menuPosition.js';

describe('menuPosition utility & actions (Issue #124)', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    // Mock viewport size
    window.innerWidth = 1000;
    window.innerHeight = 800;
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
});
