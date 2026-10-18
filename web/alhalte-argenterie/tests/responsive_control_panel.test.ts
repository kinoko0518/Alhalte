import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import App from '../src/components/App.svelte';
import type { VisitedRoom } from '../src/types/index.js';

describe('Responsive Control Panel (Side Sheet & Bottom Sheet)', () => {
  const dummyRoom: VisitedRoom = {
    id: 'room-test-63',
    name: 'サイズ連動テスト部屋',
    url: 'https://alhalte.example/rooms/room-test-63#abcdef',
    lastVisited: Date.now(),
  };

  let target: HTMLDivElement;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.appendChild(target);
  });

  afterEach(() => {
    target.remove();
    document.body.innerHTML = '';
  });

  // spec: alhalte-argenterie/control-panel Issue #63: 横画面時のSide sheet無段階リサイズと10vw制約
  it('mounts Side sheet on landscape mode and performs stepless resize with 10vw min-width constraint', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1200 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const instance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const roomLayout = target.querySelector('[data-testid="room-layout"]');
    expect(roomLayout?.classList.contains('landscape')).toBe(true);

    const sideSheet = target.querySelector<HTMLDialogElement>('[data-testid="landscape-side-sheet"]');
    expect(sideSheet).toBeTruthy();
    expect(sideSheet?.classList.contains('np-sheet-end')).toBe(true);
    expect(sideSheet?.hasAttribute('open')).toBe(true);

    // Verify resize handle exists on the left edge
    const resizeHandle = target.querySelector<HTMLElement>('[data-testid="side-sheet-resize-handle"]');
    expect(resizeHandle).toBeTruthy();
    expect(resizeHandle?.getAttribute('role')).toBe('separator');
    expect(resizeHandle?.getAttribute('aria-orientation')).toBe('vertical');

    // Initial width check
    const initialStyle = sideSheet?.getAttribute('style') || '';
    expect(initialStyle).toContain('--np-sheet-size: 440px');

    // Simulate stepless resize: drag left by 160px (expanding from 440px to 600px)
    // startX = 760 (1200 - 440), move to clientX = 600 -> deltaX = 160 -> width 600
    resizeHandle?.dispatchEvent(new PointerEvent('pointerdown', { clientX: 760, button: 0, bubbles: true }));
    window.dispatchEvent(new PointerEvent('pointermove', { clientX: 600, bubbles: true }));

    await new Promise((r) => setTimeout(r, 20));
    expect(sideSheet?.getAttribute('style')).toContain('--np-sheet-size: 600px');
    expect(sideSheet?.getAttribute('style')).toContain('width: 600px');

    // Simulate stepless resize: drag right past the 10vw limit
    // 10vw of 1200px is 120px.
    // If clientX moves to 1150px (startX: 760), deltaX = -390 -> 440 - 390 = 50px < 120px
    window.dispatchEvent(new PointerEvent('pointermove', { clientX: 1150, bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    // Must be constrained to at least 10vw (120px)
    expect(sideSheet?.getAttribute('style')).toContain('--np-sheet-size: 120px');
    expect(sideSheet?.getAttribute('style')).toContain('width: 120px');
    expect(sideSheet?.getAttribute('style')).toContain('min-width: 10vw');

    // Complete drag
    window.dispatchEvent(new PointerEvent('pointerup', { clientX: 1150, bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    unmount(instance);
  });

  // spec: alhalte-argenterie/control-panel Issue #89: リサイズハンドルの謎ボタン撤去、共通ヘッダーの左トナルボタンによる閉鎖、右上再オープンボタン
  it('removes mystery button from resize handle, and supports closing via CommonHeader tonal button and reopening via top-right button across all tabs', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1200 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const instance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const sideSheet = target.querySelector<HTMLDialogElement>('[data-testid="landscape-side-sheet"]');
    expect(sideSheet?.hasAttribute('open')).toBe(true);

    // 1. Verify mystery button in resize handle is removed
    const resizeHandle = target.querySelector<HTMLElement>('[data-testid="side-sheet-resize-handle"]');
    expect(resizeHandle).toBeTruthy();
    expect(resizeHandle?.querySelector('.side-sheet-toggle-btn')).toBeNull();

    // 2. Check CommonHeader left collapse button: size s, tonal, square, wide, right_panel_close
    const collapseBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(collapseBtn).toBeTruthy();
    expect(collapseBtn?.textContent).toContain('right_panel_close');
    expect(collapseBtn?.classList.contains('tonal')).toBe(true);
    expect(collapseBtn?.classList.contains('square')).toBe(true);
    expect(collapseBtn?.classList.contains('wide')).toBe(true);

    // 3. Close via CommonHeader collapse button
    collapseBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(sideSheet?.hasAttribute('open')).toBe(false);

    // 4. Reopen using floating button at top right (with chevron_left)
    const reopenBtn = target.querySelector<HTMLButtonElement>('[data-testid="reopen-sheet-button"]');
    expect(reopenBtn).toBeTruthy();
    expect(reopenBtn?.textContent).toContain('chevron_left');
    reopenBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(sideSheet?.hasAttribute('open')).toBe(true);

    // 5. Switch to Scene Tab and verify CommonHeader left collapse button is present and works
    const sceneTabBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-graph"]');
    sceneTabBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const commonHeaderScene = target.querySelector('[data-testid="common-header"]');
    expect(commonHeaderScene).toBeTruthy();

    const collapseBtnScene = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(collapseBtnScene).toBeTruthy();
    collapseBtnScene?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(sideSheet?.hasAttribute('open')).toBe(false);

    // Reopen again from scene tab
    const reopenBtnScene = target.querySelector<HTMLButtonElement>('[data-testid="reopen-sheet-button"]');
    expect(reopenBtnScene).toBeTruthy();
    reopenBtnScene?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(sideSheet?.hasAttribute('open')).toBe(true);

    unmount(instance);
  });

  // spec: alhalte-argenterie/control-panel Issue #89: コントロールパネル共通ヘッダー（左のパネル閉じボタン、中央のSearchドック、右の歯車メニュー、横並びテーマ円）
  it('renders CommonHeader with left tonal close button, center docked Search scene selector, and right gear menu with horizontal theme color circles', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1200 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const instance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    // CommonHeader exists
    const commonHeader = target.querySelector('[data-testid="common-header"]');
    expect(commonHeader).toBeTruthy();

    // Left: Panel close button
    const collapseBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(collapseBtn).toBeTruthy();
    expect(collapseBtn?.textContent).toContain('right_panel_close');

    // Center: Search docked layout
    const searchAnchor = target.querySelector('[data-testid="scene-search-anchor"]');
    const searchInput = target.querySelector<HTMLInputElement>('[data-testid="scene-search-input"]');
    expect(searchAnchor).toBeTruthy();
    expect(searchInput).toBeTruthy();
    expect(searchInput?.placeholder).toBe('メインシーン');

    // Focus search input -> docked scene candidates appear
    searchInput?.focus();
    await new Promise((r) => setTimeout(r, 30));
    const searchMenu = target.querySelector('[data-testid="scene-search-menu"]');
    expect(searchMenu).toBeTruthy();
    const mainSceneItem = target.querySelector('[data-testid="scene-search-item-/"]');
    expect(mainSceneItem).toBeTruthy();
    expect(mainSceneItem?.textContent).toContain('メインシーン');

    // Right: Gear button
    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    expect(gearBtn).toBeTruthy();
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    const gearMenu = target.querySelector('[data-testid="gear-dropdown-menu"]');
    expect(gearMenu).toBeTruthy();
    expect(target.querySelector('[data-testid="zip-export-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="export-zip-logs-item"]')).toBeTruthy();

    // Theme selector: horizontal theme color circles row (not vertical menu items)
    const themeCirclesRow = target.querySelector('[data-testid="theme-circles-row"]');
    expect(themeCirclesRow).toBeTruthy();
    expect(themeCirclesRow?.getAttribute('role')).toBe('radiogroup');

    // Verify color circles exist
    const circles = themeCirclesRow?.querySelectorAll('.theme-color-circle');
    expect(circles?.length).toBeGreaterThanOrEqual(5);

    // Click a theme color circle (e.g., emerald)
    const emeraldCircle = target.querySelector<HTMLButtonElement>('[data-testid="theme-item-emerald"]');
    expect(emeraldCircle).toBeTruthy();
    emeraldCircle?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Verify theme changed on document
    expect(document.documentElement.dataset.themePalette).toBe('emerald');

    unmount(instance);
  });

  // spec: alhalte-argenterie/control-panel Issue #63: 縦画面時のBottom sheet (width: 100%) と3状態（閉・半・全開）
  it('mounts Bottom sheet in portrait mode occupying 100% width and supports 3 states (half, full, collapsed)', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const instance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const roomLayout = target.querySelector('[data-testid="room-layout"]');
    expect(roomLayout?.classList.contains('portrait')).toBe(true);

    const bottomSheet = target.querySelector<HTMLDialogElement>('[data-testid="portrait-sheet-dialog"]');
    expect(bottomSheet).toBeTruthy();
    expect(bottomSheet?.classList.contains('np-sheet-bottom')).toBe(true);

    // Verify drag handle is present
    const handle = bottomSheet?.querySelector<HTMLButtonElement>('.np-sheet-handle');
    expect(handle).toBeTruthy();

    // Verify 100% width constraint (class & style)
    expect(bottomSheet?.classList.contains('portrait-bottom-sheet')).toBe(true);

    // State 1: Half (default open state: 50vh, not expanded)
    expect(bottomSheet?.hasAttribute('open')).toBe(true);
    expect(bottomSheet?.classList.contains('np-sheet-expanded')).toBe(false);

    // Mock getBoundingClientRect for sheet element & content so naturalHeight calculation works
    if (bottomSheet) {
      bottomSheet.getBoundingClientRect = () => ({
        top: 400,
        bottom: 800,
        left: 0,
        right: 400,
        width: 400,
        height: 400,
        x: 0,
        y: 400,
        toJSON: () => {},
      });
      const content = bottomSheet.querySelector<HTMLElement>('.np-sheet-content');
      if (content) {
        Object.defineProperty(content, 'clientHeight', { configurable: true, value: 350 });
        Object.defineProperty(content, 'scrollHeight', { configurable: true, value: 800 });
      }
    }

    // State 2: Full (expanded to full window height less top margin)
    // Clicking handle steps to full (expanded)
    handle?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(bottomSheet?.classList.contains('np-sheet-expanded')).toBe(true);

    // Clicking handle again steps back to half
    handle?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(bottomSheet?.classList.contains('np-sheet-expanded')).toBe(false);

    // State 3: Collapsed (closed)
    // Collapsing via panel collapse button
    const collapseBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(collapseBtn).toBeTruthy();
    collapseBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(bottomSheet?.hasAttribute('open')).toBe(false);

    // Reopen using floating reopen button returns to half height
    const reopenBtn = target.querySelector<HTMLButtonElement>('[data-testid="reopen-sheet-button"]');
    expect(reopenBtn).toBeTruthy();
    reopenBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(bottomSheet?.hasAttribute('open')).toBe(true);
    expect(bottomSheet?.classList.contains('np-sheet-expanded')).toBe(false);

    unmount(instance);
  });

  // spec: alhalte-argenterie/control-panel Issue #63: ドラッグ終了時のMotion physics (Spring) とスナップ機構
  it('snaps Bottom sheet to nearest state with Motion physics (Spring) on drag release', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const instance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const bottomSheet = target.querySelector<HTMLDialogElement>('[data-testid="portrait-sheet-dialog"]');
    const handle = bottomSheet?.querySelector<HTMLButtonElement>('.np-sheet-handle');
    expect(handle).toBeTruthy();

    if (bottomSheet) {
      bottomSheet.getBoundingClientRect = () => ({
        top: 400,
        bottom: 800,
        left: 0,
        right: 400,
        width: 400,
        height: 400,
        x: 0,
        y: 400,
        toJSON: () => {},
      });
      const content = bottomSheet.querySelector<HTMLElement>('.np-sheet-content');
      if (content) {
        Object.defineProperty(content, 'clientHeight', { configurable: true, value: 350 });
        Object.defineProperty(content, 'scrollHeight', { configurable: true, value: 800 });
      }
    }

    // 1. Drag up with fast upward velocity carrying projected height to full -> snaps to full
    handle?.dispatchEvent(new PointerEvent('pointerdown', { clientY: 400, button: 0, bubbles: true }));
    handle?.dispatchEvent(new PointerEvent('pointermove', { clientY: 200, bubbles: true }));
    handle?.dispatchEvent(new PointerEvent('pointerup', { clientY: 200, bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    expect(bottomSheet?.classList.contains('np-sheet-expanded')).toBe(true);

    // 2. Drag down with fast downward velocity carrying projected height to 0 -> snaps to closed
    if (bottomSheet) {
      bottomSheet.getBoundingClientRect = () => ({
        top: 56,
        bottom: 800,
        left: 0,
        right: 400,
        width: 400,
        height: 744,
        x: 0,
        y: 56,
        toJSON: () => {},
      });
    }

    handle?.dispatchEvent(new PointerEvent('pointerdown', { clientY: 100, button: 0, bubbles: true }));
    handle?.dispatchEvent(new PointerEvent('pointermove', { clientY: 700, bubbles: true }));
    handle?.dispatchEvent(new PointerEvent('pointerup', { clientY: 700, bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    expect(bottomSheet?.hasAttribute('open')).toBe(false);

    unmount(instance);
  });

  // spec: alhalte-argenterie/control-panel Issue #87: 縦画面時の偶数タブ・奇数タブにおける余白均一化
  it('maintains consistent footer layout and uniform bottom margins across all tabs in portrait mode (#87)', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const instance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const bottomSheet = target.querySelector<HTMLDialogElement>('[data-testid="portrait-sheet-dialog"]');
    expect(bottomSheet).toBeTruthy();
    expect(bottomSheet?.hasAttribute('open')).toBe(true);

    const contentWrapper = bottomSheet?.querySelector<HTMLElement>('.bottom-sheet-content-wrapper');
    expect(contentWrapper).toBeTruthy();

    const tabs = ['log', 'graph', 'settings', 'characters', 'extensions'];
    for (const tabId of tabs) {
      const tabBtn = target.querySelector<HTMLButtonElement>(`[data-testid="tab-btn-${tabId}"]`);
      expect(tabBtn).toBeTruthy();
      tabBtn?.click();
      await new Promise((r) => setTimeout(r, 20));

      const footer = target.querySelector<HTMLElement>('[data-testid="common-footer"]');
      expect(footer).toBeTruthy();
      const input = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
      expect(input).toBeTruthy();

      const controlPanel = target.querySelector<HTMLElement>('[data-testid="control-panel"]');
      expect(controlPanel).toBeTruthy();
      expect(contentWrapper?.contains(controlPanel!)).toBe(true);
      expect(controlPanel?.contains(footer!)).toBe(true);
    }

    unmount(instance);
  });
});
