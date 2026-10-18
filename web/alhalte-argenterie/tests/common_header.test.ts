import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import CommonHeader from '../src/components/Control/CommonHeader.svelte';
import type { Scene } from '../src/types/index.js';

describe('CommonHeader Component & Gear Menu Positioning (Issue #124)', () => {
  const sampleScene: Scene = {
    id: 'scene-1',
    name: 'メインシーン',
    members: ['char-1'],
    juices: {
      bgUrl: 'https://example.com/bg.png',
      bgWidth: 1920,
      bgHeight: 1080,
      bgmSlots: [],
    },
  };

  const sampleScenes: Scene[] = [
    sampleScene,
    {
      id: 'scene-2',
      name: '探索エリア',
      members: ['char-2'],
      juices: {
        bgUrl: '',
        bgWidth: 800,
        bgHeight: 600,
        bgmSlots: [],
      },
    },
  ];

  beforeEach(() => {
    document.body.innerHTML = '';
    window.innerWidth = 1200;
    window.innerHeight = 800;
  });

  // spec: alhalte-argenterie/common-header 歯車設定メニューのトグルとアンカーボタン直下配置スタイル (Issue #124)
  it('positions gear dropdown menu directly adjacent to anchor button with fixed inset unset and position-anchor', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const onExportZip = vi.fn();

    const instance = mount(CommonHeader, {
      target,
      props: {
        scene: sampleScene,
        allScenes: sampleScenes,
        isAdmin: true,
        onExportZip,
      },
    });

    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    expect(gearBtn).toBeTruthy();

    // Mock anchor button bounding rect (e.g. top: 10, bottom: 46, left: 1150, right: 1186, width: 36, height: 36)
    vi.spyOn(gearBtn!, 'getBoundingClientRect').mockReturnValue({
      top: 10,
      bottom: 46,
      left: 1150,
      right: 1186,
      width: 36,
      height: 36,
      x: 1150,
      y: 10,
      toJSON: () => {},
    });

    // Menu initially not open
    expect(target.querySelector('[data-testid="gear-dropdown-menu"]')).toBeNull();

    // Click gear button to open menu
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    const menu = target.querySelector<HTMLElement>('[data-testid="gear-dropdown-menu"]');
    expect(menu).toBeTruthy();

    // Verify positioning styles are applied:
    // 1. position: fixed
    expect(menu?.style.position).toBe('fixed');
    expect(menu?.getAttribute('style')).toContain('position: fixed !important');
    // 2. inset: unset
    expect(menu?.getAttribute('style')).toContain('inset: unset');
    // 3. margin: 0
    expect(menu?.style.margin).toBe('0px');
    // 4. top is adjacent to button bottom (46 + offset 4 = 50px)
    expect(menu?.style.top).toBe('50px');
    // 5. right is aligned to anchor right (1200 - 1186 = 14px)
    expect(menu?.style.right).toBe('14px');
    // 6. Anchor positioning overrides
    expect(menu?.getAttribute('style')).toContain('position-area: none !important');
    expect(menu?.getAttribute('style')).toContain('position-try-fallbacks: none !important');
    expect(menu?.getAttribute('style')).toContain('align-self: unset !important');
    expect(menu?.getAttribute('style')).toContain('justify-self: unset !important');
    // 7. anchor name on button and position-anchor on menu
    expect(gearBtn?.style.getPropertyValue('anchor-name')).toBe('--gear-menu-anchor');
    expect(menu?.style.getPropertyValue('position-anchor')).toBe('--gear-menu-anchor');
    expect(menu?.style.getPropertyValue('--np-menu-position-area')).toBe('bottom span-right');

    // Clicking an item triggers callback and closes menu
    const zipItem = target.querySelector<HTMLElement>('[data-testid="zip-export-item"]');
    expect(zipItem).toBeTruthy();
    zipItem?.click();
    await new Promise((r) => setTimeout(r, 30));

    expect(onExportZip).toHaveBeenCalledWith('full');
    expect(target.querySelector('[data-testid="gear-dropdown-menu"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-header 歯車メニューの外側クリックによる閉じる動作
  it('closes gear dropdown menu when clicking outside', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(CommonHeader, {
      target,
      props: {
        scene: sampleScene,
        allScenes: sampleScenes,
      },
    });

    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    expect(target.querySelector('[data-testid="gear-dropdown-menu"]')).toBeTruthy();

    // Click outside
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 30));

    expect(target.querySelector('[data-testid="gear-dropdown-menu"]')).toBeNull();

    unmount(instance);
    target.remove();
  });
});
