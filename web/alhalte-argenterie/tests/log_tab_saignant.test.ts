import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, unmount } from 'svelte';
import {
  initSaignant,
  loadSaignantWasm,
  getSaignantCss,
  injectSaignantCss,
  renderGenerast,
  renderGenerastSync,
  encodeStringAsGenerastTin,
  encodeGenerastAstToTin,
  clearSaignantCache,
} from '../src/utils/saignant.js';
import LogTab from '../src/components/Control/tabs/LogTab.svelte';
import { ChunkLogManager } from '../src/utils/log.js';
import { createDefaultJuices } from '../src/utils/sceneSettings.js';
import { getThemeSettings, setThemePalette } from '../src/utils/theme.js';
import type { Character, Scene } from '../src/types/index.js';

describe('Saignant WASM & LogTab M3 Expressive Integration (Issue #55)', () => {
  beforeEach(() => {
    localStorage.clear();
    clearSaignantCache();
    document.head.querySelectorAll('#alhalte-saignant-stylesheet').forEach((el) => el.remove());
  });

  // spec: alhalte-saignant/abi 実 WASM による HTML レンダリングと CSS 取得
  it('executes real alhalte_saignant.wasm ABI to render Generast HTML and CSS without TS reimplementation', async () => {
    const wasmInstance = await loadSaignantWasm();
    expect(wasmInstance).toBeTruthy();
    expect(wasmInstance.exports.alhalte_abi_version()).toBe(1);

    // CSS extraction & injection
    const css = getSaignantCss();
    expect(css.length).toBeGreaterThan(1000);
    expect(css).toContain('.saignant-bold');
    expect(css).toContain('.saignant-italic');
    expect(css).toContain('.saignant-log-entry');

    injectSaignantCss();
    const styleEl = document.getElementById('alhalte-saignant-stylesheet');
    expect(styleEl).toBeTruthy();
    expect(styleEl?.textContent).toContain('.saignant-bold');

    // Plain text rendering via Saignant WASM
    const plainHtml = await renderGenerast('こんにちは世界');
    expect(plainHtml).toBe('こんにちは世界');

    // Rich AST bold rendering via Saignant WASM
    const boldAstBytes = encodeGenerastAstToTin({
      type: 'nested',
      meta: 'bold',
      contents: [{ type: 'string', value: 'クリティカル！' }],
    });
    const boldHtml = await renderGenerast(boldAstBytes);
    expect(boldHtml).toContain('<strong class="saignant-bold">クリティカル！</strong>');

    // Rich AST italic rendering via Saignant WASM
    const italicAstBytes = encodeGenerastAstToTin({
      type: 'nested',
      meta: 'italic',
      contents: [{ type: 'string', value: 'ファンブル' }],
    });
    const italicHtml = await renderGenerast(italicAstBytes);
    expect(italicHtml).toContain('<em class="saignant-italic">ファンブル</em>');
  });

  // spec: alhalte-argenterie/log-tab ログタブ共通ヘッダー（閉じる、Search シーン切り替え、歯車 Menu）
  it('renders LogTab header with close button, search scene selector, and gear settings menu', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const sceneMain: Scene = {
      id: 'scene-main',
      name: '本堂',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };
    const sceneSub: Scene = {
      id: 'scene-sub',
      name: '奥の院',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let collapsed = false;
    let selectedSceneId: string | null = null;

    const instance = mount(LogTab, {
      target,
      props: {
        scene: sceneMain,
        allScenes: [sceneMain, sceneSub],
        isAdmin: true,
        onCollapse: () => {
          collapsed = true;
        },
        onSelectScene: (id) => {
          selectedSceneId = id;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // 1. Collapse button
    const collapseBtn = target.querySelector<HTMLButtonElement>('[data-testid="collapse-button"]');
    expect(collapseBtn).toBeTruthy();
    expect(collapseBtn?.getAttribute('aria-label')).toBe('パネルを閉じる');
    collapseBtn?.click();
    expect(collapsed).toBe(true);

    // 2. Search scene selector
    const searchAnchor = target.querySelector('[data-testid="scene-search-anchor"]');
    const searchInput = target.querySelector<HTMLInputElement>('[data-testid="scene-search-input"]');
    expect(searchAnchor).toBeTruthy();
    expect(searchInput).toBeTruthy();
    expect(searchInput?.placeholder).toBe('本堂');

    // Focus input to open scene menu
    searchInput?.focus();
    await new Promise((r) => setTimeout(r, 30));

    const sceneMenu = target.querySelector('[data-testid="scene-search-menu"]');
    expect(sceneMenu).toBeTruthy();

    const subSceneItem = target.querySelector<HTMLElement>('[data-testid="scene-search-item-scene-sub"]');
    expect(subSceneItem).toBeTruthy();
    expect(subSceneItem?.textContent).toContain('奥の院');

    subSceneItem?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(selectedSceneId).toBe('scene-sub');

    // 3. Gear menu
    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    expect(gearBtn).toBeTruthy();
    expect(gearBtn?.getAttribute('aria-label')).toBe('ログ・部屋設定');

    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    const gearMenu = target.querySelector('[data-testid="gear-dropdown-menu"]');
    expect(gearMenu).toBeTruthy();
    expect(target.querySelector('[data-testid="zip-export-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="export-zip-logs-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="manage-members-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="set-done-item"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="theme-section-header"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab 歯車メニューの ZIP 出力、PL 公開警告ダイアログ、テーマ切り替え
  it('triggers ZIP exports, setDone warning overlay, and theme palette switching from gear menu', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-dungeon',
      name: '地下牢',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: [],
    };

    let exportedType: string | null = null;
    let doneState = false;

    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        roomId: 'room-xyz-123',
        isAdmin: true,
        onExportZip: (type) => {
          exportedType = type;
        },
        onSetDone: (done) => {
          doneState = done;
        },
      },
    });

    await new Promise((r) => setTimeout(r, 20));

    // Open gear menu
    const gearBtn = target.querySelector<HTMLButtonElement>('[data-testid="gear-menu-button"]');
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Test 1: Full ZIP export
    const fullZipItem = target.querySelector<HTMLElement>('[data-testid="zip-export-item"]');
    expect(fullZipItem).toBeTruthy();
    fullZipItem?.click();
    expect(exportedType).toBe('full');

    // Reopen menu for logs export
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Test 2: Logs ZIP export
    const logsZipItem = target.querySelector<HTMLElement>('[data-testid="export-zip-logs-item"]');
    expect(logsZipItem).toBeTruthy();
    logsZipItem?.click();
    expect(exportedType).toBe('logs');

    // Test 3: Set Done warning dialog
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const setDoneItem = target.querySelector<HTMLElement>('[data-testid="set-done-item"]');
    expect(setDoneItem).toBeTruthy();
    setDoneItem?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Viewport-level full-screen warning overlay with scrim
    const warningDialog = target.querySelector('[data-testid="set-done-warning-dialog"]');
    expect(warningDialog).toBeTruthy();
    expect(target.querySelector('[data-testid="done-dialog-title"]')?.textContent).toContain(
      '部屋の全てをPLに公開しますか？',
    );
    expect(target.querySelector('[data-testid="done-dialog-warning"]')?.textContent).toContain(
      '部屋を「公開（done）」状態にします',
    );

    // Cancel does not trigger setDone
    target.querySelector<HTMLButtonElement>('[data-testid="cancel-done-btn"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="set-done-warning-dialog"]')).toBeNull();
    expect(doneState).toBe(false);

    // Reopen and confirm
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    target.querySelector<HTMLElement>('[data-testid="set-done-item"]')?.click();
    await new Promise((r) => setTimeout(r, 20));

    target.querySelector<HTMLButtonElement>('[data-testid="confirm-done-button"]')?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(doneState).toBe(true);
    expect(target.querySelector('[data-testid="set-done-warning-dialog"]')).toBeNull();

    // Test 4: Theme Palette selection (Azure, Violet, Emerald, Rose, Amber)
    gearBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const emeraldItem = target.querySelector<HTMLElement>('[data-testid="theme-item-emerald"]');
    expect(emeraldItem).toBeTruthy();
    emeraldItem?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(getThemeSettings().palette).toBe('emerald');
    expect(document.documentElement.dataset.themePalette).toBe('emerald');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/log-tab メッセージボックス（キャラアイコン、キャラカラー Pill、タイムスタンプ、Saignant WASM 描画）
  it('renders message box with character avatar icon, character color Pill badge, timestamp, and Saignant HTML content', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const testScene: Scene = {
      id: 'scene-hall',
      name: '大広間',
      juices: createDefaultJuices(),
      viewTransform: { x: 0, y: 0, scale: 1 },
      members: ['char-1', 'char-2'],
    };

    const testCharacters: Character[] = [
      {
        id: 'char-1',
        name: 'ルシア',
        initiative: 15,
        iconUrl: 'https://alhalte.example/avatars/lucia.png',
        color: '#e91e63',
        description: '魔術師',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
      {
        id: 'char-2',
        name: 'ベルナルド',
        initiative: 10,
        color: '#2e7d32',
        description: '騎士',
        statuses: [],
        parameters: [],
        chatPalette: '',
        hideStatus: false,
      },
    ];

    const chunkManager = new ChunkLogManager();
    chunkManager.appendRecord({
      id: 'msg-lucia-1',
      sceneId: 'scene-hall',
      content: '私の番ね。炎の矢を放ちます！',
      speaker: 'ルシア',
      speakerId: 'char-1',
      timestamp: 1700000000,
    });
    chunkManager.appendRecord({
      id: 'msg-bernardo-1',
      sceneId: 'scene-hall',
      content: '援護する、任せろ！',
      speaker: 'ベルナルド',
      speakerId: 'char-2',
      timestamp: 1700000060,
    });

    const instance = mount(LogTab, {
      target,
      props: {
        scene: testScene,
        characters: testCharacters,
        chunkManager,
      },
    });

    await new Promise((r) => setTimeout(r, 50));

    // 1. First record: Lucia (with avatar image & pink color)
    const recordLucia = target.querySelector('[data-testid="log-record-msg-lucia-1"]');
    expect(recordLucia).toBeTruthy();

    const avatarLucia = target.querySelector('[data-testid="msg-avatar-msg-lucia-1"]');
    expect(avatarLucia).toBeTruthy();

    const speakerPillLucia = target.querySelector<HTMLElement>(
      '[data-testid="msg-speaker-msg-lucia-1"]',
    );
    expect(speakerPillLucia).toBeTruthy();
    expect(speakerPillLucia?.textContent?.trim()).toBe('ルシア');
    expect(speakerPillLucia?.classList.contains('msg-speaker-pill')).toBe(true);
    expect(speakerPillLucia?.getAttribute('style')).toContain('--char-color: #e91e63');

    const timestampLucia = target.querySelector('[data-testid="msg-timestamp-msg-lucia-1"]');
    expect(timestampLucia).toBeTruthy();
    expect(timestampLucia?.textContent?.trim()).not.toBe('');

    const contentLucia = target.querySelector('[data-testid="msg-content-msg-lucia-1"]');
    expect(contentLucia).toBeTruthy();
    expect(contentLucia?.textContent).toContain('私の番ね。炎の矢を放ちます！');

    // 2. Second record: Bernardo (no avatar image, initials with green color)
    const recordBernardo = target.querySelector('[data-testid="log-record-msg-bernardo-1"]');
    expect(recordBernardo).toBeTruthy();

    const avatarBernardo = target.querySelector('[data-testid="msg-avatar-msg-bernardo-1"]');
    expect(avatarBernardo).toBeTruthy();
    expect(avatarBernardo?.textContent?.trim()).toBe('ベル');

    const speakerPillBernardo = target.querySelector<HTMLElement>(
      '[data-testid="msg-speaker-msg-bernardo-1"]',
    );
    expect(speakerPillBernardo).toBeTruthy();
    expect(speakerPillBernardo?.textContent?.trim()).toBe('ベルナルド');
    expect(speakerPillBernardo?.getAttribute('style')).toContain('--char-color: #2e7d32');

    const contentBernardo = target.querySelector('[data-testid="msg-content-msg-bernardo-1"]');
    expect(contentBernardo?.textContent).toContain('援護する、任せろ！');

    unmount(instance);
    target.remove();
  });
});
