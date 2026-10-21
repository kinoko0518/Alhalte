import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mount, unmount } from 'svelte';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import App from '../src/components/App.svelte';
import type { VisitedRoom } from '../src/types/index.js';

const APP_SOURCE = readFileSync(path.resolve(process.cwd(), 'src/components/App.svelte'), 'utf8');
const LOG_TAB_SOURCE = readFileSync(
  path.resolve(process.cwd(), 'src/components/Control/tabs/LogTab.svelte'),
  'utf8',
);
const CONTROL_PANEL_SOURCE = readFileSync(
  path.resolve(process.cwd(), 'src/components/Control/ControlPanel.svelte'),
  'utf8',
);

/** Returns the declaration block of the first rule whose selector starts with `selector`. */
function ruleDeclarations(source: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = source.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`));
  expect(match, `expected a CSS rule for ${selector}`).toBeTruthy();
  return match![1];
}

describe('Responsive Control Panel (Side Sheet & Bottom Sheet)', () => {
  const dummyRoom: VisitedRoom = {
    id: '0000000000000063',
    name: 'サイズ連動テスト部屋',
    url: 'https://alhalte.example/rooms/0000000000000063#abcdef',
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

  // spec: alhalte-argenterie/control-panel Issue #63 / Issue #262: 縦画面時のBottom sheet (width: 100%) と2状態（閉・全開）
  it('mounts Bottom sheet in portrait mode occupying 100% width and supports 2 states (full, collapsed)', async () => {
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

    // Issue #262: the drag handle and the half state were removed.
    expect(bottomSheet?.querySelector('.np-sheet-handle')).toBeNull();
    expect(target.querySelector('[data-testid="expand-sheet-button"]')).toBeNull();

    // Verify 100% width constraint (class & style)
    expect(bottomSheet?.classList.contains('portrait-bottom-sheet')).toBe(true);

    // State 1: Open. Opening the sheet always opens it fully.
    expect(bottomSheet?.hasAttribute('open')).toBe(true);
    expect(bottomSheet?.getAttribute('data-sheet-state')).toBe('full');

    // State 2: Closed, via the header collapse button.
    const collapseBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(collapseBtn).toBeTruthy();
    collapseBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(bottomSheet?.hasAttribute('open')).toBe(false);
    expect(bottomSheet?.getAttribute('data-sheet-state')).toBe('closed');

    // Reopen using the floating reopen button: full again, no half step.
    const reopenBtn = target.querySelector<HTMLButtonElement>('[data-testid="reopen-sheet-button"]');
    expect(reopenBtn).toBeTruthy();
    reopenBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(bottomSheet?.hasAttribute('open')).toBe(true);
    expect(bottomSheet?.getAttribute('data-sheet-state')).toBe('full');
    expect(target.querySelector('[data-testid="expand-sheet-button"]')).toBeNull();
    expect(bottomSheet?.querySelector('.np-sheet-handle')).toBeNull();

    unmount(instance);
  });

  // spec: alhalte-argenterie/control-panel Issue #262: ボトムシートは開いた時点で全開、内容高さに依存しない
  it('sizes the portrait sheet from the viewport alone and opens it fully (Issue #262)', () => {
    // CSS contract: the height comes from the viewport, never from the content
    // height or a second (half) state.
    const portraitRule = ruleDeclarations(
      APP_SOURCE,
      '.bottom-sheet-container :global(.portrait-bottom-sheet)',
    );
    expect(portraitRule).toContain('--np-sheet-size: var(--np-sheet-full-height)');
    expect(portraitRule).toContain('--np-sheet-top-margin: 4.5rem');
    expect(portraitRule).toContain(
      '--np-sheet-full-height: calc(100dvh - var(--np-sheet-top-margin))',
    );
    expect(portraitRule).toContain('height: var(--np-sheet-full-height)');
    expect(portraitRule).toContain('max-height: var(--np-sheet-full-height)');
    expect(portraitRule).not.toContain('--np-sheet-half-height');
    // `dvh` only: no `vh` fallback that would ignore the dynamic viewport.
    expect(portraitRule).not.toMatch(/(?<![a-z])vh\b/);
    expect(portraitRule).not.toMatch(/height:\s*auto/);

    // The half-state rule is gone; the base rule always uses the full height.
    expect(APP_SOURCE).not.toContain('.portrait-bottom-sheet.np-sheet-expanded');

    // Full covers ~90% of a typical portrait viewport: the only subtraction is
    // noph-ui's 4.5rem top margin (72px at the default 16px root font size).
    for (const viewportHeight of [800, 844]) {
      const covered = (viewportHeight - 4.5 * 16) / viewportHeight;
      expect(covered).toBeGreaterThanOrEqual(0.89);
      expect(covered).toBeLessThanOrEqual(0.92);
    }

    // Above 640px noph-ui raises its top margin to 3.5rem; match it.
    expect(APP_SOURCE).toMatch(/@media \(width > 640px\)\s*\{[\s\S]*?--np-sheet-top-margin: 3\.5rem/);
  });

  // spec: alhalte-argenterie/control-panel Issue #262: 半開トグルとドラッグハンドルを両レイアウトで出さない
  it('does not render the removed sheet toggle or drag handle on either placement (Issue #262)', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1200 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const landscapeInstance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const sideSheet = target.querySelector<HTMLDialogElement>('[data-testid="landscape-side-sheet"]');
    expect(sideSheet?.hasAttribute('open')).toBe(true);
    expect(target.querySelector('[data-testid="expand-sheet-button"]')).toBeNull();
    expect(target.querySelector('.np-sheet-handle')).toBeNull();
    expect(target.querySelector('[data-testid="collapse-button"]')).toBeTruthy();

    unmount(landscapeInstance);
    target.innerHTML = '';

    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const portraitInstance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const bottomSheet = target.querySelector<HTMLDialogElement>('[data-testid="portrait-sheet-dialog"]');
    expect(bottomSheet?.hasAttribute('open')).toBe(true);
    expect(target.querySelector('[data-testid="expand-sheet-button"]')).toBeNull();
    expect(target.querySelector('.np-sheet-handle')).toBeNull();

    unmount(portraitInstance);
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

  // spec: alhalte-argenterie/control-panel Issue #223: 縦画面は下向きの「パネルを閉じる」アイコン、横画面は右向きのまま
  it('uses bottom_panel_close for the portrait bottom sheet and keeps right_panel_close on the landscape side sheet', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const portraitInstance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const portraitBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(portraitBtn).toBeTruthy();
    // A bottom sheet closes downwards.
    expect(portraitBtn?.textContent).toContain('bottom_panel_close');
    expect(portraitBtn?.textContent).not.toContain('right_panel_close');
    expect(portraitBtn?.getAttribute('data-panel-placement')).toBe('bottom');
    expect(portraitBtn?.getAttribute('aria-label')).toBe('パネルを閉じる');
    // `title` is consumed by noph-ui as the tooltip text (same label).
    // The size / tonal / square / wide treatment is shared with the side sheet.
    expect(portraitBtn?.classList.contains('tonal')).toBe(true);
    expect(portraitBtn?.classList.contains('square')).toBe(true);
    expect(portraitBtn?.classList.contains('wide')).toBe(true);

    unmount(portraitInstance);
    target.innerHTML = '';

    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1200 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const landscapeInstance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const landscapeBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(landscapeBtn).toBeTruthy();
    expect(landscapeBtn?.textContent).toContain('right_panel_close');
    expect(landscapeBtn?.textContent).not.toContain('bottom_panel_close');
    expect(landscapeBtn?.getAttribute('data-panel-placement')).toBe('side');
    expect(landscapeBtn?.getAttribute('aria-label')).toBe('パネルを閉じる');
    expect(landscapeBtn?.classList.contains('tonal')).toBe(true);
    expect(landscapeBtn?.classList.contains('square')).toBe(true);
    expect(landscapeBtn?.classList.contains('wide')).toBe(true);

    unmount(landscapeInstance);
  });


  // spec: alhalte-argenterie/control-panel Issue #223: 縦画面のログ一覧がシート内でスクロールできる高さ連鎖
  it('keeps a bounded height chain from the portrait sheet content down to the log scroll container', async () => {
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 400 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });

    const instance = mount(App, {
      target,
      props: { initialRoom: dummyRoom },
    });
    await new Promise((r) => setTimeout(r, 20));

    const dialog = target.querySelector<HTMLDialogElement>('[data-testid="portrait-sheet-dialog"]');
    expect(dialog).toBeTruthy();
    expect(dialog?.classList.contains('portrait-bottom-sheet')).toBe(true);
    expect(dialog?.classList.contains('np-sheet-bottom')).toBe(true);
    expect(dialog?.getAttribute('data-sheet-state')).toBe('full');

    // The scroll chain the CSS bounds: sheet → content → wrapper → panel →
    // tab area → log tab → messages area → messages list.
    const sheetContent = Array.from(dialog!.children).find((el) =>
      el.classList.contains('np-sheet-content'),
    ) as HTMLElement | undefined;
    expect(sheetContent).toBeTruthy();

    const contentWrapper = sheetContent!.querySelector<HTMLElement>('.bottom-sheet-content-wrapper');
    expect(contentWrapper).toBeTruthy();

    const controlPanel = contentWrapper!.querySelector<HTMLElement>('[data-testid="control-panel"]');
    expect(controlPanel).toBeTruthy();
    expect(controlPanel?.classList.contains('control-panel-container')).toBe(true);

    const tabContentArea = controlPanel!.querySelector<HTMLElement>('[data-testid="tab-content-area"]');
    expect(tabContentArea).toBeTruthy();
    expect(tabContentArea?.classList.contains('tab-content-area')).toBe(true);

    const logTab = tabContentArea!.querySelector<HTMLElement>('[data-testid="log-tab"]');
    expect(logTab).toBeTruthy();
    expect(logTab?.classList.contains('log-tab-container')).toBe(true);

    const messagesArea = logTab!.querySelector<HTMLElement>('.messages-area');
    expect(messagesArea).toBeTruthy();

    const messagesList = messagesArea!.querySelector<HTMLElement>('[data-testid="messages-list"]');
    expect(messagesList).toBeTruthy();
    expect(messagesList?.classList.contains('messages-list')).toBe(true);
    // The log list is the single scroll container of the chain.
    expect(messagesList?.getAttribute('data-scroll-container')).toBe('log-messages');

    // Every ancestor really contains the next node.
    expect(sheetContent!.contains(contentWrapper!)).toBe(true);
    expect(contentWrapper!.contains(controlPanel!)).toBe(true);
    expect(controlPanel!.contains(tabContentArea!)).toBe(true);
    expect(tabContentArea!.contains(logTab!)).toBe(true);
    expect(logTab!.contains(messagesArea!)).toBe(true);
    expect(messagesArea!.contains(messagesList!)).toBe(true);

    // Definite flex / height chain: the sheet content wrapper and the control
    // panel are flex items, the sheet clips its content, and the log list is
    // the only element that scrolls.
    expect(ruleDeclarations(APP_SOURCE, '.bottom-sheet-content-wrapper')).toMatch(/flex:\s*1/);
    expect(ruleDeclarations(APP_SOURCE, '.bottom-sheet-content-wrapper')).toMatch(/min-height:\s*0/);
    expect(
      ruleDeclarations(
        APP_SOURCE,
        '.bottom-sheet-container :global(.bottom-sheet-content-wrapper .control-panel-container)',
      ),
    ).toMatch(/flex:\s*1[\s\S]*min-height:\s*0/);
    expect(
      ruleDeclarations(APP_SOURCE, '.bottom-sheet-container :global(.portrait-bottom-sheet .np-sheet-content)'),
    ).toMatch(/overflow:\s*hidden/);

    const tabContentAreaRule = ruleDeclarations(CONTROL_PANEL_SOURCE, '.tab-content-area');
    expect(tabContentAreaRule).toMatch(/flex:\s*1/);
    expect(tabContentAreaRule).toMatch(/min-height:\s*0/);
    expect(ruleDeclarations(CONTROL_PANEL_SOURCE, '.control-panel-container')).toMatch(/height:\s*100%/);

    const messagesAreaRule = ruleDeclarations(LOG_TAB_SOURCE, '.messages-area');
    expect(messagesAreaRule).toMatch(/flex:\s*1/);
    expect(messagesAreaRule).toMatch(/min-height:\s*0/);

    const messagesListRule = ruleDeclarations(LOG_TAB_SOURCE, '.messages-list');
    expect(messagesListRule).toMatch(/flex:\s*1/);
    expect(messagesListRule).toMatch(/min-height:\s*0/);
    expect(messagesListRule).toMatch(/overflow-y:\s*auto/);

    unmount(instance);
  });
});
