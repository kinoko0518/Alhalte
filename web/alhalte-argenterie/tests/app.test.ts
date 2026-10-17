import { describe, it, expect } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import type { VisitedRoom } from '../src/types/index.js';

describe('App Layout (Material 3 Expressive)', () => {
  const dummyRoom: VisitedRoom = {
    id: 'room-main',
    name: 'テスト部屋',
    url: 'https://alhalte.example/rooms/room-main#abcdef',
    lastVisited: Date.now(),
  };

  // spec: alhalte-argenterie/layout 横長画面（landscape）& Side Sheet (Issue #32)
  it('renders field panel on left and M3 Side Sheet control panel on right in landscape mode, supporting collapse and reopen', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1200 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    const roomLayout = target.querySelector('[data-testid="room-layout"]');
    expect(roomLayout?.classList.contains('landscape')).toBe(true);

    const fieldWrapper = target.querySelector('[data-testid="landscape-field-wrapper"]');
    const controlWrapper = target.querySelector('[data-testid="landscape-control-wrapper"]');

    expect(fieldWrapper).toBeTruthy();
    expect(controlWrapper).toBeTruthy();

    // Verify noph-ui Side Sheet (.np-sheet-end) is used
    const sideSheet = controlWrapper?.querySelector('.np-sheet.np-sheet-end');
    expect(sideSheet).toBeTruthy();
    expect(sideSheet?.hasAttribute('open')).toBe(true);

    // Click collapse button in LogTab to collapse the Side Sheet
    const collapseBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(collapseBtn).toBeTruthy();
    collapseBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(sideSheet?.hasAttribute('open')).toBe(false);
    const reopenBtn = target.querySelector<HTMLButtonElement>('[data-testid="reopen-sheet-button"]');
    expect(reopenBtn).toBeTruthy();

    // Reopen the Side Sheet
    reopenBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(sideSheet?.hasAttribute('open')).toBe(true);

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/layout 縦長画面（portrait）& Bottom Sheet (Issue #32)
  it('renders full field with M3 Bottom Sheet control panel in portrait mode', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    const roomLayout = target.querySelector('[data-testid="room-layout"]');
    expect(roomLayout?.classList.contains('portrait')).toBe(true);

    const portraitField = target.querySelector('[data-testid="portrait-field-wrapper"]');
    const bottomSheet = target.querySelector('[data-testid="portrait-bottom-sheet"]');

    expect(portraitField).toBeTruthy();
    expect(bottomSheet).toBeTruthy();

    // Verify noph-ui Bottom Sheet (.np-sheet-bottom) with drag handle is used
    const sheetDialog = bottomSheet?.querySelector('.np-sheet.np-sheet-bottom');
    expect(sheetDialog).toBeTruthy();
    expect(sheetDialog?.querySelector('.np-sheet-handle')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/footer シーン管理タブを廃止しシーングラフタブを「シーンタブ」に改名 (Issue #30)
  it('renders 5 tabs in the control panel footer with シーンタブ and without シーン管理タブ', () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1200 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(App, {
      target,
      props: {
        initialRoom: dummyRoom,
      },
    });

    const tabButtonsRow = target.querySelector('[data-testid="tab-buttons-row"]');
    expect(tabButtonsRow).toBeTruthy();

    const tabButtons = tabButtonsRow?.querySelectorAll('button');
    expect(tabButtons).toHaveLength(5);

    const sceneTabBtn = target.querySelector('[data-testid="tab-btn-graph"]');
    expect(sceneTabBtn).toBeTruthy();
    expect(sceneTabBtn?.textContent?.trim()).toBe('シーンタブ');

    // Scene management tab button must not exist
    expect(target.querySelector('[data-testid="tab-btn-scenes"]')).toBeNull();

    unmount(instance);
    target.remove();
  });
});
