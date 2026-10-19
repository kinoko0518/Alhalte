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
        role: 'admin',
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

// spec: alhalte-argenterie/permissions 0200 既定の権限 selector は ack 後にのみ確定する
describe('CommonHeader default permission selector (Issue #201)', () => {
  const scene: Scene = {
    id: '/',
    name: 'メインシーン',
    members: [],
    juices: { bgUrl: '', bgWidth: 1920, bgHeight: 1080, bgmSlots: [] },
  };

  beforeEach(() => {
    document.body.innerHTML = '';
    window.innerWidth = 1200;
    window.innerHeight = 800;
  });

  async function openGearMenu(target: HTMLDivElement) {
    target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
  }

  it('renders the four default permission levels for an admin instead of the old PL toggle', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(CommonHeader, {
      target,
      props: { scene, role: 'admin', defaultPermission: 'participant' },
    });
    await new Promise((r) => setTimeout(r, 20));
    await openGearMenu(target);

    // The old 「部屋の全てをPLに公開」 entry is gone.
    expect(target.querySelector('[data-testid="set-done-item"]')).toBeNull();

    expect(target.querySelector('[data-testid="default-permission-selector"]')).toBeTruthy();
    for (const role of ['denied', 'viewer', 'participant', 'admin']) {
      const button = target.querySelector<HTMLButtonElement>(
        `[data-testid="default-permission-btn-${role}"]`,
      );
      expect(button).toBeTruthy();
      expect(button?.disabled).toBe(false);
    }
    // The current level is the filled one.
    expect(
      target.querySelector('[data-testid="default-permission-btn-participant"]')?.className,
    ).toContain('filled');

    unmount(instance);
    target.remove();
  });

  it('hides the admin-only default permission selector from a participant and a viewer', async () => {
    for (const role of ['participant', 'viewer'] as const) {
      const target = document.createElement('div');
      document.body.appendChild(target);

      const instance = mount(CommonHeader, { target, props: { scene, role } });
      await new Promise((r) => setTimeout(r, 20));
      await openGearMenu(target);

      expect(target.querySelector('[data-testid="admin-section-label"]')).toBeNull();
      expect(target.querySelector('[data-testid="default-permission-selector"]')).toBeNull();

      unmount(instance);
      target.remove();
    }
  });

  it('keeps the current level until onSetDefaultPermission resolves, then shows the new one', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let resolveRequest!: () => void;
    const request = new Promise<void>((resolve) => {
      resolveRequest = resolve;
    });
    const onSetDefaultPermission = vi.fn(() => request);

    const instance = mount(CommonHeader, {
      target,
      props: { scene, role: 'admin', defaultPermission: 'participant', onSetDefaultPermission },
    });
    await new Promise((r) => setTimeout(r, 20));
    await openGearMenu(target);

    target.querySelector<HTMLButtonElement>('[data-testid="default-permission-btn-viewer"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(onSetDefaultPermission).toHaveBeenCalledWith('viewer');
    // Still waiting for the banquette ack: the selector reflects the old level.
    expect(
      target.querySelector('[data-testid="default-permission-btn-participant"]')?.className,
    ).toContain('filled');
    expect(
      target.querySelector('[data-testid="default-permission-pending"]')?.textContent,
    ).toContain('閲覧者');

    resolveRequest();
    await new Promise((r) => setTimeout(r, 30));
    // The App owns `defaultPermission`; the component only reports progress.
    expect(target.querySelector('[data-testid="default-permission-pending"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  it('shows the error and does not report success when the acknowledgement fails', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const onSetDefaultPermission = vi.fn(async () => {
      throw new Error('SetDefaultPermissionFailed');
    });

    const instance = mount(CommonHeader, {
      target,
      props: { scene, role: 'admin', defaultPermission: 'participant', onSetDefaultPermission },
    });
    await new Promise((r) => setTimeout(r, 20));
    await openGearMenu(target);

    target.querySelector<HTMLButtonElement>('[data-testid="default-permission-btn-admin"]')?.click();
    await new Promise((r) => setTimeout(r, 30));

    const error = target.querySelector('[data-testid="default-permission-error"]');
    expect(error).toBeTruthy();
    expect(error?.textContent).toContain('SetDefaultPermissionFailed');
    // Nothing was committed.
    expect(
      target.querySelector('[data-testid="default-permission-btn-admin"]')?.className,
    ).not.toContain('filled');

    unmount(instance);
    target.remove();
  });
});
