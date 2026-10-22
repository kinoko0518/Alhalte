import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, unmount } from 'svelte';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import CommonFooter from '../src/components/Control/CommonFooter.svelte';
import App from '../src/components/App.svelte';
import { asRoomSocket, FakeRoomSocket } from './helpers/fakeRoomSocket.js';
import type { Character, ExtensionItem, VisitedRoom } from '../src/types/index.js';
import type { GenerastAST } from '../src/utils/generast.js';
import { decodeExtIoTin } from '../src/utils/generast.js';
import { speakableCharacters } from '../src/utils/characterActions.js';
import { ExtensionPipeline } from '../src/wasm/runner.js';

const FOOTER_SOURCE = readFileSync(
  path.resolve(process.cwd(), 'src/components/Control/CommonFooter.svelte'),
  'utf8',
);

describe('CommonFooter Component Rebuild (Issue #54)', () => {
  const samplePalette = `
1D100<=50 【目星】
1D100<=70 【聞き耳】
1D6+1D4 ダメージボーナス
`;

  const sampleCharacters: Character[] = [
    {
      id: 'char-1',
      name: '探索者アルス',
      initiative: 12,
      iconUrl: 'https://example.com/char1.png',
      description: '敏腕探偵',
      statuses: [{ label: 'HP', current: 12, max: 12 }],
      parameters: [{ label: 'STR', value: 14 }],
      chatPalette: samplePalette,
      hideStatus: false,
    },
    {
      id: 'char-2',
      name: '助手ベアトリス',
      initiative: 9,
      iconUrl: 'https://example.com/char2.png',
      description: '助手兼司書',
      statuses: [{ label: 'HP', current: 10, max: 10 }],
      parameters: [{ label: 'INT', value: 16 }],
      chatPalette: '1D100<=80 【図書館】',
      hideStatus: false,
    },
  ];

  beforeEach(() => {
    document.body.innerHTML = '';
  });

  // spec: alhalte-argenterie/common-footer タブ選択 S ボタングループの描画・選択時アニメーション構造・Tooltip
  it('renders S standard ButtonGroup tabs with smooth expand transition structure and Tooltips', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let currentTab = 'log';
    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        onTabChange: (tab) => {
          currentTab = tab;
        },
      },
    });

    // Verify ButtonGroup container with size S
    const btnGroup = target.querySelector('.np-button-group');
    expect(btnGroup).toBeTruthy();
    expect(btnGroup?.classList.contains('standard')).toBe(true);

    const expectedTabs = [
      { id: 'log', icon: 'forum', label: 'ログ', fullLabel: 'ログタブ' },
      { id: 'graph', icon: 'account_tree', label: 'シーングラフ', fullLabel: 'シーングラフタブ' },
      { id: 'settings', icon: 'tune', label: 'シーン管理', fullLabel: 'シーン管理タブ' },
      { id: 'characters', icon: 'group', label: 'キャラクター', fullLabel: 'キャラクター管理タブ' },
      { id: 'extensions', icon: 'extension', label: '拡張機能', fullLabel: '拡張機能管理タブ' },
    ];

    for (const tab of expectedTabs) {
      const tabBtn = target.querySelector<HTMLButtonElement>(`[data-testid="tab-btn-${tab.id}"]`);
      expect(tabBtn).toBeTruthy();
      expect(tabBtn?.classList.contains('s')).toBe(true); // Size S button
      expect(tabBtn?.getAttribute('aria-label')).toBe(tab.fullLabel);
      expect(tabBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe(tab.icon);

      // Tooltip is attached with corresponding ID
      const tooltip = target.querySelector(`#tooltip-tab-${tab.id}`);
      expect(tooltip).toBeTruthy();
      expect(tooltip?.textContent?.trim()).toBe(tab.fullLabel);
    }

    // Selected tab (log) has expanded label; others are collapsed
    const logBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-log"]');
    expect(logBtn?.classList.contains('selected')).toBe(true);
    const logLabel = logBtn?.querySelector('.tab-btn-label');
    expect(logLabel?.classList.contains('expanded')).toBe(true);
    expect(logLabel?.classList.contains('tab-label-text')).toBe(true);
    expect(logLabel?.textContent?.trim()).toBe('ログ');

    const graphBtn = target.querySelector<HTMLButtonElement>('[data-testid="tab-btn-graph"]');
    expect(graphBtn?.classList.contains('selected')).toBe(false);
    const graphLabel = graphBtn?.querySelector('.tab-btn-label');
    expect(graphLabel?.classList.contains('collapsed')).toBe(true);
    expect(graphLabel?.classList.contains('tab-label-text')).toBe(true);

    // Clicking graph tab triggers onTabChange
    graphBtn?.click();
    expect(currentTab).toBe('graph');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer Issue #91: タブ遷移アニメーションの最適化とButtonGroupプレスアニメーション競合解消
  it('ensures smooth tab transitions without ButtonGroup press animation interference (#91)', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let currentTab: string = 'log';
    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        onTabChange: (tab) => {
          currentTab = tab;
        },
      },
    });

    const tabs = ['log', 'graph', 'settings', 'characters', 'extensions'];
    for (const tabId of tabs) {
      const btn = target.querySelector<HTMLButtonElement>(`[data-testid="tab-btn-${tabId}"]`);
      expect(btn).toBeTruthy();

      // Trigger pointerdown to test press behavior
      btn?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      // With expandedRatio={0}, inline style.width should NOT be added
      expect(btn?.style.width).toBe('');

      btn?.click();
      expect(currentTab).toBe(tabId);
    }

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer キャラクター選択 Filled tonal button と Menu ポップオーバー
  it('renders character selector as Filled tonal Button and opens Menu popover on click with outside click dismissal', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let selectedCharId: string | null = null;
    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        onSelectCharacter: (charId) => {
          selectedCharId = charId;
        },
      },
    });

    // Character select button is an M3 Filled tonal Button
    const charSelectBtn = target.querySelector<HTMLButtonElement>('[data-testid="character-select-button"]');
    expect(charSelectBtn).toBeTruthy();
    expect(charSelectBtn?.classList.contains('np-button')).toBe(true);
    expect(charSelectBtn?.classList.contains('tonal')).toBe(true);

    // Displays selected character avatar and name
    const avatar = charSelectBtn?.querySelector<HTMLImageElement>('.picker-avatar');
    expect(avatar?.src).toContain('char1.png');
    const nameEl = charSelectBtn?.querySelector('.picker-name');
    expect(nameEl?.textContent?.trim()).toBe('探索者アルス');

    // Initially menu is closed
    expect(target.querySelector('[data-testid="character-picker-dropdown"]')).toBeNull();

    // Click to open M3 Menu popover
    charSelectBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const menu = target.querySelector('[data-testid="character-picker-dropdown"]');
    expect(menu).toBeTruthy();
    expect(menu?.classList.contains('np-menu-container')).toBe(true);

    // Menu items for both characters
    const item1 = target.querySelector<HTMLElement>('[data-testid="char-picker-item-char-1"]');
    const item2 = target.querySelector<HTMLElement>('[data-testid="char-picker-item-char-2"]');
    expect(item1).toBeTruthy();
    expect(item2).toBeTruthy();
    expect(item2?.textContent).toContain('助手ベアトリス');

    // Clicking item2 switches character and closes menu
    item2?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(selectedCharId).toBe('char-2');
    expect(target.querySelector('[data-testid="character-picker-dropdown"]')).toBeNull();

    // Reopen and test outside-click dismissal
    charSelectBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="character-picker-dropdown"]')).toBeTruthy();

    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="character-picker-dropdown"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer 8種のダイスボタン描画、クリック時のテキスト追記、Tooltip
  it('renders 8 dice IconButtons centered with Tooltips and appends dice strings to message textarea', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
      },
    });

    const diceRow = target.querySelector('[data-testid="dice-buttons-row"]');
    expect(diceRow).toBeTruthy();

    const expectedDice = ['1D2', '1D4', '1D6', '1D8', '1D10', '1D12', '1D20', '1D100'];

    for (const dice of expectedDice) {
      const diceBtn = target.querySelector<HTMLButtonElement>(`[data-testid="dice-btn-${dice.toLowerCase()}"]`);
      expect(diceBtn).toBeTruthy();
      expect(diceBtn?.classList.contains('np-icon-button')).toBe(true);
      expect(diceBtn?.getAttribute('aria-label')).toBe(dice);

      // Verify SVG polygon/die representation exists inside button
      const svg = diceBtn?.querySelector('svg');
      expect(svg).toBeTruthy();

      // Verify Tooltip exists
      const tooltip = target.querySelector(`#tooltip-dice-${dice.toLowerCase()}`);
      expect(tooltip).toBeTruthy();
      expect(tooltip?.textContent?.trim()).toBe(dice);
    }

    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea).toBeTruthy();
    expect(textarea?.value).toBe('');

    // Click 1D6 dice button
    const dice1d6 = target.querySelector<HTMLButtonElement>('[data-testid="dice-btn-1d6"]');
    dice1d6?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(textarea?.value).toBe('1D6');

    // Click 1D20 dice button to append
    const dice1d20 = target.querySelector<HTMLButtonElement>('[data-testid="dice-btn-1d20"]');
    dice1d20?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(textarea?.value).toBe('1D61D20');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer Multiline Outlined TextField (minLines=4, maxLines=10) の入力・拡張
  it('renders Multiline Outlined TextField with minLines=4, maxLines=10, rows=4, and chat palette suggestion', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
      },
    });

    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea).toBeTruthy();
    expect(textarea?.tagName.toLowerCase()).toBe('textarea');
    expect(Number(textarea?.rows)).toBe(4);

    const field = textarea?.closest('.np-field');
    expect(field).toBeTruthy();
    expect(field?.classList.contains('np-field-outlined')).toBe(true);

    // TextField container sets minLines and maxLines inline style variables
    const labelParent = textarea?.closest('.np-text-field');
    const styleAttr = labelParent?.getAttribute('style') || '';
    expect(styleAttr).toContain('4 * 1lh');
    expect(styleAttr).toContain('10 * 1lh');


    // Typing partial match shows suggestions overlay
    if (textarea) {
      textarea.value = '聞き耳';
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    const suggestions = target.querySelector('[data-testid="chat-palette-suggestions"]');
    expect(suggestions).toBeTruthy();
    const item0 = target.querySelector<HTMLButtonElement>('[data-testid="suggestion-item-0"]');
    expect(item0?.textContent?.trim()).toBe('1D100<=70 【聞き耳】');

    // Tap suggestion to replace
    item0?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(textarea?.value).toBe('1D100<=70 【聞き耳】');
    expect(target.querySelector('[data-testid="chat-palette-suggestions"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer 送信時の Generast 生成・送信処理
  it('generates plaintext Generast AST and submits message synchronously without extensions', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let sentText: string | null = null;
    let sentCharId: string | null = null;
    let sentGenerast: GenerastAST | null = null;

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        onSendMessage: (text, charId, generast) => {
          sentText = text;
          sentCharId = charId ?? null;
          sentGenerast = generast ?? null;
        },
      },
    });

    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    if (textarea) {
      textarea.value = 'こんにちは、探索開始します！';
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
    }

    const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
    expect(sendBtn).toBeTruthy();
    expect(sendBtn?.classList.contains('np-button')).toBe(true);
    expect(sendBtn?.classList.contains('filled')).toBe(true);
    expect(sendBtn?.textContent).toContain('送信');

    sendBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    expect(sentText).toBe('こんにちは、探索開始します！');
    expect(sentCharId).toBe('char-1');
    expect(sentGenerast).toEqual({
      type: 'string',
      value: 'こんにちは、探索開始します！',
    });
    expect(textarea?.value).toBe('');


    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer 拡張機能パイプラインによる Generast AST 変換送信
  it('executes extension pipeline when extensions are enabled to generate transformed Generast AST', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let sentGenerast: GenerastAST | null = null;
    let sentRawText: string | null = null;

    const mockPipeline = new ExtensionPipeline();
    // Spy on executePipeline
    vi.spyOn(mockPipeline, 'executePipeline').mockImplementation(async (_exts, inputBytes) => {
      let decoded: string;
      try {
        decoded = decodeExtIoTin(inputBytes).text;
      } catch {
        decoded = new TextDecoder().decode(inputBytes);
      }
      // Simulate DSL expansion or dice translation
      return new TextEncoder().encode(`[GENERAST:TRANSFORMED] ${decoded}`);
    });

    const activeExtensions: ExtensionItem[] = [
      {
        id: 'mock-dsl',
        name: 'Mock DSL',
        enabled: true,
        priority: 0,
      },
    ];

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        extensions: activeExtensions,
        extensionPipeline: mockPipeline,
        onSendGenerast: (generast, _charId, rawText) => {
          sentGenerast = generast;
          sentRawText = rawText ?? null;
        },
      },
    });

    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    if (textarea) {
      textarea.value = '1D6 攻撃ロール';
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
    }

    const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
    sendBtn?.click();

    await new Promise((r) => setTimeout(r, 30));

    expect(mockPipeline.executePipeline).toHaveBeenCalledTimes(1);
    expect(sentRawText).toBe('1D6 攻撃ロール');
    expect(sentGenerast).toEqual({
      type: 'string',
      value: '[GENERAST:TRANSFORMED] 1D6 攻撃ロール',
    });

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer 送信ボタン右横の「その他」ボタンとTooltip、ダイスピッカー表示・非表示トグルメニュー (Issue #86)
  it('renders more options button next to send button with tooltip, and toggles dice picker visibility via menu', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    let dicePickerVisible = true;
    const onToggleDicePicker = vi.fn((show: boolean) => {
      dicePickerVisible = show;
    });

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        onToggleDicePicker,
      },
    });

    // 1. Verify "その他" button exists next to Send button
    const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
    expect(sendBtn).toBeTruthy();

    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="footer-more-button"]');
    expect(moreBtn).toBeTruthy();
    expect(moreBtn?.classList.contains('np-icon-button')).toBe(true);
    expect(moreBtn?.classList.contains('s')).toBe(true);
    expect(moreBtn?.classList.contains('text')).toBe(true);
    expect(moreBtn?.getAttribute('aria-label')).toBe('その他');
    expect(moreBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('more_vert');

    // 2. Verify Tooltip exists for "その他"
    const moreTooltip = target.querySelector('#tooltip-footer-more');
    expect(moreTooltip).toBeTruthy();
    expect(moreTooltip?.textContent?.trim()).toBe('その他');

    // 3. Dice picker is initially visible by default
    expect(target.querySelector('[data-testid="dice-buttons-row"]')).toBeTruthy();

    // 4. Click "その他" button to open menu
    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const menu = target.querySelector('[data-testid="footer-more-menu"]');
    expect(menu).toBeTruthy();

    const toggleItem = target.querySelector<HTMLElement>('[data-testid="toggle-dice-picker-menu-item"]');
    expect(toggleItem).toBeTruthy();
    expect(toggleItem?.textContent).toContain('ダイスピッカーを非表示');

    // 5. Click "ダイスピッカーを非表示"
    toggleItem?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Menu closes and dice picker is hidden
    expect(target.querySelector('[data-testid="footer-more-menu"]')).toBeNull();
    expect(target.querySelector('[data-testid="dice-buttons-row"]')).toBeNull();
    expect(onToggleDicePicker).toHaveBeenCalledWith(false);
    expect(dicePickerVisible).toBe(false);

    // 6. Click "その他" button again to reopen menu
    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const reopenMenu = target.querySelector('[data-testid="footer-more-menu"]');
    expect(reopenMenu).toBeTruthy();

    const toggleItem2 = target.querySelector<HTMLElement>('[data-testid="toggle-dice-picker-menu-item"]');
    expect(toggleItem2).toBeTruthy();
    expect(toggleItem2?.textContent).toContain('ダイスピッカーを表示');

    // 7. Click "ダイスピッカーを表示"
    toggleItem2?.click();
    await new Promise((r) => setTimeout(r, 20));

    // Menu closes and dice picker is restored
    expect(target.querySelector('[data-testid="footer-more-menu"]')).toBeNull();
    expect(target.querySelector('[data-testid="dice-buttons-row"]')).toBeTruthy();
    expect(onToggleDicePicker).toHaveBeenCalledWith(true);
    expect(dicePickerVisible).toBe(true);

    // 8. Test outside click dismissal for more menu
    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="footer-more-menu"]')).toBeTruthy();

    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));
    expect(target.querySelector('[data-testid="footer-more-menu"]')).toBeNull();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer showDicePicker初期プロパティ制御 (Issue #86)
  it('respects initial showDicePicker=false prop', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        showDicePicker: false,
      },
    });

    // Dice row should not be in document initially
    expect(target.querySelector('[data-testid="dice-buttons-row"]')).toBeNull();

    // Opening more menu should show "ダイスピッカーを表示"
    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="footer-more-button"]');
    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 20));

    const toggleItem = target.querySelector<HTMLElement>('[data-testid="toggle-dice-picker-menu-item"]');
    expect(toggleItem?.textContent).toContain('ダイスピッカーを表示');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer キャラクターピッカーメニューおよびその他メニューの直上アンカー配置スタイル (Issue #124)
  it('positions character picker and more menus adjacent to anchor buttons above footer with position-anchor', async () => {
    window.innerWidth = 1000;
    window.innerHeight = 800;

    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
      },
    });

    // 1. Character picker menu positioning
    const charSelectBtn = target.querySelector<HTMLButtonElement>('[data-testid="character-select-button"]');
    expect(charSelectBtn).toBeTruthy();

    vi.spyOn(charSelectBtn!, 'getBoundingClientRect').mockReturnValue({
      top: 680,
      bottom: 716,
      left: 100,
      right: 250,
      width: 150,
      height: 36,
      x: 100,
      y: 680,
      toJSON: () => {},
    });

    charSelectBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    const charMenu = target.querySelector<HTMLElement>('[data-testid="character-picker-dropdown"]');
    expect(charMenu).toBeTruthy();
    expect(charMenu?.style.position).toBe('fixed');
    expect(charMenu?.getAttribute('style')).toContain('position: fixed !important');
    expect(charMenu?.getAttribute('style')).toContain('inset: unset');
    expect(charMenu?.style.margin).toBe('0px');
    expect(charMenu?.getAttribute('style')).toContain('position-area: none !important');
    expect(charMenu?.getAttribute('style')).toContain('position-try-fallbacks: none !important');
    expect(charMenu?.getAttribute('style')).toContain('align-self: unset !important');
    expect(charMenu?.getAttribute('style')).toContain('justify-self: unset !important');
    // bottom = vh (800) - top (680) + offset (4) = 124px
    expect(charMenu?.style.bottom).toBe('124px');
    expect(charMenu?.style.left).toBe('100px');
    expect(charSelectBtn?.style.getPropertyValue('anchor-name')).toBe('--char-picker-anchor');
    expect(charMenu?.style.getPropertyValue('position-anchor')).toBe('--char-picker-anchor');
    expect(charMenu?.style.getPropertyValue('--np-menu-position-area')).toBe('top span-right');

    // Close char menu
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 30));
    expect(target.querySelector('[data-testid="character-picker-dropdown"]')).toBeNull();

    // 2. More options menu positioning
    const moreBtn = target.querySelector<HTMLButtonElement>('[data-testid="footer-more-button"]');
    expect(moreBtn).toBeTruthy();

    vi.spyOn(moreBtn!, 'getBoundingClientRect').mockReturnValue({
      top: 680,
      bottom: 716,
      left: 920,
      right: 956,
      width: 36,
      height: 36,
      x: 920,
      y: 680,
      toJSON: () => {},
    });

    moreBtn?.click();
    await new Promise((r) => setTimeout(r, 30));

    const moreMenu = target.querySelector<HTMLElement>('[data-testid="footer-more-menu"]');
    expect(moreMenu).toBeTruthy();
    expect(moreMenu?.style.position).toBe('fixed');
    expect(moreMenu?.getAttribute('style')).toContain('position: fixed !important');
    expect(moreMenu?.getAttribute('style')).toContain('inset: unset');
    expect(moreMenu?.style.margin).toBe('0px');
    expect(moreMenu?.getAttribute('style')).toContain('position-area: none !important');
    expect(moreMenu?.getAttribute('style')).toContain('position-try-fallbacks: none !important');
    expect(moreMenu?.getAttribute('style')).toContain('align-self: unset !important');
    expect(moreMenu?.getAttribute('style')).toContain('justify-self: unset !important');
    // bottom = vh (800) - top (680) + offset (4) = 124px
    expect(moreMenu?.style.bottom).toBe('124px');
    // right = vw (1000) - right (956) = 44px
    expect(moreMenu?.style.right).toBe('44px');
    expect(moreBtn?.style.getPropertyValue('anchor-name')).toBe('--footer-more-anchor');
    expect(moreMenu?.style.getPropertyValue('position-anchor')).toBe('--footer-more-anchor');
    expect(moreMenu?.style.getPropertyValue('--np-menu-position-area')).toBe('top span-right');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer 送信中は送信アイコンを波線 CircularProgress に差し替え、
  // フッター全体を暗転・無効化し aria-busy を立てる (Issue #109 / #139)
  it('replaces the send icon with a wavy indeterminate CircularProgress and dims the footer while sending', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        sending: true,
      },
    });

    const footer = target.querySelector<HTMLElement>('[data-testid="common-footer"]');
    expect(footer).toBeTruthy();
    expect(footer?.classList.contains('sending')).toBe(true);
    expect(footer?.getAttribute('aria-busy')).toBe('true');
    expect(footer?.getAttribute('data-sending')).toBe('true');

    const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
    expect(sendBtn).toBeTruthy();
    expect(sendBtn?.disabled).toBe(true);

    // The wavy indeterminate progress replaces the send icon.
    const progress = sendBtn?.querySelector<HTMLElement>('[data-testid="send-progress-indicator"]');
    expect(progress).toBeTruthy();
    expect(progress?.classList.contains('progress')).toBe(true);
    expect(progress?.classList.contains('indeterminate')).toBe(true);
    expect(progress?.classList.contains('wavy')).toBe(true);
    expect(progress?.getAttribute('role')).toBe('progressbar');
    // Wavy is drawn as an animated path, not a plain circle.
    expect(progress?.querySelector('svg path.wave')).toBeTruthy();
    expect(sendBtn?.querySelector('.np-icon')).toBeNull();

    // The whole footer is non-interactive while sending.
    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea?.disabled).toBe(true);
    expect(target.querySelector<HTMLButtonElement>('[data-testid="dice-btn-1d6"]')?.disabled).toBe(true);
    expect(target.querySelector<HTMLButtonElement>('[data-testid="character-select-button"]')?.disabled).toBe(
      true,
    );

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer 送信中の二重送信防止 (Issue #109 / #139)
  it('blocks double sends while sending and restores the send icon when sending is cleared', async () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const onSendMessage = vi.fn();
    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        sending: true,
        onSendMessage,
      },
    });

    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    if (textarea) {
      textarea.value = '二重送信テスト';
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await new Promise((r) => setTimeout(r, 20));

    const sendBtn = target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]');
    // Programmatic clicks must not reach the send handler while sending.
    sendBtn?.click();
    sendBtn?.click();
    sendBtn?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    textarea?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await new Promise((r) => setTimeout(r, 20));

    expect(onSendMessage).not.toHaveBeenCalled();
    expect(target.querySelector('[data-testid="send-progress-indicator"]')).toBeTruthy();

    unmount(instance);

    // Without `sending` the footer keeps its normal send affordances.
    const idleTarget = document.createElement('div');
    document.body.appendChild(idleTarget);
    const onIdleSend = vi.fn();
    const idleInstance = mount(CommonFooter, {
      target: idleTarget,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
        onSendMessage: onIdleSend,
      },
    });

    const idleFooter = idleTarget.querySelector<HTMLElement>('[data-testid="common-footer"]');
    expect(idleFooter?.classList.contains('sending')).toBe(false);
    expect(idleFooter?.getAttribute('aria-busy')).toBe('false');
    const idleSendBtn = idleTarget.querySelector<HTMLButtonElement>(
      '[data-testid="send-message-button"]',
    );
    expect(idleSendBtn?.disabled).toBe(false);
    expect(idleTarget.querySelector('[data-testid="send-progress-indicator"]')).toBeNull();
    expect(idleSendBtn?.querySelector('.np-icon')?.textContent?.trim()).toBe('send');

    const idleTextarea = idleTarget.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    if (idleTextarea) {
      idleTextarea.value = '通常送信';
      idleTextarea.dispatchEvent(new Event('input', { bubbles: true }));
    }
    idleSendBtn?.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(onIdleSend).toHaveBeenCalledTimes(1);

    unmount(idleInstance);
    idleTarget.remove();
    target.remove();
  });

  // spec: alhalte-argenterie/common-footer 送信中の暗転は M3 セマンティックトークンのみで表現する (Issue #109 / #139)
  it('dims the footer with token-compatible CSS and no hard-coded colors', () => {
    const sendingRule = FOOTER_SOURCE.match(/\.common-footer-container\.sending\s*\{([^}]*)\}/);
    expect(sendingRule).toBeTruthy();
    const declarations = sendingRule![1];
    expect(declarations).toContain('opacity: var(--np-footer-sending-opacity');
    expect(declarations).toContain('pointer-events: none');
    expect(declarations).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(declarations).not.toMatch(/\brgba?\(/);
    // The wavy spinner must not come from the Button `loading` prop.
    expect(FOOTER_SOURCE).not.toMatch(/loading=\{sending\}/);
    expect(FOOTER_SOURCE).toContain('CircularProgress');
    expect(FOOTER_SOURCE).toContain('wavy');
  });

  // spec: alhalte-argenterie/common-footer オフラインキューの通知はログ内へ集約する (Issue #148)
  it('does not render the offline queue notice above the footer contents', () => {
    const target = document.createElement('div');
    document.body.appendChild(target);

    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: sampleCharacters,
        selectedCharacterId: 'char-1',
      },
    });

    // Queued messages are rendered inline in `LogTab`; the footer is input only
    // and the connection indicator owns connection state (Issue #148).
    expect(target.querySelector('[data-testid="offline-queue-notice"]')).toBeNull();
    expect(target.querySelector('[data-testid="footer-queue-notice"]')).toBeNull();
    expect(FOOTER_SOURCE).not.toContain('OfflineQueueNotice');
    expect(FOOTER_SOURCE).not.toContain('footer-queue-notice');
    expect(FOOTER_SOURCE).not.toContain('pendingMessageCount');

    unmount(instance);
    target.remove();
  });
});

/**
 * Speaker picker ownership (Issue #271).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
 * - GitHub Issue #271
 */
describe('CommonFooter speaker picker ownership (Issue #271)', () => {
  const ACCOUNT = '1001';
  const ownedCharacter: Character = {
    id: 'own-1',
    name: 'アリス',
    initiative: 10,
    description: '',
    statuses: [],
    parameters: [],
    chatPalette: '',
    hideStatus: false,
    ownerAccountId: ACCOUNT,
  };
  const foreignCharacter: Character = {
    id: 'other-1',
    name: 'ボブ',
    initiative: 5,
    description: '',
    statuses: [],
    parameters: [],
    chatPalette: '',
    hideStatus: false,
    ownerAccountId: '2002',
  };
  const allCharacters = [ownedCharacter, foreignCharacter];

  function mountFooter(props: Record<string, unknown>) {
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(CommonFooter, {
      target,
      props: {
        activeTab: 'log',
        characters: allCharacters,
        ...props,
      },
    });
    return { target, instance };
  }

  // spec: alhalte-argenterie/permissions log speaker ownership (Issue #271)
  it('offers a participant only the speakable list in the picker', async () => {
    const { target, instance } = mountFooter({
      role: 'participant',
      speakableCharacters: speakableCharacters(allCharacters, 'participant', ACCOUNT),
    });

    target.querySelector<HTMLButtonElement>('[data-testid="character-select-button"]')?.click();
    await new Promise((resolve) => setTimeout(resolve, 20));

    expect(target.querySelector('[data-testid="char-picker-item-own-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="char-picker-item-other-1"]')).toBeNull();
    // The default speaker is the owned character, never the foreign one.
    expect(target.querySelector('.picker-name')?.textContent?.trim()).toBe('アリス');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/permissions log speaker ownership (Issue #271)
  it('offers an admin every character', async () => {
    const { target, instance } = mountFooter({
      role: 'admin',
      speakableCharacters: speakableCharacters(allCharacters, 'admin', ACCOUNT),
    });

    target.querySelector<HTMLButtonElement>('[data-testid="character-select-button"]')?.click();
    await new Promise((resolve) => setTimeout(resolve, 20));

    expect(target.querySelector('[data-testid="char-picker-item-own-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="char-picker-item-other-1"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/permissions log speaker ownership (Issue #271)
  it('disables sending for a viewer with no speakable characters', () => {
    const onSendMessage = vi.fn();
    const { target, instance } = mountFooter({
      role: 'viewer',
      speakableCharacters: speakableCharacters(allCharacters, 'viewer', ACCOUNT),
      onSendMessage,
    });

    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    expect(textarea?.disabled).toBe(true);
    target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]')?.click();
    expect(onSendMessage).not.toHaveBeenCalled();

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/permissions log speaker ownership (Issue #271)
  it('sends narration (no speakerId) when the speakable list is empty', async () => {
    const onSendMessage = vi.fn();
    const { target, instance } = mountFooter({
      role: 'participant',
      speakableCharacters: [],
      onSendMessage,
    });

    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    textarea!.value = 'ナレーション';
    textarea!.dispatchEvent(new Event('input', { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 10));
    target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]')?.click();

    expect(onSendMessage).toHaveBeenCalledTimes(1);
    expect(onSendMessage.mock.calls[0][0]).toBe('ナレーション');
    expect(onSendMessage.mock.calls[0][1]).toBeUndefined();

    unmount(instance);
    target.remove();
  });
});

/**
 * End-to-end speaker picker for `App` (Issue #271).
 *
 * References:
 * - ARCHITECTURE.md: `## alhalte-argenterie -> ### 権限 UI`
 * - GitHub Issue #271
 */
describe('App speaker picker derives the speakable list (Issue #271)', () => {
  const ACCOUNT = '1001';
  const ROOM: VisitedRoom = {
    id: 'room-271-speaker',
    name: '話者テスト部屋',
    url: 'https://alhalte.example/rooms/room-271-speaker#key',
    lastVisited: Date.now(),
    isAdmin: false,
  };
  const ownedCharacter: Character = {
    id: 'own-1',
    name: 'アリス',
    initiative: 10,
    description: '',
    statuses: [],
    parameters: [],
    chatPalette: '',
    hideStatus: false,
    ownerAccountId: ACCOUNT,
  };
  const foreignCharacter: Character = {
    id: 'other-1',
    name: 'ボブ',
    initiative: 5,
    description: '',
    statuses: [],
    parameters: [],
    chatPalette: '',
    hideStatus: false,
    ownerAccountId: '2002',
  };

  beforeEach(() => {
    document.body.innerHTML = '';
    localStorage.clear();
    localStorage.setItem('alhalte:account-id', ACCOUNT);
    Object.defineProperty(window, 'innerWidth', { writable: true, configurable: true, value: 1280 });
    Object.defineProperty(window, 'innerHeight', { writable: true, configurable: true, value: 800 });
  });

  async function mountForRole(role: 'admin' | 'participant', socket: FakeRoomSocket) {
    const target = document.createElement('div');
    document.body.appendChild(target);
    const instance = mount(App, {
      target,
      props: {
        initialRoom: ROOM,
        userRole: role,
        initialCharacters: [ownedCharacter, foreignCharacter],
        roomSocket: asRoomSocket(socket),
      },
    });
    await new Promise((resolve) => setTimeout(resolve, 40));
    return { target, instance };
  }

  function openPicker(target: HTMLElement): void {
    target.querySelector<HTMLButtonElement>('[data-testid="character-select-button"]')?.click();
  }

  // spec: alhalte-argenterie/permissions log speaker ownership (Issue #271)
  it('gives a participant only owned characters and a safe default speaker', async () => {
    const socket = new FakeRoomSocket();
    const { target, instance } = await mountForRole('participant', socket);

    // Default speaker: the owned character, never the foreign one.
    expect(target.querySelector('.picker-name')?.textContent?.trim()).toBe('アリス');

    openPicker(target);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(target.querySelector('[data-testid="char-picker-item-own-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="char-picker-item-other-1"]')).toBeNull();

    // A sent message carries the owned speakerId.
    const textarea = target.querySelector<HTMLTextAreaElement>('[data-testid="chat-input-field"]');
    textarea!.value = '参加者の発言';
    textarea!.dispatchEvent(new Event('input', { bubbles: true }));
    target.querySelector<HTMLButtonElement>('[data-testid="send-message-button"]')?.click();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(socket.requestAppendLogCalls).toHaveLength(1);
    expect(socket.requestAppendLogCalls[0].record.speakerId).toBe('own-1');

    unmount(instance);
    target.remove();
  });

  // spec: alhalte-argenterie/permissions log speaker ownership (Issue #271)
  it('gives an admin every character', async () => {
    const socket = new FakeRoomSocket();
    const { target, instance } = await mountForRole('admin', socket);

    openPicker(target);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(target.querySelector('[data-testid="char-picker-item-own-1"]')).toBeTruthy();
    expect(target.querySelector('[data-testid="char-picker-item-other-1"]')).toBeTruthy();

    unmount(instance);
    target.remove();
  });
});
